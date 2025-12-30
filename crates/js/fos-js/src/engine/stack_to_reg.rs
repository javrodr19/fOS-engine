//! Stack-to-Register Bytecode Compiler
//!
//! Converts stack-based bytecode to register-based bytecode.
//! This enables the faster register-based VM execution.

use super::bytecode::{Bytecode, Opcode, Constant};
use super::register_vm::{RegBytecode, RegOpcode, Reg};
use super::value::JsVal;

/// Compiler that converts stack bytecode to register bytecode
pub struct StackToRegCompiler {
    output: RegBytecode,
    /// Virtual stack to track register allocation
    stack: Vec<Reg>,
    /// Next available register
    next_reg: Reg,
    /// Local variable to register mapping
    local_to_reg: std::collections::HashMap<u16, Reg>,
}

impl Default for StackToRegCompiler {
    fn default() -> Self { Self::new() }
}

impl StackToRegCompiler {
    pub fn new() -> Self {
        Self {
            output: RegBytecode::new(),
            stack: Vec::new(),
            next_reg: 0,
            local_to_reg: std::collections::HashMap::new(),
        }
    }
    
    /// Allocate a new register
    fn alloc_reg(&mut self) -> Reg {
        let reg = self.next_reg;
        self.next_reg = self.next_reg.saturating_add(1);
        reg
    }
    
    /// Push a register onto virtual stack
    fn push(&mut self, reg: Reg) {
        self.stack.push(reg);
    }
    
    /// Pop a register from virtual stack
    fn pop(&mut self) -> Reg {
        self.stack.pop().unwrap_or(0)
    }
    
    /// Peek top of stack
    fn top(&self) -> Reg {
        *self.stack.last().unwrap_or(&0)
    }
    
    /// Compile stack bytecode to register bytecode
    pub fn compile(mut self, bytecode: &Bytecode) -> RegBytecode {
        let mut ip = 0;
        let code = &bytecode.code;
        
        while ip < code.len() {
            let op = code[ip];
            
            match op {
                // Constants
                x if x == Opcode::LoadConst as u8 => {
                    let idx = u16::from_le_bytes([code[ip + 1], code[ip + 2]]);
                    let dst = self.alloc_reg();
                    
                    // Copy constant to register bytecode and get new index
                    let new_idx = if let Some(c) = bytecode.constants.get(idx as usize) {
                        match c {
                            Constant::Number(n) => {
                                let i = self.output.constants.len() as u16;
                                self.output.constants.push(JsVal::Number(*n));
                                i
                            }
                            Constant::String(s) => {
                                let i = self.output.constants.len() as u16;
                                self.output.constants.push(JsVal::String(s.clone()));
                                i
                            }
                            _ => 0,
                        }
                    } else {
                        0
                    };
                    
                    // Emit with NEW index in register bytecode
                    let idx_bytes = new_idx.to_le_bytes();
                    self.output.emit(RegOpcode::LoadConst, dst, idx_bytes[0], idx_bytes[1]);
                    self.push(dst);
                    ip += 3;
                }
                x if x == Opcode::LoadNull as u8 => {
                    let dst = self.alloc_reg();
                    self.output.emit(RegOpcode::LoadNull, dst, 0, 0);
                    self.push(dst);
                    ip += 1;
                }
                x if x == Opcode::LoadUndefined as u8 => {
                    let dst = self.alloc_reg();
                    self.output.emit(RegOpcode::LoadUndefined, dst, 0, 0);
                    self.push(dst);
                    ip += 1;
                }
                x if x == Opcode::LoadTrue as u8 => {
                    let dst = self.alloc_reg();
                    self.output.emit(RegOpcode::LoadTrue, dst, 0, 0);
                    self.push(dst);
                    ip += 1;
                }
                x if x == Opcode::LoadFalse as u8 => {
                    let dst = self.alloc_reg();
                    self.output.emit(RegOpcode::LoadFalse, dst, 0, 0);
                    self.push(dst);
                    ip += 1;
                }
                x if x == Opcode::LoadZero as u8 => {
                    let dst = self.alloc_reg();
                    self.output.emit(RegOpcode::LoadInt, dst, 0, 0);
                    self.push(dst);
                    ip += 1;
                }
                x if x == Opcode::LoadOne as u8 => {
                    let dst = self.alloc_reg();
                    self.output.emit(RegOpcode::LoadInt, dst, 1, 0);
                    self.push(dst);
                    ip += 1;
                }
                
                // Locals
                x if x == Opcode::GetLocal as u8 => {
                    let slot = u16::from_le_bytes([code[ip + 1], code[ip + 2]]);
                    let dst = self.alloc_reg();
                    
                    // Check if local is already in a register
                    if let Some(&src) = self.local_to_reg.get(&slot) {
                        self.output.emit(RegOpcode::Move, dst, src, 0);
                    } else {
                        // Load from local slot - map to register
                        let local_reg = self.alloc_reg();
                        self.local_to_reg.insert(slot, local_reg);
                        self.output.emit(RegOpcode::Move, dst, local_reg, 0);
                    }
                    self.push(dst);
                    ip += 3;
                }
                x if x == Opcode::SetLocal as u8 => {
                    let slot = u16::from_le_bytes([code[ip + 1], code[ip + 2]]);
                    let src = self.top();  // Don't pop - SetLocal leaves value on stack
                    
                    // Map local to this register
                    self.local_to_reg.insert(slot, src);
                    ip += 3;
                }
                
                // Arithmetic - pop two, push result
                x if x == Opcode::Add as u8 => {
                    let b = self.pop();
                    let a = self.pop();
                    let dst = self.alloc_reg();
                    self.output.emit(RegOpcode::Add, dst, a, b);
                    self.push(dst);
                    ip += 1;
                }
                x if x == Opcode::Sub as u8 => {
                    let b = self.pop();
                    let a = self.pop();
                    let dst = self.alloc_reg();
                    self.output.emit(RegOpcode::Sub, dst, a, b);
                    self.push(dst);
                    ip += 1;
                }
                x if x == Opcode::Mul as u8 => {
                    let b = self.pop();
                    let a = self.pop();
                    let dst = self.alloc_reg();
                    self.output.emit(RegOpcode::Mul, dst, a, b);
                    self.push(dst);
                    ip += 1;
                }
                x if x == Opcode::Div as u8 => {
                    let b = self.pop();
                    let a = self.pop();
                    let dst = self.alloc_reg();
                    self.output.emit(RegOpcode::Div, dst, a, b);
                    self.push(dst);
                    ip += 1;
                }
                x if x == Opcode::Neg as u8 => {
                    let a = self.pop();
                    let dst = self.alloc_reg();
                    self.output.emit(RegOpcode::Neg, dst, a, 0);
                    self.push(dst);
                    ip += 1;
                }
                
                // Comparison
                x if x == Opcode::Lt as u8 => {
                    let b = self.pop();
                    let a = self.pop();
                    let dst = self.alloc_reg();
                    self.output.emit(RegOpcode::Lt, dst, a, b);
                    self.push(dst);
                    ip += 1;
                }
                x if x == Opcode::Le as u8 => {
                    let b = self.pop();
                    let a = self.pop();
                    let dst = self.alloc_reg();
                    self.output.emit(RegOpcode::Le, dst, a, b);
                    self.push(dst);
                    ip += 1;
                }
                x if x == Opcode::Gt as u8 => {
                    let b = self.pop();
                    let a = self.pop();
                    let dst = self.alloc_reg();
                    self.output.emit(RegOpcode::Gt, dst, a, b);
                    self.push(dst);
                    ip += 1;
                }
                x if x == Opcode::Ge as u8 => {
                    let b = self.pop();
                    let a = self.pop();
                    let dst = self.alloc_reg();
                    self.output.emit(RegOpcode::Ge, dst, a, b);
                    self.push(dst);
                    ip += 1;
                }
                x if x == Opcode::Eq as u8 || x == Opcode::StrictEq as u8 => {
                    let b = self.pop();
                    let a = self.pop();
                    let dst = self.alloc_reg();
                    self.output.emit(RegOpcode::Eq, dst, a, b);
                    self.push(dst);
                    ip += 1;
                }
                
                // Logical
                x if x == Opcode::Not as u8 => {
                    let a = self.pop();
                    let dst = self.alloc_reg();
                    self.output.emit(RegOpcode::Not, dst, a, 0);
                    self.push(dst);
                    ip += 1;
                }
                
                // Control flow
                x if x == Opcode::Jump as u8 => {
                    let offset = i16::from_le_bytes([code[ip + 1], code[ip + 2]]);
                    self.output.emit(RegOpcode::Jump, 0, code[ip + 1], code[ip + 2]);
                    ip += 3;
                }
                x if x == Opcode::JumpIfFalse as u8 => {
                    let cond = self.pop();
                    self.output.emit(RegOpcode::JumpIfFalse, cond, code[ip + 1], code[ip + 2]);
                    ip += 3;
                }
                x if x == Opcode::JumpIfTrue as u8 => {
                    let cond = self.pop();
                    self.output.emit(RegOpcode::JumpIfTrue, cond, code[ip + 1], code[ip + 2]);
                    ip += 3;
                }
                
                // Return
                x if x == Opcode::Return as u8 => {
                    let ret = if self.stack.is_empty() { 0 } else { self.pop() };
                    self.output.emit(RegOpcode::Return, ret, 0, 0);
                    ip += 1;
                }
                
                // Stack ops
                x if x == Opcode::Pop as u8 => {
                    self.pop();
                    ip += 1;
                }
                x if x == Opcode::Dup as u8 => {
                    let top = self.top();
                    let dst = self.alloc_reg();
                    self.output.emit(RegOpcode::Move, dst, top, 0);
                    self.push(dst);
                    ip += 1;
                }
                
                // Objects
                x if x == Opcode::NewObject as u8 => {
                    let dst = self.alloc_reg();
                    self.output.emit(RegOpcode::NewObject, dst, 0, 0);
                    self.push(dst);
                    ip += 1;
                }
                x if x == Opcode::NewArray as u8 => {
                    let dst = self.alloc_reg();
                    self.output.emit(RegOpcode::NewArray, dst, code[ip + 1], code[ip + 2]);
                    self.push(dst);
                    ip += 3;
                }
                
                // Halt
                x if x == Opcode::Halt as u8 => {
                    self.output.emit(RegOpcode::Halt, 0, 0, 0);
                    ip += 1;
                }
                
                // Default: skip unknown opcodes
                _ => {
                    ip += 1;
                }
            }
        }
        
        // Ensure we end with a return if not already
        if self.output.instructions.is_empty() || 
           self.output.instructions.last().map(|i| i.opcode) != Some(RegOpcode::Return) {
            let ret = if self.stack.is_empty() { 0 } else { self.top() };
            self.output.emit(RegOpcode::Return, ret, 0, 0);
        }
        
        self.output
    }
}

/// Convert stack bytecode to register bytecode
pub fn convert_to_register(bytecode: &Bytecode) -> RegBytecode {
    StackToRegCompiler::new().compile(bytecode)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_simple_add() {
        // Create stack bytecode: 2 + 3
        let mut bc = Bytecode::new();
        bc.emit(Opcode::LoadConst);
        bc.constants.push(Constant::Number(2.0));
        bc.emit_u16(0);
        bc.emit(Opcode::LoadConst);
        bc.constants.push(Constant::Number(3.0));
        bc.emit_u16(1);
        bc.emit(Opcode::Add);
        bc.emit(Opcode::Return);
        
        let reg_bc = convert_to_register(&bc);
        
        // Should have: LoadConst r0, LoadConst r1, Add r2 r0 r1, Return r2
        assert!(reg_bc.instructions.len() >= 4);
    }
    
    #[test]
    fn test_compile_and_run() {
        use super::super::register_vm::RegisterVM;
        
        // Create stack bytecode: 3 + 2 = 5 using LoadZero/LoadOne
        let mut bc = Bytecode::new();
        bc.emit(Opcode::LoadZero);
        bc.emit(Opcode::LoadOne);
        bc.emit(Opcode::Add);
        bc.emit(Opcode::Return);
        
        // Convert to register bytecode
        let reg_bc = convert_to_register(&bc);
        
        // Execute with register VM
        let mut vm = RegisterVM::new();
        let result = vm.execute(&reg_bc);
        
        assert_eq!(result.to_number(), 1.0);  // 0 + 1 = 1
    }
}
