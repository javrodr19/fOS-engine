//! CSS Box Model

/// Box dimensions
#[derive(Debug, Clone, Copy, Default)]
pub struct BoxDimensions {
    pub content: Rect,
    pub padding: EdgeSizes,
    pub border: EdgeSizes,
    pub margin: EdgeSizes,
}

/// Rectangle
#[derive(Debug, Clone, Copy, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Edge sizes (top, right, bottom, left)
#[derive(Debug, Clone, Copy, Default)]
pub struct EdgeSizes {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl BoxDimensions {
    /// Get the area covered by content + padding
    pub fn padding_box(&self) -> Rect {
        Rect {
            x: self.content.x - self.padding.left,
            y: self.content.y - self.padding.top,
            width: self.content.width + self.padding.left + self.padding.right,
            height: self.content.height + self.padding.top + self.padding.bottom,
        }
    }
    
    /// Get the area covered by content + padding + border
    pub fn border_box(&self) -> Rect {
        let padding = self.padding_box();
        Rect {
            x: padding.x - self.border.left,
            y: padding.y - self.border.top,
            width: padding.width + self.border.left + self.border.right,
            height: padding.height + self.border.top + self.border.bottom,
        }
    }
    
    /// Get the area covered by content + padding + border + margin
    pub fn margin_box(&self) -> Rect {
        let border = self.border_box();
        Rect {
            x: border.x - self.margin.left,
            y: border.y - self.margin.top,
            width: border.width + self.margin.left + self.margin.right,
            height: border.height + self.margin.top + self.margin.bottom,
        }
    }
}
