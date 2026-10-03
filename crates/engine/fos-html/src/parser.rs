//! HTML5 Parser implementation
//!
//! html5ever's tree builder drives a [`TreeSink`] that writes straight into
//! our arena [`DomTree`]. There is no intermediate DOM: an earlier version
//! built html5ever's reference `RcDom` (a reference-counted node per element,
//! text and attribute) and then copied it, so a page briefly existed twice.

use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

use fos_dom::{Document, DomTree, ElementData, InternedString, Node, NodeData, NodeId, QualName};
use html5ever::interface::{ElemName, ElementFlags, NodeOrText, QuirksMode, TreeSink};
use html5ever::tendril::{StrTendril, TendrilSink};
use html5ever::{parse_document, Attribute, LocalName, Namespace};

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
        let document = parse_document(DomSink::new(url), Default::default()).one(html);
        tracing::debug!("Parsed {} nodes", document.tree().len());
        document
    }
}

/// Parse `html` as the contents of a `context` element (innerHTML): the
/// result's `<html>` element holds the fragment's nodes
pub(crate) fn parse_fragment(html: &str, context: &str) -> Document {
    let name = html5ever::QualName::new(None, html5ever::ns!(html), LocalName::from(context.to_ascii_lowercase()));
    html5ever::parse_fragment(DomSink::new("about:blank"), Default::default(), name, Vec::new(), true).one(html)
}

impl Default for HtmlParser {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether `text` is only ASCII whitespace (HTML's definition: space, tab,
/// LF, FF, CR). Non-breaking spaces are content.
fn is_inter_element_whitespace(text: &str) -> bool {
    text.bytes().all(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\x0c' | b'\r'))
}

fn tag_of(tree: &DomTree, id: NodeId) -> Option<&str> {
    tree.get(id).and_then(|n| n.as_element()).map(|e| tree.resolve(e.name.local))
}

/// Whether a whitespace-only text node can never render or matter: inside
/// structural elements (tables, lists, the head), or between block-level
/// elements. Whitespace next to inline content separates words, so it
/// stays (as does any inside `pre`-like elements).
fn whitespace_is_insignificant(tree: &DomTree, id: NodeId) -> bool {
    let Some(node) = tree.get(id) else { return false };
    match tag_of(tree, node.parent) {
        None if node.parent == tree.root() => return true,
        Some("html" | "head" | "table" | "thead" | "tbody" | "tfoot" | "tr" | "colgroup" | "ul" | "ol" | "dl" | "select" | "optgroup" | "datalist" | "frameset" | "menu") => return true,
        Some("pre" | "textarea" | "listing" | "plaintext" | "xmp" | "script" | "style") => return false,
        _ => {}
    }
    let blockish = |sibling: NodeId| -> bool {
        if !sibling.is_valid() {
            return true;
        }
        match tree.get(sibling).map(|n| &n.data) {
            Some(NodeData::Comment(_)) => true,
            Some(NodeData::Element(_)) => matches!(
                tag_of(tree, sibling).unwrap_or(""),
                "address" | "article" | "aside" | "blockquote" | "body" | "center" | "details" | "dialog" | "dd" | "div" | "dl" | "dt"
                    | "fieldset" | "figcaption" | "figure" | "footer" | "form" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "head"
                    | "header" | "hgroup" | "hr" | "li" | "main" | "menu" | "nav" | "ol" | "p" | "pre" | "section" | "summary"
                    | "table" | "ul" | "script" | "style" | "link" | "meta" | "title" | "template" | "noscript" | "base" | "br"
                    | "option" | "optgroup" | "caption" | "tr" | "td" | "th" | "thead" | "tbody" | "tfoot" | "colgroup" | "col"
                    | "iframe" | "video" | "audio" | "canvas" | "svg" | "search" | "legend"
            ),
            _ => false,
        }
    };
    blockish(node.prev_sibling) && blockish(node.next_sibling)
}

/// An element name owned by the caller, so no borrow of the sink's state
/// outlives the call (tag names are almost always static atoms, so cloning
/// one is a copy)
#[derive(Debug)]
struct OwnedElemName(html5ever::QualName);

impl ElemName for OwnedElemName {
    fn ns(&self) -> &Namespace {
        &self.0.ns
    }

    fn local_name(&self) -> &LocalName {
        &self.0.local
    }
}

/// Tree sink building a [`Document`]
struct DomSink {
    url: String,
    tree: RefCell<DomTree>,
    /// html5ever names of elements, indexed by node ID (only needed while
    /// parsing: the tree builder asks for them constantly)
    names: RefCell<Vec<Option<html5ever::QualName>>>,
    /// `<template>` element -> its (detached) contents fragment
    template_contents: RefCell<HashMap<NodeId, NodeId>>,
    /// MathML `annotation-xml` elements that are HTML integration points
    integration_points: RefCell<HashSet<NodeId>>,
    quirks_mode: Cell<QuirksMode>,
}

impl DomSink {
    fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
            tree: RefCell::new(DomTree::new()),
            names: RefCell::new(Vec::new()),
            template_contents: RefCell::new(HashMap::new()),
            integration_points: RefCell::new(HashSet::new()),
            quirks_mode: Cell::new(QuirksMode::NoQuirks),
        }
    }

    /// Append text to `parent`, merging it into a trailing text node
    fn append_text(tree: &mut DomTree, parent: NodeId, text: &str) {
        let last = tree.get(parent).map_or(NodeId::NONE, |p| p.last_child);
        if let Some(NodeData::Text(existing)) = tree.get_mut(last).map(|n| &mut n.data) {
            existing.content.push_str(text);
            return;
        }
        let id = tree.create_text(text);
        tree.append_child(parent, id);
    }

    /// Set an attribute on an element, keeping the cached id and class list
    /// in sync. With `only_if_missing`, existing attributes win.
    fn set_attribute(tree: &mut DomTree, element: NodeId, attr: Attribute, only_if_missing: bool) {
        let interner = tree.interner_mut();
        let name = QualName::new(interner.intern(&attr.name.ns), interner.intern(&attr.name.local));
        let is_id = &*attr.name.local == "id";
        let is_class = &*attr.name.local == "class";
        let interned_id = is_id.then(|| interner.intern(&attr.value));
        let classes: Vec<InternedString> = if is_class {
            // Sized exactly: most class lists hold one or two names, and a
            // growing Vec would reserve four
            let mut classes = Vec::with_capacity(attr.value.split_ascii_whitespace().count());
            classes.extend(attr.value.split_ascii_whitespace().map(|c| interner.intern(c)));
            classes
        } else {
            Vec::new()
        };

        let Some(NodeData::Element(elem)) = tree.get_mut(element).map(|n| &mut n.data) else { return };
        if only_if_missing && elem.attrs.iter().any(|a| a.name == name) {
            return;
        }
        if is_id {
            elem.id = interned_id;
        } else if is_class {
            elem.classes = classes;
        }
        elem.set_attr(name, String::from(attr.value));
    }
}

/// Name of fragment elements (as the DOM bindings name them)
const TEMPLATE_FRAGMENT: &str = "#document-fragment";

impl TreeSink for DomSink {
    type Handle = NodeId;
    type Output = Document;
    type ElemName<'a> = OwnedElemName where Self: 'a;

    fn finish(self) -> Document {
        let mut tree = self.tree.into_inner();
        // Template contents ride along as the template's child through
        // compaction, then move out of the tree
        for (&template, &contents) in self.template_contents.borrow().iter() {
            tree.append_child(template, contents);
        }
        // Drop what is not part of the document (nodes the tree builder
        // removed) and inter-element whitespace, and store the rest in
        // document order
        let mut droppable = Vec::new();
        let mut stack = vec![tree.root()];
        while let Some(id) = stack.pop() {
            let mut child = tree.get(id).map_or(NodeId::NONE, |n| n.first_child);
            while child.is_valid() {
                let Some(n) = tree.get(child) else { break };
                match &n.data {
                    NodeData::Text(t) if is_inter_element_whitespace(&t.content) && whitespace_is_insignificant(&tree, child) => droppable.push(child),
                    NodeData::Element(_) | NodeData::Document => stack.push(child),
                    _ => {}
                }
                child = n.next_sibling;
            }
        }
        for id in droppable {
            tree.remove(id);
        }
        tree.compact(|_| true);

        let mut contents = Vec::new();
        fos_dom::selector::walk_elements(&tree, tree.root(), &mut |id| {
            let is_template = tree.get(id).and_then(|n| n.as_element()).is_some_and(|e| tree.resolve(e.name.local) == "template");
            if is_template {
                let fragment = tree.children(id).find(|(_, n)| n.as_element().is_some_and(|e| tree.resolve(e.name.local) == TEMPLATE_FRAGMENT));
                if let Some((fragment, _)) = fragment {
                    contents.push((id, fragment));
                }
            }
            true
        });
        for &(_, fragment) in &contents {
            tree.remove(fragment);
        }

        let mut document = Document::empty(&self.url);
        document.tree = tree;
        document.set_quirks(self.quirks_mode.get() == QuirksMode::Quirks);
        for (template, fragment) in contents {
            document.set_template_content(template, fragment);
        }
        document.finalize();
        document
    }

    fn parse_error(&self, _msg: Cow<'static, str>) {}

    fn get_document(&self) -> NodeId {
        NodeId::ROOT
    }

    fn elem_name<'a>(&'a self, target: &'a NodeId) -> OwnedElemName {
        let names = self.names.borrow();
        let name = names.get(target.index()).and_then(Option::as_ref).expect("not an element");
        OwnedElemName(name.clone())
    }

    fn create_element(&self, name: html5ever::QualName, attrs: Vec<Attribute>, flags: ElementFlags) -> NodeId {
        let mut tree = self.tree.borrow_mut();
        let interner = tree.interner_mut();
        let qname = QualName::new(interner.intern(&name.ns), interner.intern(&name.local));
        let mut element = ElementData::new(qname);
        // Sized exactly (a growing Vec reserves four attributes for the first)
        element.attrs.reserve_exact(attrs.len());
        let id = tree.create_node(NodeData::Element(element));
        for attr in attrs {
            Self::set_attribute(&mut tree, id, attr, false);
        }

        if flags.template {
            // A fragment, as scripts see `template.content`
            let contents = tree.create_element(TEMPLATE_FRAGMENT);
            self.template_contents.borrow_mut().insert(id, contents);
        }
        if flags.mathml_annotation_xml_integration_point {
            self.integration_points.borrow_mut().insert(id);
        }

        let mut names = self.names.borrow_mut();
        if names.len() <= id.index() {
            names.resize(id.index() + 1, None);
        }
        names[id.index()] = Some(name);
        id
    }

    fn create_comment(&self, text: StrTendril) -> NodeId {
        self.tree.borrow_mut().create_comment(&text)
    }

    fn create_pi(&self, target: StrTendril, data: StrTendril) -> NodeId {
        let mut tree = self.tree.borrow_mut();
        let target = tree.interner_mut().intern(&target);
        tree.create_node(NodeData::ProcessingInstruction { target, data: String::from(data) })
    }

    fn append(&self, parent: &NodeId, child: NodeOrText<NodeId>) {
        let mut tree = self.tree.borrow_mut();
        match child {
            NodeOrText::AppendText(text) => Self::append_text(&mut tree, *parent, &text),
            NodeOrText::AppendNode(node) => tree.append_child(*parent, node),
        }
    }

    fn append_based_on_parent_node(&self, element: &NodeId, prev_element: &NodeId, child: NodeOrText<NodeId>) {
        let has_parent = self.tree.borrow().get(*element).is_some_and(|n| n.parent.is_valid());
        if has_parent {
            self.append_before_sibling(element, child);
        } else {
            self.append(prev_element, child);
        }
    }

    fn append_doctype_to_document(&self, name: StrTendril, public_id: StrTendril, system_id: StrTendril) {
        let mut tree = self.tree.borrow_mut();
        let name = tree.interner_mut().intern(&name);
        let id = tree.create_node(NodeData::Doctype {
            name,
            public_id: String::from(public_id),
            system_id: String::from(system_id),
        });
        tree.append_child(NodeId::ROOT, id);
    }

    fn get_template_contents(&self, target: &NodeId) -> NodeId {
        *self.template_contents.borrow().get(target).expect("not a template element")
    }

    fn same_node(&self, x: &NodeId, y: &NodeId) -> bool {
        x == y
    }

    fn set_quirks_mode(&self, mode: QuirksMode) {
        self.quirks_mode.set(mode);
    }

    fn append_before_sibling(&self, sibling: &NodeId, new_node: NodeOrText<NodeId>) {
        let mut tree = self.tree.borrow_mut();
        let Some((parent, prev)) = tree.get(*sibling).map(|n| (n.parent, n.prev_sibling)) else { return };
        if !parent.is_valid() {
            return;
        }
        let child = match new_node {
            NodeOrText::AppendText(text) => {
                // Merge into a text node just before the insertion point
                if let Some(NodeData::Text(existing)) = tree.get_mut(prev).map(|n| &mut n.data) {
                    existing.content.push_str(&text);
                    return;
                }
                tree.create_text(&text)
            }
            NodeOrText::AppendNode(node) => node,
        };
        tree.insert_before(parent, child, *sibling);
    }

    fn add_attrs_if_missing(&self, target: &NodeId, attrs: Vec<Attribute>) {
        let mut tree = self.tree.borrow_mut();
        for attr in attrs {
            Self::set_attribute(&mut tree, *target, attr, true);
        }
    }

    fn remove_from_parent(&self, target: &NodeId) {
        self.tree.borrow_mut().remove(*target);
    }

    fn reparent_children(&self, node: &NodeId, new_parent: &NodeId) {
        let mut tree = self.tree.borrow_mut();
        loop {
            let first = tree.get(*node).map_or(NodeId::NONE, |n| n.first_child);
            if !first.is_valid() {
                break;
            }
            tree.append_child(*new_parent, first);
        }
    }

    fn is_mathml_annotation_xml_integration_point(&self, handle: &NodeId) -> bool {
        self.integration_points.borrow().contains(handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serialize the tree as nested tags, text in quotes
    fn outline(doc: &Document, id: NodeId, out: &mut String) {
        let tree = doc.tree();
        for (child, node) in tree.children(id) {
            match &node.data {
                NodeData::Element(e) => {
                    out.push('<');
                    out.push_str(tree.resolve(e.name.local));
                    for attr in &e.attrs {
                        out.push_str(&format!(" {}={}", tree.resolve(attr.name.local), attr.value));
                    }
                    out.push('>');
                    outline(doc, child, out);
                    out.push_str("</>");
                }
                NodeData::Text(t) => out.push_str(&format!("'{}'", t.content)),
                NodeData::Comment(c) => out.push_str(&format!("<!--{}-->", c)),
                NodeData::Doctype { .. } => out.push_str("<!doctype>"),
                _ => {}
            }
        }
    }

    fn parse_outline(html: &str) -> String {
        let doc = HtmlParser::new().parse(html);
        let mut out = String::new();
        outline(&doc, doc.tree().root(), &mut out);
        out
    }

    #[test]
    fn test_parse_simple() {
        let html = "<html><head><title>Test</title></head><body><p>Hello</p></body></html>";
        let doc = HtmlParser::new().parse(html);

        // Document should have nodes
        assert!(doc.tree().len() > 1, "Expected more than 1 node, got {}", doc.tree().len());
        assert_eq!(doc.title(), "Test");
        assert!(doc.body().is_valid() && doc.head().is_valid());
    }

    #[test]
    fn test_parse_fragment() {
        let html = "<div><span>Text</span></div>";
        let doc = HtmlParser::new().parse(html);

        // Even fragments get wrapped in html/head/body by html5ever
        assert!(doc.tree().len() > 1);
        assert_eq!(
            parse_outline(html),
            "<html><head></><body><div><span>'Text'</></></></>"
        );
    }

    #[test]
    fn test_tree_construction() {
        // Doctype, comments, attributes (with the id/class caches), text
        // merged across character references
        let doc = HtmlParser::new().parse(
            "<!DOCTYPE html><!-- c --><p id=main class='a  b'>x &amp; y</p>",
        );
        assert_eq!(
            {
                let mut out = String::new();
                outline(&doc, doc.tree().root(), &mut out);
                out
            },
            "<!doctype><!-- c --><html><head></><body><p id=main class=a  b>'x & y'</></></>"
        );
        let p = doc.get_element_by_id("main").expect("id is indexed");
        let elem = doc.tree().get(p).unwrap().as_element().unwrap();
        let classes: Vec<&str> = elem.classes.iter().map(|&c| doc.tree().resolve(c)).collect();
        assert_eq!(classes, ["a", "b"]);
    }

    #[test]
    fn test_error_recovery() {
        // Misnested formatting elements: the adoption agency algorithm
        // reparents and inserts nodes
        assert_eq!(
            parse_outline("<b>1<p>2</b>3</p>"),
            "<html><head></><body><b>'1'</><p><b>'2'</>'3'</></></>"
        );
        // Foster parenting: text inside a table goes before it
        assert_eq!(
            parse_outline("<table>oops<tr><td>cell</td></tr></table>"),
            "<html><head></><body>'oops'<table><tbody><tr><td>'cell'</></></></></></>"
        );
        // Duplicate <body> merges attributes (existing ones win)
        assert_eq!(
            parse_outline("<body class=a><body class=b id=x>"),
            "<html><head></><body class=a id=x></></>"
        );
    }

    #[test]
    fn test_whitespace_and_templates() {
        // Inter-element whitespace is dropped; &nbsp; is content
        assert_eq!(
            parse_outline("<ul>\n  <li>a</li>\n  <li>&nbsp;</li>\n</ul>"),
            "<html><head></><body><ul><li>'a'</><li>'\u{a0}'</></></></>"
        );
        // Whitespace between inline elements separates words
        assert_eq!(
            parse_outline("<div>\n<p><a>x</a> <span>y</span></p>\n</div>"),
            "<html><head></><body><div><p><a>'x'</>' '<span>'y'</></></></></>"
        );
        // Template contents are not part of the document tree
        assert_eq!(
            parse_outline("<template><p>inert</p></template><p>live</p>"),
            "<html><head><template></></><body><p>'live'</></></>"
        );
    }

    #[test]
    fn test_nodes_are_in_document_order() {
        let doc = HtmlParser::new().parse("<div><p>a</p><p>b</p></div><span>c</span>");
        let tree = doc.tree();
        // A pre-order walk visits IDs in increasing order
        let mut order = Vec::new();
        let mut stack = vec![tree.root()];
        while let Some(id) = stack.pop() {
            order.push(id.0);
            let children: Vec<NodeId> = tree.children(id).map(|(c, _)| c).collect();
            stack.extend(children.into_iter().rev());
        }
        assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?}");
        assert_eq!(order.len(), tree.len());
    }
}
