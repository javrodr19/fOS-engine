//! Copy-on-Write and Arena Allocator Integration
//!
//! COW wrappers for efficient cloning, bump allocator for fast allocation.

use std::sync::Arc;
use std::ops::Deref;
use std::cell::RefCell;

/// Copy-on-Write wrapper
#[derive(Debug)]
pub struct Cow<T> {
    inner: Arc<T>,
}

impl<T: Clone> Cow<T> {
    pub fn new(value: T) -> Self { Self { inner: Arc::new(value) } }
    pub fn get(&self) -> &T { &self.inner }
    pub fn get_mut(&mut self) -> &mut T { Arc::make_mut(&mut self.inner) }
    pub fn is_unique(&self) -> bool { Arc::strong_count(&self.inner) == 1 }
    pub fn ref_count(&self) -> usize { Arc::strong_count(&self.inner) }
}

impl<T> Clone for Cow<T> {
    fn clone(&self) -> Self { Self { inner: Arc::clone(&self.inner) } }
}

impl<T> Deref for Cow<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target { &self.inner }
}

/// COW buffer for byte data
#[derive(Debug, Clone)]
pub struct CowBuffer {
    data: Arc<Vec<u8>>,
}

impl CowBuffer {
    pub fn new(data: Vec<u8>) -> Self { Self { data: Arc::new(data) } }
    pub fn from_slice(slice: &[u8]) -> Self { Self::new(slice.to_vec()) }
    pub fn get(&self) -> &[u8] { &self.data }
    pub fn get_mut(&mut self) -> &mut Vec<u8> { Arc::make_mut(&mut self.data) }
    pub fn len(&self) -> usize { self.data.len() }
    pub fn is_empty(&self) -> bool { self.data.is_empty() }
    pub fn is_unique(&self) -> bool { Arc::strong_count(&self.data) == 1 }
}

/// COW string
#[derive(Debug, Clone)]
pub struct CowString {
    data: Arc<String>,
}

impl CowString {
    pub fn new(s: String) -> Self { Self { data: Arc::new(s) } }
    pub fn from_str(s: &str) -> Self { Self::new(s.to_string()) }
    pub fn get(&self) -> &str { &self.data }
    pub fn get_mut(&mut self) -> &mut String { Arc::make_mut(&mut self.data) }
    pub fn is_unique(&self) -> bool { Arc::strong_count(&self.data) == 1 }
}

impl Deref for CowString {
    type Target = str;
    fn deref(&self) -> &Self::Target { &self.data }
}

/// Simple bump allocator (stores Vec of chunks)
#[derive(Debug, Default)]
pub struct BumpAllocator {
    chunks: RefCell<Vec<Vec<u8>>>,
    chunk_size: usize,
}

impl BumpAllocator {
    pub fn new() -> Self {
        Self { chunks: RefCell::new(vec![Vec::with_capacity(64 * 1024)]), chunk_size: 64 * 1024 }
    }
    
    pub fn with_chunk_size(chunk_size: usize) -> Self {
        Self { chunks: RefCell::new(vec![Vec::with_capacity(chunk_size)]), chunk_size }
    }
    
    /// Allocate memory (simplified - returns Vec index + offset)
    pub fn alloc(&self, size: usize) -> (usize, usize) {
        let mut chunks = self.chunks.borrow_mut();
        let last_idx = chunks.len() - 1;
        let last_len = chunks[last_idx].len();
        let last_cap = chunks[last_idx].capacity();
        
        // Check if last chunk has space
        if last_len + size <= last_cap {
            let offset = last_len;
            chunks[last_idx].resize(last_len + size, 0);
            return (last_idx, offset);
        }
        
        // Need new chunk
        let new_capacity = self.chunk_size.max(size);
        let mut new_chunk = Vec::with_capacity(new_capacity);
        new_chunk.resize(size, 0);
        chunks.push(new_chunk);
        (chunks.len() - 1, 0)
    }
    
    /// Reset all allocations
    pub fn reset(&self) {
        let mut chunks = self.chunks.borrow_mut();
        chunks.clear();
        chunks.push(Vec::with_capacity(self.chunk_size));
    }
    
    /// Total allocated bytes
    pub fn total_allocated(&self) -> usize {
        self.chunks.borrow().iter().map(|c| c.len()).sum()
    }
    
    /// Number of chunks
    pub fn chunk_count(&self) -> usize {
        self.chunks.borrow().len()
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
    
    #[test]
    fn test_bump_allocator() {
        let alloc = BumpAllocator::new();
        let (chunk1, _) = alloc.alloc(100);
        let (chunk2, _) = alloc.alloc(200);
        assert_eq!(chunk1, chunk2); // Same chunk
    }
}
