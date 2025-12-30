//! VM Execution Hooks
//!
//! Hooks for integrating type profiling, OSR, and JIT into the VM execution loop.

use super::value::JsVal;
use super::type_profiler::{TypeProfiler, ObservedType};
use super::osr::{OsrRuntime, OsrState};
use super::jit::BaselineJit;
use std::collections::HashMap;

/// Execution hook manager
pub struct ExecutionHooks {
    /// Type profiler
    profiler: TypeProfiler,
    /// OSR runtime
    osr: OsrRuntime,
    /// JIT compiler
    jit: BaselineJit,
    /// Execution count per bytecode offset
    exec_counts: HashMap<u32, u64>,
    /// Hot threshold for type profiling detail
    profile_threshold: u64,
    /// Hot threshold for OSR consideration
    osr_threshold: u64,
}

impl Default for ExecutionHooks {
    fn default() -> Self { Self::new() }
}

impl ExecutionHooks {
    pub fn new() -> Self {
        Self {
            profiler: TypeProfiler::new(),
            osr: OsrRuntime::new(),
            jit: BaselineJit::new(),
            exec_counts: HashMap::new(),
            profile_threshold: 10,
            osr_threshold: 100,
        }
    }
    
    /// Called at start of each bytecode instruction
    pub fn on_instruction(&mut self, offset: u32) {
        let count = self.exec_counts.entry(offset).or_insert(0);
        *count += 1;
        
        // Record execution for JIT
        self.jit.record_execution(offset);
    }
    
    /// Called to profile a value at an instruction site
    pub fn profile_value(&mut self, offset: u32, value: &JsVal) {
        // Only profile at hot sites
        if self.exec_counts.get(&offset).copied().unwrap_or(0) >= self.profile_threshold {
            self.profiler.record(offset, value);
        }
    }
    
    /// Check if OSR should be attempted (at loop header)
    pub fn should_osr(&self, offset: u32) -> bool {
        self.exec_counts.get(&offset).copied().unwrap_or(0) >= self.osr_threshold
            && self.osr.osr_manager.can_osr(offset)
    }
    
    /// Attempt OSR transition
    pub fn try_osr(&mut self, offset: u32, locals: &[JsVal], stack: &[JsVal], ip: usize) 
        -> Option<OsrTransition> 
    {
        if !self.should_osr(offset) {
            return None;
        }
        
        let state = OsrState::capture(locals, stack, ip);
        self.osr.try_osr(offset, &state).map(|transfer| {
            OsrTransition {
                native_offset: transfer.native_offset,
                registers: transfer.registers,
            }
        })
    }
    
    /// Get profiled type at site
    pub fn get_profiled_type(&self, offset: u32) -> Option<ObservedType> {
        self.profiler.dominant_type(offset)
    }
    
    /// Check if site is monomorphic
    pub fn is_monomorphic(&self, offset: u32) -> bool {
        self.profiler.is_monomorphic(offset)
    }
    
    /// Check if site is polymorphic (multiple types seen)
    pub fn is_polymorphic(&self, offset: u32) -> bool {
        !self.profiler.is_monomorphic(offset)
    }
    
    /// Get execution count
    pub fn execution_count(&self, offset: u32) -> u64 {
        self.exec_counts.get(&offset).copied().unwrap_or(0)
    }
    
    /// Check if JIT compilation should happen
    pub fn should_compile(&self, offset: u32) -> bool {
        self.jit.should_compile(offset)
    }
    
    /// Get hook statistics
    pub fn stats(&self) -> HookStats {
        HookStats {
            total_sites: self.exec_counts.len(),
            profiled_sites: self.profiler.profile_count(),
            osr_entries: self.osr.osr_manager.stats().entry_count,
            hot_sites: self.exec_counts.values().filter(|&&c| c >= self.osr_threshold).count(),
        }
    }
    
    /// Set thresholds
    pub fn set_thresholds(&mut self, profile: u64, osr: u64) {
        self.profile_threshold = profile;
        self.osr_threshold = osr;
    }
    
    /// Access OSR runtime
    pub fn osr_runtime(&mut self) -> &mut OsrRuntime {
        &mut self.osr
    }
    
    /// Access type profiler
    pub fn type_profiler(&self) -> &TypeProfiler {
        &self.profiler
    }
}

/// OSR transition info
#[derive(Debug)]
pub struct OsrTransition {
    pub native_offset: u32,
    pub registers: Vec<JsVal>,
}

/// Hook statistics
#[derive(Debug, Clone)]
pub struct HookStats {
    pub total_sites: usize,
    pub profiled_sites: usize,
    pub osr_entries: usize,
    pub hot_sites: usize,
}

/// Convert JsVal to ObservedType
fn value_to_observed_type(value: &JsVal) -> ObservedType {
    if let Some(n) = value.as_number() {
        if n.fract() == 0.0 && n.abs() <= i32::MAX as f64 {
            ObservedType::Integer
        } else {
            ObservedType::Float
        }
    } else if value.as_string().is_some() {
        ObservedType::String
    } else if value.as_object_id().is_some() {
        ObservedType::Object
    } else if value.as_array_id().is_some() {
        ObservedType::Array
    } else if value.as_bool().is_some() {
        ObservedType::Boolean
    } else if value.is_null() {
        ObservedType::Null
    } else {
        ObservedType::Undefined
    }
}

/// Loop detection for OSR entry points
pub struct LoopDetector {
    /// Backward jump targets (potential loop headers)
    loop_headers: Vec<u32>,
}

impl Default for LoopDetector {
    fn default() -> Self { Self::new() }
}

impl LoopDetector {
    pub fn new() -> Self {
        Self { loop_headers: Vec::new() }
    }
    
    /// Detect loop headers from bytecode
    pub fn detect(&mut self, bytecode: &super::bytecode::Bytecode) {
        use super::bytecode::Opcode;
        
        let mut ip = 0;
        let code = &bytecode.code;
        
        while ip < code.len() {
            let op = code[ip];
            
            // Check for backward jumps
            if let Ok(opcode) = Opcode::try_from(op) {
                match opcode {
                    Opcode::Jump | Opcode::JumpIfFalse | Opcode::JumpIfTrue => {
                        if ip + 2 < code.len() {
                            let offset = i16::from_le_bytes([code[ip + 1], code[ip + 2]]);
                            let target = (ip as i32 + 3 + offset as i32) as u32;
                            
                            // Backward jump = loop
                            if (target as usize) < ip {
                                if !self.loop_headers.contains(&target) {
                                    self.loop_headers.push(target);
                                }
                            }
                        }
                        ip += 3;
                    }
                    _ => ip += 1,
                }
            } else {
                ip += 1;
            }
        }
    }
    
    /// Check if offset is a loop header
    pub fn is_loop_header(&self, offset: u32) -> bool {
        self.loop_headers.contains(&offset)
    }
    
    /// Get all loop headers
    pub fn loop_headers(&self) -> &[u32] {
        &self.loop_headers
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_execution_hooks() {
        let mut hooks = ExecutionHooks::new();
        
        hooks.on_instruction(0);
        hooks.on_instruction(0);
        hooks.on_instruction(0);
        
        assert_eq!(hooks.execution_count(0), 3);
    }
    
    #[test]
    fn test_value_profiling() {
        let mut hooks = ExecutionHooks::new();
        hooks.set_thresholds(2, 100);
        
        // Execute enough to enable profiling
        for _ in 0..5 {
            hooks.on_instruction(0);
        }
        
        hooks.profile_value(0, &JsVal::Number(42.0));
        
        assert_eq!(hooks.get_profiled_type(0), Some(ObservedType::Integer));
    }
}
