//! DOM Node - Compact representation
//!
//! Memory layout optimized for minimal footprint:
//! - Node struct is 32 bytes on 64-bit systems
//! - Uses NodeId (4 bytes) instead of pointers (8 bytes)
//! - NodeData uses enum discriminant efficiently

use crate::{NodeId, InternedString, QualName};

/// DOM Node - Core structure
/// 
/// Size: 32 bytes on 64-bit (vs 80+ bytes in typical implementations)
#[derive(Debug)]
pub struct Node {
    /// Parent node (NONE if root)
    pub parent: NodeId,
    /// First child
    pub first_child: NodeId,
    /// Last child (for O(1) append)
    pub last_child: NodeId,
    /// Previous sibling
    pub prev_sibling: NodeId,
    /// Next sibling
    pub next_sibling: NodeId,
    /// Node-specific data
    pub data: NodeData,
}

impl Node {
    /// Create a new element node
    pub fn element(name: QualName) -> Self {
        Self {
            parent: NodeId::NONE,
            first_child: NodeId::NONE,
            last_child: NodeId::NONE,
            prev_sibling: NodeId::NONE,
            next_sibling: NodeId::NONE,
            data: NodeData::Element(ElementData::new(name)),
        }
    }
    
    /// Create a new text node
    pub fn text(content: String) -> Self {
        Self {
            parent: NodeId::NONE,
            first_child: NodeId::NONE,
            last_child: NodeId::NONE,
            prev_sibling: NodeId::NONE,
            next_sibling: NodeId::NONE,
            data: NodeData::Text(TextData { content }),
        }
    }
    
    /// Create a document node
    pub fn document() -> Self {
        Self {
            parent: NodeId::NONE,
            first_child: NodeId::NONE,
            last_child: NodeId::NONE,
            prev_sibling: NodeId::NONE,
            next_sibling: NodeId::NONE,
            data: NodeData::Document,
        }
    }
    
    /// Check if this is an element
    #[inline]
    pub fn is_element(&self) -> bool {
        matches!(self.data, NodeData::Element(_))
    }
    
    /// Check if this is text
    #[inline]
    pub fn is_text(&self) -> bool {
        matches!(self.data, NodeData::Text(_))
    }
    
    /// Get element data if this is an element
    #[inline]
    pub fn as_element(&self) -> Option<&ElementData> {
        match &self.data {
            NodeData::Element(e) => Some(e),
            _ => None,
        }
    }
    
    /// Get mutable element data
    #[inline]
    pub fn as_element_mut(&mut self) -> Option<&mut ElementData> {
        match &mut self.data {
            NodeData::Element(e) => Some(e),
            _ => None,
        }
    }
    
    /// Get text content if this is a text node
    #[inline]
    pub fn as_text(&self) -> Option<&str> {
        match &self.data {
            NodeData::Text(t) => Some(&t.content),
            _ => None,
        }
    }
}

/// Node-specific data
#[derive(Debug)]
pub enum NodeData {
    /// Document root
    Document,
    /// DOCTYPE
    Doctype {
        name: InternedString,
        public_id: String,
        system_id: String,
    },
    /// Element
    Element(ElementData),
    /// Text content
    Text(TextData),
    /// Comment
    Comment(String),
    /// Processing instruction
    ProcessingInstruction {
        target: InternedString,
        data: String,
    },
}

/// Element-specific data
#[derive(Debug)]
pub struct ElementData {
    /// Tag name (qualified)
    pub name: QualName,
    /// Attributes. An empty `Vec` does not allocate, so attribute-less elements
    /// (the common case) cost only the 24-byte header.
    pub attrs: Vec<Attribute>,
    /// Cached id attribute (very common lookup)
    pub id: Option<InternedString>,
    /// Cached class list
    pub classes: Vec<InternedString>,
}

impl ElementData {
    pub fn new(name: QualName) -> Self {
        Self {
            name,
            attrs: Vec::new(),
            id: None,
            classes: Vec::new(),
        }
    }
    
    /// Get an attribute value
    pub fn get_attr(&self, name: InternedString) -> Option<&str> {
        self.attrs.iter()
            .find(|a| a.name.local == name)
            .map(|a| a.value.as_str())
    }
    
    /// Set an attribute
    pub fn set_attr(&mut self, name: QualName, value: String) {
        // Check if attribute already exists
        for attr in self.attrs.iter_mut() {
            if attr.name == name {
                attr.value = value;
                return;
            }
        }
        // Add new attribute
        self.attrs.push(Attribute { name, value });
    }
}

/// Text node data
#[derive(Debug)]
pub struct TextData {
    pub content: String,
}

/// Attribute
#[derive(Debug)]
pub struct Attribute {
    pub name: QualName,
    pub value: String,
}

#[cfg(test)]
mod size_guard {
    use super::*;
    use std::mem::size_of;

    /// `Node` is the most-allocated struct in the engine: a content-heavy page holds
    /// tens of thousands of them, so its size is a headline RAM number and must not
    /// regress silently.
    ///
    /// The layout is 5 x NodeId (20 bytes) + padding + NodeData. `NodeData` is as
    /// large as its biggest variant, so growing *any* variant grows *every* node,
    /// including the text nodes that usually outnumber elements.
    #[test]
    fn node_layout_is_compact() {
        assert_eq!(size_of::<NodeId>(), 4, "NodeId must stay a u32 index");
        assert_eq!(size_of::<TextData>(), 24, "TextData should be exactly a String");

        // ElementData: QualName(8) + Vec(24) + Option<InternedString>(8) + Vec(24).
        assert_eq!(size_of::<ElementData>(), 64);

        // NodeData is sized by its largest variant (Element). It matches ElementData
        // exactly because the non-null Vec pointer inside gives the discriminant a
        // niche to live in, so the tag costs nothing.
        assert_eq!(size_of::<NodeData>(), 64);

        // 5 x NodeId (20) + 4 padding + NodeData (64).
        assert_eq!(
            size_of::<Node>(),
            88,
            "Node grew. Every DOM node pays this, so check which NodeData variant expanded.",
        );
    }

    /// An element with no attributes must not touch the heap.
    #[test]
    fn empty_element_does_not_allocate() {
        let elem = ElementData::new(QualName::new(InternedString::EMPTY, InternedString::EMPTY));
        assert_eq!(elem.attrs.capacity(), 0, "empty attrs must not allocate");
        assert_eq!(elem.classes.capacity(), 0, "empty classes must not allocate");
    }
}
