//! HTML Template Element
//!
//! The `<template>` element holds HTML content that is not rendered when the page loads,
//! but can be cloned and inserted into the document via JavaScript.
//!
//! Key features:
//! - Content is stored in an inert DocumentFragment
//! - Template content is not part of the active document
//! - Cloning a template clones its content

use crate::{NodeId, DocumentFragment};

/// HTMLTemplateElement - represents a `<template>` element
#[derive(Debug, Clone)]
pub struct HTMLTemplateElement {
    /// The element's node ID in the DOM tree
    pub node_id: NodeId,
    /// The template's content - an inert DocumentFragment
    content: TemplateContent,
}

/// Template content - DocumentFragment that doesn't render
#[derive(Debug, Clone, Default)]
pub struct TemplateContent {
    /// Child nodes within the template
    pub children: Vec<NodeId>,
    /// Whether this content has been parsed (for declarative shadow DOM)
    pub parsed: bool,
}

impl TemplateContent {
    /// Create a new empty template content
    pub fn new() -> Self {
        Self::default()
    }

    /// Create from existing children
    pub fn from_children(children: Vec<NodeId>) -> Self {
        Self {
            children,
            parsed: true,
        }
    }

    /// Append a child node
    pub fn append_child(&mut self, child: NodeId) {
        self.children.push(child);
    }

    /// Prepend a child node
    pub fn prepend_child(&mut self, child: NodeId) {
        self.children.insert(0, child);
    }

    /// Remove a child at index
    pub fn remove_child(&mut self, index: usize) -> Option<NodeId> {
        if index < self.children.len() {
            Some(self.children.remove(index))
        } else {
            None
        }
    }

    /// Get children
    pub fn children(&self) -> &[NodeId] {
        &self.children
    }

    /// Check if content is empty
    pub fn is_empty(&self) -> bool {
        self.children.is_empty()
    }

    /// Number of child nodes
    pub fn len(&self) -> usize {
        self.children.len()
    }

    /// Convert to DocumentFragment for cloning
    pub fn to_fragment(&self) -> DocumentFragment {
        DocumentFragment {
            children: self.children.clone(),
        }
    }

    /// Clear all children
    pub fn clear(&mut self) {
        self.children.clear();
    }
}

impl HTMLTemplateElement {
    /// Create a new template element
    pub fn new(node_id: NodeId) -> Self {
        Self {
            node_id,
            content: TemplateContent::new(),
        }
    }

    /// Create with pre-existing content
    pub fn with_content(node_id: NodeId, content: TemplateContent) -> Self {
        Self { node_id, content }
    }

    /// Get the template's content (inert DocumentFragment)
    pub fn content(&self) -> &TemplateContent {
        &self.content
    }

    /// Get mutable access to content
    pub fn content_mut(&mut self) -> &mut TemplateContent {
        &mut self.content
    }

    /// Clone the template's content as a DocumentFragment
    /// This is the primary way to use a template - get a clone of its content
    pub fn clone_content(&self) -> DocumentFragment {
        self.content.to_fragment()
    }

    /// Set the template content
    pub fn set_content(&mut self, content: TemplateContent) {
        self.content = content;
    }

    /// Append a node to the template content
    pub fn append_to_content(&mut self, child: NodeId) {
        self.content.append_child(child);
    }

    /// Check if template has content
    pub fn has_content(&self) -> bool {
        !self.content.is_empty()
    }
}

/// Template registry - tracks all templates in a document
#[derive(Debug, Default)]
pub struct TemplateRegistry {
    /// Map from node ID to template
    templates: Vec<(NodeId, TemplateContent)>,
}

impl TemplateRegistry {
    /// Create a new registry
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a template element
    pub fn register(&mut self, node_id: NodeId, content: TemplateContent) {
        // Check if already registered
        if !self.templates.iter().any(|(id, _)| *id == node_id) {
            self.templates.push((node_id, content));
        }
    }

    /// Get template content by node ID
    pub fn get(&self, node_id: NodeId) -> Option<&TemplateContent> {
        self.templates.iter()
            .find(|(id, _)| *id == node_id)
            .map(|(_, content)| content)
    }

    /// Get mutable template content by node ID
    pub fn get_mut(&mut self, node_id: NodeId) -> Option<&mut TemplateContent> {
        self.templates.iter_mut()
            .find(|(id, _)| *id == node_id)
            .map(|(_, content)| content)
    }

    /// Remove a template
    pub fn remove(&mut self, node_id: NodeId) -> Option<TemplateContent> {
        let pos = self.templates.iter().position(|(id, _)| *id == node_id);
        pos.map(|i| self.templates.remove(i).1)
    }

    /// Number of registered templates
    pub fn len(&self) -> usize {
        self.templates.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.templates.is_empty()
    }

    /// Clear all templates
    pub fn clear(&mut self) {
        self.templates.clear();
    }
}

/// Declarative Shadow DOM support
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShadowRootMode {
    Open,
    Closed,
}

/// Template with shadowrootmode attribute (declarative shadow DOM)
#[derive(Debug, Clone)]
pub struct DeclarativeTemplate {
    /// Base template
    pub template: HTMLTemplateElement,
    /// Shadow root mode from shadowrootmode attribute
    pub shadow_root_mode: Option<ShadowRootMode>,
    /// Whether shadowrootdelegatesfocus is set
    pub delegates_focus: bool,
}

impl DeclarativeTemplate {
    /// Create a new declarative template
    pub fn new(node_id: NodeId) -> Self {
        Self {
            template: HTMLTemplateElement::new(node_id),
            shadow_root_mode: None,
            delegates_focus: false,
        }
    }

    /// Set the shadow root mode
    pub fn set_shadow_root_mode(&mut self, mode: ShadowRootMode) {
        self.shadow_root_mode = Some(mode);
    }

    /// Check if this is a declarative shadow DOM template
    pub fn is_declarative_shadow(&self) -> bool {
        self.shadow_root_mode.is_some()
    }

    /// Get the template content
    pub fn content(&self) -> &TemplateContent {
        self.template.content()
    }

    /// Get mutable content
    pub fn content_mut(&mut self) -> &mut TemplateContent {
        self.template.content_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_element() {
        let mut template = HTMLTemplateElement::new(NodeId(1));
        
        assert!(template.content().is_empty());
        assert!(!template.has_content());
        
        template.append_to_content(NodeId(10));
        template.append_to_content(NodeId(11));
        
        assert!(template.has_content());
        assert_eq!(template.content().len(), 2);
    }

    #[test]
    fn test_template_clone_content() {
        let mut template = HTMLTemplateElement::new(NodeId(1));
        template.append_to_content(NodeId(10));
        template.append_to_content(NodeId(11));
        
        let fragment = template.clone_content();
        
        assert_eq!(fragment.len(), 2);
        assert_eq!(fragment.children[0], NodeId(10));
        assert_eq!(fragment.children[1], NodeId(11));
    }

    #[test]
    fn test_template_registry() {
        let mut registry = TemplateRegistry::new();
        
        let content = TemplateContent::from_children(vec![NodeId(10), NodeId(11)]);
        registry.register(NodeId(1), content);
        
        assert_eq!(registry.len(), 1);
        
        let retrieved = registry.get(NodeId(1)).unwrap();
        assert_eq!(retrieved.len(), 2);
        
        // Register same ID again (should not duplicate)
        let content2 = TemplateContent::new();
        registry.register(NodeId(1), content2);
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn test_template_content_operations() {
        let mut content = TemplateContent::new();
        
        content.append_child(NodeId(1));
        content.append_child(NodeId(2));
        content.prepend_child(NodeId(0));
        
        assert_eq!(content.children(), &[NodeId(0), NodeId(1), NodeId(2)]);
        
        let removed = content.remove_child(1);
        assert_eq!(removed, Some(NodeId(1)));
        assert_eq!(content.children(), &[NodeId(0), NodeId(2)]);
    }

    #[test]
    fn test_declarative_template() {
        let mut template = DeclarativeTemplate::new(NodeId(1));
        
        assert!(!template.is_declarative_shadow());
        
        template.set_shadow_root_mode(ShadowRootMode::Open);
        assert!(template.is_declarative_shadow());
        assert_eq!(template.shadow_root_mode, Some(ShadowRootMode::Open));
    }
}
