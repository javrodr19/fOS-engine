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

/// String entry storing offset and length
#[derive(Debug, Clone, Copy)]
struct StringEntry {
    offset: u32,
    len: u32,
}

/// String interner for deduplicating strings
/// 
/// Memory layout:
/// - All strings stored in a single contiguous buffer
/// - Each InternedString is just a 4-byte index
/// - Lengths stored separately (supports null bytes in strings)
pub struct StringInterner {
    /// All strings concatenated (no delimiters)
    pub buffer: String,
    /// Map from string content to index
    pub(crate) map: HashMap<Box<str>, u32>,
    /// Entries storing (offset, length) for each interned string
    entries: Vec<StringEntry>,
}

impl StringInterner {
    /// Create a new string interner with common HTML strings pre-interned
    pub fn new() -> Self {
        let mut interner = Self {
            buffer: String::with_capacity(4096), // Pre-allocate for common strings
            map: HashMap::with_capacity(256),
            entries: Vec::with_capacity(256),
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
        if let Some(&index) = self.map.get(s) {
            return InternedString(index);
        }
        
        // Add to buffer
        let index = self.entries.len() as u32;
        let offset = self.buffer.len() as u32;
        let len = s.len() as u32;
        
        self.buffer.push_str(s);
        
        // Store entry
        self.entries.push(StringEntry { offset, len });
        self.map.insert(s.into(), index);
        
        InternedString(index)
    }
    
    /// Get the string for an interned ID
    #[inline]
    pub fn get(&self, id: InternedString) -> &str {
        if let Some(entry) = self.entries.get(id.0 as usize) {
            let start = entry.offset as usize;
            let end = start + entry.len as usize;
            &self.buffer[start..end]
        } else {
            ""
        }
    }
    
    /// Number of interned strings
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    
    /// Total memory used by the interner
    pub fn memory_usage(&self) -> usize {
        self.buffer.capacity() 
            + self.map.capacity() * (std::mem::size_of::<Box<str>>() + std::mem::size_of::<u32>())
            + self.entries.capacity() * std::mem::size_of::<StringEntry>()
    }
    
    /// Get offsets for backward compatibility (used by document.rs)
    pub fn offsets(&self) -> impl Iterator<Item = u32> + '_ {
        self.entries.iter().map(|e| e.offset)
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
    
    #[test]
    fn test_null_bytes() {
        let mut interner = StringInterner::new();
        let s = "null\0byte\0string";
        let id = interner.intern(s);
        assert_eq!(interner.get(id), s, "Null bytes should be preserved");
    }
}
