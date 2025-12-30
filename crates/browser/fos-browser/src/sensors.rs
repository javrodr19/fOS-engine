//! Device Sensors API
//!
//! Accelerometer, gyroscope, magnetometer, and ambient light.

/// Sensor reading with timestamp
#[derive(Debug, Clone, Copy)]
pub struct SensorReading<T: Clone> {
    pub value: T,
    pub timestamp: f64,
}

/// 3D vector for motion sensors
#[derive(Debug, Clone, Copy, Default)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// Accelerometer sensor
#[derive(Debug)]
pub struct Accelerometer {
    reading: Option<SensorReading<Vec3>>,
    frequency: f64,
    active: bool,
}

impl Default for Accelerometer {
    fn default() -> Self {
        Self::new()
    }
}

impl Accelerometer {
    pub fn new() -> Self {
        Self {
            reading: None,
            frequency: 60.0,
            active: false,
        }
    }
    
    pub fn start(&mut self) {
        self.active = true;
    }
    
    pub fn stop(&mut self) {
        self.active = false;
    }
    
    pub fn is_active(&self) -> bool {
        self.active
    }
    
    pub fn reading(&self) -> Option<Vec3> {
        self.reading.map(|r| r.value)
    }
    
    pub fn update(&mut self, x: f64, y: f64, z: f64, timestamp: f64) {
        self.reading = Some(SensorReading {
            value: Vec3 { x, y, z },
            timestamp,
        });
    }
}

/// Gyroscope sensor
#[derive(Debug)]
pub struct Gyroscope {
    reading: Option<SensorReading<Vec3>>,
    frequency: f64,
    active: bool,
}

impl Default for Gyroscope {
    fn default() -> Self {
        Self::new()
    }
}

impl Gyroscope {
    pub fn new() -> Self {
        Self {
            reading: None,
            frequency: 60.0,
            active: false,
        }
    }
    
    pub fn start(&mut self) {
        self.active = true;
    }
    
    pub fn stop(&mut self) {
        self.active = false;
    }
    
    pub fn reading(&self) -> Option<Vec3> {
        self.reading.map(|r| r.value)
    }
    
    pub fn update(&mut self, x: f64, y: f64, z: f64, timestamp: f64) {
        self.reading = Some(SensorReading {
            value: Vec3 { x, y, z },
            timestamp,
        });
    }
}

/// Magnetometer sensor
#[derive(Debug)]
pub struct Magnetometer {
    reading: Option<SensorReading<Vec3>>,
    active: bool,
}

impl Default for Magnetometer {
    fn default() -> Self {
        Self::new()
    }
}

impl Magnetometer {
    pub fn new() -> Self {
        Self {
            reading: None,
            active: false,
        }
    }
    
    pub fn start(&mut self) {
        self.active = true;
    }
    
    pub fn stop(&mut self) {
        self.active = false;
    }
    
    pub fn reading(&self) -> Option<Vec3> {
        self.reading.map(|r| r.value)
    }
}

/// Ambient light sensor
#[derive(Debug)]
pub struct AmbientLightSensor {
    illuminance: Option<f64>,
    active: bool,
}

impl Default for AmbientLightSensor {
    fn default() -> Self {
        Self::new()
    }
}

impl AmbientLightSensor {
    pub fn new() -> Self {
        Self {
            illuminance: None,
            active: false,
        }
    }
    
    pub fn start(&mut self) {
        self.active = true;
    }
    
    pub fn stop(&mut self) {
        self.active = false;
    }
    
    pub fn illuminance(&self) -> Option<f64> {
        self.illuminance
    }
    
    pub fn update(&mut self, lux: f64) {
        self.illuminance = Some(lux);
    }
}

/// Device orientation
#[derive(Debug, Clone, Copy, Default)]
pub struct DeviceOrientation {
    pub alpha: f64, // Z axis
    pub beta: f64,  // X axis
    pub gamma: f64, // Y axis
    pub absolute: bool,
}

/// Device motion
#[derive(Debug, Clone, Copy, Default)]
pub struct DeviceMotion {
    pub acceleration: Vec3,
    pub acceleration_including_gravity: Vec3,
    pub rotation_rate: Vec3,
    pub interval: f64,
}

/// Sensors manager
#[derive(Debug, Default)]
pub struct SensorsManager {
    pub accelerometer: Accelerometer,
    pub gyroscope: Gyroscope,
    pub magnetometer: Magnetometer,
    pub ambient_light: AmbientLightSensor,
    device_orientation: Option<DeviceOrientation>,
    device_motion: Option<DeviceMotion>,
}

impl SensorsManager {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn device_orientation(&self) -> Option<DeviceOrientation> {
        self.device_orientation
    }
    
    pub fn device_motion(&self) -> Option<DeviceMotion> {
        self.device_motion
    }
    
    pub fn update_orientation(&mut self, orientation: DeviceOrientation) {
        self.device_orientation = Some(orientation);
    }
    
    pub fn update_motion(&mut self, motion: DeviceMotion) {
        self.device_motion = Some(motion);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_accelerometer() {
        let mut accel = Accelerometer::new();
        
        accel.start();
        assert!(accel.is_active());
        
        accel.update(0.0, 0.0, 9.8, 0.0);
        let reading = accel.reading().unwrap();
        assert!((reading.z - 9.8).abs() < 0.001);
    }
}
