//! ES2024 Features
//!
//! Implements new ES2024 JavaScript features:
//! - Resizable ArrayBuffer
//! - Array grouping (Object.groupBy, Map.groupBy)
//! - Promise.withResolvers
//! - Atomics.waitAsync

use std::collections::HashMap;

// =============================================================================
// Resizable ArrayBuffer
// =============================================================================

/// Resizable ArrayBuffer implementation
/// Allows the buffer to grow or shrink after creation.
#[derive(Debug)]
pub struct ResizableArrayBuffer {
    /// Underlying data
    data: Vec<u8>,
    /// Current byte length
    byte_length: usize,
    /// Maximum byte length (for resizable buffers)
    max_byte_length: Option<usize>,
    /// Whether this is detached
    detached: bool,
}

impl ResizableArrayBuffer {
    /// Create a new ArrayBuffer with fixed size
    pub fn new(byte_length: usize) -> Self {
        Self {
            data: vec![0u8; byte_length],
            byte_length,
            max_byte_length: None,
            detached: false,
        }
    }

    /// Create a new resizable ArrayBuffer
    pub fn new_resizable(byte_length: usize, max_byte_length: usize) -> Result<Self, ArrayBufferError> {
        if byte_length > max_byte_length {
            return Err(ArrayBufferError::RangeError(
                "byteLength exceeds maxByteLength".into()
            ));
        }

        Ok(Self {
            data: vec![0u8; byte_length],
            byte_length,
            max_byte_length: Some(max_byte_length),
            detached: false,
        })
    }

    /// Get byte length
    pub fn byte_length(&self) -> usize {
        if self.detached { 0 } else { self.byte_length }
    }

    /// Get max byte length (if resizable)
    pub fn max_byte_length(&self) -> Option<usize> {
        self.max_byte_length
    }

    /// Check if resizable
    pub fn resizable(&self) -> bool {
        self.max_byte_length.is_some()
    }

    /// Check if detached
    pub fn detached(&self) -> bool {
        self.detached
    }

    /// Resize the buffer (ES2024)
    pub fn resize(&mut self, new_length: usize) -> Result<(), ArrayBufferError> {
        if self.detached {
            return Err(ArrayBufferError::TypeError("ArrayBuffer is detached".into()));
        }

        let max = self.max_byte_length.ok_or_else(|| {
            ArrayBufferError::TypeError("ArrayBuffer is not resizable".into())
        })?;

        if new_length > max {
            return Err(ArrayBufferError::RangeError(
                format!("newLength {} exceeds maxByteLength {}", new_length, max)
            ));
        }

        // Resize data
        if new_length > self.byte_length {
            // Growing: zero-fill new space
            self.data.resize(new_length, 0);
        } else {
            // Shrinking: truncate
            self.data.truncate(new_length);
        }

        self.byte_length = new_length;
        Ok(())
    }

    /// Transfer to a new ArrayBuffer (like structuredClone)
    pub fn transfer(&mut self, new_length: Option<usize>) -> Result<ResizableArrayBuffer, ArrayBufferError> {
        if self.detached {
            return Err(ArrayBufferError::TypeError("ArrayBuffer is detached".into()));
        }

        let new_len = new_length.unwrap_or(self.byte_length);
        let mut new_buffer = if let Some(max) = self.max_byte_length {
            ResizableArrayBuffer::new_resizable(new_len, max)?
        } else {
            ResizableArrayBuffer::new(new_len)
        };

        // Copy data
        let copy_len = new_len.min(self.byte_length);
        new_buffer.data[..copy_len].copy_from_slice(&self.data[..copy_len]);

        // Detach original
        self.detached = true;
        self.data.clear();
        self.byte_length = 0;

        Ok(new_buffer)
    }

    /// Slice the buffer
    pub fn slice(&self, start: isize, end: Option<isize>) -> Result<ResizableArrayBuffer, ArrayBufferError> {
        if self.detached {
            return Err(ArrayBufferError::TypeError("ArrayBuffer is detached".into()));
        }

        let len = self.byte_length as isize;
        
        // Handle negative indices
        let start = if start < 0 {
            (len + start).max(0) as usize
        } else {
            (start as usize).min(self.byte_length)
        };

        let end = match end {
            Some(e) if e < 0 => (len + e).max(0) as usize,
            Some(e) => (e as usize).min(self.byte_length),
            None => self.byte_length,
        };

        let new_len = if end > start { end - start } else { 0 };
        let mut new_buffer = ResizableArrayBuffer::new(new_len);
        
        if new_len > 0 {
            new_buffer.data.copy_from_slice(&self.data[start..end]);
        }

        Ok(new_buffer)
    }

    /// Get data slice
    pub fn data(&self) -> &[u8] {
        if self.detached { &[] } else { &self.data[..self.byte_length] }
    }

    /// Get mutable data slice
    pub fn data_mut(&mut self) -> &mut [u8] {
        if self.detached { &mut [] } else { &mut self.data[..self.byte_length] }
    }
}

/// ArrayBuffer error types
#[derive(Debug, Clone)]
pub enum ArrayBufferError {
    TypeError(String),
    RangeError(String),
}

// =============================================================================
// Array Grouping (Object.groupBy, Map.groupBy)
// =============================================================================

/// Result of Object.groupBy
pub type GroupByResult<K, V> = HashMap<K, Vec<V>>;

/// Group array elements by key function
/// Implements Object.groupBy semantics
pub fn object_group_by<T, K, F>(items: &[T], key_fn: F) -> GroupByResult<K, T>
where
    T: Clone,
    K: std::hash::Hash + Eq,
    F: Fn(&T) -> K,
{
    let mut result: GroupByResult<K, T> = HashMap::new();
    
    for item in items {
        let key = key_fn(item);
        result.entry(key).or_default().push(item.clone());
    }
    
    result
}

/// Group array elements by key function, returning iterator
/// More efficient for large arrays
pub fn group_by_iter<'a, T, K, F>(
    items: &'a [T],
    key_fn: F,
) -> impl Iterator<Item = (K, Vec<&'a T>)>
where
    K: std::hash::Hash + Eq,
    F: Fn(&T) -> K,
{
    let mut result: HashMap<K, Vec<&'a T>> = HashMap::new();
    
    for item in items {
        let key = key_fn(item);
        result.entry(key).or_default().push(item);
    }
    
    result.into_iter()
}

// =============================================================================
// Promise.withResolvers
// =============================================================================

/// Result of Promise.withResolvers()
/// Returns the promise along with its resolve/reject functions
#[derive(Debug)]
pub struct PromiseWithResolvers<T> {
    /// The promise ID
    pub promise_id: u32,
    /// Resolve function ID
    pub resolve_fn_id: u32,
    /// Reject function ID
    pub reject_fn_id: u32,
    /// Phantom data for type
    _phantom: std::marker::PhantomData<T>,
}

impl<T> PromiseWithResolvers<T> {
    pub fn new(promise_id: u32, resolve_fn_id: u32, reject_fn_id: u32) -> Self {
        Self {
            promise_id,
            resolve_fn_id,
            reject_fn_id,
            _phantom: std::marker::PhantomData,
        }
    }
}

/// Promise state for withResolvers tracking
#[derive(Debug)]
pub struct DeferredPromise {
    /// Promise ID
    pub id: u32,
    /// Whether resolved
    pub resolved: bool,
    /// Whether rejected
    pub rejected: bool,
    /// Resolved value (if any)
    pub value: Option<Box<dyn std::any::Any + Send>>,
    /// Rejection reason (if any)
    pub reason: Option<Box<dyn std::any::Any + Send>>,
}

impl DeferredPromise {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            resolved: false,
            rejected: false,
            value: None,
            reason: None,
        }
    }

    pub fn resolve<T: 'static + Send>(&mut self, value: T) {
        if !self.resolved && !self.rejected {
            self.resolved = true;
            self.value = Some(Box::new(value));
        }
    }

    pub fn reject<E: 'static + Send>(&mut self, reason: E) {
        if !self.resolved && !self.rejected {
            self.rejected = true;
            self.reason = Some(Box::new(reason));
        }
    }

    pub fn is_pending(&self) -> bool {
        !self.resolved && !self.rejected
    }
}

// =============================================================================
// Atomics.waitAsync
// =============================================================================

/// Result of Atomics.waitAsync
#[derive(Debug)]
pub enum WaitAsyncResult {
    /// Wait completed synchronously (value already changed)
    NotEqual,
    /// Wait timed out synchronously
    TimedOut,
    /// Wait is pending asynchronously
    Async {
        /// Promise that will resolve to "ok" or "timed-out"
        promise_id: u32,
    },
}

/// Async waiter state
#[derive(Debug)]
pub struct AsyncWaiter {
    /// Waiter ID
    pub id: u32,
    /// Buffer index being waited on
    pub buffer_index: usize,
    /// Expected value
    pub expected_value: i32,
    /// Timeout in milliseconds (None = infinite)
    pub timeout_ms: Option<u64>,
    /// Promise to resolve when done
    pub promise_id: u32,
    /// Start time
    pub start_time: std::time::Instant,
}

impl AsyncWaiter {
    pub fn new(
        id: u32,
        buffer_index: usize,
        expected_value: i32,
        timeout_ms: Option<u64>,
        promise_id: u32,
    ) -> Self {
        Self {
            id,
            buffer_index,
            expected_value,
            timeout_ms,
            promise_id,
            start_time: std::time::Instant::now(),
        }
    }

    /// Check if waiter has timed out
    pub fn is_timed_out(&self) -> bool {
        if let Some(timeout) = self.timeout_ms {
            self.start_time.elapsed().as_millis() as u64 >= timeout
        } else {
            false
        }
    }
}

/// Atomics wait/notify manager
#[derive(Debug, Default)]
pub struct AtomicsManager {
    /// Active async waiters
    waiters: HashMap<u32, AsyncWaiter>,
    /// Next waiter ID
    next_id: u32,
}

impl AtomicsManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add async waiter
    pub fn add_waiter(&mut self, waiter: AsyncWaiter) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.waiters.insert(id, waiter);
        id
    }

    /// Notify waiters on a buffer index
    pub fn notify(&mut self, buffer_index: usize, count: u32) -> u32 {
        let mut notified = 0u32;
        let mut to_remove = Vec::new();

        for (&id, waiter) in &self.waiters {
            if waiter.buffer_index == buffer_index {
                to_remove.push(id);
                notified += 1;
                if notified >= count {
                    break;
                }
            }
        }

        for id in to_remove {
            self.waiters.remove(&id);
        }

        notified
    }

    /// Check for timed out waiters
    pub fn check_timeouts(&mut self) -> Vec<u32> {
        let mut timed_out = Vec::new();

        for (&id, waiter) in &self.waiters {
            if waiter.is_timed_out() {
                timed_out.push(id);
            }
        }

        for &id in &timed_out {
            self.waiters.remove(&id);
        }

        timed_out
    }

    /// Get waiter count
    pub fn waiter_count(&self) -> usize {
        self.waiters.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resizable_arraybuffer_create() {
        let buf = ResizableArrayBuffer::new(1024);
        assert_eq!(buf.byte_length(), 1024);
        assert!(!buf.resizable());

        let resizable = ResizableArrayBuffer::new_resizable(512, 2048).unwrap();
        assert_eq!(resizable.byte_length(), 512);
        assert!(resizable.resizable());
        assert_eq!(resizable.max_byte_length(), Some(2048));
    }

    #[test]
    fn test_resizable_arraybuffer_resize() {
        let mut buf = ResizableArrayBuffer::new_resizable(100, 1000).unwrap();
        
        // Grow
        buf.resize(500).unwrap();
        assert_eq!(buf.byte_length(), 500);
        
        // Shrink
        buf.resize(200).unwrap();
        assert_eq!(buf.byte_length(), 200);
        
        // Exceed max fails
        assert!(buf.resize(2000).is_err());
    }

    #[test]
    fn test_resizable_arraybuffer_transfer() {
        let mut buf = ResizableArrayBuffer::new(100);
        buf.data_mut()[0] = 42;
        
        let new_buf = buf.transfer(None).unwrap();
        
        assert!(buf.detached());
        assert_eq!(buf.byte_length(), 0);
        assert_eq!(new_buf.byte_length(), 100);
        assert_eq!(new_buf.data()[0], 42);
    }

    #[test]
    fn test_resizable_arraybuffer_slice() {
        let mut buf = ResizableArrayBuffer::new(100);
        for i in 0..100 {
            buf.data_mut()[i] = i as u8;
        }
        
        let slice = buf.slice(10, Some(20)).unwrap();
        assert_eq!(slice.byte_length(), 10);
        assert_eq!(slice.data()[0], 10);
    }

    #[test]
    fn test_object_group_by() {
        let items = vec![
            ("apple", 1),
            ("banana", 2),
            ("apple", 3),
            ("cherry", 4),
        ];
        
        let grouped = object_group_by(&items, |(fruit, _)| *fruit);
        
        assert_eq!(grouped.get("apple").unwrap().len(), 2);
        assert_eq!(grouped.get("banana").unwrap().len(), 1);
        assert_eq!(grouped.get("cherry").unwrap().len(), 1);
    }

    #[test]
    fn test_deferred_promise() {
        let mut promise = DeferredPromise::new(0);
        
        assert!(promise.is_pending());
        
        promise.resolve(42);
        
        assert!(!promise.is_pending());
        assert!(promise.resolved);
        assert!(!promise.rejected);
    }

    #[test]
    fn test_atomics_manager() {
        let mut manager = AtomicsManager::new();
        
        let waiter = AsyncWaiter::new(0, 10, 5, Some(1000), 1);
        manager.add_waiter(waiter);
        
        assert_eq!(manager.waiter_count(), 1);
        
        let notified = manager.notify(10, 1);
        assert_eq!(notified, 1);
        assert_eq!(manager.waiter_count(), 0);
    }

    #[test]
    fn test_non_resizable_cannot_resize() {
        let mut buf = ResizableArrayBuffer::new(100);
        assert!(buf.resize(200).is_err());
    }
}
