//! Painter - paints layout boxes to canvas
//!
//! The painter traverses the layout tree and paints each box:
//! 1. Box shadow (if any)
//! 2. Background (respecting border-radius)
//! 3. Border
//! 4. Content (text, images)
//! 5. Children (recursive)
//! 6. Apply opacity (if < 1.0)

use crate::{Canvas, Color};
use crate::paint::{Border, BorderRadius, BorderStyle};
use crate::background::{Background, paint_background};
use crate::border::paint_border;
use crate::effects::{BoxShadow, Overflow, paint_box_shadow, apply_opacity};
use crate::transform::{Transform2D, TransformOrigin};
use fos_layout::{LayoutTree, LayoutBoxId, BoxType};
use fos_css::properties::Color as CssColor;

/// Painter for rendering layout trees
pub struct Painter {
    canvas: Canvas,
}

impl Painter {
    /// Create a new painter with given dimensions
    pub fn new(width: u32, height: u32) -> Option<Self> {
        Canvas::new(width, height).map(|canvas| Self { canvas })
    }
    
    /// Get the canvas
    pub fn canvas(&self) -> &Canvas {
        &self.canvas
    }
    
    /// Get mutable canvas
    pub fn canvas_mut(&mut self) -> &mut Canvas {
        &mut self.canvas
    }
    
    /// Clear with background color
    pub fn clear(&mut self, color: Color) {
        self.canvas.clear(color);
    }
    
    /// Paint an entire layout tree
    pub fn paint_tree(&mut self, tree: &LayoutTree, styles: &BoxStyles) {
        if let Some(root) = tree.root() {
            self.paint_box(tree, root, styles);
        }
    }
    
    /// Paint a single box and its children
    fn paint_box(&mut self, tree: &LayoutTree, box_id: LayoutBoxId, styles: &BoxStyles) {
        let layout_box = match tree.get(box_id) {
            Some(b) => b,
            None => return,
        };
        
        let dims = &layout_box.dimensions;
        let border_box = dims.border_box();
        
        // Get style for this box
        let style = styles.get(box_id).cloned().unwrap_or_else(|| {
            BoxStyle { opacity: 1.0, ..Default::default() }
        });
        
        // 1. Paint box shadow (before background)
        if let Some(ref shadow) = style.box_shadow {
            paint_box_shadow(
                &mut self.canvas,
                border_box.x,
                border_box.y,
                border_box.width,
                border_box.height,
                shadow,
            );
        }
        
        // 2. Paint background
        if style.background.is_visible() {
            paint_background(
                &mut self.canvas,
                border_box.x,
                border_box.y,
                border_box.width,
                border_box.height,
                &style.background,
                &style.border_radius,
            );
        }
        
        // 3. Paint border
        if style.border.has_visible() {
            paint_border(
                &mut self.canvas,
                border_box.x,
                border_box.y,
                border_box.width,
                border_box.height,
                &style.border,
                &style.border_radius,
            );
        }
        
        // 4. Content is painted as needed (text, images - future)
        
        // 5. Paint children
        for (child_id, _) in tree.children(box_id) {
            self.paint_box(tree, child_id, styles);
        }
        
        // 6. Apply opacity to this box region (if < 1.0)
        if style.opacity < 1.0 {
            apply_opacity(
                &mut self.canvas,
                border_box.x,
                border_box.y,
                border_box.width,
                border_box.height,
                style.opacity,
            );
        }
    }
    
    /// Simple method: paint a colored rectangle
    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color) {
        self.canvas.fill_rect(x, y, w, h, color);
    }
}

/// Style information for painting boxes
#[derive(Debug, Clone, Default)]
pub struct BoxStyle {
    pub background: Background,
    pub border: Border,
    pub border_radius: BorderRadius,
    pub color: Color, // Text color
    /// Box shadow(s)
    pub box_shadow: Option<BoxShadow>,
    /// Opacity (0.0 = transparent, 1.0 = opaque)
    pub opacity: f32,
    /// Overflow behavior
    pub overflow: Overflow,
    /// CSS transform
    pub transform: Option<Transform2D>,
    /// Transform origin
    pub transform_origin: TransformOrigin,
}

impl BoxStyle {
    /// Create with just a background color
    pub fn with_background(color: Color) -> Self {
        Self {
            background: Background::color(color),
            opacity: 1.0,
            ..Default::default()
        }
    }
    
    /// Create with border
    pub fn with_border(width: f32, color: Color) -> Self {
        Self {
            border: Border::all(width, BorderStyle::Solid, color),
            opacity: 1.0,
            ..Default::default()
        }
    }
    
    /// Add a box shadow
    pub fn with_shadow(mut self, shadow: BoxShadow) -> Self {
        self.box_shadow = Some(shadow);
        self
    }
    
    /// Set opacity
    pub fn with_opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity.clamp(0.0, 1.0);
        self
    }
    
    /// Set transform
    pub fn with_transform(mut self, transform: Transform2D) -> Self {
        self.transform = Some(transform);
        self
    }
    
    /// Set transform origin
    pub fn with_transform_origin(mut self, origin: TransformOrigin) -> Self {
        self.transform_origin = origin;
        self
    }
}

/// Collection of styles for all boxes
#[derive(Debug, Default)]
pub struct BoxStyles {
    styles: std::collections::HashMap<LayoutBoxId, BoxStyle>,
}

impl BoxStyles {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Set style for a box
    pub fn set(&mut self, id: LayoutBoxId, style: BoxStyle) {
        self.styles.insert(id, style);
    }
    
    /// Get style for a box
    pub fn get(&self, id: LayoutBoxId) -> Option<&BoxStyle> {
        self.styles.get(&id)
    }
}

/// Convert CSS color to render color
pub fn css_color_to_render(css: &CssColor) -> Color {
    Color::rgba(css.r, css.g, css.b, css.a)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fos_layout::{BoxType, layout_block_tree};
    
    #[test]
    fn test_painter_create() {
        let painter = Painter::new(800, 600);
        assert!(painter.is_some());
    }
    
    #[test]
    fn test_paint_simple_tree() {
        let mut painter = Painter::new(400, 300).unwrap();
        painter.clear(Color::WHITE);
        
        // Create a simple layout tree
        let mut tree = fos_layout::LayoutTree::new();
        let root = tree.create_box(BoxType::Block, None);
        tree.set_root(root);
        
        let child = tree.create_box(BoxType::Block, None);
        tree.append_child(root, child);
        
        if let Some(b) = tree.get_mut(child) {
            b.dimensions.content.height = 100.0;
        }
        
        layout_block_tree(&mut tree, 400.0, 300.0);
        
        // Set styles
        let mut styles = BoxStyles::new();
        styles.set(child, BoxStyle::with_background(Color::rgb(200, 100, 50)));
        
        // Paint
        painter.paint_tree(&tree, &styles);
        
        // Verify (center of child box should have background color)
        let pixel = painter.canvas().get_pixel(200, 50).unwrap();
        assert_eq!(pixel.r, 200);
        assert_eq!(pixel.g, 100);
        assert_eq!(pixel.b, 50);
    }
    
    #[test]
    fn test_paint_with_border() {
        let mut painter = Painter::new(200, 200).unwrap();
        painter.clear(Color::WHITE);
        
        let mut tree = fos_layout::LayoutTree::new();
        let root = tree.create_box(BoxType::Block, None);
        tree.set_root(root);
        
        if let Some(b) = tree.get_mut(root) {
            b.dimensions.content = fos_layout::Rect::new(10.0, 10.0, 100.0, 100.0);
        }
        
        let mut styles = BoxStyles::new();
        let mut style = BoxStyle::with_background(Color::rgb(100, 150, 200));
        style.border = Border::all(3.0, BorderStyle::Solid, Color::BLACK);
        styles.set(root, style);
        
        painter.paint_tree(&tree, &styles);
        
        // Should have painted without errors
    }
}
