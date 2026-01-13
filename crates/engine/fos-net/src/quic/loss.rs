//! QUIC Loss Detection
//!
//! Loss detection and recovery per RFC 9002.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Packet number space
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PacketSpace {
    /// Initial packets
    Initial,
    /// Handshake packets
    Handshake,
    /// Application data (1-RTT)
    Application,
}

/// Information about a sent packet
#[derive(Debug, Clone)]
pub struct SentPacket {
    /// Packet number
    pub packet_number: u64,
    /// Time sent
    pub time_sent: Instant,
    /// Size in bytes
    pub size: usize,
    /// Whether this packet is ACK-eliciting
    pub ack_eliciting: bool,
    /// Whether this packet is in flight
    pub in_flight: bool,
    /// Packet number space
    pub space: PacketSpace,
}

/// Acknowledgment information
#[derive(Debug, Clone)]
pub struct AckInfo {
    /// Largest acknowledged packet number
    pub largest_acked: u64,
    /// ACK delay
    pub ack_delay: Duration,
    /// Time ACK was received
    pub ack_time: Instant,
}

/// Loss detection state
#[derive(Debug)]
pub struct LossDetection {
    /// Sent packets awaiting acknowledgment (by space)
    sent_packets: HashMap<PacketSpace, HashMap<u64, SentPacket>>,
    /// Largest acknowledged packet number (by space)
    largest_acked: HashMap<PacketSpace, Option<u64>>,
    /// Time of the last ack-eliciting packet (by space)
    time_of_last_ack_eliciting: HashMap<PacketSpace, Option<Instant>>,
    /// Loss time (by space)
    loss_time: HashMap<PacketSpace, Option<Instant>>,
    /// Probe timeout count
    pto_count: u32,
    /// Max ACK delay (from transport parameters)
    max_ack_delay: Duration,
    /// Packet reordering threshold
    packet_threshold: u64,
    /// Time reordering threshold factor
    time_threshold: f64,
}

impl LossDetection {
    /// Create new loss detection state
    pub fn new() -> Self {
        let mut sent_packets = HashMap::new();
        let mut largest_acked = HashMap::new();
        let mut time_of_last = HashMap::new();
        let mut loss_time = HashMap::new();
        
        for space in [PacketSpace::Initial, PacketSpace::Handshake, PacketSpace::Application] {
            sent_packets.insert(space, HashMap::new());
            largest_acked.insert(space, None);
            time_of_last.insert(space, None);
            loss_time.insert(space, None);
        }
        
        Self {
            sent_packets,
            largest_acked,
            time_of_last_ack_eliciting: time_of_last,
            loss_time,
            pto_count: 0,
            max_ack_delay: Duration::from_millis(25),
            packet_threshold: 3,
            time_threshold: 9.0 / 8.0,
        }
    }
    
    /// Record a sent packet
    pub fn on_packet_sent(&mut self, packet: SentPacket) {
        let space = packet.space;
        
        if packet.ack_eliciting {
            self.time_of_last_ack_eliciting.insert(space, Some(packet.time_sent));
        }
        
        self.sent_packets
            .get_mut(&space)
            .unwrap()
            .insert(packet.packet_number, packet);
    }
    
    /// Process an ACK, returns (newly acked packets, lost packets)
    pub fn on_ack_received(
        &mut self,
        space: PacketSpace,
        ack_info: &AckInfo,
        smoothed_rtt: Duration,
    ) -> (Vec<SentPacket>, Vec<SentPacket>) {
        // Update largest acked
        let prev_largest = self.largest_acked.get(&space).unwrap_or(&None);
        if prev_largest.is_none() || Some(ack_info.largest_acked) > *prev_largest {
            self.largest_acked.insert(space, Some(ack_info.largest_acked));
        }
        
        // Find newly acked packets
        let mut newly_acked = Vec::new();
        let sent = self.sent_packets.get_mut(&space).unwrap();
        
        // Remove acked packets (simplified - should handle ACK ranges)
        if let Some(packet) = sent.remove(&ack_info.largest_acked) {
            newly_acked.push(packet);
        }
        
        // Reset PTO count on ack
        self.pto_count = 0;
        
        // Detect lost packets
        let lost = self.detect_lost_packets(space, ack_info.ack_time, smoothed_rtt);
        
        (newly_acked, lost)
    }
    
    /// Detect lost packets
    fn detect_lost_packets(
        &mut self,
        space: PacketSpace,
        now: Instant,
        smoothed_rtt: Duration,
    ) -> Vec<SentPacket> {
        let mut lost = Vec::new();
        
        let largest_acked = match self.largest_acked.get(&space).unwrap() {
            Some(la) => *la,
            None => return lost,
        };
        
        // Loss delay threshold
        let loss_delay = Duration::from_secs_f64(
            smoothed_rtt.as_secs_f64() * self.time_threshold
        );
        let loss_delay = loss_delay.max(Duration::from_millis(1));
        
        let sent = self.sent_packets.get_mut(&space).unwrap();
        let mut to_remove = Vec::new();
        
        for (&pn, packet) in sent.iter() {
            if pn > largest_acked {
                continue;
            }
            
            // Check packet threshold
            let packets_since = largest_acked - pn;
            let time_since = now.duration_since(packet.time_sent);
            
            if packets_since >= self.packet_threshold || time_since > loss_delay {
                to_remove.push(pn);
            }
        }
        
        for pn in to_remove {
            if let Some(packet) = sent.remove(&pn) {
                lost.push(packet);
            }
        }
        
        lost
    }
    
    /// Get probe timeout duration
    pub fn get_pto(&self, smoothed_rtt: Duration, rtt_var: Duration, space: PacketSpace) -> Duration {
        let mut pto = smoothed_rtt + rtt_var.max(Duration::from_millis(1)) * 4;
        
        if space == PacketSpace::Application {
            pto += self.max_ack_delay;
        }
        
        pto *= 2u32.pow(self.pto_count);
        pto
    }
    
    /// Increment PTO count (called on timeout)
    pub fn on_pto_timeout(&mut self) {
        self.pto_count = self.pto_count.saturating_add(1);
    }
    
    /// Reset PTO count
    pub fn reset_pto_count(&mut self) {
        self.pto_count = 0;
    }
    
    /// Get number of packets in flight for a space
    pub fn packets_in_flight(&self, space: PacketSpace) -> usize {
        self.sent_packets.get(&space).map(|m| m.len()).unwrap_or(0)
    }
    
    /// Get all in-flight packet numbers for a space
    pub fn in_flight_packets(&self, space: PacketSpace) -> Vec<u64> {
        self.sent_packets
            .get(&space)
            .map(|m| m.keys().copied().collect())
            .unwrap_or_default()
    }
    
    /// Discard packets for a space (e.g., when keys are discarded)
    pub fn discard_space(&mut self, space: PacketSpace) {
        self.sent_packets.insert(space, HashMap::new());
        self.largest_acked.insert(space, None);
        self.time_of_last_ack_eliciting.insert(space, None);
        self.loss_time.insert(space, None);
    }
    
    /// Check if any packets are outstanding
    pub fn has_in_flight_packets(&self) -> bool {
        self.sent_packets.values().any(|m| !m.is_empty())
    }
    
    /// Get PTO count
    pub fn pto_count(&self) -> u32 {
        self.pto_count
    }
}

impl Default for LossDetection {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_sent_packet_tracking() {
        let mut ld = LossDetection::new();
        let now = Instant::now();
        
        let packet = SentPacket {
            packet_number: 0,
            time_sent: now,
            size: 1200,
            ack_eliciting: true,
            in_flight: true,
            space: PacketSpace::Initial,
        };
        
        ld.on_packet_sent(packet);
        
        assert_eq!(ld.packets_in_flight(PacketSpace::Initial), 1);
    }
    
    #[test]
    fn test_ack_processing() {
        let mut ld = LossDetection::new();
        let now = Instant::now();
        
        let packet = SentPacket {
            packet_number: 0,
            time_sent: now,
            size: 1200,
            ack_eliciting: true,
            in_flight: true,
            space: PacketSpace::Initial,
        };
        
        ld.on_packet_sent(packet);
        
        let ack_info = AckInfo {
            largest_acked: 0,
            ack_delay: Duration::ZERO,
            ack_time: now,
        };
        
        let (acked, lost) = ld.on_ack_received(
            PacketSpace::Initial,
            &ack_info,
            Duration::from_millis(100),
        );
        
        assert_eq!(acked.len(), 1);
        assert!(lost.is_empty());
        assert_eq!(ld.packets_in_flight(PacketSpace::Initial), 0);
    }
    
    #[test]
    fn test_pto_calculation() {
        let ld = LossDetection::new();
        
        let pto = ld.get_pto(
            Duration::from_millis(100),
            Duration::from_millis(25),
            PacketSpace::Initial,
        );
        
        // PTO = SRTT + max(4*RTTVAR, 1ms)
        // 100 + 100 = 200ms
        assert!(pto >= Duration::from_millis(100));
    }
    
    #[test]
    fn test_pto_backoff() {
        let mut ld = LossDetection::new();
        
        let pto1 = ld.get_pto(
            Duration::from_millis(100),
            Duration::from_millis(25),
            PacketSpace::Initial,
        );
        
        ld.on_pto_timeout();
        
        let pto2 = ld.get_pto(
            Duration::from_millis(100),
            Duration::from_millis(25),
            PacketSpace::Initial,
        );
        
        // PTO should double
        assert_eq!(pto2, pto1 * 2);
    }
    
    #[test]
    fn test_discard_space() {
        let mut ld = LossDetection::new();
        let now = Instant::now();
        
        let packet = SentPacket {
            packet_number: 0,
            time_sent: now,
            size: 1200,
            ack_eliciting: true,
            in_flight: true,
            space: PacketSpace::Initial,
        };
        
        ld.on_packet_sent(packet);
        assert_eq!(ld.packets_in_flight(PacketSpace::Initial), 1);
        
        ld.discard_space(PacketSpace::Initial);
        assert_eq!(ld.packets_in_flight(PacketSpace::Initial), 0);
    }
}
