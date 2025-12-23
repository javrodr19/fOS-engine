//! String Interning Integration
//!
//! Deduplicates strings for memory efficiency.

use std::collections::HashMap;
use std::sync::Arc;

/// Interned string reference (4 bytes vs String)
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct InternedString { id: u32 }

impl InternedString { pub fn id(&self) -> u32 { self.id } }

impl std::fmt::Debug for InternedString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "InternedString({})", self.id)
    }
}

/// String interner
#[derive(Debug, Default)]
pub struct StringInterner {
    strings: Vec<Arc<str>>,
    lookup: HashMap<Arc<str>, u32>,
}

impl StringInterner {
    pub fn new() -> Self { Self::default() }
    
    pub fn intern(&mut self, s: &str) -> InternedString {
        if let Some(&id) = self.lookup.get(s) {
            return InternedString { id };
        }
        let id = self.strings.len() as u32;
        let arc: Arc<str> = s.into();
        self.strings.push(arc.clone());
        self.lookup.insert(arc, id);
        InternedString { id }
    }
    
    pub fn get(&self, interned: &InternedString) -> Option<&str> {
        self.strings.get(interned.id as usize).map(|s| s.as_ref())
    }
    
    pub fn len(&self) -> usize { self.strings.len() }
    pub fn is_empty(&self) -> bool { self.strings.is_empty() }
}

/// HTML tag interner (pre-populated)
#[derive(Debug, Default)]
pub struct TagInterner { interner: StringInterner }

impl TagInterner {
    pub fn new() -> Self {
        let mut interner = StringInterner::new();
        for tag in ["div", "span", "p", "a", "img", "button", "input", "form", "ul", "li", "table", "tr", "td", "h1", "h2", "h3", "h4", "h5", "h6", "header", "footer", "nav", "main", "section", "article"] {
            interner.intern(tag);
        }
        Self { interner }
    }
    
    pub fn intern(&mut self, tag: &str) -> InternedString { self.interner.intern(tag) }
    pub fn get(&self, interned: &InternedString) -> Option<&str> { self.interner.get(interned) }
}

/// CSS property interner (pre-populated)
#[derive(Debug, Default)]
pub struct CssPropInterner { interner: StringInterner }

impl CssPropInterner {
    pub fn new() -> Self {
        let mut interner = StringInterner::new();
        for prop in ["display", "position", "width", "height", "margin", "padding", "border", "background", "color", "font-size", "font-family", "flex", "grid", "opacity", "z-index", "overflow", "transform", "transition"] {
            interner.intern(prop);
        }
        Self { interner }
    }
    
    pub fn intern(&mut self, prop: &str) -> InternedString { self.interner.intern(prop) }
    pub fn get(&self, interned: &InternedString) -> Option<&str> { self.interner.get(interned) }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_string_interner() {
        let mut interner = StringInterner::new();
        let id1 = interner.intern("hello");
        let id2 = interner.intern("world");
        let id3 = interner.intern("hello");
        
        assert_eq!(id1, id3);
        assert_ne!(id1, id2);
        assert_eq!(interner.get(&id1), Some("hello"));
    }
    
    #[test]
    fn test_tag_interner() {
        let mut interner = TagInterner::new();
        let div = interner.intern("div");
        assert_eq!(interner.get(&div), Some("div"));
    }
    
    #[test]
    fn test_memory_savings() {
        // InternedString is 4 bytes vs String's 24+ bytes
        assert_eq!(std::mem::size_of::<InternedString>(), 4);
    }
}
