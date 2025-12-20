//! SharedArrayBuffer and Atomics
//!
//! Shared memory for multi-threaded JavaScript.

use std::sync::{Arc, atomic::{AtomicI32, AtomicI64, Ordering}};

/// SharedArrayBuffer - shared memory between workers
#[derive(Debug, Clone)]
pub struct SharedArrayBuffer {
    data: Arc<Vec<AtomicI32>>,
    byte_length: usize,
}

impl SharedArrayBuffer {
    /// Create a new shared buffer
    pub fn new(byte_length: usize) -> Self {
        let len = (byte_length + 3) / 4; // Round up to i32 slots
        let data: Vec<AtomicI32> = (0..len).map(|_| AtomicI32::new(0)).collect();
        Self {
            data: Arc::new(data),
            byte_length,
        }
    }
    
    /// Get byte length
    pub fn byte_length(&self) -> usize {
        self.byte_length
    }
    
    /// Slice the buffer
    pub fn slice(&self, begin: usize, end: Option<usize>) -> Self {
        let end = end.unwrap_or(self.byte_length).min(self.byte_length);
        Self {
            data: self.data.clone(),
            byte_length: end.saturating_sub(begin),
        }
    }
}

/// Atomics operations
pub struct Atomics;

impl Atomics {
    /// Atomic add
    pub fn add(buffer: &SharedArrayBuffer, index: usize, value: i32) -> i32 {
        if index < buffer.data.len() {
            buffer.data[index].fetch_add(value, Ordering::SeqCst)
        } else {
            0
        }
    }
    
    /// Atomic subtract
    pub fn sub(buffer: &SharedArrayBuffer, index: usize, value: i32) -> i32 {
        if index < buffer.data.len() {
            buffer.data[index].fetch_sub(value, Ordering::SeqCst)
        } else {
            0
        }
    }
    
    /// Atomic AND
    pub fn and(buffer: &SharedArrayBuffer, index: usize, value: i32) -> i32 {
        if index < buffer.data.len() {
            buffer.data[index].fetch_and(value, Ordering::SeqCst)
        } else {
            0
        }
    }
    
    /// Atomic OR
    pub fn or(buffer: &SharedArrayBuffer, index: usize, value: i32) -> i32 {
        if index < buffer.data.len() {
            buffer.data[index].fetch_or(value, Ordering::SeqCst)
        } else {
            0
        }
    }
    
    /// Atomic XOR
    pub fn xor(buffer: &SharedArrayBuffer, index: usize, value: i32) -> i32 {
        if index < buffer.data.len() {
            buffer.data[index].fetch_xor(value, Ordering::SeqCst)
        } else {
            0
        }
    }
    
    /// Atomic load
    pub fn load(buffer: &SharedArrayBuffer, index: usize) -> i32 {
        if index < buffer.data.len() {
            buffer.data[index].load(Ordering::SeqCst)
        } else {
            0
        }
    }
    
    /// Atomic store
    pub fn store(buffer: &SharedArrayBuffer, index: usize, value: i32) -> i32 {
        if index < buffer.data.len() {
            buffer.data[index].store(value, Ordering::SeqCst);
            value
        } else {
            0
        }
    }
    
    /// Atomic exchange
    pub fn exchange(buffer: &SharedArrayBuffer, index: usize, value: i32) -> i32 {
        if index < buffer.data.len() {
            buffer.data[index].swap(value, Ordering::SeqCst)
        } else {
            0
        }
    }
    
    /// Compare and exchange
    pub fn compare_exchange(buffer: &SharedArrayBuffer, index: usize, expected: i32, replacement: i32) -> i32 {
        if index < buffer.data.len() {
            match buffer.data[index].compare_exchange(expected, replacement, Ordering::SeqCst, Ordering::SeqCst) {
                Ok(v) | Err(v) => v
            }
        } else {
            0
        }
    }
    
    /// Check if lock-free
    pub fn is_lock_free(size: usize) -> bool {
        matches!(size, 1 | 2 | 4 | 8)
    }
    
    /// Wait (simplified - would use OS primitives)
    pub fn wait(_buffer: &SharedArrayBuffer, _index: usize, _value: i32, _timeout: Option<u64>) -> WaitResult {
        // Would use futex or similar
        WaitResult::NotEqual
    }
    
    /// Notify waiting threads
    pub fn notify(_buffer: &SharedArrayBuffer, _index: usize, count: Option<u32>) -> u32 {
        // Would wake waiting threads
        count.unwrap_or(u32::MAX)
    }
}

/// Wait result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitResult {
    Ok,
    NotEqual,
    TimedOut,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_shared_array_buffer() {
        let sab = SharedArrayBuffer::new(16);
        assert_eq!(sab.byte_length(), 16);
    }
    
    #[test]
    fn test_atomics_add() {
        let sab = SharedArrayBuffer::new(16);
        
        Atomics::store(&sab, 0, 10);
        let old = Atomics::add(&sab, 0, 5);
        
        assert_eq!(old, 10);
        assert_eq!(Atomics::load(&sab, 0), 15);
    }
    
    #[test]
    fn test_atomics_cas() {
        let sab = SharedArrayBuffer::new(16);
        
        Atomics::store(&sab, 0, 42);
        
        // Should succeed
        let result = Atomics::compare_exchange(&sab, 0, 42, 100);
        assert_eq!(result, 42);
        assert_eq!(Atomics::load(&sab, 0), 100);
        
        // Should fail
        let result = Atomics::compare_exchange(&sab, 0, 42, 200);
        assert_eq!(result, 100); // Returns current value on failure
    }
}
