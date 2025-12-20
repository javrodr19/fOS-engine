//! Audio Worklets
//!
//! Custom audio processing in worker threads.

use std::collections::HashMap;

/// Audio worklet node
#[derive(Debug)]
pub struct AudioWorkletNode {
    pub id: u32,
    pub name: String,
    pub number_of_inputs: u32,
    pub number_of_outputs: u32,
    pub channel_count: u32,
    pub parameters: HashMap<String, AudioWorkletParam>,
}

/// Audio worklet parameter
#[derive(Debug, Clone)]
pub struct AudioWorkletParam {
    pub name: String,
    pub value: f64,
    pub default_value: f64,
    pub min_value: f64,
    pub max_value: f64,
    pub automation_rate: AutomationRate,
}

/// Automation rate
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AutomationRate {
    #[default]
    ARate,
    KRate,
}

impl AudioWorkletNode {
    pub fn new(id: u32, name: &str) -> Self {
        Self {
            id,
            name: name.to_string(),
            number_of_inputs: 1,
            number_of_outputs: 1,
            channel_count: 2,
            parameters: HashMap::new(),
        }
    }
    
    /// Add parameter
    pub fn add_parameter(&mut self, param: AudioWorkletParam) {
        self.parameters.insert(param.name.clone(), param);
    }
    
    /// Get parameter
    pub fn get_parameter(&self, name: &str) -> Option<&AudioWorkletParam> {
        self.parameters.get(name)
    }
}

/// Audio worklet processor (interface for custom processing)
pub trait AudioWorkletProcessor {
    fn process(
        &mut self,
        inputs: &[Vec<Vec<f32>>],
        outputs: &mut [Vec<Vec<f32>>],
        parameters: &HashMap<String, Vec<f32>>,
    ) -> bool;
}

/// Audio worklet global scope
#[derive(Debug, Default)]
pub struct AudioWorkletGlobalScope {
    registered_processors: Vec<String>,
}

impl AudioWorkletGlobalScope {
    pub fn new() -> Self { Self::default() }
    
    /// Register processor
    pub fn register_processor(&mut self, name: &str) {
        self.registered_processors.push(name.to_string());
    }
    
    /// Check if processor is registered
    pub fn has_processor(&self, name: &str) -> bool {
        self.registered_processors.contains(&name.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_worklet_node() {
        let mut node = AudioWorkletNode::new(1, "custom-processor");
        node.add_parameter(AudioWorkletParam {
            name: "gain".into(),
            value: 1.0,
            default_value: 1.0,
            min_value: 0.0,
            max_value: 1.0,
            automation_rate: AutomationRate::ARate,
        });
        
        assert!(node.get_parameter("gain").is_some());
    }
}
