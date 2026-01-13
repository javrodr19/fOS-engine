//! QPACK Header Compression
//!
//! QPACK encoder and decoder for HTTP/3 per RFC 9204.

use std::collections::VecDeque;

/// QPACK static table entries (RFC 9204 Appendix A)
const STATIC_TABLE: &[(& str, &str)] = &[
    (":authority", ""),
    (":path", "/"),
    ("age", "0"),
    ("content-disposition", ""),
    ("content-length", "0"),
    ("cookie", ""),
    ("date", ""),
    ("etag", ""),
    ("if-modified-since", ""),
    ("if-none-match", ""),
    ("last-modified", ""),
    ("link", ""),
    ("location", ""),
    ("referer", ""),
    ("set-cookie", ""),
    (":method", "CONNECT"),
    (":method", "DELETE"),
    (":method", "GET"),
    (":method", "HEAD"),
    (":method", "OPTIONS"),
    (":method", "POST"),
    (":method", "PUT"),
    (":scheme", "http"),
    (":scheme", "https"),
    (":status", "103"),
    (":status", "200"),
    (":status", "304"),
    (":status", "404"),
    (":status", "503"),
    ("accept", "*/*"),
    ("accept", "application/dns-message"),
    ("accept-encoding", "gzip, deflate, br"),
    ("accept-ranges", "bytes"),
    ("access-control-allow-headers", "cache-control"),
    ("access-control-allow-headers", "content-type"),
    ("access-control-allow-origin", "*"),
    ("cache-control", "max-age=0"),
    ("cache-control", "max-age=2592000"),
    ("cache-control", "max-age=604800"),
    ("cache-control", "no-cache"),
    ("cache-control", "no-store"),
    ("cache-control", "public, max-age=31536000"),
    ("content-encoding", "br"),
    ("content-encoding", "gzip"),
    ("content-type", "application/dns-message"),
    ("content-type", "application/javascript"),
    ("content-type", "application/json"),
    ("content-type", "application/x-www-form-urlencoded"),
    ("content-type", "image/gif"),
    ("content-type", "image/jpeg"),
    ("content-type", "image/png"),
    ("content-type", "text/css"),
    ("content-type", "text/html; charset=utf-8"),
    ("content-type", "text/plain"),
    ("content-type", "text/plain;charset=utf-8"),
    ("range", "bytes=0-"),
    ("strict-transport-security", "max-age=31536000"),
    ("strict-transport-security", "max-age=31536000; includesubdomains"),
    ("strict-transport-security", "max-age=31536000; includesubdomains; preload"),
    ("vary", "accept-encoding"),
    ("vary", "origin"),
    ("x-content-type-options", "nosniff"),
    ("x-xss-protection", "1; mode=block"),
    (":status", "100"),
    (":status", "204"),
    (":status", "206"),
    (":status", "302"),
    (":status", "400"),
    (":status", "403"),
    (":status", "421"),
    (":status", "425"),
    (":status", "500"),
    ("accept-language", ""),
    ("access-control-allow-credentials", "FALSE"),
    ("access-control-allow-credentials", "TRUE"),
    ("access-control-allow-headers", "*"),
    ("access-control-allow-methods", "get"),
    ("access-control-allow-methods", "get, post, options"),
    ("access-control-allow-methods", "options"),
    ("access-control-expose-headers", "content-length"),
    ("access-control-request-headers", "content-type"),
    ("access-control-request-method", "get"),
    ("access-control-request-method", "post"),
    ("alt-svc", "clear"),
    ("authorization", ""),
    ("content-security-policy", "script-src 'none'; object-src 'none'; base-uri 'none'"),
    ("early-data", "1"),
    ("expect-ct", ""),
    ("forwarded", ""),
    ("if-range", ""),
    ("origin", ""),
    ("purpose", "prefetch"),
    ("server", ""),
    ("timing-allow-origin", "*"),
    ("upgrade-insecure-requests", "1"),
    ("user-agent", ""),
    ("x-forwarded-for", ""),
    ("x-frame-options", "deny"),
    ("x-frame-options", "sameorigin"),
];

/// Dynamic table entry
#[derive(Debug, Clone)]
struct DynamicEntry {
    name: String,
    value: String,
    size: usize,
}

impl DynamicEntry {
    fn new(name: String, value: String) -> Self {
        let size = name.len() + value.len() + 32; // 32 byte overhead per RFC 9204
        Self { name, value, size }
    }
}

/// QPACK encoder
#[derive(Debug)]
pub struct QpackEncoder {
    /// Dynamic table
    dynamic_table: VecDeque<DynamicEntry>,
    /// Current table size in bytes
    table_size: usize,
    /// Maximum table capacity
    max_capacity: usize,
    /// Number of entries inserted
    insert_count: u64,
    /// Known received count (acknowledged by decoder)
    known_received: u64,
    /// Maximum blocked streams
    max_blocked: u64,
}

impl QpackEncoder {
    /// Create new encoder
    pub fn new(max_capacity: usize) -> Self {
        Self {
            dynamic_table: VecDeque::new(),
            table_size: 0,
            max_capacity,
            insert_count: 0,
            known_received: 0,
            max_blocked: 100,
        }
    }
    
    /// Encode a header field into bytes
    pub fn encode_header(&mut self, name: &str, value: &str, buf: &mut Vec<u8>) {
        let name_lower = name.to_lowercase();
        
        // Try static table first
        if let Some((idx, has_value)) = self.find_static(&name_lower, value) {
            if has_value {
                // Indexed header field (static)
                self.encode_indexed_static(idx, buf);
            } else {
                // Literal with name reference (static)
                self.encode_literal_static_name(idx, value, buf);
            }
            return;
        }
        
        // Literal header field with literal name
        self.encode_literal(name, value, buf);
    }
    
    /// Find in static table, returns (index, has_value_match)
    fn find_static(&self, name: &str, value: &str) -> Option<(usize, bool)> {
        let mut name_match = None;
        
        for (i, (n, v)) in STATIC_TABLE.iter().enumerate() {
            if *n == name {
                if *v == value {
                    return Some((i, true));
                }
                if name_match.is_none() {
                    name_match = Some(i);
                }
            }
        }
        
        name_match.map(|i| (i, false))
    }
    
    /// Encode indexed header field from static table
    fn encode_indexed_static(&self, index: usize, buf: &mut Vec<u8>) {
        // 1xxxxxxx (indexed, static)
        if index < 64 {
            buf.push(0xC0 | (index as u8));
        } else {
            buf.push(0xFF);
            self.encode_integer(index - 63, 0, buf);
        }
    }
    
    /// Encode literal with static name reference
    fn encode_literal_static_name(&self, name_index: usize, value: &str, buf: &mut Vec<u8>) {
        // 01xxxxxx (literal with name reference, static, no indexing)
        if name_index < 16 {
            buf.push(0x50 | (name_index as u8));
        } else {
            buf.push(0x5F);
            self.encode_integer(name_index - 15, 0, buf);
        }
        
        // Encode value
        self.encode_string(value, buf);
    }
    
    /// Encode literal header field
    fn encode_literal(&self, name: &str, value: &str, buf: &mut Vec<u8>) {
        // 001xxxxx (literal without name reference)
        buf.push(0x20);
        
        // Encode name
        self.encode_string(name, buf);
        
        // Encode value
        self.encode_string(value, buf);
    }
    
    /// Encode a QPACK integer
    fn encode_integer(&self, mut value: usize, prefix_bits: u8, buf: &mut Vec<u8>) {
        if prefix_bits > 0 && value < (1 << prefix_bits) - 1 {
            // Fits in prefix
            return;
        }
        
        while value >= 128 {
            buf.push(0x80 | ((value & 0x7F) as u8));
            value >>= 7;
        }
        buf.push(value as u8);
    }
    
    /// Encode a string (not Huffman encoded for simplicity)
    fn encode_string(&self, s: &str, buf: &mut Vec<u8>) {
        let bytes = s.as_bytes();
        
        // Length with H=0 (not Huffman)
        if bytes.len() < 127 {
            buf.push(bytes.len() as u8);
        } else {
            buf.push(0x7F);
            self.encode_integer(bytes.len() - 127, 0, buf);
        }
        
        buf.extend_from_slice(bytes);
    }
    
    /// Encode header block prefix
    pub fn encode_prefix(&self, buf: &mut Vec<u8>) {
        // Required Insert Count (0 for no dynamic table usage)
        buf.push(0x00);
        // Delta Base (0)
        buf.push(0x00);
    }
    
    /// Encode multiple headers
    pub fn encode_headers(&mut self, headers: &[(&str, &str)], buf: &mut Vec<u8>) {
        // Add prefix
        self.encode_prefix(buf);
        
        // Encode each header
        for (name, value) in headers {
            self.encode_header(name, value, buf);
        }
    }
}

impl Default for QpackEncoder {
    fn default() -> Self {
        Self::new(4096)
    }
}

/// QPACK decoder
#[derive(Debug)]
pub struct QpackDecoder {
    /// Dynamic table
    dynamic_table: VecDeque<DynamicEntry>,
    /// Current table size
    table_size: usize,
    /// Maximum capacity
    max_capacity: usize,
    /// Known received count
    known_received: u64,
}

impl QpackDecoder {
    /// Create new decoder
    pub fn new(max_capacity: usize) -> Self {
        Self {
            dynamic_table: VecDeque::new(),
            table_size: 0,
            max_capacity,
            known_received: 0,
        }
    }
    
    /// Decode header block
    pub fn decode_headers(&mut self, data: &[u8]) -> Result<Vec<(String, String)>, QpackError> {
        if data.len() < 2 {
            return Err(QpackError::InvalidData);
        }
        
        let mut pos = 0;
        
        // Decode prefix
        let (_ric, n) = self.decode_integer(&data[pos..], 8)?;
        pos += n;
        
        let (_delta_base, n) = self.decode_integer(&data[pos..], 7)?;
        pos += n;
        
        let mut headers = Vec::new();
        
        while pos < data.len() {
            let (name, value, consumed) = self.decode_header(&data[pos..])?;
            headers.push((name, value));
            pos += consumed;
        }
        
        Ok(headers)
    }
    
    /// Decode a single header
    fn decode_header(&self, data: &[u8]) -> Result<(String, String, usize), QpackError> {
        if data.is_empty() {
            return Err(QpackError::InvalidData);
        }
        
        let first = data[0];
        let mut pos = 0;
        
        if first & 0x80 != 0 {
            // Indexed header field
            let is_static = (first & 0x40) != 0;
            let (index, n) = self.decode_integer(&data[pos..], 6)?;
            pos += n;
            
            if is_static {
                if index >= STATIC_TABLE.len() {
                    return Err(QpackError::InvalidIndex);
                }
                let (name, value) = STATIC_TABLE[index];
                Ok((name.to_string(), value.to_string(), pos))
            } else {
                Err(QpackError::DynamicTableNotSupported)
            }
        } else if first & 0x40 != 0 {
            // Literal with name reference
            let is_static = (first & 0x10) != 0;
            let (index, n) = self.decode_integer(&data[pos..], 4)?;
            pos += n;
            
            let (value, n) = self.decode_string(&data[pos..])?;
            pos += n;
            
            if is_static {
                if index >= STATIC_TABLE.len() {
                    return Err(QpackError::InvalidIndex);
                }
                let (name, _) = STATIC_TABLE[index];
                Ok((name.to_string(), value, pos))
            } else {
                Err(QpackError::DynamicTableNotSupported)
            }
        } else if first & 0x20 != 0 {
            // Literal without name reference
            let (name, n) = self.decode_string(&data[pos + 1..])?;
            pos += 1 + n;
            
            let (value, n) = self.decode_string(&data[pos..])?;
            pos += n;
            
            Ok((name, value, pos))
        } else {
            Err(QpackError::InvalidData)
        }
    }
    
    /// Decode a QPACK integer
    fn decode_integer(&self, data: &[u8], prefix_bits: usize) -> Result<(usize, usize), QpackError> {
        if data.is_empty() {
            return Err(QpackError::InvalidData);
        }
        
        let prefix_mask = (1 << prefix_bits) - 1;
        let mut value = (data[0] as usize) & prefix_mask;
        
        if value < prefix_mask {
            return Ok((value, 1));
        }
        
        let mut pos = 1;
        let mut m = 0;
        
        loop {
            if pos >= data.len() {
                return Err(QpackError::InvalidData);
            }
            
            let b = data[pos] as usize;
            value += (b & 0x7F) << m;
            m += 7;
            pos += 1;
            
            if b & 0x80 == 0 {
                break;
            }
        }
        
        Ok((value, pos))
    }
    
    /// Decode a string
    fn decode_string(&self, data: &[u8]) -> Result<(String, usize), QpackError> {
        if data.is_empty() {
            return Err(QpackError::InvalidData);
        }
        
        let huffman = (data[0] & 0x80) != 0;
        let (length, n) = self.decode_integer(data, 7)?;
        
        if data.len() < n + length {
            return Err(QpackError::InvalidData);
        }
        
        let bytes = &data[n..n + length];
        
        let s = if huffman {
            // Huffman decoding not implemented, return error
            return Err(QpackError::HuffmanNotSupported);
        } else {
            String::from_utf8_lossy(bytes).to_string()
        };
        
        Ok((s, n + length))
    }
}

impl Default for QpackDecoder {
    fn default() -> Self {
        Self::new(4096)
    }
}

/// QPACK error
#[derive(Debug, Clone, thiserror::Error)]
pub enum QpackError {
    #[error("Invalid data")]
    InvalidData,
    
    #[error("Invalid index")]
    InvalidIndex,
    
    #[error("Dynamic table not supported")]
    DynamicTableNotSupported,
    
    #[error("Huffman encoding not supported")]
    HuffmanNotSupported,
    
    #[error("Decoder stream error")]
    DecoderStreamError,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_encoder_creation() {
        let encoder = QpackEncoder::new(4096);
        assert_eq!(encoder.table_size, 0);
    }
    
    #[test]
    fn test_encode_static_indexed() {
        let mut encoder = QpackEncoder::new(4096);
        let mut buf = Vec::new();
        
        // :method GET should use static table
        encoder.encode_header(":method", "GET", &mut buf);
        
        assert!(!buf.is_empty());
    }
    
    #[test]
    fn test_encode_literal() {
        let mut encoder = QpackEncoder::new(4096);
        let mut buf = Vec::new();
        
        encoder.encode_header("x-custom-header", "custom-value", &mut buf);
        
        assert!(!buf.is_empty());
    }
    
    #[test]
    fn test_encode_headers() {
        let mut encoder = QpackEncoder::new(4096);
        let headers = [
            (":method", "GET"),
            (":path", "/"),
            (":scheme", "https"),
            ("accept", "*/*"),
        ];
        
        let mut buf = Vec::new();
        encoder.encode_headers(&headers, &mut buf);
        
        assert!(buf.len() > 2); // At least prefix + headers
    }
    
    #[test]
    fn test_decoder_creation() {
        let decoder = QpackDecoder::new(4096);
        assert_eq!(decoder.table_size, 0);
    }
    
    #[test]
    fn test_decode_integer() {
        let decoder = QpackDecoder::new(4096);
        
        // Small value
        let (val, len) = decoder.decode_integer(&[0x05], 8).unwrap();
        assert_eq!(val, 5);
        assert_eq!(len, 1);
        
        // Value at prefix boundary
        let (val, len) = decoder.decode_integer(&[0x1F, 0x00], 5).unwrap();
        assert_eq!(val, 31);
        assert_eq!(len, 2);
    }
    
    #[test]
    fn test_decode_string() {
        let decoder = QpackDecoder::new(4096);
        
        // Non-Huffman string "test"
        let data = [0x04, b't', b'e', b's', b't'];
        let (s, len) = decoder.decode_string(&data).unwrap();
        
        assert_eq!(s, "test");
        assert_eq!(len, 5);
    }
    
    #[test]
    fn test_static_table() {
        assert_eq!(STATIC_TABLE[17], (":method", "GET"));
        assert_eq!(STATIC_TABLE[23], (":scheme", "https"));
        assert_eq!(STATIC_TABLE[25], (":status", "200"));
    }
}
