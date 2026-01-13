# Networking Roadmap: Chromium Parity & Beyond

> Goal: Match/surpass Chromium's network stack with zero external dependencies

## Current State ✅
- HTTP/1.1, HTTP/2, HTTP/3 (QUIC)
- Custom TLS, CORS, cookies, connection pooling
- Prefetch, priority, streaming, WebSocket, SSE

---

## Phase 1: Protocol Optimization (Q1)

### 1.1 HTTP/2 Enhancements
| Feature | Chromium | fOS Status | Action |
|---------|----------|------------|--------|
| HPACK dynamic table | Full | Partial | Implement full dynamic table with 4KB default |
| Server Push | Deprecated | Basic | Keep minimal, focus on Early Hints |
| Prioritization | Extensible | Basic | Implement RFC 9218 priority signals |
| Flow control | Adaptive | Fixed | Add adaptive window sizing |

**Optimization Target**: 15% faster than Chromium on high-latency connections

```rust
// Priority signal implementation
pub struct PrioritySignal {
    urgency: u8,        // 0-7, lower = more urgent
    incremental: bool,  // true for streaming resources
}
```

### 1.2 QUIC/HTTP/3 Hardening
| Feature | Priority | Dependency-Free Approach |
|---------|----------|-------------------------|
| 0-RTT resumption | High | Custom session ticket cache |
| Connection migration | High | Path validation in `migration.rs` |
| Multipath QUIC | Medium | Implement RFC 9000 extensions |
| QPACK compression | High | Optimize `qpack.rs` encoder |
| Congestion control | High | Custom BBRv2 implementation |

**Zero-Dependency Note**: Avoid `quinn`, implement QUIC primitives directly

---

## Phase 2: Connection Intelligence (Q2)

### 2.1 Predictive Connection Management
```rust
pub struct ConnectionPredictor {
    // Bloom filter for likely-needed hosts
    host_predictions: BloomFilter<64>,
    
    // Markov chain for navigation patterns
    nav_model: NavigationModel,
    
    // Preconnect based on link hover
    hover_preconnect: bool,
}
```

| Feature | Chromium | fOS Target |
|---------|----------|------------|
| DNS prefetch | Yes | ✅ + ML prediction |
| Preconnect | Yes | ✅ + hover-based |
| Speculative fetch | Yes | ✅ + viewport-aware |
| Connection coalescing | Yes | Enhance `coalescing.rs` |

### 2.2 Resource Prioritization
| Resource Type | Urgency | Incremental | Notes |
|---------------|---------|-------------|-------|
| HTML document | 0 | No | Highest priority |
| CSS (blocking) | 1 | No | Parser blocking |
| Fonts (visible) | 2 | No | Above-fold text |
| JS (async) | 3 | Yes | Non-blocking |
| Images (viewport) | 4 | Yes | Lazy decode |
| Prefetch | 7 | Yes | Lowest priority |

---

## Phase 3: Advanced Features (Q3)

### 3.1 Shared Brotli Dictionaries
Chromium recently added shared dictionary support. Implement natively:

```rust
pub struct SharedDictionary {
    url_pattern: Pattern,
    dictionary_hash: [u8; 32],
    data: Arc<[u8]>,  // Shared across connections
}
```

### 3.2 Compression Pipeline
| Format | Dependency | Action |
|--------|------------|--------|
| gzip | `miniz_oxide` | Keep or rewrite |
| Brotli | Custom | ✅ Already in `brotli_dict.rs` |
| Zstandard | Custom | Implement for HTTP |
| Shared dict | Custom | New feature |

### 3.3 DNS-over-HTTPS (DoH)
```rust
pub enum DnsResolver {
    System,
    DoH { endpoint: Url },
    DoT { server: IpAddr },
}
```

---

## Phase 4: Surpassing Chromium (Q4)

### 4.1 Unique Optimizations
| Feature | Description | Chromium? |
|---------|-------------|-----------|
| **Request fusion** | Merge small requests | No |
| **Delta encoding** | Diff-based responses | Partial |
| **P2P hints** | Peer resource hints | No |
| **Predictive caching** | ML-based prefetch | No |

### 4.2 Memory Optimization
```rust
// Connection pool with tiered memory
pub struct TieredConnectionPool {
    hot: LruCache<Key, Connection>,    // Active
    warm: Vec<(Key, Connection)>,       // Recent
    cold: BTreeMap<Key, ConnectionState>, // Serialized
}
```

### 4.3 Zero-Copy Networking
- mmap'd receive buffers
- Scatter-gather I/O for sends
- Direct socket-to-decoder pipeline

---

## Benchmarks Target

| Metric | Chromium | fOS Target |
|--------|----------|------------|
| TTFB (local) | 5ms | 3ms |
| TTFB (remote) | 80ms | 70ms |
| Parallel requests | 6/host | 6/host |
| Memory per connection | 64KB | 32KB |
| 0-RTT success rate | 85% | 90% |

---

## Dependencies Policy

### Keep (Rust-native)
- `socket2` - low-level sockets
- `rustls` - only if custom TLS too complex

### Remove/Replace
- Any C bindings for crypto
- External HTTP parsers
- Third-party QUIC implementations

### Custom Implementation Priority
1. TLS 1.3 handshake (critical path)
2. Certificate validation
3. HKDF/AEAD primitives
4. QUIC packet protection
