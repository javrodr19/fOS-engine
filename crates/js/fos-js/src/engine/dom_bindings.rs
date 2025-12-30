//! DOM Bindings
//!
//! JavaScript DOM API bindings using StringInterner for efficient property names.

use super::integration::{StringInterner, InternedString, JsInterner};
use super::value::JsVal;
use super::object::JsObject;
use std::sync::Mutex;
use std::collections::HashMap;

/// DOM Element representation for JavaScript
#[derive(Debug, Clone)]
pub struct DomElement {
    pub tag_name: InternedString,
    pub id: Option<InternedString>,
    pub class_list: Vec<InternedString>,
    pub attributes: HashMap<InternedString, String>,
    pub children: Vec<u32>, // Element IDs
    pub parent: Option<u32>,
    pub text_content: String,
}

/// DOM Node types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NodeType {
    Element = 1,
    Text = 3,
    Comment = 8,
    Document = 9,
    DocumentFragment = 11,
}

/// DOM Document representation
#[derive(Debug)]
pub struct DomDocument {
    elements: Vec<DomElement>,
    interner: JsInterner,
    document_element: Option<u32>,
}

impl Default for DomDocument {
    fn default() -> Self { Self::new() }
}

impl DomDocument {
    pub fn new() -> Self {
        Self {
            elements: Vec::new(),
            interner: JsInterner::new(),
            document_element: None,
        }
    }
    
    /// Create a new element
    pub fn create_element(&mut self, tag_name: &str) -> u32 {
        let tag = self.interner.intern(tag_name);
        let id = self.elements.len() as u32;
        self.elements.push(DomElement {
            tag_name: tag,
            id: None,
            class_list: Vec::new(),
            attributes: HashMap::new(),
            children: Vec::new(),
            parent: None,
            text_content: String::new(),
        });
        id
    }
    
    /// Get element by ID
    pub fn get_element(&self, id: u32) -> Option<&DomElement> {
        self.elements.get(id as usize)
    }
    
    /// Get element by ID (mutable)
    pub fn get_element_mut(&mut self, id: u32) -> Option<&mut DomElement> {
        self.elements.get_mut(id as usize)
    }
    
    /// Set attribute
    pub fn set_attribute(&mut self, element_id: u32, name: &str, value: &str) {
        let attr_name = self.interner.intern(name);
        if let Some(el) = self.elements.get_mut(element_id as usize) {
            el.attributes.insert(attr_name, value.to_string());
        }
    }
    
    /// Get attribute
    pub fn get_attribute(&self, element_id: u32, name: &str) -> Option<String> {
        let attr_name = self.interner.intern(name);
        self.elements.get(element_id as usize)
            .and_then(|el| el.attributes.get(&attr_name).cloned())
    }
    
    /// Append child
    pub fn append_child(&mut self, parent_id: u32, child_id: u32) {
        if let Some(child) = self.elements.get_mut(child_id as usize) {
            child.parent = Some(parent_id);
        }
        if let Some(parent) = self.elements.get_mut(parent_id as usize) {
            parent.children.push(child_id);
        }
    }
    
    /// Get tag name
    pub fn get_tag_name(&self, element_id: u32) -> Option<String> {
        self.elements.get(element_id as usize)
            .map(|el| self.interner.get(&el.tag_name).unwrap_or_default())
    }
    
    /// Set text content
    pub fn set_text_content(&mut self, element_id: u32, text: &str) {
        if let Some(el) = self.elements.get_mut(element_id as usize) {
            el.text_content = text.to_string();
        }
    }
    
    /// Get text content
    pub fn get_text_content(&self, element_id: u32) -> Option<String> {
        self.elements.get(element_id as usize).map(|el| el.text_content.clone())
    }
    
    /// Add class
    pub fn add_class(&mut self, element_id: u32, class_name: &str) {
        let class = self.interner.intern(class_name);
        if let Some(el) = self.elements.get_mut(element_id as usize) {
            if !el.class_list.contains(&class) {
                el.class_list.push(class);
            }
        }
    }
    
    /// Remove class
    pub fn remove_class(&mut self, element_id: u32, class_name: &str) {
        let class = self.interner.intern(class_name);
        if let Some(el) = self.elements.get_mut(element_id as usize) {
            el.class_list.retain(|c| c != &class);
        }
    }
    
    /// Has class
    pub fn has_class(&self, element_id: u32, class_name: &str) -> bool {
        let class = self.interner.intern(class_name);
        self.elements.get(element_id as usize)
            .map(|el| el.class_list.contains(&class))
            .unwrap_or(false)
    }
    
    /// Query selector (simple tag name only)
    pub fn query_selector(&self, selector: &str) -> Option<u32> {
        let tag = self.interner.intern(selector);
        for (id, el) in self.elements.iter().enumerate() {
            if el.tag_name == tag {
                return Some(id as u32);
            }
        }
        None
    }
    
    /// Query selector all (simple tag name only)
    pub fn query_selector_all(&self, selector: &str) -> Vec<u32> {
        let tag = self.interner.intern(selector);
        self.elements.iter()
            .enumerate()
            .filter(|(_, el)| el.tag_name == tag)
            .map(|(id, _)| id as u32)
            .collect()
    }
}

/// Convert DOM element to JS object
pub fn element_to_js_object(doc: &DomDocument, element_id: u32) -> JsObject {
    let mut obj = JsObject::new();
    
    if let Some(el) = doc.get_element(element_id) {
        if let Some(tag) = doc.get_tag_name(element_id) {
            obj.set("tagName", JsVal::String(tag.into()));
        }
        obj.set("textContent", JsVal::String(el.text_content.clone().into()));
        obj.set("childElementCount", JsVal::Number(el.children.len() as f64));
    }
    
    obj
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_create_element() {
        let mut doc = DomDocument::new();
        let div = doc.create_element("div");
        assert_eq!(doc.get_tag_name(div).as_deref(), Some("div"));
    }
    
    #[test]
    fn test_set_attribute() {
        let mut doc = DomDocument::new();
        let div = doc.create_element("div");
        doc.set_attribute(div, "id", "main");
        assert_eq!(doc.get_attribute(div, "id").as_deref(), Some("main"));
    }
    
    #[test]
    fn test_class_list() {
        let mut doc = DomDocument::new();
        let div = doc.create_element("div");
        doc.add_class(div, "active");
        doc.add_class(div, "visible");
        assert!(doc.has_class(div, "active"));
        assert!(doc.has_class(div, "visible"));
        doc.remove_class(div, "active");
        assert!(!doc.has_class(div, "active"));
    }
    
    #[test]
    fn test_append_child() {
        let mut doc = DomDocument::new();
        let parent = doc.create_element("div");
        let child = doc.create_element("span");
        doc.append_child(parent, child);
        assert_eq!(doc.get_element(parent).unwrap().children, vec![child]);
        assert_eq!(doc.get_element(child).unwrap().parent, Some(parent));
    }
    
    #[test]
    fn test_query_selector() {
        let mut doc = DomDocument::new();
        let _div = doc.create_element("div");
        let span = doc.create_element("span");
        assert_eq!(doc.query_selector("span"), Some(span));
    }
}
