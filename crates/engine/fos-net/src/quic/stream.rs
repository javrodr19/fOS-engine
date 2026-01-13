//! QUIC Streams
//!
//! Stream management and multiplexing per RFC 9000 §2-3.

use std::collections::{HashMap, VecDeque};
use super::flow::StreamFlowControl;

/// Stream state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamState {
    /// Stream created, no data sent/received
    Idle,
    /// Stream is open for send and receive
    Open,
    /// Send side closed, can still receive
    HalfClosedLocal,
    /// Receive side closed, can still send
    HalfClosedRemote,
    /// Both sides closed, waiting for final ACKs
    DataSent,
    /// Stream fully closed
    Closed,
    /// Stream reset
    Reset,
}

impl Default for StreamState {
    fn default() -> Self {
        StreamState::Idle
    }
}

/// Stream type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamType {
    /// Bidirectional stream
    Bidirectional,
    /// Unidirectional stream
    Unidirectional,
}

/// A QUIC stream
#[derive(Debug)]
pub struct QuicStream {
    /// Stream ID
    id: u64,
    /// Stream state
    state: StreamState,
    /// Send buffer
    send_buffer: VecDeque<u8>,
    /// Receive buffer (offset -> data)
    recv_buffer: HashMap<u64, Vec<u8>>,
    /// Next offset to read
    recv_read_offset: u64,
    /// Contiguous received data
    recv_contiguous: Vec<u8>,
    /// Send offset
    send_offset: u64,
    /// Flow control
    flow: StreamFlowControl,
    /// Priority (0-255, lower is higher priority)
    priority: u8,
    /// FIN received
    fin_received: bool,
    /// FIN sent
    fin_sent: bool,
    /// Final size (if known)
    final_size: Option<u64>,
}

impl QuicStream {
    /// Create a new stream
    pub fn new(id: u64, initial_max_data: u64) -> Self {
        Self {
            id,
            state: StreamState::Open,
            send_buffer: VecDeque::new(),
            recv_buffer: HashMap::new(),
            recv_read_offset: 0,
            recv_contiguous: Vec::new(),
            send_offset: 0,
            flow: StreamFlowControl::new(initial_max_data),
            priority: 128,
            fin_received: false,
            fin_sent: false,
            final_size: None,
        }
    }
    
    /// Get stream ID
    pub fn id(&self) -> u64 {
        self.id
    }
    
    /// Get stream state
    pub fn state(&self) -> StreamState {
        self.state
    }
    
    /// Check if stream is locally initiated
    pub fn is_local(&self, is_client: bool) -> bool {
        let initiator_bit = self.id & 0x01;
        (initiator_bit == 0) == is_client
    }
    
    /// Check if stream is bidirectional
    pub fn is_bidirectional(&self) -> bool {
        (self.id & 0x02) == 0
    }
    
    /// Get stream type
    pub fn stream_type(&self) -> StreamType {
        if self.is_bidirectional() {
            StreamType::Bidirectional
        } else {
            StreamType::Unidirectional
        }
    }
    
    /// Queue data for sending
    pub fn write(&mut self, data: &[u8]) -> Result<usize, StreamError> {
        if !self.can_send() {
            return Err(StreamError::NotWritable);
        }
        
        let allowed = self.flow.send_window() as usize;
        let to_write = data.len().min(allowed);
        
        if to_write > 0 {
            self.send_buffer.extend(&data[..to_write]);
        }
        
        Ok(to_write)
    }
    
    /// Get data to send (up to max_len bytes)
    pub fn get_send_data(&mut self, max_len: usize) -> Option<(u64, Vec<u8>, bool)> {
        if self.send_buffer.is_empty() {
            return None;
        }
        
        let available = self.send_buffer.len().min(max_len);
        let data: Vec<u8> = self.send_buffer.drain(..available).collect();
        let offset = self.send_offset;
        self.send_offset += data.len() as u64;
        self.flow.record_sent(data.len() as u64);
        
        let fin = self.fin_sent && self.send_buffer.is_empty();
        
        Some((offset, data, fin))
    }
    
    /// Receive data at offset
    pub fn receive(&mut self, offset: u64, data: Vec<u8>, fin: bool) -> Result<(), StreamError> {
        if !self.can_receive() {
            return Err(StreamError::NotReadable);
        }
        
        // Check flow control
        if !self.flow.record_received(offset, data.len() as u64) {
            return Err(StreamError::FlowControl);
        }
        
        // Handle FIN
        if fin {
            self.fin_received = true;
            self.final_size = Some(offset + data.len() as u64);
        }
        
        // Store in receive buffer
        if offset == self.recv_read_offset + self.recv_contiguous.len() as u64 {
            // Contiguous data, append directly
            self.recv_contiguous.extend(data);
            
            // Try to merge any buffered out-of-order data
            self.merge_recv_buffer();
        } else {
            // Out-of-order, store in buffer
            self.recv_buffer.insert(offset, data);
        }
        
        Ok(())
    }
    
    /// Merge contiguous data from receive buffer
    fn merge_recv_buffer(&mut self) {
        loop {
            let next_offset = self.recv_read_offset + self.recv_contiguous.len() as u64;
            if let Some(data) = self.recv_buffer.remove(&next_offset) {
                self.recv_contiguous.extend(data);
            } else {
                break;
            }
        }
    }
    
    /// Read received data
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, StreamError> {
        if self.recv_contiguous.is_empty() {
            if self.fin_received && self.recv_buffer.is_empty() {
                return Err(StreamError::Finished);
            }
            return Ok(0);
        }
        
        let to_read = buf.len().min(self.recv_contiguous.len());
        buf[..to_read].copy_from_slice(&self.recv_contiguous[..to_read]);
        self.recv_contiguous.drain(..to_read);
        self.recv_read_offset += to_read as u64;
        
        Ok(to_read)
    }
    
    /// Get available read bytes
    pub fn readable_bytes(&self) -> usize {
        self.recv_contiguous.len()
    }
    
    /// Check if stream can send
    pub fn can_send(&self) -> bool {
        matches!(self.state, StreamState::Open | StreamState::HalfClosedRemote)
    }
    
    /// Check if stream can receive
    pub fn can_receive(&self) -> bool {
        matches!(self.state, StreamState::Open | StreamState::HalfClosedLocal)
    }
    
    /// Send FIN
    pub fn shutdown_send(&mut self) {
        self.fin_sent = true;
        self.state = match self.state {
            StreamState::Open => StreamState::HalfClosedLocal,
            StreamState::HalfClosedRemote => StreamState::DataSent,
            other => other,
        };
    }
    
    /// Close receive side
    pub fn shutdown_recv(&mut self) {
        self.state = match self.state {
            StreamState::Open => StreamState::HalfClosedRemote,
            StreamState::HalfClosedLocal => StreamState::Closed,
            StreamState::DataSent => StreamState::Closed,
            other => other,
        };
    }
    
    /// Reset the stream
    pub fn reset(&mut self, error_code: u64) {
        self.state = StreamState::Reset;
        self.send_buffer.clear();
        let _ = error_code; // Would be sent in RESET_STREAM frame
    }
    
    /// Update send max data (from MAX_STREAM_DATA)
    pub fn update_send_max(&mut self, max: u64) {
        self.flow.update_send_max(max);
    }
    
    /// Get priority
    pub fn priority(&self) -> u8 {
        self.priority
    }
    
    /// Set priority
    pub fn set_priority(&mut self, priority: u8) {
        self.priority = priority;
    }
    
    /// Check if stream is finished
    pub fn is_finished(&self) -> bool {
        matches!(self.state, StreamState::Closed | StreamState::Reset)
    }
    
    /// Get flow control reference
    pub fn flow(&self) -> &StreamFlowControl {
        &self.flow
    }
    
    /// Get flow control mutable reference
    pub fn flow_mut(&mut self) -> &mut StreamFlowControl {
        &mut self.flow
    }
}

/// Stream error
#[derive(Debug, Clone, thiserror::Error)]
pub enum StreamError {
    #[error("Stream not writable")]
    NotWritable,
    
    #[error("Stream not readable")]
    NotReadable,
    
    #[error("Flow control violation")]
    FlowControl,
    
    #[error("Stream finished")]
    Finished,
    
    #[error("Stream reset")]
    Reset,
}

/// Stream manager
#[derive(Debug)]
pub struct StreamManager {
    /// All streams
    streams: HashMap<u64, QuicStream>,
    /// Next bidirectional stream ID to use (client: 0, 4, 8...; server: 1, 5, 9...)
    next_bidi_id: u64,
    /// Next unidirectional stream ID to use (client: 2, 6, 10...; server: 3, 7, 11...)
    next_uni_id: u64,
    /// Max peer-initiated bidirectional streams
    max_bidi_remote: u64,
    /// Max peer-initiated unidirectional streams
    max_uni_remote: u64,
    /// Initial max stream data (local)
    initial_max_stream_data_local: u64,
    /// Initial max stream data (remote)
    initial_max_stream_data_remote: u64,
    /// Whether we are client
    is_client: bool,
}

impl StreamManager {
    /// Create a new stream manager
    pub fn new(is_client: bool) -> Self {
        Self {
            streams: HashMap::new(),
            next_bidi_id: if is_client { 0 } else { 1 },
            next_uni_id: if is_client { 2 } else { 3 },
            max_bidi_remote: 100,
            max_uni_remote: 100,
            initial_max_stream_data_local: 256 * 1024,
            initial_max_stream_data_remote: 256 * 1024,
            is_client,
        }
    }
    
    /// Open a new bidirectional stream
    pub fn open_bidi(&mut self) -> u64 {
        let id = self.next_bidi_id;
        self.next_bidi_id += 4;
        
        let stream = QuicStream::new(id, self.initial_max_stream_data_local);
        self.streams.insert(id, stream);
        
        id
    }
    
    /// Open a new unidirectional stream
    pub fn open_uni(&mut self) -> u64 {
        let id = self.next_uni_id;
        self.next_uni_id += 4;
        
        let stream = QuicStream::new(id, self.initial_max_stream_data_local);
        self.streams.insert(id, stream);
        
        id
    }
    
    /// Get a stream by ID
    pub fn get(&self, id: u64) -> Option<&QuicStream> {
        self.streams.get(&id)
    }
    
    /// Get a stream mutably by ID
    pub fn get_mut(&mut self, id: u64) -> Option<&mut QuicStream> {
        self.streams.get_mut(&id)
    }
    
    /// Get or create a stream (for received data)
    pub fn get_or_create(&mut self, id: u64) -> Result<&mut QuicStream, StreamError> {
        if !self.streams.contains_key(&id) {
            // Check if this is a valid remotely-initiated stream
            let is_remote = (id & 0x01) != if self.is_client { 0 } else { 1 };
            if !is_remote {
                return Err(StreamError::NotReadable);
            }
            
            let stream = QuicStream::new(id, self.initial_max_stream_data_remote);
            self.streams.insert(id, stream);
        }
        
        Ok(self.streams.get_mut(&id).unwrap())
    }
    
    /// Close a stream
    pub fn close(&mut self, id: u64) {
        if let Some(stream) = self.streams.get_mut(&id) {
            stream.shutdown_send();
            stream.shutdown_recv();
        }
    }
    
    /// Remove finished streams
    pub fn cleanup(&mut self) {
        self.streams.retain(|_, s| !s.is_finished());
    }
    
    /// Get all stream IDs
    pub fn stream_ids(&self) -> Vec<u64> {
        self.streams.keys().copied().collect()
    }
    
    /// Get number of streams
    pub fn count(&self) -> usize {
        self.streams.len()
    }
    
    /// Iterate over streams with data to send
    pub fn streams_with_data(&self) -> impl Iterator<Item = u64> + '_ {
        self.streams.iter()
            .filter(|(_, s)| !s.send_buffer.is_empty())
            .map(|(id, _)| *id)
    }
}

impl Default for StreamManager {
    fn default() -> Self {
        Self::new(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_stream_creation() {
        let stream = QuicStream::new(0, 1024);
        assert_eq!(stream.id(), 0);
        assert_eq!(stream.state(), StreamState::Open);
        assert!(stream.is_bidirectional());
    }
    
    #[test]
    fn test_stream_write_read() {
        let mut stream = QuicStream::new(0, 1024);
        
        // Write data
        let written = stream.write(b"hello").unwrap();
        assert_eq!(written, 5);
        
        // Get send data
        let (offset, data, _fin) = stream.get_send_data(100).unwrap();
        assert_eq!(offset, 0);
        assert_eq!(data, b"hello");
    }
    
    #[test]
    fn test_stream_receive() {
        let mut stream = QuicStream::new(0, 1024);
        
        // Receive data
        stream.receive(0, b"hello".to_vec(), false).unwrap();
        
        let mut buf = [0u8; 10];
        let read = stream.read(&mut buf).unwrap();
        
        assert_eq!(read, 5);
        assert_eq!(&buf[..5], b"hello");
    }
    
    #[test]
    fn test_stream_out_of_order() {
        let mut stream = QuicStream::new(0, 1024);
        
        // Receive out of order
        stream.receive(5, b"world".to_vec(), false).unwrap();
        stream.receive(0, b"hello".to_vec(), false).unwrap();
        
        let mut buf = [0u8; 20];
        let read = stream.read(&mut buf).unwrap();
        
        assert_eq!(read, 10);
        assert_eq!(&buf[..10], b"helloworld");
    }
    
    #[test]
    fn test_stream_manager() {
        let mut mgr = StreamManager::new(true);
        
        let id1 = mgr.open_bidi();
        let id2 = mgr.open_bidi();
        
        assert_eq!(id1, 0);
        assert_eq!(id2, 4);
        assert_eq!(mgr.count(), 2);
    }
    
    #[test]
    fn test_stream_ids() {
        // Client-initiated bidirectional: 0, 4, 8...
        let mut mgr = StreamManager::new(true);
        assert_eq!(mgr.open_bidi(), 0);
        assert_eq!(mgr.open_uni(), 2);
        
        // Server-initiated bidirectional: 1, 5, 9...
        let mut mgr = StreamManager::new(false);
        assert_eq!(mgr.open_bidi(), 1);
        assert_eq!(mgr.open_uni(), 3);
    }
}
