//! GIF Decoder (GIF89a/GIF87a)
//!
//! From-scratch GIF decoder with LZW decompression.

/// GIF decoding error
#[derive(Debug, Clone)]
pub enum GifError {
    InvalidSignature,
    InvalidHeader,
    InvalidImageDescriptor,
    LzwError(String),
    UnexpectedEof,
}

impl std::fmt::Display for GifError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSignature => write!(f, "Invalid GIF signature"),
            Self::InvalidHeader => write!(f, "Invalid GIF header"),
            Self::InvalidImageDescriptor => write!(f, "Invalid image descriptor"),
            Self::LzwError(e) => write!(f, "LZW error: {}", e),
            Self::UnexpectedEof => write!(f, "Unexpected end of file"),
        }
    }
}

impl std::error::Error for GifError {}

/// A single GIF frame
#[derive(Debug, Clone)]
pub struct GifFrame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>, // RGBA
    pub delay_ms: u32,
    pub dispose: DisposeMethod,
}

/// Frame disposal method
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DisposeMethod {
    #[default]
    None,
    Background,
    Previous,
}

/// GIF decoder
pub struct GifDecoder {
    width: u16,
    height: u16,
    global_color_table: Vec<[u8; 3]>,
    background_index: u8,
}

impl GifDecoder {
    pub fn new() -> Self {
        Self {
            width: 0,
            height: 0,
            global_color_table: Vec::new(),
            background_index: 0,
        }
    }

    /// Decode GIF from bytes, returning all frames
    pub fn decode(&mut self, data: &[u8]) -> Result<Vec<GifFrame>, GifError> {
        if data.len() < 6 {
            return Err(GifError::InvalidSignature);
        }

        // Check signature
        if &data[0..3] != b"GIF" {
            return Err(GifError::InvalidSignature);
        }

        let version = &data[3..6];
        if version != b"87a" && version != b"89a" {
            return Err(GifError::InvalidSignature);
        }

        // Logical Screen Descriptor
        if data.len() < 13 {
            return Err(GifError::InvalidHeader);
        }

        self.width = u16::from_le_bytes([data[6], data[7]]);
        self.height = u16::from_le_bytes([data[8], data[9]]);

        let packed = data[10];
        let has_gct = (packed & 0x80) != 0;
        let gct_size = 1 << ((packed & 0x07) + 1);

        self.background_index = data[11];
        // data[12] is pixel aspect ratio, usually ignored

        let mut pos = 13;

        // Read Global Color Table
        if has_gct {
            self.global_color_table = self.read_color_table(data, pos, gct_size)?;
            pos += gct_size * 3;
        }

        let mut frames = Vec::new();
        let mut delay_ms = 0u32;
        let mut dispose = DisposeMethod::None;
        let mut transparent_index: Option<u8> = None;

        // Parse blocks
        while pos < data.len() {
            match data[pos] {
                0x21 => {
                    // Extension
                    pos += 1;
                    if pos >= data.len() {
                        break;
                    }

                    match data[pos] {
                        0xF9 => {
                            // Graphics Control Extension
                            pos += 1;
                            if pos + 6 > data.len() {
                                return Err(GifError::UnexpectedEof);
                            }
                            let block_size = data[pos] as usize;
                            pos += 1;

                            let packed = data[pos];
                            dispose = match (packed >> 2) & 0x07 {
                                2 => DisposeMethod::Background,
                                3 => DisposeMethod::Previous,
                                _ => DisposeMethod::None,
                            };

                            let has_transparent = (packed & 0x01) != 0;

                            delay_ms = u16::from_le_bytes([data[pos + 1], data[pos + 2]]) as u32 * 10;

                            if has_transparent {
                                transparent_index = Some(data[pos + 3]);
                            } else {
                                transparent_index = None;
                            }

                            pos += block_size + 1; // +1 for terminator
                        }
                        0xFF => {
                            // Application Extension
                            pos += 1;
                            pos = self.skip_sub_blocks(data, pos);
                        }
                        0xFE => {
                            // Comment Extension
                            pos += 1;
                            pos = self.skip_sub_blocks(data, pos);
                        }
                        0x01 => {
                            // Plain Text Extension
                            pos += 1;
                            pos = self.skip_sub_blocks(data, pos);
                        }
                        _ => {
                            // Unknown extension
                            pos += 1;
                            pos = self.skip_sub_blocks(data, pos);
                        }
                    }
                }
                0x2C => {
                    // Image Descriptor
                    pos += 1;
                    let frame = self.decode_image(
                        data, &mut pos,
                        delay_ms, dispose, transparent_index,
                    )?;
                    frames.push(frame);

                    // Reset for next frame
                    transparent_index = None;
                }
                0x3B => {
                    // Trailer
                    break;
                }
                _ => {
                    pos += 1;
                }
            }
        }

        Ok(frames)
    }

    fn read_color_table(&self, data: &[u8], pos: usize, count: usize) -> Result<Vec<[u8; 3]>, GifError> {
        let end = pos + count * 3;
        if end > data.len() {
            return Err(GifError::UnexpectedEof);
        }

        Ok((0..count).map(|i| {
            let base = pos + i * 3;
            [data[base], data[base + 1], data[base + 2]]
        }).collect())
    }

    fn skip_sub_blocks(&self, data: &[u8], mut pos: usize) -> usize {
        while pos < data.len() {
            let block_size = data[pos] as usize;
            pos += 1;
            if block_size == 0 {
                break;
            }
            pos += block_size;
        }
        pos
    }

    fn decode_image(
        &self,
        data: &[u8],
        pos: &mut usize,
        delay_ms: u32,
        dispose: DisposeMethod,
        transparent_index: Option<u8>,
    ) -> Result<GifFrame, GifError> {
        if *pos + 9 > data.len() {
            return Err(GifError::InvalidImageDescriptor);
        }

        let left = u16::from_le_bytes([data[*pos], data[*pos + 1]]) as usize;
        let top = u16::from_le_bytes([data[*pos + 2], data[*pos + 3]]) as usize;
        let width = u16::from_le_bytes([data[*pos + 4], data[*pos + 5]]);
        let height = u16::from_le_bytes([data[*pos + 6], data[*pos + 7]]);
        let packed = data[*pos + 8];

        *pos += 9;

        let has_lct = (packed & 0x80) != 0;
        let interlaced = (packed & 0x40) != 0;
        let lct_size = 1 << ((packed & 0x07) + 1);

        // Read Local Color Table if present
        let color_table = if has_lct {
            let lct = self.read_color_table(data, *pos, lct_size)?;
            *pos += lct_size * 3;
            lct
        } else {
            self.global_color_table.clone()
        };

        // LZW Minimum Code Size
        if *pos >= data.len() {
            return Err(GifError::UnexpectedEof);
        }
        let min_code_size = data[*pos];
        *pos += 1;

        // Read sub-blocks
        let mut lzw_data = Vec::new();
        while *pos < data.len() {
            let block_size = data[*pos] as usize;
            *pos += 1;
            if block_size == 0 {
                break;
            }
            if *pos + block_size > data.len() {
                return Err(GifError::UnexpectedEof);
            }
            lzw_data.extend_from_slice(&data[*pos..*pos + block_size]);
            *pos += block_size;
        }

        // Decompress LZW
        let indices = self.decompress_lzw(&lzw_data, min_code_size)?;

        // Build frame pixels
        let frame_width = self.width as usize;
        let frame_height = self.height as usize;
        let mut rgba = vec![0u8; frame_width * frame_height * 4];

        // Fill with background
        if self.background_index < color_table.len() as u8 {
            let bg = color_table[self.background_index as usize];
            for y in 0..frame_height {
                for x in 0..frame_width {
                    let idx = (y * frame_width + x) * 4;
                    rgba[idx] = bg[0];
                    rgba[idx + 1] = bg[1];
                    rgba[idx + 2] = bg[2];
                    rgba[idx + 3] = 255;
                }
            }
        }

        // Draw image
        let img_width = width as usize;
        let img_height = height as usize;

        for (i, &color_idx) in indices.iter().enumerate() {
            let img_x = i % img_width;
            let img_y = if interlaced {
                self.deinterlace_row(i / img_width, img_height)
            } else {
                i / img_width
            };

            let x = left + img_x;
            let y = top + img_y;

            if x >= frame_width || y >= frame_height {
                continue;
            }

            let is_transparent = transparent_index == Some(color_idx);

            if !is_transparent && (color_idx as usize) < color_table.len() {
                let color = color_table[color_idx as usize];
                let idx = (y * frame_width + x) * 4;
                rgba[idx] = color[0];
                rgba[idx + 1] = color[1];
                rgba[idx + 2] = color[2];
                rgba[idx + 3] = 255;
            } else if is_transparent {
                let idx = (y * frame_width + x) * 4;
                rgba[idx + 3] = 0;
            }
        }

        Ok(GifFrame {
            width: self.width as u32,
            height: self.height as u32,
            pixels: rgba,
            delay_ms,
            dispose,
        })
    }

    fn deinterlace_row(&self, pass_row: usize, height: usize) -> usize {
        // GIF interlacing: 4 passes
        // Pass 1: rows 0, 8, 16, ... (step 8, start 0)
        // Pass 2: rows 4, 12, 20, ... (step 8, start 4)
        // Pass 3: rows 2, 6, 10, ... (step 4, start 2)
        // Pass 4: rows 1, 3, 5, ... (step 2, start 1)

        let pass1_rows = (height + 7) / 8;
        let pass2_rows = (height + 3) / 8;
        let pass3_rows = (height + 1) / 4;
        // let pass4_rows = height / 2;

        if pass_row < pass1_rows {
            pass_row * 8
        } else if pass_row < pass1_rows + pass2_rows {
            (pass_row - pass1_rows) * 8 + 4
        } else if pass_row < pass1_rows + pass2_rows + pass3_rows {
            (pass_row - pass1_rows - pass2_rows) * 4 + 2
        } else {
            (pass_row - pass1_rows - pass2_rows - pass3_rows) * 2 + 1
        }
    }

    fn decompress_lzw(&self, data: &[u8], min_code_size: u8) -> Result<Vec<u8>, GifError> {
        let clear_code = 1u16 << min_code_size;
        let end_code = clear_code + 1;

        let mut code_size = min_code_size as usize + 1;
        let mut next_code = end_code + 1;
        let mut max_code = (1 << code_size) - 1;

        // LZW table: each entry is (prefix_code, suffix_byte)
        // For codes 0..clear_code, they represent single bytes
        let mut table: Vec<(u16, u8)> = Vec::with_capacity(4096);
        for i in 0..clear_code {
            table.push((0xFFFF, i as u8)); // 0xFFFF means "no prefix"
        }
        table.push((0xFFFF, 0)); // Clear code placeholder
        table.push((0xFFFF, 0)); // End code placeholder

        let mut reader = LzwBitReader::new(data);
        let mut output = Vec::new();

        let mut prev_code: Option<u16> = None;

        loop {
            let code = reader.read_bits(code_size)?;

            if code == clear_code {
                // Reset
                code_size = min_code_size as usize + 1;
                max_code = (1 << code_size) - 1;
                next_code = end_code + 1;
                table.truncate(end_code as usize + 1);
                prev_code = None;
                continue;
            }

            if code == end_code {
                break;
            }

            if code < next_code {
                // Code is in table
                self.output_string(&table, code, &mut output);

                if let Some(prev) = prev_code {
                    let first_char = self.first_char(&table, code);
                    if next_code < 4096 {
                        table.push((prev, first_char));
                        next_code += 1;
                    }
                }
            } else if code == next_code {
                // Special case: code not yet in table
                if let Some(prev) = prev_code {
                    let first_char = self.first_char(&table, prev);
                    if next_code < 4096 {
                        table.push((prev, first_char));
                        next_code += 1;
                    }
                    self.output_string(&table, code, &mut output);
                }
            } else {
                return Err(GifError::LzwError("Invalid code".into()));
            }

            prev_code = Some(code);

            // Increase code size if needed
            if next_code > max_code && code_size < 12 {
                code_size += 1;
                max_code = (1 << code_size) - 1;
            }
        }

        Ok(output)
    }

    fn output_string(&self, table: &[(u16, u8)], mut code: u16, output: &mut Vec<u8>) {
        let mut stack = Vec::new();

        while code != 0xFFFF && (code as usize) < table.len() {
            let (prefix, suffix) = table[code as usize];
            stack.push(suffix);
            code = prefix;
        }

        while let Some(byte) = stack.pop() {
            output.push(byte);
        }
    }

    fn first_char(&self, table: &[(u16, u8)], mut code: u16) -> u8 {
        while code != 0xFFFF && (code as usize) < table.len() {
            let (prefix, suffix) = table[code as usize];
            if prefix == 0xFFFF {
                return suffix;
            }
            code = prefix;
        }
        0
    }
}

impl Default for GifDecoder {
    fn default() -> Self {
        Self::new()
    }
}

/// Bit reader for LZW data
struct LzwBitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bit_pos: usize,
}

impl<'a> LzwBitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            bit_pos: 0,
        }
    }

    fn read_bits(&mut self, n: usize) -> Result<u16, GifError> {
        let mut result = 0u16;
        let mut bits_read = 0;

        while bits_read < n {
            if self.pos >= self.data.len() {
                return Err(GifError::UnexpectedEof);
            }

            let bits_available = 8 - self.bit_pos;
            let bits_to_read = (n - bits_read).min(bits_available);

            let mask = ((1 << bits_to_read) - 1) as u8;
            let bits = (self.data[self.pos] >> self.bit_pos) & mask;

            result |= (bits as u16) << bits_read;
            bits_read += bits_to_read;
            self.bit_pos += bits_to_read;

            if self.bit_pos >= 8 {
                self.bit_pos = 0;
                self.pos += 1;
            }
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalid_gif() {
        let mut decoder = GifDecoder::new();
        let result = decoder.decode(b"not a gif");
        assert!(matches!(result, Err(GifError::InvalidSignature)));
    }

    #[test]
    fn test_gif_signature() {
        let mut decoder = GifDecoder::new();
        let result = decoder.decode(b"GIF89a");
        // Should fail because too short, but signature is valid
        assert!(matches!(result, Err(GifError::InvalidHeader)));
    }

    #[test]
    fn test_deinterlace() {
        let decoder = GifDecoder::new();
        // Test 8-row image
        assert_eq!(decoder.deinterlace_row(0, 8), 0); // Pass 1, row 0
        assert_eq!(decoder.deinterlace_row(1, 8), 4); // Pass 2, row 4
        assert_eq!(decoder.deinterlace_row(2, 8), 2); // Pass 3, row 2
        assert_eq!(decoder.deinterlace_row(3, 8), 6); // Pass 3, row 6
    }
}
