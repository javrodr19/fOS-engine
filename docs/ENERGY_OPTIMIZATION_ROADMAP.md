# Energy Optimization Roadmap: Surpassing Chromium

> Goal: 40% less power consumption than Chromium on mobile/laptop

## Current State
- No explicit energy management
- Single rendering mode

---

## Phase 1: Power-Aware Rendering (Q1)

### 1.1 Adaptive Frame Rate
```rust
pub struct AdaptiveRenderer {
    target_fps: u32,
    battery_status: BatteryStatus,
    
    pub fn get_target_fps(&self) -> u32 {
        match (self.battery_status, self.content_type) {
            (BatteryStatus::Low, _) => 30,
            (_, ContentType::Static) => 30,
            (_, ContentType::Video) => 60,
            (_, ContentType::Animation) => 60,
            _ => self.target_fps,
        }
    }
}

pub fn should_skip_frame(&self) -> bool {
    // Skip frames when content hasn't changed
    !self.has_pending_animations && 
    !self.has_pending_paints &&
    !self.has_user_input
}
```

### 1.2 GPU Power States
```rust
pub enum GpuPowerState {
    Active,          // Full performance
    LowPower,        // Reduced clocks
    Idle,            // Minimal power
    Off,             // Completely off
}

impl GpuManager {
    pub fn set_power_state(&mut self, state: GpuPowerState) {
        match state {
            GpuPowerState::Off => self.release_context(),
            GpuPowerState::Idle => self.flush_and_idle(),
            GpuPowerState::LowPower => self.set_low_clocks(),
            GpuPowerState::Active => self.set_full_clocks(),
        }
    }
}
```

---

## Phase 2: Background Tab Throttling (Q2)

### 2.1 Timer Throttling
| Tab State | Timer Resolution | RAF | Chromium | fOS |
|-----------|------------------|-----|----------|-----|
| Focused | 4ms | 60fps | Same | Same |
| Background | 1s | Paused | Same | Same |
| Hibernated | Suspended | Suspended | N/A | New |

```rust
pub struct TabThrottler {
    pub fn get_timer_throttle(&self, tab: &Tab) -> Duration {
        if tab.is_focused() {
            Duration::from_millis(4)
        } else if tab.is_audible() {
            Duration::from_millis(100)
        } else if tab.is_visible() {
            Duration::from_millis(100)
        } else {
            Duration::from_secs(1)
        }
    }
}
```

### 2.2 Network Throttling
```rust
pub struct BackgroundNetworkPolicy {
    max_concurrent_requests: usize,
    bandwidth_limit: Option<u64>,
    
    pub fn for_tab(tab: &Tab) -> Self {
        if tab.is_focused() {
            Self { max_concurrent_requests: 10, bandwidth_limit: None }
        } else {
            Self { max_concurrent_requests: 2, bandwidth_limit: Some(100_000) }
        }
    }
}
```

---

## Phase 3: CPU Frequency Hints (Q3)

### 3.1 Workload Classification
```rust
pub enum WorkloadType {
    Idle,           // No work: lowest frequency
    LightBrowsing,  // Reading: low frequency
    Interactive,    // Scrolling, clicks: medium
    HeavyProcessing,// Layout/paint: high
    MediaPlayback,  // Video: fixed
}

impl WorkloadClassifier {
    pub fn classify(&self) -> WorkloadType {
        let input_rate = self.input_events_per_second();
        let paint_rate = self.paints_per_second();
        let js_time = self.js_time_percentage();
        
        if input_rate > 10.0 { WorkloadType::Interactive }
        else if paint_rate > 30.0 { WorkloadType::HeavyProcessing }
        else if js_time > 0.5 { WorkloadType::HeavyProcessing }
        else if paint_rate > 0.0 { WorkloadType::LightBrowsing }
        else { WorkloadType::Idle }
    }
}
```

### 3.2 Energy-Aware Scheduling
```rust
// Schedule non-urgent work to efficient cores
pub fn schedule_task(&self, task: Task, urgency: Urgency) {
    match urgency {
        Urgency::Immediate => self.performance_cores.push(task),
        Urgency::Soon => self.any_core.push(task),
        Urgency::Eventually => self.efficiency_cores.push(task),
    }
}
```

---

## Phase 4: Wake Lock Management (Q4)

### 4.1 Minimal Wake Locks
```rust
pub struct WakeLockManager {
    active_locks: Vec<WakeLock>,
    
    pub fn request(&mut self, reason: WakeLockReason) -> WakeLockGuard {
        // Coalesce similar wake locks
        // Set minimum duration
        // Auto-release after timeout
    }
}

pub enum WakeLockReason {
    UserInput,      // 100ms after last input
    Animation,      // Duration of animation
    MediaPlayback,  // While media plays
    Download,       // While downloading
}
```

### 4.2 Aggressive Idle Detection
```rust
pub fn detect_idle(&self) -> bool {
    let no_input = self.last_input.elapsed() > Duration::from_secs(30);
    let no_animation = !self.has_active_animations();
    let no_media = !self.is_playing_media();
    let no_visible_change = self.last_paint.elapsed() > Duration::from_secs(1);
    
    no_input && no_animation && no_media && no_visible_change
}
```

---

## Phase 5: Media Power Optimization

### 5.1 Hardware Decode Priority
```rust
pub fn select_decoder(&self, codec: Codec) -> DecoderType {
    // Always prefer hardware decode (10x less power)
    if self.hw_decoder_available(codec) {
        DecoderType::Hardware
    } else {
        DecoderType::Software
    }
}
```

### 5.2 Adaptive Video Quality
```rust
pub struct AdaptivePlayer {
    pub fn select_quality(&self) -> QualityLevel {
        if self.on_battery() && self.battery_level() < 0.2 {
            // Reduce quality to save power
            QualityLevel::Low
        } else if self.on_battery() {
            QualityLevel::Medium
        } else {
            QualityLevel::High
        }
    }
}
```

---

## Chromium Comparison

| Feature | Chromium | fOS Target |
|---------|----------|------------|
| Background throttling | 1s timers | + hibernation |
| Frame skip | Limited | Full |
| GPU power states | OS-managed | Explicit |
| Efficient core usage | No | Yes (ARM big.LITTLE) |
| Adaptive quality | No | Yes |

---

## Power Consumption Targets

| Scenario | Chromium | fOS Target | Savings |
|----------|----------|------------|---------|
| Idle (background tabs) | 5W | 1W | 80% |
| Reading article | 8W | 5W | 38% |
| Video playback | 12W | 8W | 33% |
| Heavy JS | 25W | 20W | 20% |

---

## Implementation Priority

1. **Background throttling** - Biggest impact
2. **Frame skipping** - Common case
3. **GPU idle** - High power draw
4. **HW decode** - Video is common
5. **Tab hibernation** - Many tabs open
