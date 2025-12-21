//! HTML5 Parser implementation
//!
//! Uses html5ever's build-in RcDom and converts to our DOM format.
//! This is simpler and more reliable than implementing TreeSink directly.

use fos_dom::{Document, DomTree, Node, NodeId, NodeData, ElementData, QualName};
use html5ever::parse_document;
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{RcDom, Handle, NodeData as RcNodeData};

/// HTML5 parser
pub struct HtmlParser;

impl HtmlParser {
    /// Create a new HTML parser
    pub fn new() -> Self {
        Self
    }
    
    /// Parse HTML string into a Document
    pub fn parse(&self, html: &str) -> Document {
        self.parse_with_url(html, "about:blank")
    }
    
    /// Parse HTML with a base URL
    pub fn parse_with_url(&self, html: &str, url: &str) -> Document {
        tracing::debug!("Parsing HTML document: {}", url);
        
        // Parse using RcDom
        let dom = parse_document(RcDom::default(), Default::default())
            .from_utf8()
            .read_from(&mut html.as_bytes())
            .expect("HTML parsing should not fail");
        
        // Convert RcDom to our DOM
        let mut document = Document::empty(url);
        self.convert_node(&dom.document, document.tree_mut(), NodeId::ROOT);
        
        // Find html, head, body elements
        document.finalize();
        
        tracing::debug!("Parsed {} nodes", document.tree().len());
        document
    }
    
    /// Convert an RcDom node to our DOM format
    fn convert_node(&self, handle: &Handle, tree: &mut DomTree, parent: NodeId) {
        // Handle is Rc<Node>, access data directly
        let node_data = &handle.data;
        
        match node_data {
            RcNodeData::Document => {
                // Document node - just process children
                for child in handle.children.borrow().iter() {
                    self.convert_node(child, tree, parent);
                }
            }
            RcNodeData::Doctype { name, public_id, system_id } => {
                let name_str = name.to_string();
                let name_interned = tree.interner_mut().intern(&name_str);
                let id = NodeId(tree.len() as u32);
                
                tree.nodes.push(Node {
                    parent: NodeId::NONE,
                    first_child: NodeId::NONE,
                    last_child: NodeId::NONE,
                    prev_sibling: NodeId::NONE,
                    next_sibling: NodeId::NONE,
                    data: NodeData::Doctype {
                        name: name_interned,
                        public_id: public_id.to_string(),
                        system_id: system_id.to_string(),
                    },
                });
                tree.append_child(parent, id);
            }
            RcNodeData::Text { contents } => {
                let text = contents.borrow().to_string();
                if !text.trim().is_empty() {
                    let id = tree.create_text(&text);
                    tree.append_child(parent, id);
                }
            }
            RcNodeData::Comment { contents } => {
                let id = tree.create_comment(&contents.to_string());
                tree.append_child(parent, id);
            }
            RcNodeData::Element { name, attrs, .. } => {
                // Create element
                let ns = tree.interner_mut().intern(&name.ns.to_string());
                let local = tree.interner_mut().intern(&name.local);
                let qname = QualName::new(ns, local);
                let id = NodeId(tree.len() as u32);
                
                let mut elem = ElementData::new(qname);
                
                // Add attributes
                for attr in attrs.borrow().iter() {
                    let attr_ns = tree.interner_mut().intern(&attr.name.ns.to_string());
                    let attr_local = tree.interner_mut().intern(&attr.name.local);
                    let attr_name = QualName::new(attr_ns, attr_local);
                    let value = attr.value.to_string();
                    
                    // Cache id and class
                    if attr.name.local.as_ref() == "id" {
                        elem.id = Some(tree.interner_mut().intern(&value));
                    } else if attr.name.local.as_ref() == "class" {
                        for class in value.split_whitespace() {
                            elem.classes.push(tree.interner_mut().intern(class));
                        }
                    }
                    
                    elem.set_attr(attr_name, value);
                }
                
                tree.nodes.push(Node {
                    parent: NodeId::NONE,
                    first_child: NodeId::NONE,
                    last_child: NodeId::NONE,
                    prev_sibling: NodeId::NONE,
                    next_sibling: NodeId::NONE,
                    data: NodeData::Element(elem),
                });
                tree.append_child(parent, id);
                
                // Process children
                for child in handle.children.borrow().iter() {
                    self.convert_node(child, tree, id);
                }
            }
            RcNodeData::ProcessingInstruction { .. } => {
                // Ignore processing instructions for now
            }
        }
    }
}

impl Default for HtmlParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_simple() {
        let html = "<html><head><title>Test</title></head><body><p>Hello</p></body></html>";
        let doc = HtmlParser::new().parse(html);
        
        // Document should have nodes
        assert!(doc.tree().len() > 1, "Expected more than 1 node, got {}", doc.tree().len());
    }
    
    #[test]
    fn test_parse_fragment() {
        let html = "<div><span>Text</span></div>";
        let doc = HtmlParser::new().parse(html);
        
        // Even fragments get wrapped in html/head/body by html5ever
        assert!(doc.tree().len() > 1);
    }
}
