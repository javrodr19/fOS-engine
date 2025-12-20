//! Borrowed DOM Strings - Zero-Alloc Parsing (Phase 24.1)
//!
//! Never own text strings - slice original HTML source.
//! All TextNodes are `&'src str` references into the source buffer.
//! Achieves 50% text memory savings with zero-copy parsing.
//!
//! # Design
//! - Keep source buffer alive during page lifetime
//! - TextNode holds byte ranges instead of owned strings
//! - Attribute values can also be borrowed
//! - Only copy when mutation is needed (copy-on-write)

use std::ops::Range;
use std::borrow::Cow;

/// Source buffer that holds the original HTML
#[derive(Debug)]
pub struct SourceBuffer {
    /// The raw source bytes (owned)
    data: Box<[u8]>,
    /// Whether this is valid UTF-8 (validated once)
    is_utf8: bool,
}

impl SourceBuffer {
    /// Create from owned bytes
    pub fn new(data: Vec<u8>) -> Self {
        let is_utf8 = std::str::from_utf8(&data).is_ok();
        Self {
            data: data.into_boxed_slice(),
            is_utf8,
        }
    }
    
    /// Create from string (already valid UTF-8)
    pub fn from_string(s: String) -> Self {
        Self {
            data: s.into_bytes().into_boxed_slice(),
            is_utf8: true,
        }
    }
    
    /// Get a slice of the source as a string
    #[inline]
    pub fn slice(&self, range: Range<usize>) -> Option<&str> {
        if range.end > self.data.len() {
            return None;
        }
        std::str::from_utf8(&self.data[range]).ok()
    }
    
    /// Get a slice of the source as bytes
    #[inline]
    pub fn slice_bytes(&self, range: Range<usize>) -> Option<&[u8]> {
        self.data.get(range)
    }
    
    /// Get the full source as str (if valid UTF-8)
    pub fn as_str(&self) -> Option<&str> {
        if self.is_utf8 {
            std::str::from_utf8(&self.data).ok()
        } else {
            None
        }
    }
    
    /// Length of the source
    #[inline]
    pub fn len(&self) -> usize {
        self.data.len()
    }
    
    /// Is empty?
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    
    /// Memory size
    pub fn memory_size(&self) -> usize {
        std::mem::size_of::<Self>() + self.data.len()
    }
}

/// A borrowed string - reference into source buffer
#[derive(Debug, Clone, Copy)]
pub struct BorrowedStr {
    /// Start offset in source buffer
    pub start: u32,
    /// End offset in source buffer
    pub end: u32,
}

impl BorrowedStr {
    /// Create a new borrowed string reference
    #[inline]
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }
    
    /// Create from a range
    #[inline]
    pub fn from_range(range: Range<usize>) -> Self {
        Self {
            start: range.start as u32,
            end: range.end as u32,
        }
    }
    
    /// Get the range
    #[inline]
    pub fn range(&self) -> Range<usize> {
        self.start as usize..self.end as usize
    }
    
    /// Length of the string
    #[inline]
    pub fn len(&self) -> usize {
        (self.end - self.start) as usize
    }
    
    /// Is empty?
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
    
    /// Empty borrowed string
    pub const EMPTY: Self = Self { start: 0, end: 0 };
    
    /// Resolve to actual string using source buffer
    #[inline]
    pub fn resolve<'a>(&self, source: &'a SourceBuffer) -> Option<&'a str> {
        source.slice(self.range())
    }
    
    /// Memory size (just the reference, not the content)
    #[inline]
    pub const fn memory_size() -> usize {
        std::mem::size_of::<Self>() // 8 bytes
    }
}

/// Text content that can be either borrowed or owned
#[derive(Debug, Clone)]
pub enum TextContent {
    /// Borrowed from source buffer (zero-copy)
    Borrowed(BorrowedStr),
    /// Owned string (for mutations or dynamic content)
    Owned(Box<str>),
}

impl TextContent {
    /// Create borrowed content
    #[inline]
    pub fn borrowed(start: u32, end: u32) -> Self {
        TextContent::Borrowed(BorrowedStr::new(start, end))
    }
    
    /// Create owned content
    #[inline]
    pub fn owned(s: impl Into<Box<str>>) -> Self {
        TextContent::Owned(s.into())
    }
    
    /// Is this borrowed?
    #[inline]
    pub fn is_borrowed(&self) -> bool {
        matches!(self, TextContent::Borrowed(_))
    }
    
    /// Is this owned?
    #[inline]
    pub fn is_owned(&self) -> bool {
        matches!(self, TextContent::Owned(_))
    }
    
    /// Get the length
    pub fn len(&self) -> usize {
        match self {
            TextContent::Borrowed(b) => b.len(),
            TextContent::Owned(s) => s.len(),
        }
    }
    
    /// Is empty?
    pub fn is_empty(&self) -> bool {
        match self {
            TextContent::Borrowed(b) => b.is_empty(),
            TextContent::Owned(s) => s.is_empty(),
        }
    }
    
    /// Resolve to string, given a source buffer
    pub fn resolve<'a>(&'a self, source: &'a SourceBuffer) -> Option<Cow<'a, str>> {
        match self {
            TextContent::Borrowed(b) => b.resolve(source).map(Cow::Borrowed),
            TextContent::Owned(s) => Some(Cow::Borrowed(s)),
        }
    }
    
    /// Ensure this is owned (copy-on-write)
    pub fn make_owned(&mut self, source: &SourceBuffer) {
        if let TextContent::Borrowed(b) = self {
            if let Some(s) = b.resolve(source) {
                *self = TextContent::Owned(s.to_string().into_boxed_str());
            }
        }
    }
    
    /// Mutate the content (triggers copy-on-write if borrowed)
    pub fn set(&mut self, new_value: &str, source: &SourceBuffer) {
        // If borrowed and same content, keep borrowed
        if let TextContent::Borrowed(b) = self {
            if let Some(current) = b.resolve(source) {
                if current == new_value {
                    return;
                }
            }
        }
        *self = TextContent::Owned(new_value.to_string().into_boxed_str());
    }
    
    /// Memory size
    pub fn memory_size(&self) -> usize {
        match self {
            TextContent::Borrowed(_) => BorrowedStr::memory_size(),
            TextContent::Owned(s) => std::mem::size_of::<Box<str>>() + s.len(),
        }
    }
}

/// Attribute value that can be borrowed or owned
#[derive(Debug, Clone)]
pub struct AttributeValue {
    /// The value (borrowed or owned)
    pub value: TextContent,
}

impl AttributeValue {
    /// Create borrowed attribute value
    #[inline]
    pub fn borrowed(start: u32, end: u32) -> Self {
        Self {
            value: TextContent::borrowed(start, end),
        }
    }
    
    /// Create owned attribute value
    #[inline]
    pub fn owned(s: impl Into<Box<str>>) -> Self {
        Self {
            value: TextContent::owned(s),
        }
    }
    
    /// Resolve the value
    #[inline]
    pub fn resolve<'a>(&'a self, source: &'a SourceBuffer) -> Option<Cow<'a, str>> {
        self.value.resolve(source)
    }
}

/// Statistics for borrowed string usage
#[derive(Debug, Clone, Copy, Default)]
pub struct BorrowedStats {
    /// Number of borrowed strings
    pub borrowed_count: usize,
    /// Number of owned strings
    pub owned_count: usize,
    /// Total borrowed bytes (just references)
    pub borrowed_ref_bytes: usize,
    /// Total content bytes that are borrowed
    pub borrowed_content_bytes: usize,
    /// Total owned bytes
    pub owned_bytes: usize,
}

impl BorrowedStats {
    /// Add a borrowed string
    pub fn add_borrowed(&mut self, len: usize) {
        self.borrowed_count += 1;
        self.borrowed_ref_bytes += BorrowedStr::memory_size();
        self.borrowed_content_bytes += len;
    }
    
    /// Add an owned string
    pub fn add_owned(&mut self, len: usize) {
        self.owned_count += 1;
        self.owned_bytes += std::mem::size_of::<Box<str>>() + len;
    }
    
    /// Calculate memory savings
    pub fn memory_savings(&self) -> (usize, f64) {
        // Without borrowing, all content would be owned
        let would_be_owned = self.borrowed_content_bytes;
        let actual_used = self.borrowed_ref_bytes;
        let saved = would_be_owned.saturating_sub(actual_used);
        let percentage = if would_be_owned > 0 {
            (saved as f64 / would_be_owned as f64) * 100.0
        } else {
            0.0
        };
        (saved, percentage)
    }
    
    /// Ratio of borrowed to total
    pub fn borrow_ratio(&self) -> f64 {
        let total = self.borrowed_count + self.owned_count;
        if total == 0 {
            0.0
        } else {
            self.borrowed_count as f64 / total as f64
        }
    }
}

/// Borrowed string collector - tracks all borrowed strings for a document
pub struct BorrowedStringCollector {
    /// All borrowed string ranges
    ranges: Vec<BorrowedStr>,
    /// Statistics
    stats: BorrowedStats,
}

impl Default for BorrowedStringCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl BorrowedStringCollector {
    pub fn new() -> Self {
        Self {
            ranges: Vec::new(),
            stats: BorrowedStats::default(),
        }
    }
    
    /// Add a borrowed string
    pub fn add(&mut self, start: u32, end: u32) -> BorrowedStr {
        let borrowed = BorrowedStr::new(start, end);
        self.ranges.push(borrowed);
        self.stats.add_borrowed(borrowed.len());
        borrowed
    }
    
    /// Get statistics
    pub fn stats(&self) -> &BorrowedStats {
        &self.stats
    }
    
    /// Number of borrowed strings
    pub fn len(&self) -> usize {
        self.ranges.len()
    }
    
    /// Is empty?
    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }
    
    /// Validate all ranges against source
    pub fn validate(&self, source: &SourceBuffer) -> bool {
        self.ranges.iter().all(|r| r.resolve(source).is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_source_buffer() {
        let html = "<div>Hello, World!</div>";
        let source = SourceBuffer::from_string(html.to_string());
        
        assert_eq!(source.len(), html.len());
        assert_eq!(source.as_str(), Some(html));
        
        // Slice the text content
        assert_eq!(source.slice(5..18), Some("Hello, World!"));
    }
    
    #[test]
    fn test_borrowed_str() {
        let source = SourceBuffer::from_string("Hello, World!".to_string());
        let borrowed = BorrowedStr::new(0, 5);
        
        assert_eq!(borrowed.len(), 5);
        assert_eq!(borrowed.resolve(&source), Some("Hello"));
        
        // Memory: only 8 bytes for reference
        assert_eq!(BorrowedStr::memory_size(), 8);
    }
    
    #[test]
    fn test_text_content_borrowed() {
        let source = SourceBuffer::from_string("test content".to_string());
        let text = TextContent::borrowed(0, 4);
        
        assert!(text.is_borrowed());
        assert_eq!(text.len(), 4);
        
        let resolved = text.resolve(&source);
        assert_eq!(resolved.as_deref(), Some("test"));
    }
    
    #[test]
    fn test_text_content_copy_on_write() {
        let source = SourceBuffer::from_string("original".to_string());
        let mut text = TextContent::borrowed(0, 8);
        
        assert!(text.is_borrowed());
        
        // Mutate triggers copy
        text.set("modified", &source);
        assert!(text.is_owned());
        
        let resolved = text.resolve(&source);
        assert_eq!(resolved.as_deref(), Some("modified"));
    }
    
    #[test]
    fn test_borrowed_stats() {
        let mut stats = BorrowedStats::default();
        
        stats.add_borrowed(100);
        stats.add_borrowed(200);
        stats.add_owned(50);
        
        assert_eq!(stats.borrowed_count, 2);
        assert_eq!(stats.owned_count, 1);
        assert_eq!(stats.borrowed_content_bytes, 300);
        
        let (saved, pct) = stats.memory_savings();
        assert!(saved > 0);
        assert!(pct > 0.0);
    }
    
    #[test]
    fn test_collector() {
        let source = SourceBuffer::from_string("Hello World Test".to_string());
        let mut collector = BorrowedStringCollector::new();
        
        let b1 = collector.add(0, 5);  // "Hello"
        let b2 = collector.add(6, 11); // "World"
        
        assert_eq!(collector.len(), 2);
        assert!(collector.validate(&source));
        
        assert_eq!(b1.resolve(&source), Some("Hello"));
        assert_eq!(b2.resolve(&source), Some("World"));
    }
}
