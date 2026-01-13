//! HTTP/3 Framing
//!
//! HTTP/3 frame types and encoding per RFC 9114.

use super::packet::varint;

/// HTTP/3 frame types
#[derive(Debug, Clone)]
pub enum Http3Frame {
    /// DATA frame (type 0x00)
    Data {
        /// Payload data
        data: Vec<u8>,
    },
    
    /// HEADERS frame (type 0x01)
    Headers {
        /// QPACK-encoded headers
        encoded_headers: Vec<u8>,
    },
    
    /// CANCEL_PUSH frame (type 0x03)
    CancelPush {
        /// Push ID
        push_id: u64,
    },
    
    /// SETTINGS frame (type 0x04)
    Settings {
        /// Settings
        settings: Vec<Http3Setting>,
    },
    
    /// PUSH_PROMISE frame (type 0x05)
    PushPromise {
        /// Push ID
        push_id: u64,
        /// QPACK-encoded headers
        encoded_headers: Vec<u8>,
    },
    
    /// GOAWAY frame (type 0x07)
    Goaway {
        /// Stream/Push ID
        id: u64,
    },
    
    /// MAX_PUSH_ID frame (type 0x0d)
    MaxPushId {
        /// Maximum push ID
        push_id: u64,
    },
}

/// HTTP/3 setting identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Http3SettingId {
    /// QPACK maximum table capacity
    QpackMaxTableCapacity,
    /// Maximum header list size
    MaxFieldSectionSize,
    /// QPACK blocked streams
    QpackBlockedStreams,
    /// Enable CONNECT protocol
    EnableConnectProtocol,
    /// Enable webtransport
    EnableWebTransport,
    /// Unknown setting
    Unknown(u64),
}

impl Http3SettingId {
    /// Convert from wire format
    pub fn from_wire(id: u64) -> Self {
        match id {
            0x01 => Http3SettingId::QpackMaxTableCapacity,
            0x06 => Http3SettingId::MaxFieldSectionSize,
            0x07 => Http3SettingId::QpackBlockedStreams,
            0x08 => Http3SettingId::EnableConnectProtocol,
            0x2b603742 => Http3SettingId::EnableWebTransport,
            other => Http3SettingId::Unknown(other),
        }
    }
    
    /// Convert to wire format
    pub fn to_wire(self) -> u64 {
        match self {
            Http3SettingId::QpackMaxTableCapacity => 0x01,
            Http3SettingId::MaxFieldSectionSize => 0x06,
            Http3SettingId::QpackBlockedStreams => 0x07,
            Http3SettingId::EnableConnectProtocol => 0x08,
            Http3SettingId::EnableWebTransport => 0x2b603742,
            Http3SettingId::Unknown(id) => id,
        }
    }
}

/// A single HTTP/3 setting
#[derive(Debug, Clone, Copy)]
pub struct Http3Setting {
    /// Setting identifier
    pub id: Http3SettingId,
    /// Setting value
    pub value: u64,
}

impl Http3Frame {
    /// Get frame type
    pub fn frame_type(&self) -> u64 {
        match self {
            Http3Frame::Data { .. } => 0x00,
            Http3Frame::Headers { .. } => 0x01,
            Http3Frame::CancelPush { .. } => 0x03,
            Http3Frame::Settings { .. } => 0x04,
            Http3Frame::PushPromise { .. } => 0x05,
            Http3Frame::Goaway { .. } => 0x07,
            Http3Frame::MaxPushId { .. } => 0x0d,
        }
    }
    
    /// Encode frame to bytes
    pub fn encode(&self, buf: &mut Vec<u8>) {
        encode_varint(self.frame_type(), buf);
        
        match self {
            Http3Frame::Data { data } => {
                encode_varint(data.len() as u64, buf);
                buf.extend_from_slice(data);
            }
            
            Http3Frame::Headers { encoded_headers } => {
                encode_varint(encoded_headers.len() as u64, buf);
                buf.extend_from_slice(encoded_headers);
            }
            
            Http3Frame::CancelPush { push_id } => {
                let len = varint::encoded_len(*push_id);
                encode_varint(len as u64, buf);
                encode_varint(*push_id, buf);
            }
            
            Http3Frame::Settings { settings } => {
                // Calculate payload size
                let mut payload = Vec::new();
                for s in settings {
                    encode_varint(s.id.to_wire(), &mut payload);
                    encode_varint(s.value, &mut payload);
                }
                encode_varint(payload.len() as u64, buf);
                buf.extend(payload);
            }
            
            Http3Frame::PushPromise { push_id, encoded_headers } => {
                let push_id_len = varint::encoded_len(*push_id);
                encode_varint((push_id_len + encoded_headers.len()) as u64, buf);
                encode_varint(*push_id, buf);
                buf.extend_from_slice(encoded_headers);
            }
            
            Http3Frame::Goaway { id } => {
                let len = varint::encoded_len(*id);
                encode_varint(len as u64, buf);
                encode_varint(*id, buf);
            }
            
            Http3Frame::MaxPushId { push_id } => {
                let len = varint::encoded_len(*push_id);
                encode_varint(len as u64, buf);
                encode_varint(*push_id, buf);
            }
        }
    }
    
    /// Decode frame from bytes
    pub fn decode(buf: &[u8]) -> Option<(Self, usize)> {
        if buf.is_empty() {
            return None;
        }
        
        let (frame_type, type_len) = varint::decode(buf)?;
        let mut pos = type_len;
        
        let (payload_len, len_size) = varint::decode(&buf[pos..])?;
        pos += len_size;
        
        if buf.len() < pos + payload_len as usize {
            return None;
        }
        
        let payload = &buf[pos..pos + payload_len as usize];
        let total_len = pos + payload_len as usize;
        
        let frame = match frame_type {
            0x00 => Http3Frame::Data {
                data: payload.to_vec(),
            },
            
            0x01 => Http3Frame::Headers {
                encoded_headers: payload.to_vec(),
            },
            
            0x03 => {
                let (push_id, _) = varint::decode(payload)?;
                Http3Frame::CancelPush { push_id }
            }
            
            0x04 => {
                let mut settings = Vec::new();
                let mut p = 0;
                while p < payload.len() {
                    let (id, n) = varint::decode(&payload[p..])?;
                    p += n;
                    let (value, n) = varint::decode(&payload[p..])?;
                    p += n;
                    settings.push(Http3Setting {
                        id: Http3SettingId::from_wire(id),
                        value,
                    });
                }
                Http3Frame::Settings { settings }
            }
            
            0x05 => {
                let (push_id, n) = varint::decode(payload)?;
                let encoded_headers = payload[n..].to_vec();
                Http3Frame::PushPromise { push_id, encoded_headers }
            }
            
            0x07 => {
                let (id, _) = varint::decode(payload)?;
                Http3Frame::Goaway { id }
            }
            
            0x0d => {
                let (push_id, _) = varint::decode(payload)?;
                Http3Frame::MaxPushId { push_id }
            }
            
            _ => return None,
        };
        
        Some((frame, total_len))
    }
}

/// HTTP/3 unidirectional stream types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UniStreamType {
    /// Control stream
    Control = 0x00,
    /// Push stream
    Push = 0x01,
    /// QPACK encoder stream
    QpackEncoder = 0x02,
    /// QPACK decoder stream
    QpackDecoder = 0x03,
}

impl UniStreamType {
    /// Parse from wire format
    pub fn from_wire(value: u64) -> Option<Self> {
        match value {
            0x00 => Some(UniStreamType::Control),
            0x01 => Some(UniStreamType::Push),
            0x02 => Some(UniStreamType::QpackEncoder),
            0x03 => Some(UniStreamType::QpackDecoder),
            _ => None,
        }
    }
}

/// Helper to encode varint into a Vec
fn encode_varint(value: u64, buf: &mut Vec<u8>) {
    let mut tmp = [0u8; 8];
    let len = varint::encode(value, &mut tmp).unwrap();
    buf.extend_from_slice(&tmp[..len]);
}

/// Default HTTP/3 settings
pub fn default_settings() -> Vec<Http3Setting> {
    vec![
        Http3Setting {
            id: Http3SettingId::QpackMaxTableCapacity,
            value: 4096,
        },
        Http3Setting {
            id: Http3SettingId::MaxFieldSectionSize,
            value: 16384,
        },
        Http3Setting {
            id: Http3SettingId::QpackBlockedStreams,
            value: 100,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_data_frame_encode_decode() {
        let frame = Http3Frame::Data {
            data: vec![1, 2, 3, 4, 5],
        };
        
        let mut buf = Vec::new();
        frame.encode(&mut buf);
        
        let (decoded, len) = Http3Frame::decode(&buf).unwrap();
        assert_eq!(len, buf.len());
        
        if let Http3Frame::Data { data } = decoded {
            assert_eq!(data, vec![1, 2, 3, 4, 5]);
        } else {
            panic!("Expected Data frame");
        }
    }
    
    #[test]
    fn test_headers_frame_encode_decode() {
        let frame = Http3Frame::Headers {
            encoded_headers: vec![0x00, 0x00, 0xc0 | 17], // Prefix + :method GET
        };
        
        let mut buf = Vec::new();
        frame.encode(&mut buf);
        
        let (decoded, _) = Http3Frame::decode(&buf).unwrap();
        if let Http3Frame::Headers { encoded_headers } = decoded {
            assert!(!encoded_headers.is_empty());
        } else {
            panic!("Expected Headers frame");
        }
    }
    
    #[test]
    fn test_settings_frame_encode_decode() {
        let frame = Http3Frame::Settings {
            settings: default_settings(),
        };
        
        let mut buf = Vec::new();
        frame.encode(&mut buf);
        
        let (decoded, _) = Http3Frame::decode(&buf).unwrap();
        if let Http3Frame::Settings { settings } = decoded {
            assert_eq!(settings.len(), 3);
        } else {
            panic!("Expected Settings frame");
        }
    }
    
    #[test]
    fn test_goaway_frame_encode_decode() {
        let frame = Http3Frame::Goaway { id: 100 };
        
        let mut buf = Vec::new();
        frame.encode(&mut buf);
        
        let (decoded, _) = Http3Frame::decode(&buf).unwrap();
        if let Http3Frame::Goaway { id } = decoded {
            assert_eq!(id, 100);
        } else {
            panic!("Expected Goaway frame");
        }
    }
    
    #[test]
    fn test_uni_stream_types() {
        assert_eq!(UniStreamType::from_wire(0x00), Some(UniStreamType::Control));
        assert_eq!(UniStreamType::from_wire(0x02), Some(UniStreamType::QpackEncoder));
        assert_eq!(UniStreamType::from_wire(0xFF), None);
    }
}
