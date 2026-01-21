//! Unified Engine Integration
//!
//! This module integrates all the new roadmap features into a cohesive engine:
//! - Tiered compilation (interpreter → baseline → optimizing)
//! - Generational GC
//! - Polymorphic inline caches
//! - ES2024 features
//! - WebAssembly runtime
//! - Advanced optimizations

use std::collections::HashMap;

// Import new modules
use super::preparser::{PreParser, FunctionInfo};
use super::inline_cache::{InlineCacheManager, ShapeId};
use super::direct_dispatch::{DirectDispatch, SuperInstructionTransformer};
use super::tiered_compiler::{TieredCompiler, CompileTier, CompilationPolicy};
use super::ssa::{SsaFunction, SsaBuilder};
use super::generational_gc::{GenerationalGC, GcConfig};
use super::es2024::{ResizableArrayBuffer, AtomicsManager, DeferredPromise};
use super::wasm_runtime::{Module as WasmModule, Instance as WasmInstance};
use super::advanced_optimizations::{
    ProfileGuidedOptimizer, PredictiveJit, RustInterop, AotAnalyzer, ZeroCopyDom
};
use super::decorators::DecoratorRuntime;
use super::pattern_matching::PatternCompiler;
use super::wasm_extensions::{SharedMemory, ThreadManager, ExceptionRuntime, V128};

// =============================================================================
// Integrated Engine
// =============================================================================

/// Integrated JavaScript engine with all roadmap features
pub struct IntegratedEngine {
    // Parsing
    preparser: PreParser,
    
    // Compilation
    tiered_compiler: TieredCompiler,
    ssa_cache: HashMap<u32, SsaFunction>,
    compilation_policy: CompilationPolicy,
    
    // Runtime
    inline_caches: InlineCacheManager,
    direct_dispatch: DirectDispatch,
    super_transformer: SuperInstructionTransformer,
    
    // Memory
    gc: GenerationalGC,
    
    // ES2024
    resizable_buffers: Vec<ResizableArrayBuffer>,
    atomics_manager: AtomicsManager,
    deferred_promises: HashMap<u32, DeferredPromise>,
    
    // WebAssembly
    wasm_modules: HashMap<u32, WasmModule>,
    wasm_instances: HashMap<u32, WasmInstance>,
    wasm_shared_memory: Option<SharedMemory>,
    wasm_threads: ThreadManager,
    wasm_exceptions: ExceptionRuntime,
    
    // Optimizations
    pgo: ProfileGuidedOptimizer,
    predictive_jit: PredictiveJit,
    rust_interop: RustInterop,
    aot_analyzer: AotAnalyzer,
    zero_copy_dom: ZeroCopyDom,
    
    // Decorators & Pattern Matching
    decorator_runtime: DecoratorRuntime,
    pattern_compiler: PatternCompiler,
    
    // Counters
    next_buffer_id: u32,
    next_promise_id: u32,
    next_wasm_id: u32,
}

impl Default for IntegratedEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl IntegratedEngine {
    pub fn new() -> Self {
        Self {
            preparser: PreParser::new(),
            tiered_compiler: TieredCompiler::new(),
            ssa_cache: HashMap::new(),
            compilation_policy: CompilationPolicy::default(),
            inline_caches: InlineCacheManager::new(),
            direct_dispatch: DirectDispatch::new(),
            super_transformer: SuperInstructionTransformer::new(),
            gc: GenerationalGC::new(GcConfig::default()),
            resizable_buffers: Vec::new(),
            atomics_manager: AtomicsManager::new(),
            deferred_promises: HashMap::new(),
            wasm_modules: HashMap::new(),
            wasm_instances: HashMap::new(),
            wasm_shared_memory: None,
            wasm_threads: ThreadManager::new(),
            wasm_exceptions: ExceptionRuntime::new(),
            pgo: ProfileGuidedOptimizer::new(),
            predictive_jit: PredictiveJit::new(),
            rust_interop: RustInterop::new(),
            aot_analyzer: AotAnalyzer::new(),
            zero_copy_dom: ZeroCopyDom::new(),
            decorator_runtime: DecoratorRuntime::new(),
            pattern_compiler: PatternCompiler::new(),
            next_buffer_id: 0,
            next_promise_id: 0,
            next_wasm_id: 0,
        }
    }

    // =========================================================================
    // Parsing
    // =========================================================================

    /// Pre-parse function for quick analysis
    pub fn preparse_function(&mut self, source: &str) -> FunctionInfo {
        self.preparser.scan_function(source)
    }

    // =========================================================================
    // Compilation
    // =========================================================================

    /// Get compilation tier for function
    pub fn get_compile_tier(&self, func_id: u32) -> CompileTier {
        self.tiered_compiler.get_tier(func_id)
    }

    /// Record function call for tiered compilation
    pub fn record_function_call(&mut self, func_id: u32) {
        self.tiered_compiler.record_call(func_id);
        self.pgo.record_call(func_id);
        self.predictive_jit.enter_function(func_id);
    }

    /// Record function return
    pub fn record_function_return(&mut self, _func_id: u32) {
        self.predictive_jit.exit_function();
    }

    /// Check if function should be optimized
    pub fn should_optimize(&self, func_id: u32) -> bool {
        self.tiered_compiler.should_upgrade(func_id) || self.pgo.is_hot(func_id)
    }

    /// Build SSA for function
    pub fn build_ssa(&mut self, func_id: u32) -> &SsaFunction {
        if !self.ssa_cache.contains_key(&func_id) {
            let builder = SsaBuilder::new();
            let ssa_func = builder.build();
            self.ssa_cache.insert(func_id, ssa_func);
        }
        self.ssa_cache.get(&func_id).unwrap()
    }

    // =========================================================================
    // Inline Caching
    // =========================================================================

    /// Get or create inline cache for property access
    pub fn get_inline_cache(&mut self, cache_id: u32, shape: ShapeId) -> Option<u32> {
        self.inline_caches.lookup(cache_id, shape)
    }

    /// Update inline cache
    pub fn update_inline_cache(&mut self, cache_id: u32, shape: ShapeId, offset: u32) {
        self.inline_caches.update(cache_id, shape, offset);
    }

    // =========================================================================
    // Garbage Collection
    // =========================================================================

    /// Trigger minor GC (nursery only)
    pub fn minor_gc(&mut self) {
        self.gc.minor_gc();
    }

    /// Trigger major GC (full collection)
    pub fn major_gc(&mut self) {
        self.gc.major_gc();
    }

    /// Check if GC should run
    pub fn should_gc(&self) -> bool {
        self.gc.should_collect()
    }

    // =========================================================================
    // ES2024 Features
    // =========================================================================

    /// Create resizable ArrayBuffer
    pub fn create_resizable_buffer(&mut self, size: usize, max_size: usize) -> Result<u32, String> {
        let buffer = ResizableArrayBuffer::new_resizable(size, max_size)
            .map_err(|e| format!("{:?}", e))?;
        
        let id = self.next_buffer_id;
        self.next_buffer_id += 1;
        self.resizable_buffers.push(buffer);
        Ok(id)
    }

    /// Resize buffer
    pub fn resize_buffer(&mut self, id: u32, new_size: usize) -> Result<(), String> {
        self.resizable_buffers.get_mut(id as usize)
            .ok_or("Buffer not found")?
            .resize(new_size)
            .map_err(|e| format!("{:?}", e))
    }

    /// Create deferred promise (Promise.withResolvers)
    pub fn create_deferred_promise(&mut self) -> (u32, u32, u32) {
        let id = self.next_promise_id;
        self.next_promise_id += 1;
        
        let promise = DeferredPromise::new(id);
        self.deferred_promises.insert(id, promise);
        
        // Return (promise_id, resolve_fn_id, reject_fn_id)
        (id, id * 2, id * 2 + 1)
    }

    // =========================================================================
    // WebAssembly
    // =========================================================================

    /// Load WASM module
    pub fn load_wasm_module(&mut self, module: WasmModule) -> u32 {
        let id = self.next_wasm_id;
        self.next_wasm_id += 1;
        self.wasm_modules.insert(id, module);
        id
    }

    /// Instantiate WASM module
    pub fn instantiate_wasm(&mut self, module_id: u32) -> Result<u32, String> {
        let module = self.wasm_modules.get(&module_id)
            .ok_or("Module not found")?;
        
        // Clone module for instantiation
        let module_clone = WasmModule::new(); // Simplified
        
        let instance = WasmInstance::new(module_clone)
            .map_err(|e| format!("{:?}", e))?;
        
        let instance_id = self.next_wasm_id;
        self.next_wasm_id += 1;
        self.wasm_instances.insert(instance_id, instance);
        
        Ok(instance_id)
    }

    /// Create shared memory for WASM threads
    pub fn create_shared_memory(&mut self, initial_pages: u32, max_pages: Option<u32>) {
        self.wasm_shared_memory = Some(SharedMemory::new(initial_pages, max_pages));
    }

    /// Spawn WASM thread
    pub fn spawn_wasm_thread(&mut self, entry_func: u32) -> u32 {
        self.wasm_threads.spawn(entry_func)
    }

    /// Register WASM exception tag
    pub fn register_exception_tag(&mut self, name: Option<String>) -> u32 {
        self.wasm_exceptions.register_tag(name, vec![])
    }

    // =========================================================================
    // Optimizations
    // =========================================================================

    /// Record type feedback
    pub fn record_type(&mut self, func_id: u32, op_id: u32, type_hint: super::advanced_optimizations::TypeHint) {
        self.pgo.record_type(func_id, op_id, type_hint);
    }

    /// Get optimization recommendations
    pub fn get_optimization_hints(&self, func_id: u32) -> Vec<super::advanced_optimizations::OptimizationHint> {
        self.pgo.get_recommendations(func_id)
    }

    /// Predict next function call
    pub fn predict_next_call(&self) -> Option<u32> {
        self.predictive_jit.predict_next()
    }

    // =========================================================================
    // Decorators
    // =========================================================================

    /// Create decorator context
    pub fn create_decorator_context(
        &mut self,
        kind: super::decorators::DecoratorKind,
        name: &str,
        is_static: bool,
        is_private: bool,
    ) -> super::decorators::DecoratorContext {
        self.decorator_runtime.create_context(
            kind,
            super::decorators::DecoratorName::String(name.to_string()),
            is_static,
            is_private,
        )
    }

    // =========================================================================
    // Pattern Matching
    // =========================================================================

    /// Compile pattern
    pub fn compile_pattern(&mut self, pattern: &super::pattern_matching::Pattern) -> super::pattern_matching::CompiledPattern {
        self.pattern_compiler.compile(pattern)
    }

    // =========================================================================
    // Statistics
    // =========================================================================

    /// Get engine statistics
    pub fn get_stats(&self) -> EngineStats {
        EngineStats {
            functions_compiled: self.ssa_cache.len(),
            inline_cache_stats: self.inline_caches.stats(),
            gc_stats: self.gc.stats(),
            wasm_modules_loaded: self.wasm_modules.len(),
            wasm_threads_active: self.wasm_threads.active_count(),
            pgo_hot_functions: self.pgo_hot_count(),
            prediction_accuracy: self.predictive_jit.accuracy(),
        }
    }

    fn pgo_hot_count(&self) -> usize {
        // Count hot functions
        (0..1000).filter(|&id| self.pgo.is_hot(id)).count()
    }
}

/// Engine statistics
#[derive(Debug, Clone)]
pub struct EngineStats {
    pub functions_compiled: usize,
    pub inline_cache_stats: super::inline_cache::InlineCacheStats,
    pub gc_stats: super::generational_gc::GcStats,
    pub wasm_modules_loaded: usize,
    pub wasm_threads_active: usize,
    pub pgo_hot_functions: usize,
    pub prediction_accuracy: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_creation() {
        let engine = IntegratedEngine::new();
        assert_eq!(engine.next_buffer_id, 0);
    }

    #[test]
    fn test_function_call_recording() {
        let mut engine = IntegratedEngine::new();
        
        for _ in 0..100 {
            engine.record_function_call(0);
        }
        
        // After 100 calls, function should be baseline tier
        let tier = engine.get_compile_tier(0);
        assert!(matches!(tier, CompileTier::Baseline | CompileTier::Optimized));
    }

    #[test]
    fn test_resizable_buffer() {
        let mut engine = IntegratedEngine::new();
        
        let id = engine.create_resizable_buffer(100, 1000).unwrap();
        assert_eq!(id, 0);
        
        engine.resize_buffer(id, 500).unwrap();
    }

    #[test]
    fn test_deferred_promise() {
        let mut engine = IntegratedEngine::new();
        
        let (promise_id, resolve_id, reject_id) = engine.create_deferred_promise();
        assert_eq!(promise_id, 0);
        assert_eq!(resolve_id, 0);
        assert_eq!(reject_id, 1);
    }
}
