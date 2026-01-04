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
//! - Storage inspector
//! - Application panel
//! - Sources panel
//! - Elements panel
//! - Lighthouse audits
//! - Memory panel

pub mod console;
pub mod inspector;
pub mod network;
pub mod debugger;
pub mod performance;
pub mod storage;
pub mod application;
pub mod sources;
pub mod elements;
pub mod lighthouse;
pub mod memory;

pub use console::{Console, ConsoleMessage, ConsoleValue, LogLevel};
pub use inspector::{Inspector, InspectedNode, NodeType};
pub use network::{NetworkPanel, NetworkRequest, NetworkResponse};
pub use debugger::{Debugger, Breakpoint, CallFrame, DebuggerState};
pub use performance::{PerformancePanel, FrameTimingInfo, MemoryInfo};
pub use storage::{StorageInspector, StoragePanel, StorageType, StorageEntry};
pub use application::{ApplicationPanel, ServiceWorkerInfo, WebAppManifest, PwaStatus};
pub use sources::{SourcesPanel, SourceFile, SourceMap, JsPrettyPrinter};
pub use elements::{ElementsPanel, ElementNode, ComputedStyles, BoxModel, MatchedRule};
pub use lighthouse::{LighthousePanel, LighthouseReport, AuditResult, CategoryScore};
pub use memory::{MemoryPanel, HeapSnapshot, HeapNode, AllocationSample};

/// DevTools error
#[derive(Debug, thiserror::Error)]
pub enum DevToolsError {
    #[error("Not paused")]
    NotPaused,
    
    #[error("Expression evaluation failed: {0}")]
    EvaluationFailed(String),
    
    #[error("Snapshot not found: {0}")]
    SnapshotNotFound(u64),
}
