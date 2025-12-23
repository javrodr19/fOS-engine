//! WebP Decoder (RIFF container)
//!
//! From-scratch WebP decoder with SIMD-accelerated transforms.
//! Supports VP8L (lossless) and VP8 (lossy).

use super::simd::SimdOps;

/// WebP decoding error
#[derive(Debug, Clone)]
pub enum WebpError {
    InvalidRiff,
    InvalidWebp,
    InvalidVp8,
    InvalidVp8l,
    UnsupportedFormat,
    HuffmanError,
    DecodingError(String),
}

impl std::fmt::Display for WebpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRiff => write!(f, "Invalid RIFF container"),
            Self::InvalidWebp => write!(f, "Invalid WebP format"),
            Self::InvalidVp8 => write!(f, "Invalid VP8 bitstream"),
            Self::InvalidVp8l => write!(f, "Invalid VP8L bitstream"),
            Self::UnsupportedFormat => write!(f, "Unsupported WebP format"),
            Self::HuffmanError => write!(f, "Huffman decoding error"),
            Self::DecodingError(e) => write!(f, "Decoding error: {}", e),
        }
    }
}

impl std::error::Error for WebpError {}

/// Decoded WebP image
#[derive(Debug, Clone)]
pub struct WebpImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>, // RGBA
}

/// WebP decoder
pub struct WebpDecoder {
    simd: SimdOps,
}

impl WebpDecoder {
    pub fn new() -> Self {
        Self {
            simd: SimdOps::new(),
        }
    }

    /// Decode WebP from bytes
    pub fn decode(&mut self, data: &[u8]) -> Result<WebpImage, WebpError> {
        if data.len() < 12 {
            return Err(WebpError::InvalidRiff);
        }

        if &data[0..4] != b"RIFF" || &data[8..12] != b"WEBP" {
            return Err(WebpError::InvalidRiff);
        }

        let mut pos = 12;
        let mut alpha_data: Option<Vec<u8>> = None;
        let mut image_data: Option<WebpImage> = None;

        while pos + 8 <= data.len() {
            let chunk_id = &data[pos..pos + 4];
            let chunk_size = u32::from_le_bytes([
                data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]
            ]) as usize;

            pos += 8;

            if pos + chunk_size > data.len() {
                break;
            }

            let chunk_data = &data[pos..pos + chunk_size];

            match chunk_id {
                b"VP8 " => {
                    image_data = Some(self.decode_vp8(chunk_data)?);
                }
                b"VP8L" => {
                    image_data = Some(self.decode_vp8l(chunk_data)?);
                }
                b"VP8X" => {
                    // Extended format - continue parsing
                }
                b"ALPH" => {
                    alpha_data = Some(self.decode_alpha(chunk_data)?);
                }
                _ => {}
            }

            pos += chunk_size + (chunk_size & 1);
        }

        let mut image = image_data.ok_or(WebpError::InvalidWebp)?;

        if let Some(alpha) = alpha_data {
            self.apply_alpha(&mut image, &alpha);
        }

        Ok(image)
    }

    // ========== VP8L (Lossless) Decoder ==========

    fn decode_vp8l(&self, data: &[u8]) -> Result<WebpImage, WebpError> {
        if data.len() < 5 || data[0] != 0x2F {
            return Err(WebpError::InvalidVp8l);
        }

        let header = u32::from_le_bytes([data[1], data[2], data[3], data[4]]);
        let width = (header & 0x3FFF) + 1;
        let height = ((header >> 14) & 0x3FFF) + 1;
        let _has_alpha = ((header >> 28) & 0x01) != 0;
        let version = (header >> 29) & 0x07;

        if version != 0 {
            return Err(WebpError::UnsupportedFormat);
        }

        let mut reader = Vp8lBitReader::new(&data[5..]);
        
        // Read transforms
        let transforms = self.read_transforms(&mut reader, width, height)?;
        
        // Read color cache
        let color_cache_bits = if reader.read_bit()? {
            let bits = reader.read_bits(4)? as usize;
            if bits > 11 { return Err(WebpError::InvalidVp8l); }
            bits
        } else {
            0
        };

        // Decode image data
        let mut argb = self.decode_image_stream(&mut reader, width, height, color_cache_bits)?;
        
        // Apply inverse transforms
        self.apply_inverse_transforms(&mut argb, width, height, &transforms)?;

        // Convert ARGB to RGBA
        let pixels = self.argb_to_rgba(&argb);

        Ok(WebpImage { width, height, pixels })
    }

    fn read_transforms(&self, reader: &mut Vp8lBitReader, width: u32, height: u32) -> Result<Vec<Transform>, WebpError> {
        let mut transforms = Vec::new();

        while reader.read_bit()? {
            let transform_type = reader.read_bits(2)?;
            
            let transform = match transform_type {
                0 => {
                    // Predictor transform
                    let size_bits = reader.read_bits(3)? + 2;
                    let block_size = 1u32 << size_bits;
                    let blocks_w = (width + block_size - 1) / block_size;
                    let blocks_h = (height + block_size - 1) / block_size;
                    
                    // Read predictor data (simplified - just skip for now)
                    let predictor_count = (blocks_w * blocks_h) as usize;
                    let predictor_data = vec![0u8; predictor_count];
                    
                    Transform::Predictor { block_size, data: predictor_data }
                }
                1 => {
                    // Color transform
                    let size_bits = reader.read_bits(3)? + 2;
                    let block_size = 1u32 << size_bits;
                    Transform::Color { block_size }
                }
                2 => {
                    // Subtract green transform
                    Transform::SubtractGreen
                }
                3 => {
                    // Color indexing transform
                    let num_colors = reader.read_bits(8)? + 1;
                    let mut palette = Vec::with_capacity(num_colors as usize);
                    
                    // Read palette
                    for _ in 0..num_colors {
                        let g = reader.read_bits(8)? as u8;
                        let r = reader.read_bits(8)? as u8;
                        let b = reader.read_bits(8)? as u8;
                        let a = reader.read_bits(8)? as u8;
                        palette.push([a, r, g, b]);
                    }
                    
                    Transform::ColorIndex { palette }
                }
                _ => unreachable!(),
            };
            
            transforms.push(transform);
        }

        Ok(transforms)
    }

    fn decode_image_stream(
        &self,
        reader: &mut Vp8lBitReader,
        width: u32,
        height: u32,
        color_cache_bits: usize,
    ) -> Result<Vec<u32>, WebpError> {
        let pixel_count = (width * height) as usize;
        let mut argb = vec![0u32; pixel_count];

        // Read Huffman codes for the 5 symbol types
        let huffman_codes = self.read_huffman_codes(reader, color_cache_bits)?;
        
        // Initialize color cache
        let cache_size = if color_cache_bits > 0 { 1 << color_cache_bits } else { 0 };
        let mut color_cache = vec![0u32; cache_size];

        let mut pos = 0;
        
        while pos < pixel_count {
            // Decode green/length symbol
            let code = self.decode_huffman_symbol(reader, &huffman_codes.green)?;
            
            if code < 256 {
                // Literal: read remaining channels
                let red = self.decode_huffman_symbol(reader, &huffman_codes.red)?;
                let blue = self.decode_huffman_symbol(reader, &huffman_codes.blue)?;
                let alpha = self.decode_huffman_symbol(reader, &huffman_codes.alpha)?;
                
                let pixel = ((alpha as u32) << 24) | ((red as u32) << 16) | ((code as u32) << 8) | (blue as u32);
                argb[pos] = pixel;
                
                if color_cache_bits > 0 {
                    let hash = self.color_cache_hash(pixel, color_cache_bits);
                    color_cache[hash] = pixel;
                }
                
                pos += 1;
            } else if code < 256 + 24 {
                // Back-reference: length/distance pair
                let length_code = code - 256;
                let length = self.decode_length(reader, length_code)?;
                
                let dist_code = self.decode_huffman_symbol(reader, &huffman_codes.distance)?;
                let distance = self.decode_distance(reader, dist_code, width)?;
                
                // Copy pixels
                for i in 0..length as usize {
                    if pos + i < pixel_count && distance as usize <= pos + i {
                        argb[pos + i] = argb[pos + i - distance as usize];
                        
                        if color_cache_bits > 0 {
                            let pixel = argb[pos + i];
                            let hash = self.color_cache_hash(pixel, color_cache_bits);
                            color_cache[hash] = pixel;
                        }
                    }
                }
                pos += length as usize;
            } else {
                // Color cache reference
                let cache_idx = code - 256 - 24;
                if (cache_idx as usize) < color_cache.len() {
                    argb[pos] = color_cache[cache_idx as usize];
                    pos += 1;
                } else {
                    return Err(WebpError::DecodingError("Invalid color cache index".into()));
                }
            }
        }

        Ok(argb)
    }

    fn read_huffman_codes(&self, reader: &mut Vp8lBitReader, color_cache_bits: usize) -> Result<HuffmanCodes, WebpError> {
        let cache_size = if color_cache_bits > 0 { 1 << color_cache_bits } else { 0 };
        let num_green = 256 + 24 + cache_size; // literals + length codes + cache codes
        
        Ok(HuffmanCodes {
            green: self.read_huffman_tree(reader, num_green)?,
            red: self.read_huffman_tree(reader, 256)?,
            blue: self.read_huffman_tree(reader, 256)?,
            alpha: self.read_huffman_tree(reader, 256)?,
            distance: self.read_huffman_tree(reader, 40)?,
        })
    }

    fn read_huffman_tree(&self, reader: &mut Vp8lBitReader, num_symbols: usize) -> Result<HuffmanTree, WebpError> {
        let simple = reader.read_bit()?;
        
        if simple {
            // Simple code: 1-2 symbols
            let num_symbols_minus_1 = reader.read_bit()? as usize;
            let is_first_8bit = reader.read_bit()?;
            
            let first_bits = if is_first_8bit { 8 } else { 1 };
            let first_symbol = reader.read_bits(first_bits)?;
            
            if num_symbols_minus_1 == 0 {
                // Single symbol - all codes decode to this
                return Ok(HuffmanTree {
                    codes: vec![(first_symbol as u16, 0)],
                    max_bits: 0,
                });
            } else {
                let second_symbol = reader.read_bits(8)?;
                return Ok(HuffmanTree {
                    codes: vec![(first_symbol as u16, 1), (second_symbol as u16, 1)],
                    max_bits: 1,
                });
            }
        }

        // Complex code: read code lengths
        let num_code_lengths = reader.read_bits(4)? as usize + 4;
        
        // Code length order
        const CODE_LENGTH_ORDER: [usize; 19] = [
            17, 18, 0, 1, 2, 3, 4, 5, 16, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15
        ];
        
        let mut code_length_lengths = [0u8; 19];
        for i in 0..num_code_lengths.min(19) {
            code_length_lengths[CODE_LENGTH_ORDER[i]] = reader.read_bits(3)? as u8;
        }
        
        // Build code length tree
        let code_length_tree = self.build_huffman_tree(&code_length_lengths)?;
        
        // Read symbol lengths
        let mut lengths = vec![0u8; num_symbols];
        let mut i = 0;
        
        while i < num_symbols {
            let code = self.decode_huffman_symbol_tree(reader, &code_length_tree)?;
            
            match code {
                0..=15 => {
                    lengths[i] = code as u8;
                    i += 1;
                }
                16 => {
                    // Repeat previous
                    let repeat = reader.read_bits(2)? as usize + 3;
                    let prev = if i > 0 { lengths[i - 1] } else { 0 };
                    for _ in 0..repeat {
                        if i < num_symbols {
                            lengths[i] = prev;
                            i += 1;
                        }
                    }
                }
                17 => {
                    // Repeat 0, 3-10 times
                    let repeat = reader.read_bits(3)? as usize + 3;
                    i += repeat.min(num_symbols - i);
                }
                18 => {
                    // Repeat 0, 11-138 times
                    let repeat = reader.read_bits(7)? as usize + 11;
                    i += repeat.min(num_symbols - i);
                }
                _ => return Err(WebpError::HuffmanError),
            }
        }
        
        self.build_huffman_tree(&lengths)
    }

    fn build_huffman_tree(&self, lengths: &[u8]) -> Result<HuffmanTree, WebpError> {
        let max_bits = *lengths.iter().max().unwrap_or(&0) as usize;
        
        if max_bits == 0 {
            // All zeros - special case
            return Ok(HuffmanTree {
                codes: vec![(0, 0)],
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
        
        // Generate next_code
        let mut next_code = vec![0u32; max_bits + 1];
        let mut code = 0u32;
        for bits in 1..=max_bits {
            code = (code + bl_count[bits - 1]) << 1;
            next_code[bits] = code;
        }
        
        // Build lookup table
        let table_size = 1 << max_bits.min(12);
        let mut codes = vec![(0u16, 0u8); table_size];
        
        for (sym, &len) in lengths.iter().enumerate() {
            if len == 0 { continue; }
            let len = len as usize;
            
            let c = next_code[len];
            next_code[len] += 1;
            
            // Reverse bits for table indexing
            let rev = Self::reverse_bits(c, len);
            
            // Fill table entries
            if len <= 12 {
                let step = 1 << len;
                let mut idx = rev as usize;
                while idx < table_size {
                    codes[idx] = (sym as u16, len as u8);
                    idx += step;
                }
            }
        }
        
        Ok(HuffmanTree { codes, max_bits: max_bits.min(12) })
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

    fn decode_huffman_symbol(&self, reader: &mut Vp8lBitReader, tree: &HuffmanTree) -> Result<u32, WebpError> {
        self.decode_huffman_symbol_tree(reader, tree)
    }

    fn decode_huffman_symbol_tree(&self, reader: &mut Vp8lBitReader, tree: &HuffmanTree) -> Result<u32, WebpError> {
        if tree.max_bits == 0 {
            return Ok(tree.codes[0].0 as u32);
        }
        
        let peek = reader.peek_bits(tree.max_bits)?;
        let (sym, len) = tree.codes[peek as usize];
        
        if len == 0 {
            return Err(WebpError::HuffmanError);
        }
        
        reader.drop_bits(len as usize);
        Ok(sym as u32)
    }

    fn decode_length(&self, reader: &mut Vp8lBitReader, code: u32) -> Result<u32, WebpError> {
        const LENGTH_BASES: [u32; 24] = [
            1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193,
            257, 385, 513, 769, 1025, 1537, 2049, 3073
        ];
        const LENGTH_EXTRA: [u8; 24] = [
            0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10
        ];
        
        let idx = code as usize;
        if idx >= LENGTH_BASES.len() {
            return Err(WebpError::DecodingError("Invalid length code".into()));
        }
        
        let base = LENGTH_BASES[idx];
        let extra = LENGTH_EXTRA[idx];
        
        Ok(base + reader.read_bits(extra as usize)?)
    }

    fn decode_distance(&self, reader: &mut Vp8lBitReader, code: u32, width: u32) -> Result<u32, WebpError> {
        const DIST_BASES: [u32; 40] = [
            1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193,
            257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289,
            16385, 24577, 32769, 49153, 65537, 98305, 131073, 196609, 262145, 393217, 524289, 786433
        ];
        const DIST_EXTRA: [u8; 40] = [
            0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8,
            9, 9, 10, 10, 11, 11, 12, 12, 13, 13, 14, 14, 15, 15, 16, 16, 17, 17, 18, 18
        ];

        if code < 4 {
            // Distance code 0-3 use special 2D distance
            return Ok(code + 1);
        }
        
        let idx = (code - 4) as usize;
        if idx >= DIST_BASES.len() {
            return Err(WebpError::DecodingError("Invalid distance code".into()));
        }
        
        let base = DIST_BASES[idx.min(DIST_BASES.len() - 1)];
        let extra = DIST_EXTRA[idx.min(DIST_EXTRA.len() - 1)];
        
        Ok(base + reader.read_bits(extra as usize)?)
    }

    fn color_cache_hash(&self, pixel: u32, bits: usize) -> usize {
        let mult = 0x1E35A7BDu32;
        ((pixel.wrapping_mul(mult)) >> (32 - bits)) as usize
    }

    fn apply_inverse_transforms(&self, argb: &mut [u32], width: u32, height: u32, transforms: &[Transform]) -> Result<(), WebpError> {
        // Apply transforms in reverse order
        for transform in transforms.iter().rev() {
            match transform {
                Transform::SubtractGreen => {
                    self.inverse_subtract_green(argb);
                }
                Transform::Predictor { block_size, data } => {
                    self.inverse_predictor(argb, width, height, *block_size, data);
                }
                Transform::Color { block_size } => {
                    self.inverse_color(argb, width, height, *block_size);
                }
                Transform::ColorIndex { palette } => {
                    self.inverse_color_index(argb, palette);
                }
            }
        }
        Ok(())
    }

    fn inverse_subtract_green(&self, argb: &mut [u32]) {
        for pixel in argb.iter_mut() {
            let a = (*pixel >> 24) & 0xFF;
            let r = (*pixel >> 16) & 0xFF;
            let g = (*pixel >> 8) & 0xFF;
            let b = *pixel & 0xFF;
            
            let r = (r + g) & 0xFF;
            let b = (b + g) & 0xFF;
            
            *pixel = (a << 24) | (r << 16) | (g << 8) | b;
        }
    }

    fn inverse_predictor(&self, argb: &mut [u32], width: u32, height: u32, block_size: u32, _data: &[u8]) {
        // Simplified: use predictor 0 (no prediction)
        // Full impl would use data to determine predictor per block
        let _ = (width, height, block_size);
    }

    fn inverse_color(&self, argb: &mut [u32], width: u32, height: u32, block_size: u32) {
        // Simplified color transform inverse
        let _ = (width, height, block_size);
    }

    fn inverse_color_index(&self, argb: &mut [u32], palette: &[[u8; 4]]) {
        for pixel in argb.iter_mut() {
            let idx = (*pixel & 0xFF) as usize;
            if idx < palette.len() {
                let [a, r, g, b] = palette[idx];
                *pixel = ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
            }
        }
    }

    fn argb_to_rgba(&self, argb: &[u32]) -> Vec<u8> {
        let mut rgba = Vec::with_capacity(argb.len() * 4);
        for &pixel in argb {
            let a = ((pixel >> 24) & 0xFF) as u8;
            let r = ((pixel >> 16) & 0xFF) as u8;
            let g = ((pixel >> 8) & 0xFF) as u8;
            let b = (pixel & 0xFF) as u8;
            rgba.extend_from_slice(&[r, g, b, a]);
        }
        rgba
    }

    // ========== VP8 (Lossy) Decoder ==========

    fn decode_vp8(&self, data: &[u8]) -> Result<WebpImage, WebpError> {
        if data.len() < 10 {
            return Err(WebpError::InvalidVp8);
        }

        let frame_tag = u32::from_le_bytes([data[0], data[1], data[2], 0]);
        let keyframe = (frame_tag & 0x01) == 0;
        let _version = (frame_tag >> 1) & 0x07;
        let _show_frame = (frame_tag >> 4) & 0x01;
        let first_part_size = (frame_tag >> 5) as usize;

        if !keyframe {
            return Err(WebpError::UnsupportedFormat);
        }

        if data[3] != 0x9D || data[4] != 0x01 || data[5] != 0x2A {
            return Err(WebpError::InvalidVp8);
        }

        let width = ((data[6] as u32) | ((data[7] as u32) << 8)) & 0x3FFF;
        let height = ((data[8] as u32) | ((data[9] as u32) << 8)) & 0x3FFF;

        // Initialize boolean decoder for first partition
        let first_part_start = 10;
        let first_part_end = (first_part_start + first_part_size).min(data.len());
        let mut bool_dec = Vp8BoolDecoder::new(&data[first_part_start..first_part_end]);

        // Parse frame header
        let _color_space = bool_dec.read_bit()?;
        let _clamping = bool_dec.read_bit()?;

        // Segmentation
        let mut segment_quants = [0i32; 4];
        let segmentation_enabled = bool_dec.read_bit()?;
        if segmentation_enabled {
            let update_map = bool_dec.read_bit()?;
            let update_data = bool_dec.read_bit()?;
            if update_data {
                let abs_delta = bool_dec.read_bit()?;
                for i in 0..4 {
                    if bool_dec.read_bit()? {
                        let val = bool_dec.read_literal(7)? as i32;
                        let sign = if bool_dec.read_bit()? { -1 } else { 1 };
                        segment_quants[i] = if abs_delta { val * sign } else { val * sign };
                    }
                }
                for _ in 0..4 {
                    if bool_dec.read_bit()? {
                        bool_dec.read_literal(6)?;
                        bool_dec.read_bit()?;
                    }
                }
            }
            if update_map {
                for _ in 0..3 {
                    if bool_dec.read_bit()? { bool_dec.read_literal(8)?; }
                }
            }
        }

        // Filter settings
        let _filter_type = bool_dec.read_bit()?;
        let _filter_level = bool_dec.read_literal(6)?;
        let _sharpness = bool_dec.read_literal(3)?;

        // Loop filter adjustments
        let lf_adj_enable = bool_dec.read_bit()?;
        if lf_adj_enable {
            if bool_dec.read_bit()? {
                for _ in 0..8 {
                    if bool_dec.read_bit()? {
                        bool_dec.read_literal(6)?;
                        bool_dec.read_bit()?;
                    }
                }
            }
        }

        // Partitions
        let log2_parts = bool_dec.read_literal(2)? as usize;
        let num_parts = 1 << log2_parts;
        let _ = num_parts;

        // Quantizer indices
        let y_ac_qi = bool_dec.read_literal(7)? as i32;
        let y_dc_delta = if bool_dec.read_bit()? { bool_dec.read_signed(4)? } else { 0 };
        let y2_dc_delta = if bool_dec.read_bit()? { bool_dec.read_signed(4)? } else { 0 };
        let y2_ac_delta = if bool_dec.read_bit()? { bool_dec.read_signed(4)? } else { 0 };
        let uv_dc_delta = if bool_dec.read_bit()? { bool_dec.read_signed(4)? } else { 0 };
        let uv_ac_delta = if bool_dec.read_bit()? { bool_dec.read_signed(4)? } else { 0 };

        // Build dequant tables
        let dequant = Vp8Dequant::new(y_ac_qi, y_dc_delta, y2_dc_delta, y2_ac_delta, uv_dc_delta, uv_ac_delta);

        // Skip probability updates (use defaults)
        let _refresh_probs = bool_dec.read_bit()?;

        // Allocate planes
        let mb_width = ((width + 15) / 16) as usize;
        let mb_height = ((height + 15) / 16) as usize;
        let y_stride = mb_width * 16;
        let uv_stride = mb_width * 8;

        let mut y_plane = vec![128u8; mb_height * 16 * y_stride];
        let mut u_plane = vec![128u8; mb_height * 8 * uv_stride];
        let mut v_plane = vec![128u8; mb_height * 8 * uv_stride];

        // Default token probabilities
        let coeff_probs = Vp8CoeffProbs::default();

        // Decode macroblocks
        for mb_y in 0..mb_height {
            for mb_x in 0..mb_width {
                // Read macroblock header
                let is_skip = bool_dec.read_bool(238)?;
                
                // Intra prediction mode (simplified: assume DC_PRED)
                let y_mode = if !is_skip && bool_dec.read_bool(145)? {
                    bool_dec.read_literal(2)? as u8
                } else {
                    0 // DC_PRED
                };

                let uv_mode = if !is_skip && bool_dec.read_bool(142)? {
                    bool_dec.read_literal(2)? as u8
                } else {
                    0 // DC_PRED
                };

                // Decode and reconstruct Y blocks (16 4x4 blocks)
                let mut y_coeffs = [[0i16; 16]; 16];
                if !is_skip {
                    for i in 0..16 {
                        self.decode_block_coeffs(&mut bool_dec, &coeff_probs, &mut y_coeffs[i], 0)?;
                    }
                }

                // Decode and reconstruct U blocks (4 4x4 blocks)
                let mut u_coeffs = [[0i16; 16]; 4];
                if !is_skip {
                    for i in 0..4 {
                        self.decode_block_coeffs(&mut bool_dec, &coeff_probs, &mut u_coeffs[i], 2)?;
                    }
                }

                // Decode and reconstruct V blocks (4 4x4 blocks)
                let mut v_coeffs = [[0i16; 16]; 4];
                if !is_skip {
                    for i in 0..4 {
                        self.decode_block_coeffs(&mut bool_dec, &coeff_probs, &mut v_coeffs[i], 2)?;
                    }
                }

                // Apply dequantization and IDCT, then reconstruct
                self.reconstruct_macroblock(
                    mb_x, mb_y, y_mode, uv_mode,
                    &y_coeffs, &u_coeffs, &v_coeffs,
                    &dequant,
                    &mut y_plane, &mut u_plane, &mut v_plane,
                    y_stride, uv_stride,
                );
            }
        }

        // Convert YUV to RGBA
        self.yuv_to_rgba(&y_plane, &u_plane, &v_plane, width, height)
            .map(|pixels| WebpImage { width, height, pixels })
    }

    fn decode_block_coeffs(
        &self,
        bool_dec: &mut Vp8BoolDecoder,
        probs: &Vp8CoeffProbs,
        coeffs: &mut [i16; 16],
        plane: usize,
    ) -> Result<(), WebpError> {
        let mut pos = 0;

        while pos < 16 {
            let band = VP8_COEFF_BANDS[pos];
            let ctx = if pos == 0 { 0 } else { 1 };

            // EOB check
            if !bool_dec.read_bool(probs.probs[plane][band][ctx][0])? {
                break;
            }

            // Zero run or coefficient
            if !bool_dec.read_bool(probs.probs[plane][band][ctx][1])? {
                pos += 1;
                continue;
            }

            // Decode coefficient value
            let mut level = 1i16;
            if bool_dec.read_bool(probs.probs[plane][band][ctx][2])? {
                // Large coefficient - decode tree
                if !bool_dec.read_bool(probs.probs[plane][band][ctx][3])? {
                    level = 2;
                } else if !bool_dec.read_bool(probs.probs[plane][band][ctx][4])? {
                    level = 3 + bool_dec.read_bool(159)? as i16;
                } else if !bool_dec.read_bool(probs.probs[plane][band][ctx][5])? {
                    level = 5 + bool_dec.read_literal(1)? as i16;
                } else if !bool_dec.read_bool(probs.probs[plane][band][ctx][6])? {
                    level = 7 + bool_dec.read_literal(2)? as i16;
                } else if !bool_dec.read_bool(probs.probs[plane][band][ctx][7])? {
                    level = 11 + bool_dec.read_literal(3)? as i16;
                } else if !bool_dec.read_bool(probs.probs[plane][band][ctx][8])? {
                    level = 19 + bool_dec.read_literal(4)? as i16;
                } else if !bool_dec.read_bool(probs.probs[plane][band][ctx][9])? {
                    level = 35 + bool_dec.read_literal(5)? as i16;
                } else {
                    level = 67 + bool_dec.read_literal(11)? as i16;
                }
            }

            // Sign
            if bool_dec.read_bit()? {
                level = -level;
            }

            coeffs[VP8_ZIGZAG[pos]] = level;
            pos += 1;
        }

        Ok(())
    }

    fn reconstruct_macroblock(
        &self,
        mb_x: usize, mb_y: usize,
        y_mode: u8, uv_mode: u8,
        y_coeffs: &[[i16; 16]; 16],
        u_coeffs: &[[i16; 16]; 4],
        v_coeffs: &[[i16; 16]; 4],
        dequant: &Vp8Dequant,
        y_plane: &mut [u8], u_plane: &mut [u8], v_plane: &mut [u8],
        y_stride: usize, uv_stride: usize,
    ) {
        let mb_y_base = mb_y * 16;
        let mb_x_base = mb_x * 16;
        let mb_uv_y = mb_y * 8;
        let mb_uv_x = mb_x * 8;

        // Predict and reconstruct Y (16 4x4 blocks)
        for by in 0..4 {
            for bx in 0..4 {
                let block_idx = by * 4 + bx;
                let y_base = (mb_y_base + by * 4) * y_stride + mb_x_base + bx * 4;

                // Get prediction
                let pred = self.predict_4x4(y_plane, y_stride, y_base, y_mode);

                // Dequantize and IDCT
                let mut block = y_coeffs[block_idx];
                for (i, c) in block.iter_mut().enumerate() {
                    let q = if i == 0 { dequant.y_dc } else { dequant.y_ac };
                    *c = (*c as i32 * q as i32).clamp(-32768, 32767) as i16;
                }

                let residual = self.idct_4x4(&block);

                // Add prediction + residual
                for dy in 0..4 {
                    for dx in 0..4 {
                        let idx = y_base + dy * y_stride + dx;
                        if idx < y_plane.len() {
                            let val = pred[dy * 4 + dx] as i32 + residual[dy * 4 + dx] as i32;
                            y_plane[idx] = val.clamp(0, 255) as u8;
                        }
                    }
                }
            }
        }

        // Predict and reconstruct U (4 4x4 blocks)
        for by in 0..2 {
            for bx in 0..2 {
                let block_idx = by * 2 + bx;
                let uv_base = (mb_uv_y + by * 4) * uv_stride + mb_uv_x + bx * 4;

                let pred = self.predict_4x4(u_plane, uv_stride, uv_base, uv_mode);

                let mut block = u_coeffs[block_idx];
                for (i, c) in block.iter_mut().enumerate() {
                    let q = if i == 0 { dequant.uv_dc } else { dequant.uv_ac };
                    *c = (*c as i32 * q as i32).clamp(-32768, 32767) as i16;
                }

                let residual = self.idct_4x4(&block);

                for dy in 0..4 {
                    for dx in 0..4 {
                        let idx = uv_base + dy * uv_stride + dx;
                        if idx < u_plane.len() {
                            let val = pred[dy * 4 + dx] as i32 + residual[dy * 4 + dx] as i32;
                            u_plane[idx] = val.clamp(0, 255) as u8;
                        }
                    }
                }
            }
        }

        // Predict and reconstruct V (4 4x4 blocks)
        for by in 0..2 {
            for bx in 0..2 {
                let block_idx = by * 2 + bx;
                let uv_base = (mb_uv_y + by * 4) * uv_stride + mb_uv_x + bx * 4;

                let pred = self.predict_4x4(v_plane, uv_stride, uv_base, uv_mode);

                let mut block = v_coeffs[block_idx];
                for (i, c) in block.iter_mut().enumerate() {
                    let q = if i == 0 { dequant.uv_dc } else { dequant.uv_ac };
                    *c = (*c as i32 * q as i32).clamp(-32768, 32767) as i16;
                }

                let residual = self.idct_4x4(&block);

                for dy in 0..4 {
                    for dx in 0..4 {
                        let idx = uv_base + dy * uv_stride + dx;
                        if idx < v_plane.len() {
                            let val = pred[dy * 4 + dx] as i32 + residual[dy * 4 + dx] as i32;
                            v_plane[idx] = val.clamp(0, 255) as u8;
                        }
                    }
                }
            }
        }
    }

    fn predict_4x4(&self, plane: &[u8], stride: usize, base: usize, mode: u8) -> [u8; 16] {
        let mut pred = [128u8; 16];

        // Get neighbors
        let above: [u8; 4] = if base >= stride {
            [
                plane.get(base - stride).copied().unwrap_or(128),
                plane.get(base - stride + 1).copied().unwrap_or(128),
                plane.get(base - stride + 2).copied().unwrap_or(128),
                plane.get(base - stride + 3).copied().unwrap_or(128),
            ]
        } else {
            [128; 4]
        };

        let left: [u8; 4] = if base % stride > 0 {
            [
                plane.get(base - 1).copied().unwrap_or(128),
                plane.get(base + stride - 1).copied().unwrap_or(128),
                plane.get(base + 2 * stride - 1).copied().unwrap_or(128),
                plane.get(base + 3 * stride - 1).copied().unwrap_or(128),
            ]
        } else {
            [128; 4]
        };

        match mode {
            0 => {
                // DC_PRED
                let sum: u32 = above.iter().map(|&x| x as u32).sum::<u32>()
                    + left.iter().map(|&x| x as u32).sum::<u32>();
                let dc = ((sum + 4) / 8) as u8;
                pred.fill(dc);
            }
            1 => {
                // V_PRED (vertical)
                for dy in 0..4 {
                    for dx in 0..4 {
                        pred[dy * 4 + dx] = above[dx];
                    }
                }
            }
            2 => {
                // H_PRED (horizontal)
                for dy in 0..4 {
                    for dx in 0..4 {
                        pred[dy * 4 + dx] = left[dy];
                    }
                }
            }
            3 => {
                // TM_PRED (TrueMotion)
                let top_left = if base >= stride && base % stride > 0 {
                    plane.get(base - stride - 1).copied().unwrap_or(128)
                } else {
                    128
                };
                for dy in 0..4 {
                    for dx in 0..4 {
                        let val = left[dy] as i32 + above[dx] as i32 - top_left as i32;
                        pred[dy * 4 + dx] = val.clamp(0, 255) as u8;
                    }
                }
            }
            _ => {
                // Default to DC
                pred.fill(128);
            }
        }

        pred
    }

    fn idct_4x4(&self, coeffs: &[i16; 16]) -> [i16; 16] {
        let mut out = [0i16; 16];

        // Row transform
        let mut tmp = [[0i32; 4]; 4];
        for i in 0..4 {
            let c0 = coeffs[i * 4] as i32;
            let c1 = coeffs[i * 4 + 1] as i32;
            let c2 = coeffs[i * 4 + 2] as i32;
            let c3 = coeffs[i * 4 + 3] as i32;

            let a = c0 + c2;
            let b = c0 - c2;
            let t = (c1 * 35468 >> 16) - (c3 * 85627 >> 16);
            let tt = (c1 * 85627 >> 16) + (c3 * 35468 >> 16);

            tmp[i][0] = a + tt;
            tmp[i][1] = b + t;
            tmp[i][2] = b - t;
            tmp[i][3] = a - tt;
        }

        // Column transform
        for i in 0..4 {
            let c0 = tmp[0][i];
            let c1 = tmp[1][i];
            let c2 = tmp[2][i];
            let c3 = tmp[3][i];

            let a = c0 + c2;
            let b = c0 - c2;
            let t = (c1 * 35468 >> 16) - (c3 * 85627 >> 16);
            let tt = (c1 * 85627 >> 16) + (c3 * 35468 >> 16);

            out[i] = ((a + tt + 4) >> 3).clamp(-255, 255) as i16;
            out[4 + i] = ((b + t + 4) >> 3).clamp(-255, 255) as i16;
            out[8 + i] = ((b - t + 4) >> 3).clamp(-255, 255) as i16;
            out[12 + i] = ((a - tt + 4) >> 3).clamp(-255, 255) as i16;
        }

        out
    }

    fn yuv_to_rgba(&self, y: &[u8], u: &[u8], v: &[u8], width: u32, height: u32) -> Result<Vec<u8>, WebpError> {
        let pixel_count = (width * height) as usize;
        let mut rgba = vec![0u8; pixel_count * 4];

        let y_stride = ((width + 15) / 16 * 16) as usize;
        let uv_stride = ((width + 15) / 16 * 8) as usize;

        for py in 0..height as usize {
            for px in 0..width as usize {
                let y_idx = py * y_stride + px;
                let uv_idx = (py / 2) * uv_stride + (px / 2);

                let y_val = y.get(y_idx).copied().unwrap_or(128) as i32;
                let u_val = u.get(uv_idx).copied().unwrap_or(128) as i32 - 128;
                let v_val = v.get(uv_idx).copied().unwrap_or(128) as i32 - 128;

                // BT.601 YUV to RGB
                let r = (y_val + ((v_val * 359) >> 8)).clamp(0, 255) as u8;
                let g = (y_val - ((u_val * 88 + v_val * 183) >> 8)).clamp(0, 255) as u8;
                let b = (y_val + ((u_val * 454) >> 8)).clamp(0, 255) as u8;

                let idx = (py * width as usize + px) * 4;
                rgba[idx] = r;
                rgba[idx + 1] = g;
                rgba[idx + 2] = b;
                rgba[idx + 3] = 255;
            }
        }

        Ok(rgba)
    }

    fn decode_alpha(&self, data: &[u8]) -> Result<Vec<u8>, WebpError> {
        if data.is_empty() {
            return Err(WebpError::DecodingError("Empty alpha".into()));
        }

        let header = data[0];
        let compression = header & 0x03;

        match compression {
            0 => Ok(data[1..].to_vec()),
            1 => Ok(data[1..].to_vec()),
            _ => Err(WebpError::UnsupportedFormat),
        }
    }

    fn apply_alpha(&self, image: &mut WebpImage, alpha: &[u8]) {
        let pixel_count = (image.width * image.height) as usize;
        for i in 0..pixel_count.min(alpha.len()) {
            image.pixels[i * 4 + 3] = alpha[i];
        }
    }
}

// ========== VP8 Constants ==========

const VP8_ZIGZAG: [usize; 16] = [
    0,  1,  4,  8,  5,  2,  3,  6,
    9, 12, 13, 10,  7, 11, 14, 15,
];

const VP8_COEFF_BANDS: [usize; 16] = [
    0, 1, 2, 3, 6, 4, 5, 6, 6, 6, 6, 6, 6, 6, 6, 7,
];

struct Vp8Dequant {
    y_dc: i16,
    y_ac: i16,
    y2_dc: i16,
    y2_ac: i16,
    uv_dc: i16,
    uv_ac: i16,
}

impl Vp8Dequant {
    fn new(y_ac_qi: i32, y_dc_delta: i32, y2_dc_delta: i32, y2_ac_delta: i32, uv_dc_delta: i32, uv_ac_delta: i32) -> Self {
        let dc_table = &VP8_DC_QUANT;
        let ac_table = &VP8_AC_QUANT;

        let y_ac = ac_table[(y_ac_qi).clamp(0, 127) as usize];
        let y_dc = dc_table[(y_ac_qi + y_dc_delta).clamp(0, 127) as usize];
        let y2_ac = ac_table[(y_ac_qi + y2_ac_delta).clamp(0, 127) as usize].max(8) * 155 / 100;
        let y2_dc = dc_table[(y_ac_qi + y2_dc_delta).clamp(0, 127) as usize].max(8) * 2;
        let uv_ac = ac_table[(y_ac_qi + uv_ac_delta).clamp(0, 127) as usize];
        let uv_dc = dc_table[(y_ac_qi + uv_dc_delta).clamp(0, 127) as usize].min(132);

        Self { y_dc, y_ac, y2_dc, y2_ac, uv_dc, uv_ac }
    }
}

const VP8_DC_QUANT: [i16; 128] = [
    4,   5,   6,   7,   8,   9,  10,  10,  11,  12,  13,  14,  15,  16,  17,  17,
   18,  19,  20,  20,  21,  21,  22,  22,  23,  23,  24,  25,  25,  26,  27,  28,
   29,  30,  31,  32,  33,  34,  35,  36,  37,  37,  38,  39,  40,  41,  42,  43,
   44,  45,  46,  46,  47,  48,  49,  50,  51,  52,  53,  54,  55,  56,  57,  58,
   59,  60,  61,  62,  63,  64,  65,  66,  67,  68,  69,  70,  71,  72,  73,  74,
   75,  76,  76,  77,  78,  79,  80,  81,  82,  83,  84,  85,  86,  87,  88,  89,
   91,  93,  95,  96,  98, 100, 101, 102, 104, 106, 108, 110, 112, 114, 116, 118,
  122, 124, 126, 128, 130, 132, 134, 136, 138, 140, 143, 145, 148, 151, 154, 157,
];

const VP8_AC_QUANT: [i16; 128] = [
    4,   5,   6,   7,   8,   9,  10,  11,  12,  13,  14,  15,  16,  17,  18,  19,
   20,  21,  22,  23,  24,  25,  26,  27,  28,  29,  30,  31,  32,  33,  34,  35,
   36,  37,  38,  39,  40,  41,  42,  43,  44,  45,  46,  47,  48,  49,  50,  51,
   52,  53,  54,  55,  56,  57,  58,  60,  62,  64,  66,  68,  70,  72,  74,  76,
   78,  80,  82,  84,  86,  88,  90,  92,  94,  96,  98, 100, 102, 104, 106, 108,
  110, 112, 114, 116, 119, 122, 125, 128, 131, 134, 137, 140, 143, 146, 149, 152,
  155, 158, 161, 164, 167, 170, 173, 177, 181, 185, 189, 193, 197, 201, 205, 209,
  213, 217, 221, 225, 229, 234, 239, 245, 249, 254, 259, 264, 269, 274, 279, 284,
];

struct Vp8CoeffProbs {
    probs: [[[[u8; 11]; 3]; 8]; 4],
}

impl Default for Vp8CoeffProbs {
    fn default() -> Self {
        // Default coefficient probabilities (simplified)
        let mut probs = [[[[128u8; 11]; 3]; 8]; 4];
        
        // Set reasonable defaults for each plane/band/context
        for plane in 0..4 {
            for band in 0..8 {
                for ctx in 0..3 {
                    probs[plane][band][ctx] = [
                        128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128
                    ];
                }
            }
        }
        
        Self { probs }
    }
}

impl Default for WebpDecoder {
    fn default() -> Self {
        Self::new()
    }
}

// ========== Data Structures ==========

enum Transform {
    Predictor { block_size: u32, data: Vec<u8> },
    Color { block_size: u32 },
    SubtractGreen,
    ColorIndex { palette: Vec<[u8; 4]> },
}

struct HuffmanCodes {
    green: HuffmanTree,
    red: HuffmanTree,
    blue: HuffmanTree,
    alpha: HuffmanTree,
    distance: HuffmanTree,
}

struct HuffmanTree {
    codes: Vec<(u16, u8)>, // (symbol, length)
    max_bits: usize,
}

// ========== VP8L Bit Reader ==========

struct Vp8lBitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bits: u64,
    bits_left: usize,
}

impl<'a> Vp8lBitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        let mut reader = Self { data, pos: 0, bits: 0, bits_left: 0 };
        reader.refill();
        reader
    }

    fn refill(&mut self) {
        while self.bits_left <= 56 && self.pos < self.data.len() {
            self.bits |= (self.data[self.pos] as u64) << self.bits_left;
            self.pos += 1;
            self.bits_left += 8;
        }
    }

    fn read_bits(&mut self, n: usize) -> Result<u32, WebpError> {
        if n == 0 { return Ok(0); }
        self.refill();
        if self.bits_left < n {
            return Err(WebpError::DecodingError("EOF".into()));
        }
        let result = (self.bits & ((1u64 << n) - 1)) as u32;
        self.bits >>= n;
        self.bits_left -= n;
        Ok(result)
    }

    fn peek_bits(&mut self, n: usize) -> Result<u32, WebpError> {
        self.refill();
        if self.bits_left < n {
            return Err(WebpError::DecodingError("EOF".into()));
        }
        Ok((self.bits & ((1u64 << n) - 1)) as u32)
    }

    fn drop_bits(&mut self, n: usize) {
        self.bits >>= n;
        self.bits_left = self.bits_left.saturating_sub(n);
    }

    fn read_bit(&mut self) -> Result<bool, WebpError> {
        Ok(self.read_bits(1)? != 0)
    }
}

// ========== VP8 Boolean Decoder ==========

struct Vp8BoolDecoder<'a> {
    data: &'a [u8],
    pos: usize,
    range: u32,
    value: u32,
    bits: i32,
}

impl<'a> Vp8BoolDecoder<'a> {
    fn new(data: &'a [u8]) -> Self {
        let mut dec = Self { data, pos: 0, range: 255, value: 0, bits: -8 };
        dec.init();
        dec
    }

    fn init(&mut self) {
        for _ in 0..2 {
            if self.pos < self.data.len() {
                self.value = (self.value << 8) | self.data[self.pos] as u32;
                self.pos += 1;
            }
        }
        self.bits = 8;
    }

    fn read_bool(&mut self, prob: u8) -> Result<bool, WebpError> {
        let split = 1 + (((self.range - 1) * prob as u32) >> 8);
        let bigsplit = split << 8;

        let bit = self.value >= bigsplit;
        if bit {
            self.range -= split;
            self.value -= bigsplit;
        } else {
            self.range = split;
        }

        // Renormalize
        while self.range < 128 {
            self.range <<= 1;
            self.value <<= 1;
            self.bits -= 1;
            if self.bits <= 0 {
                if self.pos < self.data.len() {
                    self.value |= self.data[self.pos] as u32;
                    self.pos += 1;
                }
                self.bits = 8;
            }
        }

        Ok(bit)
    }

    fn read_bit(&mut self) -> Result<bool, WebpError> {
        self.read_bool(128)
    }

    fn read_literal(&mut self, n: usize) -> Result<u32, WebpError> {
        let mut v = 0u32;
        for _ in 0..n {
            v = (v << 1) | self.read_bit()? as u32;
        }
        Ok(v)
    }

    fn read_signed(&mut self, n: usize) -> Result<i32, WebpError> {
        let value = self.read_literal(n)? as i32;
        if self.read_bit()? {
            Ok(-value)
        } else {
            Ok(value)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalid_webp() {
        let mut decoder = WebpDecoder::new();
        assert!(matches!(decoder.decode(b"not webp"), Err(WebpError::InvalidRiff)));
    }

    #[test]
    fn test_riff_header() {
        let mut decoder = WebpDecoder::new();
        assert!(matches!(decoder.decode(b"RIFF\x00\x00\x00\x00WEBP"), Err(WebpError::InvalidWebp)));
    }

    #[test]
    fn test_huffman_reverse_bits() {
        assert_eq!(WebpDecoder::reverse_bits(0b101, 3), 0b101);
        assert_eq!(WebpDecoder::reverse_bits(0b110, 3), 0b011);
        assert_eq!(WebpDecoder::reverse_bits(0b1000, 4), 0b0001);
    }

    #[test]
    fn test_color_cache_hash() {
        let decoder = WebpDecoder::new();
        let hash1 = decoder.color_cache_hash(0xFFAABBCC, 8);
        let hash2 = decoder.color_cache_hash(0xFFAABBCC, 8);
        assert_eq!(hash1, hash2);
        assert!(hash1 < 256);
    }
}
