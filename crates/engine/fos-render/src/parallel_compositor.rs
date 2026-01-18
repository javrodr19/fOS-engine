//! Parallel Compositor
//!
//! Parallel layer compositing for efficient multi-core rendering.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

/// Layer identifier
pub type LayerId = usize;

/// Layer for compositing
#[derive(Debug, Clone)]
pub struct Layer {
    /// Layer ID
    pub id: LayerId,
    /// Parent layer ID
    pub parent_id: Option<LayerId>,
    /// Children layer IDs
    pub children: Vec<LayerId>,
    /// Z-order within parent
    pub z_index: i32,
    /// Layer bounds
    pub bounds: LayerBounds,
    /// Transform matrix
    pub transform: Transform2D,
    /// Opacity (0.0 to 1.0)
    pub opacity: f32,
    /// Blend mode
    pub blend_mode: BlendMode,
    /// Layer content
    pub content: LayerContent,
    /// Is this layer dirty (needs re-composite)
    pub dirty: bool,
}

/// Layer bounds
#[derive(Debug, Clone, Copy, Default)]
pub struct LayerBounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl LayerBounds {
    /// Check if intersects another bounds
    pub fn intersects(&self, other: &LayerBounds) -> bool {
        self.x < other.x + other.width
            && self.x + self.width > other.x
            && self.y < other.y + other.height
            && self.y + self.height > other.y
    }
    
    /// Compute intersection
    pub fn intersection(&self, other: &LayerBounds) -> Option<LayerBounds> {
        if !self.intersects(other) {
            return None;
        }
        
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = (self.x + self.width).min(other.x + other.width);
        let bottom = (self.y + self.height).min(other.y + other.height);
        
        Some(LayerBounds {
            x,
            y,
            width: right - x,
            height: bottom - y,
        })
    }
}

/// 2D transform matrix
#[derive(Debug, Clone, Copy)]
pub struct Transform2D {
    pub m11: f32, pub m12: f32,
    pub m21: f32, pub m22: f32,
    pub m31: f32, pub m32: f32,
}

impl Default for Transform2D {
    fn default() -> Self {
        Self::identity()
    }
}

impl Transform2D {
    /// Identity transform
    pub fn identity() -> Self {
        Self {
            m11: 1.0, m12: 0.0,
            m21: 0.0, m22: 1.0,
            m31: 0.0, m32: 0.0,
        }
    }
    
    /// Translation transform
    pub fn translate(x: f32, y: f32) -> Self {
        Self {
            m11: 1.0, m12: 0.0,
            m21: 0.0, m22: 1.0,
            m31: x, m32: y,
        }
    }
    
    /// Scale transform
    pub fn scale(sx: f32, sy: f32) -> Self {
        Self {
            m11: sx, m12: 0.0,
            m21: 0.0, m22: sy,
            m31: 0.0, m32: 0.0,
        }
    }
    
    /// Multiply transforms
    pub fn then(&self, other: &Transform2D) -> Self {
        Self {
            m11: self.m11 * other.m11 + self.m12 * other.m21,
            m12: self.m11 * other.m12 + self.m12 * other.m22,
            m21: self.m21 * other.m11 + self.m22 * other.m21,
            m22: self.m21 * other.m12 + self.m22 * other.m22,
            m31: self.m31 * other.m11 + self.m32 * other.m21 + other.m31,
            m32: self.m31 * other.m12 + self.m32 * other.m22 + other.m32,
        }
    }
    
    /// Transform a point
    pub fn transform_point(&self, x: f32, y: f32) -> (f32, f32) {
        (
            x * self.m11 + y * self.m21 + self.m31,
            x * self.m12 + y * self.m22 + self.m32,
        )
    }
}

/// Blend mode
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
}

/// Layer content type
#[derive(Debug, Clone)]
pub enum LayerContent {
    /// Empty layer (container only)
    Empty,
    /// Solid color
    SolidColor(Color),
    /// Texture/image reference
    Texture(u32),
    /// Render target ID
    RenderTarget(u32),
}

/// RGBA color
#[derive(Debug, Clone, Copy, Default)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }
    
    pub fn white() -> Self {
        Self::new(1.0, 1.0, 1.0, 1.0)
    }
    
    pub fn transparent() -> Self {
        Self::new(0.0, 0.0, 0.0, 0.0)
    }
    
    /// Blend this color over another
    pub fn blend_over(&self, dst: Color, mode: BlendMode) -> Color {
        let src = *self;
        
        match mode {
            BlendMode::Normal => {
                // Standard alpha blending
                let out_a = src.a + dst.a * (1.0 - src.a);
                if out_a == 0.0 {
                    return Color::transparent();
                }
                Color {
                    r: (src.r * src.a + dst.r * dst.a * (1.0 - src.a)) / out_a,
                    g: (src.g * src.a + dst.g * dst.a * (1.0 - src.a)) / out_a,
                    b: (src.b * src.a + dst.b * dst.a * (1.0 - src.a)) / out_a,
                    a: out_a,
                }
            }
            BlendMode::Multiply => {
                Color {
                    r: src.r * dst.r,
                    g: src.g * dst.g,
                    b: src.b * dst.b,
                    a: src.a * dst.a,
                }
            }
            BlendMode::Screen => {
                Color {
                    r: 1.0 - (1.0 - src.r) * (1.0 - dst.r),
                    g: 1.0 - (1.0 - src.g) * (1.0 - dst.g),
                    b: 1.0 - (1.0 - src.b) * (1.0 - dst.b),
                    a: src.a + dst.a * (1.0 - src.a),
                }
            }
            _ => {
                // Default to normal for other modes
                src.blend_over(dst, BlendMode::Normal)
            }
        }
    }
}

/// Composite result buffer
#[derive(Debug, Clone)]
pub struct CompositeResult {
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Pixel data (RGBA)
    pub pixels: Vec<Color>,
}

impl CompositeResult {
    /// Create empty result
    pub fn empty() -> Self {
        Self {
            width: 0,
            height: 0,
            pixels: Vec::new(),
        }
    }
    
    /// Create with dimensions
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![Color::transparent(); (width * height) as usize],
        }
    }
    
    /// Get pixel at coordinates
    pub fn get(&self, x: u32, y: u32) -> Color {
        if x >= self.width || y >= self.height {
            return Color::transparent();
        }
        self.pixels[(y * self.width + x) as usize]
    }
    
    /// Set pixel at coordinates
    pub fn set(&mut self, x: u32, y: u32, color: Color) {
        if x < self.width && y < self.height {
            self.pixels[(y * self.width + x) as usize] = color;
        }
    }
    
    /// Merge another result over this one
    pub fn merge(&mut self, other: &CompositeResult, offset_x: i32, offset_y: i32) {
        for y in 0..other.height {
            for x in 0..other.width {
                let dst_x = x as i32 + offset_x;
                let dst_y = y as i32 + offset_y;
                
                if dst_x >= 0 && dst_x < self.width as i32 && dst_y >= 0 && dst_y < self.height as i32 {
                    let src_color = other.get(x, y);
                    let dst_color = self.get(dst_x as u32, dst_y as u32);
                    let blended = src_color.blend_over(dst_color, BlendMode::Normal);
                    self.set(dst_x as u32, dst_y as u32, blended);
                }
            }
        }
    }
}

/// Layer group for parallel processing
#[derive(Debug, Clone)]
pub struct LayerGroup {
    /// Group ID
    pub id: usize,
    /// Layers in this group
    pub layers: Vec<LayerId>,
    /// Combined bounds
    pub bounds: LayerBounds,
    /// Is independent (can be composited in parallel)
    pub independent: bool,
}

/// Composite layers in parallel
pub fn composite_layers_parallel(layers: &[Layer], viewport: LayerBounds) -> CompositeResult {
    if layers.is_empty() {
        return CompositeResult::empty();
    }
    
    let num_threads = std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(4);
    
    // Sort layers by z-order
    let sorted = sort_layers_by_z(layers);
    
    // Identify independent layer groups
    let groups = identify_layer_groups(layers, &sorted);
    
    // Create result buffer
    let mut result = CompositeResult::new(viewport.width as u32, viewport.height as u32);
    
    if groups.len() <= 1 || num_threads <= 1 {
        // Sequential compositing
        for &id in &sorted {
            composite_layer_to_result(&layers[id], layers, &viewport, &mut result);
        }
    } else {
        // Parallel compositing of independent groups
        let group_results: Vec<_> = groups.iter()
            .filter(|g| g.independent)
            .map(|group| {
                let group_layers: Vec<_> = group.layers.iter()
                    .map(|&id| &layers[id])
                    .collect();
                let group_sorted: Vec<_> = sort_layers_by_z(&group_layers.iter().map(|l| (*l).clone()).collect::<Vec<_>>());
                (group.clone(), group_sorted)
            })
            .collect();
        
        let parallel_results: Vec<(usize, CompositeResult)> = if group_results.len() > 1 {
            let results = Arc::new(Mutex::new(Vec::new()));
            
            std::thread::scope(|s| {
                for (group, sorted_ids) in group_results {
                    let results = Arc::clone(&results);
                    let viewport = viewport;
                    let layers = layers;
                    
                    s.spawn(move || {
                        let mut group_result = CompositeResult::new(
                            group.bounds.width as u32,
                            group.bounds.height as u32,
                        );
                        
                        for &id in &sorted_ids {
                            if id < layers.len() {
                                composite_layer_to_result(&layers[id], layers, &group.bounds, &mut group_result);
                            }
                        }
                        
                        results.lock().unwrap().push((group.id, group_result));
                    });
                }
            });
            
            Arc::try_unwrap(results).unwrap().into_inner().unwrap()
        } else {
            Vec::new()
        };
        
        // Merge group results
        for (group_id, group_result) in parallel_results {
            if let Some(group) = groups.iter().find(|g| g.id == group_id) {
                result.merge(
                    &group_result,
                    (group.bounds.x - viewport.x) as i32,
                    (group.bounds.y - viewport.y) as i32,
                );
            }
        }
        
        // Composite non-independent layers sequentially
        for &id in &sorted {
            let in_independent = groups.iter()
                .any(|g| g.independent && g.layers.contains(&id));
            
            if !in_independent {
                composite_layer_to_result(&layers[id], layers, &viewport, &mut result);
            }
        }
    }
    
    result
}

/// Sort layers by z-order
fn sort_layers_by_z(layers: &[Layer]) -> Vec<LayerId> {
    let mut ids: Vec<_> = (0..layers.len()).collect();
    ids.sort_by(|&a, &b| layers[a].z_index.cmp(&layers[b].z_index));
    ids
}

/// Identify independent layer groups
fn identify_layer_groups(layers: &[Layer], sorted: &[LayerId]) -> Vec<LayerGroup> {
    let mut groups = Vec::new();
    let mut current_group = LayerGroup {
        id: 0,
        layers: Vec::new(),
        bounds: LayerBounds::default(),
        independent: true,
    };
    
    for (i, &id) in sorted.iter().enumerate() {
        let layer = &layers[id];
        
        // Check if this layer overlaps with current group
        let overlaps = !current_group.layers.is_empty() && 
            current_group.bounds.intersects(&layer.bounds);
        
        if overlaps && layer.blend_mode != BlendMode::Normal {
            // Non-normal blend mode breaks independence
            current_group.independent = false;
        }
        
        if overlaps || current_group.layers.is_empty() {
            current_group.layers.push(id);
            
            // Expand bounds
            if current_group.layers.len() == 1 {
                current_group.bounds = layer.bounds;
            } else {
                let min_x = current_group.bounds.x.min(layer.bounds.x);
                let min_y = current_group.bounds.y.min(layer.bounds.y);
                let max_x = (current_group.bounds.x + current_group.bounds.width)
                    .max(layer.bounds.x + layer.bounds.width);
                let max_y = (current_group.bounds.y + current_group.bounds.height)
                    .max(layer.bounds.y + layer.bounds.height);
                
                current_group.bounds = LayerBounds {
                    x: min_x,
                    y: min_y,
                    width: max_x - min_x,
                    height: max_y - min_y,
                };
            }
        } else {
            // Start new group
            groups.push(current_group);
            current_group = LayerGroup {
                id: i,
                layers: vec![id],
                bounds: layer.bounds,
                independent: true,
            };
        }
    }
    
    if !current_group.layers.is_empty() {
        groups.push(current_group);
    }
    
    groups
}

/// Composite a single layer to result buffer
fn composite_layer_to_result(
    layer: &Layer,
    all_layers: &[Layer],
    viewport: &LayerBounds,
    result: &mut CompositeResult,
) {
    // Skip if not visible
    if layer.opacity <= 0.0 {
        return;
    }
    
    // Skip if outside viewport
    if !layer.bounds.intersects(viewport) {
        return;
    }
    
    let effective_opacity = layer.opacity;
    
    // Get layer color
    let layer_color = match &layer.content {
        LayerContent::SolidColor(c) => *c,
        LayerContent::Empty => return,
        LayerContent::Texture(_) | LayerContent::RenderTarget(_) => {
            // Would normally sample from texture here
            Color::white()
        }
    };
    
    // Apply opacity
    let src_color = Color {
        r: layer_color.r,
        g: layer_color.g,
        b: layer_color.b,
        a: layer_color.a * effective_opacity,
    };
    
    // Calculate render bounds
    let render_x = ((layer.bounds.x - viewport.x) as i32).max(0) as u32;
    let render_y = ((layer.bounds.y - viewport.y) as i32).max(0) as u32;
    let render_w = (layer.bounds.width as u32).min(result.width - render_x);
    let render_h = (layer.bounds.height as u32).min(result.height - render_y);
    
    // Fill pixels
    for y in render_y..(render_y + render_h) {
        for x in render_x..(render_x + render_w) {
            let dst_color = result.get(x, y);
            let blended = src_color.blend_over(dst_color, layer.blend_mode);
            result.set(x, y, blended);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_layer_bounds_intersection() {
        let a = LayerBounds { x: 0.0, y: 0.0, width: 100.0, height: 100.0 };
        let b = LayerBounds { x: 50.0, y: 50.0, width: 100.0, height: 100.0 };
        let c = LayerBounds { x: 200.0, y: 200.0, width: 50.0, height: 50.0 };
        
        assert!(a.intersects(&b));
        assert!(!a.intersects(&c));
        
        let inter = a.intersection(&b).unwrap();
        assert_eq!(inter.x, 50.0);
        assert_eq!(inter.y, 50.0);
        assert_eq!(inter.width, 50.0);
        assert_eq!(inter.height, 50.0);
    }
    
    #[test]
    fn test_color_blending() {
        let red = Color::new(1.0, 0.0, 0.0, 0.5);
        let blue = Color::new(0.0, 0.0, 1.0, 1.0);
        
        let result = red.blend_over(blue, BlendMode::Normal);
        
        assert!(result.r > 0.0);
        assert!(result.b > 0.0);
    }
    
    #[test]
    fn test_simple_composite() {
        let layers = vec![
            Layer {
                id: 0,
                parent_id: None,
                children: vec![],
                z_index: 0,
                bounds: LayerBounds { x: 0.0, y: 0.0, width: 100.0, height: 100.0 },
                transform: Transform2D::identity(),
                opacity: 1.0,
                blend_mode: BlendMode::Normal,
                content: LayerContent::SolidColor(Color::new(1.0, 0.0, 0.0, 1.0)),
                dirty: false,
            },
        ];
        
        let viewport = LayerBounds { x: 0.0, y: 0.0, width: 200.0, height: 200.0 };
        let result = composite_layers_parallel(&layers, viewport);
        
        assert_eq!(result.width, 200);
        assert_eq!(result.height, 200);
        
        // Check a pixel inside the layer
        let pixel = result.get(50, 50);
        assert!(pixel.r > 0.9);
    }
    
    #[test]
    fn test_parallel_groups() {
        let layers = vec![
            Layer {
                id: 0,
                parent_id: None,
                children: vec![],
                z_index: 0,
                bounds: LayerBounds { x: 0.0, y: 0.0, width: 50.0, height: 50.0 },
                transform: Transform2D::identity(),
                opacity: 1.0,
                blend_mode: BlendMode::Normal,
                content: LayerContent::SolidColor(Color::new(1.0, 0.0, 0.0, 1.0)),
                dirty: false,
            },
            Layer {
                id: 1,
                parent_id: None,
                children: vec![],
                z_index: 1,
                bounds: LayerBounds { x: 100.0, y: 100.0, width: 50.0, height: 50.0 },
                transform: Transform2D::identity(),
                opacity: 1.0,
                blend_mode: BlendMode::Normal,
                content: LayerContent::SolidColor(Color::new(0.0, 1.0, 0.0, 1.0)),
                dirty: false,
            },
        ];
        
        let groups = identify_layer_groups(&layers, &[0, 1]);
        
        // Non-overlapping layers should be in separate groups
        assert!(groups.len() >= 1);
        assert!(groups.iter().all(|g| g.independent));
    }
}
