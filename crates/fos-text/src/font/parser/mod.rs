//! Custom Font Parser
//!
//! From-scratch OpenType/TrueType font parser using Arena allocation.
//! Replaces ttf-parser dependency for zero-dependency font handling.

pub mod reader;
mod tables;
mod outline;
mod cmap;
mod glyf;

pub use reader::FontReader;
pub use outline::{OutlineBuilder, GlyphOutline};


/// Glyph identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct GlyphId(pub u16);

/// Font parsing error
#[derive(Debug, Clone)]
pub enum ParseError {
    InvalidMagic,
    TableNotFound(&'static str),
    InvalidData,
    UnsupportedFormat,
}

/// Parsed font with access to tables
pub struct FontParser<'a> {
    data: &'a [u8],
    /// Table directory
    tables: Vec<TableRecord>,
    /// Font units per em
    units_per_em: u16,
    /// Number of glyphs
    num_glyphs: u16,
    /// Glyph index format (0 = short, 1 = long)
    index_format: u16,
    /// Ascender
    ascender: i16,
    /// Descender
    descender: i16,
    /// Line gap
    line_gap: i16,
}

/// Table directory record
#[derive(Debug, Clone, Copy)]
struct TableRecord {
    tag: [u8; 4],
    checksum: u32,
    offset: u32,
    length: u32,
}

impl<'a> FontParser<'a> {
    /// Parse font from raw data
    pub fn parse(data: &'a [u8]) -> Result<Self, ParseError> {
        Self::parse_index(data, 0)
    }
    
    /// Parse font with face index (for TTC)
    pub fn parse_index(data: &'a [u8], index: u32) -> Result<Self, ParseError> {
        let mut reader = FontReader::new(data);
        
        // Check for TTC (TrueType Collection)
        let magic = reader.read_u32()?;
        let (offset, _num_fonts) = if magic == 0x74746366 { // 'ttcf'
            // TTC header
            let _version = reader.read_u32()?;
            let num_fonts = reader.read_u32()?;
            if index >= num_fonts {
                return Err(ParseError::InvalidData);
            }
            reader.skip((index * 4) as usize)?;
            let offset = reader.read_u32()?;
            (offset as usize, num_fonts)
        } else {
            (0, 1)
        };
        
        // Parse offset table
        reader.set_pos(offset);
        let sfnt_version = reader.read_u32()?;
        
        // Validate magic
        match sfnt_version {
            0x00010000 | // TrueType
            0x4F54544F   // 'OTTO' OpenType with CFF
            => {}
            _ => return Err(ParseError::InvalidMagic),
        }
        
        let num_tables = reader.read_u16()?;
        let _search_range = reader.read_u16()?;
        let _entry_selector = reader.read_u16()?;
        let _range_shift = reader.read_u16()?;
        
        // Parse table records
        let mut tables = Vec::with_capacity(num_tables as usize);
        for _ in 0..num_tables {
            let tag = reader.read_tag()?;
            let checksum = reader.read_u32()?;
            let table_offset = reader.read_u32()?;
            let length = reader.read_u32()?;
            tables.push(TableRecord {
                tag,
                checksum,
                offset: table_offset,
                length,
            });
        }
        
        // Parse required tables
        let head = Self::find_table(&tables, b"head")
            .ok_or(ParseError::TableNotFound("head"))?;
        let mut head_reader = FontReader::new(&data[head.offset as usize..]);
        let _version = head_reader.read_u32()?;
        let _font_revision = head_reader.read_u32()?;
        let _checksum_adjust = head_reader.read_u32()?;
        let _magic = head_reader.read_u32()?;
        let _flags = head_reader.read_u16()?;
        let units_per_em = head_reader.read_u16()?;
        head_reader.skip(16)?; // created, modified timestamps
        let _x_min = head_reader.read_i16()?;
        let _y_min = head_reader.read_i16()?;
        let _x_max = head_reader.read_i16()?;
        let _y_max = head_reader.read_i16()?;
        let _mac_style = head_reader.read_u16()?;
        let _lowest_rec_ppem = head_reader.read_u16()?;
        let _direction_hint = head_reader.read_i16()?;
        let index_format = head_reader.read_i16()? as u16;
        
        // Parse maxp
        let maxp = Self::find_table(&tables, b"maxp")
            .ok_or(ParseError::TableNotFound("maxp"))?;
        let mut maxp_reader = FontReader::new(&data[maxp.offset as usize..]);
        let _version = maxp_reader.read_u32()?;
        let num_glyphs = maxp_reader.read_u16()?;
        
        // Parse hhea
        let hhea = Self::find_table(&tables, b"hhea")
            .ok_or(ParseError::TableNotFound("hhea"))?;
        let mut hhea_reader = FontReader::new(&data[hhea.offset as usize..]);
        let _version = hhea_reader.read_u32()?;
        let ascender = hhea_reader.read_i16()?;
        let descender = hhea_reader.read_i16()?;
        let line_gap = hhea_reader.read_i16()?;
        
        Ok(Self {
            data,
            tables,
            units_per_em,
            num_glyphs,
            index_format,
            ascender,
            descender,
            line_gap,
        })
    }
    
    fn find_table(tables: &[TableRecord], tag: &[u8; 4]) -> Option<TableRecord> {
        tables.iter().find(|t| &t.tag == tag).copied()
    }
    
    /// Get raw table data
    pub fn table_data(&self, tag: &[u8; 4]) -> Option<&'a [u8]> {
        Self::find_table(&self.tables, tag).map(|t| {
            &self.data[t.offset as usize..(t.offset + t.length) as usize]
        })
    }
    
    /// Units per em
    pub fn units_per_em(&self) -> u16 {
        self.units_per_em
    }
    
    /// Number of glyphs
    pub fn number_of_glyphs(&self) -> u16 {
        self.num_glyphs
    }
    
    /// Ascender
    pub fn ascender(&self) -> i16 {
        self.ascender
    }
    
    /// Descender
    pub fn descender(&self) -> i16 {
        self.descender
    }
    
    /// Line gap
    pub fn line_gap(&self) -> i16 {
        self.line_gap
    }
    
    /// Get glyph ID for character
    pub fn glyph_index(&self, c: char) -> Option<GlyphId> {
        let cmap_data = self.table_data(b"cmap")?;
        cmap::lookup_glyph(cmap_data, c as u32)
    }
    
    /// Get glyph horizontal advance
    pub fn glyph_hor_advance(&self, glyph_id: GlyphId) -> Option<u16> {
        let hmtx = self.table_data(b"hmtx")?;
        let hhea = Self::find_table(&self.tables, b"hhea")?;
        let mut hhea_reader = FontReader::new(&self.data[hhea.offset as usize..]);
        hhea_reader.skip(34).ok()?;
        let num_h_metrics = hhea_reader.read_u16().ok()?;
        
        let mut reader = FontReader::new(hmtx);
        if glyph_id.0 < num_h_metrics {
            reader.skip((glyph_id.0 as usize) * 4).ok()?;
            Some(reader.read_u16().ok()?)
        } else {
            // Use last advance width
            reader.skip((num_h_metrics.saturating_sub(1) as usize) * 4).ok()?;
            Some(reader.read_u16().ok()?)
        }
    }
    
    /// Get glyph bounding box
    pub fn glyph_bounding_box(&self, glyph_id: GlyphId) -> Option<BoundingBox> {
        let glyf_data = self.table_data(b"glyf")?;
        let loca_data = self.table_data(b"loca")?;
        
        let offset = glyf::get_glyph_offset(loca_data, glyph_id.0, self.index_format)?;
        let next_offset = glyf::get_glyph_offset(loca_data, glyph_id.0 + 1, self.index_format)?;
        
        if offset == next_offset {
            // Empty glyph (space, etc.)
            return None;
        }
        
        let mut reader = FontReader::new(&glyf_data[offset as usize..]);
        let _num_contours = reader.read_i16().ok()?;
        let x_min = reader.read_i16().ok()?;
        let y_min = reader.read_i16().ok()?;
        let x_max = reader.read_i16().ok()?;
        let y_max = reader.read_i16().ok()?;
        
        Some(BoundingBox { x_min, y_min, x_max, y_max })
    }
    
    /// Outline a glyph
    pub fn outline_glyph<B: OutlineBuilder>(&self, glyph_id: GlyphId, builder: &mut B) -> Option<()> {
        let glyf_data = self.table_data(b"glyf")?;
        let loca_data = self.table_data(b"loca")?;
        
        glyf::outline_glyph(glyf_data, loca_data, glyph_id.0, self.index_format, builder)
    }
}

/// Bounding box
#[derive(Debug, Clone, Copy)]
pub struct BoundingBox {
    pub x_min: i16,
    pub y_min: i16,
    pub x_max: i16,
    pub y_max: i16,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_glyph_id() {
        let g = GlyphId(42);
        assert_eq!(g.0, 42);
    }
}
