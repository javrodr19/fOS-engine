//! Startup Optimization
//!
//! Fast startup through prefork, mmap, lazy init, and profile-guided prefetch.

mod optimizer;
mod profile;

pub use optimizer::*;
pub use profile::*;
