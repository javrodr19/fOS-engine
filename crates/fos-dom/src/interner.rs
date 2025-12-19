//! String Interner - Deduplicate strings to save memory
//!
//! Common strings like tag names ("div", "span", "p") and attribute names
//! ("class", "id", "href") are stored once and referenced by ID.

use std::collections::HashMap;

/// Interned string ID - just 4 bytes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(transparent)]
pub struct InternedString(pub u32);

impl InternedString {
    /// Empty string
    pub const EMPTY: InternedString = InternedString(0);
}

/// String interner for deduplicating strings
/// 
/// Memory layout:
/// - All strings stored in a single contiguous buffer
/// - Each InternedString is just a 4-byte offset
pub struct StringInterner {
    /// All strings concatenated with null terminators
    pub buffer: String,
    /// Map from string content to offset
    map: HashMap<Box<str>, u32>,
    /// Offsets into buffer for each interned string
    pub offsets: Vec<u32>,
}

impl StringInterner {
    /// Create a new string interner with common HTML strings pre-interned
    pub fn new() -> Self {
        let mut interner = Self {
            buffer: String::with_capacity(4096), // Pre-allocate for common strings
            map: HashMap::with_capacity(256),
            offsets: Vec::with_capacity(256),
        };
        
        // Pre-intern empty string at index 0
        interner.intern("");
        
        // Pre-intern common HTML tag names
        const COMMON_TAGS: &[&str] = &[
            "html", "head", "body", "div", "span", "p", "a", "img",
            "ul", "ol", "li", "table", "tr", "td", "th", "thead", "tbody",
            "form", "input", "button", "select", "option", "textarea",
            "h1", "h2", "h3", "h4", "h5", "h6",
            "header", "footer", "nav", "main", "section", "article", "aside",
            "script", "style", "link", "meta", "title",
            "br", "hr", "strong", "em", "b", "i", "u",
            "video", "audio", "canvas", "svg", "iframe",
        ];
        
        // Pre-intern common attribute names
        const COMMON_ATTRS: &[&str] = &[
            "id", "class", "style", "href", "src", "alt", "title",
            "type", "name", "value", "placeholder", "disabled", "checked",
            "width", "height", "data", "role", "aria-label",
            "onclick", "onload", "onsubmit",
        ];
        
        for tag in COMMON_TAGS {
            interner.intern(tag);
        }
        for attr in COMMON_ATTRS {
            interner.intern(attr);
        }
        
        interner
    }
    
    /// Intern a string, returning its ID
    /// If the string is already interned, returns the existing ID
    pub fn intern(&mut self, s: &str) -> InternedString {
        // Check if already interned
        if let Some(&offset) = self.map.get(s) {
            return InternedString(offset);
        }
        
        // Add to buffer
        let offset = self.offsets.len() as u32;
        let start = self.buffer.len();
        self.buffer.push_str(s);
        self.buffer.push('\0'); // Null terminator for easy C interop
        
        // Store offset
        self.offsets.push(start as u32);
        self.map.insert(s.into(), offset);
        
        InternedString(offset)
    }
    
    /// Get the string for an interned ID
    #[inline]
    pub fn get(&self, id: InternedString) -> &str {
        let offset = self.offsets.get(id.0 as usize).copied().unwrap_or(0) as usize;
        let end = self.buffer[offset..].find('\0').unwrap_or(0);
        &self.buffer[offset..offset + end]
    }
    
    /// Number of interned strings
    pub fn len(&self) -> usize {
        self.offsets.len()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }
    
    /// Total memory used by the interner
    pub fn memory_usage(&self) -> usize {
        self.buffer.capacity() 
            + self.map.capacity() * (std::mem::size_of::<Box<str>>() + std::mem::size_of::<u32>())
            + self.offsets.capacity() * std::mem::size_of::<u32>()
    }
}

impl Default for StringInterner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_intern_dedup() {
        let mut interner = StringInterner::new();
        let id1 = interner.intern("hello");
        let id2 = interner.intern("hello");
        assert_eq!(id1, id2);
    }
    
    #[test]
    fn test_get_string() {
        let mut interner = StringInterner::new();
        let id = interner.intern("world");
        assert_eq!(interner.get(id), "world");
    }
}
