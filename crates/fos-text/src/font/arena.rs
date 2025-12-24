//! Arena Allocator (Local Copy)
//!
//! Bump allocator for efficient font data parsing.
//! Local copy to avoid cyclic dependency with fos-engine.

use std::cell::RefCell;
use std::alloc::{alloc, dealloc, Layout};
use std::ptr::NonNull;

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
        
        if let Some(ptr) = chunks.last_mut().and_then(|c| c.alloc(size, align)) {
            return ptr;
        }
        
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
        chunks.truncate(1);
    }
    
    /// Total bytes used
    pub fn bytes_used(&self) -> usize {
        self.chunks.borrow().iter().map(|c| c.used).sum()
    }
}

impl Default for BumpAllocator {
    fn default() -> Self { Self::new() }
}

/// Arena for typed allocations
pub struct Arena<T> {
    items: RefCell<Vec<T>>,
}

impl<T> Arena<T> {
    pub fn new() -> Self {
        Self::with_capacity(1024)
    }
    
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            items: RefCell::new(Vec::with_capacity(capacity)),
        }
    }
    
    /// Allocate item
    pub fn alloc(&self, value: T) -> usize {
        let mut items = self.items.borrow_mut();
        let id = items.len();
        items.push(value);
        id
    }
    
    /// Get item
    pub fn get(&self, id: usize) -> Option<std::cell::Ref<'_, T>> {
        let items = self.items.borrow();
        if id < items.len() {
            Some(std::cell::Ref::map(items, |v| &v[id]))
        } else {
            None
        }
    }
    
    /// Number of items
    pub fn len(&self) -> usize {
        self.items.borrow().len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.items.borrow().is_empty()
    }
    
    /// Clear all items
    pub fn clear(&self) {
        self.items.borrow_mut().clear();
    }
}

impl<T> Default for Arena<T> {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_bump_allocator() {
        let alloc = BumpAllocator::new();
        let _ = alloc.alloc(100, 8);
        let _ = alloc.alloc(200, 8);
        assert!(alloc.bytes_used() >= 300);
        
        alloc.reset();
        assert_eq!(alloc.bytes_used(), 0);
    }
    
    #[test]
    fn test_arena() {
        let arena: Arena<i32> = Arena::new();
        let id = arena.alloc(42);
        assert_eq!(*arena.get(id).unwrap(), 42);
    }
}
