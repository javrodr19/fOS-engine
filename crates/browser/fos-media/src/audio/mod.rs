//! Audio Module
//!
//! Web Audio API.

pub mod context;
pub mod spatial;
pub mod worklet;

pub use context::{
    AudioContext, AudioContextState, AudioContextOptions,
    AudioDestinationNode, OscillatorNode, OscillatorType,
    GainNode, AnalyserNode, AudioBufferSourceNode, AudioBuffer,
    DelayNode, BiquadFilterNode, BiquadFilterType, AudioParam,
};
pub use spatial::{PannerNode, StereoPannerNode, AudioListener};
pub use worklet::{AudioWorkletNode, AudioWorkletProcessor};
