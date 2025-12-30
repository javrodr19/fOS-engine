//! String Interning
//!
//! Deduplicates strings for memory efficiency.

use std::collections::HashMap;
use std::sync::Arc;

/// Interned string reference
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct InternedString {
    id: u32,
}

impl InternedString {
    pub fn id(&self) -> u32 {
        self.id
    }
}

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
    
    /// Intern a string
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
    
    /// Get string by ID
    pub fn get(&self, interned: &InternedString) -> Option<&str> {
        self.strings.get(interned.id as usize).map(|s| s.as_ref())
    }
    
    /// Number of interned strings
    pub fn len(&self) -> usize {
        self.strings.len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }
}

/// HTML tag name interner (pre-populated)
#[derive(Debug)]
pub struct TagInterner {
    interner: StringInterner,
}

impl TagInterner {
    pub fn new() -> Self {
        let mut interner = StringInterner::new();
        
        // Pre-intern common HTML tags
        for tag in HTML_TAGS {
            interner.intern(tag);
        }
        
        Self { interner }
    }
    
    pub fn intern(&mut self, tag: &str) -> InternedString {
        self.interner.intern(tag)
    }
    
    pub fn get(&self, interned: &InternedString) -> Option<&str> {
        self.interner.get(interned)
    }
}

impl Default for TagInterner {
    fn default() -> Self { Self::new() }
}

const HTML_TAGS: &[&str] = &[
    "a", "abbr", "address", "area", "article", "aside", "audio",
    "b", "base", "bdi", "bdo", "blockquote", "body", "br", "button",
    "canvas", "caption", "cite", "code", "col", "colgroup",
    "data", "datalist", "dd", "del", "details", "dfn", "dialog", "div", "dl", "dt",
    "em", "embed",
    "fieldset", "figcaption", "figure", "footer", "form",
    "h1", "h2", "h3", "h4", "h5", "h6", "head", "header", "hgroup", "hr", "html",
    "i", "iframe", "img", "input", "ins",
    "kbd",
    "label", "legend", "li", "link",
    "main", "map", "mark", "menu", "meta", "meter",
    "nav", "noscript",
    "object", "ol", "optgroup", "option", "output",
    "p", "param", "picture", "pre", "progress",
    "q",
    "rp", "rt", "ruby",
    "s", "samp", "script", "section", "select", "slot", "small", "source", "span", "strong", "style", "sub", "summary", "sup",
    "table", "tbody", "td", "template", "textarea", "tfoot", "th", "thead", "time", "title", "tr", "track",
    "u", "ul",
    "var", "video",
    "wbr",
];

/// Attribute name interner
#[derive(Debug)]
pub struct AttrInterner {
    interner: StringInterner,
}

impl AttrInterner {
    pub fn new() -> Self {
        let mut interner = StringInterner::new();
        
        // Pre-intern common attributes
        for attr in COMMON_ATTRS {
            interner.intern(attr);
        }
        
        Self { interner }
    }
    
    pub fn intern(&mut self, attr: &str) -> InternedString {
        self.interner.intern(attr)
    }
    
    pub fn get(&self, interned: &InternedString) -> Option<&str> {
        self.interner.get(interned)
    }
}

impl Default for AttrInterner {
    fn default() -> Self { Self::new() }
}

const COMMON_ATTRS: &[&str] = &[
    "id", "class", "style", "src", "href", "alt", "title",
    "type", "name", "value", "placeholder", "disabled", "checked",
    "data-", "aria-", "role", "tabindex",
    "width", "height", "loading", "rel", "target",
];

/// CSS property interner
#[derive(Debug)]
pub struct CssPropInterner {
    interner: StringInterner,
}

impl CssPropInterner {
    pub fn new() -> Self {
        let mut interner = StringInterner::new();
        
        // Pre-intern common CSS properties
        for prop in COMMON_CSS_PROPS {
            interner.intern(prop);
        }
        
        Self { interner }
    }
    
    pub fn intern(&mut self, prop: &str) -> InternedString {
        self.interner.intern(prop)
    }
    
    pub fn get(&self, interned: &InternedString) -> Option<&str> {
        self.interner.get(interned)
    }
}

impl Default for CssPropInterner {
    fn default() -> Self { Self::new() }
}

const COMMON_CSS_PROPS: &[&str] = &[
    "display", "position", "top", "right", "bottom", "left",
    "width", "height", "min-width", "max-width", "min-height", "max-height",
    "margin", "margin-top", "margin-right", "margin-bottom", "margin-left",
    "padding", "padding-top", "padding-right", "padding-bottom", "padding-left",
    "border", "border-width", "border-style", "border-color", "border-radius",
    "background", "background-color", "background-image",
    "color", "font", "font-size", "font-family", "font-weight",
    "text-align", "line-height", "flex", "flex-direction", "justify-content",
    "align-items", "grid", "gap", "opacity", "z-index", "overflow",
    "transform", "transition", "animation",
];

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_string_interner() {
        let mut interner = StringInterner::new();
        
        let id1 = interner.intern("hello");
        let id2 = interner.intern("world");
        let id3 = interner.intern("hello");
        
        assert_eq!(id1, id3); // Same string = same ID
        assert_ne!(id1, id2);
        assert_eq!(interner.get(&id1), Some("hello"));
    }
    
    #[test]
    fn test_tag_interner() {
        let mut interner = TagInterner::new();
        let div = interner.intern("div");
        
        assert_eq!(interner.get(&div), Some("div"));
    }
}
