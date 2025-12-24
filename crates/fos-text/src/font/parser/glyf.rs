//! Glyph outline parsing (glyf/loca tables)

use super::reader::FontReader;
use super::outline::OutlineBuilder;

/// Get glyph offset from loca table
pub fn get_glyph_offset(loca_data: &[u8], glyph_index: u16, index_format: u16) -> Option<u32> {
    let mut reader = FontReader::new(loca_data);
    
    if index_format == 0 {
        // Short format (u16, multiply by 2)
        reader.skip((glyph_index as usize) * 2).ok()?;
        Some(reader.read_u16().ok()? as u32 * 2)
    } else {
        // Long format (u32)
        reader.skip((glyph_index as usize) * 4).ok()?;
        Some(reader.read_u32().ok()?)
    }
}

/// Outline a glyph
pub fn outline_glyph<B: OutlineBuilder>(
    glyf_data: &[u8],
    loca_data: &[u8],
    glyph_index: u16,
    index_format: u16,
    builder: &mut B,
) -> Option<()> {
    let offset = get_glyph_offset(loca_data, glyph_index, index_format)?;
    let next_offset = get_glyph_offset(loca_data, glyph_index + 1, index_format)?;
    
    if offset == next_offset {
        return Some(()); // Empty glyph
    }
    
    let glyph_data = &glyf_data[offset as usize..next_offset as usize];
    let mut reader = FontReader::new(glyph_data);
    
    let num_contours = reader.read_i16().ok()?;
    let _x_min = reader.read_i16().ok()?;
    let _y_min = reader.read_i16().ok()?;
    let _x_max = reader.read_i16().ok()?;
    let _y_max = reader.read_i16().ok()?;
    
    if num_contours >= 0 {
        outline_simple_glyph(glyph_data, num_contours as u16, builder)
    } else {
        outline_compound_glyph(glyf_data, loca_data, glyph_data, index_format, builder)
    }
}

/// Parse simple glyph outline
fn outline_simple_glyph<B: OutlineBuilder>(
    glyph_data: &[u8],
    num_contours: u16,
    builder: &mut B,
) -> Option<()> {
    if num_contours == 0 {
        return Some(());
    }
    
    let mut reader = FontReader::new(glyph_data);
    reader.skip(10).ok()?; // Skip header
    
    // Read end points of contours
    let mut end_points = Vec::with_capacity(num_contours as usize);
    for _ in 0..num_contours {
        end_points.push(reader.read_u16().ok()?);
    }
    
    let num_points = (end_points.last().copied().unwrap_or(0) + 1) as usize;
    
    // Skip instructions
    let instruction_length = reader.read_u16().ok()? as usize;
    reader.skip(instruction_length).ok()?;
    
    // Read flags
    let mut flags = Vec::with_capacity(num_points);
    while flags.len() < num_points {
        let flag = reader.read_u8().ok()?;
        flags.push(flag);
        
        if flag & 0x08 != 0 {
            // Repeat flag
            let repeat_count = reader.read_u8().ok()? as usize;
            for _ in 0..repeat_count {
                flags.push(flag);
            }
        }
    }
    
    // Read x coordinates
    let mut x_coords = Vec::with_capacity(num_points);
    let mut x = 0i32;
    for &flag in &flags {
        let is_short = flag & 0x02 != 0;
        let is_same_or_positive = flag & 0x10 != 0;
        
        if is_short {
            let dx = reader.read_u8().ok()? as i32;
            x += if is_same_or_positive { dx } else { -dx };
        } else if !is_same_or_positive {
            x += reader.read_i16().ok()? as i32;
        }
        // else: same as previous (x unchanged)
        
        x_coords.push(x);
    }
    
    // Read y coordinates
    let mut y_coords = Vec::with_capacity(num_points);
    let mut y = 0i32;
    for &flag in &flags {
        let is_short = flag & 0x04 != 0;
        let is_same_or_positive = flag & 0x20 != 0;
        
        if is_short {
            let dy = reader.read_u8().ok()? as i32;
            y += if is_same_or_positive { dy } else { -dy };
        } else if !is_same_or_positive {
            y += reader.read_i16().ok()? as i32;
        }
        
        y_coords.push(y);
    }
    
    // Build outline
    let mut point_idx = 0usize;
    for &end_point in &end_points {
        let contour_end = end_point as usize;
        let contour_start = point_idx;
        let contour_len = contour_end - contour_start + 1;
        
        if contour_len < 2 {
            point_idx = contour_end + 1;
            continue;
        }
        
        // Find first on-curve point
        let mut first_on = contour_start;
        for i in 0..contour_len {
            let idx = contour_start + i;
            if flags[idx] & 0x01 != 0 {
                first_on = idx;
                break;
            }
        }
        
        let x0 = x_coords[first_on] as f32;
        let y0 = y_coords[first_on] as f32;
        builder.move_to(x0, y0);
        
        let mut i = (first_on - contour_start + 1) % contour_len;
        let mut prev_on_curve = true;
        let mut _prev_x = x0;
        let mut _prev_y = y0;
        let mut off_x = 0.0f32;
        let mut off_y = 0.0f32;
        
        for _ in 0..contour_len {
            let idx = contour_start + i;
            let on_curve = flags[idx] & 0x01 != 0;
            let x = x_coords[idx] as f32;
            let y = y_coords[idx] as f32;
            
            if on_curve {
                if prev_on_curve {
                    builder.line_to(x, y);
                } else {
                    builder.quad_to(off_x, off_y, x, y);
                }
                    _prev_x = x;
                    _prev_y = y;
            } else {
                if !prev_on_curve {
                    // Two off-curve points: insert implicit on-curve
                    let mid_x = (off_x + x) / 2.0;
                    let mid_y = (off_y + y) / 2.0;
                    builder.quad_to(off_x, off_y, mid_x, mid_y);
                    _prev_x = mid_x;
                    _prev_y = mid_y;
                }
                off_x = x;
                off_y = y;
            }
            
            prev_on_curve = on_curve;
            i = (i + 1) % contour_len;
        }
        
        // Close contour
        if !prev_on_curve {
            builder.quad_to(off_x, off_y, x0, y0);
        }
        builder.close();
        
        point_idx = contour_end + 1;
    }
    
    Some(())
}

/// Parse compound glyph outline
fn outline_compound_glyph<B: OutlineBuilder>(
    glyf_data: &[u8],
    loca_data: &[u8],
    glyph_data: &[u8],
    index_format: u16,
    builder: &mut B,
) -> Option<()> {
    let mut reader = FontReader::new(glyph_data);
    reader.skip(10).ok()?; // Skip header
    
    const ARG_1_AND_2_ARE_WORDS: u16 = 0x0001;
    const ARGS_ARE_XY_VALUES: u16 = 0x0002;
    const WE_HAVE_A_SCALE: u16 = 0x0008;
    const MORE_COMPONENTS: u16 = 0x0020;
    const WE_HAVE_AN_X_AND_Y_SCALE: u16 = 0x0040;
    const WE_HAVE_A_TWO_BY_TWO: u16 = 0x0080;
    
    let mut more = true;
    while more {
        let flags = reader.read_u16().ok()?;
        let glyph_index = reader.read_u16().ok()?;
        
        // Read offset
        let (_dx, _dy) = if flags & ARG_1_AND_2_ARE_WORDS != 0 {
            let arg1 = reader.read_i16().ok()? as f32;
            let arg2 = reader.read_i16().ok()? as f32;
            if flags & ARGS_ARE_XY_VALUES != 0 {
                (arg1, arg2)
            } else {
                (0.0, 0.0) // Point matching - not fully supported
            }
        } else {
            let arg1 = reader.read_u8().ok()? as i8 as f32;
            let arg2 = reader.read_u8().ok()? as i8 as f32;
            if flags & ARGS_ARE_XY_VALUES != 0 {
                (arg1, arg2)
            } else {
                (0.0, 0.0)
            }
        };
        
        // Read transform (simplified - just offset for now)
        if flags & WE_HAVE_A_SCALE != 0 {
            reader.skip(2).ok()?;
        } else if flags & WE_HAVE_AN_X_AND_Y_SCALE != 0 {
            reader.skip(4).ok()?;
        } else if flags & WE_HAVE_A_TWO_BY_TWO != 0 {
            reader.skip(8).ok()?;
        }
        
        // Recursively outline component glyph
        // Note: For proper implementation, we'd need transform support
        outline_glyph(glyf_data, loca_data, glyph_index, index_format, builder)?;
        
        more = flags & MORE_COMPONENTS != 0;
    }
    
    Some(())
}
