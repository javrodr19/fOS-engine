//! Visibility Culling
//!
//! Utilities for skipping layout and paint for invisible elements.
//!
//! - `display: none` → skip layout entirely
//! - `visibility: hidden` → skip paint (but still layout)
//! - Offscreen elements → skip until scroll brings them into view

/// Visibility state of an element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VisibilityState {
    /// Element is visible and in viewport.
    #[default]
    Visible,
    
    /// Element has `visibility: hidden` (layout but no paint).
    Hidden,
    
    /// Element has `display: none` (no layout, no paint).
    DisplayNone,
    
    /// Element is outside the viewport (can skip paint).
    Offscreen,
    
    /// Element is clipped by overflow (partially visible).
    Clipped,
    
    /// Element is fully occluded by other elements.
    Occluded,
}

impl VisibilityState {
    /// Should we run layout for this element?
    #[inline]
    pub fn should_layout(&self) -> bool {
        !matches!(self, VisibilityState::DisplayNone)
    }
    
    /// Should we paint this element?
    #[inline]
    pub fn should_paint(&self) -> bool {
        matches!(self, VisibilityState::Visible | VisibilityState::Clipped)
    }
    
    /// Is the element completely invisible?
    #[inline]
    pub fn is_invisible(&self) -> bool {
        !matches!(self, VisibilityState::Visible)
    }
    
    /// Is the element offscreen?
    #[inline]
    pub fn is_offscreen(&self) -> bool {
        matches!(self, VisibilityState::Offscreen)
    }
}

/// Viewport for visibility testing.
#[derive(Debug, Clone, Copy, Default)]
pub struct Viewport {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Viewport {
    /// Create a new viewport.
    #[inline]
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }
    
    /// Right edge.
    #[inline]
    pub fn right(&self) -> f32 {
        self.x + self.width
    }
    
    /// Bottom edge.
    #[inline]
    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }
    
    /// Check if a rectangle is fully inside the viewport.
    #[inline]
    pub fn contains(&self, x: f32, y: f32, w: f32, h: f32) -> bool {
        x >= self.x && 
        y >= self.y && 
        x + w <= self.right() && 
        y + h <= self.bottom()
    }
    
    /// Check if a rectangle intersects the viewport.
    #[inline]
    pub fn intersects(&self, x: f32, y: f32, w: f32, h: f32) -> bool {
        x < self.right() && 
        x + w > self.x && 
        y < self.bottom() && 
        y + h > self.y
    }
    
    /// Check if a rectangle is completely outside the viewport.
    #[inline]
    pub fn is_outside(&self, x: f32, y: f32, w: f32, h: f32) -> bool {
        !self.intersects(x, y, w, h)
    }
    
    /// Expand viewport by a margin (for prefetching).
    #[inline]
    pub fn expand(&self, margin: f32) -> Viewport {
        Viewport {
            x: self.x - margin,
            y: self.y - margin,
            width: self.width + margin * 2.0,
            height: self.height + margin * 2.0,
        }
    }
}

/// Element visibility info for culling decisions.
#[derive(Debug, Clone, Copy, Default)]
pub struct ElementVisibility {
    /// The element's bounds.
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    
    /// CSS display value (simplified).
    pub display: DisplayValue,
    
    /// CSS visibility value.
    pub visibility: VisibilityValue,
    
    /// CSS opacity value (0.0 to 1.0).
    pub opacity: f32,
}

/// Simplified CSS display values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DisplayValue {
    #[default]
    Block,
    Inline,
    Flex,
    Grid,
    None,
    Contents,
}

/// CSS visibility values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VisibilityValue {
    #[default]
    Visible,
    Hidden,
    Collapse,
}

impl ElementVisibility {
    /// Compute visibility state relative to viewport.
    pub fn compute_state(&self, viewport: &Viewport) -> VisibilityState {
        // display: none takes priority
        if self.display == DisplayValue::None {
            return VisibilityState::DisplayNone;
        }
        
        // visibility: hidden
        if self.visibility == VisibilityValue::Hidden {
            return VisibilityState::Hidden;
        }
        
        // Zero opacity is effectively hidden
        if self.opacity <= 0.0 {
            return VisibilityState::Hidden;
        }
        
        // Check if offscreen
        if viewport.is_outside(self.x, self.y, self.width, self.height) {
            return VisibilityState::Offscreen;
        }
        
        // Check if partially visible (clipped)
        if !viewport.contains(self.x, self.y, self.width, self.height) {
            return VisibilityState::Clipped;
        }
        
        VisibilityState::Visible
    }
    
    /// Quick check: should we layout this element?
    #[inline]
    pub fn should_layout(&self) -> bool {
        self.display != DisplayValue::None
    }
    
    /// Quick check: should we paint this element?
    #[inline]
    pub fn should_paint(&self, viewport: &Viewport) -> bool {
        if self.display == DisplayValue::None {
            return false;
        }
        if self.visibility == VisibilityValue::Hidden {
            return false;
        }
        if self.opacity <= 0.0 {
            return false;
        }
        viewport.intersects(self.x, self.y, self.width, self.height)
    }
}

/// Visibility culling context for a frame.
#[derive(Debug, Default)]
pub struct CullingContext {
    /// Current viewport.
    pub viewport: Viewport,
    
    /// Expanded viewport for prefetching.
    pub prefetch_viewport: Viewport,
    
    /// Number of elements skipped due to display:none.
    pub skipped_display_none: usize,
    
    /// Number of elements skipped due to offscreen.
    pub skipped_offscreen: usize,
    
    /// Number of elements skipped due to visibility:hidden.
    pub skipped_hidden: usize,
}

impl CullingContext {
    /// Create a new culling context.
    pub fn new(viewport: Viewport) -> Self {
        Self {
            viewport,
            prefetch_viewport: viewport.expand(100.0), // 100px margin
            skipped_display_none: 0,
            skipped_offscreen: 0,
            skipped_hidden: 0,
        }
    }
    
    /// Check if an element should be laid out.
    pub fn should_layout(&mut self, elem: &ElementVisibility) -> bool {
        if !elem.should_layout() {
            self.skipped_display_none += 1;
            return false;
        }
        true
    }
    
    /// Check if an element should be painted.
    pub fn should_paint(&mut self, elem: &ElementVisibility) -> bool {
        if elem.display == DisplayValue::None {
            self.skipped_display_none += 1;
            return false;
        }
        if elem.visibility == VisibilityValue::Hidden {
            self.skipped_hidden += 1;
            return false;
        }
        if !self.viewport.intersects(elem.x, elem.y, elem.width, elem.height) {
            self.skipped_offscreen += 1;
            return false;
        }
        true
    }
    
    /// Reset statistics.
    pub fn reset_stats(&mut self) {
        self.skipped_display_none = 0;
        self.skipped_offscreen = 0;
        self.skipped_hidden = 0;
    }
    
    /// Total elements skipped.
    pub fn total_skipped(&self) -> usize {
        self.skipped_display_none + self.skipped_offscreen + self.skipped_hidden
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_visibility_state() {
        assert!(VisibilityState::Visible.should_layout());
        assert!(VisibilityState::Visible.should_paint());
        
        assert!(VisibilityState::Hidden.should_layout());
        assert!(!VisibilityState::Hidden.should_paint());
        
        assert!(!VisibilityState::DisplayNone.should_layout());
        assert!(!VisibilityState::DisplayNone.should_paint());
        
        assert!(VisibilityState::Offscreen.should_layout());
        assert!(!VisibilityState::Offscreen.should_paint());
    }
    
    #[test]
    fn test_viewport_intersects() {
        let vp = Viewport::new(0.0, 0.0, 100.0, 100.0);
        
        // Fully inside
        assert!(vp.intersects(10.0, 10.0, 20.0, 20.0));
        
        // Partially inside
        assert!(vp.intersects(-10.0, -10.0, 20.0, 20.0));
        
        // Fully outside
        assert!(!vp.intersects(200.0, 200.0, 20.0, 20.0));
    }
    
    #[test]
    fn test_element_visibility() {
        let vp = Viewport::new(0.0, 0.0, 800.0, 600.0);
        
        // Visible element
        let visible = ElementVisibility {
            x: 100.0,
            y: 100.0,
            width: 50.0,
            height: 50.0,
            display: DisplayValue::Block,
            visibility: VisibilityValue::Visible,
            opacity: 1.0,
        };
        assert_eq!(visible.compute_state(&vp), VisibilityState::Visible);
        
        // Display none
        let none = ElementVisibility {
            display: DisplayValue::None,
            ..visible
        };
        assert_eq!(none.compute_state(&vp), VisibilityState::DisplayNone);
        
        // Offscreen
        let offscreen = ElementVisibility {
            x: 1000.0,
            y: 1000.0,
            ..visible
        };
        assert_eq!(offscreen.compute_state(&vp), VisibilityState::Offscreen);
    }
}
