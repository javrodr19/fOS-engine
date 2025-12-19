//! DOM Node types

/// Node type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeType {
    Element,
    Text,
    Comment,
    Document,
    DocumentType,
}

/// DOM Node
#[derive(Debug)]
pub struct Node {
    pub node_type: NodeType,
    pub parent: Option<super::NodeId>,
    pub children: Vec<super::NodeId>,
    pub data: NodeData,
}

/// Node-specific data
#[derive(Debug)]
pub enum NodeData {
    Element(Element),
    Text(Text),
    Comment(String),
    Document,
    DocumentType { name: String },
}

/// Element node data
#[derive(Debug)]
pub struct Element {
    pub tag_name: String,
    pub attributes: Vec<(String, String)>,
    pub id: Option<String>,
    pub classes: Vec<String>,
}

/// Text node data
#[derive(Debug)]
pub struct Text {
    pub content: String,
}
