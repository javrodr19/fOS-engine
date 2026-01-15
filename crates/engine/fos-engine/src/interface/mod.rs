//! Interface Definitions (Mojo Equivalent)
//!
//! Trait-based remote interfaces for cross-process communication.
//! Auto-generated IPC proxies for navigation, script execution, etc.

mod frame_host;
mod navigation;

pub use frame_host::*;
pub use navigation::*;
