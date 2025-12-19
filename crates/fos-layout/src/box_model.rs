//! CSS Box Model
//!
//! Implements the CSS box model with content, padding, border, margin areas.

/// Box dimensions - the complete box model for a layout box
#[derive(Debug, Clone, Copy, Default)]
pub struct BoxDimensions {
    /// Content area rectangle
    pub content: Rect,
    /// Padding sizes
    pub padding: EdgeSizes,
    /// Border widths
    pub border: EdgeSizes,
    /// Margin sizes
    pub margin: EdgeSizes,
}

/// Rectangle with position and size
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Edge sizes (top, right, bottom, left)
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EdgeSizes {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Rect {
    /// Create a new rectangle
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }
    
    /// Create a zero-sized rect at origin
    pub fn zero() -> Self {
        Self::default()
    }
    
    /// Right edge x coordinate
    #[inline]
    pub fn right(&self) -> f32 {
        self.x + self.width
    }
    
    /// Bottom edge y coordinate
    #[inline]
    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }
    
    /// Check if a point is inside this rectangle
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.right() && y >= self.y && y <= self.bottom()
    }
    
    /// Check if this rectangle intersects another
    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.right() && self.right() > other.x &&
        self.y < other.bottom() && self.bottom() > other.y
    }
    
    /// Expand this rect by given amounts
    pub fn expand(&self, top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self {
            x: self.x - left,
            y: self.y - top,
            width: self.width + left + right,
            height: self.height + top + bottom,
        }
    }
    
    /// Shrink this rect by given amounts
    pub fn shrink(&self, top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self {
            x: self.x + left,
            y: self.y + top,
            width: (self.width - left - right).max(0.0),
            height: (self.height - top - bottom).max(0.0),
        }
    }
}

impl EdgeSizes {
    /// Create uniform edge sizes
    pub fn all(size: f32) -> Self {
        Self { top: size, right: size, bottom: size, left: size }
    }
    
    /// Create edge sizes from (vertical, horizontal)
    pub fn symmetric(vertical: f32, horizontal: f32) -> Self {
        Self { top: vertical, right: horizontal, bottom: vertical, left: horizontal }
    }
    
    /// Total horizontal size (left + right)
    #[inline]
    pub fn horizontal(&self) -> f32 {
        self.left + self.right
    }
    
    /// Total vertical size (top + bottom)
    #[inline]
    pub fn vertical(&self) -> f32 {
        self.top + self.bottom
    }
    
    /// Check if all edges are zero
    pub fn is_zero(&self) -> bool {
        self.top == 0.0 && self.right == 0.0 && self.bottom == 0.0 && self.left == 0.0
    }
}

impl BoxDimensions {
    /// Create empty box dimensions
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Get the content area
    #[inline]
    pub fn content_box(&self) -> Rect {
        self.content
    }
    
    /// Get the area covered by content + padding
    pub fn padding_box(&self) -> Rect {
        self.content.expand(
            self.padding.top,
            self.padding.right,
            self.padding.bottom,
            self.padding.left,
        )
    }
    
    /// Get the area covered by content + padding + border
    pub fn border_box(&self) -> Rect {
        let padding = self.padding_box();
        padding.expand(
            self.border.top,
            self.border.right,
            self.border.bottom,
            self.border.left,
        )
    }
    
    /// Get the area covered by content + padding + border + margin
    pub fn margin_box(&self) -> Rect {
        let border = self.border_box();
        border.expand(
            self.margin.top,
            self.margin.right,
            self.margin.bottom,
            self.margin.left,
        )
    }
    
    /// Total width including all boxes
    pub fn total_width(&self) -> f32 {
        self.content.width + 
        self.padding.horizontal() + 
        self.border.horizontal() + 
        self.margin.horizontal()
    }
    
    /// Total height including all boxes
    pub fn total_height(&self) -> f32 {
        self.content.height + 
        self.padding.vertical() + 
        self.border.vertical() + 
        self.margin.vertical()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_rect_contains() {
        let rect = Rect::new(10.0, 10.0, 100.0, 50.0);
        assert!(rect.contains(50.0, 30.0));
        assert!(rect.contains(10.0, 10.0)); // top-left corner
        assert!(rect.contains(110.0, 60.0)); // bottom-right corner
        assert!(!rect.contains(5.0, 30.0)); // left of rect
        assert!(!rect.contains(50.0, 5.0)); // above rect
    }
    
    #[test]
    fn test_rect_intersects() {
        let a = Rect::new(0.0, 0.0, 100.0, 100.0);
        let b = Rect::new(50.0, 50.0, 100.0, 100.0);
        let c = Rect::new(200.0, 200.0, 50.0, 50.0);
        
        assert!(a.intersects(&b));
        assert!(b.intersects(&a));
        assert!(!a.intersects(&c));
    }
    
    #[test]
    fn test_box_dimensions() {
        let mut dims = BoxDimensions::new();
        dims.content = Rect::new(100.0, 100.0, 200.0, 100.0);
        dims.padding = EdgeSizes::all(10.0);
        dims.border = EdgeSizes::all(2.0);
        dims.margin = EdgeSizes::all(20.0);
        
        let padding_box = dims.padding_box();
        assert_eq!(padding_box.x, 90.0);
        assert_eq!(padding_box.width, 220.0);
        
        let border_box = dims.border_box();
        assert_eq!(border_box.x, 88.0);
        assert_eq!(border_box.width, 224.0);
        
        let margin_box = dims.margin_box();
        assert_eq!(margin_box.x, 68.0);
        assert_eq!(margin_box.width, 264.0);
    }
    
    #[test]
    fn test_edge_sizes() {
        let edges = EdgeSizes::all(10.0);
        assert_eq!(edges.horizontal(), 20.0);
        assert_eq!(edges.vertical(), 20.0);
        
        let sym = EdgeSizes::symmetric(5.0, 10.0);
        assert_eq!(sym.top, 5.0);
        assert_eq!(sym.right, 10.0);
        assert_eq!(sym.vertical(), 10.0);
        assert_eq!(sym.horizontal(), 20.0);
    }
}
