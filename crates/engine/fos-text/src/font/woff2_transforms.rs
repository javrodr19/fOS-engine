//! WOFF2 Table Transformations
//!
//! Implements the table-specific transformations used by WOFF2:
//! - glyf: Triplet encoding for point coordinates
//! - loca: Reconstructed from glyf offsets
//! - hmtx: Delta encoding for metrics

use super::parser::reader::FontReader;

/// WOFF2 transform error
#[derive(Debug, Clone)]
pub enum TransformError {
    InvalidData,
    UnexpectedEof,
    InvalidGlyph,
    InvalidInstruction,
    InvalidTriplet,
    UnsupportedTransform,
}

impl std::fmt::Display for TransformError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransformError::InvalidData => write!(f, "Invalid transform data"),
            TransformError::UnexpectedEof => write!(f, "Unexpected end of data"),
            TransformError::InvalidGlyph => write!(f, "Invalid glyph data"),
            TransformError::InvalidInstruction => write!(f, "Invalid instruction"),
            TransformError::InvalidTriplet => write!(f, "Invalid triplet encoding"),
            TransformError::UnsupportedTransform => write!(f, "Unsupported transform"),
        }
    }
}

impl std::error::Error for TransformError {}

pub type TransformResult<T> = Result<T, TransformError>;

// ============================================================================
// Glyf Table Transform
// ============================================================================

/// Glyf table transform header
#[derive(Debug)]
struct GlyfTransformHeader {
    /// Number of glyphs
    num_glyphs: u16,
    /// Offset to nContour stream
    n_contour_offset: u32,
    /// Offset to nPoints stream
    n_points_offset: u32,
    /// Offset to flag stream
    flag_offset: u32,
    /// Offset to glyph stream (instructions, composite data)
    glyph_offset: u32,
    /// Offset to composite stream
    composite_offset: u32,
    /// Offset to bbox bitmap
    bbox_bitmap_offset: u32,
    /// Offset to bbox stream
    bbox_offset: u32,
    /// Offset to instruction stream
    instruction_offset: u32,
}

/// Triplet encoding lookup tables
/// Point delta values encoded as variable-length bytes
const TRIPLET_BYTE_COUNT: [u8; 128] = triplet_byte_count_table();
const TRIPLET_X_BITS: [u8; 128] = triplet_x_bits_table();
const TRIPLET_Y_BITS: [u8; 128] = triplet_y_bits_table();

const fn triplet_byte_count_table() -> [u8; 128] {
    let mut table = [0u8; 128];
    let mut i = 0;
    while i < 128 {
        table[i] = match i {
            0..=9 => 1,
            10..=57 => 2,
            58..=83 => 3,
            84..=119 => 4,
            _ => 5,
        };
        i += 1;
    }
    table
}

const fn triplet_x_bits_table() -> [u8; 128] {
    let mut table = [0u8; 128];
    let mut i = 0;
    while i < 128 {
        table[i] = match i {
            0..=3 => 0,
            4..=9 => 0,
            10..=19 => 8,
            20..=35 => 0,
            36..=57 => 8,
            58..=67 => 8,
            68..=83 => 16,
            84..=95 => 8,
            96..=119 => 16,
            _ => 16,
        };
        i += 1;
    }
    table
}

const fn triplet_y_bits_table() -> [u8; 128] {
    let mut table = [0u8; 128];
    let mut i = 0;
    while i < 128 {
        table[i] = match i {
            0..=3 => 0,
            4..=9 => 4,
            10..=19 => 0,
            20..=35 => 8,
            36..=57 => 8,
            58..=67 => 16,
            68..=83 => 8,
            84..=95 => 16,
            96..=119 => 16,
            _ => 16,
        };
        i += 1;
    }
    table
}

/// Triplet decoder for glyf coordinates
struct TripletDecoder<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> TripletDecoder<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// Decode a single triplet, returning (dx, dy, on_curve)
    fn decode_triplet(&mut self, flag: u8) -> TransformResult<(i16, i16, bool)> {
        let flag_byte = flag & 0x7F;
        let on_curve = (flag & 0x80) == 0;

        if flag_byte as usize >= TRIPLET_BYTE_COUNT.len() {
            return Err(TransformError::InvalidTriplet);
        }

        let byte_count = TRIPLET_BYTE_COUNT[flag_byte as usize];
        let x_bits = TRIPLET_X_BITS[flag_byte as usize];
        let y_bits = TRIPLET_Y_BITS[flag_byte as usize];

        if self.pos + byte_count as usize > self.data.len() {
            return Err(TransformError::UnexpectedEof);
        }

        let (dx, dy) = self.decode_delta(flag_byte, x_bits, y_bits)?;

        Ok((dx, dy, on_curve))
    }

    fn decode_delta(&mut self, flag: u8, x_bits: u8, y_bits: u8) -> TransformResult<(i16, i16)> {
        // Decode based on the flag patterns
        let (dx, dy) = match flag {
            // One-byte deltas (dx=0 or dy=0 with small values)
            0..=3 => {
                // Both zero
                (0, 0)
            }
            4..=9 => {
                // small y delta only
                let y = self.read_delta(4, (flag - 4) & 1 != 0)?;
                (0, y)
            }
            10..=19 => {
                // x only, 8 bits
                let x = self.read_u8()? as i16;
                let sign = (flag - 10) & 1 != 0;
                (if sign { -x } else { x }, 0)
            }
            20..=35 => {
                // y only, 8 bits
                let y = self.read_u8()? as i16;
                let sign = (flag - 20) & 1 != 0;
                (0, if sign { -y } else { y })
            }
            36..=57 => {
                // x 8 bits, y 8 bits, various signs
                let x = self.read_u8()? as i16;
                let y = self.read_u8()? as i16;
                let idx = flag - 36;
                let x_sign = (idx >> 1) & 1 != 0;
                let y_sign = idx & 1 != 0;
                (if x_sign { -x } else { x }, if y_sign { -y } else { y })
            }
            58..=67 => {
                // x 8 bits, y 16 bits
                let x = self.read_u8()? as i16;
                let y = self.read_u16()? as i16;
                let idx = flag - 58;
                let x_sign = (idx >> 1) & 1 != 0;
                let y_sign = idx & 1 != 0;
                (if x_sign { -x } else { x }, if y_sign { -y } else { y })
            }
            68..=83 => {
                // x 16 bits, y 8 bits
                let x = self.read_u16()? as i16;
                let y = self.read_u8()? as i16;
                let idx = flag - 68;
                let x_sign = (idx >> 1) & 1 != 0;
                let y_sign = idx & 1 != 0;
                (if x_sign { -x } else { x }, if y_sign { -y } else { y })
            }
            84..=119 => {
                // x and y with various bit widths
                let x = self.read_u8()? as i16;
                let y = self.read_u16()? as i16;
                let idx = flag - 84;
                let x_sign = (idx >> 1) & 1 != 0;
                let y_sign = idx & 1 != 0;
                (if x_sign { -x } else { x }, if y_sign { -y } else { y })
            }
            _ => {
                // Full 16-bit coordinates
                let x = self.read_i16()?;
                let y = self.read_i16()?;
                (x, y)
            }
        };

        Ok((dx, dy))
    }

    fn read_delta(&mut self, bits: u8, negative: bool) -> TransformResult<i16> {
        let val = match bits {
            4 => (self.read_u8()? & 0x0F) as i16,
            8 => self.read_u8()? as i16,
            16 => self.read_u16()? as i16,
            _ => 0,
        };
        Ok(if negative { -val } else { val })
    }

    fn read_u8(&mut self) -> TransformResult<u8> {
        if self.pos >= self.data.len() {
            return Err(TransformError::UnexpectedEof);
        }
        let v = self.data[self.pos];
        self.pos += 1;
        Ok(v)
    }

    fn read_u16(&mut self) -> TransformResult<u16> {
        if self.pos + 2 > self.data.len() {
            return Err(TransformError::UnexpectedEof);
        }
        let v = u16::from_be_bytes([self.data[self.pos], self.data[self.pos + 1]]);
        self.pos += 2;
        Ok(v)
    }

    fn read_i16(&mut self) -> TransformResult<i16> {
        Ok(self.read_u16()? as i16)
    }
}

/// Reconstruct glyf table from WOFF2 transformed data
pub fn reconstruct_glyf(
    transform_data: &[u8],
    num_glyphs: u16,
) -> TransformResult<(Vec<u8>, Vec<u32>)> {
    if transform_data.is_empty() {
        return Ok((Vec::new(), vec![0; num_glyphs as usize + 1]));
    }

    let mut reader = FontReader::new(transform_data);

    // Read stream offsets
    let _version = reader.read_u16().map_err(|_| TransformError::InvalidData)?;
    let _options = reader.read_u16().map_err(|_| TransformError::InvalidData)?;
    let n_contours_len = reader.read_u32().map_err(|_| TransformError::InvalidData)?;
    let n_points_len = reader.read_u32().map_err(|_| TransformError::InvalidData)?;
    let flag_len = reader.read_u32().map_err(|_| TransformError::InvalidData)?;
    let glyph_len = reader.read_u32().map_err(|_| TransformError::InvalidData)?;
    let composite_len = reader.read_u32().map_err(|_| TransformError::InvalidData)?;
    let bbox_bitmap_len = reader.read_u32().map_err(|_| TransformError::InvalidData)?;
    let bbox_len = reader.read_u32().map_err(|_| TransformError::InvalidData)?;
    let instruction_len = reader.read_u32().map_err(|_| TransformError::InvalidData)?;

    let header_size = reader.pos();
    
    // Calculate stream positions
    let n_contours_pos = header_size;
    let n_points_pos = n_contours_pos + n_contours_len as usize;
    let flag_pos = n_points_pos + n_points_len as usize;
    let glyph_pos = flag_pos + flag_len as usize;
    let composite_pos = glyph_pos + glyph_len as usize;
    let bbox_bitmap_pos = composite_pos + composite_len as usize;
    let bbox_pos = bbox_bitmap_pos + bbox_bitmap_len as usize;
    let instruction_pos = bbox_pos + bbox_len as usize;

    // Get stream slices
    let n_contours_data = &transform_data[n_contours_pos..n_points_pos.min(transform_data.len())];
    let n_points_data = &transform_data[n_points_pos..flag_pos.min(transform_data.len())];
    let flag_data = &transform_data[flag_pos..glyph_pos.min(transform_data.len())];
    let glyph_data = &transform_data[glyph_pos..composite_pos.min(transform_data.len())];
    let composite_data = &transform_data[composite_pos..bbox_bitmap_pos.min(transform_data.len())];
    let bbox_bitmap = &transform_data[bbox_bitmap_pos..bbox_pos.min(transform_data.len())];
    let bbox_data = &transform_data[bbox_pos..instruction_pos.min(transform_data.len())];
    let instruction_data = &transform_data[instruction_pos..];

    // Output buffers
    let mut glyf_output = Vec::new();
    let mut loca_offsets = Vec::with_capacity(num_glyphs as usize + 1);

    // Stream readers
    let mut n_contours_reader = StreamReader::new(n_contours_data);
    let mut n_points_reader = StreamReader::new(n_points_data);
    let mut flag_reader = StreamReader::new(flag_data);
    let mut triplet_decoder = TripletDecoder::new(glyph_data);
    let mut composite_reader = StreamReader::new(composite_data);
    let mut bbox_reader = StreamReader::new(bbox_data);
    let mut instruction_reader = StreamReader::new(instruction_data);

    // Process each glyph
    for glyph_idx in 0..num_glyphs {
        loca_offsets.push(glyf_output.len() as u32);

        let n_contours = n_contours_reader.read_i16()?;

        if n_contours == 0 {
            // Empty glyph
            continue;
        }

        // Check if bbox is explicitly stored
        let bbox_explicit = if glyph_idx as usize / 8 < bbox_bitmap.len() {
            (bbox_bitmap[glyph_idx as usize / 8] >> (7 - glyph_idx % 8)) & 1 != 0
        } else {
            false
        };

        if n_contours > 0 {
            // Simple glyph
            let glyph_data = reconstruct_simple_glyph(
                n_contours as u16,
                &mut n_points_reader,
                &mut flag_reader,
                &mut triplet_decoder,
                &mut instruction_reader,
                &mut bbox_reader,
                bbox_explicit,
            )?;
            glyf_output.extend_from_slice(&glyph_data);
        } else {
            // Composite glyph
            let glyph_data = reconstruct_composite_glyph(
                &mut composite_reader,
                &mut instruction_reader,
                &mut bbox_reader,
                bbox_explicit,
            )?;
            glyf_output.extend_from_slice(&glyph_data);
        }

        // Align to 4 bytes
        while glyf_output.len() % 4 != 0 {
            glyf_output.push(0);
        }
    }

    loca_offsets.push(glyf_output.len() as u32);

    Ok((glyf_output, loca_offsets))
}

/// Simple stream reader helper
struct StreamReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> StreamReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn read_u8(&mut self) -> TransformResult<u8> {
        if self.pos >= self.data.len() {
            return Err(TransformError::UnexpectedEof);
        }
        let v = self.data[self.pos];
        self.pos += 1;
        Ok(v)
    }

    fn read_i16(&mut self) -> TransformResult<i16> {
        if self.pos + 2 > self.data.len() {
            return Err(TransformError::UnexpectedEof);
        }
        let v = i16::from_be_bytes([self.data[self.pos], self.data[self.pos + 1]]);
        self.pos += 2;
        Ok(v)
    }

    fn read_u16(&mut self) -> TransformResult<u16> {
        if self.pos + 2 > self.data.len() {
            return Err(TransformError::UnexpectedEof);
        }
        let v = u16::from_be_bytes([self.data[self.pos], self.data[self.pos + 1]]);
        self.pos += 2;
        Ok(v)
    }

    fn read_255u16(&mut self) -> TransformResult<u16> {
        // WOFF2's variable-length u16 encoding
        let first = self.read_u8()? as u16;
        if first < 253 {
            Ok(first)
        } else if first == 253 {
            let second = self.read_u8()? as u16;
            Ok(253 + second)
        } else if first == 254 {
            let hi = self.read_u8()? as u16;
            let lo = self.read_u8()? as u16;
            Ok(253 + 256 + (hi << 8) + lo)
        } else {
            let hi = self.read_u8()? as u16;
            let lo = self.read_u8()? as u16;
            Ok((hi << 8) + lo)
        }
    }

    fn read_bytes(&mut self, n: usize) -> TransformResult<&'a [u8]> {
        if self.pos + n > self.data.len() {
            return Err(TransformError::UnexpectedEof);
        }
        let slice = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }
}

/// Reconstruct a simple glyph
fn reconstruct_simple_glyph(
    n_contours: u16,
    n_points_reader: &mut StreamReader,
    flag_reader: &mut StreamReader,
    triplet_decoder: &mut TripletDecoder,
    instruction_reader: &mut StreamReader,
    bbox_reader: &mut StreamReader,
    bbox_explicit: bool,
) -> TransformResult<Vec<u8>> {
    let mut output = Vec::new();

    // Read number of points per contour
    let mut total_points = 0u16;
    let mut end_pts = Vec::with_capacity(n_contours as usize);
    for _ in 0..n_contours {
        let n_pts = n_points_reader.read_255u16()?;
        total_points += n_pts;
        end_pts.push(total_points - 1);
    }

    // Read flags for all points
    let mut flags = Vec::with_capacity(total_points as usize);
    for _ in 0..total_points {
        flags.push(flag_reader.read_u8()?);
    }

    // Decode triplets to get coordinates
    let mut x_coords = Vec::with_capacity(total_points as usize);
    let mut y_coords = Vec::with_capacity(total_points as usize);
    let mut on_curve = Vec::with_capacity(total_points as usize);

    let mut x = 0i32;
    let mut y = 0i32;

    for &flag in &flags {
        let (dx, dy, on) = triplet_decoder.decode_triplet(flag)?;
        x += dx as i32;
        y += dy as i32;
        x_coords.push(x as i16);
        y_coords.push(y as i16);
        on_curve.push(on);
    }

    // Calculate or read bounding box
    let (x_min, y_min, x_max, y_max) = if bbox_explicit {
        (
            bbox_reader.read_i16()?,
            bbox_reader.read_i16()?,
            bbox_reader.read_i16()?,
            bbox_reader.read_i16()?,
        )
    } else if total_points > 0 {
        let x_min = *x_coords.iter().min().unwrap_or(&0);
        let y_min = *y_coords.iter().min().unwrap_or(&0);
        let x_max = *x_coords.iter().max().unwrap_or(&0);
        let y_max = *y_coords.iter().max().unwrap_or(&0);
        (x_min, y_min, x_max, y_max)
    } else {
        (0, 0, 0, 0)
    };

    // Write glyph header
    output.extend_from_slice(&(n_contours as i16).to_be_bytes());
    output.extend_from_slice(&x_min.to_be_bytes());
    output.extend_from_slice(&y_min.to_be_bytes());
    output.extend_from_slice(&x_max.to_be_bytes());
    output.extend_from_slice(&y_max.to_be_bytes());

    // Write endPtsOfContours
    for end_pt in &end_pts {
        output.extend_from_slice(&end_pt.to_be_bytes());
    }

    // Read and write instructions
    let instruction_len = instruction_reader.read_255u16()?;
    output.extend_from_slice(&instruction_len.to_be_bytes());
    if instruction_len > 0 {
        let instructions = instruction_reader.read_bytes(instruction_len as usize)?;
        output.extend_from_slice(instructions);
    }

    // Write flags (convert from triplet flags to OpenType flags)
    let mut prev_x = 0i16;
    let mut prev_y = 0i16;
    let mut ot_flags = Vec::with_capacity(total_points as usize);

    for i in 0..total_points as usize {
        let mut flag: u8 = 0;
        
        // On-curve flag
        if on_curve[i] {
            flag |= 0x01;
        }

        let dx = x_coords[i] - prev_x;
        let dy = y_coords[i] - prev_y;

        // X coordinate encoding
        if dx == 0 {
            flag |= 0x10; // x-Same
        } else if dx >= -255 && dx <= 255 {
            flag |= 0x02; // x-Short
            if dx > 0 {
                flag |= 0x10; // positive
            }
        }

        // Y coordinate encoding
        if dy == 0 {
            flag |= 0x20; // y-Same
        } else if dy >= -255 && dy <= 255 {
            flag |= 0x04; // y-Short
            if dy > 0 {
                flag |= 0x20; // positive
            }
        }

        ot_flags.push(flag);
        prev_x = x_coords[i];
        prev_y = y_coords[i];
    }

    // Write flags with RLE
    let mut i = 0;
    while i < ot_flags.len() {
        let flag = ot_flags[i];
        let mut repeat = 0u8;
        while i + 1 + (repeat as usize) < ot_flags.len() 
            && repeat < 255 
            && ot_flags[i + 1 + (repeat as usize)] == flag 
        {
            repeat += 1;
        }

        if repeat > 0 {
            output.push(flag | 0x08);
            output.push(repeat);
            i += 1 + repeat as usize;
        } else {
            output.push(flag);
            i += 1;
        }
    }

    // Write x coordinates
    prev_x = 0;
    for i in 0..total_points as usize {
        let dx = x_coords[i] - prev_x;
        if dx != 0 {
            if dx >= -255 && dx <= 255 {
                output.push(dx.abs() as u8);
            } else {
                output.extend_from_slice(&dx.to_be_bytes());
            }
        }
        prev_x = x_coords[i];
    }

    // Write y coordinates
    prev_y = 0;
    for i in 0..total_points as usize {
        let dy = y_coords[i] - prev_y;
        if dy != 0 {
            if dy >= -255 && dy <= 255 {
                output.push(dy.abs() as u8);
            } else {
                output.extend_from_slice(&dy.to_be_bytes());
            }
        }
        prev_y = y_coords[i];
    }

    Ok(output)
}

/// Reconstruct a composite glyph
fn reconstruct_composite_glyph(
    composite_reader: &mut StreamReader,
    instruction_reader: &mut StreamReader,
    bbox_reader: &mut StreamReader,
    bbox_explicit: bool,
) -> TransformResult<Vec<u8>> {
    let mut output = Vec::new();

    // Read bounding box if explicit
    let (x_min, y_min, x_max, y_max) = if bbox_explicit {
        (
            bbox_reader.read_i16()?,
            bbox_reader.read_i16()?,
            bbox_reader.read_i16()?,
            bbox_reader.read_i16()?,
        )
    } else {
        (0, 0, 0, 0) // Will be calculated by renderer
    };

    // Write glyph header (n_contours = -1 for composite)
    output.extend_from_slice(&(-1i16).to_be_bytes());
    output.extend_from_slice(&x_min.to_be_bytes());
    output.extend_from_slice(&y_min.to_be_bytes());
    output.extend_from_slice(&x_max.to_be_bytes());
    output.extend_from_slice(&y_max.to_be_bytes());

    // Copy composite data directly (already in OpenType format)
    let mut has_instructions = false;
    loop {
        let flags = composite_reader.read_u16()?;
        let glyph_index = composite_reader.read_u16()?;

        output.extend_from_slice(&flags.to_be_bytes());
        output.extend_from_slice(&glyph_index.to_be_bytes());

        // Read arguments based on flags
        const ARG_1_AND_2_ARE_WORDS: u16 = 0x0001;
        const WE_HAVE_A_SCALE: u16 = 0x0008;
        const MORE_COMPONENTS: u16 = 0x0020;
        const WE_HAVE_AN_X_AND_Y_SCALE: u16 = 0x0040;
        const WE_HAVE_A_TWO_BY_TWO: u16 = 0x0080;
        const WE_HAVE_INSTRUCTIONS: u16 = 0x0100;

        let arg_size = if flags & ARG_1_AND_2_ARE_WORDS != 0 { 4 } else { 2 };
        let args = composite_reader.read_bytes(arg_size)?;
        output.extend_from_slice(args);

        // Read transform data
        let transform_size = if flags & WE_HAVE_A_TWO_BY_TWO != 0 {
            8
        } else if flags & WE_HAVE_AN_X_AND_Y_SCALE != 0 {
            4
        } else if flags & WE_HAVE_A_SCALE != 0 {
            2
        } else {
            0
        };
        if transform_size > 0 {
            let transform = composite_reader.read_bytes(transform_size)?;
            output.extend_from_slice(transform);
        }

        if flags & WE_HAVE_INSTRUCTIONS != 0 {
            has_instructions = true;
        }

        if flags & MORE_COMPONENTS == 0 {
            break;
        }
    }

    // Write instructions if present
    if has_instructions {
        let instruction_len = instruction_reader.read_255u16()?;
        output.extend_from_slice(&instruction_len.to_be_bytes());
        if instruction_len > 0 {
            let instructions = instruction_reader.read_bytes(instruction_len as usize)?;
            output.extend_from_slice(instructions);
        }
    }

    Ok(output)
}

// ============================================================================
// Loca Table Reconstruction
// ============================================================================

/// Generate loca table from glyf offsets
pub fn generate_loca(offsets: &[u32], use_short: bool) -> Vec<u8> {
    let mut output = Vec::new();

    if use_short {
        // Short format: offsets / 2 as u16
        for &offset in offsets {
            output.extend_from_slice(&((offset / 2) as u16).to_be_bytes());
        }
    } else {
        // Long format: u32 offsets
        for &offset in offsets {
            output.extend_from_slice(&offset.to_be_bytes());
        }
    }

    output
}

/// Determine if short loca format can be used
pub fn can_use_short_loca(offsets: &[u32]) -> bool {
    offsets.iter().all(|&o| o <= 0x1FFFE && o % 2 == 0)
}

// ============================================================================
// Hmtx Table Transform
// ============================================================================

/// Reconstruct hmtx table from WOFF2 transformed data
pub fn reconstruct_hmtx(
    transform_data: &[u8],
    num_glyphs: u16,
    num_h_metrics: u16,
) -> TransformResult<Vec<u8>> {
    if transform_data.is_empty() {
        return Err(TransformError::InvalidData);
    }

    let mut reader = StreamReader::new(transform_data);
    let mut output = Vec::new();

    // Read flags
    let flags = reader.read_u8()?;
    let has_proportional_lsb = (flags & 0x01) != 0;
    let has_monospace_lsb = (flags & 0x02) != 0;

    // Read advance widths (delta encoded)
    let mut advance = 0u16;
    for _ in 0..num_h_metrics {
        let delta = reader.read_i16()? as i32;
        advance = (advance as i32 + delta) as u16;
        output.extend_from_slice(&advance.to_be_bytes());

        // Placeholder for LSB (will be filled below)
        if has_proportional_lsb {
            let lsb = reader.read_i16()?;
            output.extend_from_slice(&lsb.to_be_bytes());
        } else {
            output.extend_from_slice(&0i16.to_be_bytes());
        }
    }

    // Additional LSBs for monospace section
    if has_monospace_lsb {
        for _ in num_h_metrics..num_glyphs {
            let lsb = reader.read_i16()?;
            output.extend_from_slice(&lsb.to_be_bytes());
        }
    } else {
        for _ in num_h_metrics..num_glyphs {
            output.extend_from_slice(&0i16.to_be_bytes());
        }
    }

    Ok(output)
}

// ============================================================================
// Transform Nullification (pass-through tables)
// ============================================================================

/// Tables that require transformation in WOFF2
pub const TRANSFORM_TABLES: &[[u8; 4]] = &[
    *b"glyf",
    *b"loca",
    *b"hmtx",
];

/// Check if a table requires transformation
pub fn requires_transform(tag: &[u8; 4]) -> bool {
    TRANSFORM_TABLES.contains(tag)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_triplet_tables() {
        // Verify lookup table consistency
        assert_eq!(TRIPLET_BYTE_COUNT[0], 1);
        assert_eq!(TRIPLET_BYTE_COUNT[10], 2);
        assert_eq!(TRIPLET_BYTE_COUNT[58], 3);
        assert_eq!(TRIPLET_BYTE_COUNT[84], 4);
        assert_eq!(TRIPLET_BYTE_COUNT[120], 5);
    }

    #[test]
    fn test_stream_reader_255u16() {
        // Test 255UInt16 encoding
        let data = [100, 253, 50, 254, 0x01, 0x00, 255, 0x10, 0x00];
        let mut reader = StreamReader::new(&data);

        assert_eq!(reader.read_255u16().unwrap(), 100);
        assert_eq!(reader.read_255u16().unwrap(), 253 + 50);
        assert_eq!(reader.read_255u16().unwrap(), 253 + 256 + 256);
        assert_eq!(reader.read_255u16().unwrap(), 0x1000);
    }

    #[test]
    fn test_generate_loca_short() {
        let offsets = vec![0, 100, 200, 400];
        let loca = generate_loca(&offsets, true);
        assert_eq!(loca.len(), 8);
        assert_eq!(&loca[0..2], &[0, 0]);
        assert_eq!(&loca[2..4], &[0, 50]);
    }

    #[test]
    fn test_generate_loca_long() {
        let offsets = vec![0, 100, 200, 400];
        let loca = generate_loca(&offsets, false);
        assert_eq!(loca.len(), 16);
    }

    #[test]
    fn test_can_use_short_loca() {
        assert!(can_use_short_loca(&[0, 100, 200]));
        assert!(!can_use_short_loca(&[0, 100, 0x20000])); // Too large
        assert!(!can_use_short_loca(&[0, 101, 200])); // Odd offset
    }

    #[test]
    fn test_requires_transform() {
        assert!(requires_transform(b"glyf"));
        assert!(requires_transform(b"loca"));
        assert!(requires_transform(b"hmtx"));
        assert!(!requires_transform(b"head"));
        assert!(!requires_transform(b"cmap"));
    }
}
