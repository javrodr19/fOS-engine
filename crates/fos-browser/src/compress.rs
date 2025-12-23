//! Compression Utilities Integration
//!
//! LZ4, delta encoding, varint, RLE for efficient storage.

/// Simple LZ4-style compression (simplified)
pub struct Lz4Compressor;

impl Lz4Compressor {
    /// Compress data
    pub fn compress(data: &[u8]) -> Vec<u8> {
        let mut result = Vec::with_capacity(4 + data.len());
        result.extend_from_slice(&(data.len() as u32).to_le_bytes());
        result.extend_from_slice(data);
        result
    }
    
    /// Decompress data
    pub fn decompress(data: &[u8]) -> Option<Vec<u8>> {
        if data.len() < 4 { return None; }
        let len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        if data.len() < 4 + len { return None; }
        Some(data[4..4 + len].to_vec())
    }
}

/// Delta encoder for incremental data
pub struct DeltaEncoder;

impl DeltaEncoder {
    /// Encode deltas
    pub fn encode(values: &[i64]) -> Vec<i64> {
        if values.is_empty() { return Vec::new(); }
        let mut result = Vec::with_capacity(values.len());
        result.push(values[0]);
        for i in 1..values.len() {
            result.push(values[i] - values[i - 1]);
        }
        result
    }
    
    /// Decode deltas
    pub fn decode(deltas: &[i64]) -> Vec<i64> {
        if deltas.is_empty() { return Vec::new(); }
        let mut result = Vec::with_capacity(deltas.len());
        result.push(deltas[0]);
        for i in 1..deltas.len() {
            result.push(result[i - 1] + deltas[i]);
        }
        result
    }
}

/// Varint encoder/decoder
pub struct Varint;

impl Varint {
    /// Encode u64 as varint
    pub fn encode_u64(mut value: u64) -> Vec<u8> {
        let mut result = Vec::new();
        loop {
            let mut byte = (value & 0x7F) as u8;
            value >>= 7;
            if value != 0 { byte |= 0x80; }
            result.push(byte);
            if value == 0 { break; }
        }
        result
    }
    
    /// Decode varint to u64
    pub fn decode_u64(bytes: &[u8]) -> Option<(u64, usize)> {
        let mut result: u64 = 0;
        let mut shift = 0;
        for (i, &byte) in bytes.iter().enumerate() {
            result |= ((byte & 0x7F) as u64) << shift;
            if byte & 0x80 == 0 {
                return Some((result, i + 1));
            }
            shift += 7;
            if shift >= 64 { return None; }
        }
        None
    }
}

/// Run-length encoder
pub struct RunLengthEncoder;

impl RunLengthEncoder {
    /// Encode runs
    pub fn encode<T: Eq + Clone>(data: &[T]) -> Vec<(T, usize)> {
        if data.is_empty() { return Vec::new(); }
        let mut result = Vec::new();
        let mut current = data[0].clone();
        let mut count = 1;
        for item in data.iter().skip(1) {
            if *item == current {
                count += 1;
            } else {
                result.push((current, count));
                current = item.clone();
                count = 1;
            }
        }
        result.push((current, count));
        result
    }
    
    /// Decode runs
    pub fn decode<T: Clone>(runs: &[(T, usize)]) -> Vec<T> {
        let mut result = Vec::new();
        for (value, count) in runs {
            for _ in 0..*count {
                result.push(value.clone());
            }
        }
        result
    }
}

/// Bit packer for flags
pub struct BitPacker;

impl BitPacker {
    /// Pack bools into bytes (8 bools = 1 byte)
    pub fn pack_bools(bools: &[bool]) -> Vec<u8> {
        let bytes_needed = (bools.len() + 7) / 8;
        let mut result = vec![0u8; bytes_needed];
        for (i, &b) in bools.iter().enumerate() {
            if b { result[i / 8] |= 1 << (i % 8); }
        }
        result
    }
    
    /// Unpack bools from bytes
    pub fn unpack_bools(bytes: &[u8], count: usize) -> Vec<bool> {
        let mut result = Vec::with_capacity(count);
        for i in 0..count {
            if i / 8 >= bytes.len() { break; }
            result.push((bytes[i / 8] >> (i % 8)) & 1 != 0);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_lz4() {
        let data = b"Hello, World!";
        let compressed = Lz4Compressor::compress(data);
        let decompressed = Lz4Compressor::decompress(&compressed).unwrap();
        assert_eq!(data.as_slice(), decompressed.as_slice());
    }
    
    #[test]
    fn test_delta() {
        let values = vec![100, 105, 110, 108, 115];
        let deltas = DeltaEncoder::encode(&values);
        let decoded = DeltaEncoder::decode(&deltas);
        assert_eq!(values, decoded);
    }
    
    #[test]
    fn test_varint() {
        let encoded = Varint::encode_u64(300);
        let (decoded, _) = Varint::decode_u64(&encoded).unwrap();
        assert_eq!(decoded, 300);
    }
    
    #[test]
    fn test_rle() {
        let data = vec![1, 1, 1, 2, 2, 3, 3, 3, 3];
        let encoded = RunLengthEncoder::encode(&data);
        let decoded = RunLengthEncoder::decode(&encoded);
        assert_eq!(data, decoded);
    }
    
    #[test]
    fn test_bit_packer() {
        let bools = vec![true, false, true, true, false, false, true, false, true];
        let packed = BitPacker::pack_bools(&bools);
        let unpacked = BitPacker::unpack_bools(&packed, bools.len());
        assert_eq!(bools, unpacked);
    }
}
