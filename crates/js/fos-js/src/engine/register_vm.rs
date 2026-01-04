//! Register-Based Virtual Machine
//!
//! A register-based VM is typically 30-50% faster than stack-based
//! because it reduces memory traffic and instruction count.
//!
//! Format: Each instruction has explicit register operands.
//! Example: ADD r0, r1, r2  (r0 = r1 + r2)

use super::value::JsVal;
use std::sync::Arc;

/// Number of virtual registers per frame
pub const NUM_REGISTERS: usize = 256;

/// Register index
pub type Reg = u8;

/// Register-based opcode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RegOpcode {
    // Move/Load
    Move = 0,           // dst, src - move register
    LoadConst = 1,      // dst, const_idx(u16)
    LoadNull = 2,       // dst
    LoadUndefined = 3,  // dst
    LoadTrue = 4,       // dst
    LoadFalse = 5,      // dst
    LoadInt = 6,        // dst, imm(i16) - small immediate
    
    // Arithmetic (3-address)
    Add = 10,           // dst, src1, src2
    Sub = 11,           // dst, src1, src2
    Mul = 12,           // dst, src1, src2
    Div = 13,           // dst, src1, src2
    Mod = 14,           // dst, src1, src2
    Neg = 15,           // dst, src
    
    // Comparison (result in dst)
    Lt = 20,            // dst, src1, src2
    Le = 21,            // dst, src1, src2
    Gt = 22,            // dst, src1, src2
    Ge = 23,            // dst, src1, src2
    Eq = 24,            // dst, src1, src2
    Ne = 25,            // dst, src1, src2
    
    // Logical
    Not = 30,           // dst, src
    And = 31,           // dst, src1, src2
    Or = 32,            // dst, src1, src2
    
    // Control flow
    Jump = 40,          // offset(i16)
    JumpIfFalse = 41,   // src, offset(i16)
    JumpIfTrue = 42,    // src, offset(i16)
    
    // Function calls
    Call = 50,          // dst, func_reg, argc
    TailCall = 51,      // func_reg, argc
    Return = 52,        // src (return value register)
    
    // Properties
    GetProp = 60,       // dst, obj_reg, name_idx(u16)
    SetProp = 61,       // obj_reg, name_idx(u16), src
    GetIndex = 62,      // dst, obj_reg, idx_reg
    SetIndex = 63,      // obj_reg, idx_reg, src
    
    // Objects
    NewObject = 70,     // dst
    NewArray = 71,      // dst, count
    
    // Special
    Halt = 255,
}

/// Register-based instruction (4 bytes fixed width for simplicity)
#[derive(Debug, Clone, Copy)]
pub struct RegInstruction {
    pub opcode: RegOpcode,
    pub a: u8,          // Destination or first operand
    pub b: u8,          // Second operand or immediate low
    pub c: u8,          // Third operand or immediate high
}

impl RegInstruction {
    pub fn new(opcode: RegOpcode, a: u8, b: u8, c: u8) -> Self {
        Self { opcode, a, b, c }
    }
    
    /// Get 16-bit immediate from b and c
    pub fn imm16(&self) -> i16 {
        i16::from_le_bytes([self.b, self.c])
    }
    
    /// Get unsigned 16-bit from b and c
    pub fn u16(&self) -> u16 {
        u16::from_le_bytes([self.b, self.c])
    }
}

/// Register-based bytecode chunk
#[derive(Debug, Clone, Default)]
pub struct RegBytecode {
    pub instructions: Vec<RegInstruction>,
    pub constants: Vec<JsVal>,
    pub names: Vec<Box<str>>,
}

impl RegBytecode {
    pub fn new() -> Self { Self::default() }
    
    pub fn emit(&mut self, op: RegOpcode, a: u8, b: u8, c: u8) {
        self.instructions.push(RegInstruction::new(op, a, b, c));
    }
    
    pub fn add_constant(&mut self, val: JsVal) -> u16 {
        // Deduplicate
        for (i, c) in self.constants.iter().enumerate() {
            if self.vals_equal(c, &val) {
                return i as u16;
            }
        }
        let idx = self.constants.len();
        self.constants.push(val);
        idx as u16
    }
    
    fn vals_equal(&self, a: &JsVal, b: &JsVal) -> bool {
        match (a.as_number(), b.as_number()) {
            (Some(x), Some(y)) => x.to_bits() == y.to_bits(),
            _ => false,
        }
    }
    
    pub fn add_name(&mut self, name: &str) -> u16 {
        for (i, n) in self.names.iter().enumerate() {
            if &**n == name { return i as u16; }
        }
        let idx = self.names.len();
        self.names.push(name.into());
        idx as u16
    }
}

/// Register-based call frame
#[derive(Debug)]
struct RegFrame {
    registers: [JsVal; NUM_REGISTERS],
    ip: usize,
    return_reg: Reg,
    /// The bytecode being executed (index into functions)
    func_id: u32,
}

impl Default for RegFrame {
    fn default() -> Self {
        Self {
            registers: std::array::from_fn(|_| JsVal::Undefined),
            ip: 0,
            return_reg: 0,
            func_id: 0,
        }
    }
}

/// Register-based Virtual Machine
pub struct RegisterVM {
    frames: Vec<RegFrame>,
    globals: std::collections::HashMap<Box<str>, JsVal>,
    objects: Vec<super::object::JsObject>,
    arrays: Vec<super::object::JsArray>,
    /// Compiled functions (RegBytecode for each)
    functions: Vec<RegBytecode>,
}

impl Default for RegisterVM {
    fn default() -> Self { Self::new() }
}

impl RegisterVM {
    pub fn new() -> Self {
        Self {
            frames: vec![RegFrame::default()],
            globals: std::collections::HashMap::new(),
            objects: Vec::new(),
            arrays: Vec::new(),
            functions: Vec::new(),
        }
    }
    
    /// Add a function and return its ID
    pub fn add_function(&mut self, bytecode: RegBytecode) -> u32 {
        let id = self.functions.len() as u32;
        self.functions.push(bytecode);
        id
    }
    
    /// Push a new call frame
    fn push_frame(&mut self, func_id: u32, return_reg: Reg, args: &[JsVal]) {
        let mut frame = RegFrame::default();
        frame.func_id = func_id;
        frame.return_reg = return_reg;
        
        // Copy arguments to registers 0, 1, 2, ...
        for (i, arg) in args.iter().enumerate() {
            if i < NUM_REGISTERS {
                frame.registers[i] = *arg;
            }
        }
        
        self.frames.push(frame);
    }
    
    /// Pop call frame and return to caller
    fn pop_frame(&mut self, return_value: JsVal) -> Option<JsVal> {
        if self.frames.len() <= 1 {
            return Some(return_value);
        }
        
        let old_frame = self.frames.pop()?;
        
        // Store return value in caller's return register
        if let Some(caller) = self.frames.last_mut() {
            caller.registers[old_frame.return_reg as usize] = return_value;
        }
        
        None // Continue execution
    }
    
    /// Execute a function by ID
    pub fn call_function(&mut self, func_id: u32, args: &[JsVal]) -> JsVal {
        self.push_frame(func_id, 0, args);
        
        // Get function bytecode
        if let Some(bc) = self.functions.get(func_id as usize).cloned() {
            self.execute(&bc)
        } else {
            JsVal::Undefined
        }
    }
    
    /// Execute register-based bytecode
    pub fn execute(&mut self, bytecode: &RegBytecode) -> JsVal {
        self.execute_inner(bytecode)
    }
    
    /// Internal execute for nested calls
    fn execute_inner(&mut self, bytecode: &RegBytecode) -> JsVal {
        let mut frame = self.frames.last_mut().unwrap();
        
        while frame.ip < bytecode.instructions.len() {
            let inst = bytecode.instructions[frame.ip];
            frame.ip += 1;
            
            match inst.opcode {
                RegOpcode::Move => {
                    frame.registers[inst.a as usize] = frame.registers[inst.b as usize];
                }
                RegOpcode::LoadConst => {
                    let idx = inst.u16() as usize;
                    frame.registers[inst.a as usize] = bytecode.constants[idx];
                }
                RegOpcode::LoadNull => {
                    frame.registers[inst.a as usize] = JsVal::Null;
                }
                RegOpcode::LoadUndefined => {
                    frame.registers[inst.a as usize] = JsVal::Undefined;
                }
                RegOpcode::LoadTrue => {
                    frame.registers[inst.a as usize] = JsVal::Bool(true);
                }
                RegOpcode::LoadFalse => {
                    frame.registers[inst.a as usize] = JsVal::Bool(false);
                }
                RegOpcode::LoadInt => {
                    let imm = inst.imm16() as f64;
                    frame.registers[inst.a as usize] = JsVal::Number(imm);
                }
                
                // Arithmetic
                RegOpcode::Add => {
                    let a = frame.registers[inst.b as usize].to_number();
                    let b = frame.registers[inst.c as usize].to_number();
                    frame.registers[inst.a as usize] = JsVal::Number(a + b);
                }
                RegOpcode::Sub => {
                    let a = frame.registers[inst.b as usize].to_number();
                    let b = frame.registers[inst.c as usize].to_number();
                    frame.registers[inst.a as usize] = JsVal::Number(a - b);
                }
                RegOpcode::Mul => {
                    let a = frame.registers[inst.b as usize].to_number();
                    let b = frame.registers[inst.c as usize].to_number();
                    frame.registers[inst.a as usize] = JsVal::Number(a * b);
                }
                RegOpcode::Div => {
                    let a = frame.registers[inst.b as usize].to_number();
                    let b = frame.registers[inst.c as usize].to_number();
                    frame.registers[inst.a as usize] = JsVal::Number(a / b);
                }
                RegOpcode::Neg => {
                    let v = frame.registers[inst.b as usize].to_number();
                    frame.registers[inst.a as usize] = JsVal::Number(-v);
                }
                
                // Comparison
                RegOpcode::Lt => {
                    let a = frame.registers[inst.b as usize].to_number();
                    let b = frame.registers[inst.c as usize].to_number();
                    frame.registers[inst.a as usize] = JsVal::Bool(a < b);
                }
                RegOpcode::Le => {
                    let a = frame.registers[inst.b as usize].to_number();
                    let b = frame.registers[inst.c as usize].to_number();
                    frame.registers[inst.a as usize] = JsVal::Bool(a <= b);
                }
                RegOpcode::Gt => {
                    let a = frame.registers[inst.b as usize].to_number();
                    let b = frame.registers[inst.c as usize].to_number();
                    frame.registers[inst.a as usize] = JsVal::Bool(a > b);
                }
                RegOpcode::Ge => {
                    let a = frame.registers[inst.b as usize].to_number();
                    let b = frame.registers[inst.c as usize].to_number();
                    frame.registers[inst.a as usize] = JsVal::Bool(a >= b);
                }
                RegOpcode::Eq => {
                    let a = &frame.registers[inst.b as usize];
                    let b = &frame.registers[inst.c as usize];
                    frame.registers[inst.a as usize] = JsVal::Bool(
                        a.to_number() == b.to_number()
                    );
                }
                
                // Logical
                RegOpcode::Not => {
                    let v = frame.registers[inst.b as usize].is_truthy();
                    frame.registers[inst.a as usize] = JsVal::Bool(!v);
                }
                
                // Control flow
                RegOpcode::Jump => {
                    let offset = inst.imm16();
                    frame.ip = (frame.ip as i32 + offset as i32) as usize;
                }
                RegOpcode::JumpIfFalse => {
                    if !frame.registers[inst.a as usize].is_truthy() {
                        let offset = i16::from_le_bytes([inst.b, inst.c]);
                        frame.ip = (frame.ip as i32 + offset as i32) as usize;
                    }
                }
                RegOpcode::JumpIfTrue => {
                    if frame.registers[inst.a as usize].is_truthy() {
                        let offset = i16::from_le_bytes([inst.b, inst.c]);
                        frame.ip = (frame.ip as i32 + offset as i32) as usize;
                    }
                }
                
                // Return
                RegOpcode::Return => {
                    return frame.registers[inst.a as usize];
                }
                
                // Objects
                RegOpcode::NewObject => {
                    let id = self.objects.len() as u32;
                    self.objects.push(super::object::JsObject::new());
                    frame.registers[inst.a as usize] = JsVal::Object(id);
                }
                RegOpcode::NewArray => {
                    let id = self.arrays.len() as u32;
                    self.arrays.push(super::object::JsArray::new());
                    frame.registers[inst.a as usize] = JsVal::Array(id);
                }
                
                RegOpcode::Halt => break,
                
                // Function calls - dst = a (return reg), func_reg = b, argc = c
                // Note: Implementation is simplified to avoid complex nested calls
                // Full implementation would require proper frame management
                RegOpcode::Call => {
                    let dst_reg = inst.a;
                    let func_reg = inst.b;
                    let argc = inst.c as usize;
                    
                    // Extract all needed data before calling self methods
                    let func_id_opt = frame.registers[func_reg as usize].as_function_id();
                    let mut args = Vec::with_capacity(argc);
                    for i in 0..argc {
                        let arg_reg = func_reg as usize + 1 + i;
                        if arg_reg < NUM_REGISTERS {
                            args.push(frame.registers[arg_reg]);
                        }
                    }
                    
                    if let Some(func_id) = func_id_opt {
                        // Get function bytecode
                        if let Some(func_bc) = self.functions.get(func_id as usize).cloned() {
                            // Execute function inline without recursion (simplified)
                            // For full implementation, would need non-recursive interpreter
                            let result = self.call_function(func_id, &args);
                            // Store result - need to re-borrow frame
                            if let Some(f) = self.frames.last_mut() {
                                f.registers[dst_reg as usize] = result;
                            }
                        } else if let Some(f) = self.frames.last_mut() {
                            f.registers[dst_reg as usize] = JsVal::Undefined;
                        }
                    } else if let Some(f) = self.frames.last_mut() {
                        f.registers[dst_reg as usize] = JsVal::Undefined;
                    }
                    // Update frame reference
                    frame = self.frames.last_mut().unwrap();
                }
                RegOpcode::TailCall => {
                    let func_reg = inst.a;
                    let argc = inst.b as usize;
                    
                    // Extract data
                    let func_id_opt = frame.registers[func_reg as usize].as_function_id();
                    let mut args = Vec::with_capacity(argc);
                    for i in 0..argc {
                        let arg_reg = func_reg as usize + 1 + i;
                        if arg_reg < NUM_REGISTERS {
                            args.push(frame.registers[arg_reg]);
                        }
                    }
                    
                    if let Some(func_id) = func_id_opt {
                        if let Some(func_bc) = self.functions.get(func_id as usize).cloned() {
                            // Tail call: replace frame and continue
                            if let Some(f) = self.frames.last_mut() {
                                f.ip = 0;
                                f.func_id = func_id;
                                for r in f.registers.iter_mut() {
                                    *r = JsVal::Undefined;
                                }
                                for (i, arg) in args.iter().enumerate() {
                                    if i < NUM_REGISTERS {
                                        f.registers[i] = *arg;
                                    }
                                }
                            }
                            // Continue with new bytecode
                            return self.execute_inner(&func_bc);
                        }
                    }
                    return JsVal::Undefined;
                }
                
                // Property access
                RegOpcode::GetProp => {
                    // dst = a, obj_reg = b, name_idx = (b, c) as u16
                    let obj_id = frame.registers[inst.b as usize].as_object_id();
                    if let Some(id) = obj_id {
                        if let Some(obj) = self.objects.get(id as usize) {
                            let name_idx = inst.c as usize;
                            if let Some(name) = bytecode.names.get(name_idx) {
                                let val = obj.get(name).copied().unwrap_or(JsVal::Undefined);
                                frame.registers[inst.a as usize] = val;
                            }
                        }
                    } else {
                        frame.registers[inst.a as usize] = JsVal::Undefined;
                    }
                }
                RegOpcode::SetProp => {
                    // obj_reg = a, name_idx = b, src = c
                    let obj_id = frame.registers[inst.a as usize].as_object_id();
                    if let Some(id) = obj_id {
                        if let Some(obj) = self.objects.get_mut(id as usize) {
                            let name_idx = inst.b as usize;
                            if let Some(name) = bytecode.names.get(name_idx) {
                                obj.set(name, frame.registers[inst.c as usize]);
                            }
                        }
                    }
                }
                RegOpcode::GetIndex => {
                    // dst = a, arr_reg = b, idx_reg = c
                    let arr_id = frame.registers[inst.b as usize].as_array_id();
                    let idx = frame.registers[inst.c as usize].to_number() as usize;
                    if let Some(id) = arr_id {
                        if let Some(arr) = self.arrays.get(id as usize) {
                            let val = arr.get(idx);
                            frame.registers[inst.a as usize] = val;
                        }
                    } else {
                        frame.registers[inst.a as usize] = JsVal::Undefined;
                    }
                }
                RegOpcode::SetIndex => {
                    // arr_reg = a, idx_reg = b, src = c
                    let arr_id = frame.registers[inst.a as usize].as_array_id();
                    let idx = frame.registers[inst.b as usize].to_number() as usize;
                    if let Some(id) = arr_id {
                        if let Some(arr) = self.arrays.get_mut(id as usize) {
                            arr.set(idx, frame.registers[inst.c as usize]);
                        }
                    }
                }
                
                _ => {}
            }
        }
        
        frame.registers[0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_register_vm_arithmetic() {
        let mut bc = RegBytecode::new();
        
        // r0 = 10
        bc.emit(RegOpcode::LoadInt, 0, 10, 0);
        // r1 = 5
        bc.emit(RegOpcode::LoadInt, 1, 5, 0);
        // r2 = r0 + r1
        bc.emit(RegOpcode::Add, 2, 0, 1);
        // return r2
        bc.emit(RegOpcode::Return, 2, 0, 0);
        
        let mut vm = RegisterVM::new();
        let result = vm.execute(&bc);
        
        assert_eq!(result.to_number(), 15.0);
    }
    
    #[test]
    fn test_register_vm_comparison() {
        let mut bc = RegBytecode::new();
        
        // r0 = 10, r1 = 5
        bc.emit(RegOpcode::LoadInt, 0, 10, 0);
        bc.emit(RegOpcode::LoadInt, 1, 5, 0);
        // r2 = r0 > r1
        bc.emit(RegOpcode::Gt, 2, 0, 1);
        bc.emit(RegOpcode::Return, 2, 0, 0);
        
        let mut vm = RegisterVM::new();
        let result = vm.execute(&bc);
        
        assert_eq!(result.as_bool(), Some(true));
    }
}
