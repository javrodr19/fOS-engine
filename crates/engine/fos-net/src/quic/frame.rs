//! QUIC Frames
//!
//! Frame types and encoding/decoding per RFC 9000 §12.4.

use super::cid::ConnectionId;
use super::packet::varint;

/// QUIC frame types
#[derive(Debug, Clone)]
pub enum Frame {
    /// PADDING frame (type 0x00)
    Padding,
    
    /// PING frame (type 0x01)
    Ping,
    
    /// ACK frame (types 0x02, 0x03)
    Ack {
        /// Largest acknowledged packet number
        largest_acked: u64,
        /// ACK delay in microseconds
        ack_delay: u64,
        /// ACK ranges
        ranges: Vec<AckRange>,
        /// ECN counts (if type 0x03)
        ecn_counts: Option<EcnCounts>,
    },
    
    /// RESET_STREAM frame (type 0x04)
    ResetStream {
        stream_id: u64,
        error_code: u64,
        final_size: u64,
    },
    
    /// STOP_SENDING frame (type 0x05)
    StopSending {
        stream_id: u64,
        error_code: u64,
    },
    
    /// CRYPTO frame (type 0x06)
    Crypto {
        offset: u64,
        data: Vec<u8>,
    },
    
    /// NEW_TOKEN frame (type 0x07)
    NewToken {
        token: Vec<u8>,
    },
    
    /// STREAM frame (types 0x08-0x0f)
    Stream {
        stream_id: u64,
        offset: u64,
        data: Vec<u8>,
        fin: bool,
    },
    
    /// MAX_DATA frame (type 0x10)
    MaxData {
        max_data: u64,
    },
    
    /// MAX_STREAM_DATA frame (type 0x11)
    MaxStreamData {
        stream_id: u64,
        max_data: u64,
    },
    
    /// MAX_STREAMS frame (types 0x12, 0x13)
    MaxStreams {
        max_streams: u64,
        bidirectional: bool,
    },
    
    /// DATA_BLOCKED frame (type 0x14)
    DataBlocked {
        max_data: u64,
    },
    
    /// STREAM_DATA_BLOCKED frame (type 0x15)
    StreamDataBlocked {
        stream_id: u64,
        max_data: u64,
    },
    
    /// STREAMS_BLOCKED frame (types 0x16, 0x17)
    StreamsBlocked {
        max_streams: u64,
        bidirectional: bool,
    },
    
    /// NEW_CONNECTION_ID frame (type 0x18)
    NewConnectionId {
        sequence: u64,
        retire_prior_to: u64,
        connection_id: ConnectionId,
        stateless_reset_token: [u8; 16],
    },
    
    /// RETIRE_CONNECTION_ID frame (type 0x19)
    RetireConnectionId {
        sequence: u64,
    },
    
    /// PATH_CHALLENGE frame (type 0x1a)
    PathChallenge {
        data: [u8; 8],
    },
    
    /// PATH_RESPONSE frame (type 0x1b)
    PathResponse {
        data: [u8; 8],
    },
    
    /// CONNECTION_CLOSE frame (types 0x1c, 0x1d)
    ConnectionClose {
        error_code: u64,
        frame_type: Option<u64>,
        reason: String,
    },
    
    /// HANDSHAKE_DONE frame (type 0x1e)
    HandshakeDone,
}

/// ACK range
#[derive(Debug, Clone, Copy)]
pub struct AckRange {
    /// Gap before this range
    pub gap: u64,
    /// Number of packets in this range
    pub acked: u64,
}

/// ECN counts from ACK frame
#[derive(Debug, Clone, Copy, Default)]
pub struct EcnCounts {
    /// ECT(0) count
    pub ect0: u64,
    /// ECT(1) count
    pub ect1: u64,
    /// CE count
    pub ce: u64,
}

impl Frame {
    /// Get the frame type byte(s)
    pub fn frame_type(&self) -> u8 {
        match self {
            Frame::Padding => 0x00,
            Frame::Ping => 0x01,
            Frame::Ack { ecn_counts: None, .. } => 0x02,
            Frame::Ack { ecn_counts: Some(_), .. } => 0x03,
            Frame::ResetStream { .. } => 0x04,
            Frame::StopSending { .. } => 0x05,
            Frame::Crypto { .. } => 0x06,
            Frame::NewToken { .. } => 0x07,
            Frame::Stream { offset, fin, .. } => {
                let mut t = 0x08;
                if *offset > 0 { t |= 0x04; }
                if !self.stream_data().unwrap().is_empty() { t |= 0x02; }
                if *fin { t |= 0x01; }
                t
            }
            Frame::MaxData { .. } => 0x10,
            Frame::MaxStreamData { .. } => 0x11,
            Frame::MaxStreams { bidirectional: true, .. } => 0x12,
            Frame::MaxStreams { bidirectional: false, .. } => 0x13,
            Frame::DataBlocked { .. } => 0x14,
            Frame::StreamDataBlocked { .. } => 0x15,
            Frame::StreamsBlocked { bidirectional: true, .. } => 0x16,
            Frame::StreamsBlocked { bidirectional: false, .. } => 0x17,
            Frame::NewConnectionId { .. } => 0x18,
            Frame::RetireConnectionId { .. } => 0x19,
            Frame::PathChallenge { .. } => 0x1a,
            Frame::PathResponse { .. } => 0x1b,
            Frame::ConnectionClose { frame_type: Some(_), .. } => 0x1c,
            Frame::ConnectionClose { frame_type: None, .. } => 0x1d,
            Frame::HandshakeDone => 0x1e,
        }
    }
    
    /// Get stream data if this is a Stream frame
    fn stream_data(&self) -> Option<&[u8]> {
        if let Frame::Stream { data, .. } = self {
            Some(data)
        } else {
            None
        }
    }
    
    /// Encode frame to bytes
    pub fn encode(&self, buf: &mut Vec<u8>) {
        match self {
            Frame::Padding => {
                buf.push(0x00);
            }
            
            Frame::Ping => {
                buf.push(0x01);
            }
            
            Frame::Ack { largest_acked, ack_delay, ranges, ecn_counts } => {
                buf.push(if ecn_counts.is_some() { 0x03 } else { 0x02 });
                encode_varint(*largest_acked, buf);
                encode_varint(*ack_delay, buf);
                
                // ACK Range Count is the number of ADDITIONAL ranges (after the first)
                let range_count = ranges.len().saturating_sub(1) as u64;
                encode_varint(range_count, buf);
                
                // First ACK range (required)
                if !ranges.is_empty() {
                    encode_varint(ranges[0].acked, buf);
                } else {
                    encode_varint(0, buf);
                }
                
                // Additional ranges
                for range in ranges.iter().skip(1) {
                    encode_varint(range.gap, buf);
                    encode_varint(range.acked, buf);
                }
                
                if let Some(ecn) = ecn_counts {
                    encode_varint(ecn.ect0, buf);
                    encode_varint(ecn.ect1, buf);
                    encode_varint(ecn.ce, buf);
                }
            }
            
            Frame::ResetStream { stream_id, error_code, final_size } => {
                buf.push(0x04);
                encode_varint(*stream_id, buf);
                encode_varint(*error_code, buf);
                encode_varint(*final_size, buf);
            }
            
            Frame::StopSending { stream_id, error_code } => {
                buf.push(0x05);
                encode_varint(*stream_id, buf);
                encode_varint(*error_code, buf);
            }
            
            Frame::Crypto { offset, data } => {
                buf.push(0x06);
                encode_varint(*offset, buf);
                encode_varint(data.len() as u64, buf);
                buf.extend_from_slice(data);
            }
            
            Frame::NewToken { token } => {
                buf.push(0x07);
                encode_varint(token.len() as u64, buf);
                buf.extend_from_slice(token);
            }
            
            Frame::Stream { stream_id, offset, data, fin } => {
                let mut frame_type = 0x08u8;
                if *offset > 0 { frame_type |= 0x04; }
                frame_type |= 0x02; // Always include length
                if *fin { frame_type |= 0x01; }
                
                buf.push(frame_type);
                encode_varint(*stream_id, buf);
                if *offset > 0 {
                    encode_varint(*offset, buf);
                }
                encode_varint(data.len() as u64, buf);
                buf.extend_from_slice(data);
            }
            
            Frame::MaxData { max_data } => {
                buf.push(0x10);
                encode_varint(*max_data, buf);
            }
            
            Frame::MaxStreamData { stream_id, max_data } => {
                buf.push(0x11);
                encode_varint(*stream_id, buf);
                encode_varint(*max_data, buf);
            }
            
            Frame::MaxStreams { max_streams, bidirectional } => {
                buf.push(if *bidirectional { 0x12 } else { 0x13 });
                encode_varint(*max_streams, buf);
            }
            
            Frame::DataBlocked { max_data } => {
                buf.push(0x14);
                encode_varint(*max_data, buf);
            }
            
            Frame::StreamDataBlocked { stream_id, max_data } => {
                buf.push(0x15);
                encode_varint(*stream_id, buf);
                encode_varint(*max_data, buf);
            }
            
            Frame::StreamsBlocked { max_streams, bidirectional } => {
                buf.push(if *bidirectional { 0x16 } else { 0x17 });
                encode_varint(*max_streams, buf);
            }
            
            Frame::NewConnectionId { sequence, retire_prior_to, connection_id, stateless_reset_token } => {
                buf.push(0x18);
                encode_varint(*sequence, buf);
                encode_varint(*retire_prior_to, buf);
                buf.push(connection_id.len() as u8);
                buf.extend_from_slice(connection_id.as_bytes());
                buf.extend_from_slice(stateless_reset_token);
            }
            
            Frame::RetireConnectionId { sequence } => {
                buf.push(0x19);
                encode_varint(*sequence, buf);
            }
            
            Frame::PathChallenge { data } => {
                buf.push(0x1a);
                buf.extend_from_slice(data);
            }
            
            Frame::PathResponse { data } => {
                buf.push(0x1b);
                buf.extend_from_slice(data);
            }
            
            Frame::ConnectionClose { error_code, frame_type, reason } => {
                buf.push(if frame_type.is_some() { 0x1c } else { 0x1d });
                encode_varint(*error_code, buf);
                if let Some(ft) = frame_type {
                    encode_varint(*ft, buf);
                }
                let reason_bytes = reason.as_bytes();
                encode_varint(reason_bytes.len() as u64, buf);
                buf.extend_from_slice(reason_bytes);
            }
            
            Frame::HandshakeDone => {
                buf.push(0x1e);
            }
        }
    }
    
    /// Decode a frame from bytes
    pub fn decode(buf: &[u8]) -> Option<(Self, usize)> {
        if buf.is_empty() {
            return None;
        }
        
        let (frame_type, type_len) = varint::decode(buf)?;
        let mut pos = type_len;
        
        let frame = match frame_type {
            0x00 => Frame::Padding,
            
            0x01 => Frame::Ping,
            
            0x02 | 0x03 => {
                let (largest_acked, n) = varint::decode(&buf[pos..])?;
                pos += n;
                let (ack_delay, n) = varint::decode(&buf[pos..])?;
                pos += n;
                let (range_count, n) = varint::decode(&buf[pos..])?;
                pos += n;
                
                let mut ranges = Vec::with_capacity(range_count as usize + 1);
                
                // First ACK range
                let (first_range, n) = varint::decode(&buf[pos..])?;
                pos += n;
                ranges.push(AckRange { gap: 0, acked: first_range });
                
                // Additional ranges
                for _ in 0..range_count {
                    let (gap, n) = varint::decode(&buf[pos..])?;
                    pos += n;
                    let (acked, n) = varint::decode(&buf[pos..])?;
                    pos += n;
                    ranges.push(AckRange { gap, acked });
                }
                
                let ecn_counts = if frame_type == 0x03 {
                    let (ect0, n) = varint::decode(&buf[pos..])?;
                    pos += n;
                    let (ect1, n) = varint::decode(&buf[pos..])?;
                    pos += n;
                    let (ce, n) = varint::decode(&buf[pos..])?;
                    pos += n;
                    Some(EcnCounts { ect0, ect1, ce })
                } else {
                    None
                };
                
                Frame::Ack { largest_acked, ack_delay, ranges, ecn_counts }
            }
            
            0x04 => {
                let (stream_id, n) = varint::decode(&buf[pos..])?;
                pos += n;
                let (error_code, n) = varint::decode(&buf[pos..])?;
                pos += n;
                let (final_size, n) = varint::decode(&buf[pos..])?;
                pos += n;
                Frame::ResetStream { stream_id, error_code, final_size }
            }
            
            0x05 => {
                let (stream_id, n) = varint::decode(&buf[pos..])?;
                pos += n;
                let (error_code, n) = varint::decode(&buf[pos..])?;
                pos += n;
                Frame::StopSending { stream_id, error_code }
            }
            
            0x06 => {
                let (offset, n) = varint::decode(&buf[pos..])?;
                pos += n;
                let (length, n) = varint::decode(&buf[pos..])?;
                pos += n;
                let data = buf[pos..pos + length as usize].to_vec();
                pos += length as usize;
                Frame::Crypto { offset, data }
            }
            
            0x07 => {
                let (length, n) = varint::decode(&buf[pos..])?;
                pos += n;
                let token = buf[pos..pos + length as usize].to_vec();
                pos += length as usize;
                Frame::NewToken { token }
            }
            
            0x08..=0x0f => {
                let has_offset = (frame_type & 0x04) != 0;
                let has_length = (frame_type & 0x02) != 0;
                let fin = (frame_type & 0x01) != 0;
                
                let (stream_id, n) = varint::decode(&buf[pos..])?;
                pos += n;
                
                let offset = if has_offset {
                    let (o, n) = varint::decode(&buf[pos..])?;
                    pos += n;
                    o
                } else {
                    0
                };
                
                let data = if has_length {
                    let (len, n) = varint::decode(&buf[pos..])?;
                    pos += n;
                    let d = buf[pos..pos + len as usize].to_vec();
                    pos += len as usize;
                    d
                } else {
                    buf[pos..].to_vec()
                };
                
                Frame::Stream { stream_id, offset, data, fin }
            }
            
            0x10 => {
                let (max_data, n) = varint::decode(&buf[pos..])?;
                pos += n;
                Frame::MaxData { max_data }
            }
            
            0x11 => {
                let (stream_id, n) = varint::decode(&buf[pos..])?;
                pos += n;
                let (max_data, n) = varint::decode(&buf[pos..])?;
                pos += n;
                Frame::MaxStreamData { stream_id, max_data }
            }
            
            0x12 | 0x13 => {
                let (max_streams, n) = varint::decode(&buf[pos..])?;
                pos += n;
                Frame::MaxStreams { max_streams, bidirectional: frame_type == 0x12 }
            }
            
            0x14 => {
                let (max_data, n) = varint::decode(&buf[pos..])?;
                pos += n;
                Frame::DataBlocked { max_data }
            }
            
            0x15 => {
                let (stream_id, n) = varint::decode(&buf[pos..])?;
                pos += n;
                let (max_data, n) = varint::decode(&buf[pos..])?;
                pos += n;
                Frame::StreamDataBlocked { stream_id, max_data }
            }
            
            0x16 | 0x17 => {
                let (max_streams, n) = varint::decode(&buf[pos..])?;
                pos += n;
                Frame::StreamsBlocked { max_streams, bidirectional: frame_type == 0x16 }
            }
            
            0x18 => {
                let (sequence, n) = varint::decode(&buf[pos..])?;
                pos += n;
                let (retire_prior_to, n) = varint::decode(&buf[pos..])?;
                pos += n;
                
                let cid_len = buf[pos] as usize;
                pos += 1;
                
                let connection_id = ConnectionId::from_bytes(&buf[pos..pos + cid_len])?;
                pos += cid_len;
                
                let mut stateless_reset_token = [0u8; 16];
                stateless_reset_token.copy_from_slice(&buf[pos..pos + 16]);
                pos += 16;
                
                Frame::NewConnectionId { sequence, retire_prior_to, connection_id, stateless_reset_token }
            }
            
            0x19 => {
                let (sequence, n) = varint::decode(&buf[pos..])?;
                pos += n;
                Frame::RetireConnectionId { sequence }
            }
            
            0x1a => {
                let mut data = [0u8; 8];
                data.copy_from_slice(&buf[pos..pos + 8]);
                pos += 8;
                Frame::PathChallenge { data }
            }
            
            0x1b => {
                let mut data = [0u8; 8];
                data.copy_from_slice(&buf[pos..pos + 8]);
                pos += 8;
                Frame::PathResponse { data }
            }
            
            0x1c | 0x1d => {
                let (error_code, n) = varint::decode(&buf[pos..])?;
                pos += n;
                
                let frame_type_val = if frame_type == 0x1c {
                    let (ft, n) = varint::decode(&buf[pos..])?;
                    pos += n;
                    Some(ft)
                } else {
                    None
                };
                
                let (reason_len, n) = varint::decode(&buf[pos..])?;
                pos += n;
                let reason = String::from_utf8_lossy(&buf[pos..pos + reason_len as usize]).to_string();
                pos += reason_len as usize;
                
                Frame::ConnectionClose { error_code, frame_type: frame_type_val, reason }
            }
            
            0x1e => Frame::HandshakeDone,
            
            _ => return None,
        };
        
        Some((frame, pos))
    }
    
    /// Check if this frame is ACK-eliciting
    pub fn is_ack_eliciting(&self) -> bool {
        !matches!(self, Frame::Padding | Frame::Ack { .. })
    }
}

/// Helper to encode varint into a Vec
fn encode_varint(value: u64, buf: &mut Vec<u8>) {
    let mut tmp = [0u8; 8];
    let len = varint::encode(value, &mut tmp).unwrap();
    buf.extend_from_slice(&tmp[..len]);
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_padding_encode_decode() {
        let frame = Frame::Padding;
        let mut buf = Vec::new();
        frame.encode(&mut buf);
        
        let (decoded, len) = Frame::decode(&buf).unwrap();
        assert_eq!(len, 1);
        assert!(matches!(decoded, Frame::Padding));
    }
    
    #[test]
    fn test_ping_encode_decode() {
        let frame = Frame::Ping;
        let mut buf = Vec::new();
        frame.encode(&mut buf);
        
        let (decoded, _) = Frame::decode(&buf).unwrap();
        assert!(matches!(decoded, Frame::Ping));
    }
    
    #[test]
    fn test_crypto_encode_decode() {
        let frame = Frame::Crypto {
            offset: 100,
            data: vec![1, 2, 3, 4, 5],
        };
        
        let mut buf = Vec::new();
        frame.encode(&mut buf);
        
        let (decoded, _) = Frame::decode(&buf).unwrap();
        if let Frame::Crypto { offset, data } = decoded {
            assert_eq!(offset, 100);
            assert_eq!(data, vec![1, 2, 3, 4, 5]);
        } else {
            panic!("Expected Crypto frame");
        }
    }
    
    #[test]
    fn test_stream_encode_decode() {
        let frame = Frame::Stream {
            stream_id: 4,
            offset: 1000,
            data: vec![0; 100],
            fin: true,
        };
        
        let mut buf = Vec::new();
        frame.encode(&mut buf);
        
        let (decoded, _) = Frame::decode(&buf).unwrap();
        if let Frame::Stream { stream_id, offset, data, fin } = decoded {
            assert_eq!(stream_id, 4);
            assert_eq!(offset, 1000);
            assert_eq!(data.len(), 100);
            assert!(fin);
        } else {
            panic!("Expected Stream frame");
        }
    }
    
    #[test]
    fn test_ack_encode_decode() {
        let frame = Frame::Ack {
            largest_acked: 100,
            ack_delay: 500,
            ranges: vec![
                AckRange { gap: 0, acked: 5 },
                AckRange { gap: 2, acked: 3 },
            ],
            ecn_counts: None,
        };
        
        let mut buf = Vec::new();
        frame.encode(&mut buf);
        
        let (decoded, _) = Frame::decode(&buf).unwrap();
        if let Frame::Ack { largest_acked, ack_delay, ranges, ecn_counts } = decoded {
            assert_eq!(largest_acked, 100);
            assert_eq!(ack_delay, 500);
            assert_eq!(ranges.len(), 2);
            assert!(ecn_counts.is_none());
        } else {
            panic!("Expected ACK frame");
        }
    }
    
    #[test]
    fn test_is_ack_eliciting() {
        assert!(!Frame::Padding.is_ack_eliciting());
        assert!(Frame::Ping.is_ack_eliciting());
        
        let ack = Frame::Ack {
            largest_acked: 0,
            ack_delay: 0,
            ranges: vec![],
            ecn_counts: None,
        };
        assert!(!ack.is_ack_eliciting());
    }
}
