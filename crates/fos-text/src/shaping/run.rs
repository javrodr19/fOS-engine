//! Shaped text run

/// A shaped glyph with position
#[derive(Debug, Clone, Copy)]
pub struct ShapedGlyph {
    /// Glyph ID in the font
    pub glyph_id: u16,
    /// X offset from current position (in font units)
    pub x_offset: i32,
    /// Y offset from current position (in font units)
    pub y_offset: i32,
    /// Horizontal advance (in font units)
    pub x_advance: i32,
    /// Vertical advance (in font units)
    pub y_advance: i32,
    /// Cluster index (original character position)
    pub cluster: u32,
}

/// A run of shaped glyphs
#[derive(Debug, Clone)]
pub struct ShapedRun {
    /// The shaped glyphs
    pub glyphs: Vec<ShapedGlyph>,
    /// Font size used for shaping
    pub font_size: f32,
    /// Units per em from the font
    pub units_per_em: u16,
}

impl ShapedRun {
    /// Create a new shaped run
    pub fn new(glyphs: Vec<ShapedGlyph>, font_size: f32, units_per_em: u16) -> Self {
        Self { glyphs, font_size, units_per_em }
    }
    
    /// Scale factor to convert font units to pixels
    pub fn scale(&self) -> f32 {
        self.font_size / self.units_per_em as f32
    }
    
    /// Total width in pixels
    pub fn width(&self) -> f32 {
        self.glyphs.iter()
            .map(|g| g.x_advance as f32 * self.scale())
            .sum()
    }
    
    /// Number of glyphs
    pub fn len(&self) -> usize {
        self.glyphs.len()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.glyphs.is_empty()
    }
    
    /// Iterate over glyphs with pixel positions
    pub fn positioned_glyphs(&self) -> impl Iterator<Item = PositionedGlyph> + '_ {
        let scale = self.scale();
        let mut x = 0.0;
        let mut y = 0.0;
        
        self.glyphs.iter().map(move |g| {
            let pos = PositionedGlyph {
                glyph_id: g.glyph_id,
                x: x + g.x_offset as f32 * scale,
                y: y + g.y_offset as f32 * scale,
                cluster: g.cluster,
            };
            x += g.x_advance as f32 * scale;
            y += g.y_advance as f32 * scale;
            pos
        })
    }
}

/// A glyph with pixel position
#[derive(Debug, Clone, Copy)]
pub struct PositionedGlyph {
    /// Glyph ID in the font
    pub glyph_id: u16,
    /// X position in pixels
    pub x: f32,
    /// Y position in pixels
    pub y: f32,
    /// Cluster index (original character position)
    pub cluster: u32,
}
