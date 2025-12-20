//! Custom Allocators
//!
//! Slab and pool allocators for efficient memory management.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::marker::PhantomData;

/// Slab allocator for fixed-size objects.
///
/// Efficient for allocating many objects of the same size,
/// such as DOM nodes or layout boxes.
///
/// # Example
/// ```rust
/// use fos_engine::SlabAllocator;
///
/// struct Node {
///     value: u64,
/// }
///
/// let mut slab = SlabAllocator::<Node>::new(1024);
/// let id = slab.alloc(Node { value: 42 });
/// assert_eq!(slab.get(id).map(|n| n.value), Some(42));
/// ```
pub struct SlabAllocator<T> {
    items: RefCell<Vec<Option<T>>>,
    free_list: RefCell<Vec<usize>>,
    capacity: usize,
}

impl<T> SlabAllocator<T> {
    /// Create a new slab allocator with given capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            items: RefCell::new(Vec::with_capacity(capacity)),
            free_list: RefCell::new(Vec::new()),
            capacity,
        }
    }
    
    /// Allocate an item and return its ID.
    pub fn alloc(&self, value: T) -> usize {
        let mut free_list = self.free_list.borrow_mut();
        let mut items = self.items.borrow_mut();
        
        if let Some(id) = free_list.pop() {
            items[id] = Some(value);
            id
        } else {
            let id = items.len();
            items.push(Some(value));
            id
        }
    }
    
    /// Get a reference to an item.
    pub fn get(&self, id: usize) -> Option<std::cell::Ref<'_, T>> {
        let items = self.items.borrow();
        if id < items.len() && items[id].is_some() {
            Some(std::cell::Ref::map(items, |v| v[id].as_ref().unwrap()))
        } else {
            None
        }
    }
    
    /// Get a mutable reference to an item.
    pub fn get_mut(&self, id: usize) -> Option<std::cell::RefMut<'_, T>> {
        let items = self.items.borrow_mut();
        if id < items.len() && items[id].is_some() {
            Some(std::cell::RefMut::map(items, |v| v[id].as_mut().unwrap()))
        } else {
            None
        }
    }
    
    /// Free an item by ID.
    pub fn free(&self, id: usize) -> Option<T> {
        let mut items = self.items.borrow_mut();
        if id < items.len() {
            let value = items[id].take();
            if value.is_some() {
                self.free_list.borrow_mut().push(id);
            }
            value
        } else {
            None
        }
    }
    
    /// Number of allocated items.
    pub fn len(&self) -> usize {
        let items = self.items.borrow();
        let free = self.free_list.borrow();
        items.len() - free.len()
    }
    
    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    
    /// Total capacity.
    pub fn capacity(&self) -> usize {
        self.capacity
    }
    
    /// Clear all items.
    pub fn clear(&self) {
        self.items.borrow_mut().clear();
        self.free_list.borrow_mut().clear();
    }
}

impl<T> Default for SlabAllocator<T> {
    fn default() -> Self {
        Self::new(1024)
    }
}

/// Pool allocator for reusable objects.
///
/// Objects are returned to the pool instead of being dropped,
/// allowing reuse without new allocations.
///
/// # Example
/// ```rust
/// use fos_engine::PoolAllocator;
///
/// let pool: PoolAllocator<Vec<u8>> = PoolAllocator::new(|| Vec::with_capacity(1024));
/// let mut buffer = pool.acquire();
/// buffer.extend_from_slice(b"hello");
/// pool.release(buffer);
/// ```
pub struct PoolAllocator<T> {
    pool: RefCell<VecDeque<T>>,
    factory: Box<dyn Fn() -> T>,
    max_size: usize,
}

impl<T> PoolAllocator<T> {
    /// Create a new pool with a factory function.
    pub fn new<F: Fn() -> T + 'static>(factory: F) -> Self {
        Self {
            pool: RefCell::new(VecDeque::new()),
            factory: Box::new(factory),
            max_size: 64,
        }
    }
    
    /// Create a pool with a maximum size.
    pub fn with_max_size<F: Fn() -> T + 'static>(factory: F, max_size: usize) -> Self {
        Self {
            pool: RefCell::new(VecDeque::new()),
            factory: Box::new(factory),
            max_size,
        }
    }
    
    /// Acquire an object from the pool.
    pub fn acquire(&self) -> T {
        self.pool.borrow_mut().pop_front().unwrap_or_else(|| (self.factory)())
    }
    
    /// Release an object back to the pool.
    pub fn release(&self, item: T) {
        let mut pool = self.pool.borrow_mut();
        if pool.len() < self.max_size {
            pool.push_back(item);
        }
        // Otherwise, drop the item
    }
    
    /// Number of items in the pool.
    pub fn available(&self) -> usize {
        self.pool.borrow().len()
    }
    
    /// Clear the pool.
    pub fn clear(&self) {
        self.pool.borrow_mut().clear();
    }
}

/// Type-erased object pool for heterogeneous types.
pub struct TypedPool<T> {
    items: RefCell<Vec<T>>,
    _marker: PhantomData<T>,
}

impl<T: Default + Clone> TypedPool<T> {
    /// Create a new typed pool.
    pub fn new() -> Self {
        Self {
            items: RefCell::new(Vec::new()),
            _marker: PhantomData,
        }
    }
    
    /// Pre-allocate items.
    pub fn preallocate(&self, count: usize) {
        let mut items = self.items.borrow_mut();
        items.reserve(count);
        for _ in 0..count {
            items.push(T::default());
        }
    }
    
    /// Acquire an item.
    pub fn acquire(&self) -> T {
        self.items.borrow_mut().pop().unwrap_or_default()
    }
    
    /// Release an item.
    pub fn release(&self, item: T) {
        self.items.borrow_mut().push(item);
    }
}

impl<T: Default + Clone> Default for TypedPool<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Memory statistics for allocators.
#[derive(Debug, Clone, Default)]
pub struct AllocStats {
    pub allocations: usize,
    pub deallocations: usize,
    pub bytes_allocated: usize,
    pub bytes_freed: usize,
    pub peak_usage: usize,
}

impl AllocStats {
    /// Record an allocation.
    pub fn record_alloc(&mut self, bytes: usize) {
        self.allocations += 1;
        self.bytes_allocated += bytes;
        let current = self.bytes_allocated - self.bytes_freed;
        if current > self.peak_usage {
            self.peak_usage = current;
        }
    }
    
    /// Record a deallocation.
    pub fn record_free(&mut self, bytes: usize) {
        self.deallocations += 1;
        self.bytes_freed += bytes;
    }
    
    /// Current usage in bytes.
    pub fn current_usage(&self) -> usize {
        self.bytes_allocated.saturating_sub(self.bytes_freed)
    }
}

// Mimalloc global allocator (optional feature)
#[cfg(feature = "mimalloc")]
use mimalloc::MiMalloc;

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_slab_allocator() {
        let slab = SlabAllocator::<u64>::new(100);
        
        let id1 = slab.alloc(42);
        let id2 = slab.alloc(100);
        
        assert_eq!(slab.get(id1).map(|v| *v), Some(42));
        assert_eq!(slab.get(id2).map(|v| *v), Some(100));
        
        slab.free(id1);
        assert!(slab.get(id1).is_none());
        
        // Reuse the freed slot
        let id3 = slab.alloc(200);
        assert_eq!(id3, id1);
    }
    
    #[test]
    fn test_pool_allocator() {
        let pool: PoolAllocator<Vec<u8>> = PoolAllocator::new(|| Vec::with_capacity(16));
        
        let mut buf = pool.acquire();
        buf.push(1);
        buf.push(2);
        pool.release(buf);
        
        assert_eq!(pool.available(), 1);
        
        let buf2 = pool.acquire();
        assert!(buf2.capacity() >= 16); // Reused with capacity
    }
    
    #[test]
    fn test_alloc_stats() {
        let mut stats = AllocStats::default();
        
        stats.record_alloc(100);
        stats.record_alloc(200);
        assert_eq!(stats.current_usage(), 300);
        
        stats.record_free(100);
        assert_eq!(stats.current_usage(), 200);
        assert_eq!(stats.peak_usage, 300);
    }
}
