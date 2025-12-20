//! JIT-Compiled JavaScript Bytecode (Phase 24.4)
//!
//! Emit bytecode for hot loops. Interpret cold, compile hot.
//! Tiered compilation. Profile-guided optimization.

use std::collections::HashMap;

/// Bytecode operation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Opcode {
    /// No operation
    Nop = 0,
    /// Load constant to register
    LoadConst = 1,
    /// Load from local variable
    LoadLocal = 2,
    /// Store to local variable
    StoreLocal = 3,
    /// Add two registers
    Add = 4,
    /// Subtract
    Sub = 5,
    /// Multiply
    Mul = 6,
    /// Divide
    Div = 7,
    /// Modulo
    Mod = 8,
    /// Compare equal
    Eq = 9,
    /// Compare not equal
    Ne = 10,
    /// Less than
    Lt = 11,
    /// Less or equal
    Le = 12,
    /// Greater than
    Gt = 13,
    /// Greater or equal
    Ge = 14,
    /// Logical and
    And = 15,
    /// Logical or
    Or = 16,
    /// Logical not
    Not = 17,
    /// Jump
    Jump = 18,
    /// Jump if false
    JumpIfFalse = 19,
    /// Jump if true
    JumpIfTrue = 20,
    /// Call function
    Call = 21,
    /// Return
    Return = 22,
    /// Push to stack
    Push = 23,
    /// Pop from stack
    Pop = 24,
    /// Get property
    GetProp = 25,
    /// Set property
    SetProp = 26,
    /// Create object
    NewObject = 27,
    /// Create array
    NewArray = 28,
    /// Increment
    Inc = 29,
    /// Decrement
    Dec = 30,
}

/// Single bytecode instruction
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Instruction {
    /// Operation code
    pub op: Opcode,
    /// First operand
    pub a: u8,
    /// Second operand
    pub b: u8,
    /// Third operand
    pub c: u8,
}

impl Instruction {
    pub fn new(op: Opcode, a: u8, b: u8, c: u8) -> Self {
        Self { op, a, b, c }
    }
    
    pub fn simple(op: Opcode) -> Self {
        Self { op, a: 0, b: 0, c: 0 }
    }
}

/// Compiled function bytecode
#[derive(Debug, Clone)]
pub struct BytecodeFunction {
    /// Function name
    pub name: Box<str>,
    /// Parameter count
    pub params: u8,
    /// Local variable count
    pub locals: u8,
    /// Instructions
    pub code: Vec<Instruction>,
    /// Constants pool
    pub constants: Vec<JsValue>,
    /// Execution count (for JIT)
    pub exec_count: u64,
    /// Is this a hot function?
    pub is_hot: bool,
}

/// JavaScript value (simplified)
#[derive(Debug, Clone)]
pub enum JsValue {
    Undefined,
    Null,
    Boolean(bool),
    Number(f64),
    String(Box<str>),
    Object(u32), // Object reference
    Function(u32), // Function reference
}

impl BytecodeFunction {
    pub fn new(name: &str, params: u8, locals: u8) -> Self {
        Self {
            name: name.into(),
            params,
            locals,
            code: Vec::new(),
            constants: Vec::new(),
            exec_count: 0,
            is_hot: false,
        }
    }
    
    /// Add an instruction
    pub fn emit(&mut self, instr: Instruction) {
        self.code.push(instr);
    }
    
    /// Add a constant
    pub fn add_const(&mut self, value: JsValue) -> u8 {
        let idx = self.constants.len();
        self.constants.push(value);
        idx as u8
    }
    
    /// Size in bytes
    pub fn size(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.code.len() * std::mem::size_of::<Instruction>()
            + self.constants.len() * std::mem::size_of::<JsValue>()
    }
}

/// Execution tier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// Interpreted (cold)
    Interpreter,
    /// Baseline JIT (warm)
    Baseline,
    /// Optimized JIT (hot)
    Optimized,
}

/// Profiling data for a function
#[derive(Debug, Clone, Default)]
pub struct ProfileData {
    /// Execution count
    pub exec_count: u64,
    /// Total execution time (ns)
    pub total_time_ns: u64,
    /// Loop counts
    pub loop_counts: HashMap<usize, u64>,
    /// Type feedback
    pub type_feedback: HashMap<usize, TypeFeedback>,
}

/// Type feedback for a specific site
#[derive(Debug, Clone)]
pub struct TypeFeedback {
    /// Observed types
    pub types: Vec<ObservedType>,
    /// Is monomorphic?
    pub is_mono: bool,
}

/// Observed type at a site
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservedType {
    Undefined,
    Null,
    Boolean,
    Number,
    String,
    Object,
    Array,
    Function,
}

/// JIT compilation thresholds
#[derive(Debug, Clone)]
pub struct JitThresholds {
    /// Executions before baseline compile
    pub baseline_threshold: u64,
    /// Executions before optimized compile
    pub optimize_threshold: u64,
    /// Loop iterations before OSR
    pub osr_threshold: u64,
}

impl Default for JitThresholds {
    fn default() -> Self {
        Self {
            baseline_threshold: 100,
            optimize_threshold: 10000,
            osr_threshold: 1000,
        }
    }
}

/// JIT compiler state
#[derive(Debug)]
pub struct JitCompiler {
    /// Functions
    functions: HashMap<u32, BytecodeFunction>,
    /// Profiling data
    profiles: HashMap<u32, ProfileData>,
    /// Compilation tiers
    tiers: HashMap<u32, Tier>,
    /// Thresholds
    thresholds: JitThresholds,
    /// Statistics
    stats: JitStats,
    /// Next function ID
    next_id: u32,
}

/// JIT statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct JitStats {
    pub functions_interpreted: u64,
    pub functions_baseline: u64,
    pub functions_optimized: u64,
    pub osr_compilations: u64,
    pub deoptimizations: u64,
}

impl Default for JitCompiler {
    fn default() -> Self {
        Self::new()
    }
}

impl JitCompiler {
    pub fn new() -> Self {
        Self {
            functions: HashMap::new(),
            profiles: HashMap::new(),
            tiers: HashMap::new(),
            thresholds: JitThresholds::default(),
            stats: JitStats::default(),
            next_id: 0,
        }
    }
    
    /// Set thresholds
    pub fn with_thresholds(mut self, thresholds: JitThresholds) -> Self {
        self.thresholds = thresholds;
        self
    }
    
    /// Register a function
    pub fn register(&mut self, func: BytecodeFunction) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        
        self.functions.insert(id, func);
        self.profiles.insert(id, ProfileData::default());
        self.tiers.insert(id, Tier::Interpreter);
        self.stats.functions_interpreted += 1;
        
        id
    }
    
    /// Record function execution
    pub fn record_execution(&mut self, func_id: u32, time_ns: u64) {
        if let Some(profile) = self.profiles.get_mut(&func_id) {
            profile.exec_count += 1;
            profile.total_time_ns += time_ns;
            
            // Check for tier upgrade
            self.check_tier_upgrade(func_id);
        }
    }
    
    /// Check if function should be promoted
    fn check_tier_upgrade(&mut self, func_id: u32) {
        let profile = match self.profiles.get(&func_id) {
            Some(p) => p,
            None => return,
        };
        
        let current_tier = self.tiers.get(&func_id).copied().unwrap_or(Tier::Interpreter);
        
        match current_tier {
            Tier::Interpreter if profile.exec_count >= self.thresholds.baseline_threshold => {
                self.compile_baseline(func_id);
            }
            Tier::Baseline if profile.exec_count >= self.thresholds.optimize_threshold => {
                self.compile_optimized(func_id);
            }
            _ => {}
        }
    }
    
    /// Compile to baseline tier
    fn compile_baseline(&mut self, func_id: u32) {
        if let Some(func) = self.functions.get_mut(&func_id) {
            func.is_hot = true;
        }
        self.tiers.insert(func_id, Tier::Baseline);
        self.stats.functions_baseline += 1;
    }
    
    /// Compile to optimized tier
    fn compile_optimized(&mut self, func_id: u32) {
        self.tiers.insert(func_id, Tier::Optimized);
        self.stats.functions_optimized += 1;
    }
    
    /// Record loop iteration
    pub fn record_loop(&mut self, func_id: u32, offset: usize) {
        if let Some(profile) = self.profiles.get_mut(&func_id) {
            *profile.loop_counts.entry(offset).or_insert(0) += 1;
            
            // Check for on-stack replacement
            if profile.loop_counts[&offset] >= self.thresholds.osr_threshold {
                self.osr_compile(func_id, offset);
            }
        }
    }
    
    /// On-stack replacement compile
    fn osr_compile(&mut self, func_id: u32, _offset: usize) {
        if self.tiers.get(&func_id) == Some(&Tier::Interpreter) {
            self.compile_baseline(func_id);
            self.stats.osr_compilations += 1;
        }
    }
    
    /// Record type feedback
    pub fn record_type(&mut self, func_id: u32, site: usize, observed: ObservedType) {
        if let Some(profile) = self.profiles.get_mut(&func_id) {
            let feedback = profile.type_feedback.entry(site).or_insert_with(|| TypeFeedback {
                types: Vec::new(),
                is_mono: true,
            });
            
            if !feedback.types.contains(&observed) {
                feedback.types.push(observed);
                if feedback.types.len() > 1 {
                    feedback.is_mono = false;
                }
            }
        }
    }
    
    /// Deoptimize a function
    pub fn deoptimize(&mut self, func_id: u32) {
        self.tiers.insert(func_id, Tier::Interpreter);
        self.stats.deoptimizations += 1;
        
        // Clear profiling data
        if let Some(profile) = self.profiles.get_mut(&func_id) {
            profile.exec_count = 0;
            profile.loop_counts.clear();
        }
    }
    
    /// Get function tier
    pub fn get_tier(&self, func_id: u32) -> Option<Tier> {
        self.tiers.get(&func_id).copied()
    }
    
    /// Get function
    pub fn get_function(&self, func_id: u32) -> Option<&BytecodeFunction> {
        self.functions.get(&func_id)
    }
    
    /// Get statistics
    pub fn stats(&self) -> &JitStats {
        &self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_bytecode_function() {
        let mut func = BytecodeFunction::new("add", 2, 1);
        
        // Load a + b
        func.emit(Instruction::new(Opcode::LoadLocal, 0, 0, 0)); // load a
        func.emit(Instruction::new(Opcode::LoadLocal, 1, 1, 0)); // load b
        func.emit(Instruction::new(Opcode::Add, 2, 0, 1)); // r2 = r0 + r1
        func.emit(Instruction::simple(Opcode::Return));
        
        assert_eq!(func.code.len(), 4);
    }
    
    #[test]
    fn test_jit_tiering() {
        let thresholds = JitThresholds {
            baseline_threshold: 5,
            optimize_threshold: 20,
            osr_threshold: 10,
        };
        
        let mut jit = JitCompiler::new().with_thresholds(thresholds);
        
        let func = BytecodeFunction::new("test", 0, 0);
        let func_id = jit.register(func);
        
        assert_eq!(jit.get_tier(func_id), Some(Tier::Interpreter));
        
        // Execute 5 times
        for _ in 0..5 {
            jit.record_execution(func_id, 1000);
        }
        
        assert_eq!(jit.get_tier(func_id), Some(Tier::Baseline));
        
        // Execute more
        for _ in 0..15 {
            jit.record_execution(func_id, 1000);
        }
        
        assert_eq!(jit.get_tier(func_id), Some(Tier::Optimized));
    }
    
    #[test]
    fn test_type_feedback() {
        let mut jit = JitCompiler::new();
        
        let func = BytecodeFunction::new("poly", 1, 0);
        let func_id = jit.register(func);
        
        // Monomorphic at first
        jit.record_type(func_id, 0, ObservedType::Number);
        
        if let Some(profile) = jit.profiles.get(&func_id) {
            let feedback = profile.type_feedback.get(&0).unwrap();
            assert!(feedback.is_mono);
        }
        
        // Becomes polymorphic
        jit.record_type(func_id, 0, ObservedType::String);
        
        if let Some(profile) = jit.profiles.get(&func_id) {
            let feedback = profile.type_feedback.get(&0).unwrap();
            assert!(!feedback.is_mono);
        }
    }
}
