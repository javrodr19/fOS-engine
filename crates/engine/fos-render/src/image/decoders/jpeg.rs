//! JPEG Decoder (JFIF/Exif)
//!
//! From-scratch JPEG decoder with SIMD-accelerated IDCT and YCbCr conversion.

use super::simd::SimdOps;

/// JPEG decoding error
#[derive(Debug, Clone)]
pub enum JpegError {
    InvalidMarker,
    InvalidSoi,
    InvalidDqt,
    InvalidDht,
    InvalidSof,
    InvalidSos,
    UnsupportedFormat,
    HuffmanError,
    DecodingError(String),
}

impl std::fmt::Display for JpegError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidMarker => write!(f, "Invalid JPEG marker"),
            Self::InvalidSoi => write!(f, "Missing SOI marker"),
            Self::InvalidDqt => write!(f, "Invalid DQT segment"),
            Self::InvalidDht => write!(f, "Invalid DHT segment"),
            Self::InvalidSof => write!(f, "Invalid SOF segment"),
            Self::InvalidSos => write!(f, "Invalid SOS segment"),
            Self::UnsupportedFormat => write!(f, "Unsupported JPEG format"),
            Self::HuffmanError => write!(f, "Huffman decoding error"),
            Self::DecodingError(e) => write!(f, "Decoding error: {}", e),
        }
    }
}

impl std::error::Error for JpegError {}

/// Decoded JPEG image
#[derive(Debug, Clone)]
pub struct JpegImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>, // RGBA
}

/// JPEG decoder
pub struct JpegDecoder {
    simd: SimdOps,
    // Quantization tables (up to 4)
    quant_tables: [[u16; 64]; 4],
    // Huffman tables: [DC/AC][table_id]
    dc_tables: [HuffmanTable; 4],
    ac_tables: [HuffmanTable; 4],
    // Frame info
    width: u16,
    height: u16,
    components: Vec<Component>,
    // Scan data
    mcu_width: usize,
    mcu_height: usize,
}

#[derive(Clone, Default)]
struct Component {
    id: u8,
    h_sampling: u8,
    v_sampling: u8,
    quant_table: u8,
    dc_table: u8,
    ac_table: u8,
    dc_pred: i16,
}

#[derive(Clone)]
struct HuffmanTable {
    // Fast lookup table for codes up to 8 bits
    fast: [i16; 256], // -1 = not valid
    // Values for each code
    values: Vec<u8>,
    // Min/max codes for each code length
    min_code: [u32; 17],
    max_code: [i32; 17],
    val_ptr: [usize; 17],
}

impl Default for HuffmanTable {
    fn default() -> Self {
        Self {
            fast: [-1i16; 256],
            values: Vec::new(),
            min_code: [0; 17],
            max_code: [-1; 17],
            val_ptr: [0; 17],
        }
    }
}

impl JpegDecoder {
    pub fn new() -> Self {
        Self {
            simd: SimdOps::new(),
            quant_tables: [[0; 64]; 4],
            dc_tables: Default::default(),
            ac_tables: Default::default(),
            width: 0,
            height: 0,
            components: Vec::new(),
            mcu_width: 0,
            mcu_height: 0,
        }
    }

    /// Decode JPEG from bytes
    pub fn decode(&mut self, data: &[u8]) -> Result<JpegImage, JpegError> {
        if data.len() < 2 || data[0] != 0xFF || data[1] != 0xD8 {
            return Err(JpegError::InvalidSoi);
        }

        let mut pos = 2;

        // Parse markers
        while pos + 2 <= data.len() {
            if data[pos] != 0xFF {
                pos += 1;
                continue;
            }

            // Skip padding 0xFF bytes
            while pos < data.len() && data[pos] == 0xFF {
                pos += 1;
            }

            if pos >= data.len() {
                break;
            }

            let marker = data[pos];
            pos += 1;

            match marker {
                0xD8 => {}, // SOI - already handled
                0xD9 => break, // EOI
                0xD0..=0xD7 => {}, // RST markers
                0x00 => {}, // Stuffed byte
                0xDB => pos = self.parse_dqt(data, pos)?, // DQT
                0xC4 => pos = self.parse_dht(data, pos)?, // DHT
                0xC0 | 0xC1 => pos = self.parse_sof(data, pos)?, // SOF0/SOF1 (baseline/extended)
                0xC2 => return Err(JpegError::UnsupportedFormat), // Progressive - complex
                0xDA => {
                    // SOS - Start of Scan
                    pos = self.parse_sos(data, pos)?;
                    // Decode scan data
                    let pixels = self.decode_scan(data, pos)?;
                    return Ok(JpegImage {
                        width: self.width as u32,
                        height: self.height as u32,
                        pixels,
                    });
                }
                0xE0..=0xEF | 0xFE => {
                    // APP segments and comments - skip
                    if pos + 2 > data.len() {
                        return Err(JpegError::InvalidMarker);
                    }
                    let length = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
                    pos += length;
                }
                _ => {
                    // Skip unknown marker
                    if pos + 2 > data.len() {
                        break;
                    }
                    let length = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
                    pos += length;
                }
            }
        }

        Err(JpegError::DecodingError("No image data found".into()))
    }

    fn parse_dqt(&mut self, data: &[u8], pos: usize) -> Result<usize, JpegError> {
        if pos + 2 > data.len() {
            return Err(JpegError::InvalidDqt);
        }

        let length = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
        let end = pos + length;

        let mut p = pos + 2;
        while p < end {
            let info = data[p];
            let precision = (info >> 4) & 0x0F;
            let table_id = (info & 0x0F) as usize;

            if table_id >= 4 {
                return Err(JpegError::InvalidDqt);
            }

            p += 1;

            if precision == 0 {
                // 8-bit values
                for i in 0..64 {
                    if p >= data.len() {
                        return Err(JpegError::InvalidDqt);
                    }
                    self.quant_tables[table_id][ZIGZAG[i]] = data[p] as u16;
                    p += 1;
                }
            } else {
                // 16-bit values
                for i in 0..64 {
                    if p + 1 >= data.len() {
                        return Err(JpegError::InvalidDqt);
                    }
                    self.quant_tables[table_id][ZIGZAG[i]] =
                        u16::from_be_bytes([data[p], data[p + 1]]);
                    p += 2;
                }
            }
        }

        Ok(end)
    }

    fn parse_dht(&mut self, data: &[u8], pos: usize) -> Result<usize, JpegError> {
        if pos + 2 > data.len() {
            return Err(JpegError::InvalidDht);
        }

        let length = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
        let end = pos + length;

        let mut p = pos + 2;
        while p < end {
            let info = data[p];
            let table_class = (info >> 4) & 0x0F; // 0 = DC, 1 = AC
            let table_id = (info & 0x0F) as usize;

            if table_id >= 4 {
                return Err(JpegError::InvalidDht);
            }

            p += 1;

            // Read code counts for lengths 1-16
            let mut counts = [0u8; 17];
            let mut total = 0usize;
            for i in 1..=16 {
                if p >= data.len() {
                    return Err(JpegError::InvalidDht);
                }
                counts[i] = data[p];
                total += counts[i] as usize;
                p += 1;
            }

            // Read values
            let mut values = Vec::with_capacity(total);
            for _ in 0..total {
                if p >= data.len() {
                    return Err(JpegError::InvalidDht);
                }
                values.push(data[p]);
                p += 1;
            }

            // Build Huffman table
            let table = self.build_huffman_table(&counts, &values);

            if table_class == 0 {
                self.dc_tables[table_id] = table;
            } else {
                self.ac_tables[table_id] = table;
            }
        }

        Ok(end)
    }

    fn build_huffman_table(&self, counts: &[u8; 17], values: &[u8]) -> HuffmanTable {
        let mut table = HuffmanTable::default();
        table.fast = [-1; 256];
        table.values = values.to_vec();

        let mut code = 0u32;
        let mut val_idx = 0usize;

        for bits in 1..=16 {
            table.min_code[bits] = code;
            table.val_ptr[bits] = val_idx;

            for _ in 0..counts[bits] {
                // Fill fast table for codes up to 8 bits
                if bits <= 8 {
                    let fill_bits = 8 - bits;
                    let base = (code as usize) << fill_bits;
                    for i in 0..(1 << fill_bits) {
                        table.fast[base + i] = val_idx as i16;
                    }
                }
                code += 1;
                val_idx += 1;
            }

            table.max_code[bits] = code as i32 - 1;
            code <<= 1;
        }

        table
    }

    fn parse_sof(&mut self, data: &[u8], pos: usize) -> Result<usize, JpegError> {
        if pos + 2 > data.len() {
            return Err(JpegError::InvalidSof);
        }

        let length = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;

        let precision = data[pos + 2];
        if precision != 8 {
            return Err(JpegError::UnsupportedFormat);
        }

        self.height = u16::from_be_bytes([data[pos + 3], data[pos + 4]]);
        self.width = u16::from_be_bytes([data[pos + 5], data[pos + 6]]);

        let num_components = data[pos + 7] as usize;
        self.components.clear();

        let mut max_h = 1;
        let mut max_v = 1;

        for i in 0..num_components {
            let base = pos + 8 + i * 3;
            let id = data[base];
            let sampling = data[base + 1];
            let h_sampling = (sampling >> 4) & 0x0F;
            let v_sampling = sampling & 0x0F;
            let quant_table = data[base + 2];

            max_h = max_h.max(h_sampling);
            max_v = max_v.max(v_sampling);

            self.components.push(Component {
                id,
                h_sampling,
                v_sampling,
                quant_table,
                dc_table: 0,
                ac_table: 0,
                dc_pred: 0,
            });
        }

        self.mcu_width = (self.width as usize + max_h as usize * 8 - 1) / (max_h as usize * 8);
        self.mcu_height = (self.height as usize + max_v as usize * 8 - 1) / (max_v as usize * 8);

        Ok(pos + length)
    }

    fn parse_sos(&mut self, data: &[u8], pos: usize) -> Result<usize, JpegError> {
        if pos + 2 > data.len() {
            return Err(JpegError::InvalidSos);
        }

        let length = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
        let num_components = data[pos + 2] as usize;

        for i in 0..num_components {
            let base = pos + 3 + i * 2;
            let id = data[base];
            let tables = data[base + 1];
            let dc_table = (tables >> 4) & 0x0F;
            let ac_table = tables & 0x0F;

            for comp in &mut self.components {
                if comp.id == id {
                    comp.dc_table = dc_table;
                    comp.ac_table = ac_table;
                }
            }
        }

        Ok(pos + length)
    }

    fn decode_scan(&mut self, data: &[u8], start: usize) -> Result<Vec<u8>, JpegError> {
        let mut reader = BitReader::new(&data[start..]);

        // Allocate output
        let width = self.width as usize;
        let height = self.height as usize;
        let mut rgb = vec![0u8; width * height * 3];

        // Determine MCU size
        let max_h = self.components.iter().map(|c| c.h_sampling).max().unwrap_or(1) as usize;
        let max_v = self.components.iter().map(|c| c.v_sampling).max().unwrap_or(1) as usize;
        let mcu_pixel_width = max_h * 8;
        let mcu_pixel_height = max_v * 8;

        // Allocate component buffers
        let mut comp_data: Vec<Vec<i16>> = self.components.iter().map(|c| {
            let blocks_h = c.h_sampling as usize;
            let blocks_v = c.v_sampling as usize;
            vec![0i16; blocks_h * blocks_v * 64]
        }).collect();

        // Reset DC predictors
        for comp in &mut self.components {
            comp.dc_pred = 0;
        }

        // Extract component info we need
        let comp_info: Vec<_> = self.components.iter().map(|c| {
            (c.h_sampling, c.v_sampling, c.quant_table, c.dc_table, c.ac_table)
        }).collect();

        // Clone tables to avoid borrow issues
        let dc_tables = self.dc_tables.clone();
        let ac_tables = self.ac_tables.clone();
        let quant_tables = self.quant_tables.clone();

        // Track DC predictors separately
        let mut dc_preds: Vec<i16> = self.components.iter().map(|c| c.dc_pred).collect();

        // Decode MCUs
        for mcu_y in 0..self.mcu_height {
            for mcu_x in 0..self.mcu_width {
                // Decode each component's blocks in this MCU
                for (comp_idx, &(h_sampling, v_sampling, quant_table, dc_table_idx, ac_table_idx)) in comp_info.iter().enumerate() {
                    let blocks_h = h_sampling as usize;
                    let blocks_v = v_sampling as usize;

                    for by in 0..blocks_v {
                        for bx in 0..blocks_h {
                            let block_idx = by * blocks_h + bx;
                            let block_offset = block_idx * 64;
                            let block = &mut comp_data[comp_idx][block_offset..block_offset + 64];

                            // Initialize block to zero
                            block.fill(0);

                            // Decode DC coefficient
                            let dc_table = &dc_tables[dc_table_idx as usize];
                            let dc_size = Self::decode_huffman_static(&mut reader, dc_table)?;
                            let dc_diff = if dc_size > 0 {
                                Self::decode_value_static(&mut reader, dc_size)?
                            } else {
                                0
                            };
                            dc_preds[comp_idx] = dc_preds[comp_idx].wrapping_add(dc_diff);
                            block[0] = dc_preds[comp_idx];

                            // Decode AC coefficients
                            let ac_table = &ac_tables[ac_table_idx as usize];
                            let mut k = 1;
                            while k < 64 {
                                let ac_code = Self::decode_huffman_static(&mut reader, ac_table)?;
                                let run = (ac_code >> 4) & 0x0F;
                                let size = ac_code & 0x0F;

                                if size == 0 {
                                    if run == 0 {
                                        break; // EOB
                                    } else if run == 0x0F {
                                        k += 16; // ZRL
                                    }
                                } else {
                                    k += run as usize;
                                    if k < 64 {
                                        block[ZIGZAG[k]] = Self::decode_value_static(&mut reader, size)?;
                                    }
                                    k += 1;
                                }
                            }

                            // Dequantize
                            let qt = &quant_tables[quant_table as usize];
                            for i in 0..64 {
                                block[i] = block[i].wrapping_mul(qt[i] as i16);
                            }

                            // IDCT
                            let mut block_arr = [0i16; 64];
                            block_arr.copy_from_slice(block);
                            self.simd.jpeg_idct_8x8(&mut block_arr);
                            block.copy_from_slice(&block_arr);
                        }
                    }
                }

                // Convert MCU to RGB
                self.mcu_to_rgb(
                    mcu_x, mcu_y,
                    mcu_pixel_width, mcu_pixel_height,
                    &comp_data,
                    &mut rgb,
                    width, height,
                );
            }
        }

        // Convert RGB to RGBA
        let mut rgba = vec![255u8; width * height * 4];
        for i in 0..(width * height) {
            rgba[i * 4] = rgb[i * 3];
            rgba[i * 4 + 1] = rgb[i * 3 + 1];
            rgba[i * 4 + 2] = rgb[i * 3 + 2];
        }

        Ok(rgba)
    }

    fn decode_huffman_static(reader: &mut BitReader, table: &HuffmanTable) -> Result<u8, JpegError> {
        // Try fast lookup
        let peek = reader.peek_bits(8)?;
        let fast_val = table.fast[peek as usize];
        if fast_val >= 0 {
            // Find actual code length
            let val = table.values[fast_val as usize];
            // Determine how many bits to drop
            for bits in 1..=8 {
                let code = peek >> (8 - bits);
                if table.max_code[bits] >= 0 && code as i32 <= table.max_code[bits] {
                    reader.drop_bits(bits);
                    return Ok(val);
                }
            }
        }

        // Slow path - read bit by bit
        let mut code = 0u32;
        for bits in 1..=16 {
            code = (code << 1) | reader.read_bit()? as u32;
            if table.max_code[bits] >= 0 && code as i32 <= table.max_code[bits] {
                let idx = table.val_ptr[bits] + (code - table.min_code[bits]) as usize;
                if idx < table.values.len() {
                    return Ok(table.values[idx]);
                }
            }
        }

        Err(JpegError::HuffmanError)
    }

    fn decode_value_static(reader: &mut BitReader, size: u8) -> Result<i16, JpegError> {
        if size == 0 {
            return Ok(0);
        }

        let bits = reader.read_bits(size as usize)?;
        let half = 1 << (size - 1);

        Ok(if bits < half {
            bits as i16 - (half as i16 * 2 - 1)
        } else {
            bits as i16
        })
    }

    #[allow(dead_code)]
    fn decode_huffman(&self, reader: &mut BitReader, table: &HuffmanTable) -> Result<u8, JpegError> {
        Self::decode_huffman_static(reader, table)
    }

    #[allow(dead_code)]
    fn decode_value(&self, reader: &mut BitReader, size: u8) -> Result<i16, JpegError> {
        Self::decode_value_static(reader, size)
    }

    fn mcu_to_rgb(
        &self,
        mcu_x: usize, mcu_y: usize,
        mcu_width: usize, mcu_height: usize,
        comp_data: &[Vec<i16>],
        rgb: &mut [u8],
        img_width: usize, img_height: usize,
    ) {
        let base_x = mcu_x * mcu_width;
        let base_y = mcu_y * mcu_height;

        // Determine subsampling
        let max_h = self.components.iter().map(|c| c.h_sampling).max().unwrap_or(1);
        let max_v = self.components.iter().map(|c| c.v_sampling).max().unwrap_or(1);

        for py in 0..mcu_height {
            let y = base_y + py;
            if y >= img_height {
                continue;
            }

            for px in 0..mcu_width {
                let x = base_x + px;
                if x >= img_width {
                    continue;
                }

                // Sample each component
                let mut samples = [0i16; 3];
                for (comp_idx, comp) in self.components.iter().enumerate().take(3) {
                    let h_scale = max_h / comp.h_sampling;
                    let v_scale = max_v / comp.v_sampling;

                    let cx = px / (h_scale as usize * 8);
                    let cy = py / (v_scale as usize * 8);
                    let bx = (px / h_scale as usize) % 8;
                    let by = (py / v_scale as usize) % 8;

                    let block_idx = cy * comp.h_sampling as usize + cx;
                    let sample_idx = block_idx * 64 + by * 8 + bx;

                    if sample_idx < comp_data[comp_idx].len() {
                        samples[comp_idx] = comp_data[comp_idx][sample_idx];
                    }
                }

                // YCbCr to RGB conversion
                let yy = samples[0] as i32 + 128;
                let cb = samples[1] as i32;
                let cr = samples[2] as i32;

                let r = (yy + ((cr * 359) >> 8)).clamp(0, 255) as u8;
                let g = (yy - ((cb * 88 + cr * 183) >> 8)).clamp(0, 255) as u8;
                let b = (yy + ((cb * 454) >> 8)).clamp(0, 255) as u8;

                let idx = (y * img_width + x) * 3;
                rgb[idx] = r;
                rgb[idx + 1] = g;
                rgb[idx + 2] = b;
            }
        }
    }
}

impl Default for JpegDecoder {
    fn default() -> Self {
        Self::new()
    }
}

/// Bit reader for entropy-coded data
struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bit_buf: u32,
    bits_left: usize,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            bit_buf: 0,
            bits_left: 0,
        }
    }

    fn fill_bits(&mut self) -> Result<(), JpegError> {
        while self.bits_left <= 24 && self.pos < self.data.len() {
            let mut byte = self.data[self.pos];
            self.pos += 1;

            // Handle byte stuffing (0xFF followed by 0x00)
            if byte == 0xFF {
                if self.pos < self.data.len() {
                    let next = self.data[self.pos];
                    if next == 0x00 {
                        self.pos += 1;
                    } else if next >= 0xD0 && next <= 0xD7 {
                        // RST marker - skip and continue
                        self.pos += 1;
                        continue;
                    } else {
                        // Other marker - stop
                        break;
                    }
                }
            }

            self.bit_buf |= (byte as u32) << (24 - self.bits_left);
            self.bits_left += 8;
        }
        Ok(())
    }

    fn peek_bits(&mut self, n: usize) -> Result<u32, JpegError> {
        if self.bits_left < n {
            self.fill_bits()?;
        }
        Ok(self.bit_buf >> (32 - n))
    }

    fn drop_bits(&mut self, n: usize) {
        self.bit_buf <<= n;
        self.bits_left = self.bits_left.saturating_sub(n);
    }

    fn read_bits(&mut self, n: usize) -> Result<u32, JpegError> {
        let val = self.peek_bits(n)?;
        self.drop_bits(n);
        Ok(val)
    }

    fn read_bit(&mut self) -> Result<u32, JpegError> {
        self.read_bits(1)
    }
}

/// Zigzag order for 8x8 DCT block
const ZIGZAG: [usize; 64] = [
    0,  1,  8,  16, 9,  2,  3,  10,
    17, 24, 32, 25, 18, 11, 4,  5,
    12, 19, 26, 33, 40, 48, 41, 34,
    27, 20, 13, 6,  7,  14, 21, 28,
    35, 42, 49, 56, 57, 50, 43, 36,
    29, 22, 15, 23, 30, 37, 44, 51,
    58, 59, 52, 45, 38, 31, 39, 46,
    53, 60, 61, 54, 47, 55, 62, 63,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalid_jpeg() {
        let mut decoder = JpegDecoder::new();
        let result = decoder.decode(b"not a jpeg");
        assert!(matches!(result, Err(JpegError::InvalidSoi)));
    }

    #[test]
    fn test_zigzag_order() {
        // Verify zigzag covers all 64 positions
        let mut visited = [false; 64];
        for &idx in &ZIGZAG {
            visited[idx] = true;
        }
        assert!(visited.iter().all(|&v| v));
    }
}
