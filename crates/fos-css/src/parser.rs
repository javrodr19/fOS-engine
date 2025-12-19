//! CSS parser implementation

use crate::{Stylesheet, CssError};

pub struct CssParser;

impl CssParser {
    pub fn new() -> Self {
        Self
    }
    
    pub fn parse(&self, _css: &str) -> Result<Stylesheet, CssError> {
        // TODO: Implement using lightningcss
        tracing::info!("Parsing CSS stylesheet");
        Ok(Stylesheet::default())
    }
}

impl Default for CssParser {
    fn default() -> Self {
        Self::new()
    }
}
