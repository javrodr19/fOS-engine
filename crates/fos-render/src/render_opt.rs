//! Rendering Optimizations
//!
//! Display list compilation, texture atlas, dirty rect fusion, culling, diffing.

use std::collections::HashMap;

/// Display list for GPU command compilation
#[derive(Debug, Default)]
pub struct DisplayList {
    /// Compiled draw commands
    commands: Vec<DrawCommand>,
    /// Is dirty (needs recompilation)
    dirty: bool,
}

/// Draw command in display list
#[derive(Debug, Clone)]
pub enum DrawCommand {
    Clear { color: [f32; 4] },
    DrawRect { x: f32, y: f32, width: f32, height: f32, color: [f32; 4] },
    DrawImage { x: f32, y: f32, width: f32, height: f32, texture_id: u64 },
    DrawText { x: f32, y: f32, text_id: u64 },
    PushTransform([f32; 6]),
    PopTransform,
    PushClip { x: f32, y: f32, width: f32, height: f32 },
    PopClip,
    SetOpacity(f32),
}

impl DisplayList {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn push(&mut self, cmd: DrawCommand) {
        self.commands.push(cmd);
        self.dirty = true;
    }
    
    pub fn clear(&mut self) {
        self.commands.clear();
        self.dirty = true;
    }
    
    pub fn commands(&self) -> &[DrawCommand] {
        &self.commands
    }
    
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }
    
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }
}

/// Texture atlas packer
#[derive(Debug)]
pub struct TextureAtlas {
    /// Atlas width
    pub width: u32,
    /// Atlas height
    pub height: u32,
    /// Packed regions
    regions: Vec<AtlasRegion>,
    /// Free rectangles
    free_rects: Vec<Rect>,
    /// Atlas data
    pub data: Vec<u8>,
}

/// Atlas region
#[derive(Debug, Clone)]
pub struct AtlasRegion {
    pub id: u64,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Rectangle
#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    #[inline]
    pub fn area(&self) -> u32 {
        self.width * self.height
    }
    
    #[inline]
    pub fn fits(&self, w: u32, h: u32) -> bool {
        self.width >= w && self.height >= h
    }
}

impl TextureAtlas {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            regions: Vec::new(),
            free_rects: vec![Rect { x: 0, y: 0, width, height }],
            data: vec![0u8; (width * height * 4) as usize],
        }
    }
    
    /// Pack a texture into the atlas (returns region ID or None if no space)
    pub fn pack(&mut self, id: u64, width: u32, height: u32, data: &[u8]) -> Option<AtlasRegion> {
        // Find best-fit rectangle
        let best_idx = self.free_rects.iter()
            .enumerate()
            .filter(|(_, r)| r.fits(width, height))
            .min_by_key(|(_, r)| r.area())
            .map(|(i, _)| i)?;
        
        let rect = self.free_rects.remove(best_idx);
        
        // Create region
        let region = AtlasRegion {
            id,
            x: rect.x,
            y: rect.y,
            width,
            height,
        };
        
        // Copy data into atlas
        for y in 0..height {
            let src_offset = (y * width * 4) as usize;
            let dst_offset = ((region.y + y) * self.width * 4 + region.x * 4) as usize;
            let row_size = (width * 4) as usize;
            
            if src_offset + row_size <= data.len() && dst_offset + row_size <= self.data.len() {
                self.data[dst_offset..dst_offset + row_size]
                    .copy_from_slice(&data[src_offset..src_offset + row_size]);
            }
        }
        
        // Split remaining space
        if rect.width > width {
            self.free_rects.push(Rect {
                x: rect.x + width,
                y: rect.y,
                width: rect.width - width,
                height,
            });
        }
        if rect.height > height {
            self.free_rects.push(Rect {
                x: rect.x,
                y: rect.y + height,
                width: rect.width,
                height: rect.height - height,
            });
        }
        
        self.regions.push(region.clone());
        Some(region)
    }
    
    /// Get region by ID
    pub fn get_region(&self, id: u64) -> Option<&AtlasRegion> {
        self.regions.iter().find(|r| r.id == id)
    }
    
    /// Get UV coordinates for a region
    pub fn get_uv(&self, id: u64) -> Option<[f32; 4]> {
        self.get_region(id).map(|r| [
            r.x as f32 / self.width as f32,
            r.y as f32 / self.height as f32,
            (r.x + r.width) as f32 / self.width as f32,
            (r.y + r.height) as f32 / self.height as f32,
        ])
    }
}

/// Dirty rectangle tracker
#[derive(Debug, Default)]
pub struct DirtyRectTracker {
    /// Dirty rectangles
    rects: Vec<DirtyRect>,
}

/// Dirty rectangle
#[derive(Debug, Clone, Copy)]
pub struct DirtyRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl DirtyRect {
    pub fn union(&self, other: &DirtyRect) -> DirtyRect {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let x2 = (self.x + self.width).max(other.x + other.width);
        let y2 = (self.y + self.height).max(other.y + other.height);
        
        DirtyRect {
            x,
            y,
            width: x2 - x,
            height: y2 - y,
        }
    }
    
    pub fn intersects(&self, other: &DirtyRect) -> bool {
        self.x < other.x + other.width &&
        self.x + self.width > other.x &&
        self.y < other.y + other.height &&
        self.y + self.height > other.y
    }
}

impl DirtyRectTracker {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Add a dirty rect
    pub fn add(&mut self, rect: DirtyRect) {
        // Try to merge with existing
        for existing in &mut self.rects {
            if existing.intersects(&rect) {
                *existing = existing.union(&rect);
                return;
            }
        }
        self.rects.push(rect);
    }
    
    /// Fuse overlapping rectangles
    pub fn fuse(&mut self) {
        let mut i = 0;
        while i < self.rects.len() {
            let mut j = i + 1;
            while j < self.rects.len() {
                if self.rects[i].intersects(&self.rects[j]) {
                    let merged = self.rects[i].union(&self.rects[j]);
                    self.rects[i] = merged;
                    self.rects.remove(j);
                } else {
                    j += 1;
                }
            }
            i += 1;
        }
    }
    
    /// Get fused dirty rects
    pub fn get_dirty_rects(&mut self) -> Vec<DirtyRect> {
        self.fuse();
        std::mem::take(&mut self.rects)
    }
    
    /// Clear
    pub fn clear(&mut self) {
        self.rects.clear();
    }
}

/// Occlusion culler
#[derive(Debug, Default)]
pub struct OcclusionCuller {
    /// Occluders (opaque elements)
    occluders: Vec<OccluderRect>,
}

/// Occluder rectangle
#[derive(Debug, Clone)]
pub struct OccluderRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub z_index: i32,
}

impl OcclusionCuller {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Add an occluder
    pub fn add_occluder(&mut self, rect: OccluderRect) {
        self.occluders.push(rect);
    }
    
    /// Sort occluders by z-index (front to back)
    pub fn sort(&mut self) {
        self.occluders.sort_by(|a, b| b.z_index.cmp(&a.z_index));
    }
    
    /// Check if element is occluded
    pub fn is_occluded(&self, x: f32, y: f32, width: f32, height: f32, z_index: i32) -> bool {
        for occ in &self.occluders {
            if occ.z_index > z_index {
                // Check if completely covered
                if x >= occ.x && y >= occ.y &&
                   x + width <= occ.x + occ.width &&
                   y + height <= occ.y + occ.height {
                    return true;
                }
            }
        }
        false
    }
    
    /// Clear occluders
    pub fn clear(&mut self) {
        self.occluders.clear();
    }
}

/// Render tree differ
#[derive(Debug, Default)]
pub struct RenderTreeDiffer {
    /// Previous render nodes
    prev_nodes: HashMap<u64, RenderNodeSnapshot>,
}

/// Render node snapshot
#[derive(Debug, Clone, PartialEq)]
pub struct RenderNodeSnapshot {
    pub id: u64,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub opacity: f32,
    pub transform_hash: u64,
    pub content_hash: u64,
}

/// Diff result
#[derive(Debug, Clone)]
pub enum DiffResult {
    Added(u64),
    Removed(u64),
    Changed(u64, ChangeType),
    Unchanged(u64),
}

/// Change type
#[derive(Debug, Clone)]
pub enum ChangeType {
    Position,
    Size,
    Opacity,
    Transform,
    Content,
}

impl RenderTreeDiffer {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Diff current nodes against previous
    pub fn diff(&mut self, current: &[RenderNodeSnapshot]) -> Vec<DiffResult> {
        let mut results = Vec::new();
        
        let current_map: HashMap<u64, &RenderNodeSnapshot> = 
            current.iter().map(|n| (n.id, n)).collect();
        
        // Check for removed and changed nodes
        for (id, prev) in &self.prev_nodes {
            if let Some(curr) = current_map.get(id) {
                if prev == *curr {
                    results.push(DiffResult::Unchanged(*id));
                } else {
                    let change_type = if prev.x != curr.x || prev.y != curr.y {
                        ChangeType::Position
                    } else if prev.width != curr.width || prev.height != curr.height {
                        ChangeType::Size
                    } else if prev.opacity != curr.opacity {
                        ChangeType::Opacity
                    } else if prev.transform_hash != curr.transform_hash {
                        ChangeType::Transform
                    } else {
                        ChangeType::Content
                    };
                    results.push(DiffResult::Changed(*id, change_type));
                }
            } else {
                results.push(DiffResult::Removed(*id));
            }
        }
        
        // Check for added nodes
        for node in current {
            if !self.prev_nodes.contains_key(&node.id) {
                results.push(DiffResult::Added(node.id));
            }
        }
        
        // Update previous state
        self.prev_nodes = current.iter().map(|n| (n.id, n.clone())).collect();
        
        results
    }
    
    /// Clear previous state
    pub fn clear(&mut self) {
        self.prev_nodes.clear();
    }
}

/// OffscreenCanvas for background rendering
#[derive(Debug)]
pub struct OffscreenCanvas {
    /// Width
    pub width: u32,
    /// Height
    pub height: u32,
    /// Pixel data
    pub data: Vec<u8>,
    /// Has alpha
    pub alpha: bool,
}

impl OffscreenCanvas {
    pub fn new(width: u32, height: u32, alpha: bool) -> Self {
        let size = (width * height * 4) as usize;
        Self {
            width,
            height,
            data: vec![0u8; size],
            alpha,
        }
    }
    
    /// Transfer to main thread (returns pixel data)
    pub fn transfer_to_image_bitmap(&self) -> Vec<u8> {
        self.data.clone()
    }
    
    /// Get 2D context
    pub fn get_context_2d(&mut self) -> OffscreenCanvasContext {
        OffscreenCanvasContext {
            canvas: self,
        }
    }
}

/// OffscreenCanvas 2D context
pub struct OffscreenCanvasContext<'a> {
    canvas: &'a mut OffscreenCanvas,
}

impl<'a> OffscreenCanvasContext<'a> {
    pub fn fill_rect(&mut self, x: u32, y: u32, width: u32, height: u32, color: [u8; 4]) {
        for dy in 0..height {
            for dx in 0..width {
                let px = x + dx;
                let py = y + dy;
                if px < self.canvas.width && py < self.canvas.height {
                    let offset = ((py * self.canvas.width + px) * 4) as usize;
                    if offset + 4 <= self.canvas.data.len() {
                        self.canvas.data[offset..offset + 4].copy_from_slice(&color);
                    }
                }
            }
        }
    }
    
    pub fn clear(&mut self) {
        self.canvas.data.fill(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_texture_atlas() {
        let mut atlas = TextureAtlas::new(512, 512);
        
        let data = vec![255u8; 64 * 64 * 4];
        let region = atlas.pack(1, 64, 64, &data);
        
        assert!(region.is_some());
        let r = region.unwrap();
        assert_eq!(r.width, 64);
        assert_eq!(r.height, 64);
    }
    
    #[test]
    fn test_dirty_rect_fusion() {
        let mut tracker = DirtyRectTracker::new();
        
        tracker.add(DirtyRect { x: 0, y: 0, width: 100, height: 100 });
        tracker.add(DirtyRect { x: 50, y: 50, width: 100, height: 100 });
        
        let rects = tracker.get_dirty_rects();
        assert_eq!(rects.len(), 1); // Should be fused
    }
    
    #[test]
    fn test_occlusion() {
        let mut culler = OcclusionCuller::new();
        
        culler.add_occluder(OccluderRect {
            x: 0.0, y: 0.0, width: 200.0, height: 200.0, z_index: 10,
        });
        
        // Element behind is occluded
        assert!(culler.is_occluded(50.0, 50.0, 50.0, 50.0, 5));
        // Element in front is not occluded
        assert!(!culler.is_occluded(50.0, 50.0, 50.0, 50.0, 15));
    }
    
    #[test]
    fn test_offscreen_canvas() {
        let mut canvas = OffscreenCanvas::new(100, 100, true);
        let mut ctx = canvas.get_context_2d();
        
        ctx.fill_rect(10, 10, 20, 20, [255, 0, 0, 255]);
        
        let data = canvas.transfer_to_image_bitmap();
        assert_eq!(data.len(), 100 * 100 * 4);
    }
}
