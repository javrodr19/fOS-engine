//! Optimized Virtual Machine
//!
//! Integrates all optimization components into a single execution engine:
//! - Execution hooks for profiling
//! - Register VM for hot functions
//! - JIT compilation pipeline
//! - OSR for loop optimization

use super::bytecode::{Bytecode, Opcode};
use super::value::JsVal;
use super::object::{JsObject, JsArray};
use super::execution_hooks::{ExecutionHooks, LoopDetector};
use super::register_vm::RegisterVM;
use super::stack_to_reg::convert_to_register;
use super::reg_to_native::compile_to_native;
use super::osr::{OsrEntry, LocalSlot, SlotLocation};
use std::collections::HashMap;

/// Optimized VM with full JIT pipeline
pub struct OptimizedVM {
    // Execution state
    stack: Vec<JsVal>,
    locals: Vec<JsVal>,
    ip: usize,
    
    // Object storage
    objects: Vec<JsObject>,
    arrays: Vec<JsArray>,
    globals: HashMap<Box<str>, JsVal>,
    
    // Optimization components
    hooks: ExecutionHooks,
    loop_detector: LoopDetector,
    reg_vm: RegisterVM,
    
    // Compilation cache
    compiled_functions: HashMap<u32, CompiledFunction>,
    
    // Configuration
    config: VMConfig,
}

#[derive(Debug, Clone)]
pub struct VMConfig {
    pub enable_profiling: bool,
    pub enable_jit: bool,
    pub enable_osr: bool,
    pub tier_up_threshold: u64,
    pub osr_threshold: u64,
}

impl Default for VMConfig {
    fn default() -> Self {
        Self {
            enable_profiling: true,
            enable_jit: true,
            enable_osr: true,
            tier_up_threshold: 100,
            osr_threshold: 50,
        }
    }
}

struct CompiledFunction {
    reg_bytecode: super::register_vm::RegBytecode,
    native_code: Option<Vec<u8>>,
    execution_count: u64,
}

impl Default for OptimizedVM {
    fn default() -> Self { Self::new() }
}

impl OptimizedVM {
    pub fn new() -> Self {
        Self::with_config(VMConfig::default())
    }
    
    pub fn with_config(config: VMConfig) -> Self {
        let mut hooks = ExecutionHooks::new();
        hooks.set_thresholds(10, config.osr_threshold);
        
        Self {
            stack: Vec::with_capacity(256),
            locals: vec![JsVal::Undefined; 256],
            ip: 0,
            objects: Vec::new(),
            arrays: Vec::new(),
            globals: HashMap::new(),
            hooks,
            loop_detector: LoopDetector::new(),
            reg_vm: RegisterVM::new(),
            compiled_functions: HashMap::new(),
            config,
        }
    }
    
    /// Execute bytecode with full optimization pipeline
    pub fn execute(&mut self, bytecode: &Bytecode) -> JsVal {
        // Detect loops for OSR entry points
        if self.config.enable_osr {
            self.loop_detector.detect(bytecode);
            self.register_osr_entries(bytecode);
        }
        
        // Main execution loop
        self.execute_bytecode(bytecode)
    }
    
    fn execute_bytecode(&mut self, bytecode: &Bytecode) -> JsVal {
        let code = &bytecode.code;
        self.ip = 0;
        
        while self.ip < code.len() {
            let op = code[self.ip];
            let offset = self.ip as u32;
            
            // Execution hooks
            if self.config.enable_profiling {
                self.hooks.on_instruction(offset);
            }
            
            // Check for JIT tier-up at loop headers
            if self.config.enable_jit && self.loop_detector.is_loop_header(offset) {
                if self.hooks.execution_count(offset) >= self.config.tier_up_threshold {
                    if let Some(result) = self.try_tier_up(bytecode, offset) {
                        return result;
                    }
                }
            }
            
            // Execute instruction
            match self.execute_instruction(bytecode, op) {
                ExecuteResult::Continue => {}
                ExecuteResult::Return(val) => return val,
                ExecuteResult::Jump(target) => {
                    self.ip = target;
                    continue;
                }
            }
            
            self.ip += 1;
        }
        
        self.stack.pop().unwrap_or(JsVal::Undefined)
    }
    
    fn execute_instruction(&mut self, bytecode: &Bytecode, op: u8) -> ExecuteResult {
        let code = &bytecode.code;
        
        match op {
            // Load constants
            x if x == Opcode::LoadConst as u8 => {
                if self.ip + 2 < code.len() {
                    let idx = u16::from_le_bytes([code[self.ip + 1], code[self.ip + 2]]) as usize;
                    let val = if let Some(c) = bytecode.constants.get(idx) {
                        use super::bytecode::Constant;
                        match c {
                            Constant::Number(n) => JsVal::Number(*n),
                            Constant::String(s) => JsVal::String(s.clone()),
                            _ => JsVal::Undefined,
                        }
                    } else {
                        JsVal::Undefined
                    };
                    
                    // Profile the value
                    if self.config.enable_profiling {
                        self.hooks.profile_value(self.ip as u32, &val);
                    }
                    
                    self.stack.push(val);
                    self.ip += 2;
                }
            }
            x if x == Opcode::LoadNull as u8 => self.stack.push(JsVal::Null),
            x if x == Opcode::LoadUndefined as u8 => self.stack.push(JsVal::Undefined),
            x if x == Opcode::LoadTrue as u8 => self.stack.push(JsVal::Bool(true)),
            x if x == Opcode::LoadFalse as u8 => self.stack.push(JsVal::Bool(false)),
            x if x == Opcode::LoadZero as u8 => self.stack.push(JsVal::Number(0.0)),
            x if x == Opcode::LoadOne as u8 => self.stack.push(JsVal::Number(1.0)),
            
            // Arithmetic with profiling
            x if x == Opcode::Add as u8 => {
                let b = self.stack.pop().unwrap_or(JsVal::Undefined);
                let a = self.stack.pop().unwrap_or(JsVal::Undefined);
                let result = JsVal::Number(a.to_number() + b.to_number());
                
                if self.config.enable_profiling {
                    self.hooks.profile_value(self.ip as u32, &result);
                }
                
                self.stack.push(result);
            }
            x if x == Opcode::Sub as u8 => {
                let b = self.stack.pop().unwrap_or(JsVal::Undefined);
                let a = self.stack.pop().unwrap_or(JsVal::Undefined);
                self.stack.push(JsVal::Number(a.to_number() - b.to_number()));
            }
            x if x == Opcode::Mul as u8 => {
                let b = self.stack.pop().unwrap_or(JsVal::Undefined);
                let a = self.stack.pop().unwrap_or(JsVal::Undefined);
                self.stack.push(JsVal::Number(a.to_number() * b.to_number()));
            }
            x if x == Opcode::Div as u8 => {
                let b = self.stack.pop().unwrap_or(JsVal::Undefined);
                let a = self.stack.pop().unwrap_or(JsVal::Undefined);
                self.stack.push(JsVal::Number(a.to_number() / b.to_number()));
            }
            x if x == Opcode::Neg as u8 => {
                let a = self.stack.pop().unwrap_or(JsVal::Undefined);
                self.stack.push(JsVal::Number(-a.to_number()));
            }
            
            // Comparison
            x if x == Opcode::Lt as u8 => {
                let b = self.stack.pop().unwrap_or(JsVal::Undefined);
                let a = self.stack.pop().unwrap_or(JsVal::Undefined);
                self.stack.push(JsVal::Bool(a.to_number() < b.to_number()));
            }
            x if x == Opcode::Le as u8 => {
                let b = self.stack.pop().unwrap_or(JsVal::Undefined);
                let a = self.stack.pop().unwrap_or(JsVal::Undefined);
                self.stack.push(JsVal::Bool(a.to_number() <= b.to_number()));
            }
            x if x == Opcode::Gt as u8 => {
                let b = self.stack.pop().unwrap_or(JsVal::Undefined);
                let a = self.stack.pop().unwrap_or(JsVal::Undefined);
                self.stack.push(JsVal::Bool(a.to_number() > b.to_number()));
            }
            x if x == Opcode::Ge as u8 => {
                let b = self.stack.pop().unwrap_or(JsVal::Undefined);
                let a = self.stack.pop().unwrap_or(JsVal::Undefined);
                self.stack.push(JsVal::Bool(a.to_number() >= b.to_number()));
            }
            
            // Control flow
            x if x == Opcode::Jump as u8 => {
                if self.ip + 2 < code.len() {
                    let offset = i16::from_le_bytes([code[self.ip + 1], code[self.ip + 2]]);
                    let target = (self.ip as i32 + 3 + offset as i32) as usize;
                    return ExecuteResult::Jump(target);
                }
            }
            x if x == Opcode::JumpIfFalse as u8 => {
                if self.ip + 2 < code.len() {
                    let cond = self.stack.pop().unwrap_or(JsVal::Undefined);
                    if !cond.is_truthy() {
                        let offset = i16::from_le_bytes([code[self.ip + 1], code[self.ip + 2]]);
                        let target = (self.ip as i32 + 3 + offset as i32) as usize;
                        return ExecuteResult::Jump(target);
                    }
                    self.ip += 2;
                }
            }
            x if x == Opcode::JumpIfTrue as u8 => {
                if self.ip + 2 < code.len() {
                    let cond = self.stack.pop().unwrap_or(JsVal::Undefined);
                    if cond.is_truthy() {
                        let offset = i16::from_le_bytes([code[self.ip + 1], code[self.ip + 2]]);
                        let target = (self.ip as i32 + 3 + offset as i32) as usize;
                        return ExecuteResult::Jump(target);
                    }
                    self.ip += 2;
                }
            }
            
            // Locals
            x if x == Opcode::GetLocal as u8 => {
                if self.ip + 2 < code.len() {
                    let slot = u16::from_le_bytes([code[self.ip + 1], code[self.ip + 2]]) as usize;
                    let val = self.locals.get(slot).copied().unwrap_or(JsVal::Undefined);
                    self.stack.push(val);
                    self.ip += 2;
                }
            }
            x if x == Opcode::SetLocal as u8 => {
                if self.ip + 2 < code.len() {
                    let slot = u16::from_le_bytes([code[self.ip + 1], code[self.ip + 2]]) as usize;
                    let val = self.stack.last().copied().unwrap_or(JsVal::Undefined);
                    if slot < self.locals.len() {
                        self.locals[slot] = val;
                    }
                    self.ip += 2;
                }
            }
            
            // Stack ops
            x if x == Opcode::Pop as u8 => { self.stack.pop(); }
            x if x == Opcode::Dup as u8 => {
                if let Some(&top) = self.stack.last() {
                    self.stack.push(top);
                }
            }
            
            // Return
            x if x == Opcode::Return as u8 => {
                return ExecuteResult::Return(self.stack.pop().unwrap_or(JsVal::Undefined));
            }
            x if x == Opcode::Halt as u8 => {
                return ExecuteResult::Return(self.stack.pop().unwrap_or(JsVal::Undefined));
            }
            
            _ => {}
        }
        
        ExecuteResult::Continue
    }
    
    /// Try to tier up to register VM or JIT
    fn try_tier_up(&mut self, bytecode: &Bytecode, offset: u32) -> Option<JsVal> {
        // Convert to register bytecode if not already cached
        if !self.compiled_functions.contains_key(&offset) {
            let reg_bc = convert_to_register(bytecode);
            let native_code = if self.hooks.execution_count(offset) >= self.config.tier_up_threshold * 10 {
                Some(compile_to_native(&reg_bc).code)
            } else {
                None
            };
            
            self.compiled_functions.insert(offset, CompiledFunction {
                reg_bytecode: reg_bc,
                native_code,
                execution_count: 0,
            });
        }
        
        // Execute with register VM
        if let Some(func) = self.compiled_functions.get_mut(&offset) {
            func.execution_count += 1;
            let result = self.reg_vm.execute(&func.reg_bytecode);
            return Some(result);
        }
        
        None
    }
    
    /// Register OSR entry points for loops
    fn register_osr_entries(&mut self, bytecode: &Bytecode) {
        for &header in self.loop_detector.loop_headers() {
            let entry = OsrEntry {
                bytecode_offset: header,
                native_offset: 0, // Set when JIT compiles
                local_mapping: (0..16).map(|i| LocalSlot {
                    local_idx: i,
                    location: SlotLocation::Register(i as u8),
                }).collect(),
            };
            self.hooks.osr_runtime().osr_manager.register_entry(entry);
        }
    }
    
    /// Get execution statistics
    pub fn stats(&self) -> VMStats {
        let hook_stats = self.hooks.stats();
        VMStats {
            instructions_executed: hook_stats.total_sites,
            functions_compiled: self.compiled_functions.len(),
            hot_sites: hook_stats.hot_sites,
            osr_entries: hook_stats.osr_entries,
        }
    }
}

enum ExecuteResult {
    Continue,
    Return(JsVal),
    Jump(usize),
}

#[derive(Debug, Clone)]
pub struct VMStats {
    pub instructions_executed: usize,
    pub functions_compiled: usize,
    pub hot_sites: usize,
    pub osr_entries: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_optimized_vm() {
        let mut bc = Bytecode::new();
        bc.emit(Opcode::LoadZero);
        bc.emit(Opcode::LoadOne);
        bc.emit(Opcode::Add);
        bc.emit(Opcode::Return);
        
        let mut vm = OptimizedVM::new();
        let result = vm.execute(&bc);
        
        assert_eq!(result.to_number(), 1.0);
    }
    
    #[test]
    fn test_with_profiling() {
        let config = VMConfig {
            enable_profiling: true,
            enable_jit: false,
            enable_osr: false,
            ..Default::default()
        };
        
        let mut bc = Bytecode::new();
        bc.emit(Opcode::LoadOne);
        bc.emit(Opcode::Return);
        
        let mut vm = OptimizedVM::with_config(config);
        let result = vm.execute(&bc);
        
        assert_eq!(result.to_number(), 1.0);
    }
}
