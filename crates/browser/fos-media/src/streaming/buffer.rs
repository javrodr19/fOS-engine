//! Buffer Manager
//!
//! Segment buffer management for streaming.

use super::Segment;
use std::time::Duration;
use std::collections::VecDeque;

/// Buffered segment
#[derive(Debug)]
pub struct BufferedSegment { pub segment: Segment, pub data: Vec<u8>, pub buffered_at: std::time::Instant }

/// Buffer manager
#[derive(Debug)]
pub struct BufferManager {
    video_buffer: VecDeque<BufferedSegment>,
    audio_buffer: VecDeque<BufferedSegment>,
    max_buffer: Duration,
    current_position: Duration,
}

impl BufferManager {
    pub fn new(max_buffer: Duration) -> Self {
        Self { video_buffer: VecDeque::new(), audio_buffer: VecDeque::new(), max_buffer, current_position: Duration::ZERO }
    }
    
    pub fn append_video(&mut self, segment: Segment, data: Vec<u8>) {
        self.video_buffer.push_back(BufferedSegment { segment, data, buffered_at: std::time::Instant::now() });
        self.evict_old_segments();
    }
    
    pub fn append_audio(&mut self, segment: Segment, data: Vec<u8>) {
        self.audio_buffer.push_back(BufferedSegment { segment, data, buffered_at: std::time::Instant::now() });
        self.evict_old_segments();
    }
    
    pub fn buffered_video(&self) -> Duration {
        self.video_buffer.iter().map(|s| s.segment.duration).sum()
    }
    
    pub fn buffered_audio(&self) -> Duration {
        self.audio_buffer.iter().map(|s| s.segment.duration).sum()
    }
    
    pub fn buffered(&self) -> Duration {
        self.buffered_video().min(self.buffered_audio())
    }
    
    pub fn set_position(&mut self, position: Duration) {
        self.current_position = position;
        self.evict_old_segments();
    }
    
    fn evict_old_segments(&mut self) {
        while self.buffered_video() > self.max_buffer && !self.video_buffer.is_empty() {
            self.video_buffer.pop_front();
        }
        while self.buffered_audio() > self.max_buffer && !self.audio_buffer.is_empty() {
            self.audio_buffer.pop_front();
        }
    }
    
    pub fn next_video_segment(&mut self) -> Option<BufferedSegment> { self.video_buffer.pop_front() }
    pub fn next_audio_segment(&mut self) -> Option<BufferedSegment> { self.audio_buffer.pop_front() }
    pub fn is_empty(&self) -> bool { self.video_buffer.is_empty() && self.audio_buffer.is_empty() }
}

impl Default for BufferManager { fn default() -> Self { Self::new(Duration::from_secs(30)) } }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_buffer() { let b = BufferManager::default(); assert!(b.is_empty()); }
}
