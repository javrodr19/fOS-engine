//! Streaming Protocols
//!
//! HLS, DASH, and adaptive bitrate streaming.

pub mod hls;
pub mod dash;
pub mod abr;
pub mod buffer;

use std::time::Duration;

/// Manifest representation
#[derive(Debug, Clone)]
pub struct Manifest {
    pub duration: Option<Duration>,
    pub is_live: bool,
    pub variants: Vec<Variant>,
}

/// Stream variant (quality level)
#[derive(Debug, Clone)]
pub struct Variant {
    pub bandwidth: u64,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub codecs: String,
    pub url: String,
}

/// Segment information
#[derive(Debug, Clone)]
pub struct Segment {
    pub url: String,
    pub duration: Duration,
    pub sequence: u64,
    pub is_init: bool,
    pub byte_range: Option<(u64, u64)>,
}

/// Quality level for ABR
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct QualityLevel {
    pub index: usize,
    pub bandwidth: u64,
}

/// Streaming error
#[derive(Debug, Clone, thiserror::Error)]
pub enum StreamingError {
    #[error("Network error: {0}")]
    Network(String),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Not found")]
    NotFound,
}

pub type StreamingResult<T> = Result<T, StreamingError>;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_quality() { let q = QualityLevel { index: 0, bandwidth: 1000000 }; assert_eq!(q.index, 0); }
}
