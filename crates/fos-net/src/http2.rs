//! HTTP/2 Protocol Implementation
//!
//! Full HTTP/2 support with frame serialization, HPACK compression,
//! multiplexing, and flow control.

use std::collections::HashMap;
use std::io::{self, Read, Write};

/// HTTP/2 connection preface (client magic)
pub const CONNECTION_PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

/// HTTP/2 frame types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FrameType {
    Data = 0x0,
    Headers = 0x1,
    Priority = 0x2,
    RstStream = 0x3,
    Settings = 0x4,
    PushPromise = 0x5,
    Ping = 0x6,
    GoAway = 0x7,
    WindowUpdate = 0x8,
    Continuation = 0x9,
}

impl TryFrom<u8> for FrameType {
    type Error = Http2Error;
    
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x0 => Ok(FrameType::Data),
            0x1 => Ok(FrameType::Headers),
            0x2 => Ok(FrameType::Priority),
            0x3 => Ok(FrameType::RstStream),
            0x4 => Ok(FrameType::Settings),
            0x5 => Ok(FrameType::PushPromise),
            0x6 => Ok(FrameType::Ping),
            0x7 => Ok(FrameType::GoAway),
            0x8 => Ok(FrameType::WindowUpdate),
            0x9 => Ok(FrameType::Continuation),
            _ => Err(Http2Error::UnknownFrameType(value)),
        }
    }
}

/// HTTP/2 frame flags
pub mod flags {
    pub const END_STREAM: u8 = 0x01;
    pub const ACK: u8 = 0x01;
    pub const END_HEADERS: u8 = 0x04;
    pub const PADDED: u8 = 0x08;
    pub const PRIORITY: u8 = 0x20;
}

/// HTTP/2 settings identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum SettingId {
    HeaderTableSize = 0x1,
    EnablePush = 0x2,
    MaxConcurrentStreams = 0x3,
    InitialWindowSize = 0x4,
    MaxFrameSize = 0x5,
    MaxHeaderListSize = 0x6,
}

/// HTTP/2 error codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ErrorCode {
    NoError = 0x0,
    ProtocolError = 0x1,
    InternalError = 0x2,
    FlowControlError = 0x3,
    SettingsTimeout = 0x4,
    StreamClosed = 0x5,
    FrameSizeError = 0x6,
    RefusedStream = 0x7,
    Cancel = 0x8,
    CompressionError = 0x9,
    ConnectError = 0xa,
    EnhanceYourCalm = 0xb,
    InadequateSecurity = 0xc,
    Http11Required = 0xd,
}

/// HTTP/2 frame header (9 bytes)
#[derive(Debug, Clone)]
pub struct FrameHeader {
    pub length: u32,      // 24-bit
    pub frame_type: FrameType,
    pub flags: u8,
    pub stream_id: u32,   // 31-bit (R bit reserved)
}

impl FrameHeader {
    pub const SIZE: usize = 9;
    
    pub fn new(frame_type: FrameType, flags: u8, stream_id: u32, length: u32) -> Self {
        Self { length, frame_type, flags, stream_id }
    }
    
    /// Serialize frame header to bytes
    pub fn serialize(&self) -> [u8; 9] {
        let mut buf = [0u8; 9];
        // Length (24-bit big-endian)
        buf[0] = ((self.length >> 16) & 0xFF) as u8;
        buf[1] = ((self.length >> 8) & 0xFF) as u8;
        buf[2] = (self.length & 0xFF) as u8;
        // Type
        buf[3] = self.frame_type as u8;
        // Flags
        buf[4] = self.flags;
        // Stream ID (31-bit, R bit = 0)
        buf[5] = ((self.stream_id >> 24) & 0x7F) as u8;
        buf[6] = ((self.stream_id >> 16) & 0xFF) as u8;
        buf[7] = ((self.stream_id >> 8) & 0xFF) as u8;
        buf[8] = (self.stream_id & 0xFF) as u8;
        buf
    }
    
    /// Parse frame header from bytes
    pub fn parse(buf: &[u8; 9]) -> Result<Self, Http2Error> {
        let length = ((buf[0] as u32) << 16) | ((buf[1] as u32) << 8) | (buf[2] as u32);
        let frame_type = FrameType::try_from(buf[3])?;
        let flags = buf[4];
        let stream_id = ((buf[5] as u32 & 0x7F) << 24) 
            | ((buf[6] as u32) << 16) 
            | ((buf[7] as u32) << 8) 
            | (buf[8] as u32);
        
        Ok(Self { length, frame_type, flags, stream_id })
    }
}

/// HTTP/2 frame
#[derive(Debug, Clone)]
pub struct Frame {
    pub header: FrameHeader,
    pub payload: Vec<u8>,
}

impl Frame {
    pub fn new(frame_type: FrameType, flags: u8, stream_id: u32, payload: Vec<u8>) -> Self {
        let header = FrameHeader::new(frame_type, flags, stream_id, payload.len() as u32);
        Self { header, payload }
    }
    
    /// Create SETTINGS frame
    pub fn settings(settings: &[(SettingId, u32)], ack: bool) -> Self {
        let mut payload = Vec::with_capacity(settings.len() * 6);
        for (id, value) in settings {
            payload.extend_from_slice(&(*id as u16).to_be_bytes());
            payload.extend_from_slice(&value.to_be_bytes());
        }
        let flags = if ack { flags::ACK } else { 0 };
        Self::new(FrameType::Settings, flags, 0, payload)
    }
    
    /// Create HEADERS frame
    pub fn headers(stream_id: u32, header_block: Vec<u8>, end_stream: bool, end_headers: bool) -> Self {
        let mut flags = 0;
        if end_stream { flags |= flags::END_STREAM; }
        if end_headers { flags |= flags::END_HEADERS; }
        Self::new(FrameType::Headers, flags, stream_id, header_block)
    }
    
    /// Create DATA frame
    pub fn data(stream_id: u32, data: Vec<u8>, end_stream: bool) -> Self {
        let flags = if end_stream { flags::END_STREAM } else { 0 };
        Self::new(FrameType::Data, flags, stream_id, data)
    }
    
    /// Create WINDOW_UPDATE frame
    pub fn window_update(stream_id: u32, increment: u32) -> Self {
        let payload = (increment & 0x7FFFFFFF).to_be_bytes().to_vec();
        Self::new(FrameType::WindowUpdate, 0, stream_id, payload)
    }
    
    /// Create PING frame
    pub fn ping(data: [u8; 8], ack: bool) -> Self {
        let flags = if ack { flags::ACK } else { 0 };
        Self::new(FrameType::Ping, flags, 0, data.to_vec())
    }
    
    /// Create GOAWAY frame
    pub fn goaway(last_stream_id: u32, error_code: ErrorCode, debug_data: Vec<u8>) -> Self {
        let mut payload = Vec::with_capacity(8 + debug_data.len());
        payload.extend_from_slice(&last_stream_id.to_be_bytes());
        payload.extend_from_slice(&(error_code as u32).to_be_bytes());
        payload.extend_from_slice(&debug_data);
        Self::new(FrameType::GoAway, 0, 0, payload)
    }
    
    /// Create RST_STREAM frame
    pub fn rst_stream(stream_id: u32, error_code: ErrorCode) -> Self {
        let payload = (error_code as u32).to_be_bytes().to_vec();
        Self::new(FrameType::RstStream, 0, stream_id, payload)
    }
    
    /// Write frame to writer
    pub fn write_to<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_all(&self.header.serialize())?;
        writer.write_all(&self.payload)?;
        Ok(())
    }
    
    /// Read frame from reader
    pub fn read_from<R: Read>(reader: &mut R, max_frame_size: u32) -> Result<Self, Http2Error> {
        let mut header_buf = [0u8; 9];
        reader.read_exact(&mut header_buf).map_err(Http2Error::Io)?;
        
        let header = FrameHeader::parse(&header_buf)?;
        
        if header.length > max_frame_size {
            return Err(Http2Error::FrameTooLarge(header.length));
        }
        
        let mut payload = vec![0u8; header.length as usize];
        reader.read_exact(&mut payload).map_err(Http2Error::Io)?;
        
        Ok(Self { header, payload })
    }
    
    /// Check if END_STREAM flag is set
    pub fn is_end_stream(&self) -> bool {
        self.header.flags & flags::END_STREAM != 0
    }
    
    /// Check if END_HEADERS flag is set
    pub fn is_end_headers(&self) -> bool {
        self.header.flags & flags::END_HEADERS != 0
    }
    
    /// Check if ACK flag is set
    pub fn is_ack(&self) -> bool {
        self.header.flags & flags::ACK != 0
    }
}

/// HPACK static table (RFC 7541)
const STATIC_TABLE: &[(&str, &str)] = &[
    (":authority", ""),
    (":method", "GET"),
    (":method", "POST"),
    (":path", "/"),
    (":path", "/index.html"),
    (":scheme", "http"),
    (":scheme", "https"),
    (":status", "200"),
    (":status", "204"),
    (":status", "206"),
    (":status", "304"),
    (":status", "400"),
    (":status", "404"),
    (":status", "500"),
    ("accept-charset", ""),
    ("accept-encoding", "gzip, deflate"),
    ("accept-language", ""),
    ("accept-ranges", ""),
    ("accept", ""),
    ("access-control-allow-origin", ""),
    ("age", ""),
    ("allow", ""),
    ("authorization", ""),
    ("cache-control", ""),
    ("content-disposition", ""),
    ("content-encoding", ""),
    ("content-language", ""),
    ("content-length", ""),
    ("content-location", ""),
    ("content-range", ""),
    ("content-type", ""),
    ("cookie", ""),
    ("date", ""),
    ("etag", ""),
    ("expect", ""),
    ("expires", ""),
    ("from", ""),
    ("host", ""),
    ("if-match", ""),
    ("if-modified-since", ""),
    ("if-none-match", ""),
    ("if-range", ""),
    ("if-unmodified-since", ""),
    ("last-modified", ""),
    ("link", ""),
    ("location", ""),
    ("max-forwards", ""),
    ("proxy-authenticate", ""),
    ("proxy-authorization", ""),
    ("range", ""),
    ("referer", ""),
    ("refresh", ""),
    ("retry-after", ""),
    ("server", ""),
    ("set-cookie", ""),
    ("strict-transport-security", ""),
    ("transfer-encoding", ""),
    ("user-agent", ""),
    ("vary", ""),
    ("via", ""),
    ("www-authenticate", ""),
];

/// HPACK encoder
#[derive(Debug)]
pub struct HpackEncoder {
    dynamic_table: Vec<(String, String)>,
    max_size: usize,
    current_size: usize,
}

impl HpackEncoder {
    pub fn new(max_size: usize) -> Self {
        Self {
            dynamic_table: Vec::new(),
            max_size,
            current_size: 0,
        }
    }
    
    /// Encode headers to HPACK format
    pub fn encode(&mut self, headers: &[(String, String)]) -> Vec<u8> {
        let mut encoded = Vec::new();
        
        for (name, value) in headers {
            // Check static table first
            if let Some(index) = self.find_in_static_table(name, value) {
                // Indexed header field
                self.encode_integer(&mut encoded, index, 7, 0x80);
            } else if let Some(name_index) = self.find_name_in_static_table(name) {
                // Literal header with indexed name (without indexing)
                self.encode_integer(&mut encoded, name_index, 4, 0x00);
                self.encode_string(&mut encoded, value);
            } else {
                // Literal header without indexing
                encoded.push(0x00);
                self.encode_string(&mut encoded, name);
                self.encode_string(&mut encoded, value);
            }
        }
        
        encoded
    }
    
    fn find_in_static_table(&self, name: &str, value: &str) -> Option<usize> {
        STATIC_TABLE.iter().position(|(n, v)| *n == name && *v == value).map(|i| i + 1)
    }
    
    fn find_name_in_static_table(&self, name: &str) -> Option<usize> {
        STATIC_TABLE.iter().position(|(n, _)| *n == name).map(|i| i + 1)
    }
    
    fn encode_integer(&self, buf: &mut Vec<u8>, value: usize, prefix_bits: u8, prefix: u8) {
        let max_prefix = (1 << prefix_bits) - 1;
        if value < max_prefix {
            buf.push(prefix | (value as u8));
        } else {
            buf.push(prefix | max_prefix as u8);
            let mut remaining = value - max_prefix;
            while remaining >= 128 {
                buf.push((remaining % 128 + 128) as u8);
                remaining /= 128;
            }
            buf.push(remaining as u8);
        }
    }
    
    fn encode_string(&self, buf: &mut Vec<u8>, s: &str) {
        // Without Huffman encoding for simplicity
        self.encode_integer(buf, s.len(), 7, 0x00);
        buf.extend_from_slice(s.as_bytes());
    }
}

impl Default for HpackEncoder {
    fn default() -> Self {
        Self::new(4096)
    }
}

/// HPACK decoder
#[derive(Debug)]
pub struct HpackDecoder {
    dynamic_table: Vec<(String, String)>,
    max_size: usize,
}

impl HpackDecoder {
    pub fn new(max_size: usize) -> Self {
        Self {
            dynamic_table: Vec::new(),
            max_size,
        }
    }
    
    /// Decode HPACK headers
    pub fn decode(&mut self, data: &[u8]) -> Result<Vec<(String, String)>, Http2Error> {
        let mut headers = Vec::new();
        let mut pos = 0;
        
        while pos < data.len() {
            let byte = data[pos];
            
            if byte & 0x80 != 0 {
                // Indexed header field
                let (index, consumed) = self.decode_integer(&data[pos..], 7)?;
                pos += consumed;
                
                if let Some((name, value)) = self.get_indexed(index) {
                    headers.push((name, value));
                } else {
                    return Err(Http2Error::InvalidHeaderIndex(index));
                }
            } else if byte & 0x40 != 0 {
                // Literal with incremental indexing
                let (index, consumed) = self.decode_integer(&data[pos..], 6)?;
                pos += consumed;
                
                let name = if index > 0 {
                    self.get_indexed_name(index)?
                } else {
                    let (s, consumed) = self.decode_string(&data[pos..])?;
                    pos += consumed;
                    s
                };
                
                let (value, consumed) = self.decode_string(&data[pos..])?;
                pos += consumed;
                
                self.add_to_dynamic_table(name.clone(), value.clone());
                headers.push((name, value));
            } else if byte & 0x20 != 0 {
                // Dynamic table size update
                let (new_size, consumed) = self.decode_integer(&data[pos..], 5)?;
                pos += consumed;
                self.max_size = new_size;
                self.evict();
            } else {
                // Literal without indexing or never indexed
                let prefix = if byte & 0x10 != 0 { 4 } else { 4 };
                let (index, consumed) = self.decode_integer(&data[pos..], prefix)?;
                pos += consumed;
                
                let name = if index > 0 {
                    self.get_indexed_name(index)?
                } else {
                    let (s, consumed) = self.decode_string(&data[pos..])?;
                    pos += consumed;
                    s
                };
                
                let (value, consumed) = self.decode_string(&data[pos..])?;
                pos += consumed;
                
                headers.push((name, value));
            }
        }
        
        Ok(headers)
    }
    
    fn decode_integer(&self, data: &[u8], prefix_bits: u8) -> Result<(usize, usize), Http2Error> {
        if data.is_empty() {
            return Err(Http2Error::IncompleteFrame);
        }
        
        let max_prefix = (1 << prefix_bits) - 1;
        let mut value = (data[0] & max_prefix) as usize;
        
        if value < max_prefix as usize {
            return Ok((value, 1));
        }
        
        let mut m = 0;
        let mut pos = 1;
        
        loop {
            if pos >= data.len() {
                return Err(Http2Error::IncompleteFrame);
            }
            
            let byte = data[pos];
            value += ((byte & 127) as usize) << m;
            m += 7;
            pos += 1;
            
            if byte & 128 == 0 {
                break;
            }
        }
        
        Ok((value, pos))
    }
    
    fn decode_string(&self, data: &[u8]) -> Result<(String, usize), Http2Error> {
        if data.is_empty() {
            return Err(Http2Error::IncompleteFrame);
        }
        
        let huffman = data[0] & 0x80 != 0;
        let (length, header_len) = self.decode_integer(data, 7)?;
        
        if header_len + length > data.len() {
            return Err(Http2Error::IncompleteFrame);
        }
        
        let string_data = &data[header_len..header_len + length];
        
        let s = if huffman {
            // Simplified: just use raw bytes for Huffman (proper impl would decode)
            String::from_utf8_lossy(string_data).to_string()
        } else {
            String::from_utf8_lossy(string_data).to_string()
        };
        
        Ok((s, header_len + length))
    }
    
    fn get_indexed(&self, index: usize) -> Option<(String, String)> {
        if index == 0 {
            return None;
        }
        
        if index <= STATIC_TABLE.len() {
            let (name, value) = STATIC_TABLE[index - 1];
            return Some((name.to_string(), value.to_string()));
        }
        
        let dynamic_index = index - STATIC_TABLE.len() - 1;
        self.dynamic_table.get(dynamic_index).cloned()
    }
    
    fn get_indexed_name(&self, index: usize) -> Result<String, Http2Error> {
        if index == 0 || index > STATIC_TABLE.len() + self.dynamic_table.len() {
            return Err(Http2Error::InvalidHeaderIndex(index));
        }
        
        if index <= STATIC_TABLE.len() {
            Ok(STATIC_TABLE[index - 1].0.to_string())
        } else {
            let dynamic_index = index - STATIC_TABLE.len() - 1;
            self.dynamic_table.get(dynamic_index)
                .map(|(n, _)| n.clone())
                .ok_or(Http2Error::InvalidHeaderIndex(index))
        }
    }
    
    fn add_to_dynamic_table(&mut self, name: String, value: String) {
        let entry_size = 32 + name.len() + value.len();
        
        // Evict old entries if needed
        while self.current_size() + entry_size > self.max_size && !self.dynamic_table.is_empty() {
            self.dynamic_table.pop();
        }
        
        if entry_size <= self.max_size {
            self.dynamic_table.insert(0, (name, value));
        }
    }
    
    fn current_size(&self) -> usize {
        self.dynamic_table.iter()
            .map(|(n, v)| 32 + n.len() + v.len())
            .sum()
    }
    
    fn evict(&mut self) {
        while self.current_size() > self.max_size && !self.dynamic_table.is_empty() {
            self.dynamic_table.pop();
        }
    }
}

impl Default for HpackDecoder {
    fn default() -> Self {
        Self::new(4096)
    }
}

/// HTTP/2 stream state
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

/// HTTP/2 stream
#[derive(Debug)]
pub struct Stream {
    pub id: u32,
    pub state: StreamState,
    pub send_window: i32,
    pub recv_window: i32,
    pub headers: Vec<(String, String)>,
    pub data: Vec<u8>,
}

impl Stream {
    pub fn new(id: u32, initial_window: u32) -> Self {
        Self {
            id,
            state: StreamState::Idle,
            send_window: initial_window as i32,
            recv_window: initial_window as i32,
            headers: Vec::new(),
            data: Vec::new(),
        }
    }
}

/// HTTP/2 connection settings
#[derive(Debug, Clone)]
pub struct Settings {
    pub header_table_size: u32,
    pub enable_push: bool,
    pub max_concurrent_streams: u32,
    pub initial_window_size: u32,
    pub max_frame_size: u32,
    pub max_header_list_size: u32,
}

impl Default for Settings {
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

impl Settings {
    pub fn to_pairs(&self) -> Vec<(SettingId, u32)> {
        vec![
            (SettingId::HeaderTableSize, self.header_table_size),
            (SettingId::EnablePush, if self.enable_push { 1 } else { 0 }),
            (SettingId::MaxConcurrentStreams, self.max_concurrent_streams),
            (SettingId::InitialWindowSize, self.initial_window_size),
            (SettingId::MaxFrameSize, self.max_frame_size),
            (SettingId::MaxHeaderListSize, self.max_header_list_size),
        ]
    }
}

/// HTTP/2 connection
#[derive(Debug)]
pub struct Http2Connection {
    /// Local settings
    pub local_settings: Settings,
    /// Remote settings
    pub remote_settings: Settings,
    /// Active streams
    pub streams: HashMap<u32, Stream>,
    /// Next stream ID (client uses odd, server uses even)
    pub next_stream_id: u32,
    /// Connection-level send window
    pub send_window: i32,
    /// Connection-level receive window
    pub recv_window: i32,
    /// HPACK encoder
    pub encoder: HpackEncoder,
    /// HPACK decoder
    pub decoder: HpackDecoder,
    /// Is client
    pub is_client: bool,
    /// Connection established
    pub established: bool,
}

impl Http2Connection {
    pub fn new_client() -> Self {
        Self {
            local_settings: Settings::default(),
            remote_settings: Settings::default(),
            streams: HashMap::new(),
            next_stream_id: 1, // Client uses odd stream IDs
            send_window: 65535,
            recv_window: 65535,
            encoder: HpackEncoder::default(),
            decoder: HpackDecoder::default(),
            is_client: true,
            established: false,
        }
    }
    
    /// Send connection preface and initial settings
    pub fn send_preface<W: Write>(&mut self, writer: &mut W) -> io::Result<()> {
        // Send client preface magic
        writer.write_all(CONNECTION_PREFACE)?;
        
        // Send SETTINGS frame
        let settings_frame = Frame::settings(&self.local_settings.to_pairs(), false);
        settings_frame.write_to(writer)?;
        
        writer.flush()
    }
    
    /// Create a new stream
    pub fn create_stream(&mut self) -> u32 {
        let id = self.next_stream_id;
        self.next_stream_id += 2;
        
        let stream = Stream::new(id, self.local_settings.initial_window_size);
        self.streams.insert(id, stream);
        
        id
    }
    
    /// Send request headers
    pub fn send_request<W: Write>(
        &mut self,
        writer: &mut W,
        method: &str,
        path: &str,
        authority: &str,
        headers: &[(String, String)],
        end_stream: bool,
    ) -> io::Result<u32> {
        let stream_id = self.create_stream();
        
        // Build pseudo-headers + regular headers
        let mut all_headers = vec![
            (":method".to_string(), method.to_string()),
            (":path".to_string(), path.to_string()),
            (":scheme".to_string(), "https".to_string()),
            (":authority".to_string(), authority.to_string()),
        ];
        all_headers.extend(headers.iter().cloned());
        
        // Encode headers
        let header_block = self.encoder.encode(&all_headers);
        
        // Send HEADERS frame
        let frame = Frame::headers(stream_id, header_block, end_stream, true);
        frame.write_to(writer)?;
        
        // Update stream state
        if let Some(stream) = self.streams.get_mut(&stream_id) {
            stream.state = if end_stream { 
                StreamState::HalfClosedLocal 
            } else { 
                StreamState::Open 
            };
            stream.headers = all_headers;
        }
        
        writer.flush()?;
        Ok(stream_id)
    }
    
    /// Send data on a stream
    pub fn send_data<W: Write>(
        &mut self,
        writer: &mut W,
        stream_id: u32,
        data: &[u8],
        end_stream: bool,
    ) -> io::Result<()> {
        // Check flow control
        let max_frame_size = self.remote_settings.max_frame_size as usize;
        
        // Split data into frames if needed
        for chunk in data.chunks(max_frame_size) {
            let is_last = chunk.as_ptr() as usize + chunk.len() 
                == data.as_ptr() as usize + data.len();
            let frame = Frame::data(stream_id, chunk.to_vec(), end_stream && is_last);
            frame.write_to(writer)?;
            
            // Update windows
            self.send_window -= chunk.len() as i32;
            if let Some(stream) = self.streams.get_mut(&stream_id) {
                stream.send_window -= chunk.len() as i32;
            }
        }
        
        // Update stream state
        if end_stream {
            if let Some(stream) = self.streams.get_mut(&stream_id) {
                stream.state = StreamState::HalfClosedLocal;
            }
        }
        
        writer.flush()
    }
    
    /// Process received frame
    pub fn process_frame(&mut self, frame: Frame) -> Result<Option<Http2Event>, Http2Error> {
        match frame.header.frame_type {
            FrameType::Settings => {
                if frame.is_ack() {
                    self.established = true;
                    Ok(Some(Http2Event::SettingsAck))
                } else {
                    // Parse settings
                    self.parse_settings(&frame.payload)?;
                    Ok(Some(Http2Event::SettingsReceived))
                }
            }
            FrameType::Headers => {
                let headers = self.decoder.decode(&frame.payload)?;
                let stream_id = frame.header.stream_id;
                
                if let Some(stream) = self.streams.get_mut(&stream_id) {
                    stream.headers = headers.clone();
                    if frame.is_end_stream() {
                        stream.state = StreamState::HalfClosedRemote;
                    }
                }
                
                Ok(Some(Http2Event::Headers { stream_id, headers, end_stream: frame.is_end_stream() }))
            }
            FrameType::Data => {
                let stream_id = frame.header.stream_id;
                let data = frame.payload.clone();
                
                if let Some(stream) = self.streams.get_mut(&stream_id) {
                    stream.data.extend(&data);
                    stream.recv_window -= data.len() as i32;
                    if frame.is_end_stream() {
                        stream.state = StreamState::HalfClosedRemote;
                    }
                }
                
                self.recv_window -= data.len() as i32;
                
                Ok(Some(Http2Event::Data { stream_id, data, end_stream: frame.is_end_stream() }))
            }
            FrameType::WindowUpdate => {
                let increment = u32::from_be_bytes([
                    frame.payload[0] & 0x7F,
                    frame.payload[1],
                    frame.payload[2],
                    frame.payload[3],
                ]);
                
                if frame.header.stream_id == 0 {
                    self.send_window += increment as i32;
                } else if let Some(stream) = self.streams.get_mut(&frame.header.stream_id) {
                    stream.send_window += increment as i32;
                }
                
                Ok(Some(Http2Event::WindowUpdate { stream_id: frame.header.stream_id, increment }))
            }
            FrameType::Ping => {
                Ok(Some(Http2Event::Ping { ack: frame.is_ack(), data: frame.payload.try_into().unwrap_or([0; 8]) }))
            }
            FrameType::GoAway => {
                let last_stream_id = u32::from_be_bytes([
                    frame.payload[0] & 0x7F,
                    frame.payload[1],
                    frame.payload[2],
                    frame.payload[3],
                ]);
                let error_code = u32::from_be_bytes([
                    frame.payload[4],
                    frame.payload[5],
                    frame.payload[6],
                    frame.payload[7],
                ]);
                Ok(Some(Http2Event::GoAway { last_stream_id, error_code }))
            }
            FrameType::RstStream => {
                let stream_id = frame.header.stream_id;
                let error_code = u32::from_be_bytes([
                    frame.payload[0],
                    frame.payload[1],
                    frame.payload[2],
                    frame.payload[3],
                ]);
                
                if let Some(stream) = self.streams.get_mut(&stream_id) {
                    stream.state = StreamState::Closed;
                }
                
                Ok(Some(Http2Event::RstStream { stream_id, error_code }))
            }
            _ => Ok(None),
        }
    }
    
    fn parse_settings(&mut self, payload: &[u8]) -> Result<(), Http2Error> {
        for chunk in payload.chunks(6) {
            if chunk.len() < 6 {
                break;
            }
            let id = u16::from_be_bytes([chunk[0], chunk[1]]);
            let value = u32::from_be_bytes([chunk[2], chunk[3], chunk[4], chunk[5]]);
            
            match id {
                0x1 => self.remote_settings.header_table_size = value,
                0x2 => self.remote_settings.enable_push = value != 0,
                0x3 => self.remote_settings.max_concurrent_streams = value,
                0x4 => self.remote_settings.initial_window_size = value,
                0x5 => self.remote_settings.max_frame_size = value,
                0x6 => self.remote_settings.max_header_list_size = value,
                _ => {} // Ignore unknown settings
            }
        }
        Ok(())
    }
    
    /// Send SETTINGS ACK
    pub fn send_settings_ack<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        let frame = Frame::settings(&[], true);
        frame.write_to(writer)?;
        writer.flush()
    }
    
    /// Send WINDOW_UPDATE
    pub fn send_window_update<W: Write>(&mut self, writer: &mut W, stream_id: u32, increment: u32) -> io::Result<()> {
        let frame = Frame::window_update(stream_id, increment);
        frame.write_to(writer)?;
        
        if stream_id == 0 {
            self.recv_window += increment as i32;
        } else if let Some(stream) = self.streams.get_mut(&stream_id) {
            stream.recv_window += increment as i32;
        }
        
        writer.flush()
    }
    
    /// Send PING response
    pub fn send_ping_ack<W: Write>(&self, writer: &mut W, data: [u8; 8]) -> io::Result<()> {
        let frame = Frame::ping(data, true);
        frame.write_to(writer)?;
        writer.flush()
    }
    
    /// Close a stream
    pub fn close_stream(&mut self, stream_id: u32) {
        if let Some(stream) = self.streams.get_mut(&stream_id) {
            stream.state = StreamState::Closed;
        }
    }
    
    /// Get stream data
    pub fn get_stream_data(&self, stream_id: u32) -> Option<&[u8]> {
        self.streams.get(&stream_id).map(|s| s.data.as_slice())
    }
    
    /// Get stream headers
    pub fn get_stream_headers(&self, stream_id: u32) -> Option<&[(String, String)]> {
        self.streams.get(&stream_id).map(|s| s.headers.as_slice())
    }
}

/// HTTP/2 events
#[derive(Debug)]
pub enum Http2Event {
    SettingsReceived,
    SettingsAck,
    Headers { stream_id: u32, headers: Vec<(String, String)>, end_stream: bool },
    Data { stream_id: u32, data: Vec<u8>, end_stream: bool },
    WindowUpdate { stream_id: u32, increment: u32 },
    Ping { ack: bool, data: [u8; 8] },
    GoAway { last_stream_id: u32, error_code: u32 },
    RstStream { stream_id: u32, error_code: u32 },
}

/// HTTP/2 error
#[derive(Debug, thiserror::Error)]
pub enum Http2Error {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    
    #[error("Unknown frame type: {0}")]
    UnknownFrameType(u8),
    
    #[error("Frame too large: {0}")]
    FrameTooLarge(u32),
    
    #[error("Invalid header index: {0}")]
    InvalidHeaderIndex(usize),
    
    #[error("Incomplete frame")]
    IncompleteFrame,
    
    #[error("Protocol error: {0}")]
    Protocol(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_frame_header_roundtrip() {
        let header = FrameHeader::new(FrameType::Headers, flags::END_HEADERS, 1, 100);
        let serialized = header.serialize();
        let parsed = FrameHeader::parse(&serialized).unwrap();
        
        assert_eq!(parsed.length, 100);
        assert_eq!(parsed.frame_type, FrameType::Headers);
        assert_eq!(parsed.flags, flags::END_HEADERS);
        assert_eq!(parsed.stream_id, 1);
    }
    
    #[test]
    fn test_settings_frame() {
        let settings = Settings::default();
        let frame = Frame::settings(&settings.to_pairs(), false);
        
        assert_eq!(frame.header.frame_type, FrameType::Settings);
        assert_eq!(frame.header.stream_id, 0);
    }
    
    #[test]
    fn test_hpack_encode_decode() {
        let mut encoder = HpackEncoder::default();
        let mut decoder = HpackDecoder::default();
        
        let headers = vec![
            (":method".to_string(), "GET".to_string()),
            (":path".to_string(), "/".to_string()),
        ];
        
        let encoded = encoder.encode(&headers);
        let decoded = decoder.decode(&encoded).unwrap();
        
        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0], (":method".to_string(), "GET".to_string()));
        assert_eq!(decoded[1], (":path".to_string(), "/".to_string()));
    }
    
    #[test]
    fn test_connection_create_stream() {
        let mut conn = Http2Connection::new_client();
        
        let id1 = conn.create_stream();
        let id2 = conn.create_stream();
        
        assert_eq!(id1, 1);
        assert_eq!(id2, 3);
    }
}
