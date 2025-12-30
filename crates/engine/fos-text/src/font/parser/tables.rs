//! OpenType table definitions

use super::reader::FontReader;
use super::ParseError;

/// Head table (font header)
#[derive(Debug, Clone)]
pub struct HeadTable {
    pub units_per_em: u16,
    pub x_min: i16,
    pub y_min: i16,
    pub x_max: i16,
    pub y_max: i16,
    pub index_to_loc_format: i16,
}

impl HeadTable {
    pub fn parse(data: &[u8]) -> Result<Self, ParseError> {
        let mut r = FontReader::new(data);
        r.skip(18)?; // version, revision, checksums, magic, flags
        let units_per_em = r.read_u16()?;
        r.skip(16)?; // timestamps
        let x_min = r.read_i16()?;
        let y_min = r.read_i16()?;
        let x_max = r.read_i16()?;
        let y_max = r.read_i16()?;
        r.skip(6)?; // mac style, lowest rec ppem, direction hint
        let index_to_loc_format = r.read_i16()?;
        
        Ok(Self {
            units_per_em,
            x_min,
            y_min,
            x_max,
            y_max,
            index_to_loc_format,
        })
    }
}

/// Hhea table (horizontal header)
#[derive(Debug, Clone)]
pub struct HheaTable {
    pub ascender: i16,
    pub descender: i16,
    pub line_gap: i16,
    pub advance_width_max: u16,
    pub number_of_h_metrics: u16,
}

impl HheaTable {
    pub fn parse(data: &[u8]) -> Result<Self, ParseError> {
        let mut r = FontReader::new(data);
        r.skip(4)?; // version
        let ascender = r.read_i16()?;
        let descender = r.read_i16()?;
        let line_gap = r.read_i16()?;
        let advance_width_max = r.read_u16()?;
        r.skip(22)?; // other metrics
        let number_of_h_metrics = r.read_u16()?;
        
        Ok(Self {
            ascender,
            descender,
            line_gap,
            advance_width_max,
            number_of_h_metrics,
        })
    }
}

/// Maxp table (maximum profile)
#[derive(Debug, Clone)]
pub struct MaxpTable {
    pub num_glyphs: u16,
}

impl MaxpTable {
    pub fn parse(data: &[u8]) -> Result<Self, ParseError> {
        let mut r = FontReader::new(data);
        r.skip(4)?; // version
        let num_glyphs = r.read_u16()?;
        Ok(Self { num_glyphs })
    }
}

/// OS/2 table (font metadata)
#[derive(Debug, Clone)]
pub struct Os2Table {
    pub weight_class: u16,
    pub width_class: u16,
    pub fs_type: u16,
    pub y_subscript_x_size: i16,
    pub y_subscript_y_size: i16,
    pub s_typo_ascender: i16,
    pub s_typo_descender: i16,
    pub s_typo_line_gap: i16,
    pub us_win_ascent: u16,
    pub us_win_descent: u16,
}

impl Os2Table {
    pub fn parse(data: &[u8]) -> Result<Self, ParseError> {
        let mut r = FontReader::new(data);
        let _version = r.read_u16()?;
        r.skip(2)?; // avg char width
        let weight_class = r.read_u16()?;
        let width_class = r.read_u16()?;
        let fs_type = r.read_u16()?;
        let y_subscript_x_size = r.read_i16()?;
        let y_subscript_y_size = r.read_i16()?;
        r.skip(58)?; // skip to typo metrics
        let s_typo_ascender = r.read_i16()?;
        let s_typo_descender = r.read_i16()?;
        let s_typo_line_gap = r.read_i16()?;
        let us_win_ascent = r.read_u16()?;
        let us_win_descent = r.read_u16()?;
        
        Ok(Self {
            weight_class,
            width_class,
            fs_type,
            y_subscript_x_size,
            y_subscript_y_size,
            s_typo_ascender,
            s_typo_descender,
            s_typo_line_gap,
            us_win_ascent,
            us_win_descent,
        })
    }
}

/// Name table entry
#[derive(Debug, Clone)]
pub struct NameRecord {
    pub platform_id: u16,
    pub encoding_id: u16,
    pub language_id: u16,
    pub name_id: u16,
    pub name: String,
}

/// Common name IDs
pub mod name_ids {
    pub const COPYRIGHT: u16 = 0;
    pub const FAMILY: u16 = 1;
    pub const SUBFAMILY: u16 = 2;
    pub const UNIQUE_ID: u16 = 3;
    pub const FULL_NAME: u16 = 4;
    pub const VERSION: u16 = 5;
    pub const POSTSCRIPT_NAME: u16 = 6;
    pub const TYPOGRAPHIC_FAMILY: u16 = 16;
    pub const TYPOGRAPHIC_SUBFAMILY: u16 = 17;
}
