//! fOS HTML Parser
//!
//! High-performance HTML5 parser built on html5ever.
//! Designed for minimal memory usage.

mod parser;
mod tokenizer;

pub use parser::HtmlParser;

/// Parse an HTML string into a DOM-compatible structure
pub fn parse(html: &str) -> ParseResult {
    HtmlParser::new().parse(html)
}

/// Result of parsing HTML
#[derive(Debug)]
pub struct ParseResult {
    pub root: NodeId,
    pub errors: Vec<ParseError>,
}

/// Unique identifier for a node
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub(crate) u32);

/// Parse error
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("Unexpected token at line {line}: {message}")]
    UnexpectedToken { line: u32, message: String },
    
    #[error("Unclosed tag: {tag}")]
    UnclosedTag { tag: String },
}
