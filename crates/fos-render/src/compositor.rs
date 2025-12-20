//! GPU Compositor
//!
//! GPU-accelerated compositing for smooth 60fps rendering.

use std::collections::HashMap;

/// Compositor for GPU-accelerated rendering
#[derive(Debug)]
pub struct Compositor {
    pub layers: Vec<CompositorLayer>,
    pub root_layer: Option<u64>,
    pub viewport: Viewport,
    pub settings: CompositorSettings,
    damage_rects: Vec<DamageRect>,
}

/// Compositor layer
#[derive(Debug, Clone)]
pub struct CompositorLayer {
    pub id: u64,
    pub bounds: LayerBounds,
    pub transform: LayerTransform,
    pub opacity: f32,
    pub blend_mode: BlendMode,
    pub content: LayerContent,
    pub children: Vec<u64>,
    pub parent: Option<u64>,
    pub needs_repaint: bool,
    pub tiles: Vec<Tile>,
}

/// Layer bounds
#[derive(Debug, Clone, Default)]
pub struct LayerBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Layer transform
#[derive(Debug, Clone)]
pub struct LayerTransform {
    pub matrix: [f64; 16], // 4x4 matrix
}

impl Default for LayerTransform {
    fn default() -> Self {
        Self {
            matrix: [
                1.0, 0.0, 0.0, 0.0,
                0.0, 1.0, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ],
        }
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
}

/// Layer content
#[derive(Debug, Clone)]
pub enum LayerContent {
    Empty,
    Solid { color: [u8; 4] },
    Image { texture_id: u64 },
    Tiles,
    Video { texture_id: u64 },
}

/// Tile for tile-based rendering
#[derive(Debug, Clone)]
pub struct Tile {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub texture_id: Option<u64>,
    pub dirty: bool,
}

impl Tile {
    pub const DEFAULT_SIZE: u32 = 256;
    
    pub fn new(x: u32, y: u32) -> Self {
        Self {
            x,
            y,
            width: Self::DEFAULT_SIZE,
            height: Self::DEFAULT_SIZE,
            texture_id: None,
            dirty: true,
        }
    }
}

/// Viewport
#[derive(Debug, Clone, Default)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    pub scroll_x: f64,
    pub scroll_y: f64,
}

/// Compositor settings
#[derive(Debug, Clone)]
pub struct CompositorSettings {
    pub tile_size: u32,
    pub max_texture_size: u32,
    pub enable_gpu: bool,
    pub vsync: bool,
}

impl Default for CompositorSettings {
    fn default() -> Self {
        Self {
            tile_size: 256,
            max_texture_size: 4096,
            enable_gpu: true,
            vsync: true,
        }
    }
}

/// Damage rect
#[derive(Debug, Clone)]
pub struct DamageRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Compositor {
    pub fn new() -> Self {
        Self {
            layers: Vec::new(),
            root_layer: None,
            viewport: Viewport::default(),
            settings: CompositorSettings::default(),
            damage_rects: Vec::new(),
        }
    }
    
    /// Create a new layer
    pub fn create_layer(&mut self) -> u64 {
        let id = self.layers.len() as u64;
        let layer = CompositorLayer {
            id,
            bounds: LayerBounds::default(),
            transform: LayerTransform::default(),
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            content: LayerContent::Empty,
            children: Vec::new(),
            parent: None,
            needs_repaint: true,
            tiles: Vec::new(),
        };
        self.layers.push(layer);
        
        if self.root_layer.is_none() {
            self.root_layer = Some(id);
        }
        
        id
    }
    
    /// Set layer bounds
    pub fn set_layer_bounds(&mut self, id: u64, bounds: LayerBounds) {
        if let Some(layer) = self.layers.get_mut(id as usize) {
            layer.bounds = bounds;
            layer.needs_repaint = true;
            self.create_tiles_for_layer(id);
        }
    }
    
    /// Create tiles for layer
    fn create_tiles_for_layer(&mut self, id: u64) {
        if let Some(layer) = self.layers.get_mut(id as usize) {
            layer.tiles.clear();
            
            let cols = (layer.bounds.width as u32 / Tile::DEFAULT_SIZE) + 1;
            let rows = (layer.bounds.height as u32 / Tile::DEFAULT_SIZE) + 1;
            
            for y in 0..rows {
                for x in 0..cols {
                    layer.tiles.push(Tile::new(x, y));
                }
            }
        }
    }
    
    /// Add damage rect
    pub fn add_damage(&mut self, rect: DamageRect) {
        self.damage_rects.push(rect);
    }
    
    /// Get visible layers (occlusion culling)
    pub fn get_visible_layers(&self) -> Vec<u64> {
        let mut visible = Vec::new();
        
        for layer in &self.layers {
            if layer.opacity > 0.0 && self.is_layer_visible(layer) {
                visible.push(layer.id);
            }
        }
        
        visible
    }
    
    fn is_layer_visible(&self, layer: &CompositorLayer) -> bool {
        // Check if layer intersects viewport
        let vp = &self.viewport;
        let b = &layer.bounds;
        
        b.x < (vp.scroll_x + vp.width as f64) &&
        b.x + b.width > vp.scroll_x &&
        b.y < (vp.scroll_y + vp.height as f64) &&
        b.y + b.height > vp.scroll_y
    }
    
    /// Get dirty tiles for rendering
    pub fn get_dirty_tiles(&self) -> Vec<(u64, &Tile)> {
        let mut dirty = Vec::new();
        
        for layer in &self.layers {
            for tile in &layer.tiles {
                if tile.dirty {
                    dirty.push((layer.id, tile));
                }
            }
        }
        
        dirty
    }
    
    /// Composite frame
    pub fn composite(&mut self) -> CompositeResult {
        let visible = self.get_visible_layers();
        let dirty_tiles = self.get_dirty_tiles().len();
        
        // Clear damage after compositing
        self.damage_rects.clear();
        
        // Mark tiles as clean
        for layer in &mut self.layers {
            for tile in &mut layer.tiles {
                tile.dirty = false;
            }
        }
        
        CompositeResult {
            layers_composited: visible.len(),
            tiles_rendered: dirty_tiles,
        }
    }
}

/// Composite result
#[derive(Debug)]
pub struct CompositeResult {
    pub layers_composited: usize,
    pub tiles_rendered: usize,
}

impl Default for Compositor {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_compositor() {
        let mut compositor = Compositor::new();
        let layer = compositor.create_layer();
        
        compositor.set_layer_bounds(layer, LayerBounds {
            x: 0.0, y: 0.0, width: 800.0, height: 600.0,
        });
        
        compositor.viewport = Viewport {
            width: 800, height: 600, scale: 1.0, scroll_x: 0.0, scroll_y: 0.0,
        };
        
        assert!(compositor.get_visible_layers().contains(&layer));
    }
    
    #[test]
    fn test_tiles() {
        let mut compositor = Compositor::new();
        let layer = compositor.create_layer();
        
        compositor.set_layer_bounds(layer, LayerBounds {
            x: 0.0, y: 0.0, width: 1024.0, height: 768.0,
        });
        
        // Should create multiple tiles
        assert!(compositor.layers[layer as usize].tiles.len() > 1);
    }
}
