//! RTCDataChannel
//!
//! WebRTC data channels for peer-to-peer data.

use std::collections::VecDeque;

/// Data channel state
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RTCDataChannelState {
    #[default]
    Connecting,
    Open,
    Closing,
    Closed,
}

/// RTC Data Channel
#[derive(Debug)]
pub struct RTCDataChannel {
    pub label: String,
    pub ordered: bool,
    pub max_packet_life_time: Option<u16>,
    pub max_retransmits: Option<u16>,
    pub protocol: String,
    pub negotiated: bool,
    pub id: Option<u16>,
    pub ready_state: RTCDataChannelState,
    pub buffered_amount: usize,
    pub buffered_amount_low_threshold: usize,
    pub binary_type: BinaryType,
    message_queue: VecDeque<DataChannelMessage>,
}

/// Binary type
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BinaryType {
    #[default]
    Blob,
    ArrayBuffer,
}

/// Data channel message
#[derive(Debug, Clone)]
pub enum DataChannelMessage {
    Text(String),
    Binary(Vec<u8>),
}

/// Data channel options
#[derive(Debug, Clone, Default)]
pub struct RTCDataChannelInit {
    pub ordered: Option<bool>,
    pub max_packet_life_time: Option<u16>,
    pub max_retransmits: Option<u16>,
    pub protocol: Option<String>,
    pub negotiated: Option<bool>,
    pub id: Option<u16>,
}

impl RTCDataChannel {
    pub fn new(label: &str, options: RTCDataChannelInit) -> Self {
        Self {
            label: label.to_string(),
            ordered: options.ordered.unwrap_or(true),
            max_packet_life_time: options.max_packet_life_time,
            max_retransmits: options.max_retransmits,
            protocol: options.protocol.unwrap_or_default(),
            negotiated: options.negotiated.unwrap_or(false),
            id: options.id,
            ready_state: RTCDataChannelState::Connecting,
            buffered_amount: 0,
            buffered_amount_low_threshold: 0,
            binary_type: BinaryType::Blob,
            message_queue: VecDeque::new(),
        }
    }
    
    /// Open channel
    pub fn open(&mut self) {
        self.ready_state = RTCDataChannelState::Open;
    }
    
    /// Send text message
    pub fn send(&mut self, data: &str) -> Result<(), DataChannelError> {
        if self.ready_state != RTCDataChannelState::Open {
            return Err(DataChannelError::InvalidState);
        }
        self.buffered_amount += data.len();
        Ok(())
    }
    
    /// Send binary message
    pub fn send_binary(&mut self, data: &[u8]) -> Result<(), DataChannelError> {
        if self.ready_state != RTCDataChannelState::Open {
            return Err(DataChannelError::InvalidState);
        }
        self.buffered_amount += data.len();
        Ok(())
    }
    
    /// Close channel
    pub fn close(&mut self) {
        self.ready_state = RTCDataChannelState::Closing;
        self.ready_state = RTCDataChannelState::Closed;
    }
    
    /// Receive message (for testing)
    pub fn receive(&mut self, msg: DataChannelMessage) {
        self.message_queue.push_back(msg);
    }
    
    /// Get next message
    pub fn next_message(&mut self) -> Option<DataChannelMessage> {
        self.message_queue.pop_front()
    }
}

/// Data channel error
#[derive(Debug, Clone)]
pub enum DataChannelError {
    InvalidState,
    NetworkError,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_data_channel() {
        let mut dc = RTCDataChannel::new("test", RTCDataChannelInit::default());
        dc.open();
        
        dc.send("Hello").unwrap();
        assert!(dc.buffered_amount > 0);
    }
}
