//! Media Pipeline
//!
//! Integrates demuxers, decoders, and renderers into a complete playback pipeline.

pub mod clock;
pub mod renderer;
pub mod low_latency;

use crate::containers::{Demuxer, Packet, TrackType};
use crate::decoders::{VideoFrame, AudioSamples, EncodedPacket};
use crate::decoders::hw::{DecoderBackend, HwCodec};
use crate::decoders::audio::aac::AacDecoder;
use crate::decoders::audio::vorbis::VorbisDecoder;
use crate::decoders::AudioDecoderTrait;
use clock::MediaClock;
use renderer::{VideoRenderer, AudioRenderer};
use std::time::Duration;

/// Media pipeline state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineState { Idle, Buffering, Playing, Paused, Ended, Error }

/// Audio decoder backend
#[derive(Debug)]
pub enum AudioDecoderBackend { Aac(AacDecoder), Vorbis(VorbisDecoder) }

/// Media pipeline
pub struct MediaPipeline {
    demuxer: Box<dyn Demuxer>,
    video_decoder: Option<DecoderBackend>,
    audio_decoder: Option<AudioDecoderBackend>,
    video_renderer: VideoRenderer,
    audio_renderer: AudioRenderer,
    clock: MediaClock,
    state: PipelineState,
    video_queue: Vec<VideoFrame>,
    audio_queue: Vec<AudioSamples>,
}

impl std::fmt::Debug for MediaPipeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MediaPipeline").field("state", &self.state).finish()
    }
}

impl MediaPipeline {
    pub fn new(demuxer: Box<dyn Demuxer>) -> Self {
        let video_codec = demuxer.video_track().map(|t| match t.codec {
            crate::containers::CodecId::H264 => HwCodec::H264,
            crate::containers::CodecId::H265 => HwCodec::H265,
            crate::containers::CodecId::Vp8 => HwCodec::Vp8,
            crate::containers::CodecId::Vp9 => HwCodec::Vp9,
            crate::containers::CodecId::Av1 => HwCodec::Av1,
            _ => HwCodec::H264,
        });
        
        let audio_codec = demuxer.audio_track().map(|t| match t.codec {
            crate::containers::CodecId::Aac => AudioDecoderBackend::Aac(AacDecoder::new()),
            crate::containers::CodecId::Vorbis => AudioDecoderBackend::Vorbis(VorbisDecoder::new()),
            _ => AudioDecoderBackend::Aac(AacDecoder::new()),
        });
        
        Self {
            demuxer,
            video_decoder: video_codec.map(DecoderBackend::new),
            audio_decoder: audio_codec,
            video_renderer: VideoRenderer::new(),
            audio_renderer: AudioRenderer::new(),
            clock: MediaClock::new(),
            state: PipelineState::Idle,
            video_queue: Vec::new(),
            audio_queue: Vec::new(),
        }
    }
    
    pub fn state(&self) -> PipelineState { self.state }
    pub fn duration(&self) -> Option<Duration> { self.demuxer.duration() }
    pub fn position(&self) -> Duration { self.clock.position() }
    
    pub fn play(&mut self) { self.state = PipelineState::Playing; self.clock.start(); }
    pub fn pause(&mut self) { self.state = PipelineState::Paused; self.clock.pause(); }
    
    pub fn seek(&mut self, position: Duration) -> Result<(), &'static str> {
        self.demuxer.seek(position).map_err(|_| "Seek failed")?;
        self.video_queue.clear();
        self.audio_queue.clear();
        self.clock.seek(position);
        Ok(())
    }
    
    /// Process one step of the pipeline
    pub fn step(&mut self) -> Result<(), &'static str> {
        if self.state != PipelineState::Playing { return Ok(()); }
        
        // Read and decode packets
        while self.video_queue.len() < 5 && self.audio_queue.len() < 10 {
            match self.demuxer.read_packet() {
                Ok(packet) => self.process_packet(packet)?,
                Err(crate::containers::DemuxerError::EndOfStream) => {
                    self.state = PipelineState::Ended;
                    break;
                }
                Err(_) => break,
            }
        }
        
        // Render frames at correct time
        let pos = self.clock.position();
        
        while let Some(frame) = self.video_queue.first() {
            if frame.pts <= pos {
                let frame = self.video_queue.remove(0);
                self.video_renderer.render(&frame);
            } else { break; }
        }
        
        while let Some(samples) = self.audio_queue.first() {
            if samples.pts <= pos {
                let samples = self.audio_queue.remove(0);
                self.audio_renderer.render(&samples);
            } else { break; }
        }
        
        Ok(())
    }
    
    fn process_packet(&mut self, packet: Packet) -> Result<(), &'static str> {
        let is_video = self.demuxer.video_track().map(|t| t.track_id == packet.track_id).unwrap_or(false);
        let is_audio = self.demuxer.audio_track().map(|t| t.track_id == packet.track_id).unwrap_or(false);
        
        let encoded = packet.to_encoded_packet();
        
        if is_video {
            if let Some(ref mut dec) = self.video_decoder {
                if let Ok(frames) = dec.decode(&encoded) {
                    self.video_queue.extend(frames);
                }
            }
        } else if is_audio {
            if let Some(ref mut dec) = self.audio_decoder {
                let samples = match dec {
                    AudioDecoderBackend::Aac(d) => d.decode(&encoded),
                    AudioDecoderBackend::Vorbis(d) => d.decode(&encoded),
                };
                if let Ok(s) = samples { self.audio_queue.push(s); }
            }
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_state() { assert_eq!(PipelineState::Idle, PipelineState::Idle); }
}
