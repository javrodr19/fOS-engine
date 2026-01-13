//! QUIC Flow Control
//!
//! Connection and stream-level flow control per RFC 9000 §4.

/// Flow control state for a connection
#[derive(Debug, Clone)]
pub struct FlowController {
    /// Maximum data we can send (connection level)
    send_max: u64,
    /// Data we've sent (connection level)
    send_used: u64,
    /// Maximum data we'll receive (connection level)
    recv_max: u64,
    /// Data we've received (connection level)
    recv_used: u64,
    /// Initial window size
    initial_window: u64,
    /// Auto-tune factor for window updates
    auto_tune_factor: f64,
}

impl FlowController {
    /// Create a new flow controller with default windows
    pub fn new() -> Self {
        Self::with_windows(1024 * 1024, 1024 * 1024) // 1MB default
    }
    
    /// Create with specific window sizes
    pub fn with_windows(send_max: u64, recv_max: u64) -> Self {
        Self {
            send_max,
            send_used: 0,
            recv_max,
            recv_used: 0,
            initial_window: recv_max,
            auto_tune_factor: 2.0,
        }
    }
    
    /// Check if we can send `bytes` at connection level
    pub fn can_send(&self, bytes: u64) -> bool {
        self.send_used.saturating_add(bytes) <= self.send_max
    }
    
    /// Get available send window
    pub fn send_window(&self) -> u64 {
        self.send_max.saturating_sub(self.send_used)
    }
    
    /// Record bytes sent
    pub fn record_sent(&mut self, bytes: u64) {
        self.send_used = self.send_used.saturating_add(bytes);
    }
    
    /// Update send limit (from MAX_DATA frame)
    pub fn update_send_max(&mut self, max: u64) {
        if max > self.send_max {
            self.send_max = max;
        }
    }
    
    /// Check if we need to send MAX_DATA
    pub fn should_send_max_data(&self) -> Option<u64> {
        // Send MAX_DATA when half the receive window is consumed
        if self.recv_used > self.recv_max / 2 {
            let new_max = self.recv_used.saturating_add(self.initial_window);
            Some(new_max)
        } else {
            None
        }
    }
    
    /// Record bytes received
    pub fn record_received(&mut self, bytes: u64) {
        self.recv_used = self.recv_used.saturating_add(bytes);
    }
    
    /// Update receive max after sending MAX_DATA
    pub fn update_recv_max(&mut self, new_max: u64) {
        self.recv_max = new_max;
    }
    
    /// Check if receiving `bytes` would violate flow control
    pub fn can_receive(&self, bytes: u64) -> bool {
        self.recv_used.saturating_add(bytes) <= self.recv_max
    }
    
    /// Get current receive max
    pub fn recv_max(&self) -> u64 {
        self.recv_max
    }
    
    /// Get bytes received
    pub fn recv_used(&self) -> u64 {
        self.recv_used
    }
    
    /// Reset flow control (e.g., for connection migration)
    pub fn reset(&mut self) {
        self.send_used = 0;
        self.recv_used = 0;
    }
}

impl Default for FlowController {
    fn default() -> Self {
        Self::new()
    }
}

/// Stream-level flow control
#[derive(Debug, Clone)]
pub struct StreamFlowControl {
    /// Maximum data we can send on this stream
    send_max: u64,
    /// Data we've sent on this stream
    send_used: u64,
    /// Maximum data we'll receive on this stream
    recv_max: u64,
    /// Data we've received on this stream
    recv_used: u64,
    /// Highest offset received (for reordering)
    recv_highest: u64,
}

impl StreamFlowControl {
    /// Create new stream flow control
    pub fn new(initial_max: u64) -> Self {
        Self {
            send_max: initial_max,
            send_used: 0,
            recv_max: initial_max,
            recv_used: 0,
            recv_highest: 0,
        }
    }
    
    /// Check if we can send `bytes`
    pub fn can_send(&self, bytes: u64) -> bool {
        self.send_used.saturating_add(bytes) <= self.send_max
    }
    
    /// Available send window
    pub fn send_window(&self) -> u64 {
        self.send_max.saturating_sub(self.send_used)
    }
    
    /// Record bytes sent
    pub fn record_sent(&mut self, bytes: u64) {
        self.send_used = self.send_used.saturating_add(bytes);
    }
    
    /// Update send max (from MAX_STREAM_DATA)
    pub fn update_send_max(&mut self, max: u64) {
        if max > self.send_max {
            self.send_max = max;
        }
    }
    
    /// Record data received at offset
    pub fn record_received(&mut self, offset: u64, length: u64) -> bool {
        let end_offset = offset.saturating_add(length);
        
        // Check if this would exceed our receive limit
        if end_offset > self.recv_max {
            return false;
        }
        
        // Update highest received offset
        if end_offset > self.recv_highest {
            // Only count new bytes
            let new_bytes = end_offset.saturating_sub(self.recv_highest);
            self.recv_used = self.recv_used.saturating_add(new_bytes);
            self.recv_highest = end_offset;
        }
        
        true
    }
    
    /// Check if we should send MAX_STREAM_DATA
    pub fn should_send_max_stream_data(&self) -> Option<u64> {
        if self.recv_used > self.recv_max / 2 {
            // Double the window
            Some(self.recv_max.saturating_mul(2))
        } else {
            None
        }
    }
    
    /// Update receive max after sending MAX_STREAM_DATA
    pub fn update_recv_max(&mut self, new_max: u64) {
        self.recv_max = new_max;
    }
    
    /// Get current send max
    pub fn send_max(&self) -> u64 {
        self.send_max
    }
    
    /// Get current recv max
    pub fn recv_max(&self) -> u64 {
        self.recv_max
    }
}

impl Default for StreamFlowControl {
    fn default() -> Self {
        Self::new(256 * 1024) // 256KB default per stream
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_connection_flow_control() {
        let mut fc = FlowController::with_windows(1000, 1000);
        
        assert!(fc.can_send(500));
        assert!(fc.can_send(1000));
        assert!(!fc.can_send(1001));
        
        fc.record_sent(600);
        assert_eq!(fc.send_window(), 400);
        
        fc.update_send_max(2000);
        assert_eq!(fc.send_window(), 1400);
    }
    
    #[test]
    fn test_receive_flow_control() {
        let mut fc = FlowController::with_windows(1000, 1000);
        
        assert!(fc.can_receive(500));
        fc.record_received(500);
        
        assert!(fc.can_receive(500));
        assert!(!fc.can_receive(501));
    }
    
    #[test]
    fn test_max_data_trigger() {
        let mut fc = FlowController::with_windows(1000, 1000);
        
        // No trigger initially
        assert!(fc.should_send_max_data().is_none());
        
        // After consuming half the window
        fc.record_received(501);
        assert!(fc.should_send_max_data().is_some());
    }
    
    #[test]
    fn test_stream_flow_control() {
        let mut sfc = StreamFlowControl::new(1000);
        
        assert!(sfc.can_send(500));
        sfc.record_sent(500);
        assert_eq!(sfc.send_window(), 500);
        
        assert!(sfc.record_received(0, 100));
        assert!(sfc.record_received(100, 100)); // Sequential
        assert!(sfc.record_received(50, 50));   // Overlapping (allowed)
    }
    
    #[test]
    fn test_stream_flow_violation() {
        let mut sfc = StreamFlowControl::new(100);
        
        // Receiving beyond limit should fail
        assert!(!sfc.record_received(0, 101));
        
        // Receiving at the limit should succeed
        assert!(sfc.record_received(0, 100));
    }
}
