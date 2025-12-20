//! HTTP/2 Support
//!
//! HTTP/2 protocol implementation.

use std::collections::HashMap;

/// HTTP/2 connection
#[derive(Debug)]
pub struct Http2Connection {
    pub streams: HashMap<u32, Http2Stream>,
    pub settings: Http2Settings,
    pub next_stream_id: u32,
    pub window_size: u32,
    pub hpack_encoder: HpackEncoder,
    pub hpack_decoder: HpackDecoder,
}

/// HTTP/2 stream
#[derive(Debug, Clone)]
pub struct Http2Stream {
    pub id: u32,
    pub state: StreamState,
    pub window_size: i32,
    pub headers: Vec<(String, String)>,
    pub data: Vec<u8>,
    pub priority: StreamPriority,
}

/// Stream state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamState {
    Idle,
    Open,
    HalfClosedLocal,
    HalfClosedRemote,
    Closed,
    ReservedLocal,
    ReservedRemote,
}

/// Stream priority
#[derive(Debug, Clone, Default)]
pub struct StreamPriority {
    pub dependency: u32,
    pub weight: u8,
    pub exclusive: bool,
}

/// HTTP/2 settings
#[derive(Debug, Clone)]
pub struct Http2Settings {
    pub header_table_size: u32,
    pub enable_push: bool,
    pub max_concurrent_streams: u32,
    pub initial_window_size: u32,
    pub max_frame_size: u32,
    pub max_header_list_size: u32,
}

impl Default for Http2Settings {
    fn default() -> Self {
        Self {
            header_table_size: 4096,
            enable_push: true,
            max_concurrent_streams: 100,
            initial_window_size: 65535,
            max_frame_size: 16384,
            max_header_list_size: 8192,
        }
    }
}

/// HTTP/2 frame types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    Data = 0,
    Headers = 1,
    Priority = 2,
    RstStream = 3,
    Settings = 4,
    PushPromise = 5,
    Ping = 6,
    GoAway = 7,
    WindowUpdate = 8,
    Continuation = 9,
}

/// HTTP/2 frame
#[derive(Debug, Clone)]
pub struct Http2Frame {
    pub frame_type: FrameType,
    pub flags: u8,
    pub stream_id: u32,
    pub payload: Vec<u8>,
}

impl Http2Frame {
    pub fn new(frame_type: FrameType, stream_id: u32) -> Self {
        Self {
            frame_type,
            flags: 0,
            stream_id,
            payload: Vec::new(),
        }
    }
    
    /// Check if END_STREAM flag is set
    pub fn is_end_stream(&self) -> bool {
        self.flags & 0x01 != 0
    }
    
    /// Check if END_HEADERS flag is set
    pub fn is_end_headers(&self) -> bool {
        self.flags & 0x04 != 0
    }
}

/// HPACK encoder (simplified)
#[derive(Debug, Default)]
pub struct HpackEncoder {
    dynamic_table: Vec<(String, String)>,
    max_size: usize,
}

impl HpackEncoder {
    pub fn new() -> Self {
        Self {
            dynamic_table: Vec::new(),
            max_size: 4096,
        }
    }
    
    /// Encode headers
    pub fn encode(&mut self, headers: &[(String, String)]) -> Vec<u8> {
        let mut result = Vec::new();
        
        for (name, value) in headers {
            // Simplified: literal header without indexing
            result.push(0x00);
            self.encode_string(&mut result, name);
            self.encode_string(&mut result, value);
        }
        
        result
    }
    
    fn encode_string(&self, buf: &mut Vec<u8>, s: &str) {
        // Length prefix (7-bit integer)
        buf.push(s.len() as u8);
        buf.extend_from_slice(s.as_bytes());
    }
}

/// HPACK decoder (simplified)
#[derive(Debug, Default)]
pub struct HpackDecoder {
    dynamic_table: Vec<(String, String)>,
}

impl HpackDecoder {
    pub fn new() -> Self { Self::default() }
    
    /// Decode headers
    pub fn decode(&mut self, _data: &[u8]) -> Vec<(String, String)> {
        // Simplified: would parse HPACK format
        Vec::new()
    }
}

impl Http2Connection {
    pub fn new() -> Self {
        Self {
            streams: HashMap::new(),
            settings: Http2Settings::default(),
            next_stream_id: 1,
            window_size: 65535,
            hpack_encoder: HpackEncoder::new(),
            hpack_decoder: HpackDecoder::new(),
        }
    }
    
    /// Create new stream
    pub fn create_stream(&mut self) -> u32 {
        let id = self.next_stream_id;
        self.next_stream_id += 2; // Client uses odd IDs
        
        let stream = Http2Stream {
            id,
            state: StreamState::Idle,
            window_size: self.settings.initial_window_size as i32,
            headers: Vec::new(),
            data: Vec::new(),
            priority: StreamPriority::default(),
        };
        
        self.streams.insert(id, stream);
        id
    }
    
    /// Send headers
    pub fn send_headers(&mut self, stream_id: u32, headers: Vec<(String, String)>, end_stream: bool) -> Http2Frame {
        let payload = self.hpack_encoder.encode(&headers);
        
        if let Some(stream) = self.streams.get_mut(&stream_id) {
            stream.state = if end_stream { StreamState::HalfClosedLocal } else { StreamState::Open };
            stream.headers = headers;
        }
        
        let mut frame = Http2Frame::new(FrameType::Headers, stream_id);
        frame.payload = payload;
        frame.flags = 0x04; // END_HEADERS
        if end_stream { frame.flags |= 0x01; }
        frame
    }
    
    /// Send data
    pub fn send_data(&mut self, stream_id: u32, data: Vec<u8>, end_stream: bool) -> Http2Frame {
        if let Some(stream) = self.streams.get_mut(&stream_id) {
            if end_stream {
                stream.state = StreamState::HalfClosedLocal;
            }
        }
        
        let mut frame = Http2Frame::new(FrameType::Data, stream_id);
        frame.payload = data;
        if end_stream { frame.flags |= 0x01; }
        frame
    }
    
    /// Update window size
    pub fn update_window(&mut self, stream_id: u32, increment: u32) -> Http2Frame {
        if stream_id == 0 {
            self.window_size += increment;
        } else if let Some(stream) = self.streams.get_mut(&stream_id) {
            stream.window_size += increment as i32;
        }
        
        let mut frame = Http2Frame::new(FrameType::WindowUpdate, stream_id);
        frame.payload = increment.to_be_bytes().to_vec();
        frame
    }
}

impl Default for Http2Connection {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_http2_connection() {
        let mut conn = Http2Connection::new();
        let stream_id = conn.create_stream();
        
        assert_eq!(stream_id, 1);
        assert_eq!(conn.next_stream_id, 3);
    }
    
    #[test]
    fn test_send_headers() {
        let mut conn = Http2Connection::new();
        let stream_id = conn.create_stream();
        
        let headers = vec![
            (":method".to_string(), "GET".to_string()),
            (":path".to_string(), "/".to_string()),
        ];
        
        let frame = conn.send_headers(stream_id, headers, false);
        assert_eq!(frame.frame_type, FrameType::Headers);
    }
}
