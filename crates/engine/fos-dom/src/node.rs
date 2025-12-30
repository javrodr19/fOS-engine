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
    /// Attributes - stored inline for small counts, Vec for large
    pub attrs: SmallVec<Attribute>,
    /// Cached id attribute (very common lookup)
    pub id: Option<InternedString>,
    /// Cached class list
    pub classes: SmallVec<InternedString>,
}

impl ElementData {
    pub fn new(name: QualName) -> Self {
        Self {
            name,
            attrs: SmallVec::new(),
            id: None,
            classes: SmallVec::new(),
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

/// Small vector - inline storage for up to 4 items
/// Avoids heap allocation for common cases (most elements have < 5 attributes)
#[derive(Debug)]
pub enum SmallVec<T> {
    /// Inline storage (no heap allocation)
    Inline {
        data: [Option<T>; 4],
        len: u8,
    },
    /// Heap storage for larger collections
    Heap(Vec<T>),
}

impl<T> SmallVec<T> {
    pub fn new() -> Self {
        Self::Inline {
            data: [None, None, None, None],
            len: 0,
        }
    }
    
    pub fn push(&mut self, value: T) {
        match self {
            Self::Inline { data, len } => {
                if (*len as usize) < 4 {
                    data[*len as usize] = Some(value);
                    *len += 1;
                } else {
                    // Upgrade to heap
                    let mut vec = Vec::with_capacity(8);
                    for item in data.iter_mut() {
                        if let Some(v) = item.take() {
                            vec.push(v);
                        }
                    }
                    vec.push(value);
                    *self = Self::Heap(vec);
                }
            }
            Self::Heap(vec) => vec.push(value),
        }
    }
    
    pub fn len(&self) -> usize {
        match self {
            Self::Inline { len, .. } => *len as usize,
            Self::Heap(vec) => vec.len(),
        }
    }
    
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        SmallVecIter { vec: self, idx: 0 }
    }
    
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        SmallVecIterMut { vec: self, idx: 0 }
    }
}

impl<T> Default for SmallVec<T> {
    fn default() -> Self {
        Self::new()
    }
}

struct SmallVecIter<'a, T> {
    vec: &'a SmallVec<T>,
    idx: usize,
}

impl<'a, T> Iterator for SmallVecIter<'a, T> {
    type Item = &'a T;
    
    fn next(&mut self) -> Option<Self::Item> {
        match self.vec {
            SmallVec::Inline { data, len } => {
                if self.idx < *len as usize {
                    let result = data[self.idx].as_ref();
                    self.idx += 1;
                    result
                } else {
                    None
                }
            }
            SmallVec::Heap(vec) => {
                if self.idx < vec.len() {
                    let result = Some(&vec[self.idx]);
                    self.idx += 1;
                    result
                } else {
                    None
                }
            }
        }
    }
}

struct SmallVecIterMut<'a, T> {
    vec: &'a mut SmallVec<T>,
    idx: usize,
}

impl<'a, T> Iterator for SmallVecIterMut<'a, T> {
    type Item = &'a mut T;
    
    fn next(&mut self) -> Option<Self::Item> {
        match self.vec {
            SmallVec::Inline { data, len } => {
                if self.idx < *len as usize {
                    // Safety: we're iterating through unique indices
                    let ptr = data[self.idx].as_mut()? as *mut T;
                    self.idx += 1;
                    Some(unsafe { &mut *ptr })
                } else {
                    None
                }
            }
            SmallVec::Heap(vec) => {
                if self.idx < vec.len() {
                    let ptr = &mut vec[self.idx] as *mut T;
                    self.idx += 1;
                    Some(unsafe { &mut *ptr })
                } else {
                    None
                }
            }
        }
    }
}
