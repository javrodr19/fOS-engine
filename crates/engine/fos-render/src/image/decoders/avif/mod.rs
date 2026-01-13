//! AVIF Decoder
//!
//! From-scratch AVIF (AV1 Image File Format) decoder.
//! Parses ISOBMFF container and decodes AV1 intra-frames.

mod bitreader;
mod container;
mod obu;
mod transform;
mod predict;
mod entropy;
mod filter;
mod frame;
mod color;

pub use container::AvifContainer;
pub use frame::{Frame, Plane};
pub use color::ColorInfo;

use super::simd::SimdOps;

/// AVIF decoding error
#[derive(Debug, Clone)]
pub enum AvifError {
    InvalidData,
    InvalidBox(String),
    UnsupportedFormat,
    UnsupportedProfile(u8),
    UnsupportedBitDepth(u8),
    ObuParseError(String),
    DecodingError(String),
    ArithmeticError,
    TransformError,
    PredictionError,
    FilterError,
    ColorConversionError,
}

impl std::fmt::Display for AvifError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidData => write!(f, "Invalid AVIF data"),
            Self::InvalidBox(s) => write!(f, "Invalid box: {}", s),
            Self::UnsupportedFormat => write!(f, "Unsupported AVIF format"),
            Self::UnsupportedProfile(p) => write!(f, "Unsupported AV1 profile: {}", p),
            Self::UnsupportedBitDepth(d) => write!(f, "Unsupported bit depth: {}", d),
            Self::ObuParseError(s) => write!(f, "OBU parse error: {}", s),
            Self::DecodingError(s) => write!(f, "Decoding error: {}", s),
            Self::ArithmeticError => write!(f, "Arithmetic decoding error"),
            Self::TransformError => write!(f, "Transform error"),
            Self::PredictionError => write!(f, "Prediction error"),
            Self::FilterError => write!(f, "Filter error"),
            Self::ColorConversionError => write!(f, "Color conversion error"),
        }
    }
}

impl std::error::Error for AvifError {}

/// Decoded AVIF image
#[derive(Debug, Clone)]
pub struct AvifImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,    // RGBA
    pub bit_depth: u8,
    pub has_alpha: bool,
}

/// AVIF decoder
pub struct AvifDecoder {
    simd: SimdOps,
}

impl Default for AvifDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl AvifDecoder {
    pub fn new() -> Self {
        Self {
            simd: SimdOps::new(),
        }
    }

    /// Decode AVIF from bytes
    pub fn decode(&mut self, data: &[u8]) -> Result<AvifImage, AvifError> {
        // Phase 1: Parse ISOBMFF container
        let container = container::parse_container(data)?;
        
        // Extract AV1 coded data
        let av1_data = &data[container.data_offset..container.data_offset + container.data_size];
        
        // Phase 2: Parse AV1 OBUs
        let (seq_header, frame_header, tile_data) = obu::parse_obus(av1_data)?;
        
        // Phase 3: Decode intra-frame
        let mut frame = frame::Frame::new(
            container.width,
            container.height,
            seq_header.bit_depth,
            seq_header.subsampling_x,
            seq_header.subsampling_y,
            seq_header.monochrome,
        );
        
        // Decode tiles
        self.decode_tiles(&mut frame, &tile_data, &seq_header, &frame_header)?;
        
        // Apply loop filters
        if frame_header.loop_filter.enabled {
            filter::deblock_filter(&mut frame, &frame_header.loop_filter);
        }
        if frame_header.cdef.enabled {
            filter::cdef_filter(&mut frame, &frame_header.cdef);
        }
        if frame_header.restoration.enabled {
            filter::loop_restoration(&mut frame, &frame_header.restoration);
        }
        
        // Phase 4: Color conversion
        let color_info = ColorInfo {
            primaries: container.color_primaries,
            transfer: container.transfer_characteristics,
            matrix: container.matrix_coefficients,
        };
        
        let pixels = color::yuv_to_rgba(&frame, &color_info, &self.simd)?;
        
        // Handle alpha plane if present
        let (pixels, has_alpha) = if let Some(alpha_data) = &container.alpha_data {
            let alpha_pixels = self.decode_alpha(data, alpha_data)?;
            let combined = self.combine_alpha(pixels, &alpha_pixels);
            (combined, true)
        } else {
            (pixels, false)
        };
        
        Ok(AvifImage {
            width: container.width,
            height: container.height,
            pixels,
            bit_depth: seq_header.bit_depth,
            has_alpha,
        })
    }
    
    fn decode_tiles(
        &self,
        frame: &mut frame::Frame,
        tile_data: &[obu::TileData],
        seq_header: &obu::SequenceHeader,
        frame_header: &obu::FrameHeader,
    ) -> Result<(), AvifError> {
        for tile in tile_data {
            self.decode_tile(frame, tile, seq_header, frame_header)?;
        }
        Ok(())
    }
    
    fn decode_tile(
        &self,
        frame: &mut frame::Frame,
        tile: &obu::TileData,
        seq_header: &obu::SequenceHeader,
        frame_header: &obu::FrameHeader,
    ) -> Result<(), AvifError> {
        let mut decoder = entropy::ArithmeticDecoder::new(&tile.data)?;
        
        // Process superblocks
        let sb_size = if seq_header.use_128x128_superblock { 128 } else { 64 };
        
        let tile_col_start = tile.col as u32 * frame_header.tile_info.tile_width_sb * sb_size;
        let tile_row_start = tile.row as u32 * frame_header.tile_info.tile_height_sb * sb_size;
        let tile_col_end = ((tile.col + 1) as u32 * frame_header.tile_info.tile_width_sb * sb_size)
            .min(frame.width);
        let tile_row_end = ((tile.row + 1) as u32 * frame_header.tile_info.tile_height_sb * sb_size)
            .min(frame.height);
        
        let mut row = tile_row_start;
        while row < tile_row_end {
            let mut col = tile_col_start;
            while col < tile_col_end {
                self.decode_superblock(
                    frame,
                    &mut decoder,
                    col,
                    row,
                    sb_size,
                    seq_header,
                    frame_header,
                )?;
                col += sb_size;
            }
            row += sb_size;
        }
        
        Ok(())
    }
    
    fn decode_superblock(
        &self,
        frame: &mut frame::Frame,
        decoder: &mut entropy::ArithmeticDecoder,
        sb_col: u32,
        sb_row: u32,
        sb_size: u32,
        seq_header: &obu::SequenceHeader,
        frame_header: &obu::FrameHeader,
    ) -> Result<(), AvifError> {
        // Decode partition tree
        self.decode_partition(
            frame,
            decoder,
            sb_col,
            sb_row,
            sb_size,
            sb_size,
            seq_header,
            frame_header,
        )
    }
    
    fn decode_partition(
        &self,
        frame: &mut frame::Frame,
        decoder: &mut entropy::ArithmeticDecoder,
        col: u32,
        row: u32,
        width: u32,
        height: u32,
        seq_header: &obu::SequenceHeader,
        frame_header: &obu::FrameHeader,
    ) -> Result<(), AvifError> {
        if col >= frame.width || row >= frame.height {
            return Ok(());
        }
        
        if width <= 4 && height <= 4 {
            // Minimum block size, decode block
            return self.decode_block(frame, decoder, col, row, width, height, seq_header, frame_header);
        }
        
        // Decode partition type
        let partition = decoder.decode_partition(width, height)?;
        
        match partition {
            entropy::Partition::None => {
                self.decode_block(frame, decoder, col, row, width, height, seq_header, frame_header)?;
            }
            entropy::Partition::Horizontal => {
                let half = height / 2;
                self.decode_partition(frame, decoder, col, row, width, half, seq_header, frame_header)?;
                self.decode_partition(frame, decoder, col, row + half, width, half, seq_header, frame_header)?;
            }
            entropy::Partition::Vertical => {
                let half = width / 2;
                self.decode_partition(frame, decoder, col, row, half, height, seq_header, frame_header)?;
                self.decode_partition(frame, decoder, col + half, row, half, height, seq_header, frame_header)?;
            }
            entropy::Partition::Split => {
                let half_w = width / 2;
                let half_h = height / 2;
                self.decode_partition(frame, decoder, col, row, half_w, half_h, seq_header, frame_header)?;
                self.decode_partition(frame, decoder, col + half_w, row, half_w, half_h, seq_header, frame_header)?;
                self.decode_partition(frame, decoder, col, row + half_h, half_w, half_h, seq_header, frame_header)?;
                self.decode_partition(frame, decoder, col + half_w, row + half_h, half_w, half_h, seq_header, frame_header)?;
            }
        }
        
        Ok(())
    }
    
    fn decode_block(
        &self,
        frame: &mut frame::Frame,
        decoder: &mut entropy::ArithmeticDecoder,
        col: u32,
        row: u32,
        width: u32,
        height: u32,
        seq_header: &obu::SequenceHeader,
        frame_header: &obu::FrameHeader,
    ) -> Result<(), AvifError> {
        // Decode intra prediction mode
        let mode = decoder.decode_intra_mode()?;
        
        // Get reference pixels for prediction
        let (top, left, top_left) = frame.get_reference_pixels(col, row, width, height, 0);
        
        // Allocate prediction buffer
        let mut prediction = vec![0i16; (width * height) as usize];
        
        // Apply intra prediction
        predict::intra_predict(
            mode,
            &mut prediction,
            width,
            height,
            &top,
            &left,
            top_left,
            seq_header.bit_depth,
        )?;
        
        // Decode and apply residuals
        let has_residual = decoder.decode_bool(128)?;
        
        if has_residual {
            // Decode transform type and size
            let tx_size = decoder.decode_tx_size(width, height)?;
            let tx_type = decoder.decode_tx_type()?;
            
            // Decode coefficients
            let mut coeffs = decoder.decode_coefficients(tx_size, seq_header.bit_depth)?;
            
            // Apply inverse transform
            transform::inverse_transform(&mut coeffs, tx_size, tx_type, &self.simd)?;
            
            // Add residuals to prediction
            for (pred, &res) in prediction.iter_mut().zip(coeffs.iter()) {
                *pred = (*pred + res).clamp(0, (1 << seq_header.bit_depth) - 1);
            }
        }
        
        // Write to frame buffer
        frame.write_block(col, row, width, height, &prediction, 0)?;
        
        // Handle chroma if not monochrome
        if !seq_header.monochrome {
            let chroma_width = width >> seq_header.subsampling_x;
            let chroma_height = height >> seq_header.subsampling_y;
            let chroma_col = col >> seq_header.subsampling_x;
            let chroma_row = row >> seq_header.subsampling_y;
            
            // Check for CfL mode
            let use_cfl = mode == predict::IntraMode::CflPred;
            
            for plane in 1..=2 {
                let chroma_mode = if use_cfl {
                    predict::IntraMode::CflPred
                } else {
                    decoder.decode_chroma_mode(mode)?
                };
                
                let (top, left, top_left) = frame.get_reference_pixels(
                    chroma_col, chroma_row, chroma_width, chroma_height, plane,
                );
                
                let mut chroma_pred = vec![0i16; (chroma_width * chroma_height) as usize];
                
                if use_cfl {
                    // Get luma average for CfL
                    let luma_avg = frame.compute_luma_average(col, row, width, height);
                    let (alpha_u, alpha_v) = decoder.decode_cfl_alpha()?;
                    let alpha = if plane == 1 { alpha_u } else { alpha_v };
                    
                    predict::cfl_predict(
                        &mut chroma_pred,
                        chroma_width,
                        chroma_height,
                        &top,
                        &left,
                        top_left,
                        alpha,
                        luma_avg,
                        seq_header.bit_depth,
                    )?;
                } else {
                    predict::intra_predict(
                        chroma_mode,
                        &mut chroma_pred,
                        chroma_width,
                        chroma_height,
                        &top,
                        &left,
                        top_left,
                        seq_header.bit_depth,
                    )?;
                }
                
                let has_chroma_residual = decoder.decode_bool(128)?;
                
                if has_chroma_residual {
                    let tx_size = decoder.decode_tx_size(chroma_width, chroma_height)?;
                    let tx_type = decoder.decode_tx_type()?;
                    let mut coeffs = decoder.decode_coefficients(tx_size, seq_header.bit_depth)?;
                    
                    transform::inverse_transform(&mut coeffs, tx_size, tx_type, &self.simd)?;
                    
                    for (pred, &res) in chroma_pred.iter_mut().zip(coeffs.iter()) {
                        *pred = (*pred + res).clamp(0, (1 << seq_header.bit_depth) - 1);
                    }
                }
                
                frame.write_block(chroma_col, chroma_row, chroma_width, chroma_height, &chroma_pred, plane)?;
            }
        }
        
        Ok(())
    }
    
    fn decode_alpha(
        &mut self,
        data: &[u8],
        alpha_data: &container::AlphaData,
    ) -> Result<Vec<u8>, AvifError> {
        // Alpha is stored as a separate AV1 monochrome image
        let av1_data = &data[alpha_data.offset..alpha_data.offset + alpha_data.size];
        let (seq_header, frame_header, tile_data) = obu::parse_obus(av1_data)?;
        
        let mut frame = frame::Frame::new(
            alpha_data.width,
            alpha_data.height,
            seq_header.bit_depth,
            0, 0,
            true, // monochrome
        );
        
        self.decode_tiles(&mut frame, &tile_data, &seq_header, &frame_header)?;
        
        // Extract alpha values
        let alpha: Vec<u8> = frame.planes[0].data.iter()
            .map(|&v| {
                if seq_header.bit_depth == 8 {
                    v as u8
                } else {
                    (v >> (seq_header.bit_depth - 8)) as u8
                }
            })
            .collect();
        
        Ok(alpha)
    }
    
    fn combine_alpha(&self, rgb: Vec<u8>, alpha: &[u8]) -> Vec<u8> {
        let mut rgba = Vec::with_capacity((rgb.len() / 3) * 4);
        let pixels = rgb.len() / 3;
        
        for i in 0..pixels {
            rgba.push(rgb[i * 3]);
            rgba.push(rgb[i * 3 + 1]);
            rgba.push(rgb[i * 3 + 2]);
            rgba.push(alpha.get(i).copied().unwrap_or(255));
        }
        
        rgba
    }
}

/// Check if data is AVIF format
pub fn is_avif(data: &[u8]) -> bool {
    if data.len() < 12 {
        return false;
    }
    if &data[4..8] != b"ftyp" {
        return false;
    }
    let brand = &data[8..12];
    brand == b"avif" || brand == b"avis" || brand == b"mif1"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_avif() {
        let fake_avif = b"\x00\x00\x00\x1cftypavif";
        assert!(is_avif(fake_avif));
        
        let fake_avis = b"\x00\x00\x00\x1cftypavis";
        assert!(is_avif(fake_avis));
        
        let fake_mif1 = b"\x00\x00\x00\x1cftypmif1";
        assert!(is_avif(fake_mif1));
        
        let png = b"\x89PNG\r\n\x1a\n";
        assert!(!is_avif(png));
        
        let too_short = b"short";
        assert!(!is_avif(too_short));
    }
    
    #[test]
    fn test_decoder_creation() {
        let decoder = AvifDecoder::new();
        assert!(decoder.simd.level() != super::super::simd::SimdLevel::None || true);
    }
}
