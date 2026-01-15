//! fOS Engine
//!
//! A lightweight browser engine written in Rust.
//!
//! # Goals
//! - Minimal RAM usage (~20-30MB per tab)
//! - Fast startup
//! - Modern web standards support
//!
//! # Features
//! 
//! The engine supports conditional compilation via feature flags:
//! - `full` (default): All features enabled
//! - `minimal`: Core engine only (parsing, layout, rendering)
//! - `webgl`: Canvas and WebGL support
//! - `media`: Audio/video media support
//! - `devtools`: Developer tools
//! - `accessibility`: Accessibility support
//!
//! # Example
//! ```rust,ignore
//! use fos_engine::{Engine, Config};
//!
//! let engine = Engine::new(Config::default());
//! let page = engine.load_url("https://example.com").await?;
//! let pixels = page.render(800, 600);
//! ```

mod engine;
mod page;
mod config;
pub mod memory;
pub mod arena;
pub mod intern;
pub mod cow;
pub mod compress;
pub mod cold;
pub mod url;
pub mod plugin;

// Phase 23: Low-Level Optimizations
pub mod packed;
pub mod alloc;
pub mod simd;
pub mod fixed_point;
pub mod visibility;
pub mod tiered_memory;

// Phase 7: Compatibility Testing
pub mod compat;

// Phase 21: Advanced Memory Optimization
pub mod advanced_mem;

// Architecture Roadmap: Multi-Process Architecture
pub mod process;
pub mod ipc;
pub mod thread;
pub mod interface;
pub mod startup;
pub mod budget;

pub use engine::Engine;
pub use page::Page;
pub use config::Config;
pub use memory::{MemoryManager, MemoryStats, PressureLevel};
pub use arena::{BumpAllocator, Arena, GenArena, GenIndex};
pub use intern::{StringInterner, InternedString, TagInterner};
pub use cow::{Cow, CowBuffer, CowString};
pub use compress::{Lz4Compressor, DeltaEncoder, Varint};
pub use cold::{cold_path, format_error, cold_panic, cold_unreachable, debug_check};
pub use cold::{StaticError, DynDispatch, DynWrapper};
pub use cold::errors as static_errors;
pub use plugin::{Plugin, PluginInfo, PluginCapabilities, PluginError, PluginLoader};
pub use url::{Url, ParseError as UrlParseError, Host, Query, UserInfo};
pub use url::{percent_encode, percent_decode, punycode_encode, punycode_decode};
pub use url::{idn_to_ascii, idn_to_unicode, UrlInterner};

// Phase 23 exports
pub use packed::{CacheAligned, Packed, CompactPair, CACHE_LINE_SIZE};
pub use packed::sentinel;
pub use alloc::{SlabAllocator, PoolAllocator, TypedPool, AllocStats};
pub use simd::{SimdLevel, Color4, Bounds, blend_color, lerp_f32, lerp_f32x4};
pub use fixed_point::{Fixed16, Fixed8, FixedRect};
pub use visibility::{VisibilityState, Viewport, ElementVisibility, CullingContext};
pub use tiered_memory::{Tier, TieredData, TieredMemory, TierViewport, NodePosition, TieredStats};

// Phase 7 exports
pub use compat::{CompatibilityTester, CompatibilityReport, TestResult, FeatureChecker};

// Architecture exports
pub use process::{
    ProcessArchitecture, ProcessType, ProcessId, ProcessState, ProcessArgs, TabId,
    BrowserProcess, TabInfo, RendererProcess, NetworkProcess, GpuProcess, StorageProcess,
};
pub use ipc::{
    IpcMessage, InlineMessage, SharedMemRef, TypedMessage, MessageType,
    IpcChannel, ChannelState, IpcListener,
    SharedMemHandle, SharedMemPool,
    IpcSerialize, IpcError, MessageFrame,
};
pub use thread::{
    ThreadPool, IoThread, CompositorThread, AudioThread, ThreadPoolArchitecture,
    Scheduler, Task, TaskPriority, PendingCounts, SchedulerStats,
};
pub use interface::{
    FrameHost, FrameHostProxy, InProcessFrameHost, NavigationResult, NavigationError, JsValue, LoadEvent,
    NavigationController, NavigationEntry, NavigationState, NavigationTiming,
};
pub use startup::{
    StartupOptimizer, StartupConfig, StartupPhase, StartupTiming, StartupReport,
    LazyInit, Subsystem,
    StartupProfile, FrequentOrigin, ProfileGuidedInit,
};
pub use budget::{MemoryBudget, MemoryMonitor, MemoryPressureLevel, TabMemoryUsage};

// Re-export core sub-crates (always included)
pub use fos_html as html;
pub use fos_css as css;
pub use fos_dom as dom;
pub use fos_layout as layout;
pub use fos_render as render;
pub use fos_js as js;
pub use fos_net as net;

// Re-export optional sub-crates (feature-gated)
#[cfg(feature = "webgl")]
pub use fos_canvas as canvas;

#[cfg(feature = "media")]
pub use fos_media as media;

#[cfg(feature = "devtools")]
pub use fos_devtools as devtools;

#[cfg(feature = "accessibility")]
pub use fos_a11y as a11y;

/// Engine version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
