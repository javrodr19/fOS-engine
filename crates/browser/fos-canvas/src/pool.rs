//! Object Pool for Canvas Resources
//!
//! Local pool implementation to avoid cyclic dependencies with fos-engine.

use std::collections::VecDeque;

/// Generic object pool
pub struct Pool<T> {
    items: VecDeque<T>,
    capacity: usize,
}

impl<T> Default for Pool<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Pool<T> {
    /// Create a new empty pool
    pub fn new() -> Self {
        Self {
            items: VecDeque::new(),
            capacity: 64,
        }
    }

    /// Create pool with initial capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            items: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Get an item from the pool
    pub fn get(&mut self) -> Option<T> {
        self.items.pop_front()
    }

    /// Return an item to the pool
    pub fn put(&mut self, item: T) {
        if self.items.len() < self.capacity {
            self.items.push_back(item);
        }
        // If at capacity, item is dropped
    }

    /// Number of available items
    pub fn available(&self) -> usize {
        self.items.len()
    }

    /// Clear the pool
    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Check if pool is empty
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_basic() {
        let mut pool: Pool<i32> = Pool::new();
        
        pool.put(1);
        pool.put(2);
        pool.put(3);
        
        assert_eq!(pool.available(), 3);
        assert_eq!(pool.get(), Some(1));
        assert_eq!(pool.get(), Some(2));
        assert_eq!(pool.available(), 1);
    }

    #[test]
    fn test_pool_empty() {
        let mut pool: Pool<i32> = Pool::new();
        assert!(pool.get().is_none());
    }
}
