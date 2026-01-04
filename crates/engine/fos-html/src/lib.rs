//! fOS HTML Parser
//!
//! High-performance HTML5 parser built on html5ever.
//! Parses HTML and converts to our memory-efficient DOM tree.

mod parser;
pub mod preload;
pub mod serializer;
pub mod incremental_parser;

pub use incremental_parser::{
    IncrementalParser, IncrementalTokenizer, Token, TokenizerState,
    ParsePhase, ParseYield, ChunkBoundary, ParsedNode, ParsedNodeContent,
};

pub use parser::HtmlParser;
pub use fos_dom::{Document, DomTree, Node, NodeId};
pub use serializer::{HtmlSerializer, get_inner_html, get_outer_html, FragmentContext, parse_fragment};

/// Parse an HTML string into a Document
pub fn parse(html: &str) -> Document {
    HtmlParser::new().parse(html)
}

/// Parse an HTML string with a base URL
pub fn parse_with_url(html: &str, url: &str) -> Document {
    HtmlParser::new().parse_with_url(html, url)
}

