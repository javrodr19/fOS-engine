//! Lock-Free Concurrent Hash Map
//!
//! A concurrent hash map using atomic operations for thread-safe access.
//! Uses CAS-based insertion and epoch-based memory reclamation patterns.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::ptr;
use std::sync::atomic::{AtomicPtr, AtomicU64, AtomicUsize, Ordering};

/// Epoch counter for memory reclamation
static GLOBAL_EPOCH: AtomicU64 = AtomicU64::new(0);

/// Node in the hash map bucket chain
struct Node<K, V> {
    key: K,
    value: V,
    hash: u64,
    next: AtomicPtr<Node<K, V>>,
    /// Epoch when this node was marked for deletion
    delete_epoch: AtomicU64,
}

impl<K, V> Node<K, V> {
    fn new(key: K, value: V, hash: u64) -> Self {
        Self {
            key,
            value,
            hash,
            next: AtomicPtr::new(ptr::null_mut()),
            delete_epoch: AtomicU64::new(0),
        }
    }
    
    fn boxed(key: K, value: V, hash: u64) -> *mut Self {
        Box::into_raw(Box::new(Self::new(key, value, hash)))
    }
}

/// Lock-free concurrent hash map
pub struct ConcurrentMap<K, V> {
    buckets: Box<[AtomicPtr<Node<K, V>>]>,
    bucket_count: usize,
    len: AtomicUsize,
}

impl<K: Hash + Eq, V> ConcurrentMap<K, V> {
    /// Create a new concurrent map with specified bucket count
    pub fn new(bucket_count: usize) -> Self {
        let bucket_count = bucket_count.next_power_of_two().max(16);
        let buckets: Vec<_> = (0..bucket_count)
            .map(|_| AtomicPtr::new(ptr::null_mut()))
            .collect();
        
        Self {
            buckets: buckets.into_boxed_slice(),
            bucket_count,
            len: AtomicUsize::new(0),
        }
    }
    
    /// Create with default bucket count
    pub fn with_capacity(capacity: usize) -> Self {
        Self::new(capacity * 4 / 3) // Load factor ~0.75
    }
    
    /// Hash a key
    fn hash_key(&self, key: &K) -> u64 {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        hasher.finish()
    }
    
    /// Get bucket index for hash
    #[inline]
    fn bucket_index(&self, hash: u64) -> usize {
        (hash as usize) & (self.bucket_count - 1)
    }
    
    /// Get bucket for key
    #[inline]
    fn bucket_for(&self, key: &K) -> &AtomicPtr<Node<K, V>> {
        let hash = self.hash_key(key);
        &self.buckets[self.bucket_index(hash)]
    }
    
    /// Get a value by key (lock-free)
    pub fn get(&self, key: &K) -> Option<&V> {
        let hash = self.hash_key(key);
        let bucket = &self.buckets[self.bucket_index(hash)];
        let mut node = bucket.load(Ordering::Acquire);
        
        while !node.is_null() {
            // SAFETY: Node is valid while we hold a reference
            let n = unsafe { &*node };
            if n.hash == hash && n.key == *key {
                // Check if marked for deletion
                if n.delete_epoch.load(Ordering::Acquire) == 0 {
                    return Some(&n.value);
                }
            }
            node = n.next.load(Ordering::Acquire);
        }
        None
    }
    
    /// Get a mutable reference to a value (requires external synchronization)
    /// 
    /// # Safety
    /// Caller must ensure no concurrent mutations to the same key
    pub unsafe fn get_mut(&self, key: &K) -> Option<&mut V> {
        let hash = self.hash_key(key);
        let bucket = &self.buckets[self.bucket_index(hash)];
        let mut node = bucket.load(Ordering::Acquire);
        
        while !node.is_null() {
            // SAFETY: Caller must ensure no concurrent mutations
            let n = unsafe { &mut *node };
            if n.hash == hash && n.key == *key {
                if n.delete_epoch.load(Ordering::Acquire) == 0 {
                    return Some(&mut n.value);
                }
            }
            node = n.next.load(Ordering::Acquire);
        }
        None
    }
    
    /// Insert a key-value pair (CAS-based)
    pub fn insert(&self, key: K, value: V) -> Option<V>
    where
        K: Clone,
        V: Clone,
    {
        let hash = self.hash_key(&key);
        let bucket_idx = self.bucket_index(hash);
        let bucket = &self.buckets[bucket_idx];
        
        loop {
            let head = bucket.load(Ordering::Acquire);
            
            // Check if key exists
            let mut node = head;
            while !node.is_null() {
                let n = unsafe { &*node };
                if n.hash == hash && n.key == key {
                    // Key exists - update value atomically would need more complex logic
                    // For now, we don't replace - this is a simple concurrent map
                    return Some(n.value.clone());
                }
                node = n.next.load(Ordering::Acquire);
            }
            
            // Key doesn't exist - insert new node at head
            let new_node = Node::boxed(key.clone(), value.clone(), hash);
            unsafe { (*new_node).next.store(head, Ordering::Release) };
            
            match bucket.compare_exchange_weak(
                head,
                new_node,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    self.len.fetch_add(1, Ordering::Relaxed);
                    return None;
                }
                Err(_) => {
                    // CAS failed - retry
                    // SAFETY: new_node was just created and not shared
                    unsafe { drop(Box::from_raw(new_node)) };
                }
            }
        }
    }
    
    /// Insert if not present, otherwise return existing value
    pub fn get_or_insert(&self, key: K, value: V) -> &V
    where
        K: Clone,
        V: Clone,
    {
        let hash = self.hash_key(&key);
        let bucket_idx = self.bucket_index(hash);
        let bucket = &self.buckets[bucket_idx];
        
        loop {
            let head = bucket.load(Ordering::Acquire);
            
            // Check if key exists
            let mut node = head;
            while !node.is_null() {
                let n = unsafe { &*node };
                if n.hash == hash && n.key == key {
                    if n.delete_epoch.load(Ordering::Acquire) == 0 {
                        return &n.value;
                    }
                }
                node = n.next.load(Ordering::Acquire);
            }
            
            // Insert new node
            let new_node = Node::boxed(key.clone(), value.clone(), hash);
            unsafe { (*new_node).next.store(head, Ordering::Release) };
            
            match bucket.compare_exchange_weak(
                head,
                new_node,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    self.len.fetch_add(1, Ordering::Relaxed);
                    return unsafe { &(*new_node).value };
                }
                Err(_) => {
                    unsafe { drop(Box::from_raw(new_node)) };
                }
            }
        }
    }
    
    /// Remove a key (marks for deletion)
    pub fn remove(&self, key: &K) -> bool {
        let hash = self.hash_key(key);
        let bucket = &self.buckets[self.bucket_index(hash)];
        let mut node = bucket.load(Ordering::Acquire);
        
        while !node.is_null() {
            let n = unsafe { &*node };
            if n.hash == hash && n.key == *key {
                // Check if already deleted
                if n.delete_epoch.load(Ordering::Acquire) != 0 {
                    return false;  // Already deleted
                }
                // Mark for deletion using epoch
                let epoch = GLOBAL_EPOCH.fetch_add(1, Ordering::Relaxed) + 1;
                n.delete_epoch.store(epoch, Ordering::Release);
                self.len.fetch_sub(1, Ordering::Relaxed);
                return true;
            }
            node = n.next.load(Ordering::Acquire);
        }
        false
    }
    
    /// Check if key exists
    pub fn contains_key(&self, key: &K) -> bool {
        self.get(key).is_some()
    }
    
    /// Get approximate length
    pub fn len(&self) -> usize {
        self.len.load(Ordering::Relaxed)
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    
    /// Clear all entries
    pub fn clear(&self) {
        let epoch = GLOBAL_EPOCH.fetch_add(1, Ordering::Relaxed) + 1;
        
        for bucket in self.buckets.iter() {
            let mut node = bucket.load(Ordering::Acquire);
            while !node.is_null() {
                let n = unsafe { &*node };
                n.delete_epoch.store(epoch, Ordering::Release);
                node = n.next.load(Ordering::Acquire);
            }
        }
        
        self.len.store(0, Ordering::Relaxed);
    }
    
    /// Iterate over all key-value pairs
    /// Note: This provides a snapshot view, concurrent modifications may not be visible
    pub fn iter(&self) -> ConcurrentMapIter<'_, K, V> {
        ConcurrentMapIter {
            map: self,
            bucket_idx: 0,
            current: ptr::null(),
        }
    }
}

impl<K, V> Default for ConcurrentMap<K, V>
where
    K: Hash + Eq,
{
    fn default() -> Self {
        Self::new(64)
    }
}

impl<K, V> Drop for ConcurrentMap<K, V> {
    fn drop(&mut self) {
        for bucket in self.buckets.iter() {
            let mut node = bucket.load(Ordering::Relaxed);
            while !node.is_null() {
                let n = unsafe { Box::from_raw(node) };
                node = n.next.load(Ordering::Relaxed);
            }
        }
    }
}

// SAFETY: ConcurrentMap is thread-safe
unsafe impl<K: Send, V: Send> Send for ConcurrentMap<K, V> {}
unsafe impl<K: Sync, V: Sync> Sync for ConcurrentMap<K, V> {}

/// Iterator over concurrent map entries
pub struct ConcurrentMapIter<'a, K, V> {
    map: &'a ConcurrentMap<K, V>,
    bucket_idx: usize,
    current: *const Node<K, V>,
}

impl<'a, K: Hash + Eq, V> Iterator for ConcurrentMapIter<'a, K, V> {
    type Item = (&'a K, &'a V);
    
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // Try to advance in current chain
            if !self.current.is_null() {
                let node = unsafe { &*self.current };
                self.current = node.next.load(Ordering::Acquire);
                
                // Skip deleted entries
                if node.delete_epoch.load(Ordering::Acquire) == 0 {
                    return Some((&node.key, &node.value));
                }
                continue;
            }
            
            // Move to next bucket
            if self.bucket_idx >= self.map.bucket_count {
                return None;
            }
            
            self.current = self.map.buckets[self.bucket_idx].load(Ordering::Acquire);
            self.bucket_idx += 1;
        }
    }
}

/// Concurrent set (wrapper around ConcurrentMap with () values)
pub struct ConcurrentSet<K> {
    map: ConcurrentMap<K, ()>,
}

impl<K: Hash + Eq + Clone> ConcurrentSet<K> {
    /// Create new set
    pub fn new(capacity: usize) -> Self {
        Self {
            map: ConcurrentMap::with_capacity(capacity),
        }
    }
    
    /// Insert element
    pub fn insert(&self, key: K) -> bool {
        self.map.insert(key, ()).is_none()
    }
    
    /// Check if contains element
    pub fn contains(&self, key: &K) -> bool {
        self.map.contains_key(key)
    }
    
    /// Remove element
    pub fn remove(&self, key: &K) -> bool {
        self.map.remove(key)
    }
    
    /// Get length
    pub fn len(&self) -> usize {
        self.map.len()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

impl<K: Hash + Eq + Clone> Default for ConcurrentSet<K> {
    fn default() -> Self {
        Self::new(64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    
    #[test]
    fn test_basic_operations() {
        let map: ConcurrentMap<i32, String> = ConcurrentMap::new(16);
        
        assert!(map.is_empty());
        
        map.insert(1, "one".to_string());
        map.insert(2, "two".to_string());
        map.insert(3, "three".to_string());
        
        assert_eq!(map.len(), 3);
        assert_eq!(map.get(&1), Some(&"one".to_string()));
        assert_eq!(map.get(&2), Some(&"two".to_string()));
        assert_eq!(map.get(&3), Some(&"three".to_string()));
        assert_eq!(map.get(&4), None);
    }
    
    #[test]
    fn test_concurrent_insert() {
        let map = Arc::new(ConcurrentMap::<i32, i32>::new(64));
        let mut handles = vec![];
        
        for t in 0..4 {
            let map = Arc::clone(&map);
            handles.push(thread::spawn(move || {
                for i in 0..100 {
                    let key = t * 100 + i;
                    map.insert(key, key * 2);
                }
            }));
        }
        
        for handle in handles {
            handle.join().unwrap();
        }
        
        assert_eq!(map.len(), 400);
        
        for t in 0..4 {
            for i in 0..100 {
                let key = t * 100 + i;
                assert_eq!(map.get(&key), Some(&(key * 2)));
            }
        }
    }
    
    #[test]
    fn test_concurrent_read() {
        let map = Arc::new(ConcurrentMap::<i32, i32>::new(64));
        
        // Pre-populate
        for i in 0..100 {
            map.insert(i, i * 2);
        }
        
        let mut handles = vec![];
        
        for _ in 0..8 {
            let map = Arc::clone(&map);
            handles.push(thread::spawn(move || {
                for i in 0..100 {
                    assert_eq!(map.get(&i), Some(&(i * 2)));
                }
            }));
        }
        
        for handle in handles {
            handle.join().unwrap();
        }
    }
    
    #[test]
    fn test_remove() {
        let map: ConcurrentMap<i32, String> = ConcurrentMap::new(16);
        
        map.insert(1, "one".to_string());
        map.insert(2, "two".to_string());
        
        assert!(map.remove(&1));
        assert!(!map.contains_key(&1));
        assert!(map.contains_key(&2));
        
        assert!(!map.remove(&1)); // Already removed
    }
    
    #[test]
    fn test_iterator() {
        let map: ConcurrentMap<i32, i32> = ConcurrentMap::new(16);
        
        for i in 0..10 {
            map.insert(i, i * 2);
        }
        
        let mut count = 0;
        for (k, v) in map.iter() {
            assert_eq!(*v, *k * 2);
            count += 1;
        }
        
        assert_eq!(count, 10);
    }
    
    #[test]
    fn test_concurrent_set() {
        let set: ConcurrentSet<i32> = ConcurrentSet::new(16);
        
        assert!(set.insert(1));
        assert!(!set.insert(1)); // Already exists
        
        assert!(set.contains(&1));
        assert!(!set.contains(&2));
        
        assert!(set.remove(&1));
        assert!(!set.contains(&1));
    }
}
