//! Edge case and stress tests for fos-dom
//!
//! These tests cover rare scenarios, boundary conditions, and stress testing.

use fos_dom::{DomTree, NodeId, Document, StringInterner, InternedString, QualName, Node, NodeData};

// ============================================================================
// STRING INTERNER EDGE CASES
// ============================================================================

#[test]
fn test_interner_empty_string() {
    let mut interner = StringInterner::new();
    let id = interner.intern("");
    assert_eq!(interner.get(id), "");
}

#[test]
fn test_interner_unicode_strings() {
    let mut interner = StringInterner::new();
    
    let cases = [
        "hello",
        "世界",
        "🚀🌍🎉",
        "Ñoño",
        "مرحبا",
        "שלום",
        "Привет",
        "こんにちは",
        "안녕하세요",
    ];
    
    for s in &cases {
        let id = interner.intern(s);
        assert_eq!(interner.get(id), *s, "Unicode string mismatch: {}", s);
    }
}

#[test]
fn test_interner_special_characters() {
    let mut interner = StringInterner::new();
    
    // Now supports all characters including null bytes!
    let cases = [
        "\t",
        "\n",
        "\r\n",
        "line1\nline2",
        "tab\there",
        "null\0char",  // Now works!
        "backslash\\here",
        "quote\"inside",
        "<script>alert('xss')</script>",
    ];
    
    for s in &cases {
        let id = interner.intern(s);
        assert_eq!(interner.get(id), *s);
    }
}

#[test]
fn test_interner_very_long_string() {
    let mut interner = StringInterner::new();
    
    // 10KB string
    let long_string: String = "a".repeat(10_000);
    let id = interner.intern(&long_string);
    assert_eq!(interner.get(id).len(), 10_000);
}

#[test]
fn test_interner_many_unique_strings() {
    let mut interner = StringInterner::new();
    
    // Intern 10,000 unique strings
    let mut ids = Vec::new();
    for i in 0..10_000 {
        let s = format!("unique_string_{}", i);
        ids.push((interner.intern(&s), s));
    }
    
    // Verify all are retrievable
    for (id, expected) in &ids {
        assert_eq!(interner.get(*id), expected.as_str());
    }
}

// ============================================================================
// DOM TREE EDGE CASES
// ============================================================================

#[test]
fn test_tree_empty_operations() {
    let tree = DomTree::new();
    
    // Operations on empty tree
    assert!(tree.get(NodeId(999)).is_none());
    assert_eq!(tree.children(NodeId(999)).count(), 0);
}

#[test]
fn test_tree_single_node() {
    let mut tree = DomTree::new();
    let div = tree.create_element("div");
    tree.append_child(tree.root(), div);
    
    // Single node operations
    let node = tree.get(div).unwrap();
    assert!(!node.first_child.is_valid());
    assert!(!node.last_child.is_valid());
    assert!(!node.next_sibling.is_valid());
    assert!(!node.prev_sibling.is_valid());
}

#[test]
fn test_tree_deep_nesting() {
    let mut tree = DomTree::new();
    
    // Create 100 levels of nesting
    let mut parent = tree.root();
    for i in 0..100 {
        let child = tree.create_element("div");
        tree.append_child(parent, child);
        parent = child;
    }
    
    // Add text at deepest level
    let text = tree.create_text("Deep content");
    tree.append_child(parent, text);
    
    assert_eq!(tree.len(), 102); // root + 100 divs + text
}

#[test]
fn test_tree_wide_tree() {
    let mut tree = DomTree::new();
    
    let parent = tree.create_element("ul");
    tree.append_child(tree.root(), parent);
    
    // Create 1000 siblings
    for i in 0..1000 {
        let li = tree.create_element("li");
        tree.append_child(parent, li);
    }
    
    // Verify children count
    let children: Vec<_> = tree.children(parent).collect();
    assert_eq!(children.len(), 1000);
    
    // Verify first/last child
    let parent_node = tree.get(parent).unwrap();
    assert!(parent_node.first_child.is_valid());
    assert!(parent_node.last_child.is_valid());
    assert_ne!(parent_node.first_child, parent_node.last_child);
}

#[test]
fn test_tree_remove_all_children() {
    let mut tree = DomTree::new();
    
    let parent = tree.create_element("div");
    tree.append_child(tree.root(), parent);
    
    let mut children = Vec::new();
    for _ in 0..5 {
        let child = tree.create_element("p");
        tree.append_child(parent, child);
        children.push(child);
    }
    
    // Remove all children
    for child in children {
        tree.remove(child);
    }
    
    // Parent should have no children
    let parent_node = tree.get(parent).unwrap();
    assert!(!parent_node.first_child.is_valid());
    assert!(!parent_node.last_child.is_valid());
}

#[test]
fn test_tree_remove_first_child() {
    let mut tree = DomTree::new();
    
    let parent = tree.create_element("div");
    tree.append_child(tree.root(), parent);
    
    let child1 = tree.create_element("p");
    let child2 = tree.create_element("p");
    let child3 = tree.create_element("p");
    
    tree.append_child(parent, child1);
    tree.append_child(parent, child2);
    tree.append_child(parent, child3);
    
    // Remove first child
    tree.remove(child1);
    
    let parent_node = tree.get(parent).unwrap();
    assert_eq!(parent_node.first_child, child2);
    
    let child2_node = tree.get(child2).unwrap();
    assert!(!child2_node.prev_sibling.is_valid());
}

#[test]
fn test_tree_remove_last_child() {
    let mut tree = DomTree::new();
    
    let parent = tree.create_element("div");
    tree.append_child(tree.root(), parent);
    
    let child1 = tree.create_element("p");
    let child2 = tree.create_element("p");
    let child3 = tree.create_element("p");
    
    tree.append_child(parent, child1);
    tree.append_child(parent, child2);
    tree.append_child(parent, child3);
    
    // Remove last child
    tree.remove(child3);
    
    let parent_node = tree.get(parent).unwrap();
    assert_eq!(parent_node.last_child, child2);
    
    let child2_node = tree.get(child2).unwrap();
    assert!(!child2_node.next_sibling.is_valid());
}

#[test]
fn test_tree_remove_only_child() {
    let mut tree = DomTree::new();
    
    let parent = tree.create_element("div");
    tree.append_child(tree.root(), parent);
    
    let child = tree.create_element("p");
    tree.append_child(parent, child);
    
    // Remove only child
    tree.remove(child);
    
    let parent_node = tree.get(parent).unwrap();
    assert!(!parent_node.first_child.is_valid());
    assert!(!parent_node.last_child.is_valid());
}

// ============================================================================
// TEXT NODE EDGE CASES
// ============================================================================

#[test]
fn test_text_empty() {
    let mut tree = DomTree::new();
    let text = tree.create_text("");
    
    let node = tree.get(text).unwrap();
    assert_eq!(node.as_text(), Some(""));
}

#[test]
fn test_text_whitespace_only() {
    let mut tree = DomTree::new();
    
    let cases = [
        "   ",
        "\t\t\t",
        "\n\n\n",
        "  \t\n  ",
    ];
    
    for content in &cases {
        let text = tree.create_text(content);
        let node = tree.get(text).unwrap();
        assert_eq!(node.as_text(), Some(*content));
    }
}

#[test]
fn test_text_unicode() {
    let mut tree = DomTree::new();
    
    let content = "Hello 世界! 🚀 Ñoño привет";
    let text = tree.create_text(content);
    
    let node = tree.get(text).unwrap();
    assert_eq!(node.as_text(), Some(content));
}

// ============================================================================
// COMMENT NODE EDGE CASES
// ============================================================================

#[test]
fn test_comment_empty() {
    let mut tree = DomTree::new();
    let comment = tree.create_comment("");
    
    let node = tree.get(comment).unwrap();
    if let NodeData::Comment(content) = &node.data {
        assert_eq!(content, "");
    } else {
        panic!("Expected comment node");
    }
}

#[test]
fn test_comment_with_dashes() {
    let mut tree = DomTree::new();
    
    // HTML comments can't have -- in the middle, but we store raw content
    let content = "-- invalid -- comment --";
    let comment = tree.create_comment(content);
    
    let node = tree.get(comment).unwrap();
    if let NodeData::Comment(stored) = &node.data {
        assert_eq!(stored, content);
    }
}

// ============================================================================
// NODE ID EDGE CASES
// ============================================================================

#[test]
fn test_node_id_constants() {
    assert_eq!(NodeId::NONE.0, u32::MAX);
    assert_eq!(NodeId::ROOT.0, 0);
}

#[test]
fn test_node_id_equality() {
    assert_eq!(NodeId(5), NodeId(5));
    assert_ne!(NodeId(5), NodeId(6));
    assert_ne!(NodeId::NONE, NodeId::ROOT);
}

#[test]
fn test_node_id_hash() {
    use std::collections::HashSet;
    
    let mut set = HashSet::new();
    set.insert(NodeId(1));
    set.insert(NodeId(2));
    set.insert(NodeId(1)); // duplicate
    
    assert_eq!(set.len(), 2);
}

// ============================================================================
// DOCUMENT EDGE CASES
// ============================================================================

#[test]
fn test_document_empty_url() {
    let doc = Document::new("");
    assert_eq!(doc.url(), "");
}

#[test]
fn test_document_complex_url() {
    let url = "https://user:pass@example.com:8080/path?query=1&foo=bar#section";
    let doc = Document::new(url);
    assert_eq!(doc.url(), url);
}

#[test]
fn test_document_empty_title() {
    let doc = Document::empty("about:blank");
    assert_eq!(doc.title(), "");
}

// ============================================================================
// STRESS TESTS
// ============================================================================

#[test]
fn test_stress_rapid_creation_deletion() {
    let mut tree = DomTree::new();
    let root = tree.root();
    
    // Rapidly create and reference nodes
    for _ in 0..1000 {
        let parent = tree.create_element("div");
        tree.append_child(root, parent);
        
        for _ in 0..10 {
            let child = tree.create_element("span");
            tree.append_child(parent, child);
            
            let text = tree.create_text("content");
            tree.append_child(child, text);
        }
    }
    
    // Tree should contain all nodes (nodes aren't freed on remove)
    assert!(tree.len() > 20000);
}

#[test]
fn test_stress_deep_recursion() {
    // Test that deep structures don't cause stack overflow
    let mut tree = DomTree::new();
    
    let mut current = tree.root();
    for _ in 0..500 {
        let child = tree.create_element("div");
        tree.append_child(current, child);
        current = child;
    }
    
    // Navigate back up
    let mut depth = 0;
    let mut node = current;
    while node.is_valid() && node != tree.root() {
        if let Some(n) = tree.get(node) {
            node = n.parent;
            depth += 1;
        } else {
            break;
        }
    }
    
    assert_eq!(depth, 500);
}

// ============================================================================
// QUALNAME TESTS
// ============================================================================

#[test]
fn test_qualname_equality() {
    let ns1 = InternedString::EMPTY;
    let local1 = InternedString(1);
    
    let name1 = QualName::new(ns1, local1);
    let name2 = QualName::new(ns1, local1);
    
    assert_eq!(name1, name2);
}

#[test]
fn test_qualname_hash() {
    use std::collections::HashMap;
    
    let mut map = HashMap::new();
    
    let name = QualName::new(InternedString::EMPTY, InternedString(1));
    map.insert(name, "value");
    
    assert_eq!(map.get(&name), Some(&"value"));
}
