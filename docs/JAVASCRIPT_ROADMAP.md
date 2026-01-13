# JavaScript Engine Roadmap: Chromium Parity & Beyond

> Goal: Production-quality JS engine rivaling V8 with zero dependencies

## Current State ✅
- Parser, bytecode compiler, interpreter
- JIT compilation, escape analysis, const folding
- DOM bindings, event system, Web APIs
- Workers, IndexedDB, timers

---

## Phase 1: Parser & Compiler (Q1)

### 1.1 Parser Optimization
| Feature | V8 | fOS Status | Action |
|---------|-----|------------|--------|
| Lazy parsing | Full | ✅ `lazy_compile.rs` | Expand coverage |
| Preparser | Yes | ❌ | Implement fast path |
| Source streaming | Yes | Partial | Full streaming |
| Arrow detection | Heuristic | ❌ | Add heuristics |

```rust
// Preparser: fast scan without full AST
pub struct PreParser {
    pub fn scan_function(&self, src: &str) -> FunctionInfo {
        FunctionInfo {
            param_count: self.count_params(src),
            uses_arguments: self.has_arguments_keyword(src),
            uses_eval: self.has_eval(src),
            is_strict: self.detect_strict(src),
        }
    }
}
```

### 1.2 Bytecode Improvements
| Opcode Type | Current | Target |
|-------------|---------|--------|
| Total opcodes | 100 | 150 (specialized) |
| Inline caches | ❌ | ✅ |
| Type feedback | Partial | Full |

```rust
// Inline cache for property access
pub enum InlineCache {
    Uninitialized,
    Monomorphic { shape: ShapeId, offset: u32 },
    Polymorphic { entries: SmallVec<[CacheEntry; 4]> },
    Megamorphic,
}
```

---

## Phase 2: Interpreter Performance (Q2)

### 2.1 Dispatch Optimization
| Technique | V8 | fOS Status |
|-----------|-----|------------|
| Direct threading | Ignition | ❌ Implement |
| Superinstructions | Yes | ❌ Implement |
| Stack caching | Yes | Partial |

```rust
// Direct threaded dispatch (computed goto)
macro_rules! dispatch {
    ($ip:expr, $handlers:expr) => {
        unsafe {
            let handler = *$handlers.get_unchecked($ip.opcode as usize);
            goto handler;
        }
    };
}
```

### 2.2 Inline Caching
```rust
pub struct PropertyIC {
    // Fast path for known shapes
    cached_shape: ShapeId,
    cached_offset: u32,
    
    pub fn get(&self, obj: &JsObject, key: PropertyKey) -> Option<JsValue> {
        if obj.shape() == self.cached_shape {
            // Fast path: direct offset access
            Some(obj.get_slot(self.cached_offset))
        } else {
            // Slow path: update cache
            self.update_and_get(obj, key)
        }
    }
}
```

---

## Phase 3: JIT Compilation (Q3)

### 3.1 Tiered Compilation
| Tier | V8 Equivalent | fOS Target |
|------|---------------|------------|
| Interpreter | Ignition | ✅ Done |
| Baseline JIT | Sparkplug | Implement |
| Optimizing JIT | TurboFan | Implement |

```rust
pub struct TieredCompiler {
    hot_threshold: u32,       // Calls before baseline JIT
    opt_threshold: u32,       // Calls before optimizing JIT
    
    pub fn should_compile(&self, func: &Function) -> CompileTier {
        match func.call_count {
            0..100 => CompileTier::Interpret,
            100..1000 => CompileTier::Baseline,
            _ => CompileTier::Optimized,
        }
    }
}
```

### 3.2 Baseline JIT (Fast Compile)
```rust
// Template-based baseline JIT
pub fn compile_baseline(bytecode: &[u8]) -> NativeCode {
    let mut asm = Assembler::new();
    
    for op in bytecode {
        match op {
            Op::LoadLocal(idx) => {
                asm.mov(RAX, stack_slot(*idx));
            }
            Op::Add => {
                asm.pop(RBX);
                asm.add(RAX, RBX);
                // Type check and call runtime if not int
            }
            // ... template for each opcode
        }
    }
    
    asm.finalize()
}
```

### 3.3 Optimizing JIT
| Optimization | Priority | Notes |
|--------------|----------|-------|
| Type specialization | High | From type feedback |
| Inlining | High | Call site analysis |
| Escape analysis | ✅ | Already have |
| Loop invariant motion | High | Implement |
| Dead code elimination | High | Implement |
| Register allocation | High | Linear scan |

---

## Phase 4: Memory Management (Q4)

### 4.1 Garbage Collector
| Feature | V8 | fOS Target |
|---------|-----|------------|
| Generational | Yes | Implement |
| Incremental | Yes | Implement |
| Concurrent | Yes | Implement |
| Compacting | Yes | Implement |

```rust
pub struct GenerationalGC {
    nursery: Nursery,           // Young generation (fast alloc)
    old_gen: OldGeneration,     // Long-lived objects
    large_objects: LargeObjectSpace,
    
    // Remembered set for old→young pointers
    remembered_set: CardTable,
}

impl GenerationalGC {
    pub fn minor_gc(&mut self) {
        // Scavenge nursery (fast)
        // Promote survivors to old gen
    }
    
    pub fn major_gc(&mut self) {
        // Mark-sweep-compact old gen
        // Runs incrementally
    }
}
```

### 4.2 Object Layout
```rust
// V8-style hidden classes
pub struct Shape {
    parent: Option<ShapeId>,
    transitions: HashMap<PropertyKey, ShapeId>,
    property_table: Vec<PropertyDescriptor>,
}

pub struct JsObject {
    shape: ShapeId,
    elements: ElementsStorage,      // Array elements
    properties: InlineProperties,   // Named properties
    overflow: Option<Box<[JsValue]>>,
}
```

---

## Phase 5: Spec Compliance (Q4+)

### 5.1 ES2024 Features
| Feature | Status | Priority |
|---------|--------|----------|
| Resizable ArrayBuffer | ❌ | High |
| Array grouping | ❌ | High |
| Promise.withResolvers | ❌ | Medium |
| Atomics.waitAsync | ❌ | Medium |
| Decorators | ❌ | Low |
| Pattern matching | ❌ | Low |

### 5.2 WebAssembly
| Feature | Status | Notes |
|---------|--------|-------|
| MVP | ❌ | Implement |
| Reference types | ❌ | Implement |
| SIMD | ❌ | Implement |
| Threads | ❌ | Implement |
| Exception handling | ❌ | Implement |

---

## Phase 6: Surpassing V8

### 6.1 Unique Optimizations
| Feature | Description | V8? |
|---------|-------------|-----|
| **AOT hints** | Pre-compile common patterns | No |
| **Profile-guided** | Use runtime profiles | Partial |
| **Rust interop** | Zero-cost Rust calls | No |
| **Predictive compile** | JIT during parse | No |

### 6.2 DOM Integration
```rust
// Zero-copy DOM access from JS
pub fn bind_dom_property(obj: &JsObject, node: NodeId, prop: &str) {
    // Direct memory mapping to DOM
    // No marshalling overhead
}
```

---

## Benchmarks Target

| Benchmark | V8 | fOS Target |
|-----------|-----|------------|
| Octane | 30000 | 25000 |
| JetStream | 150 | 120 |
| Speedometer | 200 | 150 |
| Startup time | 5ms | 3ms |
| Memory overhead | 10MB | 5MB |

---

## Dependencies Policy

### Keep
- None required

### Custom Implementation Priority
1. Interpreter with inline caches
2. Baseline JIT (template-based)
3. Generational GC
4. Optimizing JIT (SSA-based)
5. WebAssembly runtime
