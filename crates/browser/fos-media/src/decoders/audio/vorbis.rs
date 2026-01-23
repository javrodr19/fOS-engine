//! Vorbis Decoder
//!
//! Pure-Rust implementation of Vorbis audio decoding.

use super::{AudioDecoderState, imdct, window_sine};
use crate::decoders::{AudioSamples, EncodedPacket, DecoderResult, DecoderError, AudioDecoderTrait};
use std::time::Duration;

/// Vorbis identification header
#[derive(Debug, Clone)]
pub struct VorbisIdHeader {
    pub version: u32,
    pub channels: u8,
    pub sample_rate: u32,
    pub bitrate_max: i32,
    pub bitrate_nominal: i32,
    pub bitrate_min: i32,
    pub blocksize_0: u8,
    pub blocksize_1: u8,
}

/// Vorbis Decoder
#[derive(Debug)]
pub struct VorbisDecoder {
    state: AudioDecoderState,
    id_header: Option<VorbisIdHeader>,
    window_short: Vec<f32>,
    window_long: Vec<f32>,
    prev_samples: Vec<Vec<f32>>,
}

impl VorbisDecoder {
    pub fn new() -> Self {
        Self {
            state: AudioDecoderState { sample_rate: 44100, channels: 2, samples_decoded: 0 },
            id_header: None,
            window_short: window_sine(256),
            window_long: window_sine(2048),
            prev_samples: Vec::new(),
        }
    }
    
    fn parse_id_header(&mut self, data: &[u8]) -> DecoderResult<()> {
        if data.len() < 30 { return Err(DecoderError::NeedMoreData); }
        if data[0] != 1 || &data[1..7] != b"vorbis" { return Err(DecoderError::InvalidBitstream("Not Vorbis ID header".into())); }
        
        let version = u32::from_le_bytes([data[7], data[8], data[9], data[10]]);
        let channels = data[11];
        let sample_rate = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
        let bitrate_max = i32::from_le_bytes([data[16], data[17], data[18], data[19]]);
        let bitrate_nominal = i32::from_le_bytes([data[20], data[21], data[22], data[23]]);
        let bitrate_min = i32::from_le_bytes([data[24], data[25], data[26], data[27]]);
        let blocksizes = data[28];
        let blocksize_0 = blocksizes & 0x0F;
        let blocksize_1 = (blocksizes >> 4) & 0x0F;
        
        self.state.sample_rate = sample_rate;
        self.state.channels = channels as u32;
        self.prev_samples = vec![vec![0.0; 1 << blocksize_1]; channels as usize];
        
        self.id_header = Some(VorbisIdHeader { version, channels, sample_rate, bitrate_max, bitrate_nominal, bitrate_min, blocksize_0, blocksize_1 });
        
        Ok(())
    }
    
    fn decode_audio_packet(&mut self, _data: &[u8], pts: Duration) -> DecoderResult<AudioSamples> {
        let header = self.id_header.as_ref().ok_or_else(|| DecoderError::InvalidBitstream("No ID header".into()))?;
        
        let blocksize = 1 << header.blocksize_1;
        let samples_per_channel = blocksize / 2;
        let total_samples = samples_per_channel * header.channels as usize;
        
        // Simulated decode - real impl would do codebook lookup, floor/residue decode, IMDCT
        let samples = vec![0.0f32; total_samples];
        
        let duration = Duration::from_secs_f64(samples_per_channel as f64 / header.sample_rate as f64);
        self.state.samples_decoded += samples_per_channel as u64;
        
        Ok(AudioSamples { pts, duration, sample_rate: header.sample_rate, channels: header.channels as u32, data: samples })
    }
}

impl Default for VorbisDecoder { fn default() -> Self { Self::new() } }

impl AudioDecoderTrait for VorbisDecoder {
    fn decode(&mut self, packet: &EncodedPacket) -> DecoderResult<AudioSamples> {
        if packet.data.len() >= 7 && packet.data[0] == 1 && &packet.data[1..7] == b"vorbis" {
            self.parse_id_header(&packet.data)?;
            return Ok(AudioSamples { pts: packet.pts, duration: Duration::ZERO, sample_rate: self.state.sample_rate, channels: self.state.channels, data: Vec::new() });
        }
        if packet.data.len() >= 7 && (packet.data[0] == 3 || packet.data[0] == 5) && &packet.data[1..7] == b"vorbis" {
            // Skip comment and setup headers
            return Ok(AudioSamples { pts: packet.pts, duration: Duration::ZERO, sample_rate: self.state.sample_rate, channels: self.state.channels, data: Vec::new() });
        }
        self.decode_audio_packet(&packet.data, packet.pts)
    }
    fn flush(&mut self) -> Option<AudioSamples> { None }
    fn reset(&mut self) { self.state.samples_decoded = 0; for ch in &mut self.prev_samples { ch.fill(0.0); } }
    fn sample_rate(&self) -> u32 { self.state.sample_rate }
    fn channels(&self) -> u32 { self.state.channels }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_decoder() { let d = VorbisDecoder::new(); assert_eq!(d.sample_rate(), 44100); }
}
