//! Media Source Extensions
//!
//! MSE API for adaptive streaming.

use std::collections::VecDeque;

/// Media Source ready state
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MediaSourceReadyState {
    #[default]
    Closed,
    Open,
    Ended,
}

/// Media Source
#[derive(Debug)]
pub struct MediaSource {
    pub ready_state: MediaSourceReadyState,
    pub duration: f64,
    pub source_buffers: Vec<SourceBuffer>,
    pub active_source_buffers: Vec<usize>,
}

/// Source Buffer
#[derive(Debug)]
pub struct SourceBuffer {
    pub mode: AppendMode,
    pub updating: bool,
    pub buffered: super::element::TimeRanges,
    pub timestamp_offset: f64,
    pub append_window_start: f64,
    pub append_window_end: f64,
    buffer: VecDeque<u8>,
}

/// Append mode
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AppendMode {
    #[default]
    Segments,
    Sequence,
}

impl MediaSource {
    pub fn new() -> Self {
        Self {
            ready_state: MediaSourceReadyState::Closed,
            duration: f64::NAN,
            source_buffers: Vec::new(),
            active_source_buffers: Vec::new(),
        }
    }
    
    /// Check if type is supported
    pub fn is_type_supported(mime_type: &str) -> bool {
        matches!(mime_type, 
            "video/mp4" | "video/webm" | "audio/mp4" | "audio/webm" |
            "video/mp4; codecs=\"avc1.42E01E\"" |
            "video/webm; codecs=\"vp8\"" |
            "video/webm; codecs=\"vp9\""
        )
    }
    
    /// Add source buffer
    pub fn add_source_buffer(&mut self, mime_type: &str) -> Result<usize, MseError> {
        if self.ready_state != MediaSourceReadyState::Open {
            return Err(MseError::InvalidState);
        }
        
        if !Self::is_type_supported(mime_type) {
            return Err(MseError::NotSupported);
        }
        
        let buffer = SourceBuffer::new();
        self.source_buffers.push(buffer);
        Ok(self.source_buffers.len() - 1)
    }
    
    /// Remove source buffer
    pub fn remove_source_buffer(&mut self, index: usize) -> Result<(), MseError> {
        if index >= self.source_buffers.len() {
            return Err(MseError::InvalidState);
        }
        self.source_buffers.remove(index);
        Ok(())
    }
    
    /// End of stream
    pub fn end_of_stream(&mut self, error: Option<EndOfStreamError>) {
        if error.is_none() {
            self.ready_state = MediaSourceReadyState::Ended;
        }
    }
    
    /// Set live seekable range
    pub fn set_live_seekable_range(&mut self, _start: f64, _end: f64) {}
    
    /// Clear live seekable range
    pub fn clear_live_seekable_range(&mut self) {}
}

impl Default for MediaSource {
    fn default() -> Self { Self::new() }
}

impl SourceBuffer {
    pub fn new() -> Self {
        Self {
            mode: AppendMode::Segments,
            updating: false,
            buffered: super::element::TimeRanges::new(),
            timestamp_offset: 0.0,
            append_window_start: 0.0,
            append_window_end: f64::INFINITY,
            buffer: VecDeque::new(),
        }
    }
    
    /// Append buffer
    pub fn append_buffer(&mut self, data: &[u8]) -> Result<(), MseError> {
        if self.updating {
            return Err(MseError::InvalidState);
        }
        
        self.updating = true;
        self.buffer.extend(data);
        self.updating = false;
        Ok(())
    }
    
    /// Abort
    pub fn abort(&mut self) -> Result<(), MseError> {
        self.updating = false;
        Ok(())
    }
    
    /// Remove buffered range
    pub fn remove(&mut self, _start: f64, _end: f64) -> Result<(), MseError> {
        if self.updating {
            return Err(MseError::InvalidState);
        }
        Ok(())
    }
    
    /// Change type
    pub fn change_type(&mut self, _mime_type: &str) -> Result<(), MseError> {
        Ok(())
    }
}

impl Default for SourceBuffer {
    fn default() -> Self { Self::new() }
}

/// MSE error
#[derive(Debug, Clone)]
pub enum MseError {
    InvalidState,
    NotSupported,
    QuotaExceeded,
}

/// End of stream error
#[derive(Debug, Clone, Copy)]
pub enum EndOfStreamError {
    Network,
    Decode,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_media_source() {
        let mut ms = MediaSource::new();
        ms.ready_state = MediaSourceReadyState::Open;
        
        let idx = ms.add_source_buffer("video/mp4").unwrap();
        assert_eq!(idx, 0);
    }
    
    #[test]
    fn test_source_buffer() {
        let mut sb = SourceBuffer::new();
        sb.append_buffer(&[1, 2, 3, 4]).unwrap();
        assert!(!sb.updating);
    }
}
