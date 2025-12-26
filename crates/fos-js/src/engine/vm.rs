//! Virtual Machine
//!
//! Stack-based bytecode interpreter.

use super::bytecode::{Bytecode, Opcode, Constant};
use super::value::JsVal;
use super::object::{JsObject, JsArray, JsFunction};
use std::collections::HashMap;

/// Call frame
#[derive(Debug)]
struct CallFrame {
    ip: usize,
    bp: usize, // Base pointer into stack
}

/// Virtual Machine
pub struct VirtualMachine {
    stack: Vec<JsVal>,
    globals: HashMap<Box<str>, JsVal>,
    objects: Vec<JsObject>,
    arrays: Vec<JsArray>,
    functions: Vec<JsFunction>,
    frames: Vec<CallFrame>,
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
            functions: Vec::new(),
            frames: Vec::new(),
        }
    }
    
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
                        Constant::Function(_) => self.stack.push(JsVal::Undefined),
                    }
                }
                Opcode::LoadNull => self.stack.push(JsVal::Null),
                Opcode::LoadUndefined => self.stack.push(JsVal::Undefined),
                Opcode::LoadTrue => self.stack.push(JsVal::Bool(true)),
                Opcode::LoadFalse => self.stack.push(JsVal::Bool(false)),
                Opcode::LoadZero => self.stack.push(JsVal::Number(0.0)),
                Opcode::LoadOne => self.stack.push(JsVal::Number(1.0)),
                
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
        match (a, b) {
            (JsVal::Undefined, JsVal::Undefined) | (JsVal::Null, JsVal::Null) => true,
            (JsVal::Bool(a), JsVal::Bool(b)) => a == b,
            (JsVal::Number(a), JsVal::Number(b)) => a == b,
            (JsVal::String(a), JsVal::String(b)) => a == b,
            _ => false,
        }
    }
    
    pub fn set_global(&mut self, name: &str, val: JsVal) { self.globals.insert(name.into(), val); }
}

impl TryFrom<u8> for Opcode {
    type Error = ();
    fn try_from(v: u8) -> Result<Self, ()> {
        if v <= 255 { Ok(unsafe { std::mem::transmute(v) }) } else { Err(()) }
    }
}
