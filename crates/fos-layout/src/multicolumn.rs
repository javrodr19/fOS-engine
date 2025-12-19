//! CSS Multi-column Layout Module
//!
//! Implements CSS Multi-column Layout (column-count, column-width, etc.)

use crate::{LayoutTree, LayoutBoxId};

/// Multi-column container style
#[derive(Debug, Clone)]
pub struct MultiColumnStyle {
    /// Number of columns (0 = auto based on width)
    pub column_count: u32,
    /// Ideal column width (0 = auto based on count)
    pub column_width: f32,
    /// Gap between columns
    pub column_gap: f32,
    /// Column rule (separator line)
    pub column_rule: Option<ColumnRule>,
    /// Column fill mode
    pub column_fill: ColumnFill,
    /// Column span for child elements
    pub column_span: ColumnSpan,
}

impl Default for MultiColumnStyle {
    fn default() -> Self {
        Self {
            column_count: 0,
            column_width: 0.0,
            column_gap: 16.0, // 1em default
            column_rule: None,
            column_fill: ColumnFill::Balance,
            column_span: ColumnSpan::None,
        }
    }
}

/// Column rule (separator line between columns)
#[derive(Debug, Clone)]
pub struct ColumnRule {
    pub width: f32,
    pub style: ColumnRuleStyle,
    pub color: (u8, u8, u8, u8),
}

impl ColumnRule {
    pub fn solid(width: f32, color: (u8, u8, u8, u8)) -> Self {
        Self {
            width,
            style: ColumnRuleStyle::Solid,
            color,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ColumnRuleStyle {
    #[default]
    None,
    Solid,
    Dotted,
    Dashed,
    Double,
    Groove,
    Ridge,
    Inset,
    Outset,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ColumnFill {
    /// Distribute content evenly across columns
    #[default]
    Balance,
    /// Fill columns sequentially
    Auto,
    /// Balance content in last row only
    BalanceAll,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ColumnSpan {
    /// Element stays within its column
    #[default]
    None,
    /// Element spans all columns
    All,
}

/// Multi-column layout context
pub struct MultiColumnContext {
    /// Container width
    pub container_width: f32,
    /// Container height (for fill calculations)
    pub container_height: f32,
    /// Resolved number of columns
    pub num_columns: usize,
    /// Resolved column width
    pub column_width: f32,
    /// Column positions (left edge of each column)
    pub column_positions: Vec<f32>,
    /// Style
    pub style: MultiColumnStyle,
}

impl MultiColumnContext {
    /// Create a new multi-column context
    pub fn new(container_width: f32, container_height: f32, style: MultiColumnStyle) -> Self {
        let (num_columns, column_width) = Self::resolve_columns(
            container_width,
            style.column_count,
            style.column_width,
            style.column_gap,
        );
        
        let mut column_positions = Vec::with_capacity(num_columns);
        let mut x = 0.0;
        for _ in 0..num_columns {
            column_positions.push(x);
            x += column_width + style.column_gap;
        }
        
        Self {
            container_width,
            container_height,
            num_columns,
            column_width,
            column_positions,
            style,
        }
    }
    
    /// Resolve column count and width from style
    fn resolve_columns(
        container_width: f32,
        column_count: u32,
        column_width: f32,
        column_gap: f32,
    ) -> (usize, f32) {
        if column_count > 0 && column_width > 0.0 {
            // Both specified: use count, constrained by width
            let count = column_count as usize;
            let total_gap = (count - 1) as f32 * column_gap;
            let available = container_width - total_gap;
            let width = (available / count as f32).min(column_width);
            (count, width)
        } else if column_count > 0 {
            // Only count specified
            let count = column_count as usize;
            let total_gap = (count.saturating_sub(1)) as f32 * column_gap;
            let available = container_width - total_gap;
            let width = available / count as f32;
            (count, width.max(0.0))
        } else if column_width > 0.0 {
            // Only width specified
            let count = ((container_width + column_gap) / (column_width + column_gap))
                .floor()
                .max(1.0) as usize;
            let total_gap = (count.saturating_sub(1)) as f32 * column_gap;
            let available = container_width - total_gap;
            let width = available / count as f32;
            (count, width)
        } else {
            // Neither specified: single column
            (1, container_width)
        }
    }
    
    /// Get the column index and x position for a given y offset
    pub fn get_column_for_content(&self, content_height: f32, total_height: f32) -> (usize, f32) {
        match self.style.column_fill {
            ColumnFill::Balance => {
                let per_column = total_height / self.num_columns as f32;
                let column = (content_height / per_column).floor() as usize;
                let column = column.min(self.num_columns - 1);
                let x = self.column_positions.get(column).copied().unwrap_or(0.0);
                (column, x)
            }
            ColumnFill::Auto => {
                let per_column = self.container_height;
                let column = (content_height / per_column).floor() as usize;
                let column = column.min(self.num_columns - 1);
                let x = self.column_positions.get(column).copied().unwrap_or(0.0);
                (column, x)
            }
            ColumnFill::BalanceAll => {
                // Similar to balance
                let per_column = total_height / self.num_columns as f32;
                let column = (content_height / per_column).floor() as usize;
                let column = column.min(self.num_columns - 1);
                let x = self.column_positions.get(column).copied().unwrap_or(0.0);
                (column, x)
            }
        }
    }
    
    /// Layout content across columns
    pub fn layout_content(
        &self,
        content_heights: &[f32],
    ) -> Vec<(usize, f32, f32)> {
        // Returns (column_index, x, y) for each content item
        let total_height: f32 = content_heights.iter().sum();
        let mut results = Vec::with_capacity(content_heights.len());
        
        match self.style.column_fill {
            ColumnFill::Balance => {
                let per_column = total_height / self.num_columns as f32;
                let mut current_column = 0;
                let mut column_y = 0.0;
                
                for &height in content_heights {
                    if column_y + height > per_column && current_column < self.num_columns - 1 {
                        current_column += 1;
                        column_y = 0.0;
                    }
                    
                    let x = self.column_positions.get(current_column).copied().unwrap_or(0.0);
                    results.push((current_column, x, column_y));
                    column_y += height;
                }
            }
            ColumnFill::Auto => {
                let per_column = self.container_height;
                let mut current_column = 0;
                let mut column_y = 0.0;
                
                for &height in content_heights {
                    if column_y + height > per_column && current_column < self.num_columns - 1 {
                        current_column += 1;
                        column_y = 0.0;
                    }
                    
                    let x = self.column_positions.get(current_column).copied().unwrap_or(0.0);
                    results.push((current_column, x, column_y));
                    column_y += height;
                }
            }
            ColumnFill::BalanceAll => {
                // Same as Balance for now
                let per_column = total_height / self.num_columns as f32;
                let mut current_column = 0;
                let mut column_y = 0.0;
                
                for &height in content_heights {
                    if column_y + height > per_column && current_column < self.num_columns - 1 {
                        current_column += 1;
                        column_y = 0.0;
                    }
                    
                    let x = self.column_positions.get(current_column).copied().unwrap_or(0.0);
                    results.push((current_column, x, column_y));
                    column_y += height;
                }
            }
        }
        
        results
    }
    
    /// Get column rule positions for rendering
    pub fn get_column_rules(&self) -> Vec<f32> {
        if self.style.column_rule.is_none() {
            return vec![];
        }
        
        let mut rules = Vec::with_capacity(self.num_columns - 1);
        for i in 1..self.num_columns {
            let x = self.column_positions[i] - self.style.column_gap / 2.0;
            rules.push(x);
        }
        rules
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_column_count_only() {
        let style = MultiColumnStyle {
            column_count: 3,
            column_gap: 20.0,
            ..Default::default()
        };
        
        let ctx = MultiColumnContext::new(400.0, 600.0, style);
        
        assert_eq!(ctx.num_columns, 3);
        // (400 - 40 gaps) / 3 = 120
        assert!((ctx.column_width - 120.0).abs() < 0.01);
    }
    
    #[test]
    fn test_column_width_only() {
        let style = MultiColumnStyle {
            column_width: 150.0,
            column_gap: 20.0,
            ..Default::default()
        };
        
        // 500px container, 150px columns, 20px gap
        // (500 + 20) / (150 + 20) = 3.05 -> 3 columns
        let ctx = MultiColumnContext::new(500.0, 600.0, style);
        
        assert_eq!(ctx.num_columns, 3);
    }
    
    #[test]
    fn test_layout_content_balanced() {
        let style = MultiColumnStyle {
            column_count: 2,
            column_gap: 10.0,
            ..Default::default()
        };
        
        let ctx = MultiColumnContext::new(210.0, 600.0, style);
        
        // 4 items of 50px each = 200px total
        // Balanced: 100px per column
        let positions = ctx.layout_content(&[50.0, 50.0, 50.0, 50.0]);
        
        // First two in column 0, last two in column 1
        assert_eq!(positions[0].0, 0);
        assert_eq!(positions[1].0, 0);
        assert_eq!(positions[2].0, 1);
        assert_eq!(positions[3].0, 1);
    }
}
