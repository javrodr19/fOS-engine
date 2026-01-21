//! Masonry Layout (Phase 3.1)
//!
//! CSS Level 3 Masonry layout for asymmetric grid-like layouts.
//! Places items in columns while minimizing vertical gaps.

use std::collections::BinaryHeap;
use std::cmp::Ordering;

// ============================================================================
// Masonry Item
// ============================================================================

/// Individual masonry item
#[derive(Debug, Clone)]
pub struct MasonryItem {
    /// Item ID (index in original list)
    pub id: usize,
    /// Intrinsic width
    pub width: f32,
    /// Intrinsic height
    pub height: f32,
    /// Final computed position
    pub x: f32,
    pub y: f32,
    /// Final computed size
    pub computed_width: f32,
    pub computed_height: f32,
}

impl MasonryItem {
    /// Create a new masonry item
    pub fn new(id: usize, width: f32, height: f32) -> Self {
        Self {
            id,
            width,
            height,
            x: 0.0,
            y: 0.0,
            computed_width: width,
            computed_height: height,
        }
    }
    
    /// Aspect ratio
    pub fn aspect_ratio(&self) -> f32 {
        if self.height > 0.0 {
            self.width / self.height
        } else {
            1.0
        }
    }
}

// ============================================================================
// Column State
// ============================================================================

/// Track state of a single column
#[derive(Debug, Clone)]
struct ColumnState {
    /// Column index
    index: usize,
    /// Current height (Y position for next item)
    height: f32,
}

impl PartialEq for ColumnState {
    fn eq(&self, other: &Self) -> bool {
        self.height == other.height
    }
}

impl Eq for ColumnState {}

impl PartialOrd for ColumnState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ColumnState {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering so min-heap behavior (shortest column first)
        other.height.partial_cmp(&self.height).unwrap_or(Ordering::Equal)
    }
}

// ============================================================================
// Masonry Layout Configuration
// ============================================================================

/// Masonry layout direction
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MasonryDirection {
    /// Items flow into columns (column masonry)
    #[default]
    Columns,
    /// Items flow into rows (row masonry)
    Rows,
}

/// Masonry layout style
#[derive(Debug, Clone)]
pub struct MasonryStyle {
    /// Layout direction
    pub direction: MasonryDirection,
    /// Number of columns/rows (or 0 for auto)
    pub tracks: usize,
    /// Gap between items
    pub gap: f32,
    /// Minimum track size for auto calculation
    pub min_track_size: f32,
    /// Maximum track size for auto calculation
    pub max_track_size: f32,
}

impl Default for MasonryStyle {
    fn default() -> Self {
        Self {
            direction: MasonryDirection::Columns,
            tracks: 0, // Auto
            gap: 0.0,
            min_track_size: 200.0,
            max_track_size: f32::MAX,
        }
    }
}

impl MasonryStyle {
    /// Create style with fixed column count
    pub fn with_columns(count: usize) -> Self {
        Self {
            tracks: count,
            ..Default::default()
        }
    }
    
    /// Set gap
    pub fn with_gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }
    
    /// Set min track size for auto
    pub fn with_min_track_size(mut self, min: f32) -> Self {
        self.min_track_size = min;
        self
    }
}

// ============================================================================
// Masonry Layout Engine
// ============================================================================

/// Result of masonry layout
#[derive(Debug)]
pub struct MasonryLayout {
    /// Laid out items with positions
    pub items: Vec<MasonryItem>,
    /// Number of columns used
    pub column_count: usize,
    /// Column width
    pub column_width: f32,
    /// Total content height
    pub total_height: f32,
    /// Total content width
    pub total_width: f32,
}

/// Compute masonry layout for a set of items
/// 
/// Uses a greedy algorithm: place each item in the shortest column.
/// This produces a balanced layout with minimal vertical gaps.
pub fn layout_masonry(
    items: &[(f32, f32)], // (width, height) pairs
    container_width: f32,
    style: &MasonryStyle,
) -> MasonryLayout {
    if items.is_empty() {
        return MasonryLayout {
            items: Vec::new(),
            column_count: 0,
            column_width: 0.0,
            total_height: 0.0,
            total_width: container_width,
        };
    }
    
    // Determine column count
    let column_count = if style.tracks > 0 {
        style.tracks
    } else {
        // Auto-calculate based on container width and min track size
        let available = container_width + style.gap;
        let min_with_gap = style.min_track_size + style.gap;
        ((available / min_with_gap).floor() as usize).max(1)
    };
    
    // Calculate column width
    let total_gaps = style.gap * (column_count - 1) as f32;
    let column_width = ((container_width - total_gaps) / column_count as f32)
        .min(style.max_track_size);
    
    // Initialize column state heap (min-heap by height)
    let mut columns: BinaryHeap<ColumnState> = (0..column_count)
        .map(|i| ColumnState { index: i, height: 0.0 })
        .collect();
    
    // Place items
    let mut laid_out = Vec::with_capacity(items.len());
    
    for (id, (width, height)) in items.iter().enumerate() {
        // Find shortest column
        let mut shortest = columns.pop().unwrap();
        
        // Compute item position
        let x = shortest.index as f32 * (column_width + style.gap);
        let y = shortest.height;
        
        // Scale item to column width, preserving aspect ratio
        let aspect_ratio = if *height > 0.0 { *width / *height } else { 1.0 };
        let computed_width = column_width;
        let computed_height = column_width / aspect_ratio;
        
        // Create positioned item
        laid_out.push(MasonryItem {
            id,
            width: *width,
            height: *height,
            x,
            y,
            computed_width,
            computed_height,
        });
        
        // Update column height
        shortest.height = y + computed_height + style.gap;
        columns.push(shortest);
    }
    
    // Find total height (tallest column)
    let total_height = columns.iter()
        .map(|c| c.height - style.gap) // Remove trailing gap
        .fold(0.0f32, |a, b| a.max(b));
    
    MasonryLayout {
        items: laid_out,
        column_count,
        column_width,
        total_height: total_height.max(0.0),
        total_width: container_width,
    }
}

/// Optimized masonry layout with item reordering
/// 
/// Tries different orderings to minimize total height difference
/// between columns (more balanced layout).
pub fn layout_masonry_balanced(
    items: &[(f32, f32)],
    container_width: f32,
    style: &MasonryStyle,
) -> MasonryLayout {
    // For small item counts, try height-sorted order
    if items.len() <= 50 {
        // Sort by height (tallest first)
        let mut sorted: Vec<(usize, f32, f32)> = items.iter()
            .enumerate()
            .map(|(i, (w, h))| (i, *w, *h))
            .collect();
        sorted.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(Ordering::Equal));
        
        let sorted_items: Vec<(f32, f32)> = sorted.iter()
            .map(|(_, w, h)| (*w, *h))
            .collect();
        
        let mut result = layout_masonry(&sorted_items, container_width, style);
        
        // Restore original IDs
        for (i, item) in result.items.iter_mut().enumerate() {
            item.id = sorted[i].0;
        }
        
        result
    } else {
        // For large counts, use standard algorithm
        layout_masonry(items, container_width, style)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_empty_masonry() {
        let result = layout_masonry(&[], 800.0, &MasonryStyle::default());
        assert_eq!(result.items.len(), 0);
        assert_eq!(result.total_height, 0.0);
    }
    
    #[test]
    fn test_single_item() {
        let items = vec![(100.0, 150.0)];
        let style = MasonryStyle::with_columns(3);
        let result = layout_masonry(&items, 900.0, &style);
        
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.column_count, 3);
        assert_eq!(result.items[0].x, 0.0);
        assert_eq!(result.items[0].y, 0.0);
    }
    
    #[test]
    fn test_multiple_columns() {
        let items = vec![
            (100.0, 100.0), // Square - aspect ratio 1
            (100.0, 100.0),
            (100.0, 100.0),
        ];
        let style = MasonryStyle::with_columns(3);
        let result = layout_masonry(&items, 300.0, &style);
        
        assert_eq!(result.column_count, 3);
        // Each item should be in a different column
        let positions: Vec<f32> = result.items.iter().map(|i| i.x).collect();
        assert!(positions.contains(&0.0));
        assert!(positions.contains(&100.0));
        assert!(positions.contains(&200.0));
    }
    
    #[test]
    fn test_column_balancing() {
        // Create items that will require balancing
        let items = vec![
            (100.0, 200.0), // Tall
            (100.0, 100.0), // Short
            (100.0, 100.0), // Short
        ];
        let style = MasonryStyle::with_columns(2);
        let result = layout_masonry(&items, 200.0, &style);
        
        assert_eq!(result.column_count, 2);
        // First (tall) item in column 0
        // Second and third items should stack in column 1 or column 0
        assert_eq!(result.items.len(), 3);
    }
    
    #[test]
    fn test_gap_handling() {
        let items = vec![(100.0, 100.0), (100.0, 100.0)];
        let style = MasonryStyle::with_columns(2).with_gap(10.0);
        let result = layout_masonry(&items, 210.0, &style);
        
        // Column width should account for gap
        assert_eq!(result.column_count, 2);
        // Second column starts at column_width + gap
        if result.items[1].x > result.items[0].x {
            assert!((result.items[1].x - (result.column_width + 10.0)).abs() < 0.1);
        }
    }
    
    #[test]
    fn test_auto_columns() {
        let items = vec![(100.0, 100.0)];
        let mut style = MasonryStyle::default();
        style.min_track_size = 200.0;
        
        let result = layout_masonry(&items, 850.0, &style);
        // 850px / 200px min = 4 columns
        assert_eq!(result.column_count, 4);
    }
    
    #[test]
    fn test_aspect_ratio_preservation() {
        let items = vec![(200.0, 100.0)]; // 2:1 aspect ratio
        let style = MasonryStyle::with_columns(2);
        let result = layout_masonry(&items, 400.0, &style);
        
        let item = &result.items[0];
        let computed_ratio = item.computed_width / item.computed_height;
        let original_ratio = 2.0;
        assert!((computed_ratio - original_ratio).abs() < 0.01);
    }
}
