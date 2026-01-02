//! Local Compression Utilities for fos-js
//!
//! Minimal LZ4-style compression to avoid cyclic dependencies with fos-engine.

/// Simple LZ4-style compressor
pub struct Lz4Compressor;

impl Lz4Compressor {
    /// Compress data using LZ4-like algorithm
    /// Returns compressed bytes with a simple header
    pub fn compress(data: &[u8]) -> Vec<u8> {
        if data.len() < 16 {
            // Too small to compress - store as-is with marker
            let mut result = Vec::with_capacity(data.len() + 5);
            result.push(0); // Uncompressed marker
            result.extend_from_slice(&(data.len() as u32).to_le_bytes());
            result.extend_from_slice(data);
            return result;
        }

        // Simple RLE + literal compression
        let mut result = Vec::with_capacity(data.len());
        result.push(1); // Compressed marker
        result.extend_from_slice(&(data.len() as u32).to_le_bytes());

        let mut i = 0;
        while i < data.len() {
            // Look for runs
            let mut run_len = 1;
            while i + run_len < data.len() 
                && data[i] == data[i + run_len] 
                && run_len < 255 
            {
                run_len += 1;
            }

            if run_len >= 4 {
                // Encode as run: [0xFF, byte, length]
                result.push(0xFF);
                result.push(data[i]);
                result.push(run_len as u8);
                i += run_len;
            } else {
                // Store as literal
                if data[i] == 0xFF {
                    result.push(0xFE); // Escape
                }
                result.push(data[i]);
                i += 1;
            }
        }

        // Only use compressed if smaller
        if result.len() >= data.len() + 5 {
            let mut uncompressed = Vec::with_capacity(data.len() + 5);
            uncompressed.push(0);
            uncompressed.extend_from_slice(&(data.len() as u32).to_le_bytes());
            uncompressed.extend_from_slice(data);
            return uncompressed;
        }

        result
    }

    /// Decompress data
    pub fn decompress(data: &[u8]) -> Option<Vec<u8>> {
        if data.len() < 5 {
            return None;
        }

        let marker = data[0];
        let orig_len = u32::from_le_bytes([data[1], data[2], data[3], data[4]]) as usize;

        if marker == 0 {
            // Uncompressed
            if data.len() < 5 + orig_len {
                return None;
            }
            return Some(data[5..5 + orig_len].to_vec());
        }

        // Decompress
        let mut result = Vec::with_capacity(orig_len);
        let mut i = 5;

        while i < data.len() && result.len() < orig_len {
            match data[i] {
                0xFF => {
                    // Run
                    if i + 2 >= data.len() {
                        break;
                    }
                    let byte = data[i + 1];
                    let len = data[i + 2] as usize;
                    result.extend(std::iter::repeat(byte).take(len));
                    i += 3;
                }
                0xFE => {
                    // Escaped 0xFF
                    i += 1;
                    if i < data.len() {
                        result.push(data[i]);
                        i += 1;
                    }
                }
                b => {
                    result.push(b);
                    i += 1;
                }
            }
        }

        if result.len() == orig_len {
            Some(result)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compress_decompress() {
        let data = b"Hello, World! This is a test of compression.";
        let compressed = Lz4Compressor::compress(data);
        let decompressed = Lz4Compressor::decompress(&compressed).unwrap();
        assert_eq!(data.as_slice(), decompressed.as_slice());
    }

    #[test]
    fn test_compress_runs() {
        let data = vec![0u8; 100]; // All zeros
        let compressed = Lz4Compressor::compress(&data);
        assert!(compressed.len() < data.len());
        let decompressed = Lz4Compressor::decompress(&compressed).unwrap();
        assert_eq!(data, decompressed);
    }

    #[test]
    fn test_small_data() {
        let data = b"tiny";
        let compressed = Lz4Compressor::compress(data);
        let decompressed = Lz4Compressor::decompress(&compressed).unwrap();
        assert_eq!(data.as_slice(), decompressed.as_slice());
    }
}
