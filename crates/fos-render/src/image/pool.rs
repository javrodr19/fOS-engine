//! Image Memory Pool
//!
//! Reuses bitmap buffers to reduce memory allocations.

use std::collections::HashMap;

/// Bitmap memory pool for reusing image buffers
pub struct BitmapPool {
    /// Pools organized by size bucket
    pools: HashMap<SizeBucket, Vec<BitmapBuffer>>,
    /// Maximum buffers per bucket
    max_per_bucket: usize,
    /// Total allocated bytes
    total_bytes: usize,
    /// Maximum total bytes
    max_bytes: usize,
    /// Stats
    hits: u64,
    misses: u64,
}

/// Size bucket for pooling (rounded to power of 2)
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct SizeBucket {
    width_bucket: u32,
    height_bucket: u32,
}

impl SizeBucket {
    fn new(width: u32, height: u32) -> Self {
        Self {
            width_bucket: next_power_of_2(width.max(64)),
            height_bucket: next_power_of_2(height.max(64)),
        }
    }
}

fn next_power_of_2(n: u32) -> u32 {
    if n == 0 { return 1; }
    let mut v = n - 1;
    v |= v >> 1;
    v |= v >> 2;
    v |= v >> 4;
    v |= v >> 8;
    v |= v >> 16;
    v + 1
}

/// A reusable bitmap buffer
pub struct BitmapBuffer {
    /// RGBA pixel data
    pub data: Vec<u8>,
    /// Actual width
    pub width: u32,
    /// Actual height
    pub height: u32,
    /// Allocated width (may be larger)
    allocated_width: u32,
    /// Allocated height (may be larger)
    allocated_height: u32,
}

impl BitmapBuffer {
    /// Create a new bitmap buffer
    pub fn new(width: u32, height: u32) -> Self {
        let bucket = SizeBucket::new(width, height);
        let alloc_w = bucket.width_bucket;
        let alloc_h = bucket.height_bucket;
        
        Self {
            data: vec![0; (alloc_w * alloc_h * 4) as usize],
            width,
            height,
            allocated_width: alloc_w,
            allocated_height: alloc_h,
        }
    }
    
    /// Resize the buffer (reuses memory if possible)
    pub fn resize(&mut self, width: u32, height: u32) {
        let need_w = next_power_of_2(width.max(64));
        let need_h = next_power_of_2(height.max(64));
        
        if need_w > self.allocated_width || need_h > self.allocated_height {
            self.allocated_width = need_w;
            self.allocated_height = need_h;
            self.data.resize((need_w * need_h * 4) as usize, 0);
        }
        
        self.width = width;
        self.height = height;
    }
    
    /// Clear the buffer
    pub fn clear(&mut self) {
        self.data.fill(0);
    }
    
    /// Get pixel at (x, y)
    pub fn get_pixel(&self, x: u32, y: u32) -> Option<(u8, u8, u8, u8)> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let idx = ((y * self.allocated_width + x) * 4) as usize;
        Some((
            self.data[idx],
            self.data[idx + 1],
            self.data[idx + 2],
            self.data[idx + 3],
        ))
    }
    
    /// Set pixel at (x, y)
    pub fn set_pixel(&mut self, x: u32, y: u32, r: u8, g: u8, b: u8, a: u8) {
        if x < self.width && y < self.height {
            let idx = ((y * self.allocated_width + x) * 4) as usize;
            self.data[idx] = r;
            self.data[idx + 1] = g;
            self.data[idx + 2] = b;
            self.data[idx + 3] = a;
        }
    }
    
    /// Get allocated size in bytes
    pub fn allocated_bytes(&self) -> usize {
        self.data.len()
    }
}

impl Default for BitmapPool {
    fn default() -> Self {
        Self::new(64, 128 * 1024 * 1024) // 128 MB default limit
    }
}

impl BitmapPool {
    /// Create a new bitmap pool
    pub fn new(max_per_bucket: usize, max_bytes: usize) -> Self {
        Self {
            pools: HashMap::new(),
            max_per_bucket,
            total_bytes: 0,
            max_bytes,
            hits: 0,
            misses: 0,
        }
    }
    
    /// Checkout a bitmap buffer (may come from pool or be newly allocated)
    pub fn checkout(&mut self, width: u32, height: u32) -> BitmapBuffer {
        let bucket = SizeBucket::new(width, height);
        
        if let Some(pool) = self.pools.get_mut(&bucket) {
            if let Some(mut buffer) = pool.pop() {
                self.hits += 1;
                self.total_bytes -= buffer.allocated_bytes();
                buffer.resize(width, height);
                buffer.clear();
                return buffer;
            }
        }
        
        self.misses += 1;
        BitmapBuffer::new(width, height)
    }
    
    /// Return a bitmap buffer to the pool
    pub fn checkin(&mut self, buffer: BitmapBuffer) {
        let bucket = SizeBucket::new(buffer.allocated_width, buffer.allocated_height);
        let bytes = buffer.allocated_bytes();
        
        // Check capacity limits
        if self.total_bytes + bytes > self.max_bytes {
            return; // Drop the buffer
        }
        
        let pool = self.pools.entry(bucket).or_insert_with(Vec::new);
        if pool.len() >= self.max_per_bucket {
            return; // Drop the buffer
        }
        
        self.total_bytes += bytes;
        pool.push(buffer);
    }
    
    /// Get pool statistics
    pub fn stats(&self) -> PoolStats {
        PoolStats {
            total_bytes: self.total_bytes,
            max_bytes: self.max_bytes,
            hits: self.hits,
            misses: self.misses,
            hit_rate: if self.hits + self.misses > 0 {
                self.hits as f64 / (self.hits + self.misses) as f64
            } else {
                0.0
            },
            num_buckets: self.pools.len(),
            num_buffers: self.pools.values().map(|p| p.len()).sum(),
        }
    }
    
    /// Clear all pooled buffers
    pub fn clear(&mut self) {
        self.pools.clear();
        self.total_bytes = 0;
    }
}

/// Pool statistics
#[derive(Debug, Clone)]
pub struct PoolStats {
    pub total_bytes: usize,
    pub max_bytes: usize,
    pub hits: u64,
    pub misses: u64,
    pub hit_rate: f64,
    pub num_buckets: usize,
    pub num_buffers: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_bitmap_buffer() {
        let mut buf = BitmapBuffer::new(100, 100);
        buf.set_pixel(50, 50, 255, 0, 0, 255);
        
        let pixel = buf.get_pixel(50, 50);
        assert_eq!(pixel, Some((255, 0, 0, 255)));
    }
    
    #[test]
    fn test_pool_checkout_checkin() {
        let mut pool = BitmapPool::new(10, 1024 * 1024);
        
        let buf1 = pool.checkout(100, 100);
        assert_eq!(pool.stats().misses, 1);
        
        pool.checkin(buf1);
        assert!(pool.stats().num_buffers >= 1);
        
        let _buf2 = pool.checkout(100, 100);
        assert_eq!(pool.stats().hits, 1);
    }
    
    #[test]
    fn test_power_of_2() {
        assert_eq!(next_power_of_2(1), 1);
        assert_eq!(next_power_of_2(3), 4);
        assert_eq!(next_power_of_2(64), 64);
        assert_eq!(next_power_of_2(100), 128);
    }
}
