//! Compact Layout Storage (Phase 5.2)
//!
//! Ultra-compact layout result storage using 8 bytes instead of 32+.
//! Uses i16/u16 relative coordinates for memory efficiency.

// ============================================================================
// Compact Layout Result
// ============================================================================

/// Ultra-compact layout result (8 bytes total)
/// 
/// Stores layout data using relative coordinates and small integers.
/// Suitable for 95%+ of web layouts where dimensions are < 32K pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(C, packed)]
pub struct CompactLayoutResult {
    /// X position relative to parent (covers ±32K)
    pub x: i16,
    /// Y position relative to parent
    pub y: i16,
    /// Width (max 65K pixels)
    pub width: u16,
    /// Height (max 65K pixels)
    pub height: u16,
}

impl CompactLayoutResult {
    /// Maximum coordinate value that can be stored
    pub const MAX_COORD: f32 = i16::MAX as f32;
    /// Minimum coordinate value that can be stored
    pub const MIN_COORD: f32 = i16::MIN as f32;
    /// Maximum dimension value that can be stored
    pub const MAX_DIM: f32 = u16::MAX as f32;
    
    /// Create a new compact layout result
    pub fn new(x: i16, y: i16, width: u16, height: u16) -> Self {
        Self { x, y, width, height }
    }
    
    /// Create from f32 coordinates, clamping to valid range
    /// Returns None if values would overflow
    pub fn from_f32(x: f32, y: f32, width: f32, height: f32) -> Option<Self> {
        // Check bounds
        if x < Self::MIN_COORD || x > Self::MAX_COORD ||
           y < Self::MIN_COORD || y > Self::MAX_COORD ||
           width < 0.0 || width > Self::MAX_DIM ||
           height < 0.0 || height > Self::MAX_DIM {
            return None;
        }
        
        Some(Self {
            x: x.round() as i16,
            y: y.round() as i16,
            width: width.round() as u16,
            height: height.round() as u16,
        })
    }
    
    /// Create from f32 coordinates with clamping (never fails)
    pub fn from_f32_clamped(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x: x.clamp(Self::MIN_COORD, Self::MAX_COORD).round() as i16,
            y: y.clamp(Self::MIN_COORD, Self::MAX_COORD).round() as i16,
            width: width.clamp(0.0, Self::MAX_DIM).round() as u16,
            height: height.clamp(0.0, Self::MAX_DIM).round() as u16,
        }
    }
    
    /// Convert to f32 coordinates
    pub fn to_f32(&self) -> (f32, f32, f32, f32) {
        (self.x as f32, self.y as f32, self.width as f32, self.height as f32)
    }
    
    /// Size in bytes
    pub const fn size() -> usize {
        8 // 2 + 2 + 2 + 2
    }
    
    /// Check if this can represent the given layout without loss
    pub fn can_represent(x: f32, y: f32, width: f32, height: f32) -> bool {
        x >= Self::MIN_COORD && x <= Self::MAX_COORD &&
        y >= Self::MIN_COORD && y <= Self::MAX_COORD &&
        width >= 0.0 && width <= Self::MAX_DIM &&
        height >= 0.0 && height <= Self::MAX_DIM
    }
    
    /// Right edge
    pub fn right(&self) -> i32 {
        self.x as i32 + self.width as i32
    }
    
    /// Bottom edge
    pub fn bottom(&self) -> i32 {
        self.y as i32 + self.height as i32
    }
}

// ============================================================================
// Full Layout Result
// ============================================================================

/// Full-precision layout result (32 bytes)
/// 
/// Used when compact storage would overflow or for final output.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FullLayoutResult {
    /// X position
    pub x: f32,
    /// Y position
    pub y: f32,
    /// Width
    pub width: f32,
    /// Height
    pub height: f32,
    /// Margin box expansion
    pub margin_top: f32,
    pub margin_right: f32,
    pub margin_bottom: f32,
    pub margin_left: f32,
}

impl FullLayoutResult {
    /// Create from compact result
    pub fn from_compact(compact: CompactLayoutResult) -> Self {
        let (x, y, width, height) = compact.to_f32();
        Self {
            x,
            y,
            width,
            height,
            margin_top: 0.0,
            margin_right: 0.0,
            margin_bottom: 0.0,
            margin_left: 0.0,
        }
    }
    
    /// Try to convert to compact result
    pub fn to_compact(&self) -> Option<CompactLayoutResult> {
        CompactLayoutResult::from_f32(self.x, self.y, self.width, self.height)
    }
    
    /// Size in bytes
    pub const fn size() -> usize {
        32 // 8 * 4 bytes
    }
}

// ============================================================================
// Layout Storage
// ============================================================================

/// Mixed layout storage using compact where possible
#[derive(Debug, Clone)]
pub enum LayoutStorage {
    /// Compact 8-byte storage
    Compact(CompactLayoutResult),
    /// Full 32-byte storage
    Full(Box<FullLayoutResult>),
}

impl LayoutStorage {
    /// Create from coordinates, using compact if possible
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        if let Some(compact) = CompactLayoutResult::from_f32(x, y, width, height) {
            Self::Compact(compact)
        } else {
            Self::Full(Box::new(FullLayoutResult {
                x,
                y,
                width,
                height,
                margin_top: 0.0,
                margin_right: 0.0,
                margin_bottom: 0.0,
                margin_left: 0.0,
            }))
        }
    }
    
    /// Get coordinates as f32
    pub fn to_f32(&self) -> (f32, f32, f32, f32) {
        match self {
            Self::Compact(c) => c.to_f32(),
            Self::Full(f) => (f.x, f.y, f.width, f.height),
        }
    }
    
    /// Is this using compact storage?
    pub fn is_compact(&self) -> bool {
        matches!(self, Self::Compact(_))
    }
    
    /// Memory size of this storage
    pub fn memory_size(&self) -> usize {
        match self {
            Self::Compact(_) => CompactLayoutResult::size(),
            Self::Full(_) => FullLayoutResult::size() + std::mem::size_of::<Box<FullLayoutResult>>(),
        }
    }
}

impl Default for LayoutStorage {
    fn default() -> Self {
        Self::Compact(CompactLayoutResult::default())
    }
}

// ============================================================================
// Batch Layout Storage
// ============================================================================

/// Batch storage for many layout results
/// 
/// Optimized for memory efficiency when storing many layouts.
#[derive(Debug)]
pub struct BatchLayoutStorage {
    /// Compact results (most common)
    compact: Vec<CompactLayoutResult>,
    /// Full results for overflow cases (node_id -> result)
    full: std::collections::HashMap<usize, FullLayoutResult>,
    /// Statistics
    compact_count: usize,
    full_count: usize,
}

impl BatchLayoutStorage {
    /// Create with capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            compact: Vec::with_capacity(capacity),
            full: std::collections::HashMap::new(),
            compact_count: 0,
            full_count: 0,
        }
    }
    
    /// Store a layout result
    pub fn store(&mut self, id: usize, x: f32, y: f32, width: f32, height: f32) {
        // Ensure compact vec is large enough
        if id >= self.compact.len() {
            self.compact.resize(id + 1, CompactLayoutResult::default());
        }
        
        if let Some(compact) = CompactLayoutResult::from_f32(x, y, width, height) {
            self.compact[id] = compact;
            self.compact_count += 1;
        } else {
            // Store full result
            self.full.insert(id, FullLayoutResult {
                x,
                y,
                width,
                height,
                margin_top: 0.0,
                margin_right: 0.0,
                margin_bottom: 0.0,
                margin_left: 0.0,
            });
            self.full_count += 1;
        }
    }
    
    /// Get layout result
    pub fn get(&self, id: usize) -> Option<(f32, f32, f32, f32)> {
        // Check full first (override)
        if let Some(full) = self.full.get(&id) {
            return Some((full.x, full.y, full.width, full.height));
        }
        
        // Check compact
        if id < self.compact.len() {
            let c = &self.compact[id];
            return Some(c.to_f32());
        }
        
        None
    }
    
    /// Memory usage in bytes
    pub fn memory_usage(&self) -> usize {
        self.compact.len() * CompactLayoutResult::size() +
        self.full.len() * FullLayoutResult::size()
    }
    
    /// Compression ratio vs full storage
    pub fn compression_ratio(&self) -> f64 {
        let total_count = self.compact_count + self.full_count;
        if total_count == 0 {
            return 1.0;
        }
        
        let full_size = total_count * FullLayoutResult::size();
        let actual_size = self.memory_usage();
        
        actual_size as f64 / full_size as f64
    }
    
    /// Percentage of layouts using compact storage
    pub fn compact_percentage(&self) -> f64 {
        let total = self.compact_count + self.full_count;
        if total == 0 {
            return 100.0;
        }
        self.compact_count as f64 / total as f64 * 100.0
    }
}

impl Default for BatchLayoutStorage {
    fn default() -> Self {
        Self::with_capacity(256)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_compact_size() {
        assert_eq!(CompactLayoutResult::size(), 8);
        assert_eq!(std::mem::size_of::<CompactLayoutResult>(), 8);
    }
    
    #[test]
    fn test_compact_from_f32() {
        let compact = CompactLayoutResult::from_f32(100.0, 200.0, 300.0, 400.0);
        assert!(compact.is_some());
        
        let c = compact.unwrap();
        assert_eq!(c.x, 100);
        assert_eq!(c.y, 200);
        assert_eq!(c.width, 300);
        assert_eq!(c.height, 400);
    }
    
    #[test]
    fn test_compact_overflow() {
        // Width overflow
        let compact = CompactLayoutResult::from_f32(0.0, 0.0, 100000.0, 100.0);
        assert!(compact.is_none());
        
        // X overflow
        let compact = CompactLayoutResult::from_f32(50000.0, 0.0, 100.0, 100.0);
        assert!(compact.is_none());
    }
    
    #[test]
    fn test_compact_roundtrip() {
        let original = CompactLayoutResult::new(150, -50, 800, 600);
        let (x, y, w, h) = original.to_f32();
        
        assert_eq!(x, 150.0);
        assert_eq!(y, -50.0);
        assert_eq!(w, 800.0);
        assert_eq!(h, 600.0);
    }
    
    #[test]
    fn test_layout_storage_auto_selection() {
        // Small values -> compact
        let storage = LayoutStorage::new(100.0, 200.0, 300.0, 400.0);
        assert!(storage.is_compact());
        
        // Large values -> full
        let storage = LayoutStorage::new(0.0, 0.0, 100000.0, 100.0);
        assert!(!storage.is_compact());
    }
    
    #[test]
    fn test_batch_storage() {
        let mut batch = BatchLayoutStorage::with_capacity(10);
        
        batch.store(0, 100.0, 100.0, 200.0, 150.0);
        batch.store(1, 300.0, 100.0, 200.0, 150.0);
        
        let (x, y, w, h) = batch.get(0).unwrap();
        assert_eq!(x, 100.0);
        assert_eq!(w, 200.0);
        
        assert!(batch.compact_percentage() > 99.0);
    }
    
    #[test]
    fn test_compression_ratio() {
        let mut batch = BatchLayoutStorage::with_capacity(100);
        
        // Store 100 compact layouts
        for i in 0..100 {
            batch.store(i, (i * 10) as f32, 0.0, 100.0, 50.0);
        }
        
        // Should achieve ~4x compression (8 bytes vs 32 bytes)
        assert!(batch.compression_ratio() < 0.5);
    }
}
