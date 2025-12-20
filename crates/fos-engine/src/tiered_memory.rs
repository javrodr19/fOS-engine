//! Tiered Memory (Phase 24.2)
//!
//! Hot/Warm/Cold memory tiers for DOM content:
//! - Hot: Current viewport (fastest access)
//! - Warm: ±2 screens (in RAM, maybe compressed)
//! - Cold: Rest of document (on disk)
//! Automatic migration based on scroll position.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Memory tier
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Tier {
    /// Current viewport - uncompressed, fastest access
    Hot = 0,
    /// ±2 screens - in RAM, possibly compressed
    Warm = 1,
    /// Rest of document - on disk or heavily compressed
    Cold = 2,
}

/// Node ID type
pub type NodeId = u32;

/// Data stored for each node
#[derive(Debug, Clone)]
pub struct TieredData {
    /// The actual data bytes
    data: Vec<u8>,
    /// Whether the data is compressed
    compressed: bool,
}

impl TieredData {
    pub fn new(data: Vec<u8>) -> Self {
        Self {
            data,
            compressed: false,
        }
    }
    
    pub fn compressed(data: Vec<u8>) -> Self {
        Self {
            data,
            compressed: true,
        }
    }
    
    /// Get uncompressed data
    pub fn get(&self) -> Vec<u8> {
        if self.compressed {
            self.decompress()
        } else {
            self.data.clone()
        }
    }
    
    /// Get reference to raw data (may be compressed)
    pub fn raw(&self) -> &[u8] {
        &self.data
    }
    
    /// Compress the data (simple RLE for demonstration)
    pub fn compress(&mut self) {
        if self.compressed {
            return;
        }
        self.data = simple_compress(&self.data);
        self.compressed = true;
    }
    
    /// Decompress the data
    fn decompress(&self) -> Vec<u8> {
        if !self.compressed {
            return self.data.clone();
        }
        simple_decompress(&self.data)
    }
    
    /// Memory size
    pub fn memory_size(&self) -> usize {
        std::mem::size_of::<Self>() + self.data.len()
    }
}

/// Simple RLE compression (for demonstration)
fn simple_compress(data: &[u8]) -> Vec<u8> {
    if data.is_empty() {
        return Vec::new();
    }
    
    let mut result = Vec::new();
    let mut current = data[0];
    let mut count: u8 = 1;
    
    for &byte in &data[1..] {
        if byte == current && count < 255 {
            count += 1;
        } else {
            result.push(count);
            result.push(current);
            current = byte;
            count = 1;
        }
    }
    result.push(count);
    result.push(current);
    
    // Only use compressed if smaller
    if result.len() < data.len() {
        result
    } else {
        data.to_vec()
    }
}

fn simple_decompress(data: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();
    let mut i = 0;
    while i + 1 < data.len() {
        let count = data[i];
        let byte = data[i + 1];
        for _ in 0..count {
            result.push(byte);
        }
        i += 2;
    }
    result
}

/// Position of a node for tiering
#[derive(Debug, Clone, Copy)]
pub struct NodePosition {
    /// Y coordinate (for vertical scrolling)
    pub y: f32,
    /// Height of the node
    pub height: f32,
}

impl NodePosition {
    pub fn new(y: f32, height: f32) -> Self {
        Self { y, height }
    }
    
    /// Bottom edge
    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }
}

/// Viewport for tier calculation
#[derive(Debug, Clone, Copy)]
pub struct TierViewport {
    /// Top of viewport
    pub y: f32,
    /// Height of viewport
    pub height: f32,
    /// Distance considered "warm" above/below viewport
    pub warm_distance: f32,
}

impl TierViewport {
    pub fn new(y: f32, height: f32) -> Self {
        Self {
            y,
            height,
            warm_distance: height * 2.0, // ±2 screens
        }
    }
    
    /// Calculate tier for a node position
    pub fn tier_for(&self, pos: &NodePosition) -> Tier {
        let vp_top = self.y;
        let vp_bottom = self.y + self.height;
        let warm_top = vp_top - self.warm_distance;
        let warm_bottom = vp_bottom + self.warm_distance;
        
        // Check if overlaps with viewport
        if pos.bottom() >= vp_top && pos.y <= vp_bottom {
            return Tier::Hot;
        }
        
        // Check if in warm zone
        if pos.bottom() >= warm_top && pos.y <= warm_bottom {
            return Tier::Warm;
        }
        
        Tier::Cold
    }
}

/// Entry in the tiered storage
#[derive(Debug)]
struct TieredEntry {
    /// Current tier
    tier: Tier,
    /// Data (if in memory)
    data: Option<TieredData>,
    /// Position in cold storage (if on disk)
    cold_offset: Option<(u64, usize)>, // (offset, size)
    /// Last access time
    last_access: Instant,
    /// Position for tier calculation
    position: NodePosition,
}

/// Tiered memory manager
pub struct TieredMemory {
    /// Entries indexed by node ID
    entries: HashMap<NodeId, TieredEntry>,
    /// Current viewport
    viewport: TierViewport,
    /// Path for cold storage
    cold_storage_path: PathBuf,
    /// Cold storage file
    cold_storage: Option<std::fs::File>,
    /// Next offset in cold storage
    cold_offset: u64,
    /// Stats
    stats: TieredStats,
}

/// Statistics for tiered memory
#[derive(Debug, Clone, Copy, Default)]
pub struct TieredStats {
    pub hot_count: usize,
    pub warm_count: usize,
    pub cold_count: usize,
    pub hot_bytes: usize,
    pub warm_bytes: usize,
    pub cold_bytes: usize,
    pub promotions: u64,
    pub demotions: u64,
    pub disk_reads: u64,
    pub disk_writes: u64,
}

impl TieredStats {
    pub fn total_memory(&self) -> usize {
        self.hot_bytes + self.warm_bytes
    }
}

impl TieredMemory {
    /// Create a new tiered memory manager
    pub fn new(cold_storage_path: PathBuf) -> std::io::Result<Self> {
        let cold_storage = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&cold_storage_path)?;
        
        Ok(Self {
            entries: HashMap::new(),
            viewport: TierViewport::new(0.0, 1000.0),
            cold_storage_path,
            cold_storage: Some(cold_storage),
            cold_offset: 0,
            stats: TieredStats::default(),
        })
    }
    
    /// Update viewport position (triggers tier migration)
    pub fn update_viewport(&mut self, y: f32, height: f32) -> std::io::Result<()> {
        self.viewport = TierViewport::new(y, height);
        self.migrate_tiers()
    }
    
    /// Insert data for a node
    pub fn insert(&mut self, id: NodeId, data: Vec<u8>, position: NodePosition) {
        let tier = self.viewport.tier_for(&position);
        let tiered_data = TieredData::new(data);
        
        match tier {
            Tier::Hot => self.stats.hot_bytes += tiered_data.memory_size(),
            Tier::Warm => self.stats.warm_bytes += tiered_data.memory_size(),
            Tier::Cold => self.stats.cold_bytes += tiered_data.memory_size(),
        }
        
        let entry = TieredEntry {
            tier,
            data: Some(tiered_data),
            cold_offset: None,
            last_access: Instant::now(),
            position,
        };
        
        self.entries.insert(id, entry);
        self.update_tier_counts();
    }
    
    /// Get data for a node (may promote from cold)
    pub fn get(&mut self, id: NodeId) -> std::io::Result<Option<Vec<u8>>> {
        let entry = match self.entries.get_mut(&id) {
            Some(e) => e,
            None => return Ok(None),
        };
        
        entry.last_access = Instant::now();
        
        // If in memory, return it
        if let Some(ref data) = entry.data {
            return Ok(Some(data.get()));
        }
        
        // Read from cold storage
        if let Some((offset, size)) = entry.cold_offset {
            if let Some(ref mut file) = self.cold_storage {
                use std::io::Seek;
                file.seek(std::io::SeekFrom::Start(offset))?;
                let mut buffer = vec![0u8; size];
                file.read_exact(&mut buffer)?;
                self.stats.disk_reads += 1;
                
                // Promote to memory
                entry.data = Some(TieredData::new(buffer.clone()));
                entry.cold_offset = None;
                self.stats.promotions += 1;
                
                return Ok(Some(buffer));
            }
        }
        
        Ok(None)
    }
    
    /// Migrate entries between tiers based on current viewport
    fn migrate_tiers(&mut self) -> std::io::Result<()> {
        let mut to_cold = Vec::new();
        let mut to_warm = Vec::new();
        let mut to_hot = Vec::new();
        
        for (&id, entry) in &self.entries {
            let new_tier = self.viewport.tier_for(&entry.position);
            if new_tier != entry.tier {
                match new_tier {
                    Tier::Hot => to_hot.push(id),
                    Tier::Warm => to_warm.push(id),
                    Tier::Cold => to_cold.push(id),
                }
            }
        }
        
        // Promote to hot
        for id in to_hot {
            self.promote_to_hot(id)?;
        }
        
        // Move to warm
        for id in to_warm {
            self.move_to_warm(id);
        }
        
        // Demote to cold
        for id in to_cold {
            self.demote_to_cold(id)?;
        }
        
        self.update_tier_counts();
        Ok(())
    }
    
    fn promote_to_hot(&mut self, id: NodeId) -> std::io::Result<()> {
        if let Some(entry) = self.entries.get_mut(&id) {
            // If on disk, load it
            if entry.data.is_none() {
                if let Some((offset, size)) = entry.cold_offset {
                    if let Some(ref mut file) = self.cold_storage {
                        use std::io::Seek;
                        file.seek(std::io::SeekFrom::Start(offset))?;
                        let mut buffer = vec![0u8; size];
                        file.read_exact(&mut buffer)?;
                        entry.data = Some(TieredData::new(buffer));
                        entry.cold_offset = None;
                        self.stats.disk_reads += 1;
                    }
                }
            }
            
            // Decompress if needed
            if let Some(ref mut data) = entry.data {
                if data.compressed {
                    let decompressed = data.get();
                    *data = TieredData::new(decompressed);
                }
            }
            
            entry.tier = Tier::Hot;
            self.stats.promotions += 1;
        }
        Ok(())
    }
    
    fn move_to_warm(&mut self, id: NodeId) {
        if let Some(entry) = self.entries.get_mut(&id) {
            // Compress warm data
            if let Some(ref mut data) = entry.data {
                data.compress();
            }
            entry.tier = Tier::Warm;
        }
    }
    
    fn demote_to_cold(&mut self, id: NodeId) -> std::io::Result<()> {
        if let Some(entry) = self.entries.get_mut(&id) {
            // Write to disk
            if let Some(ref data) = entry.data {
                if let Some(ref mut file) = self.cold_storage {
                    use std::io::Seek;
                    file.seek(std::io::SeekFrom::Start(self.cold_offset))?;
                    let raw = data.raw();
                    file.write_all(raw)?;
                    
                    entry.cold_offset = Some((self.cold_offset, raw.len()));
                    self.cold_offset += raw.len() as u64;
                    self.stats.disk_writes += 1;
                }
            }
            
            entry.data = None;
            entry.tier = Tier::Cold;
            self.stats.demotions += 1;
        }
        Ok(())
    }
    
    fn update_tier_counts(&mut self) {
        self.stats.hot_count = 0;
        self.stats.warm_count = 0;
        self.stats.cold_count = 0;
        self.stats.hot_bytes = 0;
        self.stats.warm_bytes = 0;
        self.stats.cold_bytes = 0;
        
        for entry in self.entries.values() {
            let size = entry.data.as_ref().map(|d| d.memory_size()).unwrap_or(0);
            match entry.tier {
                Tier::Hot => {
                    self.stats.hot_count += 1;
                    self.stats.hot_bytes += size;
                }
                Tier::Warm => {
                    self.stats.warm_count += 1;
                    self.stats.warm_bytes += size;
                }
                Tier::Cold => {
                    self.stats.cold_count += 1;
                    if let Some((_, s)) = entry.cold_offset {
                        self.stats.cold_bytes += s;
                    }
                }
            }
        }
    }
    
    /// Get statistics
    pub fn stats(&self) -> &TieredStats {
        &self.stats
    }
    
    /// Number of entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    
    /// Is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Drop for TieredMemory {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.cold_storage_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_tier_calculation() {
        let viewport = TierViewport::new(500.0, 1000.0);
        
        // In viewport = hot
        let pos_hot = NodePosition::new(600.0, 100.0);
        assert_eq!(viewport.tier_for(&pos_hot), Tier::Hot);
        
        // Near viewport = warm
        let pos_warm = NodePosition::new(1600.0, 100.0);
        assert_eq!(viewport.tier_for(&pos_warm), Tier::Warm);
        
        // Far from viewport = cold
        let pos_cold = NodePosition::new(5000.0, 100.0);
        assert_eq!(viewport.tier_for(&pos_cold), Tier::Cold);
    }
    
    #[test]
    fn test_tiered_memory() -> std::io::Result<()> {
        let path = std::env::temp_dir().join("tiered_test");
        let mut mem = TieredMemory::new(path)?;
        
        // Insert data at different positions
        mem.insert(1, vec![1, 2, 3], NodePosition::new(100.0, 50.0));
        mem.insert(2, vec![4, 5, 6], NodePosition::new(5000.0, 50.0));
        
        assert_eq!(mem.stats().hot_count, 1);
        
        // Get data back
        let data = mem.get(1)?;
        assert_eq!(data, Some(vec![1, 2, 3]));
        
        Ok(())
    }
    
    #[test]
    fn test_compression() {
        let data = vec![0u8; 100]; // Many zeros = good compression
        let mut tiered = TieredData::new(data.clone());
        
        tiered.compress();
        assert!(tiered.compressed);
        assert!(tiered.data.len() < 100);
        
        let decompressed = tiered.get();
        assert_eq!(decompressed, data);
    }
}
