//! Threading Model
//!
//! Custom thread pool and task scheduler using OS primitives.
//! - Work-stealing for CPU tasks
//! - Dedicated I/O threads
//! - Compositor thread (timing-critical)
//! - Audio thread (real-time)

mod pool;
mod scheduler;
pub mod work_stealing;

pub use pool::*;
pub use scheduler::*;
pub use work_stealing::{WorkStealingScheduler, TaskPriority as WorkStealPriority, ScopedScheduler, scope as work_scope};
