//! Character to glyph mapping (cmap table)

use super::reader::FontReader;
use super::GlyphId;

/// Look up glyph ID for a Unicode codepoint
pub fn lookup_glyph(cmap_data: &[u8], codepoint: u32) -> Option<GlyphId> {
    let mut reader = FontReader::new(cmap_data);
    
    let _version = reader.read_u16().ok()?;
    let num_tables = reader.read_u16().ok()?;
    
    // Find best encoding table (prefer Unicode BMP/full)
    let mut best_offset = None;
    let mut best_format = 0u16;
    
    for _ in 0..num_tables {
        let platform_id = reader.read_u16().ok()?;
        let encoding_id = reader.read_u16().ok()?;
        let offset = reader.read_u32().ok()?;
        
        // Prefer Unicode platform (0) or Windows Unicode (3, 1/10)
        let priority = match (platform_id, encoding_id) {
            (0, 4) => 5, // Unicode full repertoire
            (0, 3) => 4, // Unicode BMP
            (3, 10) => 3, // Windows Unicode full
            (3, 1) => 2,  // Windows Unicode BMP
            (0, _) => 1,  // Any Unicode
            _ => 0,
        };
        
        if priority > best_format {
            best_format = priority;
            best_offset = Some(offset);
        }
    }
    
    let table_offset = best_offset? as usize;
    let mut table_reader = FontReader::new(&cmap_data[table_offset..]);
    
    let format = table_reader.read_u16().ok()?;
    
    match format {
        4 => lookup_format4(&cmap_data[table_offset..], codepoint),
        12 => lookup_format12(&cmap_data[table_offset..], codepoint),
        _ => None,
    }
}

/// Format 4: Segment mapping to delta values (BMP only)
fn lookup_format4(data: &[u8], codepoint: u32) -> Option<GlyphId> {
    if codepoint > 0xFFFF {
        return None; // Format 4 only supports BMP
    }
    
    let code = codepoint as u16;
    let mut reader = FontReader::new(data);
    
    let _format = reader.read_u16().ok()?;
    let _length = reader.read_u16().ok()?;
    let _language = reader.read_u16().ok()?;
    let seg_count_x2 = reader.read_u16().ok()?;
    let seg_count = seg_count_x2 / 2;
    
    reader.skip(6).ok()?; // search range, entry selector, range shift
    
    // Read end codes
    let end_codes_start = reader.pos();
    
    // Binary search for segment
    let mut lo = 0u16;
    let mut hi = seg_count;
    
    while lo < hi {
        let mid = (lo + hi) / 2;
        let mut r = FontReader::new(&data[end_codes_start + (mid as usize) * 2..]);
        let end_code = r.read_u16().ok()?;
        
        if end_code < code {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    
    if lo >= seg_count {
        return None;
    }
    
    let seg_idx = lo as usize;
    
    // Read segment data
    let end_codes_offset = end_codes_start;
    let start_codes_offset = end_codes_offset + (seg_count as usize) * 2 + 2; // +2 for reserved
    let deltas_offset = start_codes_offset + (seg_count as usize) * 2;
    let ranges_offset = deltas_offset + (seg_count as usize) * 2;
    
    let mut r = FontReader::new(&data[end_codes_offset + seg_idx * 2..]);
    let end_code = r.read_u16().ok()?;
    
    let mut r = FontReader::new(&data[start_codes_offset + seg_idx * 2..]);
    let start_code = r.read_u16().ok()?;
    
    if code < start_code || code > end_code {
        return None;
    }
    
    let mut r = FontReader::new(&data[deltas_offset + seg_idx * 2..]);
    let id_delta = r.read_i16().ok()?;
    
    let mut r = FontReader::new(&data[ranges_offset + seg_idx * 2..]);
    let id_range_offset = r.read_u16().ok()?;
    
    let glyph_id = if id_range_offset == 0 {
        (code as i32 + id_delta as i32) as u16
    } else {
        let glyph_offset = ranges_offset + seg_idx * 2 + id_range_offset as usize + ((code - start_code) as usize) * 2;
        let mut r = FontReader::new(&data[glyph_offset..]);
        let glyph = r.read_u16().ok()?;
        if glyph == 0 {
            0
        } else {
            (glyph as i32 + id_delta as i32) as u16
        }
    };
    
    if glyph_id == 0 {
        None
    } else {
        Some(GlyphId(glyph_id))
    }
}

/// Format 12: Segmented coverage (full Unicode)
fn lookup_format12(data: &[u8], codepoint: u32) -> Option<GlyphId> {
    let mut reader = FontReader::new(data);
    
    let _format = reader.read_u16().ok()?;
    let _reserved = reader.read_u16().ok()?;
    let _length = reader.read_u32().ok()?;
    let _language = reader.read_u32().ok()?;
    let num_groups = reader.read_u32().ok()?;
    
    // Binary search
    let groups_start = reader.pos();
    let mut lo = 0u32;
    let mut hi = num_groups;
    
    while lo < hi {
        let mid = (lo + hi) / 2;
        let mut r = FontReader::new(&data[groups_start + (mid as usize) * 12..]);
        let start_char = r.read_u32().ok()?;
        let end_char = r.read_u32().ok()?;
        
        if codepoint < start_char {
            hi = mid;
        } else if codepoint > end_char {
            lo = mid + 1;
        } else {
            // Found it
            let start_glyph = r.read_u32().ok()?;
            let glyph_id = start_glyph + (codepoint - start_char);
            return Some(GlyphId(glyph_id as u16));
        }
    }
    
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_glyph_id_default() {
        let g = GlyphId::default();
        assert_eq!(g.0, 0);
    }
}
