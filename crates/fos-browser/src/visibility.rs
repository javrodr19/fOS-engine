//! Visibility Culling Integration
//!
//! Skip layout/paint for invisible or offscreen elements.

/// Visibility state of an element
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VisibilityState {
    #[default]
    Visible,
    Hidden,       // visibility: hidden
    DisplayNone,  // display: none
    Offscreen,    // Outside viewport
    Clipped,      // Partially visible
}

impl VisibilityState {
    pub fn should_layout(&self) -> bool { !matches!(self, Self::DisplayNone) }
    pub fn should_paint(&self) -> bool { matches!(self, Self::Visible | Self::Clipped) }
    pub fn is_offscreen(&self) -> bool { matches!(self, Self::Offscreen) }
}

/// Viewport for visibility testing
#[derive(Debug, Clone, Copy, Default)]
pub struct Viewport {
    pub x: f32, pub y: f32, pub width: f32, pub height: f32,
}

impl Viewport {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }
    
    pub fn right(&self) -> f32 { self.x + self.width }
    pub fn bottom(&self) -> f32 { self.y + self.height }
    
    pub fn intersects(&self, x: f32, y: f32, w: f32, h: f32) -> bool {
        x < self.right() && x + w > self.x && y < self.bottom() && y + h > self.y
    }
    
    pub fn is_outside(&self, x: f32, y: f32, w: f32, h: f32) -> bool {
        !self.intersects(x, y, w, h)
    }
    
    pub fn expand(&self, margin: f32) -> Viewport {
        Viewport {
            x: self.x - margin, y: self.y - margin,
            width: self.width + margin * 2.0, height: self.height + margin * 2.0,
        }
    }
}

/// Element visibility info
#[derive(Debug, Clone, Copy, Default)]
pub struct ElementVisibility {
    pub x: f32, pub y: f32, pub width: f32, pub height: f32,
    pub display_none: bool,
    pub visibility_hidden: bool,
    pub opacity: f32,
}

impl ElementVisibility {
    pub fn compute_state(&self, viewport: &Viewport) -> VisibilityState {
        if self.display_none { return VisibilityState::DisplayNone; }
        if self.visibility_hidden { return VisibilityState::Hidden; }
        if self.opacity <= 0.0 { return VisibilityState::Hidden; }
        if viewport.is_outside(self.x, self.y, self.width, self.height) {
            return VisibilityState::Offscreen;
        }
        VisibilityState::Visible
    }
    
    pub fn should_paint(&self, viewport: &Viewport) -> bool {
        !self.display_none && !self.visibility_hidden && self.opacity > 0.0 &&
        viewport.intersects(self.x, self.y, self.width, self.height)
    }
}

/// Culling context with statistics
#[derive(Debug, Default)]
pub struct CullingContext {
    pub viewport: Viewport,
    pub skipped_display_none: usize,
    pub skipped_offscreen: usize,
    pub skipped_hidden: usize,
}

impl CullingContext {
    pub fn new(viewport: Viewport) -> Self {
        Self { viewport, skipped_display_none: 0, skipped_offscreen: 0, skipped_hidden: 0 }
    }
    
    pub fn should_paint(&mut self, elem: &ElementVisibility) -> bool {
        if elem.display_none { self.skipped_display_none += 1; return false; }
        if elem.visibility_hidden { self.skipped_hidden += 1; return false; }
        if !self.viewport.intersects(elem.x, elem.y, elem.width, elem.height) {
            self.skipped_offscreen += 1; return false;
        }
        true
    }
    
    pub fn total_skipped(&self) -> usize {
        self.skipped_display_none + self.skipped_offscreen + self.skipped_hidden
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_visibility_state() {
        assert!(VisibilityState::Visible.should_paint());
        assert!(!VisibilityState::Hidden.should_paint());
        assert!(!VisibilityState::DisplayNone.should_layout());
    }
    
    #[test]
    fn test_viewport_intersects() {
        let vp = Viewport::new(0.0, 0.0, 100.0, 100.0);
        assert!(vp.intersects(10.0, 10.0, 20.0, 20.0));
        assert!(!vp.intersects(200.0, 200.0, 20.0, 20.0));
    }
    
    #[test]
    fn test_culling() {
        let vp = Viewport::new(0.0, 0.0, 800.0, 600.0);
        let mut ctx = CullingContext::new(vp);
        
        let visible = ElementVisibility { x: 100.0, y: 100.0, width: 50.0, height: 50.0, opacity: 1.0, ..Default::default() };
        let offscreen = ElementVisibility { x: 1000.0, y: 1000.0, width: 50.0, height: 50.0, opacity: 1.0, ..Default::default() };
        
        assert!(ctx.should_paint(&visible));
        assert!(!ctx.should_paint(&offscreen));
        assert_eq!(ctx.skipped_offscreen, 1);
    }
}
