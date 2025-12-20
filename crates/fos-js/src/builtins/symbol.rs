//! JavaScript Symbol
//!
//! Symbol primitive with well-known symbols.

use std::sync::atomic::{AtomicU32, Ordering};

static SYMBOL_COUNTER: AtomicU32 = AtomicU32::new(1);

/// JavaScript Symbol
#[derive(Debug, Clone)]
pub struct JsSymbol {
    pub id: u32,
    pub description: Option<String>,
    pub is_well_known: bool,
}

impl JsSymbol {
    /// Create a new unique symbol
    pub fn new(description: Option<&str>) -> Self {
        Self {
            id: SYMBOL_COUNTER.fetch_add(1, Ordering::SeqCst),
            description: description.map(|s| s.to_string()),
            is_well_known: false,
        }
    }
    
    /// Create symbol for key
    pub fn for_key(key: &str) -> Self {
        // Would use global registry
        Self::new(Some(key))
    }
    
    /// Get well-known symbol
    pub fn well_known(sym: WellKnownSymbol) -> Self {
        Self {
            id: sym as u32,
            description: Some(sym.name().to_string()),
            is_well_known: true,
        }
    }
    
    /// Get description string
    pub fn to_string(&self) -> String {
        match &self.description {
            Some(desc) => format!("Symbol({})", desc),
            None => "Symbol()".to_string(),
        }
    }
}

impl PartialEq for JsSymbol {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for JsSymbol {}

/// Well-known symbols
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum WellKnownSymbol {
    Iterator = 0x8000_0001,
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

impl WellKnownSymbol {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Iterator => "Symbol.iterator",
            Self::AsyncIterator => "Symbol.asyncIterator",
            Self::HasInstance => "Symbol.hasInstance",
            Self::IsConcatSpreadable => "Symbol.isConcatSpreadable",
            Self::Match => "Symbol.match",
            Self::MatchAll => "Symbol.matchAll",
            Self::Replace => "Symbol.replace",
            Self::Search => "Symbol.search",
            Self::Species => "Symbol.species",
            Self::Split => "Symbol.split",
            Self::ToPrimitive => "Symbol.toPrimitive",
            Self::ToStringTag => "Symbol.toStringTag",
            Self::Unscopables => "Symbol.unscopables",
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
        
        assert_ne!(s1.id, s2.id);
        assert_eq!(s1.description, s2.description);
    }
    
    #[test]
    fn test_well_known() {
        let iter = JsSymbol::well_known(WellKnownSymbol::Iterator);
        assert!(iter.is_well_known);
        assert_eq!(iter.description, Some("Symbol.iterator".to_string()));
    }
}
