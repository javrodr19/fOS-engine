//! Renderers
//!
//! Video and audio rendering output.

use crate::decoders::{VideoFrame, AudioSamples, PixelFormat};
use std::time::Duration;

/// Video renderer
#[derive(Debug)]
pub struct VideoRenderer {
    last_frame_time: Duration,
    frames_rendered: u64,
    target_texture: Option<TextureHandle>,
}

/// Texture handle for GPU rendering
#[derive(Debug, Clone)]
pub struct TextureHandle { pub id: u32, pub width: u32, pub height: u32 }

impl VideoRenderer {
    pub fn new() -> Self { Self { last_frame_time: Duration::ZERO, frames_rendered: 0, target_texture: None } }
    
    pub fn render(&mut self, frame: &VideoFrame) {
        // In real impl: upload YUV data to GPU texture, run shader for YUV->RGB conversion
        self.last_frame_time = frame.pts;
        self.frames_rendered += 1;
        
        // Update or create texture if needed
        if self.target_texture.as_ref().map(|t| t.width != frame.width || t.height != frame.height).unwrap_or(true) {
            self.target_texture = Some(TextureHandle { id: 1, width: frame.width, height: frame.height });
        }
    }
    
    pub fn texture(&self) -> Option<&TextureHandle> { self.target_texture.as_ref() }
    pub fn frames_rendered(&self) -> u64 { self.frames_rendered }
}

impl Default for VideoRenderer { fn default() -> Self { Self::new() } }

/// Audio renderer
#[derive(Debug)]
pub struct AudioRenderer {
    sample_rate: u32,
    channels: u32,
    buffer: Vec<f32>,
    samples_rendered: u64,
}

impl AudioRenderer {
    pub fn new() -> Self { Self { sample_rate: 48000, channels: 2, buffer: Vec::with_capacity(4096), samples_rendered: 0 } }
    
    pub fn render(&mut self, samples: &AudioSamples) {
        // In real impl: push to audio output device via ALSA/PulseAudio/CoreAudio/WASAPI
        self.sample_rate = samples.sample_rate;
        self.channels = samples.channels;
        self.samples_rendered += samples.data.len() as u64 / samples.channels as u64;
    }
    
    pub fn samples_rendered(&self) -> u64 { self.samples_rendered }
    pub fn latency(&self) -> Duration { Duration::from_millis(20) } // Target latency
}

impl Default for AudioRenderer { fn default() -> Self { Self::new() } }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_renderers() { let v = VideoRenderer::new(); let a = AudioRenderer::new(); assert_eq!(v.frames_rendered(), 0); assert_eq!(a.samples_rendered(), 0); }
}
