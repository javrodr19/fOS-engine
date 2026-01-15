//! IPC Serialization
//!
//! Compact binary serialization for IPC messages.
//! Custom format (no serde) for hot paths.

use std::io::{self, Read, Write};

use super::message::{MessageType, TypedMessage};

/// IPC serialization trait
pub trait IpcSerialize: Sized {
    /// Serialize to buffer
    fn ipc_serialize(&self, buf: &mut Vec<u8>);
    
    /// Deserialize from buffer
    fn ipc_deserialize(buf: &[u8]) -> Result<(Self, usize), IpcError>;
    
    /// Serialized size estimate
    fn serialized_size(&self) -> usize;
}

/// IPC serialization errors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IpcError {
    /// Buffer too short
    BufferTooShort,
    /// Invalid data format
    InvalidFormat,
    /// Unknown message type
    UnknownMessageType(u16),
    /// String not valid UTF-8
    InvalidUtf8,
    /// Checksum mismatch
    ChecksumMismatch,
}

impl std::fmt::Display for IpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BufferTooShort => write!(f, "Buffer too short"),
            Self::InvalidFormat => write!(f, "Invalid format"),
            Self::UnknownMessageType(t) => write!(f, "Unknown message type: {}", t),
            Self::InvalidUtf8 => write!(f, "Invalid UTF-8"),
            Self::ChecksumMismatch => write!(f, "Checksum mismatch"),
        }
    }
}

impl std::error::Error for IpcError {}

/// Write variable-length integer (LEB128)
pub fn write_varint(buf: &mut Vec<u8>, mut value: u64) {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if value == 0 {
            break;
        }
    }
}

/// Read variable-length integer (LEB128)
pub fn read_varint(buf: &[u8]) -> Result<(u64, usize), IpcError> {
    let mut result: u64 = 0;
    let mut shift = 0;
    
    for (i, &byte) in buf.iter().enumerate() {
        if shift >= 64 {
            return Err(IpcError::InvalidFormat);
        }
        
        result |= ((byte & 0x7F) as u64) << shift;
        shift += 7;
        
        if byte & 0x80 == 0 {
            return Ok((result, i + 1));
        }
    }
    
    Err(IpcError::BufferTooShort)
}

/// Write u16 little-endian
pub fn write_u16(buf: &mut Vec<u8>, value: u16) {
    buf.extend_from_slice(&value.to_le_bytes());
}

/// Read u16 little-endian
pub fn read_u16(buf: &[u8]) -> Result<u16, IpcError> {
    if buf.len() < 2 {
        return Err(IpcError::BufferTooShort);
    }
    Ok(u16::from_le_bytes([buf[0], buf[1]]))
}

/// Write u32 little-endian
pub fn write_u32(buf: &mut Vec<u8>, value: u32) {
    buf.extend_from_slice(&value.to_le_bytes());
}

/// Read u32 little-endian
pub fn read_u32(buf: &[u8]) -> Result<u32, IpcError> {
    if buf.len() < 4 {
        return Err(IpcError::BufferTooShort);
    }
    Ok(u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]))
}

/// Write length-prefixed bytes
pub fn write_bytes(buf: &mut Vec<u8>, data: &[u8]) {
    write_varint(buf, data.len() as u64);
    buf.extend_from_slice(data);
}

/// Read length-prefixed bytes
pub fn read_bytes(buf: &[u8]) -> Result<(&[u8], usize), IpcError> {
    let (len, offset) = read_varint(buf)?;
    let len = len as usize;
    
    if buf.len() < offset + len {
        return Err(IpcError::BufferTooShort);
    }
    
    Ok((&buf[offset..offset + len], offset + len))
}

/// Write length-prefixed string
pub fn write_string(buf: &mut Vec<u8>, s: &str) {
    write_bytes(buf, s.as_bytes());
}

/// Read length-prefixed string
pub fn read_string(buf: &[u8]) -> Result<(&str, usize), IpcError> {
    let (bytes, consumed) = read_bytes(buf)?;
    let s = std::str::from_utf8(bytes).map_err(|_| IpcError::InvalidUtf8)?;
    Ok((s, consumed))
}

impl IpcSerialize for TypedMessage {
    fn ipc_serialize(&self, buf: &mut Vec<u8>) {
        // Message type (2 bytes)
        write_u16(buf, self.msg_type as u16);
        // Request ID (4 bytes)
        write_u32(buf, self.request_id);
        // Payload (length-prefixed)
        write_bytes(buf, &self.payload);
    }
    
    fn ipc_deserialize(buf: &[u8]) -> Result<(Self, usize), IpcError> {
        if buf.len() < 6 {
            return Err(IpcError::BufferTooShort);
        }
        
        let msg_type_raw = read_u16(buf)?;
        let msg_type = MessageType::from_u16(msg_type_raw)
            .ok_or(IpcError::UnknownMessageType(msg_type_raw))?;
        
        let request_id = read_u32(&buf[2..])?;
        
        let (payload_bytes, payload_consumed) = read_bytes(&buf[6..])?;
        let payload = payload_bytes.to_vec();
        
        let total_consumed = 6 + payload_consumed;
        
        Ok((Self { msg_type, request_id, payload }, total_consumed))
    }
    
    fn serialized_size(&self) -> usize {
        2 + 4 + 1 + self.payload.len() // type + id + length byte + payload
    }
}

/// Message frame with header
#[derive(Debug, Clone)]
pub struct MessageFrame {
    /// Frame length (excluding this field)
    pub length: u32,
    /// Checksum (simple XOR)
    pub checksum: u8,
    /// Payload
    pub payload: Vec<u8>,
}

impl MessageFrame {
    /// Create from payload
    pub fn new(payload: Vec<u8>) -> Self {
        let checksum = payload.iter().fold(0u8, |acc, &b| acc ^ b);
        Self {
            length: payload.len() as u32,
            checksum,
            payload,
        }
    }
    
    /// Serialize to bytes (for wire transmission)
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(5 + self.payload.len());
        write_u32(&mut buf, self.length);
        buf.push(self.checksum);
        buf.extend_from_slice(&self.payload);
        buf
    }
    
    /// Deserialize from bytes
    pub fn from_bytes(buf: &[u8]) -> Result<(Self, usize), IpcError> {
        if buf.len() < 5 {
            return Err(IpcError::BufferTooShort);
        }
        
        let length = read_u32(buf)?;
        let checksum = buf[4];
        
        let total_len = 5 + length as usize;
        if buf.len() < total_len {
            return Err(IpcError::BufferTooShort);
        }
        
        let payload = buf[5..total_len].to_vec();
        
        // Verify checksum
        let computed = payload.iter().fold(0u8, |acc, &b| acc ^ b);
        if computed != checksum {
            return Err(IpcError::ChecksumMismatch);
        }
        
        Ok((Self { length, checksum, payload }, total_len))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_varint_round_trip() {
        for val in [0u64, 1, 127, 128, 255, 1000, 65535, 0x7FFFFFFF, u64::MAX] {
            let mut buf = Vec::new();
            write_varint(&mut buf, val);
            
            let (result, _) = read_varint(&buf).unwrap();
            assert_eq!(result, val, "Failed for value {}", val);
        }
    }
    
    #[test]
    fn test_bytes_round_trip() {
        let data = b"Hello, world!";
        let mut buf = Vec::new();
        write_bytes(&mut buf, data);
        
        let (result, _) = read_bytes(&buf).unwrap();
        assert_eq!(result, data);
    }
    
    #[test]
    fn test_string_round_trip() {
        let s = "Hello, 世界!";
        let mut buf = Vec::new();
        write_string(&mut buf, s);
        
        let (result, _) = read_string(&buf).unwrap();
        assert_eq!(result, s);
    }
    
    #[test]
    fn test_typed_message_round_trip() {
        let msg = TypedMessage::new(
            MessageType::Navigate,
            42,
            b"https://example.com".to_vec(),
        );
        
        let mut buf = Vec::new();
        msg.ipc_serialize(&mut buf);
        
        let (result, _) = TypedMessage::ipc_deserialize(&buf).unwrap();
        assert_eq!(result.msg_type, MessageType::Navigate);
        assert_eq!(result.request_id, 42);
        assert_eq!(result.payload, b"https://example.com");
    }
    
    #[test]
    fn test_message_frame() {
        let payload = b"Test payload data".to_vec();
        let frame = MessageFrame::new(payload.clone());
        
        let bytes = frame.to_bytes();
        let (parsed, _) = MessageFrame::from_bytes(&bytes).unwrap();
        
        assert_eq!(parsed.payload, payload);
        assert_eq!(parsed.checksum, frame.checksum);
    }
}
