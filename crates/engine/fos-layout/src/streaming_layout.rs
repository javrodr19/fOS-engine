//! Streaming Layout
//!
//! Progressive layout as content arrives from streaming parser.

use std::collections::VecDeque;

/// Layout chunk from streaming parse
#[derive(Debug, Clone)]
pub struct LayoutChunk {
    pub node_id: u32,
    pub parent_id: Option<u32>,
    pub box_type: StreamBoxType,
    pub estimated_height: f32,
}

/// Box type for streaming layout
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamBoxType {
    Block,
    Inline,
    InlineBlock,
    Flex,
    Grid,
    None,
}

/// Incremental layout context
#[derive(Debug)]
pub struct IncrementalContext {
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub current_y: f32,
    pub content_height: f32,
    pub visible_start: f32,
    pub visible_end: f32,
}

impl IncrementalContext {
    pub fn new(viewport_width: f32, viewport_height: f32) -> Self {
        Self { viewport_width, viewport_height, current_y: 0.0, content_height: 0.0, visible_start: 0.0, visible_end: viewport_height }
    }
    
    pub fn is_visible(&self, y: f32, height: f32) -> bool {
        y + height >= self.visible_start && y <= self.visible_end
    }
    
    pub fn advance(&mut self, height: f32) {
        self.current_y += height;
        self.content_height = self.content_height.max(self.current_y);
    }
}

/// Streaming layout engine
#[derive(Debug)]
pub struct StreamingLayoutEngine {
    pending: VecDeque<LayoutChunk>,
    context: IncrementalContext,
    laid_out: Vec<StreamLayoutBox>,
    stats: StreamLayoutStats,
}

/// Laid out box from streaming
#[derive(Debug, Clone)]
pub struct StreamLayoutBox {
    pub node_id: u32,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub is_visible: bool,
}

/// Statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct StreamLayoutStats {
    pub chunks_processed: usize,
    pub visible_boxes: usize,
    pub hidden_boxes: usize,
    pub total_height: f32,
}

impl Default for StreamingLayoutEngine {
    fn default() -> Self { Self::new(800.0, 600.0) }
}

impl StreamingLayoutEngine {
    pub fn new(viewport_width: f32, viewport_height: f32) -> Self {
        Self {
            pending: VecDeque::new(),
            context: IncrementalContext::new(viewport_width, viewport_height),
            laid_out: Vec::new(),
            stats: StreamLayoutStats::default(),
        }
    }
    
    pub fn set_viewport(&mut self, width: f32, height: f32) {
        self.context.viewport_width = width;
        self.context.viewport_height = height;
        self.context.visible_end = height;
    }
    
    pub fn set_scroll(&mut self, scroll_y: f32) {
        self.context.visible_start = scroll_y;
        self.context.visible_end = scroll_y + self.context.viewport_height;
    }
    
    pub fn add_chunk(&mut self, chunk: LayoutChunk) {
        self.pending.push_back(chunk);
    }
    
    pub fn process(&mut self, max_chunks: usize) -> LayoutYield {
        let mut processed = 0;
        
        while let Some(chunk) = self.pending.pop_front() {
            let height = self.estimate_height(&chunk);
            let y = self.context.current_y;
            let is_visible = self.context.is_visible(y, height);
            
            let layout_box = StreamLayoutBox {
                node_id: chunk.node_id,
                x: 0.0,
                y,
                width: self.context.viewport_width,
                height,
                is_visible,
            };
            
            self.laid_out.push(layout_box);
            self.context.advance(height);
            
            self.stats.chunks_processed += 1;
            if is_visible { self.stats.visible_boxes += 1; } else { self.stats.hidden_boxes += 1; }
            
            processed += 1;
            if processed >= max_chunks { return LayoutYield::Yielded { processed, has_visible: is_visible }; }
        }
        
        LayoutYield::Complete
    }
    
    fn estimate_height(&self, chunk: &LayoutChunk) -> f32 {
        if chunk.estimated_height > 0.0 { chunk.estimated_height }
        else {
            match chunk.box_type {
                StreamBoxType::Block => 20.0,
                StreamBoxType::Inline | StreamBoxType::InlineBlock => 16.0,
                StreamBoxType::Flex | StreamBoxType::Grid => 40.0,
                StreamBoxType::None => 0.0,
            }
        }
    }
    
    pub fn get_visible_boxes(&self) -> Vec<&StreamLayoutBox> {
        self.laid_out.iter().filter(|b| b.is_visible).collect()
    }
    
    pub fn stats(&self) -> &StreamLayoutStats { &self.stats }
    pub fn content_height(&self) -> f32 { self.context.content_height }
    pub fn pending_count(&self) -> usize { self.pending.len() }
    pub fn has_pending(&self) -> bool { !self.pending.is_empty() }
}

/// Layout yield result
#[derive(Debug, Clone, Copy)]
pub enum LayoutYield {
    NeedMoreData,
    Yielded { processed: usize, has_visible: bool },
    Complete,
}

/// Viewport-prioritized layout scheduler
#[derive(Debug, Default)]
pub struct ViewportPriority {
    above_fold: VecDeque<u32>,
    below_fold: VecDeque<u32>,
    viewport_y: f32,
}

impl ViewportPriority {
    pub fn new() -> Self { Self::default() }
    
    pub fn set_viewport(&mut self, y: f32) { self.viewport_y = y; }
    
    pub fn add(&mut self, node_id: u32, estimated_y: f32) {
        if estimated_y <= self.viewport_y + 1000.0 { self.above_fold.push_back(node_id); }
        else { self.below_fold.push_back(node_id); }
    }
    
    pub fn next(&mut self) -> Option<u32> {
        self.above_fold.pop_front().or_else(|| self.below_fold.pop_front())
    }
    
    pub fn has_above_fold(&self) -> bool { !self.above_fold.is_empty() }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_streaming_layout() {
        let mut engine = StreamingLayoutEngine::new(800.0, 600.0);
        
        engine.add_chunk(LayoutChunk { node_id: 0, parent_id: None, box_type: StreamBoxType::Block, estimated_height: 100.0 });
        engine.add_chunk(LayoutChunk { node_id: 1, parent_id: Some(0), box_type: StreamBoxType::Block, estimated_height: 200.0 });
        
        engine.process(10);
        
        assert_eq!(engine.stats().chunks_processed, 2);
        assert!(engine.content_height() > 0.0);
    }
    
    #[test]
    fn test_visibility() {
        let mut engine = StreamingLayoutEngine::new(800.0, 100.0);
        
        for i in 0..10 {
            engine.add_chunk(LayoutChunk { node_id: i, parent_id: None, box_type: StreamBoxType::Block, estimated_height: 50.0 });
        }
        
        engine.process(100);
        
        let visible = engine.get_visible_boxes();
        assert!(visible.len() < 10); // Not all should be visible
    }
}
