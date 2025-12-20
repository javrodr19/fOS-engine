//! Speculative Offscreen Eviction (Phase 24.1)
//!
//! Track subtree visibility over time. After 5s invisible, serialize
//! to temp file. Keep only bounding box + file offset. Reconstruct
//! on scroll near. Enables long pages to use constant memory.

use std::collections::HashMap;
use std::io::{Read, Write, Seek, SeekFrom};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Node ID type
pub type NodeId = u32;

/// Configuration for offscreen eviction
#[derive(Debug, Clone)]
pub struct EvictionConfig {
    /// How long a subtree must be invisible before eviction
    pub invisibility_threshold: Duration,
    /// Distance from viewport to consider "near" (triggers reconstruction)
    pub reconstruction_distance: f32,
    /// Minimum node count for a subtree to be worth evicting
    pub min_subtree_size: usize,
    /// Maximum evicted subtrees before cleanup
    pub max_evicted: usize,
    /// Temp file path for evicted data
    pub temp_path: PathBuf,
}

impl Default for EvictionConfig {
    fn default() -> Self {
        Self {
            invisibility_threshold: Duration::from_secs(5),
            reconstruction_distance: 1000.0,
            min_subtree_size: 20,
            max_evicted: 1000,
            temp_path: std::env::temp_dir().join("fos_evicted"),
        }
    }
}

/// Bounding box for an evicted subtree
#[derive(Debug, Clone, Copy)]
pub struct BoundingBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl BoundingBox {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }
    
    /// Distance to a viewport
    pub fn distance_to_viewport(&self, viewport: &Viewport) -> f32 {
        let vx1 = viewport.x;
        let vy1 = viewport.y;
        let vx2 = viewport.x + viewport.width;
        let vy2 = viewport.y + viewport.height;
        
        let bx1 = self.x;
        let by1 = self.y;
        let bx2 = self.x + self.width;
        let by2 = self.y + self.height;
        
        // If overlapping, distance is 0
        if bx1 < vx2 && bx2 > vx1 && by1 < vy2 && by2 > vy1 {
            return 0.0;
        }
        
        // Calculate distance to nearest edge
        let dx = if bx2 < vx1 { vx1 - bx2 } else if bx1 > vx2 { bx1 - vx2 } else { 0.0 };
        let dy = if by2 < vy1 { vy1 - by2 } else if by1 > vy2 { by1 - vy2 } else { 0.0 };
        
        (dx * dx + dy * dy).sqrt()
    }
    
    /// Check if within reconstruction distance of viewport
    pub fn is_near_viewport(&self, viewport: &Viewport, distance: f32) -> bool {
        self.distance_to_viewport(viewport) <= distance
    }
}

/// Viewport for visibility calculations
#[derive(Debug, Clone, Copy)]
pub struct Viewport {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Viewport {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }
}

/// Visibility state of a subtree
#[derive(Debug, Clone, Copy)]
pub enum VisibilityState {
    /// Currently visible
    Visible,
    /// Just became invisible
    BecameInvisible(Instant),
    /// Invisible long enough to consider eviction
    EvictionCandidate(Instant),
    /// Evicted to disk
    Evicted,
}

/// Record for an evicted subtree
#[derive(Debug, Clone)]
pub struct EvictedSubtree {
    /// Root node ID of the evicted subtree
    pub root_id: NodeId,
    /// Bounding box (for reconstruction trigger)
    pub bounds: BoundingBox,
    /// Number of nodes in the subtree
    pub node_count: usize,
    /// Offset in the temp file
    pub file_offset: u64,
    /// Size in bytes
    pub byte_size: usize,
    /// When it was evicted
    pub evicted_at: Instant,
}

/// Visibility tracker for subtrees
pub struct VisibilityTracker {
    /// Current visibility state per subtree root
    states: HashMap<NodeId, VisibilityState>,
    /// Last seen bounding boxes
    bounds: HashMap<NodeId, BoundingBox>,
    /// Subtrees that have child count (for size checking)
    subtree_sizes: HashMap<NodeId, usize>,
}

impl Default for VisibilityTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl VisibilityTracker {
    pub fn new() -> Self {
        Self {
            states: HashMap::new(),
            bounds: HashMap::new(),
            subtree_sizes: HashMap::new(),
        }
    }
    
    /// Update visibility for a subtree
    pub fn update(&mut self, node_id: NodeId, is_visible: bool, bounds: BoundingBox) {
        self.bounds.insert(node_id, bounds);
        
        let state = self.states.entry(node_id).or_insert(VisibilityState::Visible);
        
        *state = match (*state, is_visible) {
            (_, true) => VisibilityState::Visible,
            (VisibilityState::Visible, false) => VisibilityState::BecameInvisible(Instant::now()),
            (VisibilityState::BecameInvisible(t), false) | 
            (VisibilityState::EvictionCandidate(t), false) => {
                if t.elapsed() > Duration::from_secs(5) {
                    VisibilityState::EvictionCandidate(t)
                } else {
                    VisibilityState::BecameInvisible(t)
                }
            }
            (VisibilityState::Evicted, false) => VisibilityState::Evicted,
        };
    }
    
    /// Register subtree size
    pub fn set_subtree_size(&mut self, node_id: NodeId, size: usize) {
        self.subtree_sizes.insert(node_id, size);
    }
    
    /// Get eviction candidates
    pub fn get_candidates(&self, config: &EvictionConfig) -> Vec<NodeId> {
        self.states.iter()
            .filter_map(|(&id, &state)| {
                if let VisibilityState::EvictionCandidate(t) = state {
                    if t.elapsed() >= config.invisibility_threshold {
                        if let Some(&size) = self.subtree_sizes.get(&id) {
                            if size >= config.min_subtree_size {
                                return Some(id);
                            }
                        }
                    }
                }
                None
            })
            .collect()
    }
    
    /// Mark as evicted
    pub fn mark_evicted(&mut self, node_id: NodeId) {
        self.states.insert(node_id, VisibilityState::Evicted);
    }
    
    /// Get bounds for a node
    pub fn get_bounds(&self, node_id: NodeId) -> Option<BoundingBox> {
        self.bounds.get(&node_id).copied()
    }
    
    /// Get state for a node
    pub fn get_state(&self, node_id: NodeId) -> Option<VisibilityState> {
        self.states.get(&node_id).copied()
    }
    
    /// Remove tracking for a node
    pub fn remove(&mut self, node_id: NodeId) {
        self.states.remove(&node_id);
        self.bounds.remove(&node_id);
        self.subtree_sizes.remove(&node_id);
    }
}

/// Serialized subtree data (simple format)
#[derive(Debug)]
pub struct SerializedSubtree {
    /// Serialized node data
    pub data: Vec<u8>,
}

impl SerializedSubtree {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }
    
    /// Size in bytes
    pub fn size(&self) -> usize {
        self.data.len()
    }
}

/// Offscreen eviction manager
pub struct EvictionManager {
    /// Configuration
    config: EvictionConfig,
    /// Visibility tracker
    tracker: VisibilityTracker,
    /// Evicted subtrees
    evicted: HashMap<NodeId, EvictedSubtree>,
    /// File for storing evicted data
    storage_file: Option<std::fs::File>,
    /// Current write offset in file
    write_offset: u64,
    /// Statistics
    stats: EvictionStats,
}

/// Eviction statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct EvictionStats {
    /// Total subtrees evicted
    pub evictions: u64,
    /// Total subtrees reconstructed
    pub reconstructions: u64,
    /// Total bytes evicted
    pub bytes_evicted: u64,
    /// Currently evicted bytes
    pub current_evicted_bytes: u64,
    /// Peak evicted bytes
    pub peak_evicted_bytes: u64,
}

impl EvictionStats {
    pub fn memory_saved(&self) -> u64 {
        self.current_evicted_bytes
    }
}

impl EvictionManager {
    pub fn new(config: EvictionConfig) -> std::io::Result<Self> {
        // Create temp directory if needed
        if let Some(parent) = config.temp_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&config.temp_path)?;
        
        Ok(Self {
            config,
            tracker: VisibilityTracker::new(),
            evicted: HashMap::new(),
            storage_file: Some(file),
            write_offset: 0,
            stats: EvictionStats::default(),
        })
    }
    
    /// Get the visibility tracker
    pub fn tracker(&self) -> &VisibilityTracker {
        &self.tracker
    }
    
    /// Get mutable visibility tracker
    pub fn tracker_mut(&mut self) -> &mut VisibilityTracker {
        &mut self.tracker
    }
    
    /// Process evictions based on current visibility
    pub fn process(&mut self) -> std::io::Result<Vec<NodeId>> {
        let candidates = self.tracker.get_candidates(&self.config);
        let mut evicted = Vec::new();
        
        // Limit evictions per frame
        for node_id in candidates.into_iter().take(5) {
            // Get bounds before evicting
            if let Some(bounds) = self.tracker.get_bounds(node_id) {
                // Mark as evicted in tracker
                self.tracker.mark_evicted(node_id);
                evicted.push(node_id);
                
                // Record eviction (actual serialization would happen externally)
                let record = EvictedSubtree {
                    root_id: node_id,
                    bounds,
                    node_count: 0, // Would be filled by caller
                    file_offset: self.write_offset,
                    byte_size: 0,
                    evicted_at: Instant::now(),
                };
                
                self.evicted.insert(node_id, record);
                self.stats.evictions += 1;
            }
        }
        
        Ok(evicted)
    }
    
    /// Evict a specific subtree with serialized data
    pub fn evict(&mut self, node_id: NodeId, bounds: BoundingBox, data: SerializedSubtree) -> std::io::Result<()> {
        let file = self.storage_file.as_mut()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "No storage file"))?;
        
        // Write to file
        file.seek(SeekFrom::Start(self.write_offset))?;
        file.write_all(&data.data)?;
        
        let byte_size = data.size();
        let record = EvictedSubtree {
            root_id: node_id,
            bounds,
            node_count: 0, // Caller would calculate
            file_offset: self.write_offset,
            byte_size,
            evicted_at: Instant::now(),
        };
        
        self.write_offset += byte_size as u64;
        self.evicted.insert(node_id, record);
        self.tracker.mark_evicted(node_id);
        
        // Update stats
        self.stats.evictions += 1;
        self.stats.bytes_evicted += byte_size as u64;
        self.stats.current_evicted_bytes += byte_size as u64;
        self.stats.peak_evicted_bytes = self.stats.peak_evicted_bytes.max(self.stats.current_evicted_bytes);
        
        Ok(())
    }
    
    /// Check if any evicted subtrees should be reconstructed
    pub fn check_reconstruction(&self, viewport: &Viewport) -> Vec<NodeId> {
        self.evicted.iter()
            .filter(|(_, record)| {
                record.bounds.is_near_viewport(viewport, self.config.reconstruction_distance)
            })
            .map(|(&id, _)| id)
            .collect()
    }
    
    /// Read evicted data for reconstruction
    pub fn read_evicted(&mut self, node_id: NodeId) -> std::io::Result<Option<Vec<u8>>> {
        let record = match self.evicted.get(&node_id) {
            Some(r) => r,
            None => return Ok(None),
        };
        
        let file = self.storage_file.as_mut()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "No storage file"))?;
        
        file.seek(SeekFrom::Start(record.file_offset))?;
        let mut data = vec![0u8; record.byte_size];
        file.read_exact(&mut data)?;
        
        Ok(Some(data))
    }
    
    /// Complete reconstruction
    pub fn complete_reconstruction(&mut self, node_id: NodeId) {
        if let Some(record) = self.evicted.remove(&node_id) {
            self.stats.reconstructions += 1;
            self.stats.current_evicted_bytes = self.stats.current_evicted_bytes.saturating_sub(record.byte_size as u64);
        }
        self.tracker.remove(node_id);
    }
    
    /// Get statistics
    pub fn stats(&self) -> &EvictionStats {
        &self.stats
    }
    
    /// Get evicted subtree info
    pub fn get_evicted(&self, node_id: NodeId) -> Option<&EvictedSubtree> {
        self.evicted.get(&node_id)
    }
    
    /// Number of evicted subtrees
    pub fn evicted_count(&self) -> usize {
        self.evicted.len()
    }
}

impl Drop for EvictionManager {
    fn drop(&mut self) {
        // Clean up temp file
        if let Some(path) = Some(&self.config.temp_path) {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_bounding_box_distance() {
        let viewport = Viewport::new(0.0, 0.0, 100.0, 100.0);
        
        // Overlapping
        let b1 = BoundingBox::new(50.0, 50.0, 100.0, 100.0);
        assert_eq!(b1.distance_to_viewport(&viewport), 0.0);
        
        // Below viewport
        let b2 = BoundingBox::new(0.0, 200.0, 100.0, 100.0);
        assert_eq!(b2.distance_to_viewport(&viewport), 100.0);
    }
    
    #[test]
    fn test_visibility_tracker() {
        let mut tracker = VisibilityTracker::new();
        let bounds = BoundingBox::new(0.0, 0.0, 100.0, 100.0);
        
        // Initially visible
        tracker.update(1, true, bounds);
        assert!(matches!(tracker.get_state(1), Some(VisibilityState::Visible)));
        
        // Becomes invisible
        tracker.update(1, false, bounds);
        assert!(matches!(tracker.get_state(1), Some(VisibilityState::BecameInvisible(_))));
    }
    
    #[test]
    fn test_eviction_manager() -> std::io::Result<()> {
        let config = EvictionConfig {
            temp_path: std::env::temp_dir().join("fos_test_eviction"),
            ..Default::default()
        };
        
        let mut manager = EvictionManager::new(config)?;
        let bounds = BoundingBox::new(0.0, 1000.0, 100.0, 100.0);
        let data = SerializedSubtree::new(vec![1, 2, 3, 4, 5]);
        
        manager.evict(1, bounds, data)?;
        
        assert_eq!(manager.evicted_count(), 1);
        assert_eq!(manager.stats().evictions, 1);
        
        // Read back
        let read = manager.read_evicted(1)?;
        assert_eq!(read, Some(vec![1, 2, 3, 4, 5]));
        
        Ok(())
    }
}
