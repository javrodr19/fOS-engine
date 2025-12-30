//! DEFLATE Decompression (RFC 1951)
//!
//! From-scratch DEFLATE inflator with SIMD-accelerated Adler-32.
//! Used by PNG and gzip streams.

use super::simd::SimdOps;

/// DEFLATE decompression error
#[derive(Debug, Clone)]
pub enum DeflateError {
    InvalidHeader,
    InvalidBlockType,
    InvalidHuffmanCode,
    InvalidDistance,
    InvalidLength,
    ChecksumMismatch,
    UnexpectedEof,
    OutputOverflow,
}

impl std::fmt::Display for DeflateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidHeader => write!(f, "Invalid DEFLATE header"),
            Self::InvalidBlockType => write!(f, "Invalid block type"),
            Self::InvalidHuffmanCode => write!(f, "Invalid Huffman code"),
            Self::InvalidDistance => write!(f, "Invalid distance"),
            Self::InvalidLength => write!(f, "Invalid length"),
            Self::ChecksumMismatch => write!(f, "Checksum mismatch"),
            Self::UnexpectedEof => write!(f, "Unexpected end of data"),
            Self::OutputOverflow => write!(f, "Output buffer overflow"),
        }
    }
}

impl std::error::Error for DeflateError {}

/// DEFLATE inflator (decompressor)
pub struct Inflate {
    simd: SimdOps,
}

impl Inflate {
    pub fn new() -> Self {
        Self {
            simd: SimdOps::new(),
        }
    }

    /// Inflate zlib-wrapped data (2-byte header + DEFLATE + 4-byte Adler-32)
    pub fn inflate_zlib(&self, data: &[u8]) -> Result<Vec<u8>, DeflateError> {
        if data.len() < 6 {
            return Err(DeflateError::InvalidHeader);
        }

        // Parse zlib header
        let cmf = data[0];
        let flg = data[1];

        // Check compression method (must be 8 = deflate)
        if cmf & 0x0F != 8 {
            return Err(DeflateError::InvalidHeader);
        }

        // Check FCHECK
        if (((cmf as u16) << 8) | flg as u16) % 31 != 0 {
            return Err(DeflateError::InvalidHeader);
        }

        // Check for preset dictionary (not supported)
        if flg & 0x20 != 0 {
            return Err(DeflateError::InvalidHeader);
        }

        // Decompress
        let deflate_data = &data[2..data.len() - 4];
        let output = self.inflate_raw(deflate_data)?;

        // Verify Adler-32
        let stored_checksum = u32::from_be_bytes([
            data[data.len() - 4],
            data[data.len() - 3],
            data[data.len() - 2],
            data[data.len() - 1],
        ]);
        let computed_checksum = self.simd.adler32(&output);

        if stored_checksum != computed_checksum {
            return Err(DeflateError::ChecksumMismatch);
        }

        Ok(output)
    }

    /// Inflate raw DEFLATE data (no header/trailer)
    pub fn inflate_raw(&self, data: &[u8]) -> Result<Vec<u8>, DeflateError> {
        let mut reader = BitReader::new(data);
        let mut output = Vec::with_capacity(data.len() * 4);

        loop {
            let bfinal = reader.read_bits(1)? == 1;
            let btype = reader.read_bits(2)?;

            match btype {
                0 => self.inflate_stored(&mut reader, &mut output)?,
                1 => self.inflate_fixed(&mut reader, &mut output)?,
                2 => self.inflate_dynamic(&mut reader, &mut output)?,
                _ => return Err(DeflateError::InvalidBlockType),
            }

            if bfinal {
                break;
            }
        }

        Ok(output)
    }

    /// Inflate non-compressed (stored) block
    fn inflate_stored(&self, reader: &mut BitReader, output: &mut Vec<u8>) -> Result<(), DeflateError> {
        reader.align_to_byte();

        if reader.remaining_bytes() < 4 {
            return Err(DeflateError::UnexpectedEof);
        }

        let len = reader.read_u16_le()?;
        let nlen = reader.read_u16_le()?;

        if len != !nlen {
            return Err(DeflateError::InvalidLength);
        }

        let bytes = reader.read_bytes(len as usize)?;
        output.extend_from_slice(bytes);

        Ok(())
    }

    /// Inflate with fixed Huffman codes
    fn inflate_fixed(&self, reader: &mut BitReader, output: &mut Vec<u8>) -> Result<(), DeflateError> {
        let lit_len_tree = HuffmanTree::fixed_literal_lengths();
        let dist_tree = HuffmanTree::fixed_distances();

        self.inflate_codes(reader, output, &lit_len_tree, &dist_tree)
    }

    /// Inflate with dynamic Huffman codes
    fn inflate_dynamic(&self, reader: &mut BitReader, output: &mut Vec<u8>) -> Result<(), DeflateError> {
        // Read code length counts
        let hlit = reader.read_bits(5)? as usize + 257;
        let hdist = reader.read_bits(5)? as usize + 1;
        let hclen = reader.read_bits(4)? as usize + 4;

        // Read code length alphabet
        const CODE_LENGTH_ORDER: [usize; 19] = [
            16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15
        ];

        let mut code_lengths = [0u8; 19];
        for i in 0..hclen {
            code_lengths[CODE_LENGTH_ORDER[i]] = reader.read_bits(3)? as u8;
        }

        let code_length_tree = HuffmanTree::from_lengths(&code_lengths)?;

        // Read literal/length and distance code lengths
        let mut all_lengths = vec![0u8; hlit + hdist];
        let mut i = 0;

        while i < all_lengths.len() {
            let sym = code_length_tree.decode(reader)?;

            match sym {
                0..=15 => {
                    all_lengths[i] = sym as u8;
                    i += 1;
                }
                16 => {
                    // Repeat previous
                    if i == 0 {
                        return Err(DeflateError::InvalidHuffmanCode);
                    }
                    let count = reader.read_bits(2)? as usize + 3;
                    let prev = all_lengths[i - 1];
                    for _ in 0..count {
                        if i >= all_lengths.len() {
                            return Err(DeflateError::InvalidHuffmanCode);
                        }
                        all_lengths[i] = prev;
                        i += 1;
                    }
                }
                17 => {
                    // Repeat 0, 3-10 times
                    let count = reader.read_bits(3)? as usize + 3;
                    for _ in 0..count {
                        if i >= all_lengths.len() {
                            return Err(DeflateError::InvalidHuffmanCode);
                        }
                        all_lengths[i] = 0;
                        i += 1;
                    }
                }
                18 => {
                    // Repeat 0, 11-138 times
                    let count = reader.read_bits(7)? as usize + 11;
                    for _ in 0..count {
                        if i >= all_lengths.len() {
                            return Err(DeflateError::InvalidHuffmanCode);
                        }
                        all_lengths[i] = 0;
                        i += 1;
                    }
                }
                _ => return Err(DeflateError::InvalidHuffmanCode),
            }
        }

        let lit_len_tree = HuffmanTree::from_lengths(&all_lengths[..hlit])?;
        let dist_tree = HuffmanTree::from_lengths(&all_lengths[hlit..])?;

        self.inflate_codes(reader, output, &lit_len_tree, &dist_tree)
    }

    /// Decode literal/length and distance codes
    fn inflate_codes(
        &self,
        reader: &mut BitReader,
        output: &mut Vec<u8>,
        lit_len_tree: &HuffmanTree,
        dist_tree: &HuffmanTree,
    ) -> Result<(), DeflateError> {
        // Length extra bits
        const LENGTH_BASE: [u16; 29] = [
            3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31,
            35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258
        ];
        const LENGTH_EXTRA: [u8; 29] = [
            0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2,
            3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0
        ];

        // Distance extra bits
        const DIST_BASE: [u16; 30] = [
            1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193,
            257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145,
            8193, 12289, 16385, 24577
        ];
        const DIST_EXTRA: [u8; 30] = [
            0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6,
            7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13
        ];

        loop {
            let sym = lit_len_tree.decode(reader)?;

            if sym < 256 {
                // Literal byte
                output.push(sym as u8);
            } else if sym == 256 {
                // End of block
                break;
            } else {
                // Length/distance pair
                let len_idx = (sym - 257) as usize;
                if len_idx >= LENGTH_BASE.len() {
                    return Err(DeflateError::InvalidLength);
                }

                let length = LENGTH_BASE[len_idx] as usize
                    + reader.read_bits(LENGTH_EXTRA[len_idx] as usize)? as usize;

                let dist_sym = dist_tree.decode(reader)? as usize;
                if dist_sym >= DIST_BASE.len() {
                    return Err(DeflateError::InvalidDistance);
                }

                let distance = DIST_BASE[dist_sym] as usize
                    + reader.read_bits(DIST_EXTRA[dist_sym] as usize)? as usize;

                if distance > output.len() {
                    return Err(DeflateError::InvalidDistance);
                }

                // Copy match using SIMD-aware copy
                let start = output.len();
                output.resize(start + length, 0);
                self.simd.copy_match(output, start, distance, length);
            }
        }

        Ok(())
    }
}

impl Default for Inflate {
    fn default() -> Self {
        Self::new()
    }
}

/// Bit reader for DEFLATE streams
struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bit_pos: usize,
    bit_buffer: u32,
    bits_in_buffer: usize,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            bit_pos: 0,
            bit_buffer: 0,
            bits_in_buffer: 0,
        }
    }

    fn remaining_bytes(&self) -> usize {
        self.data.len() - self.pos
    }

    fn ensure_bits(&mut self, count: usize) -> Result<(), DeflateError> {
        while self.bits_in_buffer < count {
            if self.pos >= self.data.len() {
                return Err(DeflateError::UnexpectedEof);
            }
            self.bit_buffer |= (self.data[self.pos] as u32) << self.bits_in_buffer;
            self.pos += 1;
            self.bits_in_buffer += 8;
        }
        Ok(())
    }

    fn read_bits(&mut self, count: usize) -> Result<u32, DeflateError> {
        if count == 0 {
            return Ok(0);
        }
        self.ensure_bits(count)?;
        let result = self.bit_buffer & ((1 << count) - 1);
        self.bit_buffer >>= count;
        self.bits_in_buffer -= count;
        Ok(result)
    }

    fn peek_bits(&mut self, count: usize) -> Result<u32, DeflateError> {
        self.ensure_bits(count)?;
        Ok(self.bit_buffer & ((1 << count) - 1))
    }

    fn drop_bits(&mut self, count: usize) {
        self.bit_buffer >>= count;
        self.bits_in_buffer -= count;
    }

    fn align_to_byte(&mut self) {
        let extra = self.bits_in_buffer % 8;
        if extra > 0 {
            self.bit_buffer >>= extra;
            self.bits_in_buffer -= extra;
        }
    }

    fn read_u16_le(&mut self) -> Result<u16, DeflateError> {
        self.align_to_byte();
        if self.bits_in_buffer >= 16 {
            let lo = self.bit_buffer as u8;
            self.bit_buffer >>= 8;
            let hi = self.bit_buffer as u8;
            self.bit_buffer >>= 8;
            self.bits_in_buffer -= 16;
            Ok(u16::from_le_bytes([lo, hi]))
        } else {
            if self.pos + 2 > self.data.len() {
                return Err(DeflateError::UnexpectedEof);
            }
            let result = u16::from_le_bytes([self.data[self.pos], self.data[self.pos + 1]]);
            self.pos += 2;
            Ok(result)
        }
    }

    fn read_bytes(&mut self, count: usize) -> Result<&'a [u8], DeflateError> {
        self.align_to_byte();
        self.bits_in_buffer = 0;
        self.bit_buffer = 0;

        if self.pos + count > self.data.len() {
            return Err(DeflateError::UnexpectedEof);
        }
        let result = &self.data[self.pos..self.pos + count];
        self.pos += count;
        Ok(result)
    }
}

/// Huffman tree for DEFLATE decoding
struct HuffmanTree {
    /// Lookup table: index by reversed code bits
    table: Vec<(u16, u8)>, // (symbol, code_length)
    max_bits: usize,
}

impl HuffmanTree {
    /// Create fixed literal/length Huffman tree
    fn fixed_literal_lengths() -> Self {
        let mut lengths = [0u8; 288];
        for i in 0..=143 { lengths[i] = 8; }
        for i in 144..=255 { lengths[i] = 9; }
        for i in 256..=279 { lengths[i] = 7; }
        for i in 280..=287 { lengths[i] = 8; }
        Self::from_lengths(&lengths).unwrap()
    }

    /// Create fixed distance Huffman tree
    fn fixed_distances() -> Self {
        let lengths = [5u8; 32];
        Self::from_lengths(&lengths).unwrap()
    }

    /// Build Huffman tree from code lengths
    fn from_lengths(lengths: &[u8]) -> Result<Self, DeflateError> {
        if lengths.is_empty() {
            return Ok(Self {
                table: vec![(0, 0); 1],
                max_bits: 0,
            });
        }

        let max_bits = *lengths.iter().max().unwrap_or(&0) as usize;
        if max_bits == 0 {
            return Ok(Self {
                table: vec![(0, 0); 1],
                max_bits: 0,
            });
        }

        // Count codes of each length
        let mut bl_count = vec![0u32; max_bits + 1];
        for &len in lengths {
            if len > 0 {
                bl_count[len as usize] += 1;
            }
        }

        // Generate next_code values
        let mut next_code = vec![0u32; max_bits + 1];
        let mut code = 0u32;
        for bits in 1..=max_bits {
            code = (code + bl_count[bits - 1]) << 1;
            next_code[bits] = code;
        }

        // Build lookup table
        let table_size = 1 << max_bits;
        let mut table = vec![(0u16, 0u8); table_size];

        for (sym, &len) in lengths.iter().enumerate() {
            if len == 0 {
                continue;
            }
            let len = len as usize;
            let code = next_code[len];
            next_code[len] += 1;

            // Reverse bits for lookup table indexing
            let reversed = Self::reverse_bits(code, len);

            // Fill all entries that match this code
            let step = 1 << len;
            let mut idx = reversed as usize;
            while idx < table_size {
                table[idx] = (sym as u16, len as u8);
                idx += step;
            }
        }

        Ok(Self { table, max_bits })
    }

    fn reverse_bits(value: u32, bits: usize) -> u32 {
        let mut result = 0;
        let mut v = value;
        for _ in 0..bits {
            result = (result << 1) | (v & 1);
            v >>= 1;
        }
        result
    }

    /// Decode one symbol
    fn decode(&self, reader: &mut BitReader) -> Result<u16, DeflateError> {
        if self.max_bits == 0 {
            return Err(DeflateError::InvalidHuffmanCode);
        }

        let bits = reader.peek_bits(self.max_bits)?;
        let (sym, len) = self.table[bits as usize];

        if len == 0 {
            return Err(DeflateError::InvalidHuffmanCode);
        }

        reader.drop_bits(len as usize);
        Ok(sym)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inflate_stored() {
        // Create a raw stored block: final=1, type=0
        // BFINAL=1, BTYPE=00, then 5 bytes padding, LEN=4, NLEN=~4, "test"
        let data = [
            0b00000001, // bfinal=1, btype=00 (stored), 5 bits padding
            0x04, 0x00, // LEN = 4
            0xFB, 0xFF, // NLEN = ~4
            b't', b'e', b's', b't',
        ];

        let inflate = Inflate::new();
        let result = inflate.inflate_raw(&data).unwrap();
        assert_eq!(&result, b"test");
    }

    #[test]
    fn test_huffman_fixed_decode() {
        let tree = HuffmanTree::fixed_literal_lengths();
        // Check that tree was built
        assert!(tree.max_bits > 0);
    }

    #[test]
    fn test_zlib_header_check() {
        let inflate = Inflate::new();

        // Invalid (too short)
        assert!(inflate.inflate_zlib(&[0x78]).is_err());

        // Valid header but no data
        assert!(inflate.inflate_zlib(&[0x78, 0x9C, 0, 0, 0, 0]).is_err());
    }
}
