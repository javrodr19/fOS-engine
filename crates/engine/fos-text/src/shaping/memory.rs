//! Memory Efficiency Utilities
//!
//! Local implementations of BumpAllocator and StringInterner
//! to avoid cyclic dependencies with fos-engine.

use std::cell::RefCell;
use std::alloc::{alloc, dealloc, Layout};
use std::collections::HashMap;
use std::ptr::NonNull;
use std::sync::Arc;

// ============================================================================
// Bump Allocator
// ============================================================================

/// Bump allocator for fast sequential allocation
#[derive(Debug)]
pub struct BumpAllocator {
    chunks: RefCell<Vec<Chunk>>,
    chunk_size: usize,
}

#[derive(Debug)]
struct Chunk {
    data: NonNull<u8>,
    size: usize,
    used: usize,
}

impl Chunk {
    fn new(size: usize) -> Self {
        let layout = Layout::from_size_align(size, 8).unwrap();
        let data = unsafe { NonNull::new(alloc(layout)).expect("allocation failed") };
        Self { data, size, used: 0 }
    }
    
    fn alloc(&mut self, size: usize, align: usize) -> Option<NonNull<u8>> {
        let aligned = (self.used + align - 1) & !(align - 1);
        if aligned + size > self.size {
            return None;
        }
        
        let ptr = unsafe { NonNull::new_unchecked(self.data.as_ptr().add(aligned)) };
        self.used = aligned + size;
        Some(ptr)
    }
    
    fn reset(&mut self) {
        self.used = 0;
    }
}

impl Drop for Chunk {
    fn drop(&mut self) {
        let layout = Layout::from_size_align(self.size, 8).unwrap();
        unsafe { dealloc(self.data.as_ptr(), layout) };
    }
}

impl BumpAllocator {
    pub fn new() -> Self {
        Self::with_chunk_size(64 * 1024) // 64KB default
    }
    
    pub fn with_chunk_size(chunk_size: usize) -> Self {
        Self {
            chunks: RefCell::new(vec![Chunk::new(chunk_size)]),
            chunk_size,
        }
    }
    
    /// Allocate memory
    pub fn alloc(&self, size: usize, align: usize) -> NonNull<u8> {
        let mut chunks = self.chunks.borrow_mut();
        
        // Try current chunk
        if let Some(ptr) = chunks.last_mut().and_then(|c| c.alloc(size, align)) {
            return ptr;
        }
        
        // Need new chunk
        let new_size = self.chunk_size.max(size + align);
        let mut chunk = Chunk::new(new_size);
        let ptr = chunk.alloc(size, align).expect("fresh chunk should have space");
        chunks.push(chunk);
        ptr
    }
    
    /// Allocate and initialize
    pub fn alloc_with<T>(&self, value: T) -> &mut T {
        let ptr = self.alloc(std::mem::size_of::<T>(), std::mem::align_of::<T>());
        unsafe {
            let typed = ptr.as_ptr() as *mut T;
            std::ptr::write(typed, value);
            &mut *typed
        }
    }
    
    /// Reset all allocations
    pub fn reset(&self) {
        let mut chunks = self.chunks.borrow_mut();
        for chunk in chunks.iter_mut() {
            chunk.reset();
        }
        // Keep only first chunk
        chunks.truncate(1);
    }
    
    /// Total bytes used
    pub fn bytes_used(&self) -> usize {
        self.chunks.borrow().iter().map(|c| c.used).sum()
    }
    
    /// Total capacity
    pub fn capacity(&self) -> usize {
        self.chunks.borrow().iter().map(|c| c.size).sum()
    }
}

impl Default for BumpAllocator {
    fn default() -> Self { Self::new() }
}

// ============================================================================
// String Interner
// ============================================================================

/// Interned string reference
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct InternedString {
    id: u32,
}

impl InternedString {
    pub fn id(&self) -> u32 {
        self.id
    }
}

impl std::fmt::Debug for InternedString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "InternedString({})", self.id)
    }
}

/// String interner for deduplication
#[derive(Debug, Default)]
pub struct StringInterner {
    strings: Vec<Arc<str>>,
    lookup: HashMap<Arc<str>, u32>,
}

impl StringInterner {
    pub fn new() -> Self { Self::default() }
    
    /// Intern a string
    pub fn intern(&mut self, s: &str) -> InternedString {
        if let Some(&id) = self.lookup.get(s) {
            return InternedString { id };
        }
        
        let id = self.strings.len() as u32;
        let arc: Arc<str> = s.into();
        self.strings.push(arc.clone());
        self.lookup.insert(arc, id);
        InternedString { id }
    }
    
    /// Get string by ID
    pub fn get(&self, interned: &InternedString) -> Option<&str> {
        self.strings.get(interned.id as usize).map(|s| s.as_ref())
    }
    
    /// Number of interned strings
    pub fn len(&self) -> usize {
        self.strings.len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_bump_allocator() {
        let alloc = BumpAllocator::new();
        let ptr = alloc.alloc(16, 8);
        assert!(!ptr.as_ptr().is_null());
    }
    
    #[test]
    fn test_string_interner() {
        let mut interner = StringInterner::new();
        let id1 = interner.intern("hello");
        let id2 = interner.intern("hello");
        assert_eq!(id1.id(), id2.id());
        assert_eq!(interner.get(&id1), Some("hello"));
    }
}
