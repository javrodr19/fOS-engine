//! Streaming DOM Construction (Phase 24.2)
//!
//! Don't wait for </html>. Render as chunks arrive.
//! Layout visible portion first. Background parse rest.

use std::collections::VecDeque;

/// Chunk of parsed DOM content
#[derive(Debug, Clone)]
pub struct DomChunk {
    /// Chunk ID
    pub id: u32,
    /// Nodes in this chunk
    pub nodes: Vec<StreamNode>,
    /// Whether this is the final chunk
    pub is_final: bool,
    /// Byte offset in source
    pub source_offset: usize,
    /// Byte length
    pub source_length: usize,
}

/// Streamed node representation
#[derive(Debug, Clone)]
pub struct StreamNode {
    /// Node ID
    pub id: u32,
    /// Parent node ID (0 for root)
    pub parent_id: u32,
    /// Node type
    pub node_type: StreamNodeType,
    /// Tag name (for elements)
    pub tag: Option<Box<str>>,
    /// Text content (for text nodes)
    pub text: Option<Box<str>>,
    /// Attributes
    pub attrs: Vec<(Box<str>, Box<str>)>,
}

/// Stream node type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamNodeType {
    Element,
    Text,
    Comment,
    Document,
    DocumentFragment,
}

/// Streaming parser state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamState {
    /// Waiting for data
    Idle,
    /// Parsing head section
    ParsingHead,
    /// Parsing body (can start rendering)
    ParsingBody,
    /// Parsing complete
    Complete,
    /// Error occurred
    Error,
}

/// Streaming DOM builder
#[derive(Debug)]
pub struct StreamingDomBuilder {
    /// Current state
    state: StreamState,
    /// Next node ID
    next_id: u32,
    /// Pending chunks
    pending: VecDeque<DomChunk>,
    /// Ready chunks (parsed, ready for consumption)
    ready: VecDeque<DomChunk>,
    /// Current chunk being built
    current_chunk: Option<DomChunk>,
    /// Nodes per chunk
    chunk_size: usize,
    /// Total bytes received
    bytes_received: usize,
    /// Total nodes created
    nodes_created: u32,
    /// Statistics
    stats: StreamStats,
}

/// Streaming statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct StreamStats {
    pub chunks_produced: u32,
    pub chunks_consumed: u32,
    pub bytes_parsed: usize,
    pub nodes_parsed: u32,
    pub time_to_first_render_ms: Option<u64>,
}

impl Default for StreamingDomBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamingDomBuilder {
    pub fn new() -> Self {
        Self {
            state: StreamState::Idle,
            next_id: 1,
            pending: VecDeque::new(),
            ready: VecDeque::new(),
            current_chunk: None,
            chunk_size: 50,
            bytes_received: 0,
            nodes_created: 0,
            stats: StreamStats::default(),
        }
    }
    
    /// Set chunk size
    pub fn with_chunk_size(mut self, size: usize) -> Self {
        self.chunk_size = size;
        self
    }
    
    /// Start streaming
    pub fn start(&mut self) {
        self.state = StreamState::ParsingHead;
        self.current_chunk = Some(DomChunk {
            id: 0,
            nodes: Vec::with_capacity(self.chunk_size),
            is_final: false,
            source_offset: 0,
            source_length: 0,
        });
    }
    
    /// Feed data to the parser
    pub fn feed(&mut self, data: &[u8]) {
        self.bytes_received += data.len();
        
        // Here we'd normally parse HTML, but for now simulate node creation
        // In reality, this would be called from the HTML parser
    }
    
    /// Add a parsed node
    pub fn add_node(&mut self, node: StreamNode) {
        if let Some(ref mut chunk) = self.current_chunk {
            chunk.nodes.push(node);
            chunk.source_length += 1; // Simplified
            self.nodes_created += 1;
            self.stats.nodes_parsed += 1;
            
            // Check if chunk is full
            if chunk.nodes.len() >= self.chunk_size {
                self.flush_chunk();
            }
        }
        
        // Update state based on parsed content
        if self.state == StreamState::ParsingHead {
            // Check for body start (simplified)
            self.state = StreamState::ParsingBody;
        }
    }
    
    /// Flush current chunk to ready queue
    fn flush_chunk(&mut self) {
        if let Some(mut chunk) = self.current_chunk.take() {
            if !chunk.nodes.is_empty() {
                chunk.id = self.stats.chunks_produced;
                self.stats.chunks_produced += 1;
                self.ready.push_back(chunk);
            }
            
            self.current_chunk = Some(DomChunk {
                id: 0,
                nodes: Vec::with_capacity(self.chunk_size),
                is_final: false,
                source_offset: self.bytes_received,
                source_length: 0,
            });
        }
    }
    
    /// End streaming (final chunk)
    pub fn end(&mut self) {
        if let Some(mut chunk) = self.current_chunk.take() {
            chunk.is_final = true;
            chunk.id = self.stats.chunks_produced;
            self.stats.chunks_produced += 1;
            self.ready.push_back(chunk);
        }
        
        self.state = StreamState::Complete;
        self.stats.bytes_parsed = self.bytes_received;
    }
    
    /// Get next ready chunk
    pub fn next_chunk(&mut self) -> Option<DomChunk> {
        let chunk = self.ready.pop_front();
        if chunk.is_some() {
            self.stats.chunks_consumed += 1;
        }
        chunk
    }
    
    /// Check if there are ready chunks
    pub fn has_chunks(&self) -> bool {
        !self.ready.is_empty()
    }
    
    /// Check if can start rendering (body found)
    pub fn can_render(&self) -> bool {
        self.state == StreamState::ParsingBody || self.state == StreamState::Complete
    }
    
    /// Check if parsing is complete
    pub fn is_complete(&self) -> bool {
        self.state == StreamState::Complete
    }
    
    /// Get current state
    pub fn state(&self) -> StreamState {
        self.state
    }
    
    /// Get statistics
    pub fn stats(&self) -> &StreamStats {
        &self.stats
    }
    
    /// Allocate a node ID
    pub fn alloc_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

/// Incremental layout hints from streaming parse
#[derive(Debug, Clone)]
pub struct LayoutHint {
    /// Node that needs layout
    pub node_id: u32,
    /// Priority (higher = more urgent)
    pub priority: u8,
    /// Is in visible viewport
    pub in_viewport: bool,
}

/// Streaming layout coordinator
#[derive(Debug)]
pub struct StreamingLayoutCoordinator {
    /// Layout hints queue
    hints: VecDeque<LayoutHint>,
    /// Nodes that have been laid out
    laid_out: std::collections::HashSet<u32>,
    /// Viewport estimate
    viewport_height: f32,
}

impl Default for StreamingLayoutCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamingLayoutCoordinator {
    pub fn new() -> Self {
        Self {
            hints: VecDeque::new(),
            laid_out: std::collections::HashSet::new(),
            viewport_height: 1000.0,
        }
    }
    
    /// Set viewport height
    pub fn set_viewport(&mut self, height: f32) {
        self.viewport_height = height;
    }
    
    /// Add layout hint from streaming parse
    pub fn add_hint(&mut self, node_id: u32, estimated_y: f32) {
        let in_viewport = estimated_y < self.viewport_height;
        let priority = if in_viewport { 100 } else { 50 };
        
        self.hints.push_back(LayoutHint {
            node_id,
            priority,
            in_viewport,
        });
        
        // Keep hints sorted by priority
        self.hints.make_contiguous().sort_by(|a, b| b.priority.cmp(&a.priority));
    }
    
    /// Get next node to layout
    pub fn next_to_layout(&mut self) -> Option<LayoutHint> {
        while let Some(hint) = self.hints.pop_front() {
            if !self.laid_out.contains(&hint.node_id) {
                self.laid_out.insert(hint.node_id);
                return Some(hint);
            }
        }
        None
    }
    
    /// Check if we have high-priority work
    pub fn has_viewport_work(&self) -> bool {
        self.hints.iter().any(|h| h.in_viewport)
    }
    
    /// Count of pending hints
    pub fn pending_count(&self) -> usize {
        self.hints.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_streaming_builder() {
        let mut builder = StreamingDomBuilder::new().with_chunk_size(5);
        
        builder.start();
        
        // Add some nodes
        for i in 0..12 {
            let id = builder.alloc_id();
            builder.add_node(StreamNode {
                id,
                parent_id: 0,
                node_type: StreamNodeType::Element,
                tag: Some("div".into()),
                text: None,
                attrs: vec![],
            });
        }
        
        // Should have 2 full chunks ready
        assert!(builder.has_chunks());
        
        let chunk1 = builder.next_chunk();
        assert!(chunk1.is_some());
        assert_eq!(chunk1.unwrap().nodes.len(), 5);
    }
    
    #[test]
    fn test_streaming_end() {
        let mut builder = StreamingDomBuilder::new();
        
        builder.start();
        
        // Add a few nodes
        for i in 0..3 {
            let id = builder.alloc_id();
            builder.add_node(StreamNode {
                id,
                parent_id: 0,
                node_type: StreamNodeType::Text,
                tag: None,
                text: Some("text".into()),
                attrs: vec![],
            });
        }
        
        assert!(!builder.is_complete());
        
        builder.end();
        assert!(builder.is_complete());
        
        // Final chunk should be available
        let chunk = builder.next_chunk();
        assert!(chunk.is_some());
        assert!(chunk.unwrap().is_final);
    }
    
    #[test]
    fn test_layout_coordinator() {
        let mut coord = StreamingLayoutCoordinator::new();
        coord.set_viewport(500.0);
        
        // Add viewport node (high priority)
        coord.add_hint(1, 100.0);
        // Add below-fold node (lower priority)
        coord.add_hint(2, 1000.0);
        
        assert!(coord.has_viewport_work());
        
        // Should get viewport node first
        let hint1 = coord.next_to_layout().unwrap();
        assert!(hint1.in_viewport);
        assert_eq!(hint1.node_id, 1);
    }
}
