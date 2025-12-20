//! Web Workers Module
//!
//! Web Workers, Shared Workers, Service Workers.

mod web_worker;
pub mod service_worker;
pub mod shared_worker;

pub use web_worker::*;
pub use service_worker::{ServiceWorkerContainer, ServiceWorker, ServiceWorkerRegistration};
pub use shared_worker::{SharedWorker, MessagePort, MessageChannel};
