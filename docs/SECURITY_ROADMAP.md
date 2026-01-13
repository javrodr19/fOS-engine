# Security Roadmap: Chromium Parity & Beyond

> Goal: Robust security model with minimal attack surface

## Current State ✅
- CSP, CORS, COOP/COEP
- SRI, Trusted Types, XSS protection
- Permissions Policy, sandbox, credential API

---

## Phase 1: Process Isolation (Q1)

### 1.1 Multi-Process Architecture
| Component | Chromium | fOS Current | Target |
|-----------|----------|-------------|--------|
| Browser process | Separate | Single | Implement |
| Renderer process | Per-site | Single | Per-tab |
| Network process | Separate | Single | Separate |
| GPU process | Separate | Single | Separate |

```rust
pub enum ProcessModel {
    Single,                    // Current
    PerTab,                    // Isolation by tab
    PerOrigin,                 // Site isolation
    PerDocument,               // Most secure
}

pub struct ProcessManager {
    model: ProcessModel,
    processes: HashMap<ProcessId, ProcessHandle>,
    ipc: IpcChannel,
}
```

### 1.2 Sandboxing
| Platform | Chromium | fOS Target |
|----------|----------|------------|
| Linux | seccomp-bpf + namespaces | Same |
| macOS | Seatbelt | Seatbelt profiles |
| Windows | AppContainer | AppContainer |

```rust
// Linux sandbox using seccomp
pub fn apply_renderer_sandbox() {
    // Create unprivileged user namespace
    unshare(CLONE_NEWUSER | CLONE_NEWNET | CLONE_NEWPID);
    
    // Apply seccomp filter
    let filter = SeccompFilter::new()
        .allow(SYS_read, SYS_write, SYS_mmap, SYS_munmap)
        .deny_all();
    filter.apply();
}
```

---

## Phase 2: Site Isolation (Q2)

### 2.1 Origin-Based Isolation
| Feature | Chromium | fOS Target |
|---------|----------|------------|
| Cross-origin iframes | Separate process | Separate process |
| Cross-origin popups | Separate process | Separate process |
| Spectre mitigations | Yes | Implement |

```rust
pub struct SiteInstance {
    origin: Origin,
    process_id: ProcessId,
    
    pub fn should_use_different_process(&self, url: &Url) -> bool {
        // Different origin = different process
        !self.origin.is_same_origin(url)
    }
}
```

### 2.2 Cross-Origin Read Blocking (CORB)
```rust
pub fn should_block_response(
    request: &Request,
    response: &Response,
) -> bool {
    // Block cross-origin responses that look like they contain
    // sensitive data (JSON, HTML, XML)
    if request.is_cross_origin() && !response.cors_allowed() {
        matches!(
            response.sniff_content_type(),
            ContentType::Html | ContentType::Json | ContentType::Xml
        )
    } else {
        false
    }
}
```

---

## Phase 3: Memory Safety (Q3)

### 3.1 Rust Guarantees
| Threat | C++ (Chromium) | Rust (fOS) |
|--------|----------------|------------|
| Use-after-free | Common | Compile-time prevented |
| Buffer overflow | Common | Compile-time prevented |
| Double free | Possible | Compile-time prevented |
| Data races | Possible | Compile-time prevented |

### 3.2 Additional Hardening
```rust
// Memory-safe IPC
pub struct IpcMessage {
    // Validated, bounds-checked access
    data: Box<[u8]>,
}

impl IpcMessage {
    pub fn read<T: Deserialize>(&self) -> Result<T, IpcError> {
        // Fuzz-tested deserialization
        serde_cbor::from_slice(&self.data)
    }
}
```

---

## Phase 4: Web Security Features (Q4)

### 4.1 CSP Enhancements
| Directive | Status | Notes |
|-----------|--------|-------|
| script-src | ✅ | Full |
| style-src | ✅ | Full |
| trusted-types | ✅ | Full |
| require-trusted-types-for | ✅ | Full |
| wasm-unsafe-eval | ❌ | Implement |

### 4.2 Permissions
```rust
pub enum Permission {
    Geolocation,
    Camera,
    Microphone,
    Notifications,
    ClipboardRead,
    ClipboardWrite,
    // New permissions
    ScreenShare,
    MidiSysex,
    StorageAccess,
}

pub struct PermissionManager {
    grants: HashMap<(Origin, Permission), PermissionState>,
    
    pub fn check(&self, origin: &Origin, perm: Permission) -> PermissionState {
        // Check granted permissions with expiry
    }
}
```

---

## Phase 5: Surpassing Chromium

### 5.1 Rust-Unique Security
| Feature | Description | Chromium? |
|---------|-------------|-----------|
| **No UAF** | Ownership system | Mitigation only |
| **No data races** | Send/Sync | Mitigation only |
| **Safe IPC** | Type-safe messages | Runtime checks |
| **Minimal C** | Pure Rust | 70%+ C++ |

### 5.2 Proactive Security
```rust
// Capability-based security model
pub struct Capability {
    resource: ResourceType,
    permissions: PermissionBits,
    expiry: Option<Instant>,
}

pub struct CapabilitySet {
    capabilities: Vec<Capability>,
    
    pub fn can(&self, action: Action) -> bool {
        self.capabilities.iter()
            .any(|c| c.allows(action) && !c.expired())
    }
}
```

---

## Benchmarks Target

| Metric | Chromium | fOS Target |
|--------|----------|------------|
| CVEs per year | 100+ | <10 (Rust safety) |
| Sandbox escapes | Occasional | Zero |
| Memory corruption | ~70% of CVEs | Zero (Rust) |
| Startup with sandbox | 50ms | 30ms |

---

## Dependencies Policy

### Keep (for sandbox)
- Linux: kernel features only
- macOS: system frameworks
- Windows: system APIs

### Custom Implementation
1. Seccomp filter generator
2. IPC serialization
3. Capability system
4. Permission manager
