//! fOS Browser - Minimalist Web Browser
//!
//! A lightweight web browser built on the fOS Engine.
//! Designed for minimal resource usage with optional feature modules.
//!
//! # Core Features (always available)
//! - Tab management and navigation
//! - HTML/CSS rendering via fOS engine
//! - JavaScript execution via fos-js
//! - HTTP networking with cache via fos-net
//! - Developer tools via fos-devtools
//! - Accessibility via fos-a11y
//! - Media playback via fos-media
//! - Canvas 2D via fos-canvas
//!
//! # Optional Features (behind feature flags)
//! - `full` - Enable all optional modules
//! - `predictive-net` - HTTP/3 and predictive networking
//! - `device-apis` - Battery, gamepad, sensors
//! - `extensions` - Browser extension support

// ============================================================================
// CORE MODULES - Essential for browser operation
// ============================================================================

/// Main browser application and event loop
pub mod app;
/// Page representation with DOM and scripts
pub mod page;
/// Tab management
pub mod tab;
/// Navigation controls and URL handling
pub mod navigation;
/// Page loader (local files, data URLs)
pub mod loader;
/// Browser chrome UI (tabs, URL bar)
pub mod ui;
/// Page rendering pipeline
pub mod renderer;
/// JavaScript runtime integration
pub mod js_runtime;
/// Network requests with HTTP cache
pub mod network;
/// Developer tools
pub mod devtools;
/// Accessibility tree and focus management
pub mod accessibility;
/// Media element handling (video, audio)
pub mod media;
/// Canvas 2D rendering
pub mod canvas;
/// Security policies (CSP, CORS, sandbox)
pub mod security;
/// Memory management and pressure handling
pub mod memory;
/// Event handling
pub mod events;
/// localStorage and sessionStorage
pub mod storage;
/// Navigation history
pub mod history;
/// Form data handling
pub mod forms;
/// Cookie management
pub mod cookies;
/// Advanced networking (WebSocket, XHR, SSE)
pub mod advanced_net;
/// Profiling and performance metrics
pub mod profiling;
/// Optimization utilities
pub mod optimization;
/// Browser compatibility layer
pub mod compat;
/// Web API integrations
pub mod webapi;
/// Web Workers support
pub mod workers;
/// Service Worker support
pub mod service_worker;
/// IndexedDB storage
pub mod indexeddb;
/// Web Animations with Fixed-Point timing
pub mod web_animations;
/// WebRTC for real-time communications
pub mod webrtc;

// ============================================================================
// PERFORMANCE MODULES - Memory and rendering optimizations from fos-engine
// ============================================================================

/// Tiered memory management
pub mod tiered;
/// Benchmarking utilities
pub mod benchmark;
/// Resource deduplication
pub mod dedup;
/// Fixed-point arithmetic
pub mod fixed;
/// Sentinel/optional value optimization
pub mod sentinel;
/// Compression utilities
pub mod compress;
/// Advanced memory structures (SmallVec, etc)
pub mod advanced_mem;
/// Copy-on-Write support
pub mod cow;
/// SIMD acceleration
pub mod simd;
/// Visibility culling
pub mod visibility;
/// String interning
pub mod intern;

// ============================================================================
// OPTIONAL MODULES - Feature-gated for smaller binary size
// ============================================================================

// Predictive networking features
#[cfg(feature = "predictive-net")]
pub mod http3;
#[cfg(feature = "predictive-net")]
pub mod predictive;

// Device and platform APIs
#[cfg(feature = "device-apis")]
pub mod battery;
#[cfg(feature = "device-apis")]
pub mod gamepad;
#[cfg(feature = "device-apis")]
pub mod sensors;
#[cfg(feature = "device-apis")]
pub mod network_info;
#[cfg(feature = "device-apis")]
pub mod geolocation;

// Extension support
#[cfg(feature = "extensions")]
pub mod extensions;
#[cfg(feature = "extensions")]
pub mod plugins;

// Additional web APIs (included with 'full' feature)
#[cfg(feature = "full")]
pub mod notifications;
#[cfg(feature = "full")]
pub mod dragdrop;
#[cfg(feature = "full")]
pub mod permissions;
#[cfg(feature = "full")]
pub mod fullscreen;
#[cfg(feature = "full")]
pub mod builtins;
#[cfg(feature = "full")]
pub mod touch;
#[cfg(feature = "full")]
pub mod validation;
#[cfg(feature = "full")]
pub mod selection;
#[cfg(feature = "full")]
pub mod scroll;
#[cfg(feature = "full")]
pub mod resize_observer;
#[cfg(feature = "full")]
pub mod intersection_observer;
#[cfg(feature = "full")]
pub mod animation;
#[cfg(feature = "full")]
pub mod performance;
#[cfg(feature = "full")]
pub mod dialog;
#[cfg(feature = "full")]
pub mod share;
#[cfg(feature = "full")]
pub mod broadcast;
#[cfg(feature = "full")]
pub mod page_visibility;
#[cfg(feature = "full")]
pub mod pointer;
#[cfg(feature = "full")]
pub mod mutation_observer;
#[cfg(feature = "full")]
pub mod find;
#[cfg(feature = "full")]
pub mod downloads;
#[cfg(feature = "full")]
pub mod clipboard;
#[cfg(feature = "full")]
pub mod bookmarks;
#[cfg(feature = "full")]
pub mod print;
#[cfg(feature = "full")]
pub mod passwords;

// ============================================================================
// PUBLIC EXPORTS - Core types available by default
// ============================================================================

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

// Form and cookie exports
pub use forms::{FormData, FormCollector, FormMethod};
pub use cookies::{Cookie, CookieJar};
pub use service_worker::{ServiceWorkerManager, CacheStorage};
pub use indexeddb::{IDBFactory, IDBDatabase};

// ============================================================================
// FEATURE-GATED EXPORTS
// ============================================================================

#[cfg(feature = "predictive-net")]
pub use http3::{Http3Manager, Http3Settings};
#[cfg(feature = "predictive-net")]
pub use predictive::{PredictiveNetwork, ResourceHint};

#[cfg(feature = "device-apis")]
pub use battery::BatteryManager;
#[cfg(feature = "device-apis")]
pub use gamepad::{GamepadManager, Gamepad, GamepadButton};
#[cfg(feature = "device-apis")]
pub use sensors::{SensorsManager, Accelerometer, Gyroscope, DeviceOrientation};
#[cfg(feature = "device-apis")]
pub use network_info::{NetworkInfoManager, NetworkInformation, ConnectionType};
#[cfg(feature = "device-apis")]
pub use geolocation::{GeolocationManager, Position, Coordinates};

#[cfg(feature = "extensions")]
pub use extensions::{ExtensionManager, Extension, ExtensionManifest};
#[cfg(feature = "extensions")]
pub use plugins::PluginManager;

#[cfg(feature = "full")]
pub use find::FindInPage;
#[cfg(feature = "full")]
pub use downloads::{Download, DownloadManager, DownloadState};
#[cfg(feature = "full")]
pub use clipboard::Clipboard;
#[cfg(feature = "full")]
pub use bookmarks::{Bookmark, BookmarkManager};
#[cfg(feature = "full")]
pub use print::{PrintManager, PrintSettings};
#[cfg(feature = "full")]
pub use passwords::PasswordManager;
#[cfg(feature = "full")]
pub use notifications::{NotificationManager, Notification, NotificationPermission};
#[cfg(feature = "full")]
pub use dragdrop::{DragDropManager, DataTransfer, DragEvent};
#[cfg(feature = "full")]
pub use permissions::{PermissionsManager, PermissionName, PermissionState};
#[cfg(feature = "full")]
pub use fullscreen::{FullscreenManager, WakeLockManager};
#[cfg(feature = "full")]
pub use builtins::{BuiltinsManager, AsyncContext};
#[cfg(feature = "full")]
pub use touch::{TouchManager, Touch, TouchEvent, Gesture};
#[cfg(feature = "full")]
pub use validation::{InputValidator, ValidityState, ValidationConstraints};
#[cfg(feature = "full")]
pub use selection::{SelectionManager, Selection, TextRange};
#[cfg(feature = "full")]
pub use scroll::{ScrollManager, ScrollBehavior, ScrollPosition};
#[cfg(feature = "full")]
pub use resize_observer::{ResizeObserver, ResizeObserverManager, ResizeObserverEntry};
#[cfg(feature = "full")]
pub use intersection_observer::{IntersectionObserver, IntersectionObserverManager, DOMRect};
#[cfg(feature = "full")]
pub use animation::{Animation, AnimationManager, AnimationTiming, Keyframe};
#[cfg(feature = "full")]
pub use performance::{PerformanceApi, NavigationTiming, ResourceTiming};
#[cfg(feature = "full")]
pub use dialog::{DialogManager, Dialog, BuiltinDialogs};
#[cfg(feature = "full")]
pub use share::{ShareManager, ShareData};
#[cfg(feature = "full")]
pub use broadcast::{BroadcastChannelManager, BroadcastChannel, BroadcastMessage};
#[cfg(feature = "full")]
pub use page_visibility::{PageVisibilityManager, DocumentVisibility};
#[cfg(feature = "full")]
pub use pointer::{PointerManager, PointerEvent, PointerType};
#[cfg(feature = "full")]
pub use mutation_observer::{MutationObserverManager, MutationObserver, MutationRecord};
