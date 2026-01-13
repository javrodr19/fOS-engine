# DevTools Roadmap: Chromium Parity & Beyond

> Goal: Chrome DevTools equivalent with native Rust implementation

## Current State ✅
- Console, Elements, Network, Performance panels
- Memory, Sources, Debugger, Application, Lighthouse

---

## Phase 1: Core Panels (Q1)

### 1.1 Elements Panel
| Feature | Chrome | fOS Status | Action |
|---------|--------|------------|--------|
| DOM tree view | Full | Partial | Complete |
| Styles sidebar | Full | Partial | Complete |
| Computed styles | Full | ❌ | Implement |
| Event listeners | Full | ❌ | Implement |
| Box model | Full | ❌ | Implement |

### 1.2 Console
| Feature | Status |
|---------|--------|
| console.log/warn/error | ✅ |
| Object inspection | Partial |
| console.table | ❌ Implement |
| console.trace | ❌ Implement |
| $0, $_ references | ❌ Implement |

---

## Phase 2: Debugging (Q2)

### 2.1 JavaScript Debugger
```rust
pub struct JsDebugger {
    breakpoints: HashMap<Location, Breakpoint>,
    call_stack: Vec<StackFrame>,
    
    pub fn pause(&mut self, reason: PauseReason) {
        // Notify DevTools frontend
        self.send_event(Event::Paused { reason, call_stack: self.call_stack.clone() });
    }
    
    pub fn evaluate(&self, expr: &str, frame: usize) -> JsValue {
        // Evaluate in specific stack frame context
    }
}
```

### 2.2 Source Maps
```rust
pub struct SourceMap {
    sources: Vec<String>,
    mappings: Vec<Mapping>,
    
    pub fn original_location(&self, gen_line: u32, gen_col: u32) -> OriginalLocation {
        // Map generated position to original source
    }
}
```

---

## Phase 3: Performance Tools (Q3)

### 3.1 Performance Timeline
| Feature | Status |
|---------|--------|
| Frame timing | ✅ |
| Layout events | Partial |
| Paint events | ❌ Implement |
| Script execution | ❌ Implement |
| Flame chart | ❌ Implement |

### 3.2 Memory Profiling
```rust
pub struct HeapProfiler {
    pub fn take_snapshot(&self) -> HeapSnapshot {
        // Capture full heap state
    }
    
    pub fn start_allocation_tracking(&mut self) {
        // Track allocations for timeline
    }
}
```

---

## Phase 4: Network Tools (Q4)

### 4.1 Request Details
| Feature | Status |
|---------|--------|
| Request list | ✅ |
| Headers | Partial |
| Preview | ❌ Implement |
| Response | ❌ Implement |
| Timing | ❌ Implement |
| Cookies | ❌ Implement |

### 4.2 Throttling
```rust
pub struct NetworkThrottle {
    download_bps: u64,
    upload_bps: u64,
    latency_ms: u32,
    
    // Presets
    pub const SLOW_3G: Self = Self { download_bps: 50_000, upload_bps: 25_000, latency_ms: 400 };
    pub const FAST_3G: Self = Self { download_bps: 150_000, upload_bps: 75_000, latency_ms: 150 };
}
```

---

## Phase 5: Protocol & Frontend

### 5.1 Chrome DevTools Protocol (CDP)
```rust
// Implement CDP for compatibility with Chrome DevTools frontend
pub struct CdpServer {
    websocket: WebSocket,
    
    pub fn handle_command(&mut self, cmd: CdpCommand) -> CdpResponse {
        match cmd.method.as_str() {
            "Runtime.evaluate" => self.runtime_evaluate(cmd.params),
            "Debugger.setBreakpoint" => self.debugger_set_breakpoint(cmd.params),
            "Network.enable" => self.network_enable(),
            _ => CdpResponse::error("Unknown method"),
        }
    }
}
```

### 5.2 Native Frontend (Optional)
| Approach | Pros | Cons |
|----------|------|------|
| Chrome frontend | Full featured | External dependency |
| Custom Rust UI | Integrated | Effort |
| Web-based | Portable | Performance |

---

## Benchmarks Target

| Metric | Chrome | fOS Target |
|--------|--------|------------|
| Startup | 100ms | 50ms |
| DOM refresh | 50ms | 20ms |
| Profiler overhead | 10% | 5% |

---

## Dependencies Policy

### Custom Implementation
1. CDP server
2. JS debugger integration
3. Profiler instrumentation
4. Source map parser
