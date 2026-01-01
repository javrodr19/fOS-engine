//! HTML Serialization (innerHTML/outerHTML)
//!
//! Serializes DOM nodes to HTML strings and parses HTML fragments.
//!
//! Key features:
//! - innerHTML/outerHTML serialization
//! - Proper HTML escaping
//! - Void element handling
//! - Fragment parsing for innerHTML assignment

use fos_dom::{NodeId, NodeData, DomTree};

/// HTML serializer
pub struct HtmlSerializer {
    /// Whether to format output with indentation
    pub pretty_print: bool,
    /// Indentation string
    pub indent: String,
}

/// Void elements (self-closing, no end tag)
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input",
    "link", "meta", "param", "source", "track", "wbr",
];

/// Raw text elements (no escaping for content)
const RAW_TEXT_ELEMENTS: &[&str] = &["script", "style"];

impl Default for HtmlSerializer {
    fn default() -> Self {
        Self {
            pretty_print: false,
            indent: "  ".to_string(),
        }
    }
}

impl HtmlSerializer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn pretty() -> Self {
        Self {
            pretty_print: true,
            indent: "  ".to_string(),
        }
    }

    /// Serialize innerHTML of a node (children only)
    pub fn serialize_inner(&self, tree: &DomTree, node_id: NodeId) -> String {
        let mut output = String::new();
        self.serialize_children(tree, node_id, &mut output, 0);
        output
    }

    /// Serialize outerHTML of a node (including the node itself)
    pub fn serialize_outer(&self, tree: &DomTree, node_id: NodeId) -> String {
        let mut output = String::new();
        self.serialize_node(tree, node_id, &mut output, 0);
        output
    }

    /// Serialize a node and its descendants
    fn serialize_node(&self, tree: &DomTree, node_id: NodeId, output: &mut String, depth: usize) {
        let Some(node) = tree.get(node_id) else {
            return;
        };

        match &node.data {
            NodeData::Document => {
                self.serialize_children(tree, node_id, output, depth);
            }
            NodeData::Element(elem) => {
                let tag = tree.resolve(elem.name.local);
                let is_void = VOID_ELEMENTS.contains(&tag);
                let is_raw = RAW_TEXT_ELEMENTS.contains(&tag);

                // Indentation
                if self.pretty_print && depth > 0 {
                    output.push('\n');
                    for _ in 0..depth {
                        output.push_str(&self.indent);
                    }
                }

                // Start tag
                output.push('<');
                output.push_str(tag);

                // Attributes
                for attr in elem.attrs.iter() {
                    output.push(' ');
                    let name = tree.resolve(attr.name.local);
                    output.push_str(name);
                    if !attr.value.is_empty() {
                        output.push_str("=\"");
                        escape_attribute(&attr.value, output);
                        output.push('"');
                    }
                }

                if is_void {
                    output.push_str(" />");
                } else {
                    output.push('>');

                    // Children
                    if is_raw {
                        // Raw content, no escaping
                        self.serialize_children_raw(tree, node_id, output);
                    } else {
                        self.serialize_children(tree, node_id, output, depth + 1);
                    }

                    // End tag
                    if self.pretty_print && node.first_child.is_valid() {
                        output.push('\n');
                        for _ in 0..depth {
                            output.push_str(&self.indent);
                        }
                    }
                    output.push_str("</");
                    output.push_str(tag);
                    output.push('>');
                }
            }
            NodeData::Text(text) => {
                escape_text(&text.content, output);
            }
            NodeData::Comment(text) => {
                output.push_str("<!--");
                output.push_str(text);
                output.push_str("-->");
            }
            NodeData::Doctype { name, .. } => {
                output.push_str("<!DOCTYPE ");
                output.push_str(tree.resolve(*name));
                output.push('>');
            }
            NodeData::ProcessingInstruction { target, data } => {
                output.push_str("<?");
                output.push_str(tree.resolve(*target));
                if !data.is_empty() {
                    output.push(' ');
                    output.push_str(data);
                }
                output.push_str("?>");
            }
        }
    }

    fn serialize_children(&self, tree: &DomTree, parent_id: NodeId, output: &mut String, depth: usize) {
        for (child_id, _) in tree.children(parent_id) {
            self.serialize_node(tree, child_id, output, depth);
        }
    }

    fn serialize_children_raw(&self, tree: &DomTree, parent_id: NodeId, output: &mut String) {
        for (_child_id, child) in tree.children(parent_id) {
            if let NodeData::Text(text) = &child.data {
                output.push_str(&text.content);
            }
        }
    }
}

/// Escape text content for HTML
fn escape_text(text: &str, output: &mut String) {
    for c in text.chars() {
        match c {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            _ => output.push(c),
        }
    }
}

/// Escape attribute value
fn escape_attribute(text: &str, output: &mut String) {
    for c in text.chars() {
        match c {
            '&' => output.push_str("&amp;"),
            '"' => output.push_str("&quot;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            _ => output.push(c),
        }
    }
}

/// Fragment context for innerHTML parsing
#[derive(Debug, Clone)]
pub struct FragmentContext {
    pub context_element: String,
    pub namespace: String,
}

impl Default for FragmentContext {
    fn default() -> Self {
        Self {
            context_element: "body".to_string(),
            namespace: "http://www.w3.org/1999/xhtml".to_string(),
        }
    }
}

/// Parse an HTML fragment (for innerHTML assignment)
/// Returns a list of node IDs that were created
pub fn parse_fragment(_html: &str, _context: FragmentContext) -> Vec<NodeId> {
    // This would integrate with the HTML parser
    // For now, return empty - actual implementation would use html5ever
    Vec::new()
}

/// Utility: Get innerHTML of an element
pub fn get_inner_html(tree: &DomTree, node_id: NodeId) -> String {
    HtmlSerializer::new().serialize_inner(tree, node_id)
}

/// Utility: Get outerHTML of an element
pub fn get_outer_html(tree: &DomTree, node_id: NodeId) -> String {
    HtmlSerializer::new().serialize_outer(tree, node_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_text() {
        let mut output = String::new();
        escape_text("Hello <world> & \"friends\"", &mut output);
        assert_eq!(output, "Hello &lt;world&gt; &amp; \"friends\"");
    }

    #[test]
    fn test_escape_attribute() {
        let mut output = String::new();
        escape_attribute("Hello <world> & \"friends\"", &mut output);
        assert_eq!(output, "Hello &lt;world&gt; &amp; &quot;friends&quot;");
    }

    #[test]
    fn test_void_elements() {
        assert!(VOID_ELEMENTS.contains(&"br"));
        assert!(VOID_ELEMENTS.contains(&"img"));
        assert!(VOID_ELEMENTS.contains(&"input"));
        assert!(!VOID_ELEMENTS.contains(&"div"));
    }

    #[test]
    fn test_serializer_creation() {
        let serializer = HtmlSerializer::new();
        assert!(!serializer.pretty_print);

        let pretty = HtmlSerializer::pretty();
        assert!(pretty.pretty_print);
    }
}
