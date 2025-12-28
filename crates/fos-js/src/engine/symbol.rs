//! Symbol Implementation
//!
//! JavaScript Symbol primitive type.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

static SYMBOL_ID: AtomicU32 = AtomicU32::new(0);

/// JavaScript Symbol
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct JsSymbol {
    id: u32,
    description: Option<Box<str>>,
}

impl JsSymbol {
    /// Create a new unique symbol
    pub fn new(description: Option<&str>) -> Self {
        Self {
            id: SYMBOL_ID.fetch_add(1, Ordering::SeqCst),
            description: description.map(|s| s.into()),
        }
    }
    
    pub fn id(&self) -> u32 { self.id }
    pub fn description(&self) -> Option<&str> { self.description.as_deref() }
}

/// Well-known symbols
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WellKnownSymbol {
    Iterator,
    AsyncIterator,
    HasInstance,
    IsConcatSpreadable,
    Match,
    MatchAll,
    Replace,
    Search,
    Species,
    Split,
    ToPrimitive,
    ToStringTag,
    Unscopables,
}

/// Symbol registry for Symbol.for/keyFor
#[derive(Debug, Default)]
pub struct SymbolRegistry {
    by_key: HashMap<Box<str>, JsSymbol>,
    by_symbol: HashMap<u32, Box<str>>,
}

impl SymbolRegistry {
    pub fn new() -> Self { Self::default() }
    
    /// Symbol.for(key) - get or create symbol for key
    pub fn for_key(&mut self, key: &str) -> JsSymbol {
        if let Some(sym) = self.by_key.get(key) {
            return sym.clone();
        }
        let sym = JsSymbol::new(Some(key));
        self.by_key.insert(key.into(), sym.clone());
        self.by_symbol.insert(sym.id, key.into());
        sym
    }
    
    /// Symbol.keyFor(symbol) - get key for registered symbol
    pub fn key_for(&self, symbol: &JsSymbol) -> Option<&str> {
        self.by_symbol.get(&symbol.id).map(|s| s.as_ref())
    }
}

/// Well-known symbol instances
pub struct WellKnownSymbols {
    pub iterator: JsSymbol,
    pub async_iterator: JsSymbol,
    pub has_instance: JsSymbol,
    pub is_concat_spreadable: JsSymbol,
    pub match_: JsSymbol,
    pub match_all: JsSymbol,
    pub replace: JsSymbol,
    pub search: JsSymbol,
    pub species: JsSymbol,
    pub split: JsSymbol,
    pub to_primitive: JsSymbol,
    pub to_string_tag: JsSymbol,
    pub unscopables: JsSymbol,
}

impl Default for WellKnownSymbols {
    fn default() -> Self { Self::new() }
}

impl WellKnownSymbols {
    pub fn new() -> Self {
        Self {
            iterator: JsSymbol::new(Some("Symbol.iterator")),
            async_iterator: JsSymbol::new(Some("Symbol.asyncIterator")),
            has_instance: JsSymbol::new(Some("Symbol.hasInstance")),
            is_concat_spreadable: JsSymbol::new(Some("Symbol.isConcatSpreadable")),
            match_: JsSymbol::new(Some("Symbol.match")),
            match_all: JsSymbol::new(Some("Symbol.matchAll")),
            replace: JsSymbol::new(Some("Symbol.replace")),
            search: JsSymbol::new(Some("Symbol.search")),
            species: JsSymbol::new(Some("Symbol.species")),
            split: JsSymbol::new(Some("Symbol.split")),
            to_primitive: JsSymbol::new(Some("Symbol.toPrimitive")),
            to_string_tag: JsSymbol::new(Some("Symbol.toStringTag")),
            unscopables: JsSymbol::new(Some("Symbol.unscopables")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_symbol_unique() {
        let s1 = JsSymbol::new(Some("test"));
        let s2 = JsSymbol::new(Some("test"));
        assert_ne!(s1.id(), s2.id());
    }
    
    #[test]
    fn test_symbol_registry() {
        let mut registry = SymbolRegistry::new();
        let s1 = registry.for_key("shared");
        let s2 = registry.for_key("shared");
        assert_eq!(s1.id(), s2.id());
    }
    
    #[test]
    fn test_key_for() {
        let mut registry = SymbolRegistry::new();
        let sym = registry.for_key("myKey");
        assert_eq!(registry.key_for(&sym), Some("myKey"));
    }
}
