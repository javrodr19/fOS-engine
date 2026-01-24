//! Zero-Copy Networking
//!
//! Memory-mapped receive buffers and scatter-gather I/O for efficient data transfer.

use std::io;
use std::ptr::NonNull;

/// Zero-copy receive buffer using memory mapping
#[derive(Debug)]
pub struct MmapBuffer {
    ptr: NonNull<u8>,
    len: usize,
    capacity: usize,
}

unsafe impl Send for MmapBuffer {}
unsafe impl Sync for MmapBuffer {}

impl MmapBuffer {
    /// Create a new buffer with given capacity
    pub fn new(capacity: usize) -> io::Result<Self> {
        let layout = std::alloc::Layout::from_size_align(capacity, 4096)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
        let ptr = NonNull::new(ptr)
            .ok_or_else(|| io::Error::new(io::ErrorKind::OutOfMemory, "allocation failed"))?;
        Ok(Self { ptr, len: 0, capacity })
    }
    
    pub fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
    
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.capacity) }
    }
    
    pub fn set_len(&mut self, len: usize) { self.len = len.min(self.capacity); }
    pub fn len(&self) -> usize { self.len }
    pub fn is_empty(&self) -> bool { self.len == 0 }
    pub fn capacity(&self) -> usize { self.capacity }
    pub fn clear(&mut self) { self.len = 0; }
}

impl Drop for MmapBuffer {
    fn drop(&mut self) {
        let layout = std::alloc::Layout::from_size_align(self.capacity, 4096).unwrap();
        unsafe { std::alloc::dealloc(self.ptr.as_ptr(), layout); }
    }
}

/// Scatter-gather I/O vector
#[derive(Debug, Clone)]
pub struct IoVec { pub base: *const u8, pub len: usize }

unsafe impl Send for IoVec {}
unsafe impl Sync for IoVec {}

impl IoVec {
    pub fn new(data: &[u8]) -> Self { Self { base: data.as_ptr(), len: data.len() } }
    pub fn as_slice(&self) -> &[u8] { unsafe { std::slice::from_raw_parts(self.base, self.len) } }
}

/// Scatter-gather writer for efficient multi-buffer sends
#[derive(Debug, Default)]
pub struct ScatterGatherWriter {
    buffers: Vec<Vec<u8>>,
    total_len: usize,
}

impl ScatterGatherWriter {
    pub fn new() -> Self { Self::default() }
    
    pub fn push(&mut self, data: Vec<u8>) {
        self.total_len += data.len();
        self.buffers.push(data);
    }
    
    pub fn push_slice(&mut self, data: &[u8]) { self.push(data.to_vec()); }
    
    pub fn io_vecs(&self) -> Vec<IoVec> {
        self.buffers.iter().map(|b| IoVec::new(b)).collect()
    }
    
    pub fn total_len(&self) -> usize { self.total_len }
    pub fn buffer_count(&self) -> usize { self.buffers.len() }
    pub fn clear(&mut self) { self.buffers.clear(); self.total_len = 0; }
    
    pub fn into_contiguous(self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.total_len);
        for buf in self.buffers { out.extend(buf); }
        out
    }
}

/// Zero-copy pipeline for socket-to-decoder transfer
#[derive(Debug)]
pub struct ZeroCopyPipeline {
    recv_buffer: MmapBuffer,
    stats: PipelineStats,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PipelineStats {
    pub bytes_received: u64,
    pub bytes_sent: u64,
    pub zero_copy_transfers: u64,
    pub fallback_copies: u64,
}

impl ZeroCopyPipeline {
    pub fn new(buffer_size: usize) -> io::Result<Self> {
        Ok(Self { recv_buffer: MmapBuffer::new(buffer_size)?, stats: PipelineStats::default() })
    }
    
    pub fn receive_buffer(&mut self) -> &mut [u8] { self.recv_buffer.as_mut_slice() }
    
    pub fn commit_received(&mut self, len: usize) {
        self.recv_buffer.set_len(len);
        self.stats.bytes_received += len as u64;
        self.stats.zero_copy_transfers += 1;
    }
    
    pub fn data(&self) -> &[u8] { self.recv_buffer.as_slice() }
    pub fn consume(&mut self) { self.recv_buffer.clear(); }
    pub fn stats(&self) -> &PipelineStats { &self.stats }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_mmap_buffer() {
        let mut buf = MmapBuffer::new(4096).unwrap();
        buf.as_mut_slice()[..5].copy_from_slice(b"Hello");
        buf.set_len(5);
        assert_eq!(buf.as_slice(), b"Hello");
    }
    
    #[test]
    fn test_scatter_gather() {
        let mut sg = ScatterGatherWriter::new();
        sg.push_slice(b"Hello");
        sg.push_slice(b", World!");
        assert_eq!(sg.total_len(), 13);
        assert_eq!(sg.into_contiguous(), b"Hello, World!");
    }
}
