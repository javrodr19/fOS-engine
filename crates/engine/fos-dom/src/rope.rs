//! Rope Data Structure
//!
//! Efficient rope data structure for text content manipulation.
//! Provides O(log n) insert/delete operations at arbitrary positions.

use std::ops::Range;
use std::sync::Arc;

/// Maximum leaf size (bytes)
const MAX_LEAF_SIZE: usize = 512;

/// Minimum leaf size before merging
const MIN_LEAF_SIZE: usize = 128;

/// Rope data structure for efficient text manipulation
#[derive(Debug, Clone)]
pub struct Rope {
    /// Root node
    root: Arc<RopeNode>,
}

/// Rope node (internal or leaf)
#[derive(Debug, Clone)]
enum RopeNode {
    /// Internal node with two children
    Branch {
        /// Left child
        left: Arc<RopeNode>,
        /// Right child
        right: Arc<RopeNode>,
        /// Total length in bytes
        len: usize,
        /// Line count in this subtree
        lines: usize,
        /// Height of subtree
        height: u8,
    },
    /// Leaf node with actual text
    Leaf {
        /// Text content
        text: String,
    },
}

impl RopeNode {
    /// Get total byte length
    fn len(&self) -> usize {
        match self {
            RopeNode::Branch { len, .. } => *len,
            RopeNode::Leaf { text } => text.len(),
        }
    }
    
    /// Check if empty
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    
    /// Get line count
    fn lines(&self) -> usize {
        match self {
            RopeNode::Branch { lines, .. } => *lines,
            RopeNode::Leaf { text } => text.chars().filter(|&c| c == '\n').count(),
        }
    }
    
    /// Get height
    fn height(&self) -> u8 {
        match self {
            RopeNode::Branch { height, .. } => *height,
            RopeNode::Leaf { .. } => 0,
        }
    }
    
    /// Create leaf node
    fn leaf(text: String) -> Arc<RopeNode> {
        Arc::new(RopeNode::Leaf { text })
    }
    
    /// Create branch node
    fn branch(left: Arc<RopeNode>, right: Arc<RopeNode>) -> Arc<RopeNode> {
        let len = left.len() + right.len();
        let lines = left.lines() + right.lines();
        let height = left.height().max(right.height()) + 1;
        
        Arc::new(RopeNode::Branch {
            left,
            right,
            len,
            lines,
            height,
        })
    }
    
    /// Concatenate two nodes with balancing
    fn concat(left: Arc<RopeNode>, right: Arc<RopeNode>) -> Arc<RopeNode> {
        if left.is_empty() {
            return right;
        }
        if right.is_empty() {
            return left;
        }
        
        // Check if we can merge leaves
        if let (RopeNode::Leaf { text: l }, RopeNode::Leaf { text: r }) = (&*left, &*right) {
            if l.len() + r.len() <= MAX_LEAF_SIZE {
                return RopeNode::leaf(format!("{}{}", l, r));
            }
        }
        
        // Create branch and potentially rebalance
        let node = RopeNode::branch(left, right);
        RopeNode::rebalance(node)
    }
    
    /// Rebalance if needed
    fn rebalance(node: Arc<RopeNode>) -> Arc<RopeNode> {
        if let RopeNode::Branch { left, right, .. } = &*node {
            let left_h = left.height();
            let right_h = right.height();
            
            // Rebalance if heights differ by more than 1
            if left_h > right_h + 1 {
                if let RopeNode::Branch { left: ll, right: lr, .. } = &**left {
                    if ll.height() >= lr.height() {
                        // Right rotation
                        let new_right = RopeNode::branch(lr.clone(), right.clone());
                        return RopeNode::branch(ll.clone(), new_right);
                    } else {
                        // Left-Right rotation
                        if let RopeNode::Branch { left: lrl, right: lrr, .. } = &**lr {
                            let new_left = RopeNode::branch(ll.clone(), lrl.clone());
                            let new_right = RopeNode::branch(lrr.clone(), right.clone());
                            return RopeNode::branch(new_left, new_right);
                        }
                    }
                }
            } else if right_h > left_h + 1 {
                if let RopeNode::Branch { left: rl, right: rr, .. } = &**right {
                    if rr.height() >= rl.height() {
                        // Left rotation
                        let new_left = RopeNode::branch(left.clone(), rl.clone());
                        return RopeNode::branch(new_left, rr.clone());
                    } else {
                        // Right-Left rotation
                        if let RopeNode::Branch { left: rll, right: rlr, .. } = &**rl {
                            let new_left = RopeNode::branch(left.clone(), rll.clone());
                            let new_right = RopeNode::branch(rlr.clone(), rr.clone());
                            return RopeNode::branch(new_left, new_right);
                        }
                    }
                }
            }
        }
        
        node
    }
    
    /// Split node at byte position
    fn split(node: &Arc<RopeNode>, pos: usize) -> (Arc<RopeNode>, Arc<RopeNode>) {
        if pos == 0 {
            return (RopeNode::leaf(String::new()), node.clone());
        }
        if pos >= node.len() {
            return (node.clone(), RopeNode::leaf(String::new()));
        }
        
        match &**node {
            RopeNode::Leaf { text } => {
                let left = text[..pos].to_string();
                let right = text[pos..].to_string();
                (RopeNode::leaf(left), RopeNode::leaf(right))
            }
            RopeNode::Branch { left, right, .. } => {
                let left_len = left.len();
                if pos < left_len {
                    let (ll, lr) = RopeNode::split(left, pos);
                    (ll, RopeNode::concat(lr, right.clone()))
                } else {
                    let (rl, rr) = RopeNode::split(right, pos - left_len);
                    (RopeNode::concat(left.clone(), rl), rr)
                }
            }
        }
    }
    
    /// Get character at byte position
    fn char_at(&self, pos: usize) -> Option<char> {
        match self {
            RopeNode::Leaf { text } => text[pos..].chars().next(),
            RopeNode::Branch { left, right, .. } => {
                let left_len = left.len();
                if pos < left_len {
                    left.char_at(pos)
                } else {
                    right.char_at(pos - left_len)
                }
            }
        }
    }
    
    /// Get slice
    fn slice(&self, range: Range<usize>) -> String {
        let start = range.start;
        let end = range.end.min(self.len());
        
        if start >= end {
            return String::new();
        }
        
        match self {
            RopeNode::Leaf { text } => text[start..end].to_string(),
            RopeNode::Branch { left, right, .. } => {
                let left_len = left.len();
                if end <= left_len {
                    left.slice(range)
                } else if start >= left_len {
                    right.slice(start - left_len..end - left_len)
                } else {
                    let left_part = left.slice(start..left_len);
                    let right_part = right.slice(0..end - left_len);
                    format!("{}{}", left_part, right_part)
                }
            }
        }
    }
}

impl Default for Rope {
    fn default() -> Self {
        Self::new()
    }
}

impl Rope {
    /// Create empty rope
    pub fn new() -> Self {
        Self {
            root: RopeNode::leaf(String::new()),
        }
    }
    
    /// Create rope from string
    pub fn from_str(text: &str) -> Self {
        if text.is_empty() {
            return Self::new();
        }
        
        // Split into chunks for balanced tree
        let chunks: Vec<String> = text
            .as_bytes()
            .chunks(MAX_LEAF_SIZE)
            .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
            .collect();
        
        Self {
            root: Self::build_tree(&chunks),
        }
    }
    
    fn build_tree(chunks: &[String]) -> Arc<RopeNode> {
        match chunks.len() {
            0 => RopeNode::leaf(String::new()),
            1 => RopeNode::leaf(chunks[0].clone()),
            _ => {
                let mid = chunks.len() / 2;
                let left = Self::build_tree(&chunks[..mid]);
                let right = Self::build_tree(&chunks[mid..]);
                RopeNode::branch(left, right)
            }
        }
    }
    
    /// Get total byte length
    pub fn len(&self) -> usize {
        self.root.len()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.root.is_empty()
    }
    
    /// Get line count
    pub fn lines(&self) -> usize {
        self.root.lines() + 1 // Lines = newlines + 1
    }
    
    /// Insert text at byte position
    pub fn insert(&mut self, pos: usize, text: &str) {
        if text.is_empty() {
            return;
        }
        
        let pos = pos.min(self.len());
        let (left, right) = RopeNode::split(&self.root, pos);
        let middle = Rope::from_str(text).root;
        
        let temp = RopeNode::concat(left, middle);
        self.root = RopeNode::concat(temp, right);
    }
    
    /// Delete byte range
    pub fn delete(&mut self, range: Range<usize>) {
        let start = range.start.min(self.len());
        let end = range.end.min(self.len());
        
        if start >= end {
            return;
        }
        
        let (left, temp) = RopeNode::split(&self.root, start);
        let (_, right) = RopeNode::split(&temp, end - start);
        
        self.root = RopeNode::concat(left, right);
    }
    
    /// Replace byte range with new text
    pub fn replace(&mut self, range: Range<usize>, text: &str) {
        self.delete(range.clone());
        self.insert(range.start, text);
    }
    
    /// Get character at byte position
    pub fn char_at(&self, pos: usize) -> Option<char> {
        self.root.char_at(pos)
    }
    
    /// Get slice of rope
    pub fn slice(&self, range: Range<usize>) -> RopeSlice<'_> {
        RopeSlice {
            rope: self,
            start: range.start,
            end: range.end.min(self.len()),
        }
    }
    
    /// Convert to string
    pub fn to_string(&self) -> String {
        self.root.slice(0..self.len())
    }
    
    /// Iterate over chunks
    pub fn chunks(&self) -> ChunkIter<'_> {
        ChunkIter {
            stack: vec![&self.root],
        }
    }
    
    /// Iterate over lines
    pub fn lines_iter(&self) -> LineIter<'_> {
        LineIter {
            rope: self,
            pos: 0,
        }
    }
    
    /// Append another rope
    pub fn append(&mut self, other: &Rope) {
        self.root = RopeNode::concat(self.root.clone(), other.root.clone());
    }
    
    /// Split rope at position
    pub fn split_off(&mut self, pos: usize) -> Rope {
        let pos = pos.min(self.len());
        let (left, right) = RopeNode::split(&self.root, pos);
        self.root = left;
        Rope { root: right }
    }
}

impl From<&str> for Rope {
    fn from(text: &str) -> Self {
        Rope::from_str(text)
    }
}

impl From<String> for Rope {
    fn from(text: String) -> Self {
        Rope::from_str(&text)
    }
}

/// Zero-copy slice into rope
#[derive(Debug)]
pub struct RopeSlice<'a> {
    rope: &'a Rope,
    start: usize,
    end: usize,
}

impl<'a> RopeSlice<'a> {
    /// Get length
    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }
    
    /// Is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    
    /// Convert to string
    pub fn to_string(&self) -> String {
        self.rope.root.slice(self.start..self.end)
    }
    
    /// Get sub-slice
    pub fn slice(&self, range: Range<usize>) -> RopeSlice<'a> {
        let start = self.start + range.start;
        let end = (self.start + range.end).min(self.end);
        RopeSlice {
            rope: self.rope,
            start,
            end,
        }
    }
}

/// Iterator over rope chunks
pub struct ChunkIter<'a> {
    stack: Vec<&'a RopeNode>,
}

impl<'a> Iterator for ChunkIter<'a> {
    type Item = &'a str;
    
    fn next(&mut self) -> Option<Self::Item> {
        while let Some(node) = self.stack.pop() {
            match node {
                RopeNode::Leaf { text } => {
                    if !text.is_empty() {
                        return Some(text.as_str());
                    }
                }
                RopeNode::Branch { left, right, .. } => {
                    // Push right first so left is processed first
                    self.stack.push(right);
                    self.stack.push(left);
                }
            }
        }
        None
    }
}

/// Iterator over lines
pub struct LineIter<'a> {
    rope: &'a Rope,
    pos: usize,
}

impl<'a> Iterator for LineIter<'a> {
    type Item = String;
    
    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.rope.len() {
            return None;
        }
        
        // Find next newline
        let remaining = self.rope.slice(self.pos..self.rope.len()).to_string();
        if let Some(newline_pos) = remaining.find('\n') {
            let line = remaining[..newline_pos].to_string();
            self.pos += newline_pos + 1;
            Some(line)
        } else {
            // Last line without newline
            self.pos = self.rope.len();
            Some(remaining)
        }
    }
}

/// Rope builder for efficient construction
#[derive(Debug, Default)]
pub struct RopeBuilder {
    chunks: Vec<String>,
    current: String,
}

impl RopeBuilder {
    /// Create new builder
    pub fn new() -> Self {
        Self {
            chunks: Vec::new(),
            current: String::new(),
        }
    }
    
    /// Append text
    pub fn append(&mut self, text: &str) {
        self.current.push_str(text);
        
        // Flush if current chunk is large enough
        if self.current.len() >= MAX_LEAF_SIZE {
            self.flush();
        }
    }
    
    /// Append char
    pub fn push(&mut self, c: char) {
        self.current.push(c);
        
        if self.current.len() >= MAX_LEAF_SIZE {
            self.flush();
        }
    }
    
    /// Build the rope
    pub fn build(mut self) -> Rope {
        // Flush any remaining content
        if !self.current.is_empty() {
            self.chunks.push(std::mem::take(&mut self.current));
        }
        
        if self.chunks.is_empty() {
            return Rope::new();
        }
        
        Rope {
            root: Rope::build_tree(&self.chunks),
        }
    }
    
    fn flush(&mut self) {
        if !self.current.is_empty() {
            self.chunks.push(std::mem::take(&mut self.current));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_empty_rope() {
        let rope = Rope::new();
        assert!(rope.is_empty());
        assert_eq!(rope.len(), 0);
        assert_eq!(rope.lines(), 1);
    }
    
    #[test]
    fn test_from_str() {
        let rope = Rope::from_str("Hello, World!");
        assert_eq!(rope.len(), 13);
        assert_eq!(rope.to_string(), "Hello, World!");
    }
    
    #[test]
    fn test_insert() {
        let mut rope = Rope::from_str("Hello World");
        rope.insert(5, ",");
        assert_eq!(rope.to_string(), "Hello, World");
    }
    
    #[test]
    fn test_delete() {
        let mut rope = Rope::from_str("Hello, World!");
        rope.delete(5..6); // Delete just the comma
        assert_eq!(rope.to_string(), "Hello World!");
    }
    
    #[test]
    fn test_replace() {
        let mut rope = Rope::from_str("Hello World");
        rope.replace(0..5, "Goodbye");
        assert_eq!(rope.to_string(), "Goodbye World");
    }
    
    #[test]
    fn test_slice() {
        let rope = Rope::from_str("Hello, World!");
        let slice = rope.slice(0..5);
        assert_eq!(slice.to_string(), "Hello");
    }
    
    #[test]
    fn test_append() {
        let mut rope1 = Rope::from_str("Hello ");
        let rope2 = Rope::from_str("World");
        rope1.append(&rope2);
        assert_eq!(rope1.to_string(), "Hello World");
    }
    
    #[test]
    fn test_split_off() {
        let mut rope = Rope::from_str("Hello World");
        let right = rope.split_off(6);
        assert_eq!(rope.to_string(), "Hello ");
        assert_eq!(right.to_string(), "World");
    }
    
    #[test]
    fn test_lines() {
        let rope = Rope::from_str("Line 1\nLine 2\nLine 3");
        assert_eq!(rope.lines(), 3);
        
        let lines: Vec<_> = rope.lines_iter().collect();
        assert_eq!(lines, vec!["Line 1", "Line 2", "Line 3"]);
    }
    
    #[test]
    fn test_chunks() {
        let text = "Hello, World!";
        let rope = Rope::from_str(text);
        let chunks: String = rope.chunks().collect();
        assert_eq!(chunks, text);
    }
    
    #[test]
    fn test_large_rope() {
        let text = "a".repeat(10000);
        let rope = Rope::from_str(&text);
        assert_eq!(rope.len(), 10000);
        assert_eq!(rope.to_string(), text);
    }
    
    #[test]
    fn test_builder() {
        let mut builder = RopeBuilder::new();
        builder.append("Hello");
        builder.append(", ");
        builder.append("World");
        builder.push('!');
        
        let rope = builder.build();
        assert_eq!(rope.to_string(), "Hello, World!");
    }
    
    #[test]
    fn test_char_at() {
        let rope = Rope::from_str("Hello");
        assert_eq!(rope.char_at(0), Some('H'));
        assert_eq!(rope.char_at(1), Some('e'));
        assert_eq!(rope.char_at(4), Some('o'));
    }
    
    #[test]
    fn test_insert_at_boundaries() {
        let mut rope = Rope::from_str("Hello");
        
        // Insert at start
        rope.insert(0, "Say ");
        assert_eq!(rope.to_string(), "Say Hello");
        
        // Insert at end
        rope.insert(rope.len(), "!");
        assert_eq!(rope.to_string(), "Say Hello!");
    }
}
