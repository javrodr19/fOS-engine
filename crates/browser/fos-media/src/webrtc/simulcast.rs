//! Simulcast
//!
//! Simulcast support for sending multiple video qualities.

use std::collections::HashMap;

/// Simulcast RID (Restriction Identifier)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Rid(pub String);

impl Rid {
    pub fn new(id: impl Into<String>) -> Self { Self(id.into()) }
    pub fn as_str(&self) -> &str { &self.0 }
}

/// Simulcast direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimulcastDirection { Send, Recv }

/// Simulcast layer configuration
#[derive(Debug, Clone)]
pub struct SimulcastLayer {
    pub rid: Rid,
    pub direction: SimulcastDirection,
    pub paused: bool,
    pub active: bool,
    pub scale_resolution_down_by: f32,
    pub max_bitrate: Option<u64>,
    pub max_framerate: Option<f32>,
}

impl SimulcastLayer {
    pub fn new(rid: impl Into<String>, direction: SimulcastDirection) -> Self {
        Self {
            rid: Rid::new(rid),
            direction,
            paused: false,
            active: true,
            scale_resolution_down_by: 1.0,
            max_bitrate: None,
            max_framerate: None,
        }
    }
    
    pub fn with_scale(mut self, scale: f32) -> Self {
        self.scale_resolution_down_by = scale;
        self
    }
    
    pub fn with_max_bitrate(mut self, bitrate: u64) -> Self {
        self.max_bitrate = Some(bitrate);
        self
    }
}

/// Simulcast stream configuration
#[derive(Debug, Clone)]
pub struct SimulcastConfig {
    pub layers: Vec<SimulcastLayer>,
}

impl SimulcastConfig {
    pub fn new() -> Self { Self { layers: Vec::new() } }
    
    /// Create standard 3-layer simulcast (low, medium, high)
    pub fn three_layer() -> Self {
        Self {
            layers: vec![
                SimulcastLayer::new("low", SimulcastDirection::Send)
                    .with_scale(4.0)
                    .with_max_bitrate(150_000),
                SimulcastLayer::new("mid", SimulcastDirection::Send)
                    .with_scale(2.0)
                    .with_max_bitrate(500_000),
                SimulcastLayer::new("high", SimulcastDirection::Send)
                    .with_scale(1.0)
                    .with_max_bitrate(2_500_000),
            ],
        }
    }
    
    /// Create 2-layer simulcast
    pub fn two_layer() -> Self {
        Self {
            layers: vec![
                SimulcastLayer::new("low", SimulcastDirection::Send)
                    .with_scale(2.0)
                    .with_max_bitrate(250_000),
                SimulcastLayer::new("high", SimulcastDirection::Send)
                    .with_scale(1.0)
                    .with_max_bitrate(2_500_000),
            ],
        }
    }
    
    pub fn add_layer(&mut self, layer: SimulcastLayer) {
        self.layers.push(layer);
    }
    
    pub fn get_layer(&self, rid: &str) -> Option<&SimulcastLayer> {
        self.layers.iter().find(|l| l.rid.as_str() == rid)
    }
    
    pub fn get_layer_mut(&mut self, rid: &str) -> Option<&mut SimulcastLayer> {
        self.layers.iter_mut().find(|l| l.rid.as_str() == rid)
    }
    
    /// Generate SDP a=simulcast attribute
    pub fn to_sdp_attr(&self) -> String {
        let send_rids: Vec<_> = self.layers.iter()
            .filter(|l| l.direction == SimulcastDirection::Send && l.active)
            .map(|l| l.rid.as_str())
            .collect();
        
        let recv_rids: Vec<_> = self.layers.iter()
            .filter(|l| l.direction == SimulcastDirection::Recv && l.active)
            .map(|l| l.rid.as_str())
            .collect();
        
        let mut parts = Vec::new();
        if !send_rids.is_empty() {
            parts.push(format!("send {}", send_rids.join(";")));
        }
        if !recv_rids.is_empty() {
            parts.push(format!("recv {}", recv_rids.join(";")));
        }
        
        parts.join(" ")
    }
    
    /// Generate RID attributes for SDP
    pub fn to_rid_attrs(&self) -> Vec<String> {
        self.layers.iter().map(|l| {
            let dir = match l.direction {
                SimulcastDirection::Send => "send",
                SimulcastDirection::Recv => "recv",
            };
            format!("{} {}", l.rid.as_str(), dir)
        }).collect()
    }
    
    /// Parse simulcast attribute from SDP
    pub fn from_sdp_attr(attr: &str) -> Option<Self> {
        let mut config = Self::new();
        
        let parts: Vec<&str> = attr.split_whitespace().collect();
        let mut i = 0;
        
        while i < parts.len() {
            let direction = match parts[i] {
                "send" => SimulcastDirection::Send,
                "recv" => SimulcastDirection::Recv,
                _ => { i += 1; continue; }
            };
            i += 1;
            
            if i >= parts.len() { break; }
            let rids = parts[i];
            
            for rid in rids.split(';') {
                let rid = rid.trim_start_matches('~'); // '~' means paused
                if !rid.is_empty() {
                    let mut layer = SimulcastLayer::new(rid, direction);
                    layer.paused = rids.starts_with('~');
                    config.add_layer(layer);
                }
            }
            i += 1;
        }
        
        if config.layers.is_empty() { None } else { Some(config) }
    }
}

impl Default for SimulcastConfig { fn default() -> Self { Self::new() } }

/// Simulcast stream selector (for receivers)
#[derive(Debug)]
pub struct SimulcastSelector {
    selected_rid: Option<Rid>,
    auto_select: bool,
    quality_preference: QualityPreference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityPreference { Low, Medium, High, Auto }

impl SimulcastSelector {
    pub fn new() -> Self {
        Self { selected_rid: None, auto_select: true, quality_preference: QualityPreference::Auto }
    }
    
    pub fn select(&mut self, rid: impl Into<String>) {
        self.selected_rid = Some(Rid::new(rid));
        self.auto_select = false;
    }
    
    pub fn set_preference(&mut self, pref: QualityPreference) {
        self.quality_preference = pref;
        if pref == QualityPreference::Auto { self.auto_select = true; }
    }
    
    pub fn auto_select_layer(&mut self, layers: &[SimulcastLayer], bandwidth: u64) -> Option<&SimulcastLayer> {
        if !self.auto_select {
            return self.selected_rid.as_ref().and_then(|rid| layers.iter().find(|l| l.rid == *rid));
        }
        
        // Select highest quality that fits bandwidth
        layers.iter()
            .filter(|l| l.active && !l.paused && l.direction == SimulcastDirection::Send)
            .filter(|l| l.max_bitrate.map(|b| b <= bandwidth).unwrap_or(true))
            .max_by_key(|l| l.max_bitrate.unwrap_or(0))
    }
    
    pub fn selected(&self) -> Option<&Rid> { self.selected_rid.as_ref() }
}

impl Default for SimulcastSelector { fn default() -> Self { Self::new() } }

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_three_layer() {
        let config = SimulcastConfig::three_layer();
        assert_eq!(config.layers.len(), 3);
        assert!(config.get_layer("low").is_some());
        assert!(config.get_layer("mid").is_some());
        assert!(config.get_layer("high").is_some());
    }
    
    #[test]
    fn test_sdp_attr() {
        let config = SimulcastConfig::three_layer();
        let attr = config.to_sdp_attr();
        assert!(attr.contains("send"));
        assert!(attr.contains("low"));
    }
    
    #[test]
    fn test_parse_sdp() {
        let config = SimulcastConfig::from_sdp_attr("send low;mid;high").unwrap();
        assert_eq!(config.layers.len(), 3);
    }
}
