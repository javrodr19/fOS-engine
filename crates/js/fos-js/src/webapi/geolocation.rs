//! Geolocation API
//!
//! Device location access with platform backend support.

use std::sync::{Arc, Mutex};

/// Platform backend for geolocation
pub trait GeolocationBackend: Send + Sync {
    /// Request current position from platform
    fn request_position(&self, high_accuracy: bool) -> Result<Position, PositionError>;
    
    /// Start watching position changes
    fn start_watch(&self, watch_id: u32, high_accuracy: bool);
    
    /// Stop watching position
    fn stop_watch(&self, watch_id: u32);
}

/// Default backend that uses simulated positions (for testing/non-platform environments)
#[derive(Debug, Default)]
pub struct SimulatedBackend {
    pub position: Mutex<Option<Position>>,
}

impl SimulatedBackend {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Set a simulated position for testing
    pub fn set_position(&self, pos: Position) {
        *self.position.lock().unwrap() = Some(pos);
    }
}

impl GeolocationBackend for SimulatedBackend {
    fn request_position(&self, _high_accuracy: bool) -> Result<Position, PositionError> {
        if let Some(pos) = self.position.lock().unwrap().clone() {
            Ok(pos)
        } else {
            Err(PositionError {
                code: PositionErrorCode::PositionUnavailable,
                message: "No simulated position set".into(),
            })
        }
    }
    
    fn start_watch(&self, _watch_id: u32, _high_accuracy: bool) {
        // Simulated backend doesn't actively watch
    }
    
    fn stop_watch(&self, _watch_id: u32) {
        // No-op for simulation
    }
}

/// Geolocation interface
pub struct Geolocation {
    watching: Vec<u32>,
    backend: Arc<dyn GeolocationBackend>,
    next_watch_id: u32,
}

impl Default for Geolocation {
    fn default() -> Self {
        Self::with_backend(Arc::new(SimulatedBackend::default()))
    }
}

/// Position data
#[derive(Debug, Clone)]
pub struct Position {
    pub coords: Coordinates,
    pub timestamp: u64,
}

/// Geographic coordinates
#[derive(Debug, Clone, Copy)]
pub struct Coordinates {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: Option<f64>,
    pub accuracy: f64,
    pub altitude_accuracy: Option<f64>,
    pub heading: Option<f64>,
    pub speed: Option<f64>,
}

/// Position options
#[derive(Debug, Clone, Default)]
pub struct PositionOptions {
    pub enable_high_accuracy: bool,
    pub timeout: Option<u32>,
    pub maximum_age: Option<u32>,
}

/// Position error
#[derive(Debug, Clone)]
pub struct PositionError {
    pub code: PositionErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionErrorCode {
    PermissionDenied = 1,
    PositionUnavailable = 2,
    Timeout = 3,
}

impl Geolocation {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Create with a custom backend
    pub fn with_backend(backend: Arc<dyn GeolocationBackend>) -> Self {
        Self {
            watching: Vec::new(),
            backend,
            next_watch_id: 1,
        }
    }
    
    /// Get current position
    pub fn get_current_position(&self, options: PositionOptions) -> Result<Position, PositionError> {
        self.backend.request_position(options.enable_high_accuracy)
    }
    
    /// Watch position changes
    pub fn watch_position(&mut self, options: PositionOptions) -> u32 {
        let id = self.next_watch_id;
        self.next_watch_id += 1;
        self.watching.push(id);
        self.backend.start_watch(id, options.enable_high_accuracy);
        id
    }
    
    /// Stop watching
    pub fn clear_watch(&mut self, watch_id: u32) {
        if self.watching.contains(&watch_id) {
            self.backend.stop_watch(watch_id);
            self.watching.retain(|&id| id != watch_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_geolocation() {
        let mut geo = Geolocation::new();
        let watch_id = geo.watch_position(PositionOptions::default());
        
        assert!(watch_id > 0);
        geo.clear_watch(watch_id);
    }
    
    #[test]
    fn test_simulated_position() {
        let backend = Arc::new(SimulatedBackend::new());
        backend.set_position(Position {
            coords: Coordinates {
                latitude: 37.7749,
                longitude: -122.4194,
                altitude: Some(10.0),
                accuracy: 5.0,
                altitude_accuracy: Some(3.0),
                heading: None,
                speed: None,
            },
            timestamp: 1234567890,
        });
        
        let geo = Geolocation::with_backend(backend);
        let pos = geo.get_current_position(PositionOptions::default()).unwrap();
        
        assert!((pos.coords.latitude - 37.7749).abs() < 0.001);
    }
}
