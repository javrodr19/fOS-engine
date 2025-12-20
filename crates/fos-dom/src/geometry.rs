//! Geometry APIs
//!
//! DOMRect, getBoundingClientRect, and scroll/offset properties.

/// DOMRect - rectangle geometry
#[derive(Debug, Clone, Copy, Default)]
pub struct DOMRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl DOMRect {
    /// Create empty rect
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Create with dimensions
    pub fn from_xywh(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self { x, y, width, height }
    }
    
    /// Top edge (same as y)
    pub fn top(&self) -> f64 {
        self.y
    }
    
    /// Right edge
    pub fn right(&self) -> f64 {
        self.x + self.width
    }
    
    /// Bottom edge
    pub fn bottom(&self) -> f64 {
        self.y + self.height
    }
    
    /// Left edge (same as x)
    pub fn left(&self) -> f64 {
        self.x
    }
    
    /// Check if point is inside
    pub fn contains_point(&self, x: f64, y: f64) -> bool {
        x >= self.x && x <= self.right() && y >= self.y && y <= self.bottom()
    }
    
    /// Check if rects intersect
    pub fn intersects(&self, other: &DOMRect) -> bool {
        !(self.right() < other.x || 
          self.x > other.right() ||
          self.bottom() < other.y ||
          self.y > other.bottom())
    }
    
    /// Get intersection rect
    pub fn intersection(&self, other: &DOMRect) -> Option<DOMRect> {
        if !self.intersects(other) {
            return None;
        }
        
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        
        Some(DOMRect::from_xywh(x, y, right - x, bottom - y))
    }
}

/// DOMRectList - collection of rects
#[derive(Debug, Clone, Default)]
pub struct DOMRectList {
    rects: Vec<DOMRect>,
}

impl DOMRectList {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn length(&self) -> usize {
        self.rects.len()
    }
    
    pub fn item(&self, index: usize) -> Option<&DOMRect> {
        self.rects.get(index)
    }
    
    pub fn push(&mut self, rect: DOMRect) {
        self.rects.push(rect);
    }
}

/// Element geometry state
#[derive(Debug, Clone, Default)]
pub struct ElementGeometry {
    // Offset properties (relative to offsetParent)
    pub offset_top: f64,
    pub offset_left: f64,
    pub offset_width: f64,
    pub offset_height: f64,
    pub offset_parent: Option<u32>,
    
    // Client properties (content + padding, no scrollbar)
    pub client_top: f64,
    pub client_left: f64,
    pub client_width: f64,
    pub client_height: f64,
    
    // Scroll properties
    pub scroll_top: f64,
    pub scroll_left: f64,
    pub scroll_width: f64,
    pub scroll_height: f64,
}

impl ElementGeometry {
    /// Get bounding client rect
    pub fn bounding_client_rect(&self) -> DOMRect {
        // In real impl, this would account for transforms and viewport
        DOMRect::from_xywh(
            self.offset_left,
            self.offset_top,
            self.offset_width,
            self.offset_height,
        )
    }
    
    /// Scroll to position
    pub fn scroll_to(&mut self, x: f64, y: f64) {
        self.scroll_left = x.max(0.0).min(self.scroll_width - self.client_width);
        self.scroll_top = y.max(0.0).min(self.scroll_height - self.client_height);
    }
    
    /// Scroll by amount
    pub fn scroll_by(&mut self, dx: f64, dy: f64) {
        self.scroll_to(self.scroll_left + dx, self.scroll_top + dy);
    }
}

/// Scroll behavior
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ScrollBehavior {
    #[default]
    Auto,
    Smooth,
}

/// Scroll options
#[derive(Debug, Clone, Default)]
pub struct ScrollOptions {
    pub top: Option<f64>,
    pub left: Option<f64>,
    pub behavior: ScrollBehavior,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_dom_rect() {
        let rect = DOMRect::from_xywh(10.0, 20.0, 100.0, 50.0);
        
        assert_eq!(rect.top(), 20.0);
        assert_eq!(rect.right(), 110.0);
        assert_eq!(rect.bottom(), 70.0);
        assert_eq!(rect.left(), 10.0);
    }
    
    #[test]
    fn test_contains_point() {
        let rect = DOMRect::from_xywh(0.0, 0.0, 100.0, 100.0);
        
        assert!(rect.contains_point(50.0, 50.0));
        assert!(!rect.contains_point(150.0, 50.0));
    }
    
    #[test]
    fn test_intersects() {
        let rect1 = DOMRect::from_xywh(0.0, 0.0, 100.0, 100.0);
        let rect2 = DOMRect::from_xywh(50.0, 50.0, 100.0, 100.0);
        let rect3 = DOMRect::from_xywh(200.0, 200.0, 50.0, 50.0);
        
        assert!(rect1.intersects(&rect2));
        assert!(!rect1.intersects(&rect3));
    }
}
