//! CSS Table Layout Module
//!
//! Implements CSS Table Layout for the fOS browser engine.

use crate::{LayoutTree, LayoutBoxId};

/// Table layout algorithm
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TableLayout {
    /// Auto layout - widths based on content
    #[default]
    Auto,
    /// Fixed layout - widths from first row
    Fixed,
}

/// Border collapse mode
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BorderCollapse {
    /// Separate borders (default)
    #[default]
    Separate,
    /// Collapsed borders
    Collapse,
}

/// Caption position
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CaptionSide {
    #[default]
    Top,
    Bottom,
}

/// Table style definition
#[derive(Debug, Clone, Default)]
pub struct TableStyle {
    pub layout: TableLayout,
    pub border_collapse: BorderCollapse,
    pub border_spacing: (f32, f32), // horizontal, vertical
    pub caption_side: CaptionSide,
    pub empty_cells: EmptyCells,
}

/// Empty cell visibility
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EmptyCells {
    #[default]
    Show,
    Hide,
}

/// Table cell spanning
#[derive(Debug, Clone, Copy, Default)]
pub struct CellSpan {
    pub colspan: u32,
    pub rowspan: u32,
}

impl CellSpan {
    pub fn new(colspan: u32, rowspan: u32) -> Self {
        Self {
            colspan: colspan.max(1),
            rowspan: rowspan.max(1),
        }
    }
    
    pub fn single() -> Self {
        Self { colspan: 1, rowspan: 1 }
    }
}

/// Table cell definition
#[derive(Debug, Clone)]
pub struct TableCell {
    pub row: usize,
    pub col: usize,
    pub span: CellSpan,
    pub content_width: f32,
    pub content_height: f32,
}

/// Table structure for layout
#[derive(Debug, Clone)]
pub struct TableStructure {
    /// Number of columns
    pub num_cols: usize,
    /// Number of rows
    pub num_rows: usize,
    /// Cells (may have gaps for spanned cells)
    pub cells: Vec<TableCell>,
    /// Column widths
    pub col_widths: Vec<f32>,
    /// Row heights
    pub row_heights: Vec<f32>,
    /// Table style
    pub style: TableStyle,
}

impl TableStructure {
    /// Create a new table structure
    pub fn new(num_cols: usize, num_rows: usize, style: TableStyle) -> Self {
        Self {
            num_cols,
            num_rows,
            cells: Vec::new(),
            col_widths: vec![0.0; num_cols],
            row_heights: vec![0.0; num_rows],
            style,
        }
    }
    
    /// Add a cell
    pub fn add_cell(&mut self, cell: TableCell) {
        // Update column widths based on content
        if cell.span.colspan == 1 && cell.col < self.num_cols {
            self.col_widths[cell.col] = self.col_widths[cell.col].max(cell.content_width);
        }
        
        // Update row heights based on content
        if cell.span.rowspan == 1 && cell.row < self.num_rows {
            self.row_heights[cell.row] = self.row_heights[cell.row].max(cell.content_height);
        }
        
        self.cells.push(cell);
    }
    
    /// Set column widths explicitly (for fixed layout)
    pub fn set_column_widths(&mut self, widths: Vec<f32>) {
        for (i, w) in widths.into_iter().enumerate() {
            if i < self.col_widths.len() {
                self.col_widths[i] = w;
            }
        }
    }
    
    /// Compute table dimensions
    pub fn compute_dimensions(&self) -> (f32, f32) {
        let spacing = if self.style.border_collapse == BorderCollapse::Separate {
            self.style.border_spacing
        } else {
            (0.0, 0.0)
        };
        
        let width: f32 = self.col_widths.iter().sum::<f32>() 
            + spacing.0 * (self.num_cols.saturating_sub(1)) as f32;
        let height: f32 = self.row_heights.iter().sum::<f32>()
            + spacing.1 * (self.num_rows.saturating_sub(1)) as f32;
        
        (width, height)
    }
    
    /// Get cell position (x, y, width, height)
    pub fn get_cell_bounds(&self, row: usize, col: usize, span: &CellSpan) -> (f32, f32, f32, f32) {
        let spacing = if self.style.border_collapse == BorderCollapse::Separate {
            self.style.border_spacing
        } else {
            (0.0, 0.0)
        };
        
        // Calculate x position
        let x: f32 = self.col_widths[..col].iter().sum::<f32>()
            + spacing.0 * col as f32;
        
        // Calculate y position
        let y: f32 = self.row_heights[..row].iter().sum::<f32>()
            + spacing.1 * row as f32;
        
        // Calculate width (sum of spanned columns)
        let end_col = (col + span.colspan as usize).min(self.num_cols);
        let width: f32 = self.col_widths[col..end_col].iter().sum::<f32>()
            + spacing.0 * (span.colspan.saturating_sub(1)) as f32;
        
        // Calculate height (sum of spanned rows)
        let end_row = (row + span.rowspan as usize).min(self.num_rows);
        let height: f32 = self.row_heights[row..end_row].iter().sum::<f32>()
            + spacing.1 * (span.rowspan.saturating_sub(1)) as f32;
        
        (x, y, width, height)
    }
}

/// Table layout context
pub struct TableLayoutContext {
    /// Container width
    pub container_width: f32,
    /// Table structure
    pub structure: TableStructure,
}

impl TableLayoutContext {
    /// Create a new table layout context
    pub fn new(container_width: f32, structure: TableStructure) -> Self {
        Self {
            container_width,
            structure,
        }
    }
    
    /// Compute column widths for auto layout
    pub fn compute_auto_widths(&mut self) {
        let total_content: f32 = self.structure.col_widths.iter().sum();
        
        if total_content > 0.0 && total_content != self.container_width {
            let scale = self.container_width / total_content;
            for w in &mut self.structure.col_widths {
                *w *= scale;
            }
        }
    }
    
    /// Compute column widths for fixed layout (equal distribution)
    pub fn compute_fixed_widths(&mut self) {
        let num_cols = self.structure.num_cols;
        if num_cols > 0 {
            let spacing = if self.structure.style.border_collapse == BorderCollapse::Separate {
                self.structure.style.border_spacing.0
            } else {
                0.0
            };
            
            let total_spacing = spacing * (num_cols.saturating_sub(1)) as f32;
            let available = self.container_width - total_spacing;
            let col_width = available / num_cols as f32;
            
            for w in &mut self.structure.col_widths {
                *w = col_width;
            }
        }
    }
    
    /// Layout the table
    pub fn layout(&mut self) {
        match self.structure.style.layout {
            TableLayout::Auto => self.compute_auto_widths(),
            TableLayout::Fixed => self.compute_fixed_widths(),
        }
    }
    
    /// Get cell positions for all cells
    pub fn get_cell_positions(&self) -> Vec<(usize, usize, f32, f32, f32, f32)> {
        self.structure.cells.iter().map(|cell| {
            let (x, y, w, h) = self.structure.get_cell_bounds(cell.row, cell.col, &cell.span);
            (cell.row, cell.col, x, y, w, h)
        }).collect()
    }
}

/// Build a table structure from rows of cells
pub fn build_table_structure(
    rows: &[Vec<(f32, f32, CellSpan)>], // (content_width, content_height, span)
    style: TableStyle,
) -> TableStructure {
    let num_rows = rows.len();
    let num_cols = rows.iter()
        .map(|row| row.iter().map(|(_, _, s)| s.colspan as usize).sum())
        .max()
        .unwrap_or(0);
    
    let mut table = TableStructure::new(num_cols, num_rows, style);
    
    // Track occupied cells for rowspan
    let mut occupied = vec![vec![false; num_cols]; num_rows];
    
    for (row_idx, row) in rows.iter().enumerate() {
        let mut col_idx = 0;
        
        for (content_w, content_h, span) in row {
            // Skip occupied cells
            while col_idx < num_cols && occupied[row_idx][col_idx] {
                col_idx += 1;
            }
            
            if col_idx >= num_cols {
                break;
            }
            
            // Mark cells as occupied for rowspan
            for r in row_idx..(row_idx + span.rowspan as usize).min(num_rows) {
                for c in col_idx..(col_idx + span.colspan as usize).min(num_cols) {
                    occupied[r][c] = true;
                }
            }
            
            table.add_cell(TableCell {
                row: row_idx,
                col: col_idx,
                span: *span,
                content_width: *content_w,
                content_height: *content_h,
            });
            
            col_idx += span.colspan as usize;
        }
    }
    
    table
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_simple_table() {
        let style = TableStyle::default();
        let mut table = TableStructure::new(3, 2, style);
        
        table.add_cell(TableCell { row: 0, col: 0, span: CellSpan::single(), content_width: 100.0, content_height: 30.0 });
        table.add_cell(TableCell { row: 0, col: 1, span: CellSpan::single(), content_width: 150.0, content_height: 30.0 });
        table.add_cell(TableCell { row: 0, col: 2, span: CellSpan::single(), content_width: 100.0, content_height: 30.0 });
        
        let (w, h) = table.compute_dimensions();
        assert_eq!(w, 350.0);
        assert_eq!(h, 30.0);
    }
    
    #[test]
    fn test_table_with_spacing() {
        let style = TableStyle {
            border_spacing: (10.0, 5.0),
            ..Default::default()
        };
        let mut table = TableStructure::new(3, 2, style);
        
        table.add_cell(TableCell { row: 0, col: 0, span: CellSpan::single(), content_width: 100.0, content_height: 30.0 });
        table.add_cell(TableCell { row: 0, col: 1, span: CellSpan::single(), content_width: 100.0, content_height: 30.0 });
        table.add_cell(TableCell { row: 0, col: 2, span: CellSpan::single(), content_width: 100.0, content_height: 30.0 });
        table.add_cell(TableCell { row: 1, col: 0, span: CellSpan::single(), content_width: 100.0, content_height: 40.0 });
        
        // 300 + 20 (2 gaps) = 320
        // 70 + 5 (1 gap) = 75
        let (w, h) = table.compute_dimensions();
        assert_eq!(w, 320.0);
        assert_eq!(h, 75.0);
    }
    
    #[test]
    fn test_cell_spanning() {
        let style = TableStyle::default();
        let mut table = TableStructure::new(3, 2, style);
        
        // First row: cell spans 2 columns
        table.add_cell(TableCell { row: 0, col: 0, span: CellSpan::new(2, 1), content_width: 200.0, content_height: 30.0 });
        table.add_cell(TableCell { row: 0, col: 2, span: CellSpan::single(), content_width: 100.0, content_height: 30.0 });
        
        table.col_widths = vec![100.0, 100.0, 100.0];
        
        let (x, y, w, h) = table.get_cell_bounds(0, 0, &CellSpan::new(2, 1));
        assert_eq!(x, 0.0);
        assert_eq!(w, 200.0); // Spans 2 columns
    }
    
    #[test]
    fn test_fixed_layout() {
        let style = TableStyle {
            layout: TableLayout::Fixed,
            ..Default::default()
        };
        let table = TableStructure::new(4, 2, style);
        
        let mut ctx = TableLayoutContext::new(400.0, table);
        ctx.layout();
        
        // Each column should be 100px
        for w in &ctx.structure.col_widths {
            assert!((w - 100.0).abs() < 0.01);
        }
    }
}
