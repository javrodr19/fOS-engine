//! fOS DevTools
//!
//! Developer tools for the fOS browser engine.
//!
//! Features:
//! - Console (log, warn, error)
//! - Element inspector
//! - Network panel
//! - JavaScript debugger
//! - Performance profiling

pub mod console;
pub mod inspector;
pub mod network;
pub mod debugger;
pub mod performance;

pub use console::{Console, ConsoleMessage, ConsoleValue, LogLevel};
pub use inspector::{Inspector, InspectedNode, NodeType};
pub use network::{NetworkPanel, NetworkRequest, NetworkResponse};
pub use debugger::{Debugger, Breakpoint, CallFrame, DebuggerState};
pub use performance::{PerformancePanel, FrameTimingInfo, MemoryInfo};

/// DevTools error
#[derive(Debug, thiserror::Error)]
pub enum DevToolsError {
    #[error("Not paused")]
    NotPaused,
    
    #[error("Expression evaluation failed: {0}")]
    EvaluationFailed(String),
}
