//! IPC (Inter-Process Communication)
//!
//! Zero-copy IPC primitives for process communication.
//! - Message types (inline, shared memory, file descriptors)
//! - Channel abstraction (Unix sockets, Windows named pipes)
//! - Shared memory regions
//! - Compact binary serialization

mod message;
mod channel;
mod shared_memory;
mod serialize;

pub use message::*;
pub use channel::*;
pub use shared_memory::*;
pub use serialize::*;
