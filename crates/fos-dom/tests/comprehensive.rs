//! Comprehensive tests for fos-dom
//!
//! Tests for memory efficiency, edge cases, and correctness.

use fos_dom::{DomTree, NodeId, Document, StringInterner, InternedString};

#[test]
fn test_string_interner_deduplication() {
    let mut interner = StringInterner::new();
    
    // Same string should return same ID
    let id1 = interner.intern("hello");
    let id2 = interner.intern("hello");
    assert_eq!(id1, id2, "Same string should return same ID");
    
    // Different strings should have different IDs
    let id3 = interner.intern("world");
    assert_ne!(id1, id3, "Different strings should have different IDs");
}

#[test]
fn test_string_interner_memory_efficiency() {
    let mut interner = StringInterner::new();
    
    // Intern many duplicate strings
    for _ in 0..1000 {
        interner.intern("div");
        interner.intern("span");
        interner.intern("class");
    }
    
    // Should only have unique strings + pre-interned ones
    println!("Interner len after 3000 interns: {}", interner.len());
    assert!(interner.len() < 100, "Should deduplicate effectively");
}

#[test]
fn test_dom_tree_creation() {
    let mut tree = DomTree::new();
    
    // Create a simple structure: div > span > text
    let div = tree.create_element("div");
    let span = tree.create_element("span");
    let text = tree.create_text("Hello, World!");
    
    tree.append_child(tree.root(), div);
    tree.append_child(div, span);
    tree.append_child(span, text);
    
    // Verify structure
    assert_eq!(tree.len(), 4); // root + div + span + text
    
    // Verify parent-child relationships
    let div_node = tree.get(div).unwrap();
    assert_eq!(div_node.parent, tree.root());
    assert_eq!(div_node.first_child, span);
    
    let span_node = tree.get(span).unwrap();
    assert_eq!(span_node.parent, div);
    assert_eq!(span_node.first_child, text);
}

#[test]
fn test_dom_tree_siblings() {
    let mut tree = DomTree::new();
    
    let div = tree.create_element("div");
    let child1 = tree.create_element("p");
    let child2 = tree.create_element("p");
    let child3 = tree.create_element("p");
    
    tree.append_child(tree.root(), div);
    tree.append_child(div, child1);
    tree.append_child(div, child2);
    tree.append_child(div, child3);
    
    // Verify sibling chain
    let node1 = tree.get(child1).unwrap();
    assert_eq!(node1.next_sibling, child2);
    assert!(!node1.prev_sibling.is_valid());
    
    let node2 = tree.get(child2).unwrap();
    assert_eq!(node2.prev_sibling, child1);
    assert_eq!(node2.next_sibling, child3);
    
    let node3 = tree.get(child3).unwrap();
    assert_eq!(node3.prev_sibling, child2);
    assert!(!node3.next_sibling.is_valid());
}

#[test]
fn test_dom_tree_removal() {
    let mut tree = DomTree::new();
    
    let div = tree.create_element("div");
    let p1 = tree.create_element("p");
    let p2 = tree.create_element("p");
    let p3 = tree.create_element("p");
    
    tree.append_child(tree.root(), div);
    tree.append_child(div, p1);
    tree.append_child(div, p2);
    tree.append_child(div, p3);
    
    // Remove middle child
    tree.remove(p2);
    
    // Verify sibling chain is fixed
    let node1 = tree.get(p1).unwrap();
    assert_eq!(node1.next_sibling, p3);
    
    let node3 = tree.get(p3).unwrap();
    assert_eq!(node3.prev_sibling, p1);
}

#[test]
fn test_children_iterator() {
    let mut tree = DomTree::new();
    
    let parent = tree.create_element("ul");
    tree.append_child(tree.root(), parent);
    
    for _ in 0..5 {
        let li = tree.create_element("li");
        tree.append_child(parent, li);
    }
    
    let children: Vec<_> = tree.children(parent).collect();
    assert_eq!(children.len(), 5);
}

#[test]
fn test_document_structure() {
    let doc = Document::new("about:blank");
    
    assert!(doc.document_element().is_valid());
    assert!(doc.head().is_valid());
    assert!(doc.body().is_valid());
    
    // html should be child of root
    let tree = doc.tree();
    let html_node = tree.get(doc.document_element()).unwrap();
    assert_eq!(html_node.parent, tree.root());
}

#[test]
fn test_large_tree_memory() {
    let mut tree = DomTree::with_capacity(10000);
    
    // Create a large tree
    let root_elem = tree.create_element("div");
    tree.append_child(tree.root(), root_elem);
    
    for _ in 0..1000 {
        let parent = tree.create_element("section");
        tree.append_child(root_elem, parent);
        
        for _ in 0..10 {
            let child = tree.create_element("p");
            tree.append_child(parent, child);
            
            let text = tree.create_text("Lorem ipsum dolor sit amet");
            tree.append_child(child, text);
        }
    }
    
    println!("Tree node count: {}", tree.len());
    println!("Tree memory usage: {} bytes", tree.memory_usage());
    println!("Bytes per node: {:.2}", tree.memory_usage() as f64 / tree.len() as f64);
    
    // Should be reasonable memory usage
    assert!(tree.memory_usage() < 50_000_000, "Memory usage should be < 50MB for 21001 nodes");
}

#[test]
fn test_node_id_validity() {
    assert!(!NodeId::NONE.is_valid());
    assert!(NodeId::ROOT.is_valid());
    assert!(NodeId(0).is_valid());
    assert!(NodeId(100).is_valid());
    assert!(!NodeId(u32::MAX).is_valid());
}

#[test]
fn test_memory_sizes() {
    use std::mem::size_of;
    
    println!("=== Memory Size Analysis ===");
    println!("NodeId: {} bytes", size_of::<NodeId>());
    println!("InternedString: {} bytes", size_of::<InternedString>());
    println!("QualName: {} bytes", size_of::<fos_dom::QualName>());
    println!("Node: {} bytes", size_of::<fos_dom::Node>());
    println!("NodeData: {} bytes", size_of::<fos_dom::NodeData>());
    println!("ElementData: {} bytes", size_of::<fos_dom::ElementData>());
    
    // These should be compact
    assert_eq!(size_of::<NodeId>(), 4);
    assert_eq!(size_of::<InternedString>(), 4);
    assert_eq!(size_of::<fos_dom::QualName>(), 8);
}
