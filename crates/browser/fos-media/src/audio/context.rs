//! Web Audio API
//!
//! AudioContext and audio nodes.

/// Audio context state
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AudioContextState {
    #[default]
    Suspended,
    Running,
    Closed,
}

/// Base audio context
#[derive(Debug)]
pub struct AudioContext {
    pub state: AudioContextState,
    pub sample_rate: f32,
    pub current_time: f64,
    pub destination: AudioDestinationNode,
    nodes: Vec<AudioNode>,
    next_id: u32,
}

/// Audio destination node
#[derive(Debug, Clone)]
pub struct AudioDestinationNode {
    pub max_channel_count: u32,
    pub channel_count: u32,
}

impl Default for AudioDestinationNode {
    fn default() -> Self {
        Self {
            max_channel_count: 2,
            channel_count: 2,
        }
    }
}

impl AudioContext {
    pub fn new() -> Self {
        Self::with_options(AudioContextOptions::default())
    }
    
    pub fn with_options(options: AudioContextOptions) -> Self {
        Self {
            state: AudioContextState::Suspended,
            sample_rate: options.sample_rate.unwrap_or(44100.0),
            current_time: 0.0,
            destination: AudioDestinationNode::default(),
            nodes: Vec::new(),
            next_id: 1,
        }
    }
    
    /// Resume playback
    pub fn resume(&mut self) -> Result<(), &'static str> {
        self.state = AudioContextState::Running;
        Ok(())
    }
    
    /// Suspend playback
    pub fn suspend(&mut self) -> Result<(), &'static str> {
        self.state = AudioContextState::Suspended;
        Ok(())
    }
    
    /// Close context
    pub fn close(&mut self) -> Result<(), &'static str> {
        self.state = AudioContextState::Closed;
        Ok(())
    }
    
    /// Create oscillator
    pub fn create_oscillator(&mut self) -> OscillatorNode {
        let id = self.next_id;
        self.next_id += 1;
        OscillatorNode::new(id)
    }
    
    /// Create gain
    pub fn create_gain(&mut self) -> GainNode {
        let id = self.next_id;
        self.next_id += 1;
        GainNode::new(id)
    }
    
    /// Create analyser
    pub fn create_analyser(&mut self) -> AnalyserNode {
        let id = self.next_id;
        self.next_id += 1;
        AnalyserNode::new(id)
    }
    
    /// Create buffer source
    pub fn create_buffer_source(&mut self) -> AudioBufferSourceNode {
        let id = self.next_id;
        self.next_id += 1;
        AudioBufferSourceNode::new(id)
    }
    
    /// Create delay
    pub fn create_delay(&mut self, max_delay: f64) -> DelayNode {
        let id = self.next_id;
        self.next_id += 1;
        DelayNode::new(id, max_delay)
    }
    
    /// Create biquad filter
    pub fn create_biquad_filter(&mut self) -> BiquadFilterNode {
        let id = self.next_id;
        self.next_id += 1;
        BiquadFilterNode::new(id)
    }
    
    /// Decode audio data
    pub fn decode_audio_data(&self, _data: &[u8]) -> Result<AudioBuffer, &'static str> {
        // Would decode audio
        Ok(AudioBuffer::new(2, 44100, 44100.0))
    }
}

impl Default for AudioContext {
    fn default() -> Self { Self::new() }
}

/// Audio context options
#[derive(Debug, Clone, Default)]
pub struct AudioContextOptions {
    pub sample_rate: Option<f32>,
    pub latency_hint: LatencyHint,
}

/// Latency hint
#[derive(Debug, Clone, Copy, Default)]
pub enum LatencyHint {
    #[default]
    Interactive,
    Balanced,
    Playback,
}

/// Audio node base
#[derive(Debug, Clone)]
pub struct AudioNode {
    pub id: u32,
    pub channel_count: u32,
    pub channel_count_mode: ChannelCountMode,
    pub channel_interpretation: ChannelInterpretation,
}

#[derive(Debug, Clone, Copy, Default)]
pub enum ChannelCountMode {
    Max,
    #[default]
    ClampedMax,
    Explicit,
}

#[derive(Debug, Clone, Copy, Default)]
pub enum ChannelInterpretation {
    #[default]
    Speakers,
    Discrete,
}

/// Oscillator node
#[derive(Debug)]
pub struct OscillatorNode {
    pub id: u32,
    pub oscillator_type: OscillatorType,
    pub frequency: AudioParam,
    pub detune: AudioParam,
    started: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub enum OscillatorType {
    #[default]
    Sine,
    Square,
    Sawtooth,
    Triangle,
    Custom,
}

impl OscillatorNode {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            oscillator_type: OscillatorType::Sine,
            frequency: AudioParam::new(440.0),
            detune: AudioParam::new(0.0),
            started: false,
        }
    }
    
    pub fn start(&mut self, _when: f64) { self.started = true; }
    pub fn stop(&mut self, _when: f64) { self.started = false; }
}

/// Gain node
#[derive(Debug)]
pub struct GainNode {
    pub id: u32,
    pub gain: AudioParam,
}

impl GainNode {
    pub fn new(id: u32) -> Self {
        Self { id, gain: AudioParam::new(1.0) }
    }
}

/// Analyser node
#[derive(Debug)]
pub struct AnalyserNode {
    pub id: u32,
    pub fft_size: usize,
    pub min_decibels: f64,
    pub max_decibels: f64,
    pub smoothing_time_constant: f64,
}

impl AnalyserNode {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            fft_size: 2048,
            min_decibels: -100.0,
            max_decibels: -30.0,
            smoothing_time_constant: 0.8,
        }
    }
    
    pub fn frequency_bin_count(&self) -> usize { self.fft_size / 2 }
    pub fn get_byte_frequency_data(&self, _array: &mut [u8]) {}
    pub fn get_float_frequency_data(&self, _array: &mut [f32]) {}
    pub fn get_byte_time_domain_data(&self, _array: &mut [u8]) {}
    pub fn get_float_time_domain_data(&self, _array: &mut [f32]) {}
}

/// Audio buffer source node
#[derive(Debug)]
pub struct AudioBufferSourceNode {
    pub id: u32,
    pub buffer: Option<AudioBuffer>,
    pub playback_rate: AudioParam,
    pub loop_: bool,
    pub loop_start: f64,
    pub loop_end: f64,
}

impl AudioBufferSourceNode {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            buffer: None,
            playback_rate: AudioParam::new(1.0),
            loop_: false,
            loop_start: 0.0,
            loop_end: 0.0,
        }
    }
    
    pub fn start(&mut self, _when: f64, _offset: f64, _duration: Option<f64>) {}
    pub fn stop(&mut self, _when: f64) {}
}

/// Audio buffer
#[derive(Debug, Clone)]
pub struct AudioBuffer {
    pub number_of_channels: u32,
    pub length: u32,
    pub sample_rate: f32,
    data: Vec<Vec<f32>>,
}

impl AudioBuffer {
    pub fn new(channels: u32, length: u32, sample_rate: f32) -> Self {
        Self {
            number_of_channels: channels,
            length,
            sample_rate,
            data: vec![vec![0.0; length as usize]; channels as usize],
        }
    }
    
    pub fn duration(&self) -> f64 { self.length as f64 / self.sample_rate as f64 }
    pub fn get_channel_data(&self, channel: u32) -> Option<&[f32]> {
        self.data.get(channel as usize).map(|v| v.as_slice())
    }
    pub fn copy_to_channel(&mut self, source: &[f32], channel: u32) {
        if let Some(ch) = self.data.get_mut(channel as usize) {
            let len = ch.len().min(source.len());
            ch[..len].copy_from_slice(&source[..len]);
        }
    }
}

/// Delay node
#[derive(Debug)]
pub struct DelayNode {
    pub id: u32,
    pub delay_time: AudioParam,
}

impl DelayNode {
    pub fn new(id: u32, max_delay: f64) -> Self {
        Self { id, delay_time: AudioParam::new(max_delay.min(1.0)) }
    }
}

/// Biquad filter node
#[derive(Debug)]
pub struct BiquadFilterNode {
    pub id: u32,
    pub filter_type: BiquadFilterType,
    pub frequency: AudioParam,
    pub q: AudioParam,
    pub gain: AudioParam,
}

#[derive(Debug, Clone, Copy, Default)]
pub enum BiquadFilterType {
    #[default]
    Lowpass,
    Highpass,
    Bandpass,
    Lowshelf,
    Highshelf,
    Peaking,
    Notch,
    Allpass,
}

impl BiquadFilterNode {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            filter_type: BiquadFilterType::Lowpass,
            frequency: AudioParam::new(350.0),
            q: AudioParam::new(1.0),
            gain: AudioParam::new(0.0),
        }
    }
}

/// Audio parameter
#[derive(Debug, Clone)]
pub struct AudioParam {
    pub value: f64,
    pub default_value: f64,
    pub min_value: f64,
    pub max_value: f64,
}

impl AudioParam {
    pub fn new(default: f64) -> Self {
        Self {
            value: default,
            default_value: default,
            min_value: f64::MIN,
            max_value: f64::MAX,
        }
    }
    
    pub fn set_value_at_time(&mut self, value: f64, _start_time: f64) {
        self.value = value;
    }
    
    pub fn linear_ramp_to_value_at_time(&mut self, value: f64, _end_time: f64) {
        self.value = value;
    }
    
    pub fn exponential_ramp_to_value_at_time(&mut self, value: f64, _end_time: f64) {
        self.value = value;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_audio_context() {
        let mut ctx = AudioContext::new();
        assert_eq!(ctx.state, AudioContextState::Suspended);
        
        ctx.resume().unwrap();
        assert_eq!(ctx.state, AudioContextState::Running);
    }
    
    #[test]
    fn test_oscillator() {
        let mut ctx = AudioContext::new();
        let osc = ctx.create_oscillator();
        
        assert_eq!(osc.frequency.value, 440.0);
    }
}
