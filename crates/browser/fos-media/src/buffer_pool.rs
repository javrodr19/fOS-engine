//! Media Buffer Pooling
//!
//! Object pooling for audio and video buffers to reduce allocation overhead
//! during media playback and processing.

use std::sync::Mutex;

/// Generic buffer pool
#[derive(Debug)]
pub struct BufferPool<T> {
    available: Mutex<Vec<T>>,
    capacity: usize,
}

impl<T> BufferPool<T> {
    pub fn new() -> Self {
        Self {
            available: Mutex::new(Vec::new()),
            capacity: 32,
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            available: Mutex::new(Vec::with_capacity(capacity)),
            capacity,
        }
    }

    pub fn get(&self) -> Option<T> {
        self.available.lock().unwrap().pop()
    }

    pub fn put(&self, item: T) {
        let mut pool = self.available.lock().unwrap();
        if pool.len() < self.capacity {
            pool.push(item);
        }
    }

    pub fn available(&self) -> usize {
        self.available.lock().unwrap().len()
    }

    pub fn clear(&self) {
        self.available.lock().unwrap().clear();
    }
}

impl<T> Default for BufferPool<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Audio buffer for decoded PCM samples
#[derive(Debug)]
pub struct AudioBuffer {
    /// Sample data (interleaved for multi-channel)
    pub samples: Vec<f32>,
    /// Number of channels
    pub channels: u32,
    /// Sample rate in Hz
    pub sample_rate: u32,
    /// Number of samples per channel
    pub length: usize,
}

impl AudioBuffer {
    pub fn new(channels: u32, sample_rate: u32, length: usize) -> Self {
        Self {
            samples: vec![0.0; length * channels as usize],
            channels,
            sample_rate,
            length,
        }
    }

    /// Get buffer duration in seconds
    pub fn duration(&self) -> f64 {
        self.length as f64 / self.sample_rate as f64
    }

    /// Reset buffer for reuse
    pub fn reset(&mut self) {
        self.samples.fill(0.0);
    }

    /// Get channel data
    pub fn get_channel_data(&self, channel: u32) -> Option<&[f32]> {
        if channel >= self.channels {
            return None;
        }
        let start = channel as usize * self.length;
        let end = start + self.length;
        Some(&self.samples[start..end])
    }

    /// Get mutable channel data
    pub fn get_channel_data_mut(&mut self, channel: u32) -> Option<&mut [f32]> {
        if channel >= self.channels {
            return None;
        }
        let start = channel as usize * self.length;
        let end = start + self.length;
        Some(&mut self.samples[start..end])
    }
}

/// Video frame buffer
#[derive(Debug)]
pub struct VideoFrame {
    /// Pixel data (RGBA format)
    pub data: Vec<u8>,
    /// Frame width
    pub width: u32,
    /// Frame height
    pub height: u32,
    /// Presentation timestamp (microseconds)
    pub timestamp: u64,
    /// Frame format
    pub format: VideoFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VideoFormat {
    #[default]
    Rgba,
    Bgra,
    I420,
    Nv12,
}

impl VideoFrame {
    pub fn new(width: u32, height: u32, format: VideoFormat) -> Self {
        let size = Self::calculate_size(width, height, format);
        Self {
            data: vec![0u8; size],
            width,
            height,
            timestamp: 0,
            format,
        }
    }

    fn calculate_size(width: u32, height: u32, format: VideoFormat) -> usize {
        let pixels = (width * height) as usize;
        match format {
            VideoFormat::Rgba | VideoFormat::Bgra => pixels * 4,
            VideoFormat::I420 => pixels + pixels / 2, // Y + U/4 + V/4
            VideoFormat::Nv12 => pixels + pixels / 2, // Y + UV/2
        }
    }

    /// Reset frame for reuse
    pub fn reset(&mut self) {
        self.data.fill(0);
        self.timestamp = 0;
    }
}

/// Audio buffer pool
pub type AudioBufferPool = BufferPool<AudioBuffer>;

/// Video frame pool
pub type VideoFramePool = BufferPool<VideoFrame>;

/// Media buffer manager
#[derive(Debug, Default)]
pub struct MediaBufferManager {
    audio_pool: AudioBufferPool,
    video_pool: VideoFramePool,
}

impl MediaBufferManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Acquire audio buffer
    pub fn acquire_audio(&self, channels: u32, sample_rate: u32, length: usize) -> AudioBuffer {
        if let Some(mut buf) = self.audio_pool.get() {
            // Check if buffer is compatible
            if buf.channels == channels && buf.sample_rate == sample_rate && buf.length >= length {
                buf.length = length;
                buf.samples.truncate(length * channels as usize);
                buf.reset();
                return buf;
            }
        }
        AudioBuffer::new(channels, sample_rate, length)
    }

    /// Release audio buffer back to pool
    pub fn release_audio(&self, buf: AudioBuffer) {
        self.audio_pool.put(buf);
    }

    /// Acquire video frame
    pub fn acquire_video(&self, width: u32, height: u32, format: VideoFormat) -> VideoFrame {
        if let Some(mut frame) = self.video_pool.get() {
            if frame.width == width && frame.height == height && frame.format == format {
                frame.reset();
                return frame;
            }
        }
        VideoFrame::new(width, height, format)
    }

    /// Release video frame back to pool
    pub fn release_video(&self, frame: VideoFrame) {
        self.video_pool.put(frame);
    }

    /// Get pool statistics
    pub fn stats(&self) -> MediaPoolStats {
        MediaPoolStats {
            audio_available: self.audio_pool.available(),
            video_available: self.video_pool.available(),
        }
    }

    /// Clear all pools
    pub fn clear(&self) {
        self.audio_pool.clear();
        self.video_pool.clear();
    }
}

/// Pool statistics
#[derive(Debug, Clone)]
pub struct MediaPoolStats {
    pub audio_available: usize,
    pub video_available: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_buffer() {
        let buf = AudioBuffer::new(2, 44100, 1024);
        assert_eq!(buf.channels, 2);
        assert_eq!(buf.sample_rate, 44100);
        assert!((buf.duration() - 0.023).abs() < 0.001);
    }

    #[test]
    fn test_video_frame() {
        let frame = VideoFrame::new(1920, 1080, VideoFormat::Rgba);
        assert_eq!(frame.data.len(), 1920 * 1080 * 4);
    }

    #[test]
    fn test_buffer_pooling() {
        let mgr = MediaBufferManager::new();

        // Acquire and release audio
        let buf = mgr.acquire_audio(2, 44100, 1024);
        mgr.release_audio(buf);
        assert_eq!(mgr.stats().audio_available, 1);

        // Reuse
        let _buf = mgr.acquire_audio(2, 44100, 1024);
        assert_eq!(mgr.stats().audio_available, 0);
    }

    #[test]
    fn test_video_pooling() {
        let mgr = MediaBufferManager::new();

        let frame = mgr.acquire_video(1920, 1080, VideoFormat::Rgba);
        mgr.release_video(frame);
        assert_eq!(mgr.stats().video_available, 1);
    }
}
