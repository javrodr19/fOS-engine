//! Math Layout (Phase 3.1)
//!
//! MathML and mathematical formula layout support.
//! Handles fractions, radicals, scripts, limits, and matrix layouts.

// ============================================================================
// Math Element Types
// ============================================================================

/// Type of math element
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MathElementType {
    /// Number (mn)
    Number,
    /// Identifier/variable (mi)
    Identifier,
    /// Operator (mo)
    Operator,
    /// Text (mtext)
    Text,
    /// Row of elements (mrow)
    Row,
    /// Fraction (mfrac)
    Fraction,
    /// Square root (msqrt)
    SquareRoot,
    /// Nth root (mroot)
    NthRoot,
    /// Superscript (msup)
    Superscript,
    /// Subscript (msub)
    Subscript,
    /// Sub+superscript (msubsup)
    SubSuperscript,
    /// Underscript (munder)
    Underscript,
    /// Overscript (mover)
    Overscript,
    /// Under+overscript (munderover)
    UnderOverscript,
    /// Table/matrix (mtable)
    Table,
    /// Table row (mtr)
    TableRow,
    /// Table cell (mtd)
    TableCell,
    /// Fenced group (mfenced) - deprecated but still used
    Fenced,
    /// Padded space (mpadded)
    Padded,
    /// Phantom (mphantom)
    Phantom,
    /// Generic math container
    Math,
}

// ============================================================================
// Math Box
// ============================================================================

/// Math layout box
#[derive(Debug, Clone)]
pub struct MathBox {
    /// Element type
    pub element_type: MathElementType,
    /// Width
    pub width: f32,
    /// Height above baseline
    pub ascent: f32,
    /// Depth below baseline
    pub descent: f32,
    /// X position relative to parent
    pub x: f32,
    /// Y position relative to parent (baseline relative)
    pub y: f32,
    /// Children
    pub children: Vec<MathBox>,
    /// Content (for leaf elements)
    pub content: Option<String>,
    /// Italic correction for proper kerning
    pub italic_correction: f32,
}

impl MathBox {
    /// Create a new math box
    pub fn new(element_type: MathElementType) -> Self {
        Self {
            element_type,
            width: 0.0,
            ascent: 0.0,
            descent: 0.0,
            x: 0.0,
            y: 0.0,
            children: Vec::new(),
            content: None,
            italic_correction: 0.0,
        }
    }
    
    /// Total height (ascent + descent)
    pub fn height(&self) -> f32 {
        self.ascent + self.descent
    }
    
    /// Create a text/content box
    pub fn with_content(mut self, content: impl Into<String>) -> Self {
        self.content = Some(content.into());
        self
    }
    
    /// Add child
    pub fn add_child(&mut self, child: MathBox) {
        self.children.push(child);
    }
}

// ============================================================================
// Math Layout Parameters
// ============================================================================

/// Mathematical typesetting parameters (based on OpenType MATH table)
#[derive(Debug, Clone)]
pub struct MathLayoutParams {
    /// Base font size
    pub font_size: f32,
    /// Script size multiplier (for superscript/subscript)
    pub script_ratio: f32,
    /// Script-script size multiplier
    pub script_script_ratio: f32,
    /// Fraction rule thickness
    pub fraction_rule_thickness: f32,
    /// Fraction numerator shift up
    pub fraction_num_shift_up: f32,
    /// Fraction denominator shift down
    pub fraction_denom_shift_down: f32,
    /// Radical vertical gap
    pub radical_vertical_gap: f32,
    /// Radical rule thickness
    pub radical_rule_thickness: f32,
    /// Superscript shift up
    pub superscript_shift_up: f32,
    /// Subscript shift down
    pub subscript_shift_down: f32,
    /// Sub-superscript gap minimum
    pub sub_superscript_gap_min: f32,
    /// Underscript gap
    pub underscript_gap: f32,
    /// Overscript gap
    pub overscript_gap: f32,
    /// Axis height (center of fraction bar, etc)
    pub axis_height: f32,
}

impl Default for MathLayoutParams {
    fn default() -> Self {
        // Default values based on typical math fonts
        Self {
            font_size: 16.0,
            script_ratio: 0.71,       // ~71% for scripts
            script_script_ratio: 0.5,  // 50% for script-scripts
            fraction_rule_thickness: 0.66,
            fraction_num_shift_up: 6.0,
            fraction_denom_shift_down: 6.0,
            radical_vertical_gap: 1.5,
            radical_rule_thickness: 0.66,
            superscript_shift_up: 5.0,
            subscript_shift_down: 2.5,
            sub_superscript_gap_min: 2.0,
            underscript_gap: 2.0,
            overscript_gap: 2.0,
            axis_height: 4.0, // Approximate x-height / 2
        }
    }
}

impl MathLayoutParams {
    /// Create with font size
    pub fn with_font_size(font_size: f32) -> Self {
        let scale = font_size / 16.0;
        let mut params = Self::default();
        params.font_size = font_size;
        params.fraction_rule_thickness *= scale;
        params.fraction_num_shift_up *= scale;
        params.fraction_denom_shift_down *= scale;
        params.radical_vertical_gap *= scale;
        params.radical_rule_thickness *= scale;
        params.superscript_shift_up *= scale;
        params.subscript_shift_down *= scale;
        params.sub_superscript_gap_min *= scale;
        params.underscript_gap *= scale;
        params.overscript_gap *= scale;
        params.axis_height *= scale;
        params
    }
}

// ============================================================================
// Layout Functions
// ============================================================================

/// Layout a math row (horizontal arrangement)
pub fn layout_math_row(children: &mut [MathBox], params: &MathLayoutParams) -> (f32, f32, f32) {
    let mut x = 0.0;
    let mut max_ascent: f32 = 0.0;
    let mut max_descent: f32 = 0.0;
    
    for child in children.iter_mut() {
        child.x = x;
        child.y = 0.0;
        x += child.width + child.italic_correction;
        max_ascent = max_ascent.max(child.ascent);
        max_descent = max_descent.max(child.descent);
    }
    
    (x, max_ascent, max_descent)
}

/// Layout a fraction (numerator over denominator)
pub fn layout_fraction(
    numerator: &mut MathBox,
    denominator: &mut MathBox,
    params: &MathLayoutParams,
) -> MathBox {
    let mut result = MathBox::new(MathElementType::Fraction);
    
    // Center numerator and denominator horizontally
    let max_width = numerator.width.max(denominator.width);
    
    numerator.x = (max_width - numerator.width) / 2.0;
    denominator.x = (max_width - denominator.width) / 2.0;
    
    // Position relative to axis (fraction bar is at axis height)
    let num_shift = params.fraction_num_shift_up + params.fraction_rule_thickness / 2.0;
    let denom_shift = params.fraction_denom_shift_down + params.fraction_rule_thickness / 2.0;
    
    numerator.y = -(num_shift + numerator.descent);
    denominator.y = denom_shift + denominator.ascent;
    
    result.width = max_width;
    result.ascent = num_shift + numerator.height();
    result.descent = denom_shift + denominator.height();
    
    result.children.push(numerator.clone());
    result.children.push(denominator.clone());
    
    result
}

/// Layout a radical (square root or nth root)
pub fn layout_radical(
    radicand: &mut MathBox,
    index: Option<&mut MathBox>,
    params: &MathLayoutParams,
) -> MathBox {
    let mut result = MathBox::new(if index.is_some() {
        MathElementType::NthRoot
    } else {
        MathElementType::SquareRoot
    });
    
    // Radical symbol dimensions (simplified)
    let radical_width = params.font_size * 0.6;
    let gap = params.radical_vertical_gap;
    let rule_thickness = params.radical_rule_thickness;
    
    let content_height = radicand.height() + gap + rule_thickness;
    
    // Position radicand
    radicand.x = radical_width;
    radicand.y = 0.0;
    
    result.width = radical_width + radicand.width;
    result.ascent = radicand.ascent + gap + rule_thickness;
    result.descent = radicand.descent;
    
    // Handle index (for nth root)
    if let Some(idx) = index {
        // Index is positioned in upper left of radical
        let scale = params.script_script_ratio;
        idx.x = 0.0;
        idx.y = -(content_height * 0.6);
        
        result.width = result.width.max(idx.width * scale + radical_width);
        result.children.push(idx.clone());
    }
    
    result.children.push(radicand.clone());
    
    result
}

/// Layout superscript
pub fn layout_superscript(
    base: &mut MathBox,
    script: &mut MathBox,
    params: &MathLayoutParams,
) -> MathBox {
    let mut result = MathBox::new(MathElementType::Superscript);
    
    // Scale script
    let script_scale = params.script_ratio;
    
    base.x = 0.0;
    base.y = 0.0;
    
    script.x = base.width + base.italic_correction;
    script.y = -(params.superscript_shift_up);
    
    result.width = script.x + script.width * script_scale;
    result.ascent = base.ascent.max(params.superscript_shift_up + script.ascent * script_scale);
    result.descent = base.descent;
    
    result.children.push(base.clone());
    result.children.push(script.clone());
    
    result
}

/// Layout subscript
pub fn layout_subscript(
    base: &mut MathBox,
    script: &mut MathBox,
    params: &MathLayoutParams,
) -> MathBox {
    let mut result = MathBox::new(MathElementType::Subscript);
    
    let script_scale = params.script_ratio;
    
    base.x = 0.0;
    base.y = 0.0;
    
    script.x = base.width;
    script.y = params.subscript_shift_down;
    
    result.width = script.x + script.width * script_scale;
    result.ascent = base.ascent;
    result.descent = base.descent.max(params.subscript_shift_down + script.descent * script_scale);
    
    result.children.push(base.clone());
    result.children.push(script.clone());
    
    result
}

/// Layout sub+superscript together
pub fn layout_sub_superscript(
    base: &mut MathBox,
    subscript: &mut MathBox,
    superscript: &mut MathBox,
    params: &MathLayoutParams,
) -> MathBox {
    let mut result = MathBox::new(MathElementType::SubSuperscript);
    
    let script_scale = params.script_ratio;
    
    base.x = 0.0;
    base.y = 0.0;
    
    // Position scripts
    let script_x = base.width + base.italic_correction;
    
    superscript.x = script_x;
    superscript.y = -params.superscript_shift_up;
    
    subscript.x = script_x;
    subscript.y = params.subscript_shift_down;
    
    // Ensure minimum gap between scripts
    let gap = (subscript.y - subscript.ascent * script_scale) - 
              (superscript.y + superscript.descent * script_scale);
    if gap < params.sub_superscript_gap_min {
        let adjust = (params.sub_superscript_gap_min - gap) / 2.0;
        superscript.y -= adjust;
        subscript.y += adjust;
    }
    
    result.width = script_x + subscript.width.max(superscript.width) * script_scale;
    result.ascent = base.ascent.max(-superscript.y + superscript.ascent * script_scale);
    result.descent = base.descent.max(subscript.y + subscript.descent * script_scale);
    
    result.children.push(base.clone());
    result.children.push(subscript.clone());
    result.children.push(superscript.clone());
    
    result
}

/// Layout underscript (element with something below, like limits)
pub fn layout_underscript(
    base: &mut MathBox,
    under: &mut MathBox,
    params: &MathLayoutParams,
) -> MathBox {
    let mut result = MathBox::new(MathElementType::Underscript);
    
    let max_width = base.width.max(under.width);
    
    base.x = (max_width - base.width) / 2.0;
    base.y = 0.0;
    
    under.x = (max_width - under.width) / 2.0;
    under.y = base.descent + params.underscript_gap + under.ascent;
    
    result.width = max_width;
    result.ascent = base.ascent;
    result.descent = under.y + under.descent;
    
    result.children.push(base.clone());
    result.children.push(under.clone());
    
    result
}

/// Layout overscript (element with something above)
pub fn layout_overscript(
    base: &mut MathBox,
    over: &mut MathBox,
    params: &MathLayoutParams,
) -> MathBox {
    let mut result = MathBox::new(MathElementType::Overscript);
    
    let max_width = base.width.max(over.width);
    
    base.x = (max_width - base.width) / 2.0;
    base.y = 0.0;
    
    over.x = (max_width - over.width) / 2.0;
    over.y = -(base.ascent + params.overscript_gap + over.descent);
    
    result.width = max_width;
    result.ascent = -over.y + over.ascent;
    result.descent = base.descent;
    
    result.children.push(base.clone());
    result.children.push(over.clone());
    
    result
}

// ============================================================================
// Math Table Layout
// ============================================================================

/// Math table (matrix) style
#[derive(Debug, Clone, Default)]
pub struct MathTableStyle {
    /// Column gap
    pub column_gap: f32,
    /// Row gap
    pub row_gap: f32,
    /// Column alignments (Left, Center, Right)
    pub column_alignments: Vec<ColumnAlign>,
}

/// Column alignment
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ColumnAlign {
    Left,
    #[default]
    Center,
    Right,
}

/// Layout a math table (matrix)
pub fn layout_math_table(
    rows: &mut [Vec<MathBox>],
    style: &MathTableStyle,
    params: &MathLayoutParams,
) -> MathBox {
    if rows.is_empty() {
        return MathBox::new(MathElementType::Table);
    }
    
    // Determine column count and widths
    let num_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let mut col_widths: Vec<f32> = vec![0.0; num_cols];
    let mut row_heights: Vec<(f32, f32)> = Vec::with_capacity(rows.len()); // (ascent, descent)
    
    // First pass: measure
    for row in rows.iter() {
        let mut row_ascent: f32 = 0.0;
        let mut row_descent: f32 = 0.0;
        
        for (col_idx, cell) in row.iter().enumerate() {
            col_widths[col_idx] = col_widths[col_idx].max(cell.width);
            row_ascent = row_ascent.max(cell.ascent);
            row_descent = row_descent.max(cell.descent);
        }
        
        row_heights.push((row_ascent, row_descent));
    }
    
    // Second pass: position
    let total_width: f32 = col_widths.iter().sum::<f32>() + 
                           style.column_gap * (num_cols.saturating_sub(1)) as f32;
    
    let mut y = 0.0;
    let mut result_children = Vec::new();
    
    for (row_idx, row) in rows.iter_mut().enumerate() {
        let (row_ascent, row_descent) = row_heights[row_idx];
        let row_center_y = y + row_ascent;
        
        let mut x = 0.0;
        for (col_idx, cell) in row.iter_mut().enumerate() {
            let col_width = col_widths[col_idx];
            
            // Apply column alignment
            let align = style.column_alignments.get(col_idx)
                .copied()
                .unwrap_or(ColumnAlign::Center);
            
            cell.x = x + match align {
                ColumnAlign::Left => 0.0,
                ColumnAlign::Center => (col_width - cell.width) / 2.0,
                ColumnAlign::Right => col_width - cell.width,
            };
            cell.y = row_center_y - cell.ascent;
            
            result_children.push(cell.clone());
            
            x += col_width + style.column_gap;
        }
        
        y += row_ascent + row_descent + style.row_gap;
    }
    
    let total_height = y - style.row_gap;
    
    let mut result = MathBox::new(MathElementType::Table);
    result.width = total_width;
    result.ascent = total_height / 2.0 + params.axis_height;
    result.descent = total_height / 2.0 - params.axis_height;
    result.children = result_children;
    
    result
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    fn make_box(width: f32, ascent: f32, descent: f32) -> MathBox {
        let mut b = MathBox::new(MathElementType::Identifier);
        b.width = width;
        b.ascent = ascent;
        b.descent = descent;
        b
    }
    
    #[test]
    fn test_math_row() {
        let mut boxes = vec![
            make_box(10.0, 8.0, 2.0),
            make_box(15.0, 10.0, 3.0),
            make_box(10.0, 7.0, 2.0),
        ];
        
        let params = MathLayoutParams::default();
        let (width, ascent, descent) = layout_math_row(&mut boxes, &params);
        
        assert_eq!(width, 35.0);
        assert_eq!(ascent, 10.0);
        assert_eq!(descent, 3.0);
        
        assert_eq!(boxes[0].x, 0.0);
        assert_eq!(boxes[1].x, 10.0);
        assert_eq!(boxes[2].x, 25.0);
    }
    
    #[test]
    fn test_fraction() {
        let mut num = make_box(20.0, 8.0, 2.0);
        let mut denom = make_box(30.0, 8.0, 2.0);
        
        let params = MathLayoutParams::default();
        let result = layout_fraction(&mut num, &mut denom, &params);
        
        assert_eq!(result.width, 30.0); // Max of num/denom widths
        assert!(result.ascent > 0.0);
        assert!(result.descent > 0.0);
    }
    
    #[test]
    fn test_superscript() {
        let mut base = make_box(15.0, 10.0, 2.0);
        let mut script = make_box(8.0, 6.0, 1.0);
        
        let params = MathLayoutParams::default();
        let result = layout_superscript(&mut base, &mut script, &params);
        
        assert!(result.width > base.width);
        assert!(result.ascent >= base.ascent);
    }
    
    #[test]
    fn test_subscript() {
        let mut base = make_box(15.0, 10.0, 2.0);
        let mut script = make_box(8.0, 6.0, 1.0);
        
        let params = MathLayoutParams::default();
        let result = layout_subscript(&mut base, &mut script, &params);
        
        assert!(result.width > base.width);
        assert!(result.descent >= base.descent);
    }
    
    #[test]
    fn test_math_table() {
        let mut rows = vec![
            vec![make_box(10.0, 8.0, 2.0), make_box(10.0, 8.0, 2.0)],
            vec![make_box(10.0, 8.0, 2.0), make_box(10.0, 8.0, 2.0)],
        ];
        
        let style = MathTableStyle {
            column_gap: 5.0,
            row_gap: 3.0,
            column_alignments: vec![ColumnAlign::Center, ColumnAlign::Center],
        };
        
        let params = MathLayoutParams::default();
        let result = layout_math_table(&mut rows, &style, &params);
        
        assert_eq!(result.width, 25.0); // 10 + 5 + 10
        assert!(result.height() > 0.0);
    }
}
