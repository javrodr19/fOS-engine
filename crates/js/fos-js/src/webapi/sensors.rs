//! Device Orientation and Sensor APIs
//!
//! Motion and orientation sensors.

/// Device Orientation Event
#[derive(Debug, Clone, Copy, Default)]
pub struct DeviceOrientationEvent {
    pub alpha: Option<f64>, // Z-axis rotation (0-360)
    pub beta: Option<f64>,  // X-axis rotation (-180 to 180)
    pub gamma: Option<f64>, // Y-axis rotation (-90 to 90)
    pub absolute: bool,
}

/// Device Motion Event
#[derive(Debug, Clone, Copy, Default)]
pub struct DeviceMotionEvent {
    pub acceleration: Option<DeviceAcceleration>,
    pub acceleration_including_gravity: Option<DeviceAcceleration>,
    pub rotation_rate: Option<DeviceRotationRate>,
    pub interval: f64,
}

/// Device Acceleration
#[derive(Debug, Clone, Copy, Default)]
pub struct DeviceAcceleration {
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub z: Option<f64>,
}

/// Device Rotation Rate
#[derive(Debug, Clone, Copy, Default)]
pub struct DeviceRotationRate {
    pub alpha: Option<f64>,
    pub beta: Option<f64>,
    pub gamma: Option<f64>,
}

/// Sensor base trait
pub trait Sensor {
    fn start(&mut self);
    fn stop(&mut self);
    fn is_activated(&self) -> bool;
}

/// Accelerometer sensor
#[derive(Debug, Default)]
pub struct Accelerometer {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    activated: bool,
    timestamp: f64,
}

impl Accelerometer {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn timestamp(&self) -> f64 {
        self.timestamp
    }
}

impl Sensor for Accelerometer {
    fn start(&mut self) {
        self.activated = true;
    }
    
    fn stop(&mut self) {
        self.activated = false;
    }
    
    fn is_activated(&self) -> bool {
        self.activated
    }
}

/// Gyroscope sensor
#[derive(Debug, Default)]
pub struct Gyroscope {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    activated: bool,
}

impl Gyroscope {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Sensor for Gyroscope {
    fn start(&mut self) {
        self.activated = true;
    }
    
    fn stop(&mut self) {
        self.activated = false;
    }
    
    fn is_activated(&self) -> bool {
        self.activated
    }
}

/// Magnetometer sensor
#[derive(Debug, Default)]
pub struct Magnetometer {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    activated: bool,
}

impl Magnetometer {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Sensor for Magnetometer {
    fn start(&mut self) {
        self.activated = true;
    }
    
    fn stop(&mut self) {
        self.activated = false;
    }
    
    fn is_activated(&self) -> bool {
        self.activated
    }
}

/// Ambient Light Sensor
#[derive(Debug, Default)]
pub struct AmbientLightSensor {
    pub illuminance: f64,
    activated: bool,
}

impl AmbientLightSensor {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Sensor for AmbientLightSensor {
    fn start(&mut self) {
        self.activated = true;
    }
    
    fn stop(&mut self) {
        self.activated = false;
    }
    
    fn is_activated(&self) -> bool {
        self.activated
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_accelerometer() {
        let mut sensor = Accelerometer::new();
        assert!(!sensor.is_activated());
        
        sensor.start();
        assert!(sensor.is_activated());
        
        sensor.stop();
        assert!(!sensor.is_activated());
    }
    
    #[test]
    fn test_device_orientation() {
        let event = DeviceOrientationEvent {
            alpha: Some(45.0),
            beta: Some(10.0),
            gamma: Some(-5.0),
            absolute: true,
        };
        
        assert_eq!(event.alpha, Some(45.0));
    }
}
