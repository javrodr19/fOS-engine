//! Copy-on-Write
//!
//! COW wrappers for efficient cloning.

use std::sync::Arc;
use std::ops::Deref;

/// Copy-on-Write wrapper
#[derive(Debug)]
pub struct Cow<T> {
    inner: Arc<T>,
}

impl<T: Clone> Cow<T> {
    pub fn new(value: T) -> Self {
        Self { inner: Arc::new(value) }
    }
    
    /// Get shared reference
    pub fn get(&self) -> &T {
        &self.inner
    }
    
    /// Get mutable reference (clones if shared)
    pub fn get_mut(&mut self) -> &mut T {
        Arc::make_mut(&mut self.inner)
    }
    
    /// Check if we own the only reference
    pub fn is_unique(&self) -> bool {
        Arc::strong_count(&self.inner) == 1
    }
    
    /// Number of references
    pub fn ref_count(&self) -> usize {
        Arc::strong_count(&self.inner)
    }
}

impl<T> Clone for Cow<T> {
    fn clone(&self) -> Self {
        Self { inner: Arc::clone(&self.inner) }
    }
}

impl<T> Deref for Cow<T> {
    type Target = T;
    
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

/// COW buffer for byte data
#[derive(Debug, Clone)]
pub struct CowBuffer {
    data: Arc<Vec<u8>>,
}

impl CowBuffer {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data: Arc::new(data) }
    }
    
    pub fn from_slice(slice: &[u8]) -> Self {
        Self::new(slice.to_vec())
    }
    
    pub fn get(&self) -> &[u8] {
        &self.data
    }
    
    pub fn get_mut(&mut self) -> &mut Vec<u8> {
        Arc::make_mut(&mut self.data)
    }
    
    pub fn len(&self) -> usize {
        self.data.len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    
    pub fn is_unique(&self) -> bool {
        Arc::strong_count(&self.data) == 1
    }
}

/// COW string
#[derive(Debug, Clone)]
pub struct CowString {
    data: Arc<String>,
}

impl CowString {
    pub fn new(s: String) -> Self {
        Self { data: Arc::new(s) }
    }
    
    pub fn from_str(s: &str) -> Self {
        Self::new(s.to_string())
    }
    
    pub fn get(&self) -> &str {
        &self.data
    }
    
    pub fn get_mut(&mut self) -> &mut String {
        Arc::make_mut(&mut self.data)
    }
    
    pub fn is_unique(&self) -> bool {
        Arc::strong_count(&self.data) == 1
    }
}

impl Deref for CowString {
    type Target = str;
    
    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

/// COW DOM tree (for cloning document fragments)
#[derive(Debug, Clone)]
pub struct CowTree<T> {
    nodes: Arc<Vec<T>>,
}

impl<T: Clone> CowTree<T> {
    pub fn new(nodes: Vec<T>) -> Self {
        Self { nodes: Arc::new(nodes) }
    }
    
    pub fn get(&self, index: usize) -> Option<&T> {
        self.nodes.get(index)
    }
    
    pub fn len(&self) -> usize {
        self.nodes.len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
    
    pub fn mutate(&mut self) -> &mut Vec<T> {
        Arc::make_mut(&mut self.nodes)
    }
    
    pub fn is_unique(&self) -> bool {
        Arc::strong_count(&self.nodes) == 1
    }
}

/// COW style inheritance
#[derive(Debug, Clone)]
pub struct CowStyles<T> {
    inherited: Arc<T>,
    overrides: Option<T>,
}

impl<T: Clone + Default> CowStyles<T> {
    pub fn new(inherited: T) -> Self {
        Self {
            inherited: Arc::new(inherited),
            overrides: None,
        }
    }
    
    pub fn get_inherited(&self) -> &T {
        &self.inherited
    }
    
    pub fn get_overrides(&self) -> Option<&T> {
        self.overrides.as_ref()
    }
    
    pub fn set_override(&mut self, overrides: T) {
        self.overrides = Some(overrides);
    }
    
    pub fn inherit(&self) -> Self {
        Self {
            inherited: Arc::clone(&self.inherited),
            overrides: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cow() {
        let cow1 = Cow::new(vec![1, 2, 3]);
        let mut cow2 = cow1.clone();
        
        assert_eq!(cow1.ref_count(), 2);
        
        // Mutation clones
        cow2.get_mut().push(4);
        
        assert_eq!(cow1.get(), &vec![1, 2, 3]);
        assert_eq!(cow2.get(), &vec![1, 2, 3, 4]);
    }
    
    #[test]
    fn test_cow_buffer() {
        let buf1 = CowBuffer::new(vec![1, 2, 3]);
        let buf2 = buf1.clone();
        
        assert!(!buf1.is_unique());
        assert_eq!(buf1.get(), buf2.get());
    }
}
