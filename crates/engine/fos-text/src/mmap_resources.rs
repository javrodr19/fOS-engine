//! Memory-Mapped Resources
//!
//! Memory-efficient font file access. When `memmap2` crate is available,
//! uses memory mapping. Otherwise loads into memory.

use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::io;

/// Memory-mapped font file
#[derive(Debug)]
pub struct MappedFont {
    /// Font data
    data: Vec<u8>,
    /// Font ID
    pub id: u32,
    /// File path
    pub path: PathBuf,
    /// File size
    pub size: usize,
}

impl MappedFont {
    /// Open font file
    pub fn open(path: impl AsRef<Path>, id: u32) -> io::Result<Self> {
        let path = path.as_ref();
        let _file = File::open(path)?;
        let data = std::fs::read(path)?;
        let size = data.len();
        
        Ok(Self {
            data,
            id,
            path: path.to_path_buf(),
            size,
        })
    }
    
    /// Get font data
    pub fn data(&self) -> &[u8] {
        &self.data
    }
    
    /// Is memory-mapped (always false in this implementation)
    pub fn is_mapped(&self) -> bool {
        false
    }
}

/// Pool of memory-mapped resources
#[derive(Debug, Default)]
pub struct MappingPool {
    fonts: HashMap<u32, MappedFont>,
    next_id: u32,
    max_mappings: usize,
    stats: MappingStats,
}

/// Mapping statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct MappingStats {
    pub total_mapped: usize,
    pub total_loaded: usize,
    pub bytes_mapped: usize,
    pub bytes_loaded: usize,
    pub mapping_failures: usize,
}

impl MappingPool {
    /// Create new pool
    pub fn new() -> Self {
        Self { fonts: HashMap::new(), next_id: 1, max_mappings: 256, stats: MappingStats::default() }
    }
    
    /// Set max concurrent mappings
    pub fn set_max_mappings(&mut self, max: usize) { self.max_mappings = max; }
    
    /// Open and map font
    pub fn open_font(&mut self, path: impl AsRef<Path>) -> io::Result<u32> {
        // Evict if at capacity
        while self.fonts.len() >= self.max_mappings {
            if let Some(&id) = self.fonts.keys().next() {
                self.close(id);
            }
        }
        
        let id = self.next_id;
        self.next_id += 1;
        
        let font = MappedFont::open(path, id)?;
        
        if font.is_mapped() {
            self.stats.total_mapped += 1;
            self.stats.bytes_mapped += font.size;
        } else {
            self.stats.total_loaded += 1;
            self.stats.bytes_loaded += font.size;
        }
        
        self.fonts.insert(id, font);
        Ok(id)
    }
    
    /// Get font by ID
    pub fn get(&self, id: u32) -> Option<&MappedFont> { self.fonts.get(&id) }
    
    /// Close mapping
    pub fn close(&mut self, id: u32) { self.fonts.remove(&id); }
    
    /// Get stats
    pub fn stats(&self) -> &MappingStats { &self.stats }
    
    /// Active mappings count
    pub fn len(&self) -> usize { self.fonts.len() }
    
    /// Is empty
    pub fn is_empty(&self) -> bool { self.fonts.is_empty() }
}

/// Generic memory-mapped resource
#[derive(Debug)]
pub struct MappedResource {
    path: PathBuf,
    data: Vec<u8>, // Simplified: just load for now
    size: usize,
}

impl MappedResource {
    /// Open resource
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let data = std::fs::read(&path)?;
        let size = data.len();
        Ok(Self { path, data, size })
    }
    
    /// Get data
    pub fn data(&self) -> &[u8] { &self.data }
    
    /// Get size
    pub fn size(&self) -> usize { self.size }
    
    /// Get path
    pub fn path(&self) -> &Path { &self.path }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    
    #[test]
    fn test_mapping_pool() {
        let mut pool = MappingPool::new();
        assert!(pool.is_empty());
        
        // Create temp file
        let dir = std::env::temp_dir();
        let path = dir.join("test_font.ttf");
        {
            let mut f = File::create(&path).unwrap();
            f.write_all(&[0u8; 1000]).unwrap();
        }
        
        let id = pool.open_font(&path).unwrap();
        assert!(!pool.is_empty());
        
        let font = pool.get(id).unwrap();
        assert_eq!(font.data().len(), 1000);
        
        pool.close(id);
        assert!(pool.is_empty());
        
        std::fs::remove_file(path).ok();
    }
}
