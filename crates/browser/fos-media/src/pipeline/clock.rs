//! Media Clock
//!
//! Precise timing for audio/video synchronization.

use std::time::{Duration, Instant};

/// Media clock for A/V sync
#[derive(Debug)]
pub struct MediaClock {
    start_time: Option<Instant>,
    pause_time: Option<Instant>,
    base_position: Duration,
    playback_rate: f64,
}

impl MediaClock {
    pub fn new() -> Self {
        Self { start_time: None, pause_time: None, base_position: Duration::ZERO, playback_rate: 1.0 }
    }
    
    pub fn start(&mut self) {
        if let Some(pause_instant) = self.pause_time.take() {
            if let Some(ref mut start) = self.start_time {
                *start += pause_instant.elapsed();
            }
        } else {
            self.start_time = Some(Instant::now());
        }
    }
    
    pub fn pause(&mut self) { self.pause_time = Some(Instant::now()); }
    
    pub fn seek(&mut self, position: Duration) {
        self.base_position = position;
        self.start_time = Some(Instant::now());
        self.pause_time = None;
    }
    
    pub fn position(&self) -> Duration {
        match (self.start_time, self.pause_time) {
            (Some(start), None) => {
                let elapsed = start.elapsed();
                self.base_position + Duration::from_secs_f64(elapsed.as_secs_f64() * self.playback_rate)
            }
            (Some(start), Some(pause)) => {
                let elapsed = pause.duration_since(start);
                self.base_position + Duration::from_secs_f64(elapsed.as_secs_f64() * self.playback_rate)
            }
            _ => self.base_position,
        }
    }
    
    pub fn set_playback_rate(&mut self, rate: f64) {
        self.base_position = self.position();
        self.start_time = Some(Instant::now());
        self.playback_rate = rate.clamp(0.25, 4.0);
    }
    
    pub fn playback_rate(&self) -> f64 { self.playback_rate }
    pub fn is_running(&self) -> bool { self.start_time.is_some() && self.pause_time.is_none() }
}

impl Default for MediaClock { fn default() -> Self { Self::new() } }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_clock() { let c = MediaClock::new(); assert_eq!(c.position(), Duration::ZERO); assert!(!c.is_running()); }
}
