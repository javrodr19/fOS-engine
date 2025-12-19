//! Document API

use crate::DomTree;

/// HTML Document
#[derive(Debug)]
pub struct Document {
    pub tree: DomTree,
    pub title: String,
    pub url: String,
}

impl Document {
    /// Create a new empty document
    pub fn new(url: &str) -> Self {
        Self {
            tree: DomTree::new(),
            title: String::new(),
            url: url.to_string(),
        }
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::new("about:blank")
    }
}
