//! Threading Model
//!
//! Custom thread pool and task scheduler using OS primitives.
//! - Work-stealing for CPU tasks
//! - Dedicated I/O threads
//! - Compositor thread (timing-critical)
//! - Audio thread (real-time)

mod pool;
mod scheduler;

pub use pool::*;
pub use scheduler::*;
