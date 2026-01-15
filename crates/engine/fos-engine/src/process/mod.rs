//! Process Model
//!
//! Multi-process architecture for browser isolation and performance.
//! - Browser process: UI, navigation, coordination
//! - Renderer process: DOM, layout, paint, JS (per tab)
//! - Network process: All network I/O
//! - GPU process: Compositing, WebGL
//! - Storage process: IndexedDB, cache

mod types;
mod architecture;
mod browser;
mod renderer;
mod network;
mod gpu;
mod storage;

pub use types::*;
pub use architecture::*;
pub use browser::*;
pub use renderer::*;
pub use network::*;
pub use gpu::*;
pub use storage::*;
