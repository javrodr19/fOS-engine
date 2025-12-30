//! Paint Batching (Phase 24.5)
//!
//! Batch similar paint operations. Sort by z-order once.
//! Minimize GPU state changes. Parallel paint preparation.

use std::collections::HashMap;

/// Paint operation type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PaintOpType {
    /// Fill rectangle
    FillRect = 0,
    /// Draw border
    DrawBorder = 1,
    /// Draw text
    DrawText = 2,
    /// Draw image
    DrawImage = 3,
    /// Draw shadow
    DrawShadow = 4,
    /// Draw gradient
    DrawGradient = 5,
    /// Draw rounded rect
    DrawRoundedRect = 6,
    /// Draw clip
    PushClip = 7,
    /// Pop clip
    PopClip = 8,
    /// Draw mask
    DrawMask = 9,
}

/// Paint operation
#[derive(Debug, Clone)]
pub struct PaintOp {
    /// Operation type
    pub op_type: PaintOpType,
    /// Z-order (for sorting)
    pub z_order: i32,
    /// Bounding rect
    pub rect: PaintRect,
    /// Operation data
    pub data: PaintData,
    /// Texture/shader ID (for batching)
    pub texture_id: Option<u32>,
    /// Shader program ID
    pub shader_id: u32,
}

/// Paint rectangle
#[derive(Debug, Clone, Copy, Default)]
pub struct PaintRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl PaintRect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }
}

/// Paint operation data
#[derive(Debug, Clone)]
pub enum PaintData {
    /// Color (RGBA)
    Color(u32),
    /// Gradient
    Gradient { colors: Vec<u32>, stops: Vec<f32> },
    /// Image
    Image { texture_id: u32, uv: (f32, f32, f32, f32) },
    /// Text
    Text { font_id: u32, text: Box<str>, size: f32 },
    /// Shadow
    Shadow { blur: f32, offset: (f32, f32), color: u32 },
    /// Border
    Border { widths: [f32; 4], colors: [u32; 4] },
    /// None
    None,
}

/// Batch key for grouping operations
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct BatchKey {
    op_type: PaintOpType,
    shader_id: u32,
    texture_id: Option<u32>,
}

/// Paint batch
#[derive(Debug)]
pub struct PaintBatch {
    /// Batch key
    pub op_type: PaintOpType,
    pub shader_id: u32,
    pub texture_id: Option<u32>,
    /// Operations in this batch
    pub ops: Vec<PaintOp>,
    /// Vertex count
    pub vertex_count: usize,
}

impl PaintBatch {
    fn new(key: &BatchKey) -> Self {
        Self {
            op_type: key.op_type,
            shader_id: key.shader_id,
            texture_id: key.texture_id,
            ops: Vec::new(),
            vertex_count: 0,
        }
    }
    
    fn add(&mut self, op: PaintOp) {
        // Estimate vertex count
        self.vertex_count += match op.op_type {
            PaintOpType::FillRect | PaintOpType::DrawImage => 6, // 2 triangles
            PaintOpType::DrawBorder => 24, // 4 sides * 2 triangles
            PaintOpType::DrawRoundedRect => 32, // Corners + sides
            _ => 6,
        };
        self.ops.push(op);
    }
    
    /// Is batch empty
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
    
    /// Number of operations
    pub fn len(&self) -> usize {
        self.ops.len()
    }
}

/// Paint batcher
#[derive(Debug)]
pub struct PaintBatcher {
    /// Pending operations
    pending: Vec<PaintOp>,
    /// Configuration
    config: BatcherConfig,
    /// Statistics
    stats: BatcherStats,
}

/// Batcher configuration
#[derive(Debug, Clone)]
pub struct BatcherConfig {
    /// Maximum operations per batch
    pub max_batch_ops: usize,
    /// Maximum vertices per batch
    pub max_batch_vertices: usize,
    /// Enable z-sorting
    pub enable_sort: bool,
}

impl Default for BatcherConfig {
    fn default() -> Self {
        Self {
            max_batch_ops: 1000,
            max_batch_vertices: 65536,
            enable_sort: true,
        }
    }
}

/// Batcher statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct BatcherStats {
    pub total_ops: u64,
    pub batches_created: u64,
    pub state_changes_saved: u64,
    pub ops_per_batch_avg: f64,
}

impl BatcherStats {
    pub fn efficiency(&self) -> f64 {
        if self.batches_created == 0 {
            0.0
        } else {
            self.ops_per_batch_avg
        }
    }
}

impl Default for PaintBatcher {
    fn default() -> Self {
        Self::new(BatcherConfig::default())
    }
}

impl PaintBatcher {
    pub fn new(config: BatcherConfig) -> Self {
        Self {
            pending: Vec::new(),
            config,
            stats: BatcherStats::default(),
        }
    }
    
    /// Add a paint operation
    pub fn add(&mut self, op: PaintOp) {
        self.pending.push(op);
        self.stats.total_ops += 1;
    }
    
    /// Add fill rect
    pub fn fill_rect(&mut self, rect: PaintRect, color: u32, z_order: i32) {
        self.add(PaintOp {
            op_type: PaintOpType::FillRect,
            z_order,
            rect,
            data: PaintData::Color(color),
            texture_id: None,
            shader_id: 0, // Default shader
        });
    }
    
    /// Add draw image
    pub fn draw_image(&mut self, rect: PaintRect, texture_id: u32, uv: (f32, f32, f32, f32), z_order: i32) {
        self.add(PaintOp {
            op_type: PaintOpType::DrawImage,
            z_order,
            rect,
            data: PaintData::Image { texture_id, uv },
            texture_id: Some(texture_id),
            shader_id: 1, // Image shader
        });
    }
    
    /// Add draw text
    pub fn draw_text(&mut self, rect: PaintRect, font_id: u32, text: &str, size: f32, color: u32, z_order: i32) {
        self.add(PaintOp {
            op_type: PaintOpType::DrawText,
            z_order,
            rect,
            data: PaintData::Text { font_id, text: text.into(), size },
            texture_id: Some(font_id), // Font atlas
            shader_id: 2, // Text shader
        });
    }
    
    /// Finish and create batches
    pub fn finish(&mut self) -> Vec<PaintBatch> {
        if self.pending.is_empty() {
            return Vec::new();
        }
        
        // Sort by z-order if enabled
        if self.config.enable_sort {
            self.pending.sort_by_key(|op| op.z_order);
        }
        
        // Group into batches
        let mut batches: HashMap<BatchKey, PaintBatch> = HashMap::new();
        let mut batch_order: Vec<BatchKey> = Vec::new();
        
        for op in self.pending.drain(..) {
            let key = BatchKey {
                op_type: op.op_type,
                shader_id: op.shader_id,
                texture_id: op.texture_id,
            };
            
            if !batches.contains_key(&key) {
                batch_order.push(key.clone());
                batches.insert(key.clone(), PaintBatch::new(&key));
            }
            
            let batch = batches.get_mut(&key).unwrap();
            
            // Check if batch is full
            if batch.ops.len() >= self.config.max_batch_ops ||
               batch.vertex_count >= self.config.max_batch_vertices {
                // Start new batch
                let mut new_batch = PaintBatch::new(&key);
                new_batch.add(op);
                batches.insert(key.clone(), new_batch);
            } else {
                batch.add(op);
            }
        }
        
        // Calculate state changes saved
        let total_ops: usize = batches.values().map(|b| b.ops.len()).sum();
        let batch_count = batches.len();
        
        if batch_count > 0 {
            self.stats.batches_created += batch_count as u64;
            self.stats.state_changes_saved += (total_ops - batch_count) as u64;
            self.stats.ops_per_batch_avg = total_ops as f64 / batch_count as f64;
        }
        
        // Return in order
        batch_order.into_iter()
            .filter_map(|key| batches.remove(&key))
            .collect()
    }
    
    /// Clear pending operations
    pub fn clear(&mut self) {
        self.pending.clear();
    }
    
    /// Get statistics
    pub fn stats(&self) -> &BatcherStats {
        &self.stats
    }
}

/// Vertex data for GPU
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Vertex {
    /// Position (x, y)
    pub pos: [f32; 2],
    /// Texture coordinates
    pub uv: [f32; 2],
    /// Color (RGBA as floats)
    pub color: [f32; 4],
}

impl Vertex {
    pub fn new(x: f32, y: f32, u: f32, v: f32, color: u32) -> Self {
        Self {
            pos: [x, y],
            uv: [u, v],
            color: [
                ((color >> 24) & 0xFF) as f32 / 255.0,
                ((color >> 16) & 0xFF) as f32 / 255.0,
                ((color >> 8) & 0xFF) as f32 / 255.0,
                (color & 0xFF) as f32 / 255.0,
            ],
        }
    }
}

/// Generate vertices for a batch
pub fn generate_vertices(batch: &PaintBatch) -> Vec<Vertex> {
    let mut vertices = Vec::with_capacity(batch.vertex_count);
    
    for op in &batch.ops {
        match op.op_type {
            PaintOpType::FillRect => {
                if let PaintData::Color(color) = op.data {
                    let r = &op.rect;
                    // Two triangles
                    vertices.push(Vertex::new(r.x, r.y, 0.0, 0.0, color));
                    vertices.push(Vertex::new(r.x + r.width, r.y, 1.0, 0.0, color));
                    vertices.push(Vertex::new(r.x + r.width, r.y + r.height, 1.0, 1.0, color));
                    vertices.push(Vertex::new(r.x, r.y, 0.0, 0.0, color));
                    vertices.push(Vertex::new(r.x + r.width, r.y + r.height, 1.0, 1.0, color));
                    vertices.push(Vertex::new(r.x, r.y + r.height, 0.0, 1.0, color));
                }
            }
            PaintOpType::DrawImage => {
                if let PaintData::Image { uv, .. } = &op.data {
                    let r = &op.rect;
                    let white = 0xFFFFFFFF;
                    vertices.push(Vertex::new(r.x, r.y, uv.0, uv.1, white));
                    vertices.push(Vertex::new(r.x + r.width, r.y, uv.2, uv.1, white));
                    vertices.push(Vertex::new(r.x + r.width, r.y + r.height, uv.2, uv.3, white));
                    vertices.push(Vertex::new(r.x, r.y, uv.0, uv.1, white));
                    vertices.push(Vertex::new(r.x + r.width, r.y + r.height, uv.2, uv.3, white));
                    vertices.push(Vertex::new(r.x, r.y + r.height, uv.0, uv.3, white));
                }
            }
            _ => {
                // Other types would have their own vertex generation
            }
        }
    }
    
    vertices
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_paint_batcher() {
        let mut batcher = PaintBatcher::default();
        
        // Add some operations
        batcher.fill_rect(PaintRect::new(0.0, 0.0, 100.0, 100.0), 0xFF0000FF, 0);
        batcher.fill_rect(PaintRect::new(50.0, 50.0, 100.0, 100.0), 0xFF0000FF, 1);
        batcher.fill_rect(PaintRect::new(100.0, 100.0, 100.0, 100.0), 0xFF0000FF, 2);
        
        let batches = batcher.finish();
        
        // Should be batched together (same type, shader, texture)
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].ops.len(), 3);
    }
    
    #[test]
    fn test_different_types_separate_batches() {
        let mut batcher = PaintBatcher::default();
        
        batcher.fill_rect(PaintRect::new(0.0, 0.0, 50.0, 50.0), 0xFF0000FF, 0);
        batcher.draw_image(PaintRect::new(0.0, 0.0, 100.0, 100.0), 1, (0.0, 0.0, 1.0, 1.0), 1);
        batcher.fill_rect(PaintRect::new(0.0, 0.0, 50.0, 50.0), 0x00FF00FF, 2);
        
        let batches = batcher.finish();
        
        // Different types should be separate batches
        assert!(batches.len() >= 2);
    }
    
    #[test]
    fn test_vertex_generation() {
        let mut batcher = PaintBatcher::default();
        
        batcher.fill_rect(PaintRect::new(0.0, 0.0, 100.0, 100.0), 0xFF0000FF, 0);
        
        let batches = batcher.finish();
        let vertices = generate_vertices(&batches[0]);
        
        assert_eq!(vertices.len(), 6); // 2 triangles
    }
    
    #[test]
    fn test_stats() {
        let mut batcher = PaintBatcher::default();
        
        for _ in 0..10 {
            batcher.fill_rect(PaintRect::new(0.0, 0.0, 50.0, 50.0), 0xFF0000FF, 0);
        }
        
        let _ = batcher.finish();
        
        let stats = batcher.stats();
        assert_eq!(stats.total_ops, 10);
        assert!(stats.ops_per_batch_avg >= 1.0);
    }
}
