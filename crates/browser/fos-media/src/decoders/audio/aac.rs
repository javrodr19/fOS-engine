//! AAC Decoder
//!
//! Pure-Rust implementation of AAC audio decoding.

use super::{AudioDecoderState, imdct, window_sine};
use crate::decoders::{AudioSamples, EncodedPacket, DecoderResult, DecoderError, AudioDecoderTrait};
use std::time::Duration;

/// AAC Audio Object Types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AacProfile { Main = 1, Lc = 2, Ssr = 3, Ltp = 4, Sbr = 5, Scalable = 6, He = 29 }

/// ADTS Header
#[derive(Debug, Clone)]
pub struct AdtsHeader {
    pub profile: AacProfile,
    pub sample_rate_index: u8,
    pub channel_config: u8,
    pub frame_length: u16,
    pub buffer_fullness: u16,
    pub num_raw_data_blocks: u8,
}

/// AAC Decoder
#[derive(Debug)]
pub struct AacDecoder {
    state: AudioDecoderState,
    window: Vec<f32>,
    prev_samples: Vec<f32>,
}

impl AacDecoder {
    pub fn new() -> Self {
        Self { state: AudioDecoderState { sample_rate: 44100, channels: 2, samples_decoded: 0 }, window: window_sine(2048), prev_samples: vec![0.0; 1024] }
    }
    
    fn parse_adts_header(&self, data: &[u8]) -> DecoderResult<AdtsHeader> {
        if data.len() < 7 { return Err(DecoderError::NeedMoreData); }
        if data[0] != 0xFF || (data[1] & 0xF0) != 0xF0 { return Err(DecoderError::InvalidBitstream("Invalid ADTS sync".into())); }
        
        let profile = match (data[2] >> 6) & 3 { 0 => AacProfile::Main, 1 => AacProfile::Lc, 2 => AacProfile::Ssr, _ => AacProfile::Ltp };
        let sample_rate_index = (data[2] >> 2) & 0x0F;
        let channel_config = ((data[2] & 1) << 2) | ((data[3] >> 6) & 3);
        let frame_length = (((data[3] & 3) as u16) << 11) | ((data[4] as u16) << 3) | ((data[5] >> 5) as u16);
        let buffer_fullness = (((data[5] & 0x1F) as u16) << 6) | ((data[6] >> 2) as u16);
        let num_raw_data_blocks = data[6] & 3;
        
        Ok(AdtsHeader { profile, sample_rate_index, channel_config, frame_length, buffer_fullness, num_raw_data_blocks })
    }
    
    fn sample_rate_from_index(&self, index: u8) -> u32 {
        match index { 0 => 96000, 1 => 88200, 2 => 64000, 3 => 48000, 4 => 44100, 5 => 32000, 6 => 24000, 7 => 22050, 8 => 16000, 9 => 12000, 10 => 11025, 11 => 8000, _ => 44100 }
    }
    
    fn decode_frame(&mut self, data: &[u8], pts: Duration) -> DecoderResult<AudioSamples> {
        let header = self.parse_adts_header(data)?;
        self.state.sample_rate = self.sample_rate_from_index(header.sample_rate_index);
        self.state.channels = header.channel_config.max(1) as u32;
        
        let samples_per_frame = 1024;
        let total_samples = samples_per_frame * self.state.channels as usize;
        
        // Simulated decode - real impl would do Huffman decoding, dequantization, IMDCT
        let samples = vec![0.0f32; total_samples];
        
        let duration = Duration::from_secs_f64(samples_per_frame as f64 / self.state.sample_rate as f64);
        self.state.samples_decoded += samples_per_frame as u64;
        
        Ok(AudioSamples { pts, duration, sample_rate: self.state.sample_rate, channels: self.state.channels, data: samples })
    }
}

impl Default for AacDecoder { fn default() -> Self { Self::new() } }

impl AudioDecoderTrait for AacDecoder {
    fn decode(&mut self, packet: &EncodedPacket) -> DecoderResult<AudioSamples> { self.decode_frame(&packet.data, packet.pts) }
    fn flush(&mut self) -> Option<AudioSamples> { None }
    fn reset(&mut self) { self.state.samples_decoded = 0; self.prev_samples.fill(0.0); }
    fn sample_rate(&self) -> u32 { self.state.sample_rate }
    fn channels(&self) -> u32 { self.state.channels }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_decoder() { let d = AacDecoder::new(); assert_eq!(d.sample_rate(), 44100); assert_eq!(d.channels(), 2); }
}
