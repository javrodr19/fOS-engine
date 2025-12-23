//! PNG Decoder (RFC 2083)
//!
//! From-scratch PNG decoder with SIMD-accelerated filter reconstruction.

use super::deflate::{Inflate, DeflateError};
use super::simd::SimdOps;

/// PNG decoding error
#[derive(Debug, Clone)]
pub enum PngError {
    InvalidSignature,
    InvalidChunk,
    MissingIhdr,
    InvalidColorType,
    InvalidBitDepth,
    DecompressionFailed(String),
    InvalidFilterType,
    InterlaceError,
    CrcMismatch,
}

impl std::fmt::Display for PngError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSignature => write!(f, "Invalid PNG signature"),
            Self::InvalidChunk => write!(f, "Invalid chunk"),
            Self::MissingIhdr => write!(f, "Missing IHDR chunk"),
            Self::InvalidColorType => write!(f, "Invalid color type"),
            Self::InvalidBitDepth => write!(f, "Invalid bit depth"),
            Self::DecompressionFailed(e) => write!(f, "Decompression failed: {}", e),
            Self::InvalidFilterType => write!(f, "Invalid filter type"),
            Self::InterlaceError => write!(f, "Interlace error"),
            Self::CrcMismatch => write!(f, "CRC mismatch"),
        }
    }
}

impl std::error::Error for PngError {}

impl From<DeflateError> for PngError {
    fn from(e: DeflateError) -> Self {
        PngError::DecompressionFailed(e.to_string())
    }
}

/// PNG color types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorType {
    Grayscale = 0,
    Rgb = 2,
    Indexed = 3,
    GrayscaleAlpha = 4,
    Rgba = 6,
}

impl ColorType {
    fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Grayscale),
            2 => Some(Self::Rgb),
            3 => Some(Self::Indexed),
            4 => Some(Self::GrayscaleAlpha),
            6 => Some(Self::Rgba),
            _ => None,
        }
    }

    /// Channels per pixel
    fn channels(self) -> usize {
        match self {
            Self::Grayscale => 1,
            Self::Rgb => 3,
            Self::Indexed => 1,
            Self::GrayscaleAlpha => 2,
            Self::Rgba => 4,
        }
    }
}

/// Decoded PNG image
#[derive(Debug, Clone)]
pub struct PngImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>, // RGBA
}

/// PNG decoder
pub struct PngDecoder {
    simd: SimdOps,
    inflate: Inflate,
}

impl PngDecoder {
    pub fn new() -> Self {
        Self {
            simd: SimdOps::new(),
            inflate: Inflate::new(),
        }
    }

    /// Decode PNG from bytes
    pub fn decode(&mut self, data: &[u8]) -> Result<PngImage, PngError> {
        // Check signature
        if data.len() < 8 || &data[0..8] != b"\x89PNG\r\n\x1a\n" {
            return Err(PngError::InvalidSignature);
        }

        let mut pos = 8;
        let mut ihdr: Option<IhdrData> = None;
        let mut palette: Vec<[u8; 3]> = Vec::new();
        let mut transparency: Option<Vec<u8>> = None;
        let mut idat_data = Vec::new();

        // Parse chunks
        while pos + 12 <= data.len() {
            let length = u32::from_be_bytes([data[pos], data[pos+1], data[pos+2], data[pos+3]]) as usize;
            let chunk_type = &data[pos+4..pos+8];

            if pos + 12 + length > data.len() {
                return Err(PngError::InvalidChunk);
            }

            let chunk_data = &data[pos+8..pos+8+length];
            let _crc = u32::from_be_bytes([
                data[pos+8+length],
                data[pos+9+length],
                data[pos+10+length],
                data[pos+11+length],
            ]);

            match chunk_type {
                b"IHDR" => {
                    ihdr = Some(self.parse_ihdr(chunk_data)?);
                }
                b"PLTE" => {
                    palette = self.parse_plte(chunk_data)?;
                }
                b"tRNS" => {
                    transparency = Some(chunk_data.to_vec());
                }
                b"IDAT" => {
                    idat_data.extend_from_slice(chunk_data);
                }
                b"IEND" => {
                    break;
                }
                _ => {
                    // Skip unknown chunks
                }
            }

            pos += 12 + length;
        }

        let ihdr = ihdr.ok_or(PngError::MissingIhdr)?;

        // Decompress IDAT
        let decompressed = self.inflate.inflate_zlib(&idat_data)?;

        // Reconstruct image
        let raw_pixels = if ihdr.interlace == 1 {
            self.decode_interlaced(&decompressed, &ihdr)?
        } else {
            self.decode_non_interlaced(&decompressed, &ihdr)?
        };

        // Convert to RGBA
        let pixels = self.to_rgba(&raw_pixels, &ihdr, &palette, &transparency)?;

        Ok(PngImage {
            width: ihdr.width,
            height: ihdr.height,
            pixels,
        })
    }

    fn parse_ihdr(&self, data: &[u8]) -> Result<IhdrData, PngError> {
        if data.len() < 13 {
            return Err(PngError::InvalidChunk);
        }

        let width = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        let height = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        let bit_depth = data[8];
        let color_type = ColorType::from_u8(data[9]).ok_or(PngError::InvalidColorType)?;
        let _compression = data[10];
        let _filter = data[11];
        let interlace = data[12];

        // Validate bit depth for color type
        let valid = match color_type {
            ColorType::Grayscale => matches!(bit_depth, 1 | 2 | 4 | 8 | 16),
            ColorType::Rgb => matches!(bit_depth, 8 | 16),
            ColorType::Indexed => matches!(bit_depth, 1 | 2 | 4 | 8),
            ColorType::GrayscaleAlpha => matches!(bit_depth, 8 | 16),
            ColorType::Rgba => matches!(bit_depth, 8 | 16),
        };

        if !valid {
            return Err(PngError::InvalidBitDepth);
        }

        Ok(IhdrData {
            width,
            height,
            bit_depth,
            color_type,
            interlace,
        })
    }

    fn parse_plte(&self, data: &[u8]) -> Result<Vec<[u8; 3]>, PngError> {
        if data.len() % 3 != 0 {
            return Err(PngError::InvalidChunk);
        }

        Ok(data.chunks(3)
            .map(|c| [c[0], c[1], c[2]])
            .collect())
    }

    fn decode_non_interlaced(&self, data: &[u8], ihdr: &IhdrData) -> Result<Vec<u8>, PngError> {
        let bpp = self.bytes_per_pixel(ihdr);
        let scanline_bytes = self.scanline_bytes(ihdr.width as usize, ihdr);

        let mut output = vec![0u8; ihdr.height as usize * scanline_bytes];
        let mut prev_row = vec![0u8; scanline_bytes];

        let mut in_pos = 0;
        for y in 0..ihdr.height as usize {
            if in_pos >= data.len() {
                return Err(PngError::InvalidFilterType);
            }

            let filter_type = data[in_pos];
            in_pos += 1;

            if in_pos + scanline_bytes > data.len() {
                return Err(PngError::InvalidFilterType);
            }

            let row_start = y * scanline_bytes;
            output[row_start..row_start + scanline_bytes]
                .copy_from_slice(&data[in_pos..in_pos + scanline_bytes]);
            in_pos += scanline_bytes;

            let row = &mut output[row_start..row_start + scanline_bytes];
            self.unfilter_row(filter_type, row, &prev_row, bpp)?;

            prev_row.copy_from_slice(row);
        }

        Ok(output)
    }

    fn decode_interlaced(&self, data: &[u8], ihdr: &IhdrData) -> Result<Vec<u8>, PngError> {
        // Adam7 interlacing pass parameters
        const ADAM7_X_START: [usize; 7] = [0, 4, 0, 2, 0, 1, 0];
        const ADAM7_Y_START: [usize; 7] = [0, 0, 4, 0, 2, 0, 1];
        const ADAM7_X_STEP: [usize; 7] = [8, 8, 4, 4, 2, 2, 1];
        const ADAM7_Y_STEP: [usize; 7] = [8, 8, 8, 4, 4, 2, 2];

        let full_scanline = self.scanline_bytes(ihdr.width as usize, ihdr);
        let mut output = vec![0u8; ihdr.height as usize * full_scanline];
        let bpp = self.bytes_per_pixel(ihdr);

        let mut in_pos = 0;

        for pass in 0..7 {
            let x_start = ADAM7_X_START[pass];
            let y_start = ADAM7_Y_START[pass];
            let x_step = ADAM7_X_STEP[pass];
            let y_step = ADAM7_Y_STEP[pass];

            let pass_width = (ihdr.width as usize + x_step - 1 - x_start) / x_step;
            let pass_height = (ihdr.height as usize + y_step - 1 - y_start) / y_step;

            if pass_width == 0 || pass_height == 0 {
                continue;
            }

            let pass_scanline = self.scanline_bytes(pass_width, ihdr);
            let mut prev_row = vec![0u8; pass_scanline];
            let mut row = vec![0u8; pass_scanline];

            for py in 0..pass_height {
                if in_pos >= data.len() {
                    return Err(PngError::InterlaceError);
                }

                let filter_type = data[in_pos];
                in_pos += 1;

                if in_pos + pass_scanline > data.len() {
                    return Err(PngError::InterlaceError);
                }

                row.copy_from_slice(&data[in_pos..in_pos + pass_scanline]);
                in_pos += pass_scanline;

                self.unfilter_row(filter_type, &mut row, &prev_row, bpp)?;

                // Copy to output
                let y = y_start + py * y_step;
                for px in 0..pass_width {
                    let x = x_start + px * x_step;
                    let src_start = px * bpp;
                    let dst_start = y * full_scanline + x * bpp;

                    for b in 0..bpp {
                        output[dst_start + b] = row[src_start + b];
                    }
                }

                prev_row.copy_from_slice(&row);
            }
        }

        Ok(output)
    }

    fn unfilter_row(&self, filter: u8, row: &mut [u8], prev: &[u8], bpp: usize) -> Result<(), PngError> {
        match filter {
            0 => {}, // None
            1 => self.simd.png_unfilter_sub(row, bpp),
            2 => self.simd.png_unfilter_up(row, prev),
            3 => self.simd.png_unfilter_avg(row, prev, bpp),
            4 => self.simd.png_unfilter_paeth(row, prev, bpp),
            _ => return Err(PngError::InvalidFilterType),
        }
        Ok(())
    }

    fn bytes_per_pixel(&self, ihdr: &IhdrData) -> usize {
        let bits = ihdr.color_type.channels() * ihdr.bit_depth as usize;
        (bits + 7) / 8
    }

    fn scanline_bytes(&self, width: usize, ihdr: &IhdrData) -> usize {
        let bits = width * ihdr.color_type.channels() * ihdr.bit_depth as usize;
        (bits + 7) / 8
    }

    fn to_rgba(
        &self,
        raw: &[u8],
        ihdr: &IhdrData,
        palette: &[[u8; 3]],
        transparency: &Option<Vec<u8>>,
    ) -> Result<Vec<u8>, PngError> {
        let pixel_count = ihdr.width as usize * ihdr.height as usize;
        let mut rgba = vec![255u8; pixel_count * 4];
        let scanline = self.scanline_bytes(ihdr.width as usize, ihdr);

        for y in 0..ihdr.height as usize {
            for x in 0..ihdr.width as usize {
                let out_idx = (y * ihdr.width as usize + x) * 4;

                match ihdr.color_type {
                    ColorType::Grayscale => {
                        let v = self.read_sample(raw, y, x, 0, ihdr, scanline);
                        rgba[out_idx] = v;
                        rgba[out_idx + 1] = v;
                        rgba[out_idx + 2] = v;
                        if let Some(t) = transparency {
                            if t.len() >= 2 {
                                let trans_val = if ihdr.bit_depth == 16 {
                                    ((t[0] as u16) << 8 | t[1] as u16) as u8
                                } else {
                                    t[1]
                                };
                                if v == trans_val {
                                    rgba[out_idx + 3] = 0;
                                }
                            }
                        }
                    }
                    ColorType::Rgb => {
                        rgba[out_idx] = self.read_sample(raw, y, x, 0, ihdr, scanline);
                        rgba[out_idx + 1] = self.read_sample(raw, y, x, 1, ihdr, scanline);
                        rgba[out_idx + 2] = self.read_sample(raw, y, x, 2, ihdr, scanline);
                    }
                    ColorType::Indexed => {
                        let idx = self.read_sample(raw, y, x, 0, ihdr, scanline) as usize;
                        if idx < palette.len() {
                            rgba[out_idx] = palette[idx][0];
                            rgba[out_idx + 1] = palette[idx][1];
                            rgba[out_idx + 2] = palette[idx][2];
                            if let Some(t) = transparency {
                                if idx < t.len() {
                                    rgba[out_idx + 3] = t[idx];
                                }
                            }
                        }
                    }
                    ColorType::GrayscaleAlpha => {
                        let v = self.read_sample(raw, y, x, 0, ihdr, scanline);
                        rgba[out_idx] = v;
                        rgba[out_idx + 1] = v;
                        rgba[out_idx + 2] = v;
                        rgba[out_idx + 3] = self.read_sample(raw, y, x, 1, ihdr, scanline);
                    }
                    ColorType::Rgba => {
                        rgba[out_idx] = self.read_sample(raw, y, x, 0, ihdr, scanline);
                        rgba[out_idx + 1] = self.read_sample(raw, y, x, 1, ihdr, scanline);
                        rgba[out_idx + 2] = self.read_sample(raw, y, x, 2, ihdr, scanline);
                        rgba[out_idx + 3] = self.read_sample(raw, y, x, 3, ihdr, scanline);
                    }
                }
            }
        }

        Ok(rgba)
    }

    fn read_sample(&self, raw: &[u8], y: usize, x: usize, channel: usize, ihdr: &IhdrData, scanline: usize) -> u8 {
        let channels = ihdr.color_type.channels();
        let bit_depth = ihdr.bit_depth as usize;

        if bit_depth == 8 {
            let byte_idx = y * scanline + x * channels + channel;
            if byte_idx < raw.len() {
                raw[byte_idx]
            } else {
                0
            }
        } else if bit_depth == 16 {
            let byte_idx = y * scanline + (x * channels + channel) * 2;
            if byte_idx + 1 < raw.len() {
                // Return high byte (downscale 16-bit to 8-bit)
                raw[byte_idx]
            } else {
                0
            }
        } else {
            // Sub-byte bit depths (1, 2, 4)
            let pixel_bit = x * channels * bit_depth + channel * bit_depth;
            let byte_idx = y * scanline + pixel_bit / 8;
            let bit_offset = 8 - bit_depth - (pixel_bit % 8);

            if byte_idx < raw.len() {
                let mask = (1 << bit_depth) - 1;
                let value = (raw[byte_idx] >> bit_offset) & mask as u8;
                // Scale to 8-bit
                match bit_depth {
                    1 => value * 255,
                    2 => value * 85,
                    4 => value * 17,
                    _ => value,
                }
            } else {
                0
            }
        }
    }
}

impl Default for PngDecoder {
    fn default() -> Self {
        Self::new()
    }
}

/// IHDR chunk data
struct IhdrData {
    width: u32,
    height: u32,
    bit_depth: u8,
    color_type: ColorType,
    interlace: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalid_signature() {
        let mut decoder = PngDecoder::new();
        let result = decoder.decode(b"not a png");
        assert!(matches!(result, Err(PngError::InvalidSignature)));
    }

    #[test]
    fn test_color_type() {
        assert_eq!(ColorType::from_u8(0), Some(ColorType::Grayscale));
        assert_eq!(ColorType::from_u8(2), Some(ColorType::Rgb));
        assert_eq!(ColorType::from_u8(6), Some(ColorType::Rgba));
        assert_eq!(ColorType::from_u8(7), None);
    }

    #[test]
    fn test_channels() {
        assert_eq!(ColorType::Grayscale.channels(), 1);
        assert_eq!(ColorType::Rgb.channels(), 3);
        assert_eq!(ColorType::Rgba.channels(), 4);
    }
}
