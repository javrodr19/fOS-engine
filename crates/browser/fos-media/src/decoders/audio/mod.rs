//! Audio Decoders
//!
//! Pure-Rust implementations of AAC and Vorbis audio decoders.

pub mod aac;
pub mod vorbis;

use super::{AudioSamples, EncodedPacket, DecoderResult, DecoderError, AudioDecoderTrait};
use std::time::Duration;

/// Common audio decoder state
#[derive(Debug, Default)]
pub struct AudioDecoderState {
    pub sample_rate: u32,
    pub channels: u32,
    pub samples_decoded: u64,
}

/// Window function for IMDCT
pub fn window_sine(size: usize) -> Vec<f32> {
    let mut w = Vec::with_capacity(size);
    let pi = std::f32::consts::PI;
    for i in 0..size {
        w.push(((pi / size as f32) * (i as f32 + 0.5)).sin());
    }
    w
}

/// Simple IMDCT implementation
pub fn imdct(input: &[f32], output: &mut [f32]) {
    let n = input.len();
    let n2 = n * 2;
    let pi = std::f32::consts::PI;
    
    for k in 0..n2 {
        let mut sum = 0.0f32;
        for m in 0..n {
            let angle = pi / n as f32 * (k as f32 + 0.5 + n as f32 / 2.0) * (m as f32 + 0.5);
            sum += input[m] * angle.cos();
        }
        output[k] = sum * (2.0 / n as f32).sqrt();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_window() {
        let w = window_sine(8);
        assert_eq!(w.len(), 8);
        assert!(w[0] > 0.0 && w[0] < 1.0);
    }
}
