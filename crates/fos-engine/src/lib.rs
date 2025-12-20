//! fOS Engine
//!
//! A lightweight browser engine written in Rust.
//!
//! # Goals
//! - Minimal RAM usage (~20-30MB per tab)
//! - Fast startup
//! - Modern web standards support
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

pub use engine::Engine;
pub use page::Page;
pub use config::Config;
pub use memory::{MemoryManager, MemoryStats, PressureLevel};
pub use arena::{BumpAllocator, Arena, GenArena, GenIndex};
pub use intern::{StringInterner, InternedString, TagInterner};
pub use cow::{Cow, CowBuffer, CowString};
pub use compress::{Lz4Compressor, DeltaEncoder, Varint};

// Re-export sub-crates for advanced usage
pub use fos_html as html;
pub use fos_css as css;
pub use fos_dom as dom;
pub use fos_layout as layout;
pub use fos_render as render;
pub use fos_js as js;
pub use fos_net as net;

/// Engine version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
