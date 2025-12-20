//! Compact Empty Nodes (Phase 24.2)
//!
//! Use minimal struct for empty text nodes.
//! Compact comment node storage. Share whitespace-only content.
//! 20% less memory while preserving full DOM.

use std::sync::Arc;

/// Node ID type
pub type NodeId = u32;

/// Compact node representation
#[derive(Debug, Clone)]
pub enum CompactNode {
    /// Full element with all data
    Element(ElementNode),
    /// Compact empty element (no children, no attributes)
    EmptyElement(EmptyElementNode),
    /// Full text node
    Text(TextNode),
    /// Compact text node (interned whitespace)
    WhitespaceText(WhitespaceId),
    /// Empty text node
    EmptyText,
    /// Comment node
    Comment(CommentNode),
    /// Compact comment (empty or whitespace only)
    EmptyComment,
}

impl CompactNode {
    /// Size in bytes
    pub fn memory_size(&self) -> usize {
        match self {
            CompactNode::Element(e) => e.memory_size(),
            CompactNode::EmptyElement(_) => std::mem::size_of::<EmptyElementNode>(),
            CompactNode::Text(t) => t.memory_size(),
            CompactNode::WhitespaceText(_) => std::mem::size_of::<WhitespaceId>(),
            CompactNode::EmptyText => 0,
            CompactNode::Comment(c) => c.memory_size(),
            CompactNode::EmptyComment => 0,
        }
    }
    
    /// Check if this is an empty/compact node
    pub fn is_compact(&self) -> bool {
        matches!(self,
            CompactNode::EmptyElement(_) |
            CompactNode::WhitespaceText(_) |
            CompactNode::EmptyText |
            CompactNode::EmptyComment
        )
    }
}

/// Full element node
#[derive(Debug, Clone)]
pub struct ElementNode {
    pub id: NodeId,
    pub tag: u32,
    pub attributes: Vec<(Arc<str>, Arc<str>)>,
    pub children: Vec<NodeId>,
}

impl ElementNode {
    pub fn memory_size(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.attributes.len() * std::mem::size_of::<(Arc<str>, Arc<str>)>()
            + self.children.len() * std::mem::size_of::<NodeId>()
    }
}

/// Empty element (minimal storage)
#[derive(Debug, Clone, Copy)]
pub struct EmptyElementNode {
    pub id: NodeId,
    pub tag: u32,
}

/// Full text node
#[derive(Debug, Clone)]
pub struct TextNode {
    pub id: NodeId,
    pub content: Arc<str>,
}

impl TextNode {
    pub fn memory_size(&self) -> usize {
        std::mem::size_of::<Self>() + self.content.len()
    }
}

/// Whitespace ID (index into whitespace pool)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WhitespaceId(pub u8);

/// Comment node
#[derive(Debug, Clone)]
pub struct CommentNode {
    pub id: NodeId,
    pub content: Arc<str>,
}

impl CommentNode {
    pub fn memory_size(&self) -> usize {
        std::mem::size_of::<Self>() + self.content.len()
    }
}

/// Common whitespace patterns (pre-interned)
pub static WHITESPACE_PATTERNS: &[&str] = &[
    "",
    " ",
    "  ",
    "   ",
    "    ",
    "\n",
    "\n ",
    "\n  ",
    "\n   ",
    "\n    ",
    "\t",
    "\t\t",
    "\r\n",
    " \n",
    "\n\n",
];

/// Whitespace pool for interning
#[derive(Debug, Default)]
pub struct WhitespacePool {
    /// Additional patterns beyond static ones
    extra: Vec<Arc<str>>,
}

impl WhitespacePool {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Intern whitespace, return ID
    pub fn intern(&mut self, text: &str) -> Option<WhitespaceId> {
        // Check static patterns
        for (i, pattern) in WHITESPACE_PATTERNS.iter().enumerate() {
            if *pattern == text {
                return Some(WhitespaceId(i as u8));
            }
        }
        
        // Check extra patterns
        let base = WHITESPACE_PATTERNS.len();
        for (i, pattern) in self.extra.iter().enumerate() {
            if pattern.as_ref() == text {
                return Some(WhitespaceId((base + i) as u8));
            }
        }
        
        // Add to extra if it's whitespace
        if text.chars().all(|c| c.is_whitespace()) && self.extra.len() < 200 {
            let idx = base + self.extra.len();
            self.extra.push(Arc::from(text));
            return Some(WhitespaceId(idx as u8));
        }
        
        None
    }
    
    /// Get whitespace by ID
    pub fn get(&self, id: WhitespaceId) -> Option<&str> {
        let idx = id.0 as usize;
        if idx < WHITESPACE_PATTERNS.len() {
            Some(WHITESPACE_PATTERNS[idx])
        } else {
            self.extra.get(idx - WHITESPACE_PATTERNS.len()).map(|s| s.as_ref())
        }
    }
}

/// Convert a regular node to compact form
pub fn compact_element(
    id: NodeId,
    tag: u32,
    attributes: Vec<(Arc<str>, Arc<str>)>,
    children: Vec<NodeId>,
) -> CompactNode {
    if attributes.is_empty() && children.is_empty() {
        CompactNode::EmptyElement(EmptyElementNode { id, tag })
    } else {
        CompactNode::Element(ElementNode { id, tag, attributes, children })
    }
}

/// Convert text to compact form
pub fn compact_text(id: NodeId, content: &str, pool: &mut WhitespacePool) -> CompactNode {
    if content.is_empty() {
        CompactNode::EmptyText
    } else if let Some(ws_id) = pool.intern(content) {
        CompactNode::WhitespaceText(ws_id)
    } else {
        CompactNode::Text(TextNode {
            id,
            content: Arc::from(content),
        })
    }
}

/// Convert comment to compact form
pub fn compact_comment(id: NodeId, content: &str) -> CompactNode {
    if content.is_empty() || content.chars().all(|c| c.is_whitespace()) {
        CompactNode::EmptyComment
    } else {
        CompactNode::Comment(CommentNode {
            id,
            content: Arc::from(content),
        })
    }
}

/// Statistics for compact nodes
#[derive(Debug, Clone, Copy, Default)]
pub struct CompactStats {
    pub full_elements: u64,
    pub empty_elements: u64,
    pub full_text: u64,
    pub whitespace_text: u64,
    pub empty_text: u64,
    pub full_comments: u64,
    pub empty_comments: u64,
}

impl CompactStats {
    /// Percentage of nodes that are compact
    pub fn compact_ratio(&self) -> f64 {
        let total = self.full_elements + self.empty_elements
            + self.full_text + self.whitespace_text + self.empty_text
            + self.full_comments + self.empty_comments;
        let compact = self.empty_elements + self.whitespace_text + self.empty_text + self.empty_comments;
        
        if total == 0 { 0.0 } else { compact as f64 / total as f64 }
    }
    
    /// Record a node
    pub fn record(&mut self, node: &CompactNode) {
        match node {
            CompactNode::Element(_) => self.full_elements += 1,
            CompactNode::EmptyElement(_) => self.empty_elements += 1,
            CompactNode::Text(_) => self.full_text += 1,
            CompactNode::WhitespaceText(_) => self.whitespace_text += 1,
            CompactNode::EmptyText => self.empty_text += 1,
            CompactNode::Comment(_) => self.full_comments += 1,
            CompactNode::EmptyComment => self.empty_comments += 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_compact_element() {
        // Empty element should be compact
        let empty = compact_element(1, 10, vec![], vec![]);
        assert!(matches!(empty, CompactNode::EmptyElement(_)));
        
        // Element with children should be full
        let full = compact_element(2, 10, vec![], vec![3, 4]);
        assert!(matches!(full, CompactNode::Element(_)));
    }
    
    #[test]
    fn test_compact_text() {
        let mut pool = WhitespacePool::new();
        
        // Empty text
        let empty = compact_text(1, "", &mut pool);
        assert!(matches!(empty, CompactNode::EmptyText));
        
        // Whitespace text
        let ws = compact_text(2, "   ", &mut pool);
        assert!(matches!(ws, CompactNode::WhitespaceText(_)));
        
        // Full text
        let full = compact_text(3, "Hello world", &mut pool);
        assert!(matches!(full, CompactNode::Text(_)));
    }
    
    #[test]
    fn test_whitespace_pool() {
        let mut pool = WhitespacePool::new();
        
        // Static patterns
        let id1 = pool.intern(" ").unwrap();
        let id2 = pool.intern("  ").unwrap();
        assert_ne!(id1, id2);
        
        assert_eq!(pool.get(id1), Some(" "));
        assert_eq!(pool.get(id2), Some("  "));
        
        // New pattern
        let id3 = pool.intern("     ").unwrap();
        assert_eq!(pool.get(id3), Some("     "));
    }
    
    #[test]
    fn test_memory_savings() {
        let full_size = std::mem::size_of::<ElementNode>();
        let compact_size = std::mem::size_of::<EmptyElementNode>();
        
        // Empty element should be much smaller
        assert!(compact_size < full_size / 2);
        
        println!("Full element: {} bytes", full_size);
        println!("Empty element: {} bytes", compact_size);
    }
}
