//! Object Pooling
//!
//! Reuse deallocated objects to reduce allocation overhead.
//! Thread-local pools for lock-free access.

use super::object::{JsObject, JsArray};
use std::cell::RefCell;

/// Object pool for reusing deallocated objects
#[derive(Debug, Default)]
pub struct ObjectPool {
    objects: Vec<JsObject>,
    arrays: Vec<JsArray>,
    max_size: usize,
    allocations_saved: u64,
}

impl ObjectPool {
    pub fn new() -> Self { Self::with_max_size(1024) }
    
    pub fn with_max_size(max: usize) -> Self {
        Self {
            objects: Vec::with_capacity(max),
            arrays: Vec::with_capacity(max),
            max_size: max,
            allocations_saved: 0,
        }
    }
    
    /// Get or create an object
    #[inline]
    pub fn get_object(&mut self) -> JsObject {
        if let Some(mut obj) = self.objects.pop() {
            self.allocations_saved += 1;
            obj
        } else {
            JsObject::new()
        }
    }
    
    /// Return object to pool for reuse
    #[inline]
    pub fn return_object(&mut self, obj: JsObject) {
        if self.objects.len() < self.max_size {
            self.objects.push(obj);
        }
    }
    
    /// Get or create an array
    #[inline]
    pub fn get_array(&mut self) -> JsArray {
        if let Some(arr) = self.arrays.pop() {
            self.allocations_saved += 1;
            arr
        } else {
            JsArray::new()
        }
    }
    
    /// Return array to pool for reuse
    #[inline]
    pub fn return_array(&mut self, arr: JsArray) {
        if self.arrays.len() < self.max_size {
            self.arrays.push(arr);
        }
    }
    
    pub fn stats(&self) -> PoolStats {
        PoolStats {
            objects_pooled: self.objects.len(),
            arrays_pooled: self.arrays.len(),
            allocations_saved: self.allocations_saved,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PoolStats {
    pub objects_pooled: usize,
    pub arrays_pooled: usize,
    pub allocations_saved: u64,
}

// Thread-local object pool
thread_local! {
    static OBJECT_POOL: RefCell<ObjectPool> = RefCell::new(ObjectPool::new());
}

/// Get object from thread-local pool
#[inline]
pub fn pooled_object() -> JsObject {
    OBJECT_POOL.with(|pool| pool.borrow_mut().get_object())
}

/// Return object to thread-local pool
#[inline]
pub fn return_object(obj: JsObject) {
    OBJECT_POOL.with(|pool| pool.borrow_mut().return_object(obj));
}

/// Get array from thread-local pool
#[inline]
pub fn pooled_array() -> JsArray {
    OBJECT_POOL.with(|pool| pool.borrow_mut().get_array())
}

/// Return array to thread-local pool
#[inline]
pub fn return_array(arr: JsArray) {
    OBJECT_POOL.with(|pool| pool.borrow_mut().return_array(arr));
}

/// Get pool stats
pub fn pool_stats() -> PoolStats {
    OBJECT_POOL.with(|pool| pool.borrow().stats())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_object_pool() {
        let obj = pooled_object();
        return_object(obj);
        
        let stats = pool_stats();
        assert_eq!(stats.objects_pooled, 1);
    }
    
    #[test]
    fn test_reuse() {
        let obj1 = pooled_object();
        return_object(obj1);
        let _obj2 = pooled_object(); // Should reuse
        
        let stats = pool_stats();
        assert!(stats.allocations_saved >= 1);
    }
}
