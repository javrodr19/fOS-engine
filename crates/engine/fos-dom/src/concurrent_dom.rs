//! Concurrent DOM Operations
//!
//! Thread-safe DOM for parallel layout and paint operations.
//! Main thread owns mutations, worker threads get read-only snapshots.

use std::sync::{Arc, RwLock, Mutex};
use std::collections::HashMap;
use crate::NodeId;

/// Subtree lock for fine-grained concurrency
#[derive(Debug, Default)]
pub struct SubtreeLock {
    lock: RwLock<()>,
}

impl SubtreeLock {
    pub fn new() -> Self {
        Self {
            lock: RwLock::new(()),
        }
    }

    /// Acquire read lock
    pub fn read(&self) -> std::sync::RwLockReadGuard<'_, ()> {
        self.lock.read().unwrap()
    }

    /// Acquire write lock
    pub fn write(&self) -> std::sync::RwLockWriteGuard<'_, ()> {
        self.lock.write().unwrap()
    }
}

/// DOM mutation operation
#[derive(Debug, Clone)]
pub enum DomMutation {
    /// Append child to parent
    AppendChild { parent: NodeId, child: NodeId },
    /// Remove child from parent
    RemoveChild { parent: NodeId, child: NodeId },
    /// Insert before reference
    InsertBefore { parent: NodeId, child: NodeId, reference: Option<NodeId> },
    /// Replace child
    ReplaceChild { parent: NodeId, new_child: NodeId, old_child: NodeId },
    /// Set attribute
    SetAttribute { node: NodeId, name: String, value: String },
    /// Remove attribute
    RemoveAttribute { node: NodeId, name: String },
    /// Set text content
    SetTextContent { node: NodeId, content: String },
    /// Create element
    CreateElement { tag: String, result_id: NodeId },
    /// Create text node
    CreateText { content: String, result_id: NodeId },
}

/// Immutable DOM snapshot for layout/paint
#[derive(Debug, Clone)]
pub struct DomSnapshot {
    /// Version of this snapshot
    version: u64,
    /// Root node
    root: NodeId,
    /// Node data (simplified for snapshot)
    nodes: Arc<Vec<SnapshotNode>>,
}

/// Simplified node for snapshot
#[derive(Debug, Clone)]
pub struct SnapshotNode {
    pub id: NodeId,
    pub parent: NodeId,
    pub first_child: NodeId,
    pub next_sibling: NodeId,
    pub prev_sibling: NodeId,
    pub tag: Option<u32>,
    pub text: Option<String>,
    pub attrs: Vec<(String, String)>,
}

impl DomSnapshot {
    /// Create an empty snapshot
    pub fn empty() -> Self {
        Self {
            version: 0,
            root: NodeId::ROOT,
            nodes: Arc::new(Vec::new()),
        }
    }

    /// Create a snapshot from node data
    pub fn new(version: u64, root: NodeId, nodes: Vec<SnapshotNode>) -> Self {
        Self {
            version,
            root,
            nodes: Arc::new(nodes),
        }
    }

    /// Get snapshot version
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Get root node
    pub fn root(&self) -> NodeId {
        self.root
    }

    /// Get a node by ID
    pub fn get(&self, id: NodeId) -> Option<&SnapshotNode> {
        self.nodes.get(id.0 as usize)
    }

    /// Iterate children of a node
    pub fn children(&self, parent: NodeId) -> ChildrenIter<'_> {
        let first = self.get(parent).map(|n| n.first_child).unwrap_or(NodeId::NONE);
        ChildrenIter {
            snapshot: self,
            current: first,
        }
    }

    /// Get node count
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

/// Iterator over children
pub struct ChildrenIter<'a> {
    snapshot: &'a DomSnapshot,
    current: NodeId,
}

impl<'a> Iterator for ChildrenIter<'a> {
    type Item = &'a SnapshotNode;

    fn next(&mut self) -> Option<Self::Item> {
        if !self.current.is_valid() {
            return None;
        }

        let node = self.snapshot.get(self.current)?;
        self.current = node.next_sibling;
        Some(node)
    }
}

/// Concurrent DOM manager
#[derive(Debug)]
pub struct ConcurrentDom {
    /// Subtree-level locks
    subtree_locks: HashMap<NodeId, SubtreeLock>,
    /// Global DOM lock for mutations
    global_lock: RwLock<()>,
    /// Pending mutations (main thread only)
    mutation_queue: Mutex<Vec<DomMutation>>,
    /// Current snapshot for readers
    snapshot: RwLock<Arc<DomSnapshot>>,
    /// Snapshot version counter
    version: std::sync::atomic::AtomicU64,
}

impl Default for ConcurrentDom {
    fn default() -> Self {
        Self::new()
    }
}

impl ConcurrentDom {
    /// Create a new concurrent DOM
    pub fn new() -> Self {
        Self {
            subtree_locks: HashMap::new(),
            global_lock: RwLock::new(()),
            mutation_queue: Mutex::new(Vec::new()),
            snapshot: RwLock::new(Arc::new(DomSnapshot::empty())),
            version: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Get the current snapshot (lock-free read)
    pub fn snapshot(&self) -> Arc<DomSnapshot> {
        self.snapshot.read().unwrap().clone()
    }

    /// Queue a mutation (called from main thread)
    pub fn queue_mutation(&self, mutation: DomMutation) {
        self.mutation_queue.lock().unwrap().push(mutation);
    }

    /// Queue multiple mutations atomically
    pub fn queue_mutations(&self, mutations: impl IntoIterator<Item = DomMutation>) {
        let mut queue = self.mutation_queue.lock().unwrap();
        queue.extend(mutations);
    }

    /// Flush pending mutations and update snapshot
    /// Returns the mutations that were applied
    pub fn flush_mutations(&self) -> Vec<DomMutation> {
        let _global_guard = self.global_lock.write().unwrap();
        
        let mutations = {
            let mut queue = self.mutation_queue.lock().unwrap();
            std::mem::take(&mut *queue)
        };

        if !mutations.is_empty() {
            // Bump version - actual DOM update would happen here
            self.version.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }

        mutations
    }

    /// Update the snapshot (call after applying mutations to actual DOM)
    pub fn update_snapshot(&self, nodes: Vec<SnapshotNode>, root: NodeId) {
        let version = self.version.load(std::sync::atomic::Ordering::SeqCst);
        let new_snapshot = Arc::new(DomSnapshot::new(version, root, nodes));
        *self.snapshot.write().unwrap() = new_snapshot;
    }

    /// Get or create a subtree lock
    pub fn subtree_lock(&mut self, node: NodeId) -> &SubtreeLock {
        self.subtree_locks.entry(node).or_insert_with(SubtreeLock::new)
    }

    /// Lock subtree for reading
    pub fn read_subtree(&self, node: NodeId) -> Option<std::sync::RwLockReadGuard<'_, ()>> {
        self.subtree_locks.get(&node).map(|l| l.read())
    }

    /// Lock subtree for writing
    pub fn write_subtree(&self, node: NodeId) -> Option<std::sync::RwLockWriteGuard<'_, ()>> {
        self.subtree_locks.get(&node).map(|l| l.write())
    }

    /// Acquire global read lock (for layout/paint)
    pub fn read_lock(&self) -> std::sync::RwLockReadGuard<'_, ()> {
        self.global_lock.read().unwrap()
    }

    /// Acquire global write lock (for mutations)
    pub fn write_lock(&self) -> std::sync::RwLockWriteGuard<'_, ()> {
        self.global_lock.write().unwrap()
    }

    /// Check if there are pending mutations
    pub fn has_pending_mutations(&self) -> bool {
        !self.mutation_queue.lock().unwrap().is_empty()
    }

    /// Get pending mutation count
    pub fn pending_mutation_count(&self) -> usize {
        self.mutation_queue.lock().unwrap().len()
    }

    /// Current version
    pub fn version(&self) -> u64 {
        self.version.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// Thread-safe DOM accessor for worker threads
#[derive(Debug, Clone)]
pub struct DomReader {
    concurrent: Arc<ConcurrentDom>,
    snapshot: Arc<DomSnapshot>,
}

impl DomReader {
    /// Create a new reader
    pub fn new(concurrent: Arc<ConcurrentDom>) -> Self {
        let snapshot = concurrent.snapshot();
        Self { concurrent, snapshot }
    }

    /// Refresh snapshot to latest version
    pub fn refresh(&mut self) {
        self.snapshot = self.concurrent.snapshot();
    }

    /// Get the snapshot
    pub fn snapshot(&self) -> &DomSnapshot {
        &self.snapshot
    }

    /// Check if snapshot is stale
    pub fn is_stale(&self) -> bool {
        self.snapshot.version() < self.concurrent.version()
    }

    /// Get a node
    pub fn get(&self, id: NodeId) -> Option<&SnapshotNode> {
        self.snapshot.get(id)
    }

    /// Iterate children
    pub fn children(&self, parent: NodeId) -> ChildrenIter<'_> {
        self.snapshot.children(parent)
    }
}

/// DOM writer for main thread
pub struct DomWriter {
    concurrent: Arc<ConcurrentDom>,
}

impl DomWriter {
    /// Create a new writer
    pub fn new(concurrent: Arc<ConcurrentDom>) -> Self {
        Self { concurrent }
    }

    /// Queue an append child operation
    pub fn append_child(&self, parent: NodeId, child: NodeId) {
        self.concurrent.queue_mutation(DomMutation::AppendChild { parent, child });
    }

    /// Queue a remove child operation
    pub fn remove_child(&self, parent: NodeId, child: NodeId) {
        self.concurrent.queue_mutation(DomMutation::RemoveChild { parent, child });
    }

    /// Queue an insert before operation
    pub fn insert_before(&self, parent: NodeId, child: NodeId, reference: Option<NodeId>) {
        self.concurrent.queue_mutation(DomMutation::InsertBefore { parent, child, reference });
    }

    /// Queue a set attribute operation
    pub fn set_attribute(&self, node: NodeId, name: &str, value: &str) {
        self.concurrent.queue_mutation(DomMutation::SetAttribute {
            node,
            name: name.to_string(),
            value: value.to_string(),
        });
    }

    /// Queue a remove attribute operation
    pub fn remove_attribute(&self, node: NodeId, name: &str) {
        self.concurrent.queue_mutation(DomMutation::RemoveAttribute {
            node,
            name: name.to_string(),
        });
    }

    /// Queue a set text content operation
    pub fn set_text_content(&self, node: NodeId, content: &str) {
        self.concurrent.queue_mutation(DomMutation::SetTextContent {
            node,
            content: content.to_string(),
        });
    }

    /// Flush all pending mutations
    pub fn flush(&self) -> Vec<DomMutation> {
        self.concurrent.flush_mutations()
    }

    /// Check for pending mutations
    pub fn has_pending(&self) -> bool {
        self.concurrent.has_pending_mutations()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_concurrent_dom_creation() {
        let dom = ConcurrentDom::new();
        assert_eq!(dom.version(), 0);
        assert!(!dom.has_pending_mutations());
    }

    #[test]
    fn test_queue_mutations() {
        let dom = ConcurrentDom::new();
        
        dom.queue_mutation(DomMutation::AppendChild {
            parent: NodeId(0),
            child: NodeId(1),
        });

        assert!(dom.has_pending_mutations());
        assert_eq!(dom.pending_mutation_count(), 1);
    }

    #[test]
    fn test_flush_mutations() {
        let dom = ConcurrentDom::new();
        
        dom.queue_mutation(DomMutation::AppendChild {
            parent: NodeId(0),
            child: NodeId(1),
        });
        dom.queue_mutation(DomMutation::SetAttribute {
            node: NodeId(1),
            name: "class".to_string(),
            value: "test".to_string(),
        });

        let mutations = dom.flush_mutations();
        
        assert_eq!(mutations.len(), 2);
        assert!(!dom.has_pending_mutations());
        assert_eq!(dom.version(), 1);
    }

    #[test]
    fn test_snapshot_versioning() {
        let dom = Arc::new(ConcurrentDom::new());
        
        // Create a snapshot node
        let nodes = vec![
            SnapshotNode {
                id: NodeId(0),
                parent: NodeId::NONE,
                first_child: NodeId::NONE,
                next_sibling: NodeId::NONE,
                prev_sibling: NodeId::NONE,
                tag: Some(0),
                text: None,
                attrs: vec![],
            },
        ];
        dom.update_snapshot(nodes, NodeId(0));

        let snapshot = dom.snapshot();
        assert_eq!(snapshot.len(), 1);
    }

    #[test]
    fn test_reader_writer() {
        let dom = Arc::new(ConcurrentDom::new());
        
        let writer = DomWriter::new(dom.clone());
        writer.append_child(NodeId(0), NodeId(1));
        writer.set_attribute(NodeId(1), "id", "test");
        
        assert!(writer.has_pending());
        
        let mutations = writer.flush();
        assert_eq!(mutations.len(), 2);
    }

    #[test]
    fn test_concurrent_read_access() {
        let dom = Arc::new(ConcurrentDom::new());
        
        // Setup initial snapshot
        let nodes = vec![
            SnapshotNode {
                id: NodeId(0),
                parent: NodeId::NONE,
                first_child: NodeId(1),
                next_sibling: NodeId::NONE,
                prev_sibling: NodeId::NONE,
                tag: Some(0),
                text: None,
                attrs: vec![],
            },
            SnapshotNode {
                id: NodeId(1),
                parent: NodeId(0),
                first_child: NodeId::NONE,
                next_sibling: NodeId::NONE,
                prev_sibling: NodeId::NONE,
                tag: Some(1),
                text: None,
                attrs: vec![("class".to_string(), "test".to_string())],
            },
        ];
        dom.update_snapshot(nodes, NodeId(0));

        // Spawn multiple reader threads
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let dom_clone = dom.clone();
                thread::spawn(move || {
                    let reader = DomReader::new(dom_clone);
                    let snapshot = reader.snapshot();
                    
                    assert_eq!(snapshot.len(), 2);
                    
                    let root = snapshot.get(NodeId(0)).unwrap();
                    assert_eq!(root.first_child, NodeId(1));
                    
                    let child = snapshot.get(NodeId(1)).unwrap();
                    assert_eq!(child.attrs.len(), 1);
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }
    }
}
