//! Canvas Integration
//!
//! Integrates fos-canvas for <canvas> element support.

use std::collections::HashMap;
use fos_dom::{Document, DomTree, NodeId};
use fos_canvas::{
    CanvasRenderingContext2D, Color,
    FillStyle, StrokeStyle,
};

/// Canvas manager for the browser
pub struct CanvasManager {
    /// Canvas instances by node ID
    canvases: HashMap<u64, CanvasInstance>,
    /// Next canvas ID
    next_id: u64,
}

/// Canvas element instance
#[derive(Debug)]
pub struct CanvasInstance {
    pub id: u64,
    pub context: CanvasRenderingContext2D,
    pub bounds: CanvasBounds,
    pub context_type: CanvasContextType,
}

/// Canvas bounds for rendering
#[derive(Debug, Clone, Default)]
pub struct CanvasBounds {
    pub x: f32,
    pub y: f32,
    pub width: u32,
    pub height: u32,
}

/// Canvas context type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasContextType {
    Context2D,
    WebGL,
    WebGL2,
    BitmapRenderer,
}

impl Default for CanvasContextType {
    fn default() -> Self {
        CanvasContextType::Context2D
    }
}

impl CanvasManager {
    /// Create new canvas manager
    pub fn new() -> Self {
        Self {
            canvases: HashMap::new(),
            next_id: 1,
        }
    }
    
    /// Extract canvas elements from DOM
    pub fn extract_from_document(&mut self, document: &Document) {
        self.canvases.clear();
        
        let tree = document.tree();
        self.scan_tree(tree, tree.root());
        
        log::debug!("Found {} canvas elements", self.canvases.len());
    }
    
    /// Recursively scan DOM tree for canvas elements
    fn scan_tree(&mut self, tree: &DomTree, node_id: NodeId) {
        if !node_id.is_valid() {
            return;
        }
        
        if let Some(node) = tree.get(node_id) {
            if let Some(element) = node.as_element() {
                let tag = tree.resolve(element.name.local).to_lowercase();
                
                if tag == "canvas" {
                    let mut width = 300u32; // Default canvas size
                    let mut height = 150u32;
                    
                    // Parse attributes
                    for attr in element.attrs.iter() {
                        let name = tree.resolve(attr.name.local);
                        match name {
                            "width" => {
                                width = attr.value.parse().unwrap_or(300);
                            }
                            "height" => {
                                height = attr.value.parse().unwrap_or(150);
                            }
                            _ => {}
                        }
                    }
                    
                    let id = self.next_id;
                    self.next_id += 1;
                    
                    // Create 2D context
                    let context = CanvasRenderingContext2D::new(width, height);
                    
                    self.canvases.insert(id, CanvasInstance {
                        id,
                        context,
                        bounds: CanvasBounds {
                            x: 0.0,
                            y: 0.0,
                            width,
                            height,
                        },
                        context_type: CanvasContextType::Context2D,
                    });
                }
            }
        }
        
        // Recurse into children
        for (child_id, _) in tree.children(node_id) {
            self.scan_tree(tree, child_id);
        }
    }
    
    /// Get all canvas instances
    pub fn get_canvases(&self) -> impl Iterator<Item = &CanvasInstance> {
        self.canvases.values()
    }
    
    /// Get canvas by ID
    pub fn get_canvas(&self, id: u64) -> Option<&CanvasInstance> {
        self.canvases.get(&id)
    }
    
    /// Get mutable canvas by ID
    pub fn get_canvas_mut(&mut self, id: u64) -> Option<&mut CanvasInstance> {
        self.canvases.get_mut(&id)
    }
    
    /// Get 2D context for canvas
    pub fn get_context_2d(&self, id: u64) -> Option<&CanvasRenderingContext2D> {
        self.canvases.get(&id).map(|c| &c.context)
    }
    
    /// Get mutable 2D context for canvas
    pub fn get_context_2d_mut(&mut self, id: u64) -> Option<&mut CanvasRenderingContext2D> {
        self.canvases.get_mut(&id).map(|c| &mut c.context)
    }
    
    // === Drawing operations ===
    
    /// Fill a rectangle on canvas
    pub fn fill_rect(&mut self, id: u64, x: f64, y: f64, width: f64, height: f64) {
        if let Some(canvas) = self.canvases.get_mut(&id) {
            canvas.context.fill_rect(x, y, width, height);
        }
    }
    
    /// Stroke a rectangle on canvas
    pub fn stroke_rect(&mut self, id: u64, x: f64, y: f64, width: f64, height: f64) {
        if let Some(canvas) = self.canvases.get_mut(&id) {
            canvas.context.stroke_rect(x, y, width, height);
        }
    }
    
    /// Clear a rectangle on canvas
    pub fn clear_rect(&mut self, id: u64, x: f64, y: f64, width: f64, height: f64) {
        if let Some(canvas) = self.canvases.get_mut(&id) {
            canvas.context.clear_rect(x, y, width, height);
        }
    }
    
    /// Clear entire canvas
    pub fn clear(&mut self, id: u64) {
        if let Some(canvas) = self.canvases.get_mut(&id) {
            let w = canvas.bounds.width as f64;
            let h = canvas.bounds.height as f64;
            canvas.context.clear_rect(0.0, 0.0, w, h);
        }
    }
    
    /// Set fill color
    pub fn set_fill_color(&mut self, id: u64, r: u8, g: u8, b: u8, a: u8) {
        if let Some(canvas) = self.canvases.get_mut(&id) {
            canvas.context.state_mut().fill_style = FillStyle::Color(Color::rgba(r, g, b, a));
        }
    }
    
    /// Set stroke color
    pub fn set_stroke_color(&mut self, id: u64, r: u8, g: u8, b: u8, a: u8) {
        if let Some(canvas) = self.canvases.get_mut(&id) {
            canvas.context.state_mut().stroke_style = StrokeStyle::Color(Color::rgba(r, g, b, a));
        }
    }
    
    /// Set line width
    pub fn set_line_width(&mut self, id: u64, width: f64) {
        if let Some(canvas) = self.canvases.get_mut(&id) {
            canvas.context.state_mut().line_width = width;
        }
    }
    
    // === Path operations ===
    
    /// Begin a new path
    pub fn begin_path(&mut self, id: u64) {
        if let Some(canvas) = self.canvases.get_mut(&id) {
            canvas.context.begin_path();
        }
    }
    
    /// Close the current path
    pub fn close_path(&mut self, id: u64) {
        if let Some(canvas) = self.canvases.get_mut(&id) {
            canvas.context.close_path();
        }
    }
    
    /// Move to point
    pub fn move_to(&mut self, id: u64, x: f64, y: f64) {
        if let Some(canvas) = self.canvases.get_mut(&id) {
            canvas.context.move_to(x, y);
        }
    }
    
    /// Line to point
    pub fn line_to(&mut self, id: u64, x: f64, y: f64) {
        if let Some(canvas) = self.canvases.get_mut(&id) {
            canvas.context.line_to(x, y);
        }
    }
    
    /// Fill the current path
    pub fn fill(&mut self, id: u64) {
        if let Some(canvas) = self.canvases.get_mut(&id) {
            canvas.context.fill();
        }
    }
    
    /// Stroke the current path
    pub fn stroke(&mut self, id: u64) {
        if let Some(canvas) = self.canvases.get_mut(&id) {
            canvas.context.stroke();
        }
    }
    
    // === State management ===
    
    /// Save canvas state
    pub fn save(&mut self, id: u64) {
        if let Some(canvas) = self.canvases.get_mut(&id) {
            canvas.context.save();
        }
    }
    
    /// Restore canvas state
    pub fn restore(&mut self, id: u64) {
        if let Some(canvas) = self.canvases.get_mut(&id) {
            canvas.context.restore();
        }
    }
    
    /// Get pixel data for a canvas
    pub fn get_image_data(&self, id: u64) -> Option<&[u8]> {
        self.canvases.get(&id).map(|c| c.context.data())
    }
    
    /// Get canvas statistics
    pub fn stats(&self) -> CanvasStats {
        CanvasStats {
            canvas_count: self.canvases.len(),
            total_pixels: self.canvases.values()
                .map(|c| (c.bounds.width as usize) * (c.bounds.height as usize))
                .sum(),
        }
    }
}

impl Default for CanvasManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Canvas statistics
#[derive(Debug, Clone)]
pub struct CanvasStats {
    pub canvas_count: usize,
    pub total_pixels: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_canvas_manager_creation() {
        let manager = CanvasManager::new();
        assert_eq!(manager.canvases.len(), 0);
    }
    
    #[test]
    fn test_canvas_stats() {
        let manager = CanvasManager::new();
        let stats = manager.stats();
        assert_eq!(stats.canvas_count, 0);
        assert_eq!(stats.total_pixels, 0);
    }
}
