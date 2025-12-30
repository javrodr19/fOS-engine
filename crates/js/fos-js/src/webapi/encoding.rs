//! Text Encoding/Decoding
//!
//! TextEncoder and TextDecoder for UTF-8 conversion.

/// TextEncoder - encode strings to UTF-8 bytes
#[derive(Debug, Clone, Default)]
pub struct TextEncoder;

impl TextEncoder {
    pub fn new() -> Self {
        Self
    }
    
    /// Get encoding name
    pub fn encoding(&self) -> &'static str {
        "utf-8"
    }
    
    /// Encode string to bytes
    pub fn encode(&self, input: &str) -> Vec<u8> {
        input.as_bytes().to_vec()
    }
    
    /// Encode into existing buffer
    pub fn encode_into(&self, source: &str, dest: &mut [u8]) -> EncodeResult {
        let bytes = source.as_bytes();
        let written = bytes.len().min(dest.len());
        dest[..written].copy_from_slice(&bytes[..written]);
        
        EncodeResult {
            read: source.chars().take_while(|c| {
                let mut buf = [0; 4];
                c.encode_utf8(&mut buf).len() <= written
            }).count(),
            written,
        }
    }
}

/// Encode result
#[derive(Debug, Clone, Copy)]
pub struct EncodeResult {
    pub read: usize,
    pub written: usize,
}

/// TextDecoder - decode UTF-8 bytes to string
#[derive(Debug, Clone)]
pub struct TextDecoder {
    encoding: String,
    fatal: bool,
    ignore_bom: bool,
}

impl Default for TextDecoder {
    fn default() -> Self {
        Self::new("utf-8")
    }
}

impl TextDecoder {
    pub fn new(label: &str) -> Self {
        Self {
            encoding: label.to_lowercase(),
            fatal: false,
            ignore_bom: false,
        }
    }
    
    pub fn with_options(label: &str, fatal: bool, ignore_bom: bool) -> Self {
        Self {
            encoding: label.to_lowercase(),
            fatal,
            ignore_bom,
        }
    }
    
    /// Get encoding name
    pub fn encoding(&self) -> &str {
        &self.encoding
    }
    
    /// Is fatal mode enabled
    pub fn fatal(&self) -> bool {
        self.fatal
    }
    
    /// Decode bytes to string
    pub fn decode(&self, input: &[u8]) -> Result<String, DecodeError> {
        let mut bytes = input;
        
        // Remove BOM if present
        if !self.ignore_bom && bytes.len() >= 3 {
            if bytes[0] == 0xEF && bytes[1] == 0xBB && bytes[2] == 0xBF {
                bytes = &bytes[3..];
            }
        }
        
        match std::str::from_utf8(bytes) {
            Ok(s) => Ok(s.to_string()),
            Err(e) if self.fatal => Err(DecodeError {
                position: e.valid_up_to(),
            }),
            Err(e) => {
                // Replacement mode
                let valid = &bytes[..e.valid_up_to()];
                let mut result = String::from_utf8_lossy(valid).to_string();
                result.push('\u{FFFD}'); // Replacement character
                Ok(result)
            }
        }
    }
}

/// Decode error
#[derive(Debug, Clone)]
pub struct DecodeError {
    pub position: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_encode() {
        let encoder = TextEncoder::new();
        let bytes = encoder.encode("Hello");
        
        assert_eq!(bytes, b"Hello");
    }
    
    #[test]
    fn test_decode() {
        let decoder = TextDecoder::new("utf-8");
        let result = decoder.decode(b"World").unwrap();
        
        assert_eq!(result, "World");
    }
    
    #[test]
    fn test_decode_with_bom() {
        let decoder = TextDecoder::new("utf-8");
        let with_bom = [0xEF, 0xBB, 0xBF, b'H', b'i'];
        let result = decoder.decode(&with_bom).unwrap();
        
        assert_eq!(result, "Hi");
    }
}
