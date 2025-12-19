//! fOS CSS Parser & Style System
//!
//! CSS parsing and cascade implementation.

mod parser;
mod cascade;
mod properties;

pub use parser::CssParser;
pub use cascade::StyleResolver;

/// Parse a CSS stylesheet
pub fn parse_stylesheet(css: &str) -> Result<Stylesheet, CssError> {
    CssParser::new().parse(css)
}

/// Parsed stylesheet
#[derive(Debug, Default)]
pub struct Stylesheet {
    pub rules: Vec<Rule>,
}

/// CSS rule
#[derive(Debug)]
pub struct Rule {
    pub selectors: Vec<Selector>,
    pub declarations: Vec<Declaration>,
}

/// CSS selector
#[derive(Debug)]
pub struct Selector {
    pub text: String,
    pub specificity: Specificity,
}

/// Selector specificity (a, b, c)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Specificity(pub u32, pub u32, pub u32);

/// CSS declaration (property: value)
#[derive(Debug)]
pub struct Declaration {
    pub property: String,
    pub value: String,
    pub important: bool,
}

/// CSS parsing error
#[derive(Debug, thiserror::Error)]
pub enum CssError {
    #[error("Parse error at line {line}: {message}")]
    ParseError { line: u32, message: String },
}
