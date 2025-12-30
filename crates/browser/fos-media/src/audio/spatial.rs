//! Spatial Audio
//!
//! PannerNode and AudioListener for 3D audio.

use super::context::AudioParam;

/// Audio listener (represents the listener in 3D space)
#[derive(Debug, Clone)]
pub struct AudioListener {
    pub position_x: AudioParam,
    pub position_y: AudioParam,
    pub position_z: AudioParam,
    pub forward_x: AudioParam,
    pub forward_y: AudioParam,
    pub forward_z: AudioParam,
    pub up_x: AudioParam,
    pub up_y: AudioParam,
    pub up_z: AudioParam,
}

impl Default for AudioListener {
    fn default() -> Self {
        Self {
            position_x: AudioParam::new(0.0),
            position_y: AudioParam::new(0.0),
            position_z: AudioParam::new(0.0),
            forward_x: AudioParam::new(0.0),
            forward_y: AudioParam::new(0.0),
            forward_z: AudioParam::new(-1.0),
            up_x: AudioParam::new(0.0),
            up_y: AudioParam::new(1.0),
            up_z: AudioParam::new(0.0),
        }
    }
}

/// Panner node for 3D spatial audio
#[derive(Debug)]
pub struct PannerNode {
    pub id: u32,
    pub panning_model: PanningModel,
    pub distance_model: DistanceModel,
    pub position_x: AudioParam,
    pub position_y: AudioParam,
    pub position_z: AudioParam,
    pub orientation_x: AudioParam,
    pub orientation_y: AudioParam,
    pub orientation_z: AudioParam,
    pub ref_distance: f64,
    pub max_distance: f64,
    pub rolloff_factor: f64,
    pub cone_inner_angle: f64,
    pub cone_outer_angle: f64,
    pub cone_outer_gain: f64,
}

/// Panning model
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PanningModel {
    EqualPower,
    #[default]
    HRTF,
}

/// Distance model
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DistanceModel {
    Linear,
    #[default]
    Inverse,
    Exponential,
}

impl PannerNode {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            panning_model: PanningModel::HRTF,
            distance_model: DistanceModel::Inverse,
            position_x: AudioParam::new(0.0),
            position_y: AudioParam::new(0.0),
            position_z: AudioParam::new(0.0),
            orientation_x: AudioParam::new(1.0),
            orientation_y: AudioParam::new(0.0),
            orientation_z: AudioParam::new(0.0),
            ref_distance: 1.0,
            max_distance: 10000.0,
            rolloff_factor: 1.0,
            cone_inner_angle: 360.0,
            cone_outer_angle: 360.0,
            cone_outer_gain: 0.0,
        }
    }
    
    /// Set position
    pub fn set_position(&mut self, x: f64, y: f64, z: f64) {
        self.position_x.value = x;
        self.position_y.value = y;
        self.position_z.value = z;
    }
    
    /// Set orientation
    pub fn set_orientation(&mut self, x: f64, y: f64, z: f64) {
        self.orientation_x.value = x;
        self.orientation_y.value = y;
        self.orientation_z.value = z;
    }
}

/// Stereo panner node (simpler alternative)
#[derive(Debug)]
pub struct StereoPannerNode {
    pub id: u32,
    pub pan: AudioParam,
}

impl StereoPannerNode {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            pan: AudioParam::new(0.0), // -1.0 = left, 0 = center, 1.0 = right
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_panner_node() {
        let mut panner = PannerNode::new(1);
        panner.set_position(10.0, 0.0, -5.0);
        
        assert_eq!(panner.position_x.value, 10.0);
        assert_eq!(panner.position_z.value, -5.0);
    }
    
    #[test]
    fn test_stereo_panner() {
        let panner = StereoPannerNode::new(1);
        assert_eq!(panner.pan.value, 0.0);
    }
}
