# Media Roadmap: Chromium Parity & Beyond

> Goal: Full media support with zero native codec dependencies

## Current State ✅
- Audio playback, codecs, EME (DRM)
- MSE, WebRTC, media elements
- Buffer pooling, fullscreen

---

## Phase 1: Codec Implementation (Q1)

### 1.1 Pure-Rust Decoders
| Codec | Chromium | fOS Status | Action |
|-------|----------|------------|--------|
| H.264 | FFmpeg/HW | ❌ | Implement |
| H.265 | FFmpeg/HW | ❌ | Implement |
| VP8 | libvpx | ❌ | Implement |
| VP9 | libvpx | ❌ | Implement |
| AV1 | dav1d | ❌ | Implement |
| AAC | FFmpeg | ❌ | Implement |
| Opus | libopus | ✅ | Done |
| Vorbis | libvorbis | ❌ | Implement |

```rust
pub trait VideoDecoder {
    fn decode(&mut self, packet: &EncodedPacket) -> Result<VideoFrame>;
    fn flush(&mut self) -> Vec<VideoFrame>;
    fn capabilities() -> DecoderCaps;
}

pub struct H264Decoder {
    sps: Option<Sps>,
    pps: HashMap<u8, Pps>,
    dpb: DecodedPictureBuffer,
}
```

### 1.2 Hardware Acceleration
| Platform | API | Status |
|----------|-----|--------|
| Linux | VA-API | ❌ Implement |
| macOS | VideoToolbox | ❌ Implement |
| Windows | DXVA2/D3D11VA | ❌ Implement |

```rust
pub enum DecoderBackend {
    Software(Box<dyn VideoDecoder>),
    Hardware(HwAcceleratedDecoder),
}

impl DecoderBackend {
    pub fn new(codec: Codec) -> Self {
        // Try hardware first, fall back to software
        if let Some(hw) = try_hw_decoder(codec) {
            Self::Hardware(hw)
        } else {
            Self::Software(software_decoder(codec))
        }
    }
}
```

---

## Phase 2: Media Pipeline (Q2)

### 2.1 Pipeline Architecture
```rust
pub struct MediaPipeline {
    demuxer: Box<dyn Demuxer>,
    video_decoder: DecoderBackend,
    audio_decoder: DecoderBackend,
    video_renderer: VideoRenderer,
    audio_renderer: AudioRenderer,
    clock: MediaClock,
}
```

### 2.2 Container Support
| Format | Status | Notes |
|--------|--------|-------|
| MP4 | ❌ | Implement parser |
| WebM | ❌ | Implement parser |
| MKV | ❌ | Implement parser |
| MPEG-TS | ❌ | For HLS |
| fMP4 | ❌ | For DASH |

---

## Phase 3: Streaming (Q3)

### 3.1 Adaptive Streaming
| Protocol | Status | Notes |
|----------|--------|-------|
| HLS | ❌ | Implement |
| DASH | ❌ | Implement |
| MSE | ✅ | Enhance |

```rust
pub struct AdaptivePlayer {
    manifest: Manifest,
    quality_selector: QualitySelector,
    buffer_manager: BufferManager,
    
    pub fn select_quality(&mut self, bandwidth: u64) -> QualityLevel {
        // ABR algorithm (BOLA, buffer-based, etc.)
    }
}
```

### 3.2 DRM Support
| DRM | Chromium | fOS Target |
|-----|----------|------------|
| Widevine | Yes | Research feasibility |
| FairPlay | No | macOS only |
| Clear Key | Yes | ✅ Implement |

---

## Phase 4: WebRTC (Q4)

### 4.1 Core Features
| Feature | Status | Notes |
|---------|--------|-------|
| PeerConnection | ✅ | Enhance |
| DataChannel | ✅ | Done |
| ICE/STUN/TURN | Partial | Complete |
| DTLS-SRTP | ❌ | Implement |
| Simulcast | ❌ | Implement |

### 4.2 Custom Implementation
```rust
pub struct PeerConnection {
    local_description: SessionDescription,
    remote_description: Option<SessionDescription>,
    ice_agent: IceAgent,
    dtls: DtlsTransport,
    srtp: SrtpSession,
}

pub struct IceAgent {
    candidates: Vec<IceCandidate>,
    stun_servers: Vec<Url>,
    turn_servers: Vec<TurnServer>,
}
```

---

## Phase 5: Web Audio (Q4+)

### 5.1 Audio Graph
```rust
pub struct AudioContext {
    sample_rate: u32,
    destination: AudioDestination,
    nodes: Arena<AudioNode>,
    graph: AudioGraph,
}

pub enum AudioNode {
    Oscillator(OscillatorNode),
    Gain(GainNode),
    BiquadFilter(BiquadFilterNode),
    Analyser(AnalyserNode),
    // ... all Web Audio nodes
}
```

### 5.2 Audio Worklet
```rust
pub struct AudioWorklet {
    processor: JsFunction,
    inputs: Vec<AudioBuffer>,
    outputs: Vec<AudioBuffer>,
    
    pub fn process(&mut self, js_runtime: &JsRuntime) {
        // Call JS processor on audio thread
    }
}
```

---

## Phase 6: Surpassing Chromium

### 6.1 Unique Optimizations
| Feature | Description | Chromium? |
|---------|-------------|-----------|
| **Pure-Rust codecs** | No C/C++ deps | FFmpeg |
| **SIMD everywhere** | AVX-512/NEON | Partial |
| **Memory pools** | Zero-alloc decode | Partial |
| **GPU decode path** | Direct to texture | Yes |

### 6.2 Low-Latency Pipeline
```rust
pub struct LowLatencyPipeline {
    // Minimize buffering for live streams
    buffer_target: Duration::from_millis(100),
    
    // Skip frames if behind
    frame_drop_strategy: FrameDropStrategy::NonReference,
}
```

---

## Benchmarks Target

| Metric | Chromium | fOS Target |
|--------|----------|------------|
| 4K decode (SW) | 30fps | 30fps |
| 4K decode (HW) | 60fps | 60fps |
| Audio latency | 50ms | 20ms |
| Startup time | 200ms | 100ms |
| Memory (1080p) | 50MB | 30MB |

---

## Dependencies Policy

### Keep
- Platform HW APIs (VA-API, VT, DXVA)
- No runtime libraries

### Custom Implementation Priority
1. VP9 decoder (most common)
2. AV1 decoder (future-proof)
3. AAC decoder
4. H.264 decoder
5. Container parsers
