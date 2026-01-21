//! Tiered Compiler
//!
//! Multi-tier compilation infrastructure for the JavaScript engine.
//! Functions start interpreted and get compiled to increasingly optimized
//! native code as they become "hot".
//!
//! ## Tiers
//! - **Interpreter**: Bytecode execution, collects type feedback
//! - **Baseline JIT**: Fast template-based compilation, moderate speedup
//! - **Optimized JIT**: SSA-based, type-specialized, highest performance
//!
//! ## Compilation Policy
//! - After 100 calls: compile to Baseline
//! - After 1000 calls with stable types: compile to Optimized
//! - On type stability violation: deoptimize back to Baseline

use std::collections::HashMap;
use super::bytecode::Bytecode;
use super::type_profiler::{TypeProfiler, ObservedType};

/// Compilation tier
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CompileTier {
    /// Interpreted bytecode
    Interpreter = 0,
    /// Baseline JIT (template-based, fast compile)
    Baseline = 1,
    /// Optimizing JIT (SSA-based, slow compile, fast run)
    Optimized = 2,
}

impl Default for CompileTier {
    fn default() -> Self {
        Self::Interpreter
    }
}

/// Function compilation state
#[derive(Debug, Clone)]
pub struct FunctionCompileState {
    /// Function ID
    pub func_id: u32,
    /// Current compilation tier
    pub tier: CompileTier,
    /// Total call count
    pub call_count: u64,
    /// Calls since last tier change
    pub calls_at_tier: u64,
    /// Number of deoptimizations
    pub deopt_count: u32,
    /// Whether function is currently being compiled
    pub compiling: bool,
    /// Recorded type stability (true = types are stable)
    pub type_stable: bool,
}

impl FunctionCompileState {
    pub fn new(func_id: u32) -> Self {
        Self {
            func_id,
            tier: CompileTier::Interpreter,
            call_count: 0,
            calls_at_tier: 0,
            deopt_count: 0,
            compiling: false,
            type_stable: true,
        }
    }

    /// Record a function call
    pub fn record_call(&mut self) {
        self.call_count = self.call_count.saturating_add(1);
        self.calls_at_tier = self.calls_at_tier.saturating_add(1);
    }

    /// Check if function should be compiled to next tier
    pub fn should_upgrade(&self, policy: &CompilationPolicy) -> bool {
        if self.compiling {
            return false;
        }

        match self.tier {
            CompileTier::Interpreter => {
                self.calls_at_tier >= policy.baseline_threshold as u64
            }
            CompileTier::Baseline => {
                self.type_stable && 
                self.calls_at_tier >= policy.opt_threshold as u64 &&
                self.deopt_count < policy.max_deopt_count
            }
            CompileTier::Optimized => false,
        }
    }

    /// Upgrade to next tier
    pub fn upgrade(&mut self) {
        match self.tier {
            CompileTier::Interpreter => {
                self.tier = CompileTier::Baseline;
                self.calls_at_tier = 0;
            }
            CompileTier::Baseline => {
                self.tier = CompileTier::Optimized;
                self.calls_at_tier = 0;
            }
            CompileTier::Optimized => {}
        }
    }

    /// Deoptimize to lower tier
    pub fn deoptimize(&mut self) {
        self.deopt_count += 1;
        match self.tier {
            CompileTier::Optimized => {
                self.tier = CompileTier::Baseline;
                self.calls_at_tier = 0;
            }
            CompileTier::Baseline => {
                self.tier = CompileTier::Interpreter;
                self.calls_at_tier = 0;
            }
            CompileTier::Interpreter => {}
        }
    }
}

/// Compilation policy configuration
#[derive(Debug, Clone)]
pub struct CompilationPolicy {
    /// Calls before baseline compilation
    pub baseline_threshold: u32,
    /// Calls before optimizing compilation
    pub opt_threshold: u32,
    /// Maximum deoptimizations before giving up on optimization
    pub max_deopt_count: u32,
    /// Whether to compile in background thread
    pub background_compile: bool,
}

impl Default for CompilationPolicy {
    fn default() -> Self {
        Self {
            baseline_threshold: 100,
            opt_threshold: 1000,
            max_deopt_count: 3,
            background_compile: true,
        }
    }
}

/// Tiered compiler manager
#[derive(Debug)]
pub struct TieredCompiler {
    /// Compilation policy
    policy: CompilationPolicy,
    /// Per-function compilation state
    function_states: HashMap<u32, FunctionCompileState>,
    /// Baseline compiled code cache
    baseline_code: HashMap<u32, BaselineCode>,
    /// Optimized compiled code cache
    optimized_code: HashMap<u32, OptimizedCode>,
    /// Compilation statistics
    stats: TieredCompilerStats,
}

impl Default for TieredCompiler {
    fn default() -> Self {
        Self::new()
    }
}

impl TieredCompiler {
    pub fn new() -> Self {
        Self::with_policy(CompilationPolicy::default())
    }

    pub fn with_policy(policy: CompilationPolicy) -> Self {
        Self {
            policy,
            function_states: HashMap::new(),
            baseline_code: HashMap::new(),
            optimized_code: HashMap::new(),
            stats: TieredCompilerStats::default(),
        }
    }

    /// Record a function call and determine if compilation is needed
    pub fn record_call(&mut self, func_id: u32) -> Option<CompileRequest> {
        let state = self.function_states
            .entry(func_id)
            .or_insert_with(|| FunctionCompileState::new(func_id));
        
        state.record_call();
        
        if state.should_upgrade(&self.policy) {
            state.compiling = true;
            let target_tier = match state.tier {
                CompileTier::Interpreter => CompileTier::Baseline,
                CompileTier::Baseline => CompileTier::Optimized,
                CompileTier::Optimized => return None,
            };
            
            return Some(CompileRequest {
                func_id,
                target_tier,
            });
        }
        
        None
    }

    /// Get current tier for function
    pub fn get_tier(&self, func_id: u32) -> CompileTier {
        self.function_states
            .get(&func_id)
            .map(|s| s.tier)
            .unwrap_or(CompileTier::Interpreter)
    }

    /// Register completed baseline compilation
    pub fn register_baseline(&mut self, func_id: u32, code: BaselineCode) {
        if let Some(state) = self.function_states.get_mut(&func_id) {
            state.compiling = false;
            state.upgrade();
            self.stats.baseline_compilations += 1;
        }
        self.baseline_code.insert(func_id, code);
    }

    /// Register completed optimized compilation
    pub fn register_optimized(&mut self, func_id: u32, code: OptimizedCode) {
        if let Some(state) = self.function_states.get_mut(&func_id) {
            state.compiling = false;
            state.upgrade();
            self.stats.optimizing_compilations += 1;
        }
        self.optimized_code.insert(func_id, code);
    }

    /// Handle deoptimization
    pub fn deoptimize(&mut self, func_id: u32, reason: DeoptReason) {
        if let Some(state) = self.function_states.get_mut(&func_id) {
            state.deoptimize();
            state.type_stable = false;
            self.stats.deoptimizations += 1;
        }
        
        // Remove optimized code
        self.optimized_code.remove(&func_id);
        
        tracing::debug!("Deoptimized function {} due to {:?}", func_id, reason);
    }

    /// Get baseline code for function
    pub fn get_baseline(&self, func_id: u32) -> Option<&BaselineCode> {
        self.baseline_code.get(&func_id)
    }

    /// Get optimized code for function
    pub fn get_optimized(&self, func_id: u32) -> Option<&OptimizedCode> {
        self.optimized_code.get(&func_id)
    }

    /// Get compilation statistics
    pub fn stats(&self) -> &TieredCompilerStats {
        &self.stats
    }

    /// Mark type as unstable for function
    pub fn mark_type_unstable(&mut self, func_id: u32) {
        if let Some(state) = self.function_states.get_mut(&func_id) {
            state.type_stable = false;
        }
    }
}

/// Compilation request
#[derive(Debug, Clone)]
pub struct CompileRequest {
    pub func_id: u32,
    pub target_tier: CompileTier,
}

/// Deoptimization reason
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeoptReason {
    /// Type check failed
    TypeMismatch,
    /// Guard check failed
    GuardFailed,
    /// Uncommon trap hit
    UncommonTrap,
    /// Stack overflow
    StackOverflow,
    /// Called with wrong arity
    ArityMismatch,
}

/// Baseline compiled code
#[derive(Debug, Clone)]
pub struct BaselineCode {
    /// Function ID
    pub func_id: u32,
    /// Native machine code
    pub code: Vec<u8>,
    /// Entry point offset
    pub entry_offset: usize,
    /// Stack frame size
    pub frame_size: u32,
    /// Compilation time in microseconds
    pub compile_time_us: u64,
}

impl BaselineCode {
    pub fn new(func_id: u32, code: Vec<u8>) -> Self {
        Self {
            func_id,
            code,
            entry_offset: 0,
            frame_size: 0,
            compile_time_us: 0,
        }
    }
}

/// Optimized compiled code
#[derive(Debug, Clone)]
pub struct OptimizedCode {
    /// Function ID
    pub func_id: u32,
    /// Native machine code
    pub code: Vec<u8>,
    /// Entry point offset
    pub entry_offset: usize,
    /// Stack frame size
    pub frame_size: u32,
    /// Type assumptions made during compilation
    pub type_assumptions: Vec<TypeAssumption>,
    /// Guards for deoptimization
    pub guards: Vec<Guard>,
    /// Compilation time in microseconds
    pub compile_time_us: u64,
    /// Whether function was inlined
    pub has_inlining: bool,
}

impl OptimizedCode {
    pub fn new(func_id: u32, code: Vec<u8>) -> Self {
        Self {
            func_id,
            code,
            entry_offset: 0,
            frame_size: 0,
            type_assumptions: Vec::new(),
            guards: Vec::new(),
            compile_time_us: 0,
            has_inlining: false,
        }
    }
}

/// Type assumption for optimization
#[derive(Debug, Clone)]
pub struct TypeAssumption {
    /// Bytecode offset where assumption was made
    pub bytecode_offset: u32,
    /// Expected type
    pub expected_type: ObservedType,
}

/// Guard for deoptimization
#[derive(Debug, Clone)]
pub struct Guard {
    /// Native code offset of guard check
    pub code_offset: usize,
    /// Bytecode offset to resume at if guard fails
    pub resume_offset: u32,
    /// Reason for this guard
    pub kind: GuardKind,
}

/// Kind of guard check
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardKind {
    /// Check type matches expected
    TypeCheck(ObservedType),
    /// Check shape matches expected
    ShapeCheck(u32), // shape ID
    /// Check array bounds
    BoundsCheck,
    /// Check not null/undefined
    NullCheck,
}

/// Tiered compiler statistics
#[derive(Debug, Clone, Default)]
pub struct TieredCompilerStats {
    /// Number of baseline compilations
    pub baseline_compilations: u64,
    /// Number of optimizing compilations
    pub optimizing_compilations: u64,
    /// Number of deoptimizations
    pub deoptimizations: u64,
    /// Total baseline compile time (microseconds)
    pub baseline_compile_time_us: u64,
    /// Total optimizing compile time (microseconds)
    pub opt_compile_time_us: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_function_state_upgrade() {
        let mut state = FunctionCompileState::new(0);
        let policy = CompilationPolicy::default();
        
        // Not ready to upgrade yet
        for _ in 0..99 {
            state.record_call();
        }
        assert!(!state.should_upgrade(&policy));
        
        // Now ready for baseline
        state.record_call();
        assert!(state.should_upgrade(&policy));
        
        state.upgrade();
        assert_eq!(state.tier, CompileTier::Baseline);
        assert_eq!(state.calls_at_tier, 0);
    }

    #[test]
    fn test_tiered_compiler_progression() {
        let mut compiler = TieredCompiler::new();
        let func_id = 42;
        
        // Warm up to baseline
        for _ in 0..100 {
            if let Some(req) = compiler.record_call(func_id) {
                assert_eq!(req.target_tier, CompileTier::Baseline);
                compiler.register_baseline(func_id, BaselineCode::new(func_id, vec![]));
            }
        }
        
        assert_eq!(compiler.get_tier(func_id), CompileTier::Baseline);
    }

    #[test]
    fn test_deoptimization() {
        let mut compiler = TieredCompiler::new();
        let func_id = 1;
        
        // Get to optimized
        compiler.function_states.insert(func_id, FunctionCompileState {
            func_id,
            tier: CompileTier::Optimized,
            call_count: 2000,
            calls_at_tier: 0,
            deopt_count: 0,
            compiling: false,
            type_stable: true,
        });
        compiler.optimized_code.insert(func_id, OptimizedCode::new(func_id, vec![]));
        
        // Deoptimize
        compiler.deoptimize(func_id, DeoptReason::TypeMismatch);
        
        assert_eq!(compiler.get_tier(func_id), CompileTier::Baseline);
        assert!(compiler.get_optimized(func_id).is_none());
    }

    #[test]
    fn test_max_deopt() {
        let mut state = FunctionCompileState::new(0);
        let policy = CompilationPolicy {
            max_deopt_count: 2,
            ..Default::default()
        };
        
        // Simulate getting to baseline then deopting multiple times
        state.tier = CompileTier::Baseline;
        state.calls_at_tier = 1001;
        state.deopt_count = 2;
        
        // Should not upgrade due to deopt limit
        assert!(!state.should_upgrade(&policy));
    }
}
