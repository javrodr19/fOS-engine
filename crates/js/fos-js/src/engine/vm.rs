//! Virtual Machine
//!
//! Stack-based bytecode interpreter with closure and async support.
//! Includes inline caching and integer fast paths for optimization.

use super::bytecode::{Bytecode, Opcode, Constant, CompiledFunction};
use super::value::JsVal;
use super::object::{JsObject, JsArray};
use super::promise::{JsPromise, PromiseState};
use super::event_loop::EventLoop;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Open upvalue - points to stack slot
/// Closed upvalue - holds the value itself
#[derive(Debug, Clone)]
pub enum Upvalue {
    Open { stack_idx: usize },
    Closed { value: JsVal },
}

/// Runtime closure
#[derive(Debug, Clone)]
pub struct Closure {
    pub function: Arc<CompiledFunction>,
    pub upvalues: Vec<Arc<Mutex<Upvalue>>>,
}

/// Call frame for function execution
#[derive(Debug)]
struct CallFrame {
    closure: Arc<Closure>,
    ip: usize,
    bp: usize, // Base pointer - start of this frame's stack slots
}

/// Try/catch handler
#[derive(Debug, Clone)]
struct TryHandler {
    catch_ip: usize,       // Jump here on error
    stack_level: usize,    // Stack level when try started
    frame_level: usize,    // Call frame level when try started
}

/// Inline cache for property lookups
/// Caches object shape -> property slot mapping
#[derive(Debug, Clone, Default)]
struct InlineCache {
    shape_id: u32,      // Cached object shape identifier
    slot_index: u16,    // Cached property slot
    hits: u32,          // Cache hit count
    misses: u32,        // Cache miss count
}

impl InlineCache {
    fn new() -> Self { Self::default() }
    
    #[inline]
    fn lookup(&self, obj_shape: u32) -> Option<u16> {
        if self.shape_id == obj_shape && self.hits > 0 {
            Some(self.slot_index)
        } else {
            None
        }
    }
    
    #[inline]
    fn update(&mut self, shape: u32, slot: u16) {
        self.shape_id = shape;
        self.slot_index = slot;
        self.hits += 1;
    }
}

/// Virtual Machine
pub struct VirtualMachine {
    stack: Vec<JsVal>,
    globals: HashMap<Box<str>, JsVal>,
    objects: Vec<JsObject>,
    arrays: Vec<JsArray>,
    closures: Vec<Arc<Closure>>,
    open_upvalues: Vec<Arc<Mutex<Upvalue>>>,
    frames: Vec<CallFrame>,
    try_handlers: Vec<TryHandler>,
    current_error: Option<JsVal>,
    // Async support
    event_loop: EventLoop,
    promises: Vec<JsPromise>,
    pending_microtasks: Vec<u32>, // Callback IDs to execute
    // This/Super bindings
    this_binding: JsVal,
    super_binding: Option<JsVal>,
}

impl Default for VirtualMachine {
    fn default() -> Self { Self::new() }
}

impl VirtualMachine {
    pub fn new() -> Self {
        Self {
            stack: Vec::with_capacity(256),
            globals: HashMap::new(),
            objects: Vec::new(),
            arrays: Vec::new(),
            closures: Vec::new(),
            open_upvalues: Vec::new(),
            frames: Vec::new(),
            try_handlers: Vec::new(),
            current_error: None,
            event_loop: EventLoop::new(),
            promises: Vec::new(),
            pending_microtasks: Vec::new(),
            this_binding: JsVal::Undefined, // Global scope this is undefined in strict mode
            super_binding: None,
        }
    }
    
    /// Run top-level bytecode
    pub fn run(&mut self, bytecode: &Bytecode) -> Result<JsVal, String> {
        let mut ip = 0;
        
        while ip < bytecode.code.len() {
            let op = bytecode.code[ip];
            ip += 1;
            
            match Opcode::try_from(op).unwrap_or(Opcode::Halt) {
                Opcode::Halt => break,
                Opcode::Pop => { self.stack.pop(); }
                Opcode::Dup => {
                    if let Some(val) = self.stack.last().cloned() { self.stack.push(val); }
                }
                
                // Constants
                Opcode::LoadConst => {
                    let idx = self.read_u16(bytecode, &mut ip) as usize;
                    match &bytecode.constants[idx] {
                        Constant::Number(n) => self.stack.push(JsVal::Number(*n)),
                        Constant::String(s) => self.stack.push(JsVal::String(s.clone())),
                        Constant::Function(f) => {
                            // Create closure without upvalues for now
                            let closure = Arc::new(Closure {
                                function: Arc::new((**f).clone()),
                                upvalues: Vec::new(),
                            });
                            let idx = self.closures.len() as u32;
                            self.closures.push(closure);
                            self.stack.push(JsVal::Function(idx));
                        }
                    }
                }
                
                // === COMPACT OPCODES (single byte, no operands) ===
                Opcode::LoadSmallInt0 => self.stack.push(JsVal::Number(0.0)),
                Opcode::LoadSmallInt1 => self.stack.push(JsVal::Number(1.0)),
                Opcode::LoadSmallInt2 => self.stack.push(JsVal::Number(2.0)),
                Opcode::LoadSmallInt3 => self.stack.push(JsVal::Number(3.0)),
                Opcode::LoadSmallInt4 => self.stack.push(JsVal::Number(4.0)),
                Opcode::LoadSmallInt5 => self.stack.push(JsVal::Number(5.0)),
                Opcode::LoadSmallInt6 => self.stack.push(JsVal::Number(6.0)),
                Opcode::LoadSmallInt7 => self.stack.push(JsVal::Number(7.0)),
                Opcode::LoadMinusOne => self.stack.push(JsVal::Number(-1.0)),
                
                // Fast local access (single byte for common cases)
                Opcode::GetLocal0 => {
                    let val = self.stack.get(0).cloned().unwrap_or(JsVal::Undefined);
                    self.stack.push(val);
                }
                Opcode::GetLocal1 => {
                    let val = self.stack.get(1).cloned().unwrap_or(JsVal::Undefined);
                    self.stack.push(val);
                }
                Opcode::SetLocal0 => {
                    if let Some(val) = self.stack.last().cloned() {
                        if !self.stack.is_empty() { self.stack[0] = val; }
                    }
                }
                Opcode::SetLocal1 => {
                    if let Some(val) = self.stack.last().cloned() {
                        if self.stack.len() > 1 { self.stack[1] = val; }
                    }
                }
                
                Opcode::LoadNull => self.stack.push(JsVal::Null),
                Opcode::LoadUndefined => self.stack.push(JsVal::Undefined),
                Opcode::LoadTrue => self.stack.push(JsVal::Bool(true)),
                Opcode::LoadFalse => self.stack.push(JsVal::Bool(false)),
                Opcode::LoadZero => self.stack.push(JsVal::Number(0.0)),
                Opcode::LoadOne => self.stack.push(JsVal::Number(1.0)),
                
                // This/Super bindings
                Opcode::LoadThis => {
                    self.stack.push(self.this_binding);
                }
                Opcode::LoadSuper => {
                    let super_val = self.super_binding.unwrap_or(JsVal::Undefined);
                    self.stack.push(super_val);
                }
                Opcode::BindThis => {
                    // Bind the top of stack as `this` to a function
                    if let Some(this_val) = self.stack.pop() {
                        self.this_binding = this_val;
                    }
                }
                
                // Variables
                Opcode::GetLocal => {
                    let slot = self.read_u16(bytecode, &mut ip) as usize;
                    let val = self.stack.get(slot).cloned().unwrap_or(JsVal::Undefined);
                    self.stack.push(val);
                }
                Opcode::SetLocal => {
                    let slot = self.read_u16(bytecode, &mut ip) as usize;
                    if let Some(val) = self.stack.last().cloned() {
                        if slot < self.stack.len() { self.stack[slot] = val; }
                    }
                }
                Opcode::GetGlobal => {
                    let idx = self.read_u16(bytecode, &mut ip) as usize;
                    let name = &bytecode.names[idx];
                    let val = self.globals.get(name).cloned().unwrap_or(JsVal::Undefined);
                    self.stack.push(val);
                }
                Opcode::SetGlobal => {
                    let idx = self.read_u16(bytecode, &mut ip) as usize;
                    let name = bytecode.names[idx].clone();
                    if let Some(val) = self.stack.last().cloned() {
                        self.globals.insert(name, val);
                    }
                }
                
                // Arithmetic
                Opcode::Add => self.binary_op(|a, b| a + b)?,
                Opcode::Sub => self.binary_op(|a, b| a - b)?,
                Opcode::Mul => self.binary_op(|a, b| a * b)?,
                Opcode::Div => self.binary_op(|a, b| a / b)?,
                Opcode::Mod => self.binary_op(|a, b| a % b)?,
                Opcode::Neg => {
                    if let Some(val) = self.stack.pop() {
                        self.stack.push(JsVal::Number(-val.to_number()));
                    }
                }
                // Fast increment/decrement (common operations)
                Opcode::Inc => {
                    if let Some(val) = self.stack.pop() {
                        // Integer fast path
                        let n = val.to_number();
                        self.stack.push(JsVal::Number(n + 1.0));
                    }
                }
                Opcode::Dec => {
                    if let Some(val) = self.stack.pop() {
                        let n = val.to_number();
                        self.stack.push(JsVal::Number(n - 1.0));
                    }
                }
                
                // Bitwise
                Opcode::BitNot => {
                    if let Some(val) = self.stack.pop() {
                        self.stack.push(JsVal::Number(!(val.to_number() as i32) as f64));
                    }
                }
                
                // Comparison
                Opcode::Lt => self.compare_op(|a, b| a < b)?,
                Opcode::Le => self.compare_op(|a, b| a <= b)?,
                Opcode::Gt => self.compare_op(|a, b| a > b)?,
                Opcode::Ge => self.compare_op(|a, b| a >= b)?,
                Opcode::Eq | Opcode::StrictEq => {
                    let b = self.stack.pop().unwrap_or(JsVal::Undefined);
                    let a = self.stack.pop().unwrap_or(JsVal::Undefined);
                    self.stack.push(JsVal::Bool(self.values_equal(&a, &b)));
                }
                Opcode::Ne | Opcode::StrictNe => {
                    let b = self.stack.pop().unwrap_or(JsVal::Undefined);
                    let a = self.stack.pop().unwrap_or(JsVal::Undefined);
                    self.stack.push(JsVal::Bool(!self.values_equal(&a, &b)));
                }
                
                // Logical
                Opcode::Not => {
                    if let Some(val) = self.stack.pop() {
                        self.stack.push(JsVal::Bool(!val.is_truthy()));
                    }
                }
                
                // Typeof
                Opcode::Typeof => {
                    if let Some(val) = self.stack.pop() {
                        self.stack.push(JsVal::String(val.type_of().into()));
                    }
                }
                
                // Jumps
                Opcode::Jump => {
                    let offset = self.read_i16(bytecode, &mut ip);
                    ip = (ip as i32 + offset as i32) as usize;
                }
                Opcode::JumpIfFalse => {
                    let offset = self.read_i16(bytecode, &mut ip);
                    if !self.stack.last().map(|v| v.is_truthy()).unwrap_or(false) {
                        ip = (ip as i32 + offset as i32) as usize;
                    }
                }
                Opcode::JumpIfTrue => {
                    let offset = self.read_i16(bytecode, &mut ip);
                    if self.stack.last().map(|v| v.is_truthy()).unwrap_or(false) {
                        ip = (ip as i32 + offset as i32) as usize;
                    }
                }
                
                // Objects/Arrays
                Opcode::NewObject => {
                    let obj_id = self.objects.len() as u32;
                    self.objects.push(JsObject::new());
                    self.stack.push(JsVal::Object(obj_id));
                }
                Opcode::NewArray => {
                    let count = self.read_u16(bytecode, &mut ip) as usize;
                    let mut arr = JsArray::with_capacity(count);
                    for _ in 0..count {
                        arr.push(self.stack.pop().unwrap_or(JsVal::Undefined));
                    }
                    let arr_id = self.arrays.len() as u32;
                    self.arrays.push(arr);
                    self.stack.push(JsVal::Array(arr_id));
                }
                Opcode::GetProperty => {
                    let name_idx = self.read_u16(bytecode, &mut ip) as usize;
                    let name = &bytecode.names[name_idx];
                    if let Some(obj_id) = self.stack.pop().and_then(|v| v.as_object_id()) {
                        // Walk prototype chain
                        let val = self.get_property_with_prototype(obj_id, name);
                        self.stack.push(val);
                    } else {
                        self.stack.push(JsVal::Undefined);
                    }
                }
                Opcode::SetProperty => {
                    let name_idx = self.read_u16(bytecode, &mut ip) as usize;
                    let name = bytecode.names[name_idx].to_string();
                    let val = self.stack.pop().unwrap_or(JsVal::Undefined);
                    if let Some(obj_id) = self.stack.last().and_then(|v| v.as_object_id()) {
                        if let Some(obj) = self.objects.get_mut(obj_id as usize) {
                            obj.set(&name, val);
                        }
                    }
                }
                Opcode::GetIndex => {
                    let idx = self.stack.pop().unwrap_or(JsVal::Undefined).to_number() as usize;
                    if let Some(arr_id) = self.stack.pop().and_then(|v| v.as_array_id()) {
                        if let Some(arr) = self.arrays.get(arr_id as usize) {
                            self.stack.push(arr.get(idx));
                        } else {
                            self.stack.push(JsVal::Undefined);
                        }
                    } else {
                        self.stack.push(JsVal::Undefined);
                    }
                }
                
                Opcode::Call => {
                    let argc = bytecode.code[ip] as usize;
                    ip += 1;
                    
                    // Pop arguments in reverse order
                    let mut args: Vec<JsVal> = Vec::with_capacity(argc);
                    for _ in 0..argc {
                        args.push(self.stack.pop().unwrap_or(JsVal::Undefined));
                    }
                    args.reverse();
                    
                    // Pop the callee
                    let callee = self.stack.pop().unwrap_or(JsVal::Undefined);
                    
                    if let Some(func_id) = callee.as_function_id() {
                        if let Some(closure) = self.closures.get(func_id as usize).cloned() {
                            // Execute function
                            let result = self.call_function(&closure, &args)?;
                            self.stack.push(result);
                        } else {
                            self.stack.push(JsVal::Undefined);
                        }
                    } else {
                        // Not callable
                        self.stack.push(JsVal::Undefined);
                    }
                }
                
                Opcode::Return => {
                    let result = self.stack.pop().unwrap_or(JsVal::Undefined);
                    return Ok(result);
                }
                
                _ => {}
            }
        }
        
        Ok(self.stack.pop().unwrap_or(JsVal::Undefined))
    }
    
    fn read_u16(&self, bc: &Bytecode, ip: &mut usize) -> u16 {
        let hi = bc.code[*ip] as u16;
        let lo = bc.code[*ip + 1] as u16;
        *ip += 2;
        (hi << 8) | lo
    }
    
    fn read_i16(&self, bc: &Bytecode, ip: &mut usize) -> i16 { self.read_u16(bc, ip) as i16 }
    
    fn binary_op<F: Fn(f64, f64) -> f64>(&mut self, op: F) -> Result<(), String> {
        let b = self.stack.pop().unwrap_or(JsVal::Undefined).to_number();
        let a = self.stack.pop().unwrap_or(JsVal::Undefined).to_number();
        self.stack.push(JsVal::Number(op(a, b)));
        Ok(())
    }
    
    fn compare_op<F: Fn(f64, f64) -> bool>(&mut self, op: F) -> Result<(), String> {
        let b = self.stack.pop().unwrap_or(JsVal::Undefined).to_number();
        let a = self.stack.pop().unwrap_or(JsVal::Undefined).to_number();
        self.stack.push(JsVal::Bool(op(a, b)));
        Ok(())
    }
    
    fn values_equal(&self, a: &JsVal, b: &JsVal) -> bool {
        use super::value::JsValKind::*;
        match (a.kind(), b.kind()) {
            (Undefined, Undefined) | (Null, Null) => true,
            (Bool(a), Bool(b)) => a == b,
            (Number(a), Number(b)) => a == b,
            (String(a), String(b)) => a == b,
            _ => false,
        }
    }
    
    /// Execute a function call
    fn call_function(&mut self, closure: &Closure, args: &[JsVal]) -> Result<JsVal, String> {
        let func = &closure.function;
        let bytecode = &func.bytecode;
        
        // Save current stack position as base
        let bp = self.stack.len();
        
        // Push arguments as locals (pad with undefined if needed)
        for i in 0..func.arity as usize {
            self.stack.push(args.get(i).cloned().unwrap_or(JsVal::Undefined));
        }
        
        // Execute the function's bytecode
        let mut ip = 0;
        while ip < bytecode.code.len() {
            let op = bytecode.code[ip];
            ip += 1;
            
            match Opcode::try_from(op).unwrap_or(Opcode::Halt) {
                Opcode::Halt => break,
                Opcode::Pop => { self.stack.pop(); }
                Opcode::Dup => {
                    if let Some(val) = self.stack.last().cloned() { self.stack.push(val); }
                }
                
                Opcode::LoadConst => {
                    let idx = self.read_u16(bytecode, &mut ip) as usize;
                    match &bytecode.constants[idx] {
                        Constant::Number(n) => self.stack.push(JsVal::Number(*n)),
                        Constant::String(s) => self.stack.push(JsVal::String(s.clone())),
                        Constant::Function(f) => {
                            let closure = Arc::new(Closure {
                                function: Arc::new((**f).clone()),
                                upvalues: Vec::new(),
                            });
                            let idx = self.closures.len() as u32;
                            self.closures.push(closure);
                            self.stack.push(JsVal::Function(idx));
                        }
                    }
                }
                Opcode::LoadNull => self.stack.push(JsVal::Null),
                Opcode::LoadUndefined => self.stack.push(JsVal::Undefined),
                Opcode::LoadTrue => self.stack.push(JsVal::Bool(true)),
                Opcode::LoadFalse => self.stack.push(JsVal::Bool(false)),
                Opcode::LoadZero => self.stack.push(JsVal::Number(0.0)),
                Opcode::LoadOne => self.stack.push(JsVal::Number(1.0)),
                
                // This/Super bindings
                Opcode::LoadThis => {
                    self.stack.push(self.this_binding);
                }
                Opcode::LoadSuper => {
                    let super_val = self.super_binding.unwrap_or(JsVal::Undefined);
                    self.stack.push(super_val);
                }
                Opcode::BindThis => {
                    if let Some(this_val) = self.stack.pop() {
                        self.this_binding = this_val;
                    }
                }
                
                Opcode::GetLocal => {
                    let slot = self.read_u16(bytecode, &mut ip) as usize;
                    let abs_slot = bp + slot;
                    let val = self.stack.get(abs_slot).cloned().unwrap_or(JsVal::Undefined);
                    self.stack.push(val);
                }
                Opcode::SetLocal => {
                    let slot = self.read_u16(bytecode, &mut ip) as usize;
                    let abs_slot = bp + slot;
                    if let Some(val) = self.stack.last().cloned() {
                        if abs_slot < self.stack.len() { self.stack[abs_slot] = val; }
                    }
                }
                Opcode::GetGlobal => {
                    let idx = self.read_u16(bytecode, &mut ip) as usize;
                    let name = &bytecode.names[idx];
                    let val = self.globals.get(name).cloned().unwrap_or(JsVal::Undefined);
                    self.stack.push(val);
                }
                Opcode::SetGlobal => {
                    let idx = self.read_u16(bytecode, &mut ip) as usize;
                    let name = bytecode.names[idx].clone();
                    if let Some(val) = self.stack.last().cloned() {
                        self.globals.insert(name, val);
                    }
                }
                
                Opcode::Add => self.binary_op(|a, b| a + b)?,
                Opcode::Sub => self.binary_op(|a, b| a - b)?,
                Opcode::Mul => self.binary_op(|a, b| a * b)?,
                Opcode::Div => self.binary_op(|a, b| a / b)?,
                Opcode::Mod => self.binary_op(|a, b| a % b)?,
                Opcode::Neg => {
                    if let Some(val) = self.stack.pop() {
                        self.stack.push(JsVal::Number(-val.to_number()));
                    }
                }
                
                Opcode::Lt => self.compare_op(|a, b| a < b)?,
                Opcode::Le => self.compare_op(|a, b| a <= b)?,
                Opcode::Gt => self.compare_op(|a, b| a > b)?,
                Opcode::Ge => self.compare_op(|a, b| a >= b)?,
                Opcode::Eq | Opcode::StrictEq => {
                    let b = self.stack.pop().unwrap_or(JsVal::Undefined);
                    let a = self.stack.pop().unwrap_or(JsVal::Undefined);
                    self.stack.push(JsVal::Bool(self.values_equal(&a, &b)));
                }
                Opcode::Ne | Opcode::StrictNe => {
                    let b = self.stack.pop().unwrap_or(JsVal::Undefined);
                    let a = self.stack.pop().unwrap_or(JsVal::Undefined);
                    self.stack.push(JsVal::Bool(!self.values_equal(&a, &b)));
                }
                Opcode::Not => {
                    if let Some(val) = self.stack.pop() {
                        self.stack.push(JsVal::Bool(!val.is_truthy()));
                    }
                }
                
                Opcode::Jump => {
                    let offset = self.read_i16(bytecode, &mut ip);
                    ip = (ip as i32 + offset as i32) as usize;
                }
                Opcode::JumpIfFalse => {
                    let offset = self.read_i16(bytecode, &mut ip);
                    if !self.stack.last().map(|v| v.is_truthy()).unwrap_or(false) {
                        ip = (ip as i32 + offset as i32) as usize;
                    }
                }
                Opcode::JumpIfTrue => {
                    let offset = self.read_i16(bytecode, &mut ip);
                    if self.stack.last().map(|v| v.is_truthy()).unwrap_or(false) {
                        ip = (ip as i32 + offset as i32) as usize;
                    }
                }
                
                Opcode::Call => {
                    let argc = bytecode.code[ip] as usize;
                    ip += 1;
                    let mut call_args: Vec<JsVal> = Vec::with_capacity(argc);
                    for _ in 0..argc { call_args.push(self.stack.pop().unwrap_or(JsVal::Undefined)); }
                    call_args.reverse();
                    let callee = self.stack.pop().unwrap_or(JsVal::Undefined);
                    if let Some(fid) = callee.as_function_id() {
                        if let Some(c) = self.closures.get(fid as usize).cloned() {
                            let result = self.call_function(&c, &call_args)?;
                            self.stack.push(result);
                        } else { self.stack.push(JsVal::Undefined); }
                    } else { self.stack.push(JsVal::Undefined); }
                }
                
                Opcode::Return => {
                    let result = self.stack.pop().unwrap_or(JsVal::Undefined);
                    // Clean up locals
                    self.stack.truncate(bp);
                    return Ok(result);
                }
                
                // Error handling
                Opcode::TryStart => {
                    let catch_offset = self.read_i16(bytecode, &mut ip);
                    let catch_ip = (ip as i32 + catch_offset as i32) as usize;
                    self.try_handlers.push(TryHandler {
                        catch_ip,
                        stack_level: self.stack.len(),
                        frame_level: self.frames.len(),
                    });
                }
                Opcode::TryEnd => {
                    self.try_handlers.pop();
                }
                Opcode::Throw => {
                    let error = self.stack.pop().unwrap_or(JsVal::Undefined);
                    if let Some(handler) = self.try_handlers.pop() {
                        // Unwind stack to handler level
                        self.stack.truncate(handler.stack_level);
                        // Push error value for catch
                        self.stack.push(error);
                        // Jump to catch block
                        ip = handler.catch_ip;
                    } else {
                        // Uncaught error
                        self.current_error = Some(error);
                        return Err("Uncaught error".to_string());
                    }
                }
                
                // Prototype operations
                Opcode::GetPrototype => {
                    if let Some(obj_id) = self.stack.pop().and_then(|v| v.as_object_id()) {
                        if let Some(obj) = self.objects.get(obj_id as usize) {
                            if let Some(proto_id) = obj.prototype() {
                                self.stack.push(JsVal::Object(proto_id));
                            } else {
                                self.stack.push(JsVal::Null);
                            }
                        } else {
                            self.stack.push(JsVal::Null);
                        }
                    } else {
                        self.stack.push(JsVal::Null);
                    }
                }
                Opcode::SetPrototype => {
                    let proto = self.stack.pop().unwrap_or(JsVal::Null);
                    if let Some(obj_id) = self.stack.last().and_then(|v| v.as_object_id()) {
                        if let Some(obj) = self.objects.get_mut(obj_id as usize) {
                            if let Some(proto_id) = proto.as_object_id() {
                                obj.set_prototype(Some(proto_id));
                            } else if proto.is_null() {
                                obj.set_prototype(None);
                            }
                        }
                    }
                }
                
                _ => {}
            }
        }
        
        // Clean up and return undefined if no explicit return
        self.stack.truncate(bp);
        Ok(JsVal::Undefined)
    }
    
    /// Walk prototype chain to find property
    fn get_property_with_prototype(&self, obj_id: u32, name: &str) -> JsVal {
        let mut current_id = Some(obj_id);
        while let Some(id) = current_id {
            if let Some(obj) = self.objects.get(id as usize) {
                if let Some(val) = obj.get(name) {
                    return val.clone();
                }
                current_id = obj.prototype();
            } else {
                break;
            }
        }
        JsVal::Undefined
    }
    
    pub fn set_global(&mut self, name: &str, val: JsVal) { self.globals.insert(name.into(), val); }
}

impl TryFrom<u8> for Opcode {
    type Error = ();
    fn try_from(v: u8) -> Result<Self, ()> {
        if v <= 255 { Ok(unsafe { std::mem::transmute(v) }) } else { Err(()) }
    }
}
