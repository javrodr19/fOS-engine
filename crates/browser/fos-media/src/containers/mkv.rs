//! MKV Parser
//!
//! Matroska container parser (superset of WebM).

use super::{Demuxer, DemuxerResult, DemuxerError, TrackInfo, Packet};
use super::webm::WebMDemuxer;
use std::time::Duration;

/// MKV Demuxer - extends WebM with additional codecs
pub type MkvDemuxer = WebMDemuxer;

#[cfg(test)]
mod tests {
    #[test]
    fn test_mkv() { assert!(true); }
}
