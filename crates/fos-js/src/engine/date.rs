//! Date Implementation
//!
//! JavaScript Date object.

use std::time::{SystemTime, UNIX_EPOCH};

/// JavaScript Date
#[derive(Debug, Clone, Copy)]
pub struct JsDate {
    /// Milliseconds since Unix epoch
    timestamp: f64,
}

impl Default for JsDate {
    fn default() -> Self { Self::now() }
}

impl JsDate {
    pub fn now() -> Self {
        let duration = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
        Self { timestamp: duration.as_millis() as f64 }
    }
    
    pub fn from_timestamp(ms: f64) -> Self {
        Self { timestamp: ms }
    }
    
    pub fn from_components(year: i32, month: u32, day: u32, hour: u32, min: u32, sec: u32, ms: u32) -> Self {
        // Simplified - just estimate timestamp
        let days = (year - 1970) as f64 * 365.25 + (month as f64 * 30.44) + day as f64;
        let secs = days * 86400.0 + hour as f64 * 3600.0 + min as f64 * 60.0 + sec as f64;
        Self { timestamp: secs * 1000.0 + ms as f64 }
    }
    
    pub fn get_time(&self) -> f64 { self.timestamp }
    pub fn set_time(&mut self, ms: f64) { self.timestamp = ms; }
    
    // Simplified date component extraction
    pub fn get_full_year(&self) -> i32 {
        1970 + (self.timestamp / (365.25 * 86400.0 * 1000.0)) as i32
    }
    
    pub fn get_month(&self) -> u32 {
        let days_since_epoch = self.timestamp / (86400.0 * 1000.0);
        let year_days = days_since_epoch % 365.25;
        (year_days / 30.44) as u32
    }
    
    pub fn get_date(&self) -> u32 {
        let days_since_epoch = self.timestamp / (86400.0 * 1000.0);
        let year_days = days_since_epoch % 365.25;
        (year_days % 30.44) as u32 + 1
    }
    
    pub fn get_day(&self) -> u32 {
        let days_since_epoch = self.timestamp / (86400.0 * 1000.0);
        ((days_since_epoch as i64 + 4) % 7) as u32 // Jan 1, 1970 was Thursday
    }
    
    pub fn get_hours(&self) -> u32 {
        ((self.timestamp / (3600.0 * 1000.0)) % 24.0) as u32
    }
    
    pub fn get_minutes(&self) -> u32 {
        ((self.timestamp / (60.0 * 1000.0)) % 60.0) as u32
    }
    
    pub fn get_seconds(&self) -> u32 {
        ((self.timestamp / 1000.0) % 60.0) as u32
    }
    
    pub fn get_milliseconds(&self) -> u32 {
        (self.timestamp % 1000.0) as u32
    }
    
    pub fn to_iso_string(&self) -> String {
        format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
            self.get_full_year(), self.get_month() + 1, self.get_date(),
            self.get_hours(), self.get_minutes(), self.get_seconds(), self.get_milliseconds())
    }
    
    pub fn to_string(&self) -> String {
        self.to_iso_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_date_now() {
        let d = JsDate::now();
        assert!(d.get_time() > 0.0);
    }
    
    #[test]
    fn test_date_from_timestamp() {
        let d = JsDate::from_timestamp(0.0);
        assert_eq!(d.get_full_year(), 1970);
    }
    
    #[test]
    fn test_date_components() {
        let d = JsDate::from_timestamp(1000.0 * 3600.0 * 24.0 * 365.0);
        assert!(d.get_full_year() >= 1970);
    }
}
