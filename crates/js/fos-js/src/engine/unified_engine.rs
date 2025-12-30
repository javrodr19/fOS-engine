//! Unified Execution Engine
//!
//! Integrates stack VM, register VM, and JIT compilation
//! with automatic tier-up based on execution profile.

use super::bytecode::Bytecode;
use super::register_vm::{RegisterVM, RegBytecode};
use super::stack_to_reg::convert_to_register;
use super::jit::{BaselineJit, JitTier};
use super::type_profiler::TypeProfiler;
use super::value::JsVal;
use std::collections::HashMap;

/// Execution tier for a function
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionTier {
    /// Interpreted via stack VM
    Interpreter,
    /// Converted to register bytecode
    RegisterVM,
    /// JIT compiled to native code
    JitCompiled,
}

/// Compiled function info
#[derive(Debug)]
struct CompiledFunction {
    tier: ExecutionTier,
    execution_count: u64,
    reg_bytecode: Option<RegBytecode>,
}

/// Unified execution engine
pub struct UnifiedEngine {
    /// Original stack bytecode
    bytecode: Bytecode,
    /// Function compilation state
    functions: HashMap<u32, CompiledFunction>,
    /// Register VM instance
    reg_vm: RegisterVM,
    /// JIT compiler
    jit: BaselineJit,
    /// Type profiler
    profiler: TypeProfiler,
    /// Tier-up thresholds
    register_threshold: u64,
    jit_threshold: u64,
}

impl UnifiedEngine {
    pub fn new(bytecode: Bytecode) -> Self {
        Self {
            bytecode,
            functions: HashMap::new(),
            reg_vm: RegisterVM::new(),
            jit: BaselineJit::new(),
            profiler: TypeProfiler::new(),
            register_threshold: 100,
            jit_threshold: 1000,
        }
    }
    
    /// Execute a function by ID
    pub fn execute_function(&mut self, func_id: u32) -> JsVal {
        // Get or create compilation state
        let func = self.functions.entry(func_id).or_insert_with(|| {
            CompiledFunction {
                tier: ExecutionTier::Interpreter,
                execution_count: 0,
                reg_bytecode: None,
            }
        });
        
        func.execution_count += 1;
        
        // Check for tier-up
        let tier = if func.execution_count >= self.jit_threshold {
            ExecutionTier::JitCompiled
        } else if func.execution_count >= self.register_threshold {
            ExecutionTier::RegisterVM
        } else {
            ExecutionTier::Interpreter
        };
        
        // Tier up if needed
        if tier != func.tier {
            self.tier_up(func_id, tier);
        }
        
        // Execute based on current tier
        match self.functions.get(&func_id).map(|f| f.tier) {
            Some(ExecutionTier::RegisterVM) => {
                if let Some(ref reg_bc) = self.functions.get(&func_id).and_then(|f| f.reg_bytecode.as_ref()) {
                    return self.reg_vm.execute(reg_bc);
                }
            }
            Some(ExecutionTier::JitCompiled) => {
                // For now, fall back to register VM 
                // Full JIT execution requires mprotect
                if let Some(ref reg_bc) = self.functions.get(&func_id).and_then(|f| f.reg_bytecode.as_ref()) {
                    return self.reg_vm.execute(reg_bc);
                }
            }
            _ => {}
        }
        
        // Default: interpret
        JsVal::Undefined
    }
    
    /// Tier up a function
    fn tier_up(&mut self, func_id: u32, new_tier: ExecutionTier) {
        if let Some(func) = self.functions.get_mut(&func_id) {
            match new_tier {
                ExecutionTier::RegisterVM => {
                    // Convert to register bytecode
                    let reg_bc = convert_to_register(&self.bytecode);
                    func.reg_bytecode = Some(reg_bc);
                    func.tier = ExecutionTier::RegisterVM;
                }
                ExecutionTier::JitCompiled => {
                    // Ensure we have register bytecode
                    if func.reg_bytecode.is_none() {
                        let reg_bc = convert_to_register(&self.bytecode);
                        func.reg_bytecode = Some(reg_bc);
                    }
                    func.tier = ExecutionTier::JitCompiled;
                }
                _ => {}
            }
        }
    }
    
    /// Execute the main bytecode directly
    pub fn execute(&mut self) -> JsVal {
        // Simple path: convert and run
        let reg_bc = convert_to_register(&self.bytecode);
        self.reg_vm.execute(&reg_bc)
    }
    
    /// Get execution stats
    pub fn stats(&self) -> EngineStats {
        let mut interpreter_count = 0;
        let mut register_count = 0;
        let mut jit_count = 0;
        
        for func in self.functions.values() {
            match func.tier {
                ExecutionTier::Interpreter => interpreter_count += 1,
                ExecutionTier::RegisterVM => register_count += 1,
                ExecutionTier::JitCompiled => jit_count += 1,
            }
        }
        
        EngineStats {
            functions_interpreted: interpreter_count,
            functions_register: register_count,
            functions_jit: jit_count,
        }
    }
    
    /// Set tier-up thresholds
    pub fn set_thresholds(&mut self, register: u64, jit: u64) {
        self.register_threshold = register;
        self.jit_threshold = jit;
    }
}

#[derive(Debug, Clone)]
pub struct EngineStats {
    pub functions_interpreted: usize,
    pub functions_register: usize,
    pub functions_jit: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::bytecode::Opcode;
    
    #[test]
    fn test_unified_execute() {
        let mut bc = Bytecode::new();
        bc.emit(Opcode::LoadZero);
        bc.emit(Opcode::LoadOne);
        bc.emit(Opcode::Add);
        bc.emit(Opcode::Return);
        
        let mut engine = UnifiedEngine::new(bc);
        let result = engine.execute();
        
        assert_eq!(result.to_number(), 1.0);
    }
    
    #[test]
    fn test_tier_up() {
        let mut bc = Bytecode::new();
        bc.emit(Opcode::LoadZero);
        bc.emit(Opcode::Return);
        
        let mut engine = UnifiedEngine::new(bc);
        engine.set_thresholds(2, 5);
        
        // First few calls - interpreter tier
        for _ in 0..2 {
            engine.execute_function(0);
        }
        
        // Should tier up to register VM
        engine.execute_function(0);
        assert_eq!(engine.functions.get(&0).unwrap().tier, ExecutionTier::RegisterVM);
    }
}
