//! Roaring Bitmaps (Phase 24.3)
//!
//! Compressed bitmap sets for node IDs with O(1) set operations.
//! Fast intersection (visible ∩ dirty), chunk-based encoding.
//! Used by Lucene, Netflix, Google for efficient set operations.

use std::collections::HashMap;

/// Chunk size for roaring bitmap (16-bit chunks = 65536 values each)
const CHUNK_SIZE: u32 = 65536;
const CHUNK_BITS: u32 = 16;

/// Container type for a chunk
#[derive(Debug, Clone)]
enum Container {
    /// Array container (for sparse chunks)
    Array(Vec<u16>),
    /// Bitmap container (for dense chunks)
    Bitmap(Box<[u64; 1024]>), // 65536 bits = 1024 * 64
    /// Run container (for runs of consecutive values)
    Run(Vec<(u16, u16)>), // (start, length) pairs
}

impl Container {
    /// Maximum size for array container before converting to bitmap
    const ARRAY_MAX: usize = 4096;
    
    /// Create a new empty array container
    fn new_array() -> Self {
        Container::Array(Vec::new())
    }
    
    /// Create a new bitmap container
    fn new_bitmap() -> Self {
        Container::Bitmap(Box::new([0u64; 1024]))
    }
    
    /// Check if contains a value
    fn contains(&self, value: u16) -> bool {
        match self {
            Container::Array(arr) => arr.binary_search(&value).is_ok(),
            Container::Bitmap(bits) => {
                let idx = value as usize / 64;
                let bit = value as u64 % 64;
                (bits[idx] >> bit) & 1 == 1
            }
            Container::Run(runs) => {
                runs.iter().any(|&(start, len)| {
                    value >= start && value <= start.saturating_add(len)
                })
            }
        }
    }
    
    /// Insert a value
    fn insert(&mut self, value: u16) -> bool {
        match self {
            Container::Array(arr) => {
                match arr.binary_search(&value) {
                    Ok(_) => false, // Already present
                    Err(idx) => {
                        arr.insert(idx, value);
                        true
                    }
                }
            }
            Container::Bitmap(bits) => {
                let idx = value as usize / 64;
                let bit = value as u64 % 64;
                let was_set = (bits[idx] >> bit) & 1 == 1;
                bits[idx] |= 1 << bit;
                !was_set
            }
            Container::Run(runs) => {
                // Simplified: convert to array first
                let mut arr: Vec<u16> = runs.iter()
                    .flat_map(|&(start, len)| (start..=start.saturating_add(len)))
                    .collect();
                match arr.binary_search(&value) {
                    Ok(_) => false,
                    Err(idx) => {
                        arr.insert(idx, value);
                        *self = Container::Array(arr);
                        true
                    }
                }
            }
        }
    }
    
    /// Remove a value
    fn remove(&mut self, value: u16) -> bool {
        match self {
            Container::Array(arr) => {
                if let Ok(idx) = arr.binary_search(&value) {
                    arr.remove(idx);
                    true
                } else {
                    false
                }
            }
            Container::Bitmap(bits) => {
                let idx = value as usize / 64;
                let bit = value as u64 % 64;
                let was_set = (bits[idx] >> bit) & 1 == 1;
                bits[idx] &= !(1 << bit);
                was_set
            }
            Container::Run(runs) => {
                // Simplified implementation
                let mut found = false;
                let mut new_runs = Vec::new();
                for &(start, len) in runs.iter() {
                    if value >= start && value <= start.saturating_add(len) {
                        found = true;
                        if value == start {
                            if len > 0 {
                                new_runs.push((start + 1, len - 1));
                            }
                        } else if value == start.saturating_add(len) {
                            new_runs.push((start, len - 1));
                        } else {
                            // Split the run
                            new_runs.push((start, value - start - 1));
                            new_runs.push((value + 1, start.saturating_add(len) - value - 1));
                        }
                    } else {
                        new_runs.push((start, len));
                    }
                }
                *runs = new_runs;
                found
            }
        }
    }
    
    /// Count of values in this container
    fn cardinality(&self) -> u32 {
        match self {
            Container::Array(arr) => arr.len() as u32,
            Container::Bitmap(bits) => bits.iter().map(|&w| w.count_ones()).sum(),
            Container::Run(runs) => runs.iter().map(|&(_, len)| len as u32 + 1).sum(),
        }
    }
    
    /// Convert to bitmap if array is too large
    fn optimize(&mut self) {
        if let Container::Array(arr) = self {
            if arr.len() > Self::ARRAY_MAX {
                let mut bitmap = Box::new([0u64; 1024]);
                for &value in arr.iter() {
                    let idx = value as usize / 64;
                    let bit = value as u64 % 64;
                    bitmap[idx] |= 1 << bit;
                }
                *self = Container::Bitmap(bitmap);
            }
        }
    }
    
    /// Estimate memory usage
    fn memory_size(&self) -> usize {
        match self {
            Container::Array(arr) => std::mem::size_of::<Vec<u16>>() + arr.len() * 2,
            Container::Bitmap(_) => std::mem::size_of::<Box<[u64; 1024]>>() + 1024 * 8,
            Container::Run(runs) => std::mem::size_of::<Vec<(u16, u16)>>() + runs.len() * 4,
        }
    }
}

/// Roaring bitmap - compressed bitmap set
#[derive(Debug, Clone, Default)]
pub struct RoaringBitmap {
    /// Containers indexed by high 16 bits
    containers: HashMap<u16, Container>,
}

impl RoaringBitmap {
    /// Create a new empty bitmap
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Split value into high and low parts
    #[inline]
    fn split(value: u32) -> (u16, u16) {
        ((value >> CHUNK_BITS) as u16, value as u16)
    }
    
    /// Combine high and low parts
    #[inline]
    fn combine(high: u16, low: u16) -> u32 {
        ((high as u32) << CHUNK_BITS) | (low as u32)
    }
    
    /// Check if contains a value
    pub fn contains(&self, value: u32) -> bool {
        let (high, low) = Self::split(value);
        self.containers.get(&high).map(|c| c.contains(low)).unwrap_or(false)
    }
    
    /// Insert a value
    pub fn insert(&mut self, value: u32) -> bool {
        let (high, low) = Self::split(value);
        let container = self.containers.entry(high).or_insert_with(Container::new_array);
        let inserted = container.insert(low);
        container.optimize();
        inserted
    }
    
    /// Remove a value
    pub fn remove(&mut self, value: u32) -> bool {
        let (high, low) = Self::split(value);
        if let Some(container) = self.containers.get_mut(&high) {
            let removed = container.remove(low);
            if container.cardinality() == 0 {
                self.containers.remove(&high);
            }
            removed
        } else {
            false
        }
    }
    
    /// Cardinality (count of values)
    pub fn cardinality(&self) -> u64 {
        self.containers.values().map(|c| c.cardinality() as u64).sum()
    }
    
    /// Is empty
    pub fn is_empty(&self) -> bool {
        self.containers.is_empty()
    }
    
    /// Clear all values
    pub fn clear(&mut self) {
        self.containers.clear();
    }
    
    /// Union with another bitmap (in-place)
    pub fn union_with(&mut self, other: &RoaringBitmap) {
        for (&high, container) in &other.containers {
            match container {
                Container::Array(arr) => {
                    for &low in arr {
                        let value = Self::combine(high, low);
                        self.insert(value);
                    }
                }
                Container::Bitmap(bits) => {
                    for idx in 0..1024 {
                        if bits[idx] != 0 {
                            for bit in 0..64 {
                                if (bits[idx] >> bit) & 1 == 1 {
                                    let low = (idx * 64 + bit) as u16;
                                    let value = Self::combine(high, low);
                                    self.insert(value);
                                }
                            }
                        }
                    }
                }
                Container::Run(runs) => {
                    for &(start, len) in runs {
                        for low in start..=start.saturating_add(len) {
                            let value = Self::combine(high, low);
                            self.insert(value);
                        }
                    }
                }
            }
        }
    }
    
    /// Intersection with another bitmap (returns new)
    pub fn intersection(&self, other: &RoaringBitmap) -> RoaringBitmap {
        let mut result = RoaringBitmap::new();
        
        for (&high, container) in &self.containers {
            if let Some(other_container) = other.containers.get(&high) {
                // Both have this chunk - intersect
                match container {
                    Container::Array(arr) => {
                        for &low in arr {
                            if other_container.contains(low) {
                                result.insert(Self::combine(high, low));
                            }
                        }
                    }
                    Container::Bitmap(bits) => {
                        for idx in 0..1024 {
                            if bits[idx] != 0 {
                                for bit in 0..64 {
                                    if (bits[idx] >> bit) & 1 == 1 {
                                        let low = (idx * 64 + bit) as u16;
                                        if other_container.contains(low) {
                                            result.insert(Self::combine(high, low));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Container::Run(runs) => {
                        for &(start, len) in runs {
                            for low in start..=start.saturating_add(len) {
                                if other_container.contains(low) {
                                    result.insert(Self::combine(high, low));
                                }
                            }
                        }
                    }
                }
            }
        }
        
        result
    }
    
    /// Difference (self - other)
    pub fn difference(&self, other: &RoaringBitmap) -> RoaringBitmap {
        let mut result = self.clone();
        for (&high, container) in &other.containers {
            match container {
                Container::Array(arr) => {
                    for &low in arr {
                        result.remove(Self::combine(high, low));
                    }
                }
                Container::Bitmap(bits) => {
                    for idx in 0..1024 {
                        if bits[idx] != 0 {
                            for bit in 0..64 {
                                if (bits[idx] >> bit) & 1 == 1 {
                                    let low = (idx * 64 + bit) as u16;
                                    result.remove(Self::combine(high, low));
                                }
                            }
                        }
                    }
                }
                Container::Run(runs) => {
                    for &(start, len) in runs {
                        for low in start..=start.saturating_add(len) {
                            result.remove(Self::combine(high, low));
                        }
                    }
                }
            }
        }
        result
    }
    
    /// Estimate memory usage
    pub fn memory_size(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.containers.values().map(|c| c.memory_size() + 2).sum::<usize>()
    }
    
    /// Iterate over all values
    pub fn iter(&self) -> impl Iterator<Item = u32> + '_ {
        RoaringIterator {
            bitmap: self,
            chunks: self.containers.keys().copied().collect::<Vec<_>>().into_iter(),
            current_chunk: None,
            current_iter: None,
        }
    }
}

struct RoaringIterator<'a> {
    bitmap: &'a RoaringBitmap,
    chunks: std::vec::IntoIter<u16>,
    current_chunk: Option<u16>,
    current_iter: Option<Box<dyn Iterator<Item = u16> + 'a>>,
}

impl<'a> Iterator for RoaringIterator<'a> {
    type Item = u32;
    
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(ref mut iter) = self.current_iter {
                if let Some(low) = iter.next() {
                    return Some(RoaringBitmap::combine(self.current_chunk.unwrap(), low));
                }
            }
            
            // Move to next chunk
            self.current_chunk = self.chunks.next();
            if let Some(high) = self.current_chunk {
                if let Some(container) = self.bitmap.containers.get(&high) {
                    match container {
                        Container::Array(arr) => {
                            self.current_iter = Some(Box::new(arr.iter().copied()));
                        }
                        Container::Bitmap(bits) => {
                            let values: Vec<u16> = (0..65536u32)
                                .filter(|&i| {
                                    let idx = i as usize / 64;
                                    let bit = i as u64 % 64;
                                    (bits[idx] >> bit) & 1 == 1
                                })
                                .map(|i| i as u16)
                                .collect();
                            self.current_iter = Some(Box::new(values.into_iter()));
                        }
                        Container::Run(runs) => {
                            let values: Vec<u16> = runs.iter()
                                .flat_map(|&(start, len)| start..=start.saturating_add(len))
                                .collect();
                            self.current_iter = Some(Box::new(values.into_iter()));
                        }
                    }
                }
            } else {
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_basic_operations() {
        let mut bitmap = RoaringBitmap::new();
        
        assert!(bitmap.is_empty());
        assert!(!bitmap.contains(42));
        
        bitmap.insert(42);
        assert!(bitmap.contains(42));
        assert_eq!(bitmap.cardinality(), 1);
        
        bitmap.remove(42);
        assert!(!bitmap.contains(42));
        assert!(bitmap.is_empty());
    }
    
    #[test]
    fn test_large_values() {
        let mut bitmap = RoaringBitmap::new();
        
        bitmap.insert(0);
        bitmap.insert(65535);
        bitmap.insert(65536);
        bitmap.insert(1_000_000);
        
        assert!(bitmap.contains(0));
        assert!(bitmap.contains(65535));
        assert!(bitmap.contains(65536));
        assert!(bitmap.contains(1_000_000));
        assert!(!bitmap.contains(65537));
        
        assert_eq!(bitmap.cardinality(), 4);
    }
    
    #[test]
    fn test_intersection() {
        let mut a = RoaringBitmap::new();
        let mut b = RoaringBitmap::new();
        
        for i in 0..100 {
            a.insert(i);
        }
        for i in 50..150 {
            b.insert(i);
        }
        
        let c = a.intersection(&b);
        
        // Intersection should be 50..100
        assert_eq!(c.cardinality(), 50);
        for i in 50..100 {
            assert!(c.contains(i));
        }
    }
    
    #[test]
    fn test_union() {
        let mut a = RoaringBitmap::new();
        let mut b = RoaringBitmap::new();
        
        a.insert(1);
        a.insert(3);
        b.insert(2);
        b.insert(3);
        
        a.union_with(&b);
        
        assert!(a.contains(1));
        assert!(a.contains(2));
        assert!(a.contains(3));
        assert_eq!(a.cardinality(), 3);
    }
    
    #[test]
    fn test_iteration() {
        let mut bitmap = RoaringBitmap::new();
        
        bitmap.insert(1);
        bitmap.insert(100);
        bitmap.insert(1000);
        
        let values: Vec<u32> = bitmap.iter().collect();
        assert_eq!(values.len(), 3);
        assert!(values.contains(&1));
        assert!(values.contains(&100));
        assert!(values.contains(&1000));
    }
    
    #[test]
    fn test_memory_efficiency() {
        let mut bitmap = RoaringBitmap::new();
        
        // Sparse data - should use array containers
        for i in (0..10000).step_by(100) {
            bitmap.insert(i);
        }
        
        // Memory should be much less than a full bitmap
        assert!(bitmap.memory_size() < 10000); // Well under 10KB
    }
}
