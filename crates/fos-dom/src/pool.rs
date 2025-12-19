//! Object Pool for DOM Nodes
//!
//! Provides node recycling to reduce allocation overhead.

use std::collections::VecDeque;

/// Generic object pool
#[derive(Debug)]
pub struct Pool<T> {
    items: VecDeque<T>,
    max_size: usize,
}

impl<T> Pool<T> {
    /// Create a new pool with maximum size
    pub fn new(max_size: usize) -> Self {
        Self {
            items: VecDeque::with_capacity(max_size.min(64)),
            max_size,
        }
    }
    
    /// Get an item from the pool, or create with factory
    pub fn get<F>(&mut self, factory: F) -> T 
    where
        F: FnOnce() -> T,
    {
        self.items.pop_front().unwrap_or_else(factory)
    }
    
    /// Return an item to the pool
    pub fn put(&mut self, item: T) {
        if self.items.len() < self.max_size {
            self.items.push_back(item);
        }
        // Drop if pool is full
    }
    
    /// Clear the pool
    pub fn clear(&mut self) {
        self.items.clear();
    }
    
    /// Get pool size
    pub fn len(&self) -> usize {
        self.items.len()
    }
    
    /// Check if pool is empty
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    
    /// Get pool capacity
    pub fn capacity(&self) -> usize {
        self.max_size
    }
}

impl<T> Default for Pool<T> {
    fn default() -> Self {
        Self::new(256)
    }
}

/// Node pool for DOM elements
pub struct NodePool {
    /// Pool of element node data
    element_pool: Pool<ElementData>,
    /// Pool of text node data
    text_pool: Pool<TextData>,
    /// Stats
    pub stats: PoolStats,
}

/// Poolable element data
#[derive(Debug, Default)]
pub struct ElementData {
    pub tag: String,
    pub attributes: Vec<(String, String)>,
}

impl ElementData {
    pub fn reset(&mut self) {
        self.tag.clear();
        self.attributes.clear();
    }
}

/// Poolable text data
#[derive(Debug, Default)]
pub struct TextData {
    pub content: String,
}

impl TextData {
    pub fn reset(&mut self) {
        self.content.clear();
    }
}

/// Pool statistics
#[derive(Debug, Default, Clone)]
pub struct PoolStats {
    pub elements_reused: usize,
    pub elements_created: usize,
    pub texts_reused: usize,
    pub texts_created: usize,
}

impl PoolStats {
    pub fn reuse_rate(&self) -> f64 {
        let total = self.elements_reused + self.elements_created + 
                    self.texts_reused + self.texts_created;
        if total == 0 {
            0.0
        } else {
            (self.elements_reused + self.texts_reused) as f64 / total as f64
        }
    }
}

impl NodePool {
    pub fn new() -> Self {
        Self {
            element_pool: Pool::new(512),
            text_pool: Pool::new(256),
            stats: PoolStats::default(),
        }
    }
    
    /// Get or create element data
    pub fn get_element(&mut self) -> ElementData {
        let elem = self.element_pool.get(|| {
            self.stats.elements_created += 1;
            ElementData::default()
        });
        
        if !elem.tag.is_empty() {
            self.stats.elements_reused += 1;
        }
        
        elem
    }
    
    /// Return element data to pool
    pub fn put_element(&mut self, mut elem: ElementData) {
        elem.reset();
        self.element_pool.put(elem);
    }
    
    /// Get or create text data
    pub fn get_text(&mut self) -> TextData {
        let text = self.text_pool.get(|| {
            self.stats.texts_created += 1;
            TextData::default()
        });
        
        if !text.content.is_empty() {
            self.stats.texts_reused += 1;
        }
        
        text
    }
    
    /// Return text data to pool
    pub fn put_text(&mut self, mut text: TextData) {
        text.reset();
        self.text_pool.put(text);
    }
    
    /// Clear all pools
    pub fn clear(&mut self) {
        self.element_pool.clear();
        self.text_pool.clear();
    }
    
    /// Get pool statistics
    pub fn stats(&self) -> &PoolStats {
        &self.stats
    }
}

impl Default for NodePool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_pool_basic() {
        let mut pool: Pool<String> = Pool::new(10);
        
        // Empty pool creates new
        let s = pool.get(|| "new".to_string());
        assert_eq!(s, "new");
        
        // Put and get back
        pool.put("recycled".to_string());
        let s = pool.get(|| "new".to_string());
        assert_eq!(s, "recycled");
    }
    
    #[test]
    fn test_pool_max_size() {
        let mut pool: Pool<i32> = Pool::new(2);
        
        pool.put(1);
        pool.put(2);
        pool.put(3); // Should be dropped
        
        assert_eq!(pool.len(), 2);
    }
    
    #[test]
    fn test_node_pool() {
        let mut pool = NodePool::new();
        
        // Get element
        let mut elem = pool.get_element();
        elem.tag = "div".to_string();
        elem.attributes.push(("id".to_string(), "test".to_string()));
        
        // Return and get again (should be reset)
        pool.put_element(elem);
        let elem = pool.get_element();
        assert!(elem.tag.is_empty());
        assert!(elem.attributes.is_empty());
    }
    
    #[test]
    fn test_pool_stats() {
        let mut pool = NodePool::new();
        
        // Create some elements
        let e1 = pool.get_element();
        let e2 = pool.get_element();
        
        pool.put_element(e1);
        pool.put_element(e2);
        
        // Reuse
        let _e3 = pool.get_element();
        let _e4 = pool.get_element();
        
        assert!(pool.stats.elements_created >= 2);
    }
    
    #[test]
    fn test_text_pool() {
        let mut pool = NodePool::new();
        
        let mut text = pool.get_text();
        text.content = "Hello".to_string();
        
        pool.put_text(text);
        
        let text = pool.get_text();
        assert!(text.content.is_empty());
    }
}
