//! Copy-on-Write Utilities for fos-js
//!
//! Minimal CoW buffer implementation to avoid cyclic dependencies with fos-engine.

use std::sync::Arc;

/// Copy-on-Write buffer
#[derive(Debug, Clone)]
pub struct CowBuffer {
    data: Arc<Vec<u8>>,
}

impl CowBuffer {
    /// Create a new CowBuffer
    pub fn new(data: Vec<u8>) -> Self {
        Self { data: Arc::new(data) }
    }

    /// Get immutable reference to data
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Get the length
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Check if this is the only reference
    pub fn is_unique(&self) -> bool {
        Arc::strong_count(&self.data) == 1
    }

    /// Get mutable access (copies if shared)
    pub fn make_mut(&mut self) -> &mut Vec<u8> {
        Arc::make_mut(&mut self.data)
    }

    /// Clone the underlying data
    pub fn to_vec(&self) -> Vec<u8> {
        self.data.as_ref().clone()
    }
}

/// Generic Copy-on-Write wrapper
#[derive(Debug, Clone)]
pub struct Cow<T: Clone> {
    data: Arc<T>,
}

impl<T: Clone> Cow<T> {
    /// Create new Cow wrapper
    pub fn new(data: T) -> Self {
        Self { data: Arc::new(data) }
    }

    /// Get immutable reference
    pub fn get(&self) -> &T {
        &self.data
    }

    /// Check if unique
    pub fn is_unique(&self) -> bool {
        Arc::strong_count(&self.data) == 1
    }

    /// Get mutable access (copies if shared)
    pub fn make_mut(&mut self) -> &mut T {
        Arc::make_mut(&mut self.data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cow_buffer() {
        let buf1 = CowBuffer::new(vec![1, 2, 3, 4, 5]);
        assert_eq!(buf1.len(), 5);
        assert!(buf1.is_unique());

        let buf2 = buf1.clone();
        assert!(!buf1.is_unique());
        assert!(!buf2.is_unique());

        assert_eq!(buf1.data(), buf2.data());
    }

    #[test]
    fn test_cow_buffer_mut() {
        let mut buf = CowBuffer::new(vec![1, 2, 3]);
        let buf2 = buf.clone();

        // Modify buf - should copy
        buf.make_mut().push(4);

        assert_eq!(buf.data(), &[1, 2, 3, 4]);
        assert_eq!(buf2.data(), &[1, 2, 3]);
    }

    #[test]
    fn test_cow_generic() {
        let cow1: Cow<String> = Cow::new("hello".to_string());
        let cow2 = cow1.clone();

        assert_eq!(cow1.get(), "hello");
        assert_eq!(cow2.get(), "hello");
    }
}
