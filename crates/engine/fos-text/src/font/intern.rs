//! String Interning for Font Names
//!
//! Local copy of StringInterner to avoid cyclic dependency with fos-engine.

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

/// String interner for font family names
#[derive(Debug, Default)]
pub struct StringInterner {
    strings: Vec<Arc<str>>,
    lookup: HashMap<Arc<str>, u32>,
}

impl StringInterner {
    pub fn new() -> Self { 
        Self::default() 
    }
    
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
}
