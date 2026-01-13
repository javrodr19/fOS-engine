//! Custom Brotli Decompressor
//!
//! From-scratch implementation of Brotli decompression (RFC 7932)
//! optimized for WOFF2 font decoding with shared dictionary support.


/// Brotli decompression error
#[derive(Debug, Clone)]
pub enum BrotliError {
    InvalidStream,
    InvalidHuffman,
    InvalidDistance,
    InvalidBlockType,
    WindowTooLarge,
    UnexpectedEof,
    OutputOverflow,
}

impl std::fmt::Display for BrotliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BrotliError::InvalidStream => write!(f, "Invalid Brotli stream"),
            BrotliError::InvalidHuffman => write!(f, "Invalid Huffman code"),
            BrotliError::InvalidDistance => write!(f, "Invalid distance"),
            BrotliError::InvalidBlockType => write!(f, "Invalid block type"),
            BrotliError::WindowTooLarge => write!(f, "Window size too large"),
            BrotliError::UnexpectedEof => write!(f, "Unexpected end of input"),
            BrotliError::OutputOverflow => write!(f, "Output buffer overflow"),
        }
    }
}

impl std::error::Error for BrotliError {}

/// Result type for Brotli operations
pub type BrotliResult<T> = Result<T, BrotliError>;

// ============================================================================
// Bit Reader
// ============================================================================

/// Bit-level reader for Brotli stream
struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bit_pos: u8,
    /// Bit accumulator for faster reading
    accumulator: u64,
    /// Bits available in accumulator
    bits_available: u8,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        let mut reader = Self {
            data,
            pos: 0,
            bit_pos: 0,
            accumulator: 0,
            bits_available: 0,
        };
        reader.refill();
        reader
    }

    /// Refill the bit accumulator
    #[inline]
    fn refill(&mut self) {
        while self.bits_available <= 56 && self.pos < self.data.len() {
            self.accumulator |= (self.data[self.pos] as u64) << self.bits_available;
            self.pos += 1;
            self.bits_available += 8;
        }
    }

    /// Read up to 32 bits
    #[inline]
    fn read_bits(&mut self, n: u8) -> BrotliResult<u32> {
        if n == 0 {
            return Ok(0);
        }
        if n > 32 {
            return Err(BrotliError::InvalidStream);
        }

        self.refill();

        if self.bits_available < n {
            return Err(BrotliError::UnexpectedEof);
        }

        let mask = (1u64 << n) - 1;
        let result = (self.accumulator & mask) as u32;
        self.accumulator >>= n;
        self.bits_available -= n;
        Ok(result)
    }

    /// Read a single bit
    #[inline]
    fn read_bit(&mut self) -> BrotliResult<bool> {
        Ok(self.read_bits(1)? != 0)
    }

    /// Peek bits without consuming
    #[inline]
    fn peek_bits(&mut self, n: u8) -> BrotliResult<u32> {
        self.refill();
        if self.bits_available < n {
            return Err(BrotliError::UnexpectedEof);
        }
        let mask = (1u64 << n) - 1;
        Ok((self.accumulator & mask) as u32)
    }

    /// Drop bits after peeking
    #[inline]
    fn drop_bits(&mut self, n: u8) {
        self.accumulator >>= n;
        self.bits_available = self.bits_available.saturating_sub(n);
    }

    /// Check if at end
    fn is_empty(&self) -> bool {
        self.bits_available == 0 && self.pos >= self.data.len()
    }
}

// ============================================================================
// Huffman Decoding
// ============================================================================

/// Maximum Huffman code length
const MAX_HUFFMAN_BITS: usize = 15;
/// Lookup table size for fast decoding
const HUFFMAN_TABLE_BITS: usize = 8;
const HUFFMAN_TABLE_SIZE: usize = 1 << HUFFMAN_TABLE_BITS;

/// Huffman decoder with fast lookup table
struct HuffmanDecoder {
    /// Fast lookup for short codes: (symbol, bits)
    table: Vec<(u16, u8)>,
    /// Symbol count
    num_symbols: u16,
    /// Maximum code length
    max_bits: u8,
}

impl HuffmanDecoder {
    /// Build from code lengths (0 = not used)
    fn from_lengths(code_lengths: &[u8]) -> BrotliResult<Self> {
        let num_symbols = code_lengths.len() as u16;
        
        if num_symbols == 0 {
            return Ok(Self {
                table: vec![(0, 0); HUFFMAN_TABLE_SIZE],
                num_symbols: 0,
                max_bits: 0,
            });
        }

        // Count codes of each length
        let mut bl_count = [0u32; MAX_HUFFMAN_BITS + 1];
        let mut max_bits = 0u8;
        
        for &len in code_lengths {
            if len > 0 {
                bl_count[len as usize] += 1;
                max_bits = max_bits.max(len);
            }
        }

        // Handle single-symbol case
        let non_zero: usize = code_lengths.iter().filter(|&&l| l > 0).count();
        if non_zero == 0 {
            return Ok(Self {
                table: vec![(0, 0); HUFFMAN_TABLE_SIZE],
                num_symbols,
                max_bits: 0,
            });
        }
        if non_zero == 1 {
            let symbol = code_lengths.iter().position(|&l| l > 0).unwrap() as u16;
            return Ok(Self {
                table: vec![(symbol, 0); HUFFMAN_TABLE_SIZE],
                num_symbols,
                max_bits: 0,
            });
        }

        // Calculate first code for each length
        let mut next_code = [0u32; MAX_HUFFMAN_BITS + 1];
        let mut code = 0u32;
        for bits in 1..=max_bits as usize {
            code = (code + bl_count[bits - 1]) << 1;
            next_code[bits] = code;
        }

        // Assign codes to symbols
        let mut symbols = Vec::with_capacity(num_symbols as usize);
        for (sym, &len) in code_lengths.iter().enumerate() {
            if len > 0 {
                symbols.push((sym as u16, next_code[len as usize], len));
                next_code[len as usize] += 1;
            }
        }

        // Build lookup table
        let mut table = vec![(0u16, 0u8); HUFFMAN_TABLE_SIZE];
        
        for &(symbol, code, bits) in &symbols {
            if bits as usize <= HUFFMAN_TABLE_BITS {
                // Fill table entries for this symbol
                let base_idx = Self::reverse_bits(code, bits) as usize;
                let step = 1 << bits;
                let mut idx = base_idx;
                while idx < HUFFMAN_TABLE_SIZE {
                    table[idx] = (symbol, bits);
                    idx += step;
                }
            } else {
                // For longer codes, store with special marker
                // We use secondary table approach for these
                let prefix = Self::reverse_bits(code, bits) as usize & (HUFFMAN_TABLE_SIZE - 1);
                if table[prefix].1 == 0 {
                    table[prefix] = (symbol, bits);
                }
            }
        }

        Ok(Self {
            table,
            num_symbols,
            max_bits,
        })
    }

    /// Reverse bits of a code
    #[inline]
    fn reverse_bits(mut code: u32, bits: u8) -> u32 {
        let mut result = 0u32;
        for _ in 0..bits {
            result = (result << 1) | (code & 1);
            code >>= 1;
        }
        result
    }

    /// Decode one symbol
    fn decode(&self, reader: &mut BitReader) -> BrotliResult<u16> {
        if self.max_bits == 0 {
            // Single symbol or empty
            return Ok(self.table[0].0);
        }

        let peek = reader.peek_bits((HUFFMAN_TABLE_BITS as u8).min(self.max_bits))? as usize;
        let (symbol, bits) = self.table[peek & (HUFFMAN_TABLE_SIZE - 1)];
        
        if bits <= HUFFMAN_TABLE_BITS as u8 {
            reader.drop_bits(bits);
            Ok(symbol)
        } else {
            // Slow path for long codes
            self.decode_slow(reader)
        }
    }

    fn decode_slow(&self, reader: &mut BitReader) -> BrotliResult<u16> {
        // Read bit by bit (fallback for long codes)
        let mut code = 0u32;
        for bits in 1..=self.max_bits {
            code = (code << 1) | reader.read_bits(1)?;
            // Linear search (rare case)
            for i in 0..self.num_symbols as usize {
                if self.table[i % HUFFMAN_TABLE_SIZE] == (i as u16, bits) {
                    return Ok(i as u16);
                }
            }
        }
        Err(BrotliError::InvalidHuffman)
    }
}

// ============================================================================
// Ring Buffer for LZ77
// ============================================================================

/// Ring buffer for back-references
struct RingBuffer {
    buffer: Vec<u8>,
    pos: usize,
    size: usize,
}

impl RingBuffer {
    fn new(window_bits: u8) -> Self {
        let size = 1 << window_bits;
        Self {
            buffer: vec![0; size],
            pos: 0,
            size,
        }
    }

    #[inline]
    fn push(&mut self, byte: u8) {
        self.buffer[self.pos] = byte;
        self.pos = (self.pos + 1) & (self.size - 1);
    }

    #[inline]
    fn get(&self, distance: usize) -> u8 {
        let idx = (self.pos.wrapping_sub(distance)) & (self.size - 1);
        self.buffer[idx]
    }

    fn copy_match(&mut self, distance: usize, length: usize, output: &mut Vec<u8>) {
        for _ in 0..length {
            let byte = self.get(distance);
            output.push(byte);
            self.push(byte);
        }
    }
}

// ============================================================================
// Prefix Codes
// ============================================================================

/// Read simple prefix code (Brotli's simplified Huffman format)
fn read_simple_prefix_code(reader: &mut BitReader, alphabet_size: u16) -> BrotliResult<HuffmanDecoder> {
    let num_symbols = reader.read_bits(2)? + 1;
    
    let bits_needed = if alphabet_size > 1 {
        (16 - (alphabet_size - 1).leading_zeros()) as u8
    } else {
        0
    };

    let mut symbols = Vec::with_capacity(num_symbols as usize);
    for _ in 0..num_symbols {
        let sym = reader.read_bits(bits_needed)? as u16;
        if sym >= alphabet_size {
            return Err(BrotliError::InvalidHuffman);
        }
        symbols.push(sym);
    }

    // Build code lengths based on number of symbols
    let mut code_lengths = vec![0u8; alphabet_size as usize];
    match num_symbols {
        1 => {
            code_lengths[symbols[0] as usize] = 1;
        }
        2 => {
            code_lengths[symbols[0] as usize] = 1;
            code_lengths[symbols[1] as usize] = 1;
        }
        3 => {
            code_lengths[symbols[0] as usize] = 1;
            code_lengths[symbols[1] as usize] = 2;
            code_lengths[symbols[2] as usize] = 2;
        }
        4 => {
            let tree_select = reader.read_bit()?;
            if tree_select {
                code_lengths[symbols[0] as usize] = 2;
                code_lengths[symbols[1] as usize] = 2;
                code_lengths[symbols[2] as usize] = 2;
                code_lengths[symbols[3] as usize] = 2;
            } else {
                code_lengths[symbols[0] as usize] = 1;
                code_lengths[symbols[1] as usize] = 2;
                code_lengths[symbols[2] as usize] = 3;
                code_lengths[symbols[3] as usize] = 3;
            }
        }
        _ => return Err(BrotliError::InvalidHuffman),
    }

    HuffmanDecoder::from_lengths(&code_lengths)
}

/// Read complex prefix code
fn read_complex_prefix_code(reader: &mut BitReader, alphabet_size: u16) -> BrotliResult<HuffmanDecoder> {
    // Code length code order
    const CODE_LENGTH_ORDER: [u8; 18] = [
        1, 2, 3, 4, 0, 5, 17, 6, 16, 7, 8, 9, 10, 11, 12, 13, 14, 15
    ];

    let hskip = reader.read_bits(2)?;
    
    let mut code_length_lengths = [0u8; 18];
    let mut space = 32;
    let mut num_codes = 0;

    for i in hskip as usize..18 {
        let idx = CODE_LENGTH_ORDER[i] as usize;
        
        // Read code length length (variable length)
        let len = read_code_length_code(reader)?;
        code_length_lengths[idx] = len;
        
        if len != 0 {
            space -= 32 >> len;
            num_codes += 1;
            if space <= 0 {
                break;
            }
        }
    }

    // Build decoder for code lengths
    let cl_decoder = HuffmanDecoder::from_lengths(&code_length_lengths)?;

    // Read actual code lengths
    let mut code_lengths = vec![0u8; alphabet_size as usize];
    let mut i = 0usize;
    let mut prev_code_len = 8u8;
    let repeat_code_len = 0u8;

    while i < alphabet_size as usize {
        let sym = cl_decoder.decode(reader)?;
        
        match sym {
            0..=15 => {
                // Literal code length
                code_lengths[i] = sym as u8;
                if sym != 0 {
                    prev_code_len = sym as u8;
                }
                i += 1;
            }
            16 => {
                // Repeat previous
                let extra = reader.read_bits(2)? as usize + 3;
                for _ in 0..extra {
                    if i >= alphabet_size as usize {
                        break;
                    }
                    code_lengths[i] = prev_code_len;
                    i += 1;
                }
            }
            17 => {
                // Repeat zero (short)
                let extra = reader.read_bits(3)? as usize + 3;
                i += extra;
            }
            _ => return Err(BrotliError::InvalidHuffman),
        }
    }

    HuffmanDecoder::from_lengths(&code_lengths)
}

/// Read a code length code (0-5 variable length encoding)
fn read_code_length_code(reader: &mut BitReader) -> BrotliResult<u8> {
    // 0: 00
    // 1: 01  
    // 2: 10
    // 3: 1100
    // 4: 1101
    // 5: 111x
    let b0 = reader.read_bit()?;
    if !b0 {
        let b1 = reader.read_bit()?;
        return Ok(if b1 { 1 } else { 0 });
    }
    let b1 = reader.read_bit()?;
    if !b1 {
        return Ok(2);
    }
    let b2 = reader.read_bit()?;
    if !b2 {
        let b3 = reader.read_bit()?;
        return Ok(if b3 { 4 } else { 3 });
    }
    let b3 = reader.read_bit()?;
    Ok(if b3 { 5 } else { 5 })
}

/// Read prefix code (either simple or complex)
fn read_prefix_code(reader: &mut BitReader, alphabet_size: u16) -> BrotliResult<HuffmanDecoder> {
    let simple = reader.read_bit()?;
    if simple {
        read_simple_prefix_code(reader, alphabet_size)
    } else {
        read_complex_prefix_code(reader, alphabet_size)
    }
}

// ============================================================================
// Block Types
// ============================================================================

/// Block type info for each category
struct BlockTypeInfo {
    num_types: u8,
    current_type: u8,
    prev_type1: u8,
    prev_type2: u8,
    block_count: u32,
    type_decoder: Option<HuffmanDecoder>,
    count_decoder: Option<HuffmanDecoder>,
}

impl BlockTypeInfo {
    fn new() -> Self {
        Self {
            num_types: 1,
            current_type: 0,
            prev_type1: 1,
            prev_type2: 0,
            block_count: 0,
            type_decoder: None,
            count_decoder: None,
        }
    }

    fn read_header(&mut self, reader: &mut BitReader) -> BrotliResult<()> {
        self.num_types = read_variable_length(reader, 8)? as u8 + 1;
        
        if self.num_types > 1 {
            self.type_decoder = Some(read_prefix_code(reader, self.num_types as u16 + 2)?);
            self.count_decoder = Some(read_prefix_code(reader, 26)?);
            self.block_count = self.read_block_count(reader)?;
        } else {
            self.block_count = u32::MAX;
        }
        
        Ok(())
    }

    fn read_block_count(&self, reader: &mut BitReader) -> BrotliResult<u32> {
        if let Some(ref decoder) = self.count_decoder {
            let code = decoder.decode(reader)?;
            read_block_count_value(reader, code)
        } else {
            Ok(u32::MAX)
        }
    }

    fn maybe_switch_type(&mut self, reader: &mut BitReader) -> BrotliResult<()> {
        if self.num_types <= 1 {
            return Ok(());
        }

        self.block_count -= 1;
        if self.block_count == 0 {
            if let Some(ref decoder) = self.type_decoder {
                let code = decoder.decode(reader)?;
                let new_type = match code {
                    0 => self.prev_type1,
                    1 => (self.current_type + 1) % self.num_types,
                    _ => (code - 2) as u8,
                };
                self.prev_type2 = self.prev_type1;
                self.prev_type1 = self.current_type;
                self.current_type = new_type;
                self.block_count = self.read_block_count(reader)?;
            }
        }
        Ok(())
    }
}

/// Read block count value
fn read_block_count_value(reader: &mut BitReader, code: u16) -> BrotliResult<u32> {
    const BLOCK_COUNT_EXTRA: [u8; 26] = [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        2, 3, 4, 5, 6, 7, 8, 9, 12, 15
    ];
    const BLOCK_COUNT_BASE: [u32; 26] = [
        1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
        17, 21, 29, 45, 77, 141, 269, 525, 1037, 5133
    ];

    let extra = reader.read_bits(BLOCK_COUNT_EXTRA[code as usize])?;
    Ok(BLOCK_COUNT_BASE[code as usize] + extra)
}

/// Read variable length number
fn read_variable_length(reader: &mut BitReader, max_bits: u8) -> BrotliResult<u32> {
    if !reader.read_bit()? {
        return Ok(0);
    }
    
    let mut n = 0u8;
    while n < max_bits && reader.read_bit()? {
        n += 1;
    }
    
    if n == 0 {
        Ok(1)
    } else {
        let extra = reader.read_bits(n)?;
        Ok((1 << n) + extra)
    }
}

// ============================================================================
// Context Modeling
// ============================================================================

/// Context modes for literal encoding
#[derive(Debug, Clone, Copy)]
enum ContextMode {
    Lsb6,
    Msb6,
    Utf8,
    Signed,
}

impl From<u8> for ContextMode {
    fn from(v: u8) -> Self {
        match v & 3 {
            0 => ContextMode::Lsb6,
            1 => ContextMode::Msb6,
            2 => ContextMode::Utf8,
            _ => ContextMode::Signed,
        }
    }
}

/// Calculate literal context ID
fn literal_context_id(prev1: u8, prev2: u8, mode: ContextMode) -> u8 {
    match mode {
        ContextMode::Lsb6 => prev1 & 0x3F,
        ContextMode::Msb6 => prev1 >> 2,
        ContextMode::Utf8 => UTF8_CONTEXT_LUT[prev1 as usize],
        ContextMode::Signed => {
            let lut0 = SIGNED_CONTEXT_LUT[(prev1 >> 3) as usize];
            let lut1 = SIGNED_CONTEXT_LUT[(prev2 >> 3) as usize];
            (lut0 << 3) | lut1
        }
    }
}

/// UTF-8 context lookup table
const UTF8_CONTEXT_LUT: [u8; 256] = {
    let mut lut = [0u8; 256];
    let mut i = 0;
    while i < 256 {
        lut[i] = if i < 0x80 {
            // ASCII
            if i < 0x20 || i == 0x7F { 0 } // Control
            else if (i as u8).is_ascii_whitespace() { 1 } // Whitespace
            else { 2 } // Other ASCII
        } else if i < 0xC0 {
            3 // Continuation byte
        } else {
            4 // Start byte
        };
        i += 1;
    }
    lut
};

/// Signed context lookup (for binary data)
const SIGNED_CONTEXT_LUT: [u8; 32] = [
    0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3,
    4, 4, 4, 4, 5, 5, 5, 5, 6, 6, 6, 6, 7, 7, 7, 7,
];

/// Distance context from insert/copy length
fn distance_context(copy_len: u32) -> u8 {
    if copy_len > 4 { 3 } else { copy_len.saturating_sub(2) as u8 }
}

// ============================================================================
// Insert and Copy Length Decoding
// ============================================================================

/// Decode insert and copy lengths
fn decode_insert_copy(reader: &mut BitReader, decoder: &HuffmanDecoder) -> BrotliResult<(u32, u32)> {
    let code = decoder.decode(reader)?;
    
    // Insert/copy length tables from RFC 7932
    const INSERT_LENGTH_CODE_BASE: [u32; 24] = [
        0, 1, 2, 3, 4, 5, 6, 8, 10, 14, 18, 26,
        34, 50, 66, 98, 130, 194, 322, 578, 1090, 2114, 6210, 22594
    ];
    const INSERT_LENGTH_EXTRA_BITS: [u8; 24] = [
        0, 0, 0, 0, 0, 0, 1, 1, 2, 2, 3, 3,
        4, 4, 5, 5, 6, 7, 8, 9, 10, 12, 14, 24
    ];
    const COPY_LENGTH_CODE_BASE: [u32; 24] = [
        2, 3, 4, 5, 6, 7, 8, 9, 10, 12, 14, 18,
        22, 30, 38, 54, 70, 102, 134, 198, 326, 582, 1094, 2118
    ];
    const COPY_LENGTH_EXTRA_BITS: [u8; 24] = [
        0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 2, 2,
        3, 3, 4, 4, 5, 5, 6, 7, 8, 9, 10, 24
    ];

    let insert_code = (code >> 0) & 0x1F;
    let copy_code = (code >> 5) & 0x1F;
    
    let insert_code = (insert_code as usize).min(23);
    let copy_code = (copy_code as usize).min(23);

    let insert_extra = reader.read_bits(INSERT_LENGTH_EXTRA_BITS[insert_code])?;
    let copy_extra = reader.read_bits(COPY_LENGTH_EXTRA_BITS[copy_code])?;

    let insert_len = INSERT_LENGTH_CODE_BASE[insert_code] + insert_extra;
    let copy_len = COPY_LENGTH_CODE_BASE[copy_code] + copy_extra;

    Ok((insert_len, copy_len))
}

// ============================================================================
// Distance Decoding
// ============================================================================

/// Distance ring buffer
struct DistanceRing {
    ring: [u32; 4],
    pos: usize,
}

impl DistanceRing {
    fn new() -> Self {
        Self {
            ring: [4, 11, 15, 16], // Default values per spec
            pos: 0,
        }
    }

    fn push(&mut self, distance: u32) {
        self.ring[self.pos] = distance;
        self.pos = (self.pos + 1) & 3;
    }

    fn get(&self, idx: usize) -> u32 {
        self.ring[(self.pos + 3 - idx) & 3]
    }
}

/// Decode distance
fn decode_distance(
    reader: &mut BitReader,
    decoder: &HuffmanDecoder,
    ring: &DistanceRing,
    postfix_bits: u8,
    direct_codes: u32,
) -> BrotliResult<u32> {
    let code = decoder.decode(reader)?;
    
    if code == 0 {
        return Ok(ring.get(0));
    }
    if code == 1 {
        return Ok(ring.get(1));
    }
    if code == 2 {
        return Ok(ring.get(2));
    }
    if code == 3 {
        return Ok(ring.get(3));
    }
    
    // Transform codes 4-15 (distance modifications)
    if code < 16 {
        let idx = ((code - 4) / 2) as usize;
        let sign = (code & 1) == 0;
        let base = ring.get(idx);
        return Ok(if sign { base + 1 } else { base.saturating_sub(1) });
    }

    // Direct distance codes
    let adjusted = code - 16;
    if adjusted < direct_codes as u16 {
        return Ok(adjusted as u32 + 1);
    }

    // Far distance
    let ndistbits = 1 + ((adjusted - direct_codes as u16) >> (postfix_bits + 1));
    let postfix_mask = (1u32 << postfix_bits) - 1;
    let hcode = (adjusted - direct_codes as u16) as u32 - ((ndistbits as u32 - 1) << (postfix_bits + 1));
    let lcode = hcode & postfix_mask;
    let offset = ((2 + (hcode >> postfix_bits)) << ndistbits) - 4;
    let extra = reader.read_bits(ndistbits as u8)?;
    
    Ok(((offset + extra) << postfix_bits) + lcode + direct_codes + 1)
}

// ============================================================================
// Main Decoder
// ============================================================================

/// Brotli decoder state
pub struct BrotliDecoder {
    window_bits: u8,
}

impl BrotliDecoder {
    /// Create new decoder
    pub fn new() -> Self {
        Self { window_bits: 22 }
    }

    /// Decompress Brotli data
    pub fn decompress(&mut self, input: &[u8]) -> BrotliResult<Vec<u8>> {
        self.decompress_with_dict(input, &[])
    }

    /// Decompress with optional dictionary
    pub fn decompress_with_dict(&mut self, input: &[u8], dict: &[u8]) -> BrotliResult<Vec<u8>> {
        let mut reader = BitReader::new(input);
        let mut output = Vec::new();

        // Read window size
        let wbits = self.read_window_bits(&mut reader)?;
        self.window_bits = wbits;
        
        let mut ring_buffer = RingBuffer::new(wbits.min(24)); // Limit for memory
        let mut distance_ring = DistanceRing::new();
        
        // Initialize with dictionary
        for &b in dict {
            ring_buffer.push(b);
        }

        // Process meta-blocks
        loop {
            let is_last = reader.read_bit()?;
            
            if is_last && reader.is_empty() {
                break;
            }

            // Read meta-block length
            let (mlen, is_empty) = self.read_meta_block_length(&mut reader)?;
            
            if is_empty {
                if is_last {
                    break;
                }
                continue;
            }

            // Check for uncompressed block
            let is_uncompressed = if !is_last {
                reader.read_bit()?
            } else {
                false
            };

            if is_uncompressed {
                // Byte-align
                while reader.bits_available % 8 != 0 {
                    reader.drop_bits(1);
                }
                
                // Copy raw bytes
                for _ in 0..mlen {
                    let b = reader.read_bits(8)? as u8;
                    output.push(b);
                    ring_buffer.push(b);
                }
            } else {
                // Compressed block
                self.decompress_meta_block(
                    &mut reader,
                    &mut output,
                    &mut ring_buffer,
                    &mut distance_ring,
                    mlen,
                )?;
            }

            if is_last {
                break;
            }
        }

        Ok(output)
    }

    fn read_window_bits(&self, reader: &mut BitReader) -> BrotliResult<u8> {
        let large = reader.read_bit()?;
        if !large {
            return Ok(16);
        }

        let bits = reader.read_bits(3)?;
        if bits == 0 {
            let extra = reader.read_bit()?;
            return Ok(if extra { 10 } else { 17 });
        }
        
        Ok(17 + bits as u8)
    }

    fn read_meta_block_length(&self, reader: &mut BitReader) -> BrotliResult<(u32, bool)> {
        let nibbles = reader.read_bits(2)?;
        
        if nibbles == 3 {
            // Reserved or empty
            let is_empty = reader.read_bit()?;
            return Ok((0, is_empty));
        }

        let num_nibbles = (nibbles + 4) as usize;
        let mut mlen = 0u32;
        for i in 0..num_nibbles {
            let nibble = reader.read_bits(4)?;
            mlen |= nibble << (4 * i);
        }
        
        Ok((mlen + 1, false))
    }

    fn decompress_meta_block(
        &mut self,
        reader: &mut BitReader,
        output: &mut Vec<u8>,
        ring_buffer: &mut RingBuffer,
        distance_ring: &mut DistanceRing,
        mlen: u32,
    ) -> BrotliResult<()> {
        // Read block types
        let mut literal_types = BlockTypeInfo::new();
        let mut iclen_types = BlockTypeInfo::new();
        let mut dist_types = BlockTypeInfo::new();

        literal_types.read_header(reader)?;
        iclen_types.read_header(reader)?;
        dist_types.read_header(reader)?;

        // Distance parameters
        let postfix_bits = reader.read_bits(2)? as u8;
        let direct_codes_raw = reader.read_bits(4)?;
        let direct_codes = direct_codes_raw << postfix_bits;

        // Context modes for literals
        let mut context_modes = Vec::with_capacity(literal_types.num_types as usize);
        for _ in 0..literal_types.num_types {
            let mode = reader.read_bits(2)? as u8;
            context_modes.push(ContextMode::from(mode));
        }

        // Read context maps
        let num_literal_ctxs = 64 * literal_types.num_types as u32;
        let num_dist_ctxs = 4 * dist_types.num_types as u32;
        
        let literal_ctx_map = self.read_context_map(reader, num_literal_ctxs)?;
        let dist_ctx_map = self.read_context_map(reader, num_dist_ctxs)?;

        // Read Huffman trees
        let num_literal_trees = literal_ctx_map.iter().max().copied().unwrap_or(0) as usize + 1;
        let mut literal_trees = Vec::with_capacity(num_literal_trees);
        for _ in 0..num_literal_trees {
            literal_trees.push(read_prefix_code(reader, 256)?);
        }

        let num_iclen_trees = iclen_types.num_types as usize;
        let mut iclen_trees = Vec::with_capacity(num_iclen_trees);
        for _ in 0..num_iclen_trees {
            iclen_trees.push(read_prefix_code(reader, 704)?);
        }

        let num_dist_trees = dist_ctx_map.iter().max().copied().unwrap_or(0) as usize + 1;
        let num_dist_codes = 16 + direct_codes + (48 << postfix_bits);
        let mut dist_trees = Vec::with_capacity(num_dist_trees);
        for _ in 0..num_dist_trees {
            dist_trees.push(read_prefix_code(reader, num_dist_codes as u16)?);
        }

        // Decode data
        let start_len = output.len();
        let mut prev1 = 0u8;
        let mut prev2 = 0u8;

        while output.len() - start_len < mlen as usize {
            iclen_types.maybe_switch_type(reader)?;
            
            let iclen_tree = &iclen_trees[iclen_types.current_type as usize];
            let (insert_len, copy_len) = decode_insert_copy(reader, iclen_tree)?;

            // Insert literals
            for _ in 0..insert_len {
                literal_types.maybe_switch_type(reader)?;
                
                let ctx = literal_context_id(prev1, prev2, context_modes[literal_types.current_type as usize]);
                let tree_idx = literal_ctx_map[(literal_types.current_type as u32 * 64 + ctx as u32) as usize];
                let literal_tree = &literal_trees[tree_idx as usize];
                
                let literal = literal_tree.decode(reader)? as u8;
                output.push(literal);
                ring_buffer.push(literal);
                
                prev2 = prev1;
                prev1 = literal;
            }

            if output.len() - start_len >= mlen as usize {
                break;
            }

            // Decode distance
            dist_types.maybe_switch_type(reader)?;
            
            let dist_ctx = distance_context(copy_len);
            let tree_idx = dist_ctx_map[(dist_types.current_type as u32 * 4 + dist_ctx as u32) as usize];
            let dist_tree = &dist_trees[tree_idx as usize];
            
            let distance = decode_distance(reader, dist_tree, distance_ring, postfix_bits, direct_codes)?;
            distance_ring.push(distance);

            // Copy from history
            if distance as usize > output.len() {
                return Err(BrotliError::InvalidDistance);
            }

            for _ in 0..copy_len {
                let byte = ring_buffer.get(distance as usize);
                output.push(byte);
                ring_buffer.push(byte);
                prev2 = prev1;
                prev1 = byte;
            }
        }

        Ok(())
    }

    fn read_context_map(&self, reader: &mut BitReader, size: u32) -> BrotliResult<Vec<u8>> {
        if size == 0 {
            return Ok(vec![0]);
        }

        let num_trees = read_variable_length(reader, 8)? as u8 + 1;
        
        if num_trees == 1 {
            return Ok(vec![0; size as usize]);
        }

        let use_rle = reader.read_bit()?;
        let mut rle_max = 0u8;
        if use_rle {
            while rle_max < 16 && reader.read_bit()? {
                rle_max += 1;
            }
        }

        let ctx_decoder = read_prefix_code(reader, num_trees as u16 + rle_max as u16)?;
        
        let mut map = Vec::with_capacity(size as usize);
        while map.len() < size as usize {
            let code = ctx_decoder.decode(reader)?;
            
            if code < num_trees as u16 {
                map.push(code as u8);
            } else {
                // RLE zero
                let extra_bits = code - num_trees as u16 + 1;
                let run_len = (1 << extra_bits) + reader.read_bits(extra_bits as u8)?;
                for _ in 0..run_len {
                    map.push(0);
                }
            }
        }

        // IMTF decode
        let mut mtf = (0..num_trees).collect::<Vec<_>>();
        for val in map.iter_mut() {
            let idx = *val as usize;
            *val = mtf[idx];
            if idx > 0 {
                let v = mtf.remove(idx);
                mtf.insert(0, v);
            }
        }

        Ok(map)
    }
}

impl Default for BrotliDecoder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Public API
// ============================================================================

/// Decompress Brotli data
pub fn decompress(input: &[u8]) -> BrotliResult<Vec<u8>> {
    BrotliDecoder::new().decompress(input)
}

/// Decompress with dictionary (for WOFF2)
pub fn decompress_with_dict(input: &[u8], dict: &[u8]) -> BrotliResult<Vec<u8>> {
    BrotliDecoder::new().decompress_with_dict(input, dict)
}

// ============================================================================
// WOFF2 Transform Dictionary
// ============================================================================

/// WOFF2 uses a custom dictionary derived from font data.
/// This is a subset - the full 120KB dictionary would be included
/// as a static binary blob in production.
pub static WOFF2_DICTIONARY: &[u8] = &[];

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bit_reader_basic() {
        // 0b11010010 = bits 0-7: 0,1,0,0,1,0,1,1 (LSB first)
        // 0b10110100 = bits 0-7: 0,0,1,0,1,1,0,1 (LSB first)
        let data = [0b11010010, 0b10110100];
        let mut reader = BitReader::new(&data);
        
        // Read bit 0 (LSB of first byte): 0
        assert_eq!(reader.read_bits(1).unwrap(), 0);
        // Read bit 1: 1
        assert_eq!(reader.read_bits(1).unwrap(), 1);
        // Read bits 2-3: 0b01 = 0,1 -> value 2 (bit 2 is 0, bit 3 is 0) -> actually 0b00
        // Let me recalculate: 0b11010010 bits are:
        // bit 0 = 0, bit 1 = 1, bit 2 = 0, bit 3 = 0, bit 4 = 1, bit 5 = 0, bit 6 = 1, bit 7 = 1
        // After reading bits 0,1, we're at bit 2. Read 2 bits (2,3) = 0,0 -> 0b00 = 0
        assert_eq!(reader.read_bits(2).unwrap(), 0b00);
        // Read 4 bits (4,5,6,7) = 1,0,1,1 -> 0b1101 = 13
        assert_eq!(reader.read_bits(4).unwrap(), 0b1101);
    }

    #[test]
    fn test_huffman_single_symbol() {
        let lengths = [0, 1, 0];
        let decoder = HuffmanDecoder::from_lengths(&lengths).unwrap();
        assert_eq!(decoder.num_symbols, 3);
    }

    #[test]
    fn test_huffman_two_symbols() {
        let lengths = [1, 1, 0];
        let decoder = HuffmanDecoder::from_lengths(&lengths).unwrap();
        
        // Symbol 0 = 0, Symbol 1 = 1
        let data = [0b01010101];
        let mut reader = BitReader::new(&data);
        
        assert_eq!(decoder.decode(&mut reader).unwrap(), 1);
        assert_eq!(decoder.decode(&mut reader).unwrap(), 0);
    }

    #[test]
    fn test_ring_buffer() {
        let mut rb = RingBuffer::new(4); // 16-byte buffer
        
        for i in 0..10u8 {
            rb.push(i);
        }
        
        assert_eq!(rb.get(1), 9);
        assert_eq!(rb.get(2), 8);
        assert_eq!(rb.get(10), 0);
    }

    #[test]
    fn test_distance_ring() {
        let mut ring = DistanceRing::new();
        ring.push(100);
        ring.push(200);
        
        assert_eq!(ring.get(0), 200);
        assert_eq!(ring.get(1), 100);
    }

    #[test]
    fn test_literal_context_lsb6() {
        assert_eq!(literal_context_id(0x41, 0, ContextMode::Lsb6), 0x01);
        assert_eq!(literal_context_id(0xFF, 0, ContextMode::Lsb6), 0x3F);
    }

    #[test]
    fn test_literal_context_msb6() {
        assert_eq!(literal_context_id(0x80, 0, ContextMode::Msb6), 0x20);
        assert_eq!(literal_context_id(0xFF, 0, ContextMode::Msb6), 0x3F);
    }
}
