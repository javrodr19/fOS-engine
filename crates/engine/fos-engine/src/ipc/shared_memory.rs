//! Shared Memory
//!
//! Zero-copy shared memory regions for large data transfer between processes.

use std::collections::HashMap;
use std::io;
use std::sync::atomic::{AtomicU32, Ordering};

#[cfg(unix)]
use std::os::unix::io::RawFd;

/// Shared memory handle ID counter
static NEXT_HANDLE_ID: AtomicU32 = AtomicU32::new(1);

fn next_handle_id() -> u32 {
    NEXT_HANDLE_ID.fetch_add(1, Ordering::SeqCst)
}

/// Shared memory handle
/// Uses Vec<u8> as backing storage (for simplicity and safety)
/// In production, would use mmap for true zero-copy
#[derive(Debug)]
pub struct SharedMemHandle {
    /// Unique handle ID
    id: u32,
    /// Memory region (heap-allocated for safety)
    data: Vec<u8>,
    /// Path/name of shared memory
    name: String,
    #[cfg(unix)]
    /// File descriptor (for future shm_open support)
    fd: Option<RawFd>,
}

impl SharedMemHandle {
    /// Create a new shared memory region
    pub fn create(name: &str, size: usize) -> io::Result<Self> {
        Ok(Self {
            id: next_handle_id(),
            data: vec![0u8; size],
            name: name.to_string(),
            #[cfg(unix)]
            fd: None,
        })
    }
    
    /// Create from existing file descriptor (placeholder for future)
    #[cfg(unix)]
    pub fn from_fd(name: &str, _fd: RawFd, size: usize) -> io::Result<Self> {
        // For now, just create a regular region
        // Real implementation would mmap the fd
        Self::create(name, size)
    }
    
    /// Get handle ID
    pub fn id(&self) -> u32 {
        self.id
    }
    
    /// Get region name
    pub fn name(&self) -> &str {
        &self.name
    }
    
    /// Get region size
    pub fn size(&self) -> usize {
        self.data.len()
    }
    
    /// Get pointer to memory
    pub fn as_ptr(&self) -> *const u8 {
        self.data.as_ptr()
    }
    
    /// Get mutable pointer to memory
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.data.as_mut_ptr()
    }
    
    /// Get slice view
    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }
    
    /// Get mutable slice view
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.data
    }
    
    /// Write data at offset
    pub fn write(&mut self, offset: usize, data: &[u8]) -> io::Result<()> {
        if offset + data.len() > self.data.len() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Write beyond region bounds"));
        }
        
        self.data[offset..offset + data.len()].copy_from_slice(data);
        Ok(())
    }
    
    /// Read data at offset
    pub fn read(&self, offset: usize, len: usize) -> io::Result<&[u8]> {
        if offset + len > self.data.len() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Read beyond region bounds"));
        }
        
        Ok(&self.data[offset..offset + len])
    }
}

// SharedMemHandle is Send + Sync
unsafe impl Send for SharedMemHandle {}
unsafe impl Sync for SharedMemHandle {}

/// Shared memory pool for managing multiple regions
#[derive(Debug, Default)]
pub struct SharedMemPool {
    /// Active handles
    handles: HashMap<u32, SharedMemHandle>,
    /// Total allocated size
    total_size: usize,
    /// Maximum total size
    max_size: usize,
}

impl SharedMemPool {
    /// Create new pool
    pub fn new(max_size: usize) -> Self {
        Self {
            handles: HashMap::new(),
            total_size: 0,
            max_size,
        }
    }
    
    /// Create new shared memory region
    pub fn create(&mut self, name: &str, size: usize) -> io::Result<u32> {
        if self.total_size + size > self.max_size {
            return Err(io::Error::new(io::ErrorKind::OutOfMemory, "Shared memory pool exhausted"));
        }
        
        let handle = SharedMemHandle::create(name, size)?;
        let id = handle.id();
        self.total_size += size;
        self.handles.insert(id, handle);
        
        Ok(id)
    }
    
    /// Get handle by ID
    pub fn get(&self, id: u32) -> Option<&SharedMemHandle> {
        self.handles.get(&id)
    }
    
    /// Get mutable handle by ID
    pub fn get_mut(&mut self, id: u32) -> Option<&mut SharedMemHandle> {
        self.handles.get_mut(&id)
    }
    
    /// Free a shared memory region
    pub fn free(&mut self, id: u32) -> bool {
        if let Some(handle) = self.handles.remove(&id) {
            self.total_size = self.total_size.saturating_sub(handle.size());
            true
        } else {
            false
        }
    }
    
    /// Get total allocated size
    pub fn total_size(&self) -> usize {
        self.total_size
    }
    
    /// Get remaining capacity
    pub fn remaining(&self) -> usize {
        self.max_size.saturating_sub(self.total_size)
    }
    
    /// Get handle count
    pub fn handle_count(&self) -> usize {
        self.handles.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[cfg(unix)]
    #[test]
    fn test_shared_mem_create() {
        let mut handle = SharedMemHandle::create("test-region", 4096).unwrap();
        
        assert_eq!(handle.size(), 4096);
        assert!(!handle.name().is_empty());
        
        // Write and read
        handle.write(0, b"Hello, shared memory!").unwrap();
        let data = handle.read(0, 21).unwrap();
        assert_eq!(data, b"Hello, shared memory!");
    }
    
    #[cfg(unix)]
    #[test]
    fn test_shared_mem_bounds() {
        let handle = SharedMemHandle::create("test-bounds", 100).unwrap();
        
        // Read beyond bounds
        assert!(handle.read(90, 20).is_err());
    }
    
    #[cfg(unix)]
    #[test]
    fn test_pool() {
        let mut pool = SharedMemPool::new(1024 * 1024); // 1MB max
        
        let id1 = pool.create("region1", 4096).unwrap();
        let id2 = pool.create("region2", 8192).unwrap();
        
        assert_eq!(pool.handle_count(), 2);
        assert_eq!(pool.total_size(), 4096 + 8192);
        
        pool.free(id1);
        assert_eq!(pool.handle_count(), 1);
        assert_eq!(pool.total_size(), 8192);
    }
}
