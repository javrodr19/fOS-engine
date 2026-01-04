//! Compressed DOM
//!
//! Memory-efficient compressed DOM representation using variable-length
//! encoding. Enables efficient serialization for tab hibernation.

use std::collections::HashMap;

/// Compressed tree representation
#[derive(Debug)]
pub struct CompressedTree {
    /// Compressed node data
    data: Vec<u8>,
    /// String table for deduplication
    strings: StringTable,
    /// Node count
    node_count: u32,
    /// Root offset
    root_offset: u32,
    /// Statistics
    stats: CompressionStats,
}

/// String table for deduplication
#[derive(Debug, Default)]
pub struct StringTable {
    /// Strings stored
    strings: Vec<String>,
    /// Index by string  
    index: HashMap<String, u32>,
}

/// Compression statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct CompressionStats {
    /// Original size (bytes)
    pub original_size: usize,
    /// Compressed size (bytes)
    pub compressed_size: usize,
    /// Number of nodes
    pub node_count: usize,
    /// Unique strings
    pub unique_strings: usize,
    /// String table size
    pub string_table_size: usize,
}

impl CompressionStats {
    /// Compression ratio
    pub fn ratio(&self) -> f64 {
        if self.original_size == 0 {
            1.0
        } else {
            self.compressed_size as f64 / self.original_size as f64
        }
    }
    
    /// Memory saved
    pub fn bytes_saved(&self) -> usize {
        self.original_size.saturating_sub(self.compressed_size)
    }
    
    /// Savings percentage
    pub fn savings_percent(&self) -> f64 {
        if self.original_size == 0 {
            0.0
        } else {
            100.0 * (1.0 - self.ratio())
        }
    }
}

impl StringTable {
    /// Create new string table
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Intern a string, return its ID
    pub fn intern(&mut self, s: &str) -> u32 {
        if let Some(&id) = self.index.get(s) {
            id
        } else {
            let id = self.strings.len() as u32;
            self.strings.push(s.to_string());
            self.index.insert(s.to_string(), id);
            id
        }
    }
    
    /// Get string by ID
    pub fn get(&self, id: u32) -> Option<&str> {
        self.strings.get(id as usize).map(String::as_str)
    }
    
    /// Number of strings
    pub fn len(&self) -> usize {
        self.strings.len()
    }
    
    /// Is empty
    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }
    
    /// Memory size
    pub fn memory_size(&self) -> usize {
        self.strings.iter().map(|s| s.len()).sum::<usize>()
            + self.index.len() * (std::mem::size_of::<String>() + std::mem::size_of::<u32>())
    }
}

/// Compressed node type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CompressedNodeType {
    Element = 1,
    Text = 2,
    Comment = 3,
    Document = 4,
    DocumentFragment = 5,
}

impl TryFrom<u8> for CompressedNodeType {
    type Error = ();
    
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Element),
            2 => Ok(Self::Text),
            3 => Ok(Self::Comment),
            4 => Ok(Self::Document),
            5 => Ok(Self::DocumentFragment),
            _ => Err(()),
        }
    }
}

impl Default for CompressedTree {
    fn default() -> Self {
        Self::new()
    }
}

impl CompressedTree {
    /// Create new empty compressed tree
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            strings: StringTable::new(),
            node_count: 0,
            root_offset: 0,
            stats: CompressionStats::default(),
        }
    }
    
    /// Create from uncompressed DOM representation
    pub fn compress<N, F>(nodes: &[N], get_node_info: F) -> Self
    where
        F: Fn(&N) -> NodeInfo,
    {
        let mut tree = Self::new();
        let mut original_size = 0;
        
        for node in nodes {
            let info = get_node_info(node);
            original_size += info.estimated_size();
            tree.write_node(&info);
        }
        
        tree.stats.original_size = original_size;
        tree.stats.compressed_size = tree.data.len() + tree.strings.memory_size();
        tree.stats.node_count = tree.node_count as usize;
        tree.stats.unique_strings = tree.strings.len();
        tree.stats.string_table_size = tree.strings.memory_size();
        
        tree
    }
    
    /// Get statistics
    pub fn stats(&self) -> &CompressionStats {
        &self.stats
    }
    
    /// Get compressed data
    pub fn data(&self) -> &[u8] {
        &self.data
    }
    
    /// Get string table
    pub fn strings(&self) -> &StringTable {
        &self.strings
    }
    
    /// Number of nodes
    pub fn node_count(&self) -> u32 {
        self.node_count
    }
    
    /// Create iterator over compressed nodes
    pub fn iter(&self) -> CompressedNodeIter<'_> {
        CompressedNodeIter {
            tree: self,
            offset: 0,
        }
    }
    
    fn write_node(&mut self, info: &NodeInfo) {
        // Write node type
        self.data.push(info.node_type as u8);
        
        match info.node_type {
            CompressedNodeType::Element => {
                // Write tag name ID
                let tag_id = self.strings.intern(&info.tag_name);
                self.write_varint(tag_id);
                
                // Write attribute count
                self.write_varint(info.attributes.len() as u32);
                
                // Write attributes
                for (name, value) in &info.attributes {
                    let name_id = self.strings.intern(name);
                    let value_id = self.strings.intern(value);
                    self.write_varint(name_id);
                    self.write_varint(value_id);
                }
                
                // Write child count
                self.write_varint(info.child_count);
            }
            
            CompressedNodeType::Text | CompressedNodeType::Comment => {
                // Write text content
                let content_id = self.strings.intern(&info.text_content);
                self.write_varint(content_id);
            }
            
            CompressedNodeType::Document | CompressedNodeType::DocumentFragment => {
                // Write child count
                self.write_varint(info.child_count);
            }
        }
        
        self.node_count += 1;
    }
    
    fn write_varint(&mut self, value: u32) {
        let mut v = value;
        loop {
            let mut byte = (v & 0x7f) as u8;
            v >>= 7;
            if v != 0 {
                byte |= 0x80;
            }
            self.data.push(byte);
            if v == 0 {
                break;
            }
        }
    }
}

/// Node information for compression
#[derive(Debug, Clone)]
pub struct NodeInfo {
    /// Node type
    pub node_type: CompressedNodeType,
    /// Tag name (for elements)
    pub tag_name: String,
    /// Attributes (for elements)
    pub attributes: Vec<(String, String)>,
    /// Text content (for text/comment)
    pub text_content: String,
    /// Child count
    pub child_count: u32,
}

impl NodeInfo {
    /// Create element node info
    pub fn element(tag: &str, attributes: Vec<(String, String)>, child_count: u32) -> Self {
        Self {
            node_type: CompressedNodeType::Element,
            tag_name: tag.to_string(),
            attributes,
            text_content: String::new(),
            child_count,
        }
    }
    
    /// Create text node info
    pub fn text(content: &str) -> Self {
        Self {
            node_type: CompressedNodeType::Text,
            tag_name: String::new(),
            attributes: Vec::new(),
            text_content: content.to_string(),
            child_count: 0,
        }
    }
    
    /// Create comment node info
    pub fn comment(content: &str) -> Self {
        Self {
            node_type: CompressedNodeType::Comment,
            tag_name: String::new(),
            attributes: Vec::new(),
            text_content: content.to_string(),
            child_count: 0,
        }
    }
    
    /// Create document node info
    pub fn document(child_count: u32) -> Self {
        Self {
            node_type: CompressedNodeType::Document,
            tag_name: String::new(),
            attributes: Vec::new(),
            text_content: String::new(),
            child_count,
        }
    }
    
    /// Estimated uncompressed size
    fn estimated_size(&self) -> usize {
        let base = 32; // Node struct overhead
        let tag = self.tag_name.len();
        let attrs: usize = self.attributes.iter()
            .map(|(k, v)| k.len() + v.len() + 16)
            .sum();
        let text = self.text_content.len();
        base + tag + attrs + text
    }
}

/// Iterator over compressed nodes
pub struct CompressedNodeIter<'a> {
    tree: &'a CompressedTree,
    offset: usize,
}

impl<'a> Iterator for CompressedNodeIter<'a> {
    type Item = DecompressedNode;
    
    fn next(&mut self) -> Option<Self::Item> {
        if self.offset >= self.tree.data.len() {
            return None;
        }
        
        // Read node type
        let node_type = CompressedNodeType::try_from(self.tree.data[self.offset]).ok()?;
        self.offset += 1;
        
        match node_type {
            CompressedNodeType::Element => {
                let tag_id = self.read_varint()?;
                let attr_count = self.read_varint()? as usize;
                
                let mut attributes = Vec::with_capacity(attr_count);
                for _ in 0..attr_count {
                    let name_id = self.read_varint()?;
                    let value_id = self.read_varint()?;
                    let name = self.tree.strings.get(name_id)?.to_string();
                    let value = self.tree.strings.get(value_id)?.to_string();
                    attributes.push((name, value));
                }
                
                let child_count = self.read_varint()?;
                let tag = self.tree.strings.get(tag_id)?.to_string();
                
                Some(DecompressedNode::Element {
                    tag,
                    attributes,
                    child_count,
                })
            }
            
            CompressedNodeType::Text => {
                let content_id = self.read_varint()?;
                let content = self.tree.strings.get(content_id)?.to_string();
                Some(DecompressedNode::Text(content))
            }
            
            CompressedNodeType::Comment => {
                let content_id = self.read_varint()?;
                let content = self.tree.strings.get(content_id)?.to_string();
                Some(DecompressedNode::Comment(content))
            }
            
            CompressedNodeType::Document => {
                let child_count = self.read_varint()?;
                Some(DecompressedNode::Document { child_count })
            }
            
            CompressedNodeType::DocumentFragment => {
                let child_count = self.read_varint()?;
                Some(DecompressedNode::DocumentFragment { child_count })
            }
        }
    }
}

impl<'a> CompressedNodeIter<'a> {
    fn read_varint(&mut self) -> Option<u32> {
        let mut result = 0u32;
        let mut shift = 0;
        
        loop {
            if self.offset >= self.tree.data.len() {
                return None;
            }
            let byte = self.tree.data[self.offset];
            self.offset += 1;
            
            result |= ((byte & 0x7f) as u32) << shift;
            if byte & 0x80 == 0 {
                break;
            }
            shift += 7;
        }
        
        Some(result)
    }
}

/// Decompressed node
#[derive(Debug, Clone)]
pub enum DecompressedNode {
    Element {
        tag: String,
        attributes: Vec<(String, String)>,
        child_count: u32,
    },
    Text(String),
    Comment(String),
    Document { child_count: u32 },
    DocumentFragment { child_count: u32 },
}

/// Delta encoding for incremental DOM updates
#[derive(Debug, Clone)]
pub struct DomDelta {
    /// Operations
    pub ops: Vec<DeltaOp>,
}

/// Delta operation
#[derive(Debug, Clone)]
pub enum DeltaOp {
    /// Insert node at position
    Insert {
        parent_id: u32,
        position: u32,
        node: NodeInfo,
    },
    /// Remove node
    Remove {
        node_id: u32,
    },
    /// Update text content
    UpdateText {
        node_id: u32,
        new_content: String,
    },
    /// Update attribute
    UpdateAttr {
        node_id: u32,
        name: String,
        value: Option<String>,
    },
    /// Move node
    Move {
        node_id: u32,
        new_parent: u32,
        position: u32,
    },
}

impl DomDelta {
    /// Create empty delta
    pub fn new() -> Self {
        Self { ops: Vec::new() }
    }
    
    /// Add insert operation
    pub fn insert(&mut self, parent_id: u32, position: u32, node: NodeInfo) {
        self.ops.push(DeltaOp::Insert { parent_id, position, node });
    }
    
    /// Add remove operation
    pub fn remove(&mut self, node_id: u32) {
        self.ops.push(DeltaOp::Remove { node_id });
    }
    
    /// Add text update
    pub fn update_text(&mut self, node_id: u32, new_content: String) {
        self.ops.push(DeltaOp::UpdateText { node_id, new_content });
    }
    
    /// Add attribute update
    pub fn update_attr(&mut self, node_id: u32, name: String, value: Option<String>) {
        self.ops.push(DeltaOp::UpdateAttr { node_id, name, value });
    }
    
    /// Add move operation
    pub fn move_node(&mut self, node_id: u32, new_parent: u32, position: u32) {
        self.ops.push(DeltaOp::Move { node_id, new_parent, position });
    }
    
    /// Number of operations
    pub fn len(&self) -> usize {
        self.ops.len()
    }
    
    /// Is empty
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}

impl Default for DomDelta {
    fn default() -> Self {
        Self::new()
    }
}

/// Lazy decompressor for on-demand subtree decompression
#[derive(Debug)]
pub struct LazyDecompressor<'a> {
    /// Source tree
    tree: &'a CompressedTree,
    /// Decompressed cache
    cache: HashMap<u32, DecompressedNode>,
    /// Maximum cache size
    max_cache: usize,
}

impl<'a> LazyDecompressor<'a> {
    /// Create new lazy decompressor
    pub fn new(tree: &'a CompressedTree) -> Self {
        Self {
            tree,
            cache: HashMap::new(),
            max_cache: 1024,
        }
    }
    
    /// Get node by index (decompresses on demand)
    pub fn get(&mut self, index: u32) -> Option<DecompressedNode> {
        if let Some(node) = self.cache.get(&index) {
            return Some(node.clone());
        }
        
        // Find and decompress node
        let mut iter = self.tree.iter();
        for i in 0..=index {
            if let Some(node) = iter.next() {
                if i == index {
                    // Cache if space available
                    if self.cache.len() < self.max_cache {
                        self.cache.insert(index, node.clone());
                    }
                    return Some(node);
                }
            } else {
                return None;
            }
        }
        
        None
    }
    
    /// Clear cache
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }
    
    /// Cache size
    pub fn cache_size(&self) -> usize {
        self.cache.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_string_table() {
        let mut table = StringTable::new();
        
        let id1 = table.intern("hello");
        let id2 = table.intern("world");
        let id3 = table.intern("hello"); // Duplicate
        
        assert_eq!(id1, id3); // Same string, same ID
        assert_ne!(id1, id2);
        
        assert_eq!(table.get(id1), Some("hello"));
        assert_eq!(table.get(id2), Some("world"));
    }
    
    #[test]
    fn test_compress_decompress() {
        let nodes = vec![
            NodeInfo::document(2),
            NodeInfo::element("html", vec![], 2),
            NodeInfo::element("head", vec![], 1),
            NodeInfo::element("title", vec![], 1),
            NodeInfo::text("Test Page"),
            NodeInfo::element("body", vec![], 1),
            NodeInfo::element("div", vec![
                ("class".to_string(), "container".to_string()),
                ("id".to_string(), "main".to_string()),
            ], 1),
            NodeInfo::text("Hello World"),
        ];
        
        let tree = CompressedTree::compress(&nodes, |n| n.clone());
        
        assert_eq!(tree.node_count(), 8);
        // Stats are computed (compression ratio varies with data size)
        assert!(tree.stats().node_count == 8);
        assert!(tree.stats().unique_strings > 0);
        
        // Decompress and verify
        let decompressed: Vec<_> = tree.iter().collect();
        assert_eq!(decompressed.len(), 8);
        
        if let DecompressedNode::Element { tag, .. } = &decompressed[1] {
            assert_eq!(tag, "html");
        } else {
            panic!("Expected Element");
        }
    }
    
    #[test]
    fn test_dom_delta() {
        let mut delta = DomDelta::new();
        
        delta.insert(0, 0, NodeInfo::element("div", vec![], 0));
        delta.update_text(1, "new content".to_string());
        delta.remove(2);
        
        assert_eq!(delta.len(), 3);
    }
    
    #[test]
    fn test_lazy_decompress() {
        let nodes = vec![
            NodeInfo::document(1),
            NodeInfo::element("html", vec![], 1),
            NodeInfo::element("body", vec![], 1),
            NodeInfo::text("Content"),
        ];
        
        let tree = CompressedTree::compress(&nodes, |n| n.clone());
        let mut lazy = LazyDecompressor::new(&tree);
        
        // Access node 2
        let node = lazy.get(2).unwrap();
        if let DecompressedNode::Element { tag, .. } = node {
            assert_eq!(tag, "body");
        } else {
            panic!("Expected Element");
        }
        
        // Should be cached now
        assert_eq!(lazy.cache_size(), 1);
    }
}
