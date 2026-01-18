//! CSS Grid Layout Module
//!
//! Implements CSS Grid Layout (Level 1) for the fOS browser engine.
//! Uses arena allocation for efficient grid track storage during layout.

use crate::{LayoutTree, LayoutBoxId, BoxDimensions};
use crate::box_model::EdgeSizes;

// ============================================================================
// Parallel Intrinsic Size Computation (Phase 4.2)
// ============================================================================

/// Grid item intrinsic size result
#[derive(Debug, Clone, Copy)]
pub struct GridItemIntrinsic {
    /// Item box ID
    pub box_id: LayoutBoxId,
    /// Min-content width
    pub min_width: f32,
    /// Max-content width
    pub max_width: f32,
    /// Min-content height
    pub min_height: f32,
    /// Max-content height
    pub max_height: f32,
}

/// Compute intrinsic sizes for all grid items in parallel
/// 
/// This function computes min-content and max-content sizes for all
/// grid items concurrently, enabling faster track sizing resolution.
pub fn compute_grid_item_intrinsic_parallel(
    tree: &LayoutTree,
    items: &[LayoutBoxId],
) -> Vec<GridItemIntrinsic> {
    if items.is_empty() {
        return Vec::new();
    }
    
    // For small numbers of items, compute sequentially
    if items.len() < 4 {
        return items.iter()
            .map(|&box_id| compute_item_intrinsic(tree, box_id))
            .collect();
    }
    
    // Parallel computation using scoped threads
    let num_threads = std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(4)
        .min(items.len());
    
    let chunk_size = (items.len() + num_threads - 1) / num_threads;
    
    std::thread::scope(|s| {
        let handles: Vec<_> = items
            .chunks(chunk_size)
            .map(|chunk| {
                s.spawn(|| {
                    chunk.iter()
                        .map(|&box_id| compute_item_intrinsic(tree, box_id))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        
        handles.into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    })
}

/// Compute intrinsic size for a single grid item
fn compute_item_intrinsic(
    tree: &LayoutTree,
    box_id: LayoutBoxId,
) -> GridItemIntrinsic {
    let layout_box = match tree.get(box_id) {
        Some(b) => b,
        None => return GridItemIntrinsic {
            box_id,
            min_width: 0.0,
            max_width: 0.0,
            min_height: 0.0,
            max_height: 0.0,
        },
    };
    
    let dims = &layout_box.dimensions;
    
    // Use content dimensions as intrinsic sizes
    GridItemIntrinsic {
        box_id,
        min_width: dims.content.width.max(0.0),
        max_width: dims.content.width.max(0.0),
        min_height: dims.content.height.max(0.0),
        max_height: dims.content.height.max(0.0),
    }
}

/// Compute grid track sizes using intrinsic item data in parallel
/// 
/// This distributes the track sizing algorithm for auto/min-content/max-content
/// tracks across multiple threads.
pub fn compute_track_sizes_parallel(
    tracks: &[TrackSize],
    container_size: f32,
    gap: f32,
    item_contributions: &[(usize, f32)], // (track_index, contribution)
) -> Vec<f32> {
    if tracks.is_empty() {
        return Vec::new();
    }
    
    let num_gaps = tracks.len().saturating_sub(1);
    let total_gap = gap * num_gaps as f32;
    let available = container_size - total_gap;
    
    // Compute base sizes in parallel for large track counts
    if tracks.len() >= 8 {
        return parallel_track_resolve(tracks, available, item_contributions);
    }
    
    // Sequential for small track counts
    sequential_track_resolve(tracks, available, item_contributions)
}

fn parallel_track_resolve(
    tracks: &[TrackSize],
    available: f32,
    item_contributions: &[(usize, f32)],
) -> Vec<f32> {
    // First pass: compute base sizes
    let num_threads = std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(4)
        .min(tracks.len());
    
    let chunk_size = (tracks.len() + num_threads - 1) / num_threads;
    
    let base_sizes: Vec<f32> = std::thread::scope(|s| {
        let handles: Vec<_> = tracks
            .chunks(chunk_size)
            .enumerate()
            .map(|(chunk_idx, chunk)| {
                let offset = chunk_idx * chunk_size;
                s.spawn(move || {
                    chunk.iter()
                        .enumerate()
                        .map(|(i, track)| {
                            let idx = offset + i;
                            compute_base_track_size(track, available, idx, item_contributions)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        
        handles.into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    });
    
    // Second pass: resolve flexible tracks
    resolve_flexible_tracks(tracks, &base_sizes, available)
}

fn sequential_track_resolve(
    tracks: &[TrackSize],
    available: f32,
    item_contributions: &[(usize, f32)],
) -> Vec<f32> {
    let base_sizes: Vec<f32> = tracks.iter()
        .enumerate()
        .map(|(idx, track)| compute_base_track_size(track, available, idx, item_contributions))
        .collect();
    
    resolve_flexible_tracks(tracks, &base_sizes, available)
}

fn compute_base_track_size(
    track: &TrackSize,
    container_size: f32,
    track_idx: usize,
    item_contributions: &[(usize, f32)],
) -> f32 {
    match track {
        TrackSize::Length(px) => *px,
        TrackSize::Percentage(pct) => container_size * pct / 100.0,
        TrackSize::Auto | TrackSize::MinContent | TrackSize::MaxContent => {
            // Find max contribution for this track
            item_contributions.iter()
                .filter(|(idx, _)| *idx == track_idx)
                .map(|(_, c)| *c)
                .fold(0.0f32, |a, b| a.max(b))
        }
        TrackSize::FitContent(limit) => {
            let content_size = item_contributions.iter()
                .filter(|(idx, _)| *idx == track_idx)
                .map(|(_, c)| *c)
                .fold(0.0f32, |a, b| a.max(b));
            content_size.min(*limit)
        }
        TrackSize::MinMax(min, max) => {
            let min_size = compute_base_track_size(min, container_size, track_idx, item_contributions);
            let max_size = compute_base_track_size(max, container_size, track_idx, item_contributions);
            min_size.max(max_size.min(min_size)) // Will be clamped properly later
        }
        TrackSize::Fraction(_) => 0.0, // Resolved in second pass
    }
}

fn resolve_flexible_tracks(tracks: &[TrackSize], base_sizes: &[f32], available: f32) -> Vec<f32> {
    let fixed_total: f32 = base_sizes.iter().sum();
    let remaining = (available - fixed_total).max(0.0);
    
    let total_flex: f32 = tracks.iter()
        .map(|t| t.flex_factor())
        .sum();
    
    if total_flex <= 0.0 {
        return base_sizes.to_vec();
    }
    
    let fr_size = remaining / total_flex;
    
    tracks.iter()
        .zip(base_sizes.iter())
        .map(|(track, &base)| {
            if let TrackSize::Fraction(fr) = track {
                fr_size * fr
            } else {
                base
            }
        })
        .collect()
}

// ============================================================================
// Arena Allocation for Grid Layout
// ============================================================================

/// Simple bump allocator for grid layout calculations
/// Avoids repeated heap allocations during layout passes
#[derive(Debug)]
pub struct GridArena {
    /// Pre-allocated storage for track sizes
    track_buffer: Vec<f32>,
    /// Pre-allocated storage for positions
    position_buffer: Vec<f32>,
    /// Current offset in track buffer
    track_offset: usize,
    /// Current offset in position buffer
    position_offset: usize,
}

impl Default for GridArena {
    fn default() -> Self {
        Self::new(64) // Default capacity for 64 tracks
    }
}

impl GridArena {
    /// Create a new grid arena with specified capacity
    pub fn new(capacity: usize) -> Self {
        Self {
            track_buffer: vec![0.0; capacity * 2], // For columns and rows
            position_buffer: vec![0.0; capacity * 2 + 2], // +1 for each dimension
            track_offset: 0,
            position_offset: 0,
        }
    }
    
    /// Reset the arena for reuse
    pub fn reset(&mut self) {
        self.track_offset = 0;
        self.position_offset = 0;
    }
    
    /// Allocate space for track sizes
    pub fn alloc_tracks(&mut self, count: usize) -> &mut [f32] {
        let start = self.track_offset;
        let end = start + count;
        
        // Grow buffer if needed
        if end > self.track_buffer.len() {
            self.track_buffer.resize(end * 2, 0.0);
        }
        
        self.track_offset = end;
        &mut self.track_buffer[start..end]
    }
    
    /// Allocate space for positions (count + 1)
    pub fn alloc_positions(&mut self, count: usize) -> &mut [f32] {
        let start = self.position_offset;
        let end = start + count + 1; // Positions are count + 1
        
        if end > self.position_buffer.len() {
            self.position_buffer.resize(end * 2, 0.0);
        }
        
        self.position_offset = end;
        &mut self.position_buffer[start..end]
    }
    
    /// Get current track buffer usage
    pub fn track_usage(&self) -> usize {
        self.track_offset
    }
}

/// Grid track sizing
#[derive(Debug, Clone, PartialEq)]
pub enum TrackSize {
    /// Fixed length in pixels
    Length(f32),
    /// Percentage of container
    Percentage(f32),
    /// Flexible fraction (fr unit)
    Fraction(f32),
    /// Auto (content-based)
    Auto,
    /// min-content
    MinContent,
    /// max-content
    MaxContent,
    /// minmax(min, max)
    MinMax(Box<TrackSize>, Box<TrackSize>),
    /// fit-content(limit)
    FitContent(f32),
}

impl Default for TrackSize {
    fn default() -> Self {
        Self::Auto
    }
}

impl TrackSize {
    /// Resolve to a fixed size if possible (returns None for flexible tracks)
    pub fn resolve(&self, container_size: f32, fr_size: f32) -> Option<f32> {
        match self {
            Self::Length(px) => Some(*px),
            Self::Percentage(pct) => Some(container_size * pct / 100.0),
            Self::Fraction(fr) => {
                // Only resolve if we have a valid fr_size
                if fr_size > 0.0 {
                    Some(fr_size * fr)
                } else {
                    None // Will be resolved in second pass
                }
            }
            Self::Auto => None,
            Self::MinContent => None,
            Self::MaxContent => None,
            Self::MinMax(_, _) => None,
            Self::FitContent(_) => None,
        }
    }
    
    /// Check if this is a flexible track
    pub fn is_flexible(&self) -> bool {
        matches!(self, Self::Fraction(_))
    }
    
    /// Get the flex factor
    pub fn flex_factor(&self) -> f32 {
        match self {
            Self::Fraction(fr) => *fr,
            _ => 0.0,
        }
    }
}

/// Grid template definition
#[derive(Debug, Clone, Default)]
pub struct GridTemplate {
    /// Column track sizes
    pub columns: Vec<TrackSize>,
    /// Row track sizes
    pub rows: Vec<TrackSize>,
    /// Column gap
    pub column_gap: f32,
    /// Row gap
    pub row_gap: f32,
}

impl GridTemplate {
    /// Create a new grid template
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Set columns
    pub fn with_columns(mut self, columns: Vec<TrackSize>) -> Self {
        self.columns = columns;
        self
    }
    
    /// Set rows
    pub fn with_rows(mut self, rows: Vec<TrackSize>) -> Self {
        self.rows = rows;
        self
    }
    
    /// Set gap
    pub fn with_gap(mut self, gap: f32) -> Self {
        self.column_gap = gap;
        self.row_gap = gap;
        self
    }
    
    /// repeat(count, size)
    pub fn repeat(count: usize, size: TrackSize) -> Vec<TrackSize> {
        vec![size; count]
    }
}

/// Grid item placement
#[derive(Debug, Clone, Default)]
pub struct GridPlacement {
    /// Column start (1-indexed, or auto)
    pub column_start: GridLine,
    /// Column end
    pub column_end: GridLine,
    /// Row start
    pub row_start: GridLine,
    /// Row end
    pub row_end: GridLine,
}

/// Grid line specification
#[derive(Debug, Clone, Default, PartialEq)]
pub enum GridLine {
    /// Auto placement
    #[default]
    Auto,
    /// Line number (1-indexed)
    Line(i32),
    /// Span count
    Span(u32),
}

impl GridPlacement {
    /// Create placement for a single cell
    pub fn cell(column: i32, row: i32) -> Self {
        Self {
            column_start: GridLine::Line(column),
            column_end: GridLine::Line(column + 1),
            row_start: GridLine::Line(row),
            row_end: GridLine::Line(row + 1),
        }
    }
    
    /// Create placement spanning columns
    pub fn span_columns(column: i32, span: u32, row: i32) -> Self {
        Self {
            column_start: GridLine::Line(column),
            column_end: GridLine::Span(span),
            row_start: GridLine::Line(row),
            row_end: GridLine::Line(row + 1),
        }
    }
    
    /// Create placement spanning rows
    pub fn span_rows(column: i32, row: i32, span: u32) -> Self {
        Self {
            column_start: GridLine::Line(column),
            column_end: GridLine::Line(column + 1),
            row_start: GridLine::Line(row),
            row_end: GridLine::Span(span),
        }
    }
}

/// Resolved grid cell position
#[derive(Debug, Clone)]
pub struct GridArea {
    pub column_start: usize,
    pub column_end: usize,
    pub row_start: usize,
    pub row_end: usize,
}

impl GridArea {
    pub fn width(&self) -> usize {
        self.column_end.saturating_sub(self.column_start)
    }
    
    pub fn height(&self) -> usize {
        self.row_end.saturating_sub(self.row_start)
    }
}

/// Grid layout context
pub struct GridLayoutContext {
    /// Container width
    pub container_width: f32,
    /// Container height
    pub container_height: f32,
    /// Grid template
    pub template: GridTemplate,
    /// Resolved column sizes
    column_sizes: Vec<f32>,
    /// Resolved row sizes
    row_sizes: Vec<f32>,
    /// Column positions
    column_positions: Vec<f32>,
    /// Row positions
    row_positions: Vec<f32>,
}

impl GridLayoutContext {
    /// Create a new grid layout context
    pub fn new(container_width: f32, container_height: f32, template: GridTemplate) -> Self {
        let mut ctx = Self {
            container_width,
            container_height,
            template,
            column_sizes: Vec::new(),
            row_sizes: Vec::new(),
            column_positions: Vec::new(),
            row_positions: Vec::new(),
        };
        ctx.resolve_tracks();
        ctx
    }
    
    /// Resolve track sizes
    fn resolve_tracks(&mut self) {
        self.column_sizes = self.resolve_track_sizes(&self.template.columns, self.container_width, self.template.column_gap);
        self.row_sizes = self.resolve_track_sizes(&self.template.rows, self.container_height, self.template.row_gap);
        
        // Compute positions
        self.column_positions = self.compute_positions(&self.column_sizes, self.template.column_gap);
        self.row_positions = self.compute_positions(&self.row_sizes, self.template.row_gap);
    }
    
    /// Resolve a list of track sizes
    fn resolve_track_sizes(&self, tracks: &[TrackSize], container_size: f32, gap: f32) -> Vec<f32> {
        if tracks.is_empty() {
            return vec![];
        }
        
        let num_gaps = tracks.len().saturating_sub(1);
        let total_gap = gap * num_gaps as f32;
        let available = container_size - total_gap;
        
        // First pass: resolve fixed sizes
        let mut sizes: Vec<Option<f32>> = tracks.iter()
            .map(|t| t.resolve(container_size, 0.0))
            .collect();
        
        // Calculate remaining space for flexible tracks
        let fixed_total: f32 = sizes.iter().filter_map(|s| *s).sum();
        let remaining = (available - fixed_total).max(0.0);
        
        // Calculate total flex factor
        let total_flex: f32 = tracks.iter()
            .filter(|t| t.is_flexible())
            .map(|t| t.flex_factor())
            .sum();
        
        let fr_size = if total_flex > 0.0 { remaining / total_flex } else { 0.0 };
        
        // Second pass: resolve flexible sizes
        for (i, track) in tracks.iter().enumerate() {
            if sizes[i].is_none() {
                sizes[i] = Some(track.resolve(container_size, fr_size).unwrap_or(0.0));
            }
        }
        
        sizes.into_iter().map(|s| s.unwrap_or(0.0)).collect()
    }
    
    /// Compute positions from sizes
    fn compute_positions(&self, sizes: &[f32], gap: f32) -> Vec<f32> {
        let mut positions = Vec::with_capacity(sizes.len() + 1);
        let mut pos = 0.0;
        positions.push(pos);
        
        for (i, &size) in sizes.iter().enumerate() {
            pos += size;
            if i < sizes.len() - 1 {
                pos += gap;
            }
            positions.push(pos);
        }
        
        positions
    }
    
    /// Get the bounding box for a grid area
    pub fn get_area_bounds(&self, area: &GridArea) -> (f32, f32, f32, f32) {
        let x = self.column_positions.get(area.column_start).copied().unwrap_or(0.0);
        let y = self.row_positions.get(area.row_start).copied().unwrap_or(0.0);
        
        let x2 = self.column_positions.get(area.column_end).copied().unwrap_or(x);
        let y2 = self.row_positions.get(area.row_end).copied().unwrap_or(y);
        
        // Account for gaps within the area
        let gap_x = if area.width() > 1 {
            self.template.column_gap * (area.width() - 1) as f32
        } else { 0.0 };
        let gap_y = if area.height() > 1 {
            self.template.row_gap * (area.height() - 1) as f32
        } else { 0.0 };
        
        (x, y, x2 - x - gap_x, y2 - y - gap_y)
    }
    
    /// Number of columns
    pub fn num_columns(&self) -> usize {
        self.column_sizes.len()
    }
    
    /// Number of rows
    pub fn num_rows(&self) -> usize {
        self.row_sizes.len()
    }
}

/// Resolve grid placement to concrete area
pub fn resolve_placement(
    placement: &GridPlacement,
    num_columns: usize,
    num_rows: usize,
    auto_column: &mut usize,
    auto_row: &mut usize,
) -> GridArea {
    let col_start = match &placement.column_start {
        GridLine::Auto => {
            let col = *auto_column;
            *auto_column += 1;
            if *auto_column > num_columns {
                *auto_column = 1;
                *auto_row += 1;
            }
            col
        }
        GridLine::Line(n) => (*n - 1).max(0) as usize,
        GridLine::Span(_) => 0,
    };
    
    let col_end = match &placement.column_end {
        GridLine::Auto => col_start + 1,
        GridLine::Line(n) => (*n - 1).max(col_start as i32 + 1) as usize,
        GridLine::Span(n) => col_start + (*n as usize),
    };
    
    let row_start = match &placement.row_start {
        GridLine::Auto => *auto_row,
        GridLine::Line(n) => (*n - 1).max(0) as usize,
        GridLine::Span(_) => 0,
    };
    
    let row_end = match &placement.row_end {
        GridLine::Auto => row_start + 1,
        GridLine::Line(n) => (*n - 1).max(row_start as i32 + 1) as usize,
        GridLine::Span(n) => row_start + (*n as usize),
    };
    
    GridArea {
        column_start: col_start.min(num_columns),
        column_end: col_end.min(num_columns + 1),
        row_start: row_start.min(num_rows),
        row_end: row_end.min(num_rows + 1),
    }
}

/// Layout children in a grid
pub fn layout_grid_children(
    tree: &mut LayoutTree,
    container_id: LayoutBoxId,
    context: &GridLayoutContext,
    placements: &[(LayoutBoxId, GridPlacement)],
) {
    let mut auto_column = 0;
    let mut auto_row = 0;
    
    for (child_id, placement) in placements {
        let area = resolve_placement(
            placement,
            context.num_columns(),
            context.num_rows(),
            &mut auto_column,
            &mut auto_row,
        );
        
        let (x, y, width, height) = context.get_area_bounds(&area);
        
        if let Some(child) = tree.get_mut(*child_id) {
            child.dimensions.content.x = x;
            child.dimensions.content.y = y;
            child.dimensions.content.width = width;
            child.dimensions.content.height = height;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_track_size_fixed() {
        let size = TrackSize::Length(100.0);
        assert_eq!(size.resolve(1000.0, 0.0), Some(100.0));
    }
    
    #[test]
    fn test_track_size_percentage() {
        let size = TrackSize::Percentage(50.0);
        assert_eq!(size.resolve(200.0, 0.0), Some(100.0));
    }
    
    #[test]
    fn test_track_size_fraction() {
        let size = TrackSize::Fraction(1.0);
        assert_eq!(size.resolve(0.0, 50.0), Some(50.0));
    }
    
    #[test]
    fn test_grid_template() {
        let template = GridTemplate::new()
            .with_columns(vec![
                TrackSize::Length(100.0),
                TrackSize::Fraction(1.0),
                TrackSize::Length(100.0),
            ])
            .with_gap(10.0);
        
        // Container: 400px, 2 gaps = 20px, 2 fixed columns = 200px
        // Remaining for 1fr = 400 - 20 - 200 = 180px
        let ctx = GridLayoutContext::new(400.0, 300.0, template);
        
        assert_eq!(ctx.num_columns(), 3);
        assert_eq!(ctx.column_sizes[0], 100.0);
        assert_eq!(ctx.column_sizes[2], 100.0);
        // The 1fr column gets remaining space
        let fr_size = ctx.column_sizes[1];
        assert!(fr_size > 0.0, "fr size should be positive: {}", fr_size);
    }
    
    #[test]
    fn test_grid_placement() {
        let placement = GridPlacement::cell(2, 1);
        let area = resolve_placement(&placement, 3, 3, &mut 0, &mut 0);
        
        assert_eq!(area.column_start, 1);
        assert_eq!(area.column_end, 2);
        assert_eq!(area.row_start, 0);
        assert_eq!(area.row_end, 1);
    }
    
    #[test]
    fn test_grid_area_bounds_no_gap() {
        let template = GridTemplate::new()
            .with_columns(vec![TrackSize::Length(100.0), TrackSize::Length(100.0)])
            .with_rows(vec![TrackSize::Length(50.0), TrackSize::Length(50.0)]);
        
        let ctx = GridLayoutContext::new(200.0, 100.0, template);
        
        let area = GridArea {
            column_start: 0,
            column_end: 1,
            row_start: 0,
            row_end: 1,
        };
        
        let (x, y, w, h) = ctx.get_area_bounds(&area);
        assert_eq!(x, 0.0);
        assert_eq!(y, 0.0);
        assert_eq!(w, 100.0);
        assert_eq!(h, 50.0);
    }
    
    #[test]
    fn test_grid_area_bounds_with_gap() {
        let template = GridTemplate::new()
            .with_columns(vec![TrackSize::Length(100.0), TrackSize::Length(100.0)])
            .with_rows(vec![TrackSize::Length(50.0)])
            .with_gap(10.0);
        
        let ctx = GridLayoutContext::new(210.0, 50.0, template);
        
        // Second column should start after first column + gap
        let area = GridArea {
            column_start: 1,
            column_end: 2,
            row_start: 0,
            row_end: 1,
        };
        
        let (x, y, w, h) = ctx.get_area_bounds(&area);
        assert_eq!(x, 110.0); // 100 + 10 gap
        assert_eq!(y, 0.0);
        assert_eq!(w, 100.0);
        assert_eq!(h, 50.0);
    }
}
