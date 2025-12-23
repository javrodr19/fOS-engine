//! fOS Browser - Minimalist Web Browser
//!
//! A lightweight web browser built on the fOS Engine.
//! Designed for minimal resource usage.
//!
//! # Features
//! - JavaScript execution via fos-js
//! - HTTP caching and prefetch via fos-net
//! - Security policies via fos-security
//! - Accessibility via fos-a11y
//! - Developer tools via fos-devtools
//! - Media playback via fos-media
//! - Canvas 2D/WebGL via fos-canvas
//! - WebSocket, XHR, SSE via fos-net
//! - Memory pressure and tab hibernation via fos-engine

pub mod app;
pub mod page;
pub mod tab;
pub mod navigation;
pub mod loader;
pub mod ui;
pub mod renderer;
pub mod js_runtime;
pub mod network;
pub mod devtools;
pub mod accessibility;
pub mod media;
pub mod canvas;
pub mod advanced_net;
pub mod security;
pub mod memory;
pub mod events;
pub mod storage;
pub mod workers;
pub mod webapi;
pub mod history;
pub mod profiling;
pub mod optimization;
pub mod compat;
pub mod plugins;
pub mod tiered;
pub mod benchmark;
pub mod dedup;
pub mod fixed;
pub mod sentinel;
pub mod compress;
pub mod advanced_mem;
pub mod cow;
pub mod simd;
pub mod visibility;
pub mod intern;

// New integration modules
pub mod forms;
pub mod cookies;
pub mod find;
pub mod downloads;
pub mod clipboard;
pub mod bookmarks;
pub mod print;
pub mod service_worker;
pub mod indexeddb;
pub mod passwords;
pub mod extensions;

pub use app::Browser;
pub use page::Page;
pub use tab::Tab;
pub use renderer::{PageRenderer, RenderedPage};
pub use js_runtime::PageJsRuntime;
pub use network::NetworkManager;
pub use devtools::DevTools;
pub use accessibility::AccessibilityManager;
pub use media::MediaManager;
pub use canvas::CanvasManager;
pub use advanced_net::AdvancedNetworking;
pub use security::SecurityManager;
pub use memory::MemoryIntegration;
pub use events::EventManager;
pub use storage::StorageManager;
pub use workers::WorkerIntegration;
pub use webapi::WebApiManager;
pub use history::NavigationIntegration;
pub use profiling::PerformanceProfiler;
pub use optimization::OptimizationManager;
pub use compat::CompatibilityManager;
pub use plugins::PluginManager;
pub use tiered::TieredMemoryManager;
pub use benchmark::BrowserBenchmark;
pub use dedup::DeduplicationManager;
pub use fixed::{Fixed16, FixedRect};
pub use sentinel::{Opt, OptF32, OptDimension, OptEdges};
pub use compress::{Lz4Compressor, DeltaEncoder, Varint, RunLengthEncoder, BitPacker};
pub use advanced_mem::{SmallVec, PackedElement, InternedCssValue, ViewportRegion};
pub use cow::{Cow, CowBuffer, CowString, BumpAllocator};
pub use simd::{SimdLevel, Color4, Bounds, blend_color};
pub use visibility::{VisibilityState, Viewport, CullingContext};
pub use intern::{StringInterner, TagInterner, CssPropInterner};

// New feature exports
pub use forms::{FormData, FormCollector, FormMethod};
pub use cookies::{Cookie, CookieJar};
pub use find::FindInPage;
pub use downloads::{Download, DownloadManager, DownloadState};
pub use clipboard::Clipboard;
pub use bookmarks::{Bookmark, BookmarkManager};
pub use print::{PrintManager, PrintSettings};
pub use service_worker::{ServiceWorkerManager, CacheStorage};
pub use indexeddb::{IDBFactory, IDBDatabase};
pub use passwords::PasswordManager;
pub use extensions::{ExtensionManager, Extension, ExtensionManifest};

