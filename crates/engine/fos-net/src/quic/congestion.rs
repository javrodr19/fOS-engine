//! QUIC Congestion Control
//!
//! Cubic congestion control algorithm per RFC 9002.

use std::time::{Duration, Instant};

/// Minimum congestion window (2 * max datagram size)
const MIN_CWND: u64 = 2 * 1200;

/// Initial congestion window (10 * max datagram size per RFC 9002)
const INITIAL_CWND: u64 = 10 * 1200;

/// Maximum datagram size
const MAX_DATAGRAM_SIZE: u64 = 1200;

/// Cubic scaling constant
const CUBIC_C: f64 = 0.4;

/// Cubic beta (multiplicative decrease factor)
const CUBIC_BETA: f64 = 0.7;

/// Congestion controller using Cubic algorithm
#[derive(Debug)]
pub struct CubicController {
    /// Current congestion window
    cwnd: u64,
    /// Slow start threshold
    ssthresh: u64,
    /// Bytes in flight
    bytes_in_flight: u64,
    /// W_max: window size before last congestion event
    w_max: f64,
    /// Time of last congestion event
    epoch_start: Option<Instant>,
    /// K: time to reach W_max
    k: f64,
    /// W_cubic at epoch start
    origin_point: f64,
    /// Smoothed RTT
    smoothed_rtt: Duration,
    /// RTT variance
    rtt_var: Duration,
    /// Minimum RTT observed
    min_rtt: Duration,
    /// Congestion event count
    congestion_events: u64,
    /// Persistent congestion threshold
    persistent_congestion_threshold: Duration,
}

impl CubicController {
    /// Create a new Cubic congestion controller
    pub fn new() -> Self {
        Self {
            cwnd: INITIAL_CWND,
            ssthresh: u64::MAX,
            bytes_in_flight: 0,
            w_max: 0.0,
            epoch_start: None,
            k: 0.0,
            origin_point: 0.0,
            smoothed_rtt: Duration::from_millis(333), // Initial RTT estimate
            rtt_var: Duration::from_millis(166),
            min_rtt: Duration::MAX,
            congestion_events: 0,
            persistent_congestion_threshold: Duration::from_secs(0),
        }
    }
    
    /// Get current congestion window
    pub fn cwnd(&self) -> u64 {
        self.cwnd
    }
    
    /// Get bytes in flight
    pub fn bytes_in_flight(&self) -> u64 {
        self.bytes_in_flight
    }
    
    /// Check if we can send `bytes`
    pub fn can_send(&self, bytes: u64) -> bool {
        self.bytes_in_flight.saturating_add(bytes) <= self.cwnd
    }
    
    /// Record bytes sent
    pub fn record_sent(&mut self, bytes: u64) {
        self.bytes_in_flight = self.bytes_in_flight.saturating_add(bytes);
    }
    
    /// Record packet acknowledged
    pub fn on_ack(&mut self, bytes_acked: u64, now: Instant) {
        self.bytes_in_flight = self.bytes_in_flight.saturating_sub(bytes_acked);
        
        if self.cwnd < self.ssthresh {
            // Slow start
            self.cwnd = self.cwnd.saturating_add(bytes_acked);
        } else {
            // Congestion avoidance (Cubic)
            self.cubic_update(bytes_acked, now);
        }
    }
    
    /// Update using Cubic algorithm
    fn cubic_update(&mut self, bytes_acked: u64, now: Instant) {
        let epoch_start = self.epoch_start.get_or_insert(now);
        
        // Time since epoch start in seconds
        let t = now.duration_since(*epoch_start).as_secs_f64();
        
        // Compute W_cubic(t)
        let w_cubic = CUBIC_C * (t - self.k).powi(3) + self.origin_point;
        
        // Standard AIMD increase
        let w_est = self.origin_point + 
            (bytes_acked as f64 / MAX_DATAGRAM_SIZE as f64) * 
            (3.0 * CUBIC_BETA / (2.0 - CUBIC_BETA));
        
        // Use the larger of Cubic and standard
        let target = w_cubic.max(w_est);
        
        // Update cwnd
        if target > self.cwnd as f64 {
            let increase = ((target - self.cwnd as f64) / self.cwnd as f64 * bytes_acked as f64) as u64;
            self.cwnd = self.cwnd.saturating_add(increase.max(1));
        }
    }
    
    /// Handle packet loss
    pub fn on_loss(&mut self, now: Instant) {
        self.congestion_events += 1;
        
        // Save current window
        self.w_max = self.cwnd as f64;
        
        // Multiplicative decrease
        self.cwnd = ((self.cwnd as f64) * CUBIC_BETA) as u64;
        self.cwnd = self.cwnd.max(MIN_CWND);
        self.ssthresh = self.cwnd;
        
        // Reset epoch
        self.epoch_start = Some(now);
        self.origin_point = self.cwnd as f64;
        
        // Compute K
        self.k = ((self.w_max - self.cwnd as f64) / CUBIC_C).cbrt();
    }
    
    /// Handle persistent congestion
    pub fn on_persistent_congestion(&mut self) {
        self.cwnd = MIN_CWND;
        self.ssthresh = MIN_CWND;
        self.epoch_start = None;
        self.w_max = 0.0;
        self.k = 0.0;
    }
    
    /// Update RTT measurements
    pub fn update_rtt(&mut self, latest_rtt: Duration, ack_delay: Duration) {
        // Update min_rtt
        if latest_rtt < self.min_rtt {
            self.min_rtt = latest_rtt;
        }
        
        // Adjust for ack delay
        let adjusted_rtt = if latest_rtt > ack_delay {
            latest_rtt - ack_delay
        } else {
            latest_rtt
        };
        
        // First RTT sample
        if self.smoothed_rtt == Duration::from_millis(333) {
            self.smoothed_rtt = adjusted_rtt;
            self.rtt_var = adjusted_rtt / 2;
        } else {
            // RFC 6298 EWMA
            let diff = if self.smoothed_rtt > adjusted_rtt {
                self.smoothed_rtt - adjusted_rtt
            } else {
                adjusted_rtt - self.smoothed_rtt
            };
            
            self.rtt_var = self.rtt_var * 3 / 4 + diff / 4;
            self.smoothed_rtt = self.smoothed_rtt * 7 / 8 + adjusted_rtt / 8;
        }
        
        // Update persistent congestion threshold
        self.persistent_congestion_threshold = 
            self.smoothed_rtt * 3 + Duration::from_millis(25).max(self.rtt_var * 4);
    }
    
    /// Get smoothed RTT
    pub fn smoothed_rtt(&self) -> Duration {
        self.smoothed_rtt
    }
    
    /// Get minimum RTT
    pub fn min_rtt(&self) -> Duration {
        if self.min_rtt == Duration::MAX {
            self.smoothed_rtt
        } else {
            self.min_rtt
        }
    }
    
    /// Get PTO (Probe Timeout)
    pub fn pto(&self) -> Duration {
        self.smoothed_rtt + Duration::from_millis(25).max(self.rtt_var * 4)
    }
    
    /// Check if in slow start
    pub fn in_slow_start(&self) -> bool {
        self.cwnd < self.ssthresh
    }
    
    /// Get congestion event count
    pub fn congestion_events(&self) -> u64 {
        self.congestion_events
    }
    
    /// Reset congestion controller
    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

impl Default for CubicController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_initial_state() {
        let cc = CubicController::new();
        assert_eq!(cc.cwnd(), INITIAL_CWND);
        assert_eq!(cc.bytes_in_flight(), 0);
        assert!(cc.in_slow_start());
    }
    
    #[test]
    fn test_slow_start() {
        let mut cc = CubicController::new();
        let now = Instant::now();
        
        let initial = cc.cwnd();
        cc.record_sent(1200);
        cc.on_ack(1200, now);
        
        // Cwnd should increase in slow start
        assert!(cc.cwnd() > initial);
    }
    
    #[test]
    fn test_can_send() {
        let mut cc = CubicController::new();
        
        // Should be able to send up to cwnd
        assert!(cc.can_send(INITIAL_CWND));
        assert!(!cc.can_send(INITIAL_CWND + 1));
        
        cc.record_sent(1000);
        assert!(cc.can_send(INITIAL_CWND - 1000));
    }
    
    #[test]
    fn test_loss_handling() {
        let mut cc = CubicController::new();
        let now = Instant::now();
        
        // Force out of slow start
        cc.ssthresh = 1000;
        let before_loss = cc.cwnd();
        
        cc.on_loss(now);
        
        // Cwnd should decrease
        assert!(cc.cwnd() < before_loss);
        assert!(cc.cwnd() >= MIN_CWND);
    }
    
    #[test]
    fn test_rtt_update() {
        let mut cc = CubicController::new();
        
        let rtt = Duration::from_millis(50);
        cc.update_rtt(rtt, Duration::ZERO);
        
        assert_eq!(cc.smoothed_rtt(), rtt);
        assert_eq!(cc.min_rtt(), rtt);
    }
    
    #[test]
    fn test_persistent_congestion() {
        let mut cc = CubicController::new();
        cc.cwnd = 100_000;
        
        cc.on_persistent_congestion();
        
        assert_eq!(cc.cwnd(), MIN_CWND);
    }
}
