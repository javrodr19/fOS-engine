//! Lazy Layout
//!
//! Viewport-based lazy layout computation.

use crate::Rect;

/// Viewport for lazy layout
#[derive(Debug, Clone, Copy, Default)]
pub struct Viewport {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Viewport {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }
    
    /// Check if a rectangle intersects the viewport
    pub fn intersects(&self, rect: &Rect) -> bool {
        !(rect.x + rect.width < self.x ||
          rect.x > self.x + self.width ||
          rect.y + rect.height < self.y ||
          rect.y > self.y + self.height)
    }
    
    /// Check if a rectangle is fully visible
    pub fn contains(&self, rect: &Rect) -> bool {
        rect.x >= self.x &&
        rect.y >= self.y &&
        rect.x + rect.width <= self.x + self.width &&
        rect.y + rect.height <= self.y + self.height
    }
    
    /// Expand viewport by margin (for pre-loading)
    pub fn expand(&self, margin: f32) -> Self {
        Self {
            x: self.x - margin,
            y: self.y - margin,
            width: self.width + margin * 2.0,
            height: self.height + margin * 2.0,
        }
    }
}

/// Lazy layout result
#[derive(Debug, Default)]
pub struct LazyLayoutResult {
    /// Boxes that need to be laid out
    pub visible_boxes: Vec<usize>,
    /// Boxes that can be skipped
    pub skipped_boxes: usize,
    /// Total boxes processed
    pub total_boxes: usize,
}

impl LazyLayoutResult {
    pub fn efficiency(&self) -> f64 {
        if self.total_boxes == 0 {
            1.0
        } else {
            self.skipped_boxes as f64 / self.total_boxes as f64
        }
    }
}

/// Compute which boxes need layout based on viewport
/// Takes a slice of content boxes
pub fn compute_visible_boxes_from_rects(rects: &[Rect], viewport: &Viewport) -> LazyLayoutResult {
    let mut result = LazyLayoutResult::default();
    let expanded = viewport.expand(200.0); // Pre-load 200px margin
    
    for (idx, rect) in rects.iter().enumerate() {
        result.total_boxes += 1;
        
        if expanded.intersects(rect) {
            result.visible_boxes.push(idx);
        } else {
            result.skipped_boxes += 1;
        }
    }
    
    result
}

/// Layout priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LayoutPriority {
    /// Must layout immediately (in viewport)
    Critical = 0,
    /// Should layout soon (near viewport)
    High = 1,
    /// Can defer (far from viewport)
    Low = 2,
    /// Can skip (invisible)
    Skip = 3,
}

/// Get layout priority for a box
pub fn get_priority(rect: &Rect, viewport: &Viewport) -> LayoutPriority {
    if viewport.intersects(rect) {
        LayoutPriority::Critical
    } else {
        let expanded = viewport.expand(500.0);
        if expanded.intersects(rect) {
            LayoutPriority::High
        } else {
            let far_expanded = viewport.expand(2000.0);
            if far_expanded.intersects(rect) {
                LayoutPriority::Low
            } else {
                LayoutPriority::Skip
            }
        }
    }
}

/// Incremental layout state
#[derive(Debug, Default)]
pub struct IncrementalLayout {
    /// Boxes that need relayout
    dirty_boxes: Vec<usize>,
    /// Next box to process
    next_index: usize,
}

impl IncrementalLayout {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Mark a box as needing layout
    pub fn mark_dirty(&mut self, box_index: usize) {
        if !self.dirty_boxes.contains(&box_index) {
            self.dirty_boxes.push(box_index);
        }
    }
    
    /// Get next dirty box to process
    pub fn next_dirty(&mut self) -> Option<usize> {
        if self.next_index < self.dirty_boxes.len() {
            let idx = self.dirty_boxes[self.next_index];
            self.next_index += 1;
            Some(idx)
        } else {
            None
        }
    }
    
    /// Process N dirty boxes
    pub fn process_batch(&mut self, count: usize) -> Vec<usize> {
        let mut batch = Vec::with_capacity(count);
        for _ in 0..count {
            if let Some(idx) = self.next_dirty() {
                batch.push(idx);
            } else {
                break;
            }
        }
        batch
    }
    
    /// Check if layout is complete
    pub fn is_complete(&self) -> bool {
        self.next_index >= self.dirty_boxes.len()
    }
    
    /// Reset incremental state
    pub fn reset(&mut self) {
        self.dirty_boxes.clear();
        self.next_index = 0;
    }
    
    /// Get progress
    pub fn progress(&self) -> f64 {
        if self.dirty_boxes.is_empty() {
            1.0
        } else {
            self.next_index as f64 / self.dirty_boxes.len() as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_viewport_intersects() {
        let viewport = Viewport::new(0.0, 0.0, 800.0, 600.0);
        
        // Fully inside
        let rect = Rect { x: 100.0, y: 100.0, width: 200.0, height: 200.0 };
        assert!(viewport.intersects(&rect));
        
        // Partially overlapping
        let rect = Rect { x: -50.0, y: -50.0, width: 100.0, height: 100.0 };
        assert!(viewport.intersects(&rect));
        
        // Outside
        let rect = Rect { x: 1000.0, y: 1000.0, width: 100.0, height: 100.0 };
        assert!(!viewport.intersects(&rect));
    }
    
    #[test]
    fn test_viewport_contains() {
        let viewport = Viewport::new(0.0, 0.0, 800.0, 600.0);
        
        // Fully inside
        let rect = Rect { x: 100.0, y: 100.0, width: 200.0, height: 200.0 };
        assert!(viewport.contains(&rect));
        
        // Partially overlapping (not contained)
        let rect = Rect { x: 700.0, y: 0.0, width: 200.0, height: 100.0 };
        assert!(!viewport.contains(&rect));
    }
    
    #[test]
    fn test_viewport_expand() {
        let viewport = Viewport::new(100.0, 100.0, 400.0, 300.0);
        let expanded = viewport.expand(50.0);
        
        assert_eq!(expanded.x, 50.0);
        assert_eq!(expanded.y, 50.0);
        assert_eq!(expanded.width, 500.0);
        assert_eq!(expanded.height, 400.0);
    }
    
    #[test]
    fn test_layout_priority() {
        let viewport = Viewport::new(0.0, 0.0, 800.0, 600.0);
        
        // In viewport
        let rect = Rect { x: 100.0, y: 100.0, width: 100.0, height: 100.0 };
        assert_eq!(get_priority(&rect, &viewport), LayoutPriority::Critical);
        
        // Near viewport
        let rect = Rect { x: 900.0, y: 100.0, width: 100.0, height: 100.0 };
        assert_eq!(get_priority(&rect, &viewport), LayoutPriority::High);
        
        // Far from viewport
        let rect = Rect { x: 2000.0, y: 2000.0, width: 100.0, height: 100.0 };
        assert_eq!(get_priority(&rect, &viewport), LayoutPriority::Low);
        
        // Very far
        let rect = Rect { x: 10000.0, y: 10000.0, width: 100.0, height: 100.0 };
        assert_eq!(get_priority(&rect, &viewport), LayoutPriority::Skip);
    }
    
    #[test]
    fn test_incremental_layout() {
        let mut inc = IncrementalLayout::new();
        
        inc.mark_dirty(0);
        inc.mark_dirty(1);
        inc.mark_dirty(2);
        
        assert_eq!(inc.progress(), 0.0);
        
        let batch = inc.process_batch(2);
        assert_eq!(batch, vec![0, 1]);
        assert!(!inc.is_complete());
        
        let batch = inc.process_batch(2);
        assert_eq!(batch, vec![2]);
        assert!(inc.is_complete());
    }
}
