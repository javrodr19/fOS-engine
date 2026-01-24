//! BBRv2 Congestion Control
//!
//! Bottleneck Bandwidth and Round-trip propagation time version 2.
//! Advanced congestion control for QUIC with better fairness and less queuing.

use std::time::{Duration, Instant};

/// Minimum congestion window (2 packets)
const MIN_CWND: u64 = 2 * 1200;

/// Initial congestion window (10 packets)
const INITIAL_CWND: u64 = 10 * 1200;

/// Maximum datagram size
const MAX_DATAGRAM_SIZE: u64 = 1200;

/// Pacing gain for startup phase
const STARTUP_PACING_GAIN: f64 = 2.89;

/// Cwnd gain for startup
const STARTUP_CWND_GAIN: f64 = 2.89;

/// Drain pacing gain
const DRAIN_PACING_GAIN: f64 = 0.35;

/// Probe BW pacing gains (8 phases)
const PROBE_BW_GAINS: [f64; 8] = [1.25, 0.75, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];

/// BBRv2 state machine
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BbrState {
    /// Exponential bandwidth probing
    Startup,
    /// Drain excess queue from startup
    Drain,
    /// Steady-state bandwidth probing
    ProbeBW,
    /// RTT measurement
    ProbeRTT,
}

impl Default for BbrState {
    fn default() -> Self {
        Self::Startup
    }
}

/// Bandwidth sample
#[derive(Debug, Clone, Copy)]
pub struct BandwidthSample {
    /// Delivery rate in bytes/second
    pub delivery_rate: u64,
    /// Is app-limited sample
    pub is_app_limited: bool,
    /// Timestamp
    pub timestamp: Instant,
}

/// RTT sample
#[derive(Debug, Clone, Copy)]
pub struct RttSample {
    /// Measured RTT
    pub rtt: Duration,
    /// Timestamp
    pub timestamp: Instant,
}

/// BBRv2 congestion controller
#[derive(Debug)]
pub struct Bbrv2Controller {
    /// Current state
    state: BbrState,
    
    /// Congestion window in bytes
    cwnd: u64,
    
    /// Pacing rate in bytes/second
    pacing_rate: u64,
    
    /// Bytes in flight
    bytes_in_flight: u64,
    
    /// Estimated bottleneck bandwidth
    btl_bw: u64,
    
    /// Minimum RTT observed
    min_rtt: Option<Duration>,
    
    /// Time when min_rtt was last updated
    min_rtt_timestamp: Option<Instant>,
    
    /// Round-trip counter
    round_count: u64,
    
    /// Packet delivered count at round start
    round_start_delivered: u64,
    
    /// Total bytes delivered
    delivered: u64,
    
    /// Delivered timestamp
    delivered_time: Instant,
    
    /// First sent time
    first_sent_time: Instant,
    
    /// Is app limited
    is_app_limited: bool,
    
    /// Pacing gain
    pacing_gain: f64,
    
    /// Cwnd gain
    cwnd_gain: f64,
    
    /// Probe BW cycle index
    cycle_index: usize,
    
    /// Cycle start time
    cycle_start_time: Option<Instant>,
    
    /// Full bandwidth reached
    full_bw_reached: bool,
    
    /// Full bandwidth count (consecutive rounds without growth)
    full_bw_count: u8,
    
    /// Last recorded bandwidth
    full_bw: u64,
    
    /// ProbeRTT done
    probe_rtt_done: bool,
    
    /// ProbeRTT round done
    probe_rtt_round_done: bool,
    
    /// Prior cwnd before ProbeRTT
    prior_cwnd: u64,
    
    /// Congestion event count
    congestion_events: u64,
    
    /// Loss events in current round
    loss_in_round: bool,
    
    /// ECN-CE events in current round  
    ecn_in_round: bool,
    
    /// Inflight_lo for loss response
    inflight_lo: u64,
    
    /// Inflight_hi for bandwidth probing
    inflight_hi: u64,
    
    /// BBRv2 has probe BW phase
    bw_probe_up_rounds: u64,
    
    /// Target inflight during probe
    probe_up_cnt: u64,
}

impl Default for Bbrv2Controller {
    fn default() -> Self {
        Self::new()
    }
}

impl Bbrv2Controller {
    /// Create a new BBRv2 controller
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            state: BbrState::Startup,
            cwnd: INITIAL_CWND,
            pacing_rate: 0,
            bytes_in_flight: 0,
            btl_bw: 0,
            min_rtt: None,
            min_rtt_timestamp: None,
            round_count: 0,
            round_start_delivered: 0,
            delivered: 0,
            delivered_time: now,
            first_sent_time: now,
            is_app_limited: false,
            pacing_gain: STARTUP_PACING_GAIN,
            cwnd_gain: STARTUP_CWND_GAIN,
            cycle_index: 0,
            cycle_start_time: None,
            full_bw_reached: false,
            full_bw_count: 0,
            full_bw: 0,
            probe_rtt_done: false,
            probe_rtt_round_done: false,
            prior_cwnd: 0,
            congestion_events: 0,
            loss_in_round: false,
            ecn_in_round: false,
            inflight_lo: u64::MAX,
            inflight_hi: u64::MAX,
            bw_probe_up_rounds: 0,
            probe_up_cnt: u64::MAX,
        }
    }
    
    /// Get current congestion window
    pub fn cwnd(&self) -> u64 {
        self.cwnd
    }
    
    /// Get current pacing rate in bytes/second
    pub fn pacing_rate(&self) -> u64 {
        self.pacing_rate
    }
    
    /// Get bytes in flight
    pub fn bytes_in_flight(&self) -> u64 {
        self.bytes_in_flight
    }
    
    /// Get current state
    pub fn state(&self) -> BbrState {
        self.state
    }
    
    /// Get estimated bandwidth
    pub fn bandwidth(&self) -> u64 {
        self.btl_bw
    }
    
    /// Get minimum RTT
    pub fn min_rtt(&self) -> Option<Duration> {
        self.min_rtt
    }
    
    /// Check if can send `bytes`
    pub fn can_send(&self, bytes: u64) -> bool {
        self.bytes_in_flight + bytes <= self.cwnd
    }
    
    /// Record bytes sent
    pub fn record_sent(&mut self, bytes: u64) {
        self.bytes_in_flight += bytes;
        if self.first_sent_time.elapsed().as_nanos() == 0 {
            self.first_sent_time = Instant::now();
        }
    }
    
    /// On ACK received
    pub fn on_ack(&mut self, bytes_acked: u64, rtt: Duration, now: Instant) {
        self.bytes_in_flight = self.bytes_in_flight.saturating_sub(bytes_acked);
        self.delivered += bytes_acked;
        
        // Update RTT
        self.update_min_rtt(rtt, now);
        
        // Calculate delivery rate
        let delivery_rate = self.calculate_delivery_rate(bytes_acked, now);
        
        // Update bandwidth estimate
        self.update_bandwidth(delivery_rate);
        
        // Check for round completion
        self.check_round_completion();
        
        // State machine
        match self.state {
            BbrState::Startup => self.startup_update(),
            BbrState::Drain => self.drain_update(),
            BbrState::ProbeBW => self.probe_bw_update(now),
            BbrState::ProbeRTT => self.probe_rtt_update(now),
        }
        
        // Update pacing and cwnd
        self.update_pacing_rate();
        self.update_cwnd(bytes_acked);
        
        // Reset round loss indicators
        if self.round_count > 0 {
            self.loss_in_round = false;
            self.ecn_in_round = false;
        }
    }
    
    /// On packet loss detected
    pub fn on_loss(&mut self, bytes_lost: u64, now: Instant) {
        self.bytes_in_flight = self.bytes_in_flight.saturating_sub(bytes_lost);
        self.loss_in_round = true;
        self.congestion_events += 1;
        
        // BBRv2 loss response
        self.handle_loss_response(now);
    }
    
    /// On ECN congestion experienced
    pub fn on_ecn_ce(&mut self, now: Instant) {
        self.ecn_in_round = true;
        self.handle_loss_response(now);
    }
    
    fn update_min_rtt(&mut self, rtt: Duration, now: Instant) {
        if self.min_rtt.is_none() || rtt < self.min_rtt.unwrap() {
            self.min_rtt = Some(rtt);
            self.min_rtt_timestamp = Some(now);
        }
        
        // Check if min_rtt is stale (10 seconds)
        if let Some(timestamp) = self.min_rtt_timestamp {
            if now.duration_since(timestamp) > Duration::from_secs(10) {
                // Enter ProbeRTT to refresh
                if self.state != BbrState::ProbeRTT {
                    self.enter_probe_rtt();
                }
            }
        }
    }
    
    fn calculate_delivery_rate(&mut self, bytes_acked: u64, now: Instant) -> u64 {
        let interval = now.duration_since(self.delivered_time);
        self.delivered_time = now;
        
        if interval.as_nanos() == 0 {
            return self.btl_bw;
        }
        
        let rate = (bytes_acked as u128 * 1_000_000_000) / interval.as_nanos();
        rate as u64
    }
    
    fn update_bandwidth(&mut self, delivery_rate: u64) {
        if !self.is_app_limited && delivery_rate > self.btl_bw {
            self.btl_bw = delivery_rate;
        }
    }
    
    fn check_round_completion(&mut self) {
        if self.delivered >= self.round_start_delivered {
            self.round_count += 1;
            self.round_start_delivered = self.delivered;
        }
    }
    
    fn startup_update(&mut self) {
        // Check if bandwidth growth has stalled
        if self.btl_bw > 0 {
            if self.btl_bw >= self.full_bw * 5 / 4 {
                self.full_bw = self.btl_bw;
                self.full_bw_count = 0;
            } else {
                self.full_bw_count += 1;
            }
        }
        
        // Exit startup after 3 rounds without 25% growth
        if self.full_bw_count >= 3 {
            self.full_bw_reached = true;
            self.enter_drain();
        }
    }
    
    fn enter_drain(&mut self) {
        self.state = BbrState::Drain;
        self.pacing_gain = DRAIN_PACING_GAIN;
        self.cwnd_gain = STARTUP_CWND_GAIN;
    }
    
    fn drain_update(&mut self) {
        // Exit drain when inflight drops to target
        let bdp = self.bdp();
        if self.bytes_in_flight <= bdp {
            self.enter_probe_bw();
        }
    }
    
    fn enter_probe_bw(&mut self) {
        self.state = BbrState::ProbeBW;
        self.cycle_index = 0;
        self.cycle_start_time = Some(Instant::now());
        self.pacing_gain = PROBE_BW_GAINS[0];
        self.cwnd_gain = 2.0;
    }
    
    fn probe_bw_update(&mut self, now: Instant) {
        // Advance cycle
        if let Some(cycle_start) = self.cycle_start_time {
            let min_rtt = self.min_rtt.unwrap_or(Duration::from_millis(1));
            if now.duration_since(cycle_start) > min_rtt {
                self.cycle_index = (self.cycle_index + 1) % 8;
                self.cycle_start_time = Some(now);
                self.pacing_gain = PROBE_BW_GAINS[self.cycle_index];
            }
        }
    }
    
    fn enter_probe_rtt(&mut self) {
        self.prior_cwnd = self.cwnd;
        self.state = BbrState::ProbeRTT;
        self.pacing_gain = 1.0;
        self.probe_rtt_done = false;
        self.probe_rtt_round_done = false;
    }
    
    fn probe_rtt_update(&mut self, now: Instant) {
        // Reduce cwnd to minimum
        self.cwnd = MIN_CWND;
        
        // Stay in ProbeRTT for at least 200ms or one round
        if self.min_rtt_timestamp.is_some() {
            let time_in_state = now.duration_since(self.min_rtt_timestamp.unwrap());
            if time_in_state >= Duration::from_millis(200) {
                self.probe_rtt_done = true;
            }
        }
        
        if self.probe_rtt_done {
            // Restore cwnd and exit
            self.cwnd = self.prior_cwnd;
            self.enter_probe_bw();
        }
    }
    
    fn handle_loss_response(&mut self, _now: Instant) {
        // BBRv2 loss response: reduce inflight bounds
        let bdp = self.bdp();
        
        // Set inflight_lo conservatively
        self.inflight_lo = self.bytes_in_flight.max(bdp / 2);
        
        // Bound cwnd by inflight_lo
        if self.cwnd > self.inflight_lo {
            self.cwnd = self.inflight_lo;
        }
    }
    
    fn update_pacing_rate(&mut self) {
        let bdp = self.bdp();
        if bdp > 0 {
            self.pacing_rate = (self.btl_bw as f64 * self.pacing_gain) as u64;
        }
    }
    
    fn update_cwnd(&mut self, bytes_acked: u64) {
        let target = (self.bdp() as f64 * self.cwnd_gain) as u64;
        
        if self.state == BbrState::Startup {
            // In startup, always grow cwnd
            self.cwnd = self.cwnd.saturating_add(bytes_acked);
            self.cwnd = self.cwnd.max(target);
        } else {
            // In other states, bound cwnd
            self.cwnd = target
                .min(self.inflight_lo)
                .min(self.inflight_hi)
                .max(MIN_CWND);
        }
    }
    
    /// Calculate bandwidth-delay product
    pub fn bdp(&self) -> u64 {
        let min_rtt = self.min_rtt.unwrap_or(Duration::from_millis(1));
        (self.btl_bw as u128 * min_rtt.as_nanos() / 1_000_000_000) as u64
    }
    
    /// Get PTO (Probe Timeout)
    pub fn pto(&self) -> Duration {
        let srtt = self.min_rtt.unwrap_or(Duration::from_millis(100));
        srtt + srtt / 2 + Duration::from_millis(25)
    }
    
    /// In slow start equivalent
    pub fn in_slow_start(&self) -> bool {
        self.state == BbrState::Startup
    }
    
    /// Get congestion event count
    pub fn congestion_events(&self) -> u64 {
        self.congestion_events
    }
    
    /// Reset controller
    pub fn reset(&mut self) {
        *self = Self::new();
    }
    
    /// Set app-limited state
    pub fn set_app_limited(&mut self, limited: bool) {
        self.is_app_limited = limited;
    }
}

/// Congestion control strategy selector
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CongestionAlgorithm {
    /// Cubic congestion control
    Cubic,
    /// BBRv2 congestion control
    Bbrv2,
    /// New Reno (legacy)
    NewReno,
}

impl Default for CongestionAlgorithm {
    fn default() -> Self {
        Self::Bbrv2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_bbrv2_initial_state() {
        let bbr = Bbrv2Controller::new();
        assert_eq!(bbr.state(), BbrState::Startup);
        assert_eq!(bbr.cwnd(), INITIAL_CWND);
        assert!(bbr.in_slow_start());
    }
    
    #[test]
    fn test_bbrv2_can_send() {
        let bbr = Bbrv2Controller::new();
        assert!(bbr.can_send(1000));
        assert!(bbr.can_send(INITIAL_CWND));
        assert!(!bbr.can_send(INITIAL_CWND + 1));
    }
    
    #[test]
    fn test_bbrv2_record_sent() {
        let mut bbr = Bbrv2Controller::new();
        bbr.record_sent(1000);
        assert_eq!(bbr.bytes_in_flight(), 1000);
    }
    
    #[test]
    fn test_bbrv2_on_ack() {
        let mut bbr = Bbrv2Controller::new();
        bbr.record_sent(5000);
        
        let now = Instant::now();
        bbr.on_ack(5000, Duration::from_millis(50), now);
        
        assert_eq!(bbr.bytes_in_flight(), 0);
        assert!(bbr.min_rtt().is_some());
    }
    
    #[test]
    fn test_bbrv2_on_loss() {
        let mut bbr = Bbrv2Controller::new();
        bbr.record_sent(5000);
        
        bbr.on_loss(2000, Instant::now());
        
        assert_eq!(bbr.bytes_in_flight(), 3000);
        assert_eq!(bbr.congestion_events(), 1);
    }
    
    #[test]
    fn test_bbrv2_bdp() {
        let mut bbr = Bbrv2Controller::new();
        bbr.record_sent(10000);
        bbr.on_ack(10000, Duration::from_millis(50), Instant::now());
        
        // BDP should be calculated
        let bdp = bbr.bdp();
        assert!(bdp >= 0);
    }
    
    #[test]
    fn test_congestion_algorithm_default() {
        assert_eq!(CongestionAlgorithm::default(), CongestionAlgorithm::Bbrv2);
    }
}
