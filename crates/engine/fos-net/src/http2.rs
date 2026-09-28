//! HTTP/2 Protocol Implementation
//!
//! Full HTTP/2 support with frame serialization, HPACK compression,
//! multiplexing, and flow control.

use std::collections::HashMap;
use std::io::{self, Read, Write};

/// HTTP/2 connection preface (client magic)
pub const CONNECTION_PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

/// Initial flow-control window size defined by RFC 9113 §6.9.2
pub const DEFAULT_WINDOW_SIZE: u32 = 65_535;

/// Largest header block we accept across HEADERS + CONTINUATION frames
/// (guards against CONTINUATION floods)
const MAX_HEADER_BLOCK_SIZE: usize = 256 * 1024;

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

    /// Read the next frame from reader.
    ///
    /// Frames of unknown type are read and discarded, as RFC 9113 §4.1
    /// requires (servers send extension frames such as ORIGIN or
    /// PRIORITY_UPDATE that must not break the connection).
    pub fn read_from<R: Read>(reader: &mut R, max_frame_size: u32) -> Result<Self, Http2Error> {
        loop {
            let mut header_buf = [0u8; 9];
            reader.read_exact(&mut header_buf).map_err(Http2Error::Io)?;

            let length = u32::from_be_bytes([0, header_buf[0], header_buf[1], header_buf[2]]);
            if length > max_frame_size {
                return Err(Http2Error::FrameTooLarge(length));
            }

            let mut payload = vec![0u8; length as usize];
            reader.read_exact(&mut payload).map_err(Http2Error::Io)?;

            match FrameHeader::parse(&header_buf) {
                Ok(header) => return Ok(Self { header, payload }),
                Err(Http2Error::UnknownFrameType(_)) => continue,
                Err(e) => return Err(e),
            }
        }
    }

    /// Frame content with padding (and, for HEADERS, the priority block) removed.
    pub fn content(&self) -> Result<&[u8], Http2Error> {
        let mut data = &self.payload[..];
        let mut pad = 0usize;

        let paddable = matches!(
            self.header.frame_type,
            FrameType::Data | FrameType::Headers | FrameType::PushPromise
        );
        if paddable && self.header.flags & flags::PADDED != 0 {
            let (&pad_len, rest) = data.split_first()
                .ok_or_else(|| Http2Error::Protocol("missing pad length".into()))?;
            pad = pad_len as usize;
            data = rest;
        }

        if self.header.frame_type == FrameType::Headers && self.header.flags & flags::PRIORITY != 0 {
            // Stream dependency (4 bytes) + weight (1 byte)
            data = data.get(5..)
                .ok_or_else(|| Http2Error::Protocol("truncated priority block".into()))?;
        }

        if pad > data.len() {
            return Err(Http2Error::Protocol("padding exceeds frame payload".into()));
        }
        Ok(&data[..data.len() - pad])
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
            // HTTP/2 requires lowercase field names (RFC 9113 §8.2.1);
            // uppercase names make strict servers reset the stream.
            let lowered;
            let name: &str = if name.bytes().any(|b| b.is_ascii_uppercase()) {
                lowered = name.to_ascii_lowercase();
                &lowered
            } else {
                name
            };

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
        // Use Huffman coding whenever it is shorter (it usually is)
        let huffman_len = crate::hpack_huffman::encoded_len(s.as_bytes());
        if huffman_len < s.len() {
            self.encode_integer(buf, huffman_len, 7, 0x80);
            crate::hpack_huffman::encode(s.as_bytes(), buf);
        } else {
            self.encode_integer(buf, s.len(), 7, 0x00);
            buf.extend_from_slice(s.as_bytes());
        }
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
    /// Upper bound advertised via SETTINGS_HEADER_TABLE_SIZE
    limit: usize,
}

impl HpackDecoder {
    pub fn new(max_size: usize) -> Self {
        Self {
            dynamic_table: Vec::new(),
            max_size,
            limit: max_size,
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
                if new_size > self.limit {
                    return Err(Http2Error::Compression(format!(
                        "dynamic table size {} exceeds limit {}", new_size, self.limit
                    )));
                }
                self.max_size = new_size;
                self.evict();
            } else {
                // Literal without indexing (0000xxxx) or never indexed (0001xxxx)
                let (index, consumed) = self.decode_integer(&data[pos..], 4)?;
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

            // Reject integers that would overflow (a malicious peer could
            // otherwise trigger a shift-overflow panic)
            if m > 28 {
                return Err(Http2Error::Compression("HPACK integer overflow".into()));
            }

            let byte = data[pos];
            value = value.checked_add(((byte & 127) as usize) << m)
                .ok_or_else(|| Http2Error::Compression("HPACK integer overflow".into()))?;
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
            let decoded = crate::hpack_huffman::decode(string_data)
                .map_err(|e| Http2Error::Compression(e.to_string()))?;
            match String::from_utf8(decoded) {
                Ok(s) => s,
                Err(e) => String::from_utf8_lossy(e.as_bytes()).into_owned(),
            }
        } else {
            String::from_utf8_lossy(string_data).into_owned()
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
///
/// DATA payloads are handed to the caller through [`Http2Event::Data`]
/// rather than buffered here, so response bodies are held in memory once.
#[derive(Debug)]
pub struct Stream {
    pub id: u32,
    pub state: StreamState,
    pub send_window: i32,
    pub recv_window: i32,
    pub headers: Vec<(String, String)>,
}

impl Stream {
    pub fn new(id: u32, initial_window: u32) -> Self {
        Self {
            id,
            state: StreamState::Idle,
            send_window: initial_window as i32,
            recv_window: initial_window as i32,
            headers: Vec::new(),
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
    /// Protocol defaults (RFC 9113 §6.5.2), which apply to the peer until
    /// its SETTINGS frame arrives.
    fn default() -> Self {
        Self {
            header_table_size: 4096,
            enable_push: true,
            max_concurrent_streams: 100,
            initial_window_size: DEFAULT_WINDOW_SIZE,
            max_frame_size: 16384,
            max_header_list_size: 8192,
        }
    }
}

impl Settings {
    /// Settings advertised by this client.
    ///
    /// Server push is disabled (major browsers dropped it and pushed streams
    /// would only waste bandwidth and memory here), and the stream window is
    /// raised well above 64 KiB so documents download without stalling for a
    /// WINDOW_UPDATE round trip every 64 KiB.
    pub fn client() -> Self {
        Self {
            header_table_size: 4096,
            enable_push: false,
            max_concurrent_streams: 100,
            initial_window_size: 1024 * 1024,
            max_frame_size: 16384,
            max_header_list_size: MAX_HEADER_BLOCK_SIZE as u32,
        }
    }

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

/// Header block being assembled from HEADERS + CONTINUATION frames
#[derive(Debug)]
struct PendingHeaders {
    stream_id: u32,
    block: Vec<u8>,
    end_stream: bool,
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
    /// Connection-level receive window we keep topped up via WINDOW_UPDATE
    pub conn_window_target: u32,
    /// HPACK encoder
    pub encoder: HpackEncoder,
    /// HPACK decoder
    pub decoder: HpackDecoder,
    /// Is client
    pub is_client: bool,
    /// Connection established
    pub established: bool,
    /// Header block awaiting CONTINUATION frames
    pending_headers: Option<PendingHeaders>,
}

/// Headers that are meaningful only for a single HTTP/1.1 hop and are
/// forbidden in HTTP/2 requests (RFC 9113 §8.2.2). `host` is replaced by
/// the `:authority` pseudo-header.
fn is_connection_specific_header(name: &str) -> bool {
    matches!(
        name,
        "connection" | "keep-alive" | "proxy-connection" | "transfer-encoding" | "upgrade" | "host"
    )
}

impl Http2Connection {
    pub fn new_client() -> Self {
        let local_settings = Settings::client();
        Self {
            decoder: HpackDecoder::new(local_settings.header_table_size as usize),
            local_settings,
            remote_settings: Settings::default(),
            streams: HashMap::new(),
            next_stream_id: 1, // Client uses odd stream IDs
            send_window: DEFAULT_WINDOW_SIZE as i32,
            recv_window: DEFAULT_WINDOW_SIZE as i32,
            conn_window_target: 2 * 1024 * 1024,
            encoder: HpackEncoder::default(),
            is_client: true,
            established: false,
            pending_headers: None,
        }
    }

    /// Send connection preface and initial settings
    pub fn send_preface<W: Write>(&mut self, writer: &mut W) -> io::Result<()> {
        // Send client preface magic
        writer.write_all(CONNECTION_PREFACE)?;

        // Send SETTINGS frame
        let settings_frame = Frame::settings(&self.local_settings.to_pairs(), false);
        settings_frame.write_to(writer)?;

        // The connection window can only be changed with WINDOW_UPDATE;
        // grow it immediately so it does not throttle the stream windows.
        let increment = self.conn_window_target.saturating_sub(DEFAULT_WINDOW_SIZE);
        if increment > 0 {
            Frame::window_update(0, increment).write_to(writer)?;
            self.recv_window += increment as i32;
        }

        writer.flush()
    }

    /// Create a new stream
    pub fn create_stream(&mut self) -> u32 {
        let id = self.next_stream_id;
        self.next_stream_id += 2;

        let mut stream = Stream::new(id, self.local_settings.initial_window_size);
        stream.send_window = self.remote_settings.initial_window_size as i32;
        self.streams.insert(id, stream);

        id
    }

    /// Send request headers
    ///
    /// Header names are lowercased and connection-specific headers are
    /// dropped, since either would make the request malformed in HTTP/2.
    /// Large header blocks are split into CONTINUATION frames.
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
        for (name, value) in headers {
            let name = name.to_ascii_lowercase();
            if name.starts_with(':') || is_connection_specific_header(&name) {
                continue;
            }
            // TE is only allowed with the value "trailers"
            if name == "te" && !value.eq_ignore_ascii_case("trailers") {
                continue;
            }
            all_headers.push((name, value.clone()));
        }

        // Encode headers
        let header_block = self.encoder.encode(&all_headers);

        // Send HEADERS (+ CONTINUATION) frames
        let max_frame = (self.remote_settings.max_frame_size as usize).max(1);
        let mut chunks = header_block.chunks(max_frame).peekable();
        let first = chunks.next().unwrap_or(&[]);
        Frame::headers(stream_id, first.to_vec(), end_stream, chunks.peek().is_none())
            .write_to(writer)?;
        while let Some(chunk) = chunks.next() {
            let flags = if chunks.peek().is_none() { flags::END_HEADERS } else { 0 };
            Frame::new(FrameType::Continuation, flags, stream_id, chunk.to_vec()).write_to(writer)?;
        }

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
        let max_frame_size = (self.remote_settings.max_frame_size as usize).max(1);

        // Split data into frames if needed
        let mut chunks = data.chunks(max_frame_size).peekable();
        if chunks.peek().is_none() && end_stream {
            Frame::data(stream_id, Vec::new(), true).write_to(writer)?;
        }
        while let Some(chunk) = chunks.next() {
            let is_last = chunks.peek().is_none();
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
        // A header block split across frames must be followed immediately
        // by its CONTINUATION frames (RFC 9113 §6.10)
        if let Some(pending) = &self.pending_headers {
            if frame.header.frame_type != FrameType::Continuation
                || frame.header.stream_id != pending.stream_id
            {
                return Err(Http2Error::Protocol("expected CONTINUATION frame".into()));
            }
        }

        match frame.header.frame_type {
            FrameType::Settings => {
                if frame.is_ack() {
                    self.established = true;
                    Ok(Some(Http2Event::SettingsAck))
                } else {
                    self.apply_remote_settings(&frame.payload)?;
                    Ok(Some(Http2Event::SettingsReceived))
                }
            }
            FrameType::Headers => {
                let stream_id = frame.header.stream_id;
                let end_stream = frame.is_end_stream();
                let block = frame.content()?;

                if frame.is_end_headers() {
                    self.finish_headers(stream_id, block, end_stream).map(Some)
                } else {
                    self.pending_headers = Some(PendingHeaders {
                        stream_id,
                        block: block.to_vec(),
                        end_stream,
                    });
                    Ok(None)
                }
            }
            FrameType::Continuation => {
                let mut pending = self.pending_headers.take()
                    .ok_or_else(|| Http2Error::Protocol("unexpected CONTINUATION frame".into()))?;
                pending.block.extend_from_slice(&frame.payload);
                if pending.block.len() > MAX_HEADER_BLOCK_SIZE {
                    return Err(Http2Error::Protocol("header block too large".into()));
                }

                if frame.is_end_headers() {
                    self.finish_headers(pending.stream_id, &pending.block, pending.end_stream).map(Some)
                } else {
                    self.pending_headers = Some(pending);
                    Ok(None)
                }
            }
            FrameType::Data => {
                let stream_id = frame.header.stream_id;
                let end_stream = frame.is_end_stream();
                // The whole payload, padding included, counts against flow control
                let flow_len = frame.payload.len() as i32;

                self.recv_window -= flow_len;
                if let Some(stream) = self.streams.get_mut(&stream_id) {
                    stream.recv_window -= flow_len;
                    if end_stream {
                        stream.state = StreamState::HalfClosedRemote;
                    }
                }

                let data = if frame.header.flags & flags::PADDED != 0 {
                    frame.content()?.to_vec()
                } else {
                    frame.payload
                };

                Ok(Some(Http2Event::Data { stream_id, data, end_stream }))
            }
            FrameType::WindowUpdate => {
                let increment = read_u31(&frame.payload)
                    .ok_or_else(|| Http2Error::Protocol("malformed WINDOW_UPDATE".into()))?;

                if frame.header.stream_id == 0 {
                    self.send_window = self.send_window.saturating_add(increment as i32);
                } else if let Some(stream) = self.streams.get_mut(&frame.header.stream_id) {
                    stream.send_window = stream.send_window.saturating_add(increment as i32);
                }

                Ok(Some(Http2Event::WindowUpdate { stream_id: frame.header.stream_id, increment }))
            }
            FrameType::Ping => {
                let data: [u8; 8] = frame.payload.as_slice().try_into()
                    .map_err(|_| Http2Error::Protocol("PING payload must be 8 bytes".into()))?;
                Ok(Some(Http2Event::Ping { ack: frame.is_ack(), data }))
            }
            FrameType::GoAway => {
                if frame.payload.len() < 8 {
                    return Err(Http2Error::Protocol("malformed GOAWAY".into()));
                }
                let last_stream_id = read_u31(&frame.payload[..4]).unwrap_or(0);
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
                let error_code = match frame.payload.as_slice() {
                    [a, b, c, d] => u32::from_be_bytes([*a, *b, *c, *d]),
                    _ => return Err(Http2Error::Protocol("malformed RST_STREAM".into())),
                };

                if let Some(stream) = self.streams.get_mut(&stream_id) {
                    stream.state = StreamState::Closed;
                }

                Ok(Some(Http2Event::RstStream { stream_id, error_code }))
            }
            FrameType::PushPromise if !self.local_settings.enable_push => {
                Err(Http2Error::Protocol("PUSH_PROMISE received with push disabled".into()))
            }
            _ => Ok(None),
        }
    }

    /// Decode a complete header block and update stream state
    fn finish_headers(&mut self, stream_id: u32, block: &[u8], end_stream: bool) -> Result<Http2Event, Http2Error> {
        // Every header block must be decoded, even for unknown streams, to
        // keep the HPACK dynamic table in sync with the peer
        let headers = self.decoder.decode(block)?;

        if let Some(stream) = self.streams.get_mut(&stream_id) {
            stream.headers = headers.clone();
            if end_stream {
                stream.state = StreamState::HalfClosedRemote;
            }
        }

        Ok(Http2Event::Headers { stream_id, headers, end_stream })
    }

    fn apply_remote_settings(&mut self, payload: &[u8]) -> Result<(), Http2Error> {
        if payload.len() % 6 != 0 {
            return Err(Http2Error::Protocol("SETTINGS length not a multiple of 6".into()));
        }

        for chunk in payload.chunks_exact(6) {
            let id = u16::from_be_bytes([chunk[0], chunk[1]]);
            let value = u32::from_be_bytes([chunk[2], chunk[3], chunk[4], chunk[5]]);

            match id {
                0x1 => self.remote_settings.header_table_size = value,
                0x2 => self.remote_settings.enable_push = value != 0,
                0x3 => self.remote_settings.max_concurrent_streams = value,
                0x4 => {
                    if value > 0x7FFF_FFFF {
                        return Err(Http2Error::Protocol("initial window size too large".into()));
                    }
                    // Changing the initial window adjusts every open stream (§6.9.2)
                    let delta = value as i64 - self.remote_settings.initial_window_size as i64;
                    for stream in self.streams.values_mut() {
                        stream.send_window = (stream.send_window as i64 + delta)
                            .clamp(i32::MIN as i64, i32::MAX as i64) as i32;
                    }
                    self.remote_settings.initial_window_size = value;
                }
                0x5 => {
                    if !(16_384..=16_777_215).contains(&value) {
                        return Err(Http2Error::Protocol("invalid max frame size".into()));
                    }
                    self.remote_settings.max_frame_size = value;
                }
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

    /// Hand consumed receive capacity back to the peer.
    ///
    /// Without this the peer stops sending once the initial window is used
    /// up, so every response larger than the window would stall. Updates are
    /// batched: a WINDOW_UPDATE is sent only once a window has dropped below
    /// half of its target, which keeps control-frame overhead negligible.
    pub fn replenish_windows<W: Write>(&mut self, writer: &mut W, stream_id: u32) -> io::Result<()> {
        let mut wrote = false;

        let conn_target = self.conn_window_target as i32;
        if self.recv_window < conn_target / 2 {
            let increment = (conn_target - self.recv_window) as u32;
            Frame::window_update(0, increment).write_to(writer)?;
            self.recv_window += increment as i32;
            wrote = true;
        }

        let stream_target = self.local_settings.initial_window_size as i32;
        if let Some(stream) = self.streams.get_mut(&stream_id) {
            let receiving = !matches!(stream.state, StreamState::HalfClosedRemote | StreamState::Closed);
            if receiving && stream.recv_window < stream_target / 2 {
                let increment = (stream_target - stream.recv_window) as u32;
                Frame::window_update(stream_id, increment).write_to(writer)?;
                stream.recv_window += increment as i32;
                wrote = true;
            }
        }

        if wrote {
            writer.flush()?;
        }
        Ok(())
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

    /// Remove a finished stream and release its state
    pub fn remove_stream(&mut self, stream_id: u32) -> Option<Stream> {
        self.streams.remove(&stream_id)
    }

    /// Get stream headers
    pub fn get_stream_headers(&self, stream_id: u32) -> Option<&[(String, String)]> {
        self.streams.get(&stream_id).map(|s| s.headers.as_slice())
    }
}

/// Read a 31-bit big-endian value (the reserved high bit is ignored)
fn read_u31(bytes: &[u8]) -> Option<u32> {
    match bytes {
        [a, b, c, d] => Some(u32::from_be_bytes([*a & 0x7F, *b, *c, *d])),
        _ => None,
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

    #[error("Header compression error: {0}")]
    Compression(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    fn pairs(headers: &[(&str, &str)]) -> Vec<(String, String)> {
        headers.iter().map(|(n, v)| (n.to_string(), v.to_string())).collect()
    }

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
    fn test_client_settings_disable_push() {
        let settings = Settings::client();
        assert!(!settings.enable_push);
        assert!(settings.initial_window_size > DEFAULT_WINDOW_SIZE);
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
    fn test_hpack_roundtrip_lowercases_and_huffman() {
        let mut encoder = HpackEncoder::default();
        let mut decoder = HpackDecoder::default();

        let headers = pairs(&[
            ("User-Agent", "fOS-Engine/0.1"),
            ("X-Custom-Header", "some value with spaces"),
            ("accept", "text/html,application/xhtml+xml"),
        ]);
        let decoded = decoder.decode(&encoder.encode(&headers)).unwrap();

        assert_eq!(decoded, pairs(&[
            ("user-agent", "fOS-Engine/0.1"),
            ("x-custom-header", "some value with spaces"),
            ("accept", "text/html,application/xhtml+xml"),
        ]));
    }

    #[test]
    fn test_hpack_rfc7541_c6_huffman_responses() {
        // RFC 7541 Appendix C.6: responses with Huffman coding
        let mut decoder = HpackDecoder::new(256);

        let first = hex(
            "4882 6402 5885 aec3 771a 4b61 96d0 7abe 9410 54d4 44a8 2005 9504 0b81 66e0 82a6 \
             2d1b ff6e 919d 29ad 1718 63c7 8f0b 97c8 e9ae 82ae 43d3"
        );
        assert_eq!(decoder.decode(&first).unwrap(), pairs(&[
            (":status", "302"),
            ("cache-control", "private"),
            ("date", "Mon, 21 Oct 2013 20:13:21 GMT"),
            ("location", "https://www.example.com"),
        ]));

        // Second response reuses the dynamic table
        let second = hex("4883 640e ffc1 c0bf");
        assert_eq!(decoder.decode(&second).unwrap(), pairs(&[
            (":status", "307"),
            ("cache-control", "private"),
            ("date", "Mon, 21 Oct 2013 20:13:21 GMT"),
            ("location", "https://www.example.com"),
        ]));
    }

    #[test]
    fn test_hpack_integer_overflow_rejected() {
        let mut decoder = HpackDecoder::default();
        // Indexed field with an absurdly long integer continuation
        let data = [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01];
        assert!(decoder.decode(&data).is_err());
    }

    #[test]
    fn test_connection_create_stream() {
        let mut conn = Http2Connection::new_client();

        let id1 = conn.create_stream();
        let id2 = conn.create_stream();

        assert_eq!(id1, 1);
        assert_eq!(id2, 3);
    }

    #[test]
    fn test_request_drops_connection_specific_headers() {
        let mut conn = Http2Connection::new_client();
        let mut out = Vec::new();
        let headers = pairs(&[
            ("Host", "example.com"),
            ("Connection", "keep-alive"),
            ("User-Agent", "test"),
        ]);
        let id = conn.send_request(&mut out, "GET", "/", "example.com", &headers, true).unwrap();

        let sent = conn.get_stream_headers(id).unwrap();
        assert!(sent.iter().all(|(n, _)| n != "host" && n != "connection"));
        assert!(sent.iter().any(|(n, v)| n == "user-agent" && v == "test"));
        assert!(sent.iter().all(|(n, _)| !n.bytes().any(|b| b.is_ascii_uppercase())));
    }

    #[test]
    fn test_unknown_frame_types_are_skipped() {
        let mut wire = Vec::new();
        // Unknown extension frame (type 0x0c, ORIGIN) followed by a PING
        wire.extend_from_slice(&[0, 0, 3, 0x0c, 0, 0, 0, 0, 0, 1, 2, 3]);
        Frame::ping([7; 8], false).write_to(&mut wire).unwrap();

        let frame = Frame::read_from(&mut wire.as_slice(), 16384).unwrap();
        assert_eq!(frame.header.frame_type, FrameType::Ping);
    }

    #[test]
    fn test_padded_data_is_stripped() {
        let mut conn = Http2Connection::new_client();
        let id = conn.create_stream();

        // PADDED flag: pad length 3, data "abc", 3 bytes padding
        let payload = vec![3, b'a', b'b', b'c', 0, 0, 0];
        let frame = Frame::new(FrameType::Data, flags::PADDED | flags::END_STREAM, id, payload);

        match conn.process_frame(frame).unwrap() {
            Some(Http2Event::Data { data, end_stream, .. }) => {
                assert_eq!(data, b"abc");
                assert!(end_stream);
            }
            other => panic!("unexpected event: {:?}", other),
        }
        // Padding still counts against flow control
        assert_eq!(conn.streams[&id].recv_window, Settings::client().initial_window_size as i32 - 7);
    }

    #[test]
    fn test_headers_with_priority_and_continuation() {
        let mut encoder = HpackEncoder::default();
        let mut conn = Http2Connection::new_client();
        let id = conn.create_stream();

        let block = encoder.encode(&pairs(&[(":status", "200"), ("content-type", "text/html")]));
        let (first, rest) = block.split_at(block.len() / 2);

        // HEADERS with PRIORITY block, no END_HEADERS
        let mut payload = vec![0, 0, 0, 0, 16];
        payload.extend_from_slice(first);
        let headers = Frame::new(FrameType::Headers, flags::PRIORITY, id, payload);
        assert!(conn.process_frame(headers).unwrap().is_none());

        // Any other frame before the CONTINUATION is a protocol error
        let mut probe = Http2Connection::new_client();
        probe.pending_headers = Some(PendingHeaders { stream_id: id, block: Vec::new(), end_stream: false });
        assert!(probe.process_frame(Frame::ping([0; 8], false)).is_err());

        let continuation = Frame::new(FrameType::Continuation, flags::END_HEADERS, id, rest.to_vec());
        match conn.process_frame(continuation).unwrap() {
            Some(Http2Event::Headers { headers, .. }) => {
                assert_eq!(headers, pairs(&[(":status", "200"), ("content-type", "text/html")]));
            }
            other => panic!("unexpected event: {:?}", other),
        }
    }

    #[test]
    fn test_replenish_windows_after_consumption() {
        let mut conn = Http2Connection::new_client();
        let mut out = Vec::new();
        conn.send_preface(&mut out).unwrap();
        assert_eq!(conn.recv_window, conn.conn_window_target as i32);

        let id = conn.create_stream();
        let target = conn.local_settings.initial_window_size as usize;

        // Receive more than half the stream window
        for _ in 0..(target / 16384 / 2 + 1) {
            let frame = Frame::data(id, vec![0; 16384], false);
            conn.process_frame(frame).unwrap();
        }
        assert!(conn.streams[&id].recv_window < target as i32 / 2);

        let mut wire = Vec::new();
        conn.replenish_windows(&mut wire, id).unwrap();
        assert!(!wire.is_empty());
        assert_eq!(conn.streams[&id].recv_window, target as i32);

        let update = Frame::read_from(&mut wire.as_slice(), 16384).unwrap();
        assert_eq!(update.header.frame_type, FrameType::WindowUpdate);
    }

    #[test]
    fn test_malformed_control_frames_do_not_panic() {
        let mut conn = Http2Connection::new_client();
        assert!(conn.process_frame(Frame::new(FrameType::GoAway, 0, 0, vec![0; 3])).is_err());
        assert!(conn.process_frame(Frame::new(FrameType::RstStream, 0, 1, vec![0; 2])).is_err());
        assert!(conn.process_frame(Frame::new(FrameType::WindowUpdate, 0, 0, vec![0; 1])).is_err());
        assert!(conn.process_frame(Frame::new(FrameType::Ping, 0, 0, vec![0; 3])).is_err());
    }

    #[test]
    fn test_initial_window_change_adjusts_streams() {
        let mut conn = Http2Connection::new_client();
        let id = conn.create_stream();
        assert_eq!(conn.streams[&id].send_window, DEFAULT_WINDOW_SIZE as i32);

        let settings = Frame::settings(&[(SettingId::InitialWindowSize, 100_000)], false);
        conn.process_frame(settings).unwrap();
        assert_eq!(conn.streams[&id].send_window, 100_000);
    }
}
