//! Display List Compilation (Phase 24.5)
//!
//! Convert paint ops to GPU command buffer once. Replay without CPU
//! involvement. Cache compiled lists. 10x repaint speed.

use std::collections::HashMap;

/// Display list command
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum DisplayCommand {
    /// Push a clip rect
    PushClip = 0,
    /// Pop clip rect
    PopClip = 1,
    /// Push transform
    PushTransform = 2,
    /// Pop transform
    PopTransform = 3,
    /// Draw filled rect
    FillRect = 4,
    /// Draw rect border
    StrokeRect = 5,
    /// Draw image
    DrawImage = 6,
    /// Draw text
    DrawText = 7,
    /// Draw line
    DrawLine = 8,
    /// Set color
    SetColor = 9,
    /// Set opacity
    SetOpacity = 10,
    /// Draw rounded rect
    FillRoundedRect = 11,
    /// Draw shadow
    DrawShadow = 12,
    /// Draw gradient
    DrawGradient = 13,
}

/// Color in RGBA
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
    
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
    
    pub const TRANSPARENT: Self = Self { r: 0, g: 0, b: 0, a: 0 };
    pub const WHITE: Self = Self { r: 255, g: 255, b: 255, a: 255 };
    pub const BLACK: Self = Self { r: 0, g: 0, b: 0, a: 255 };
}

/// Rectangle
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }
}

/// Transform matrix (2D affine)
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Transform {
    pub a: f32, pub b: f32,
    pub c: f32, pub d: f32,
    pub e: f32, pub f: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self::identity()
    }
}

impl Transform {
    pub const fn identity() -> Self {
        Self { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 }
    }
    
    pub fn translate(x: f32, y: f32) -> Self {
        Self { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: x, f: y }
    }
    
    pub fn scale(sx: f32, sy: f32) -> Self {
        Self { a: sx, b: 0.0, c: 0.0, d: sy, e: 0.0, f: 0.0 }
    }
}

/// Compiled display item - packed for cache efficiency
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct DisplayItem {
    pub command: DisplayCommand,
    pub _pad: [u8; 3],
    pub data: DisplayItemData,
}

/// Data for display items
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub union DisplayItemData {
    pub rect: Rect,
    pub color: Color,
    pub transform: Transform,
    pub image: ImageRef,
    pub text: TextRef,
    pub opacity: f32,
    pub corner_radius: f32,
    pub raw: [u8; 24],
}

impl Default for DisplayItemData {
    fn default() -> Self {
        DisplayItemData { raw: [0u8; 24] }
    }
}

/// Reference to an image in the atlas
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct ImageRef {
    pub atlas_id: u16,
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

/// Reference to text in the buffer
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct TextRef {
    pub offset: u32,
    pub length: u16,
    pub font_id: u16,
    pub size: f32,
}

/// Compiled display list
#[derive(Debug, Clone)]
pub struct DisplayList {
    /// Unique ID for caching
    id: u64,
    /// Compiled commands
    items: Vec<DisplayItem>,
    /// Text data referenced by TextRef
    text_data: Vec<u8>,
    /// Bounding box of entire list
    bounds: Rect,
    /// Whether this list is cacheable
    cacheable: bool,
}

impl DisplayList {
    /// Create a new display list
    pub fn new(id: u64) -> Self {
        Self {
            id,
            items: Vec::new(),
            text_data: Vec::new(),
            bounds: Rect::default(),
            cacheable: true,
        }
    }
    
    /// Get the ID
    pub fn id(&self) -> u64 {
        self.id
    }
    
    /// Add a fill rect command
    pub fn fill_rect(&mut self, rect: Rect) {
        self.items.push(DisplayItem {
            command: DisplayCommand::FillRect,
            _pad: [0; 3],
            data: DisplayItemData { rect },
        });
        self.expand_bounds(&rect);
    }
    
    /// Add a stroke rect command
    pub fn stroke_rect(&mut self, rect: Rect) {
        self.items.push(DisplayItem {
            command: DisplayCommand::StrokeRect,
            _pad: [0; 3],
            data: DisplayItemData { rect },
        });
        self.expand_bounds(&rect);
    }
    
    /// Add a set color command
    pub fn set_color(&mut self, color: Color) {
        self.items.push(DisplayItem {
            command: DisplayCommand::SetColor,
            _pad: [0; 3],
            data: DisplayItemData { color },
        });
    }
    
    /// Add set opacity command
    pub fn set_opacity(&mut self, opacity: f32) {
        self.items.push(DisplayItem {
            command: DisplayCommand::SetOpacity,
            _pad: [0; 3],
            data: DisplayItemData { opacity },
        });
    }
    
    /// Push clip rect
    pub fn push_clip(&mut self, rect: Rect) {
        self.items.push(DisplayItem {
            command: DisplayCommand::PushClip,
            _pad: [0; 3],
            data: DisplayItemData { rect },
        });
    }
    
    /// Pop clip rect
    pub fn pop_clip(&mut self) {
        self.items.push(DisplayItem {
            command: DisplayCommand::PopClip,
            _pad: [0; 3],
            data: DisplayItemData::default(),
        });
    }
    
    /// Push transform
    pub fn push_transform(&mut self, transform: Transform) {
        self.items.push(DisplayItem {
            command: DisplayCommand::PushTransform,
            _pad: [0; 3],
            data: DisplayItemData { transform },
        });
    }
    
    /// Pop transform
    pub fn pop_transform(&mut self) {
        self.items.push(DisplayItem {
            command: DisplayCommand::PopTransform,
            _pad: [0; 3],
            data: DisplayItemData::default(),
        });
    }
    
    /// Draw image
    pub fn draw_image(&mut self, image: ImageRef, dest: Rect) {
        self.items.push(DisplayItem {
            command: DisplayCommand::DrawImage,
            _pad: [0; 3],
            data: DisplayItemData { image },
        });
        self.expand_bounds(&dest);
    }
    
    /// Draw text
    pub fn draw_text(&mut self, text: &str, font_id: u16, size: f32) {
        let offset = self.text_data.len() as u32;
        self.text_data.extend_from_slice(text.as_bytes());
        
        let text_ref = TextRef {
            offset,
            length: text.len() as u16,
            font_id,
            size,
        };
        
        self.items.push(DisplayItem {
            command: DisplayCommand::DrawText,
            _pad: [0; 3],
            data: DisplayItemData { text: text_ref },
        });
    }
    
    /// Expand bounds to include rect
    fn expand_bounds(&mut self, rect: &Rect) {
        if self.bounds.width == 0.0 {
            self.bounds = *rect;
            return;
        }
        
        let x1 = self.bounds.x.min(rect.x);
        let y1 = self.bounds.y.min(rect.y);
        let x2 = (self.bounds.x + self.bounds.width).max(rect.x + rect.width);
        let y2 = (self.bounds.y + self.bounds.height).max(rect.y + rect.height);
        
        self.bounds = Rect::new(x1, y1, x2 - x1, y2 - y1);
    }
    
    /// Get bounds
    pub fn bounds(&self) -> Rect {
        self.bounds
    }
    
    /// Number of commands
    pub fn len(&self) -> usize {
        self.items.len()
    }
    
    /// Is empty
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    
    /// Get items for replay
    pub fn items(&self) -> &[DisplayItem] {
        &self.items
    }
    
    /// Get text data
    pub fn text_data(&self) -> &[u8] {
        &self.text_data
    }
    
    /// Memory size
    pub fn memory_size(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.items.len() * std::mem::size_of::<DisplayItem>()
            + self.text_data.len()
    }
    
    /// Mark as not cacheable
    pub fn mark_uncacheable(&mut self) {
        self.cacheable = false;
    }
    
    /// Check if cacheable
    pub fn is_cacheable(&self) -> bool {
        self.cacheable
    }
}

/// Display list cache
#[derive(Debug)]
pub struct DisplayListCache {
    /// Cached lists by ID
    cache: HashMap<u64, DisplayList>,
    /// Max cache size in bytes
    max_size: usize,
    /// Current size
    current_size: usize,
    /// Stats
    stats: CacheStats,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
}

impl CacheStats {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 { 0.0 } else { self.hits as f64 / total as f64 }
    }
}

impl Default for DisplayListCache {
    fn default() -> Self {
        Self::new()
    }
}

impl DisplayListCache {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            max_size: 64 * 1024 * 1024, // 64 MB
            current_size: 0,
            stats: CacheStats::default(),
        }
    }
    
    /// Set max cache size
    pub fn with_max_size(mut self, size: usize) -> Self {
        self.max_size = size;
        self
    }
    
    /// Get a cached display list
    pub fn get(&mut self, id: u64) -> Option<&DisplayList> {
        if self.cache.contains_key(&id) {
            self.stats.hits += 1;
            self.cache.get(&id)
        } else {
            self.stats.misses += 1;
            None
        }
    }
    
    /// Insert a display list
    pub fn insert(&mut self, list: DisplayList) {
        if !list.is_cacheable() {
            return;
        }
        
        let size = list.memory_size();
        
        // Evict if necessary
        while self.current_size + size > self.max_size && !self.cache.is_empty() {
            // Simple eviction: remove first entry
            if let Some(&key) = self.cache.keys().next() {
                if let Some(evicted) = self.cache.remove(&key) {
                    self.current_size -= evicted.memory_size();
                    self.stats.evictions += 1;
                }
            }
        }
        
        self.current_size += size;
        self.cache.insert(list.id(), list);
    }
    
    /// Remove a display list
    pub fn remove(&mut self, id: u64) {
        if let Some(list) = self.cache.remove(&id) {
            self.current_size -= list.memory_size();
        }
    }
    
    /// Clear the cache
    pub fn clear(&mut self) {
        self.cache.clear();
        self.current_size = 0;
    }
    
    /// Get stats
    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }
    
    /// Number of cached lists
    pub fn len(&self) -> usize {
        self.cache.len()
    }
    
    /// Is empty
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}

/// Display list builder
pub struct DisplayListBuilder {
    list: DisplayList,
    id_counter: u64,
}

impl Default for DisplayListBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl DisplayListBuilder {
    pub fn new() -> Self {
        Self {
            list: DisplayList::new(0),
            id_counter: 0,
        }
    }
    
    /// Start a new display list
    pub fn begin(&mut self) -> u64 {
        self.id_counter += 1;
        self.list = DisplayList::new(self.id_counter);
        self.id_counter
    }
    
    /// Get mutable reference to current list
    pub fn list(&mut self) -> &mut DisplayList {
        &mut self.list
    }
    
    /// Finish and return the display list
    pub fn finish(&mut self) -> DisplayList {
        std::mem::replace(&mut self.list, DisplayList::new(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_display_list() {
        let mut list = DisplayList::new(1);
        
        list.set_color(Color::rgb(255, 0, 0));
        list.fill_rect(Rect::new(10.0, 10.0, 100.0, 50.0));
        list.set_color(Color::rgb(0, 0, 255));
        list.stroke_rect(Rect::new(10.0, 10.0, 100.0, 50.0));
        
        assert_eq!(list.len(), 4);
        assert!(list.memory_size() > 0);
    }
    
    #[test]
    fn test_bounds_expansion() {
        let mut list = DisplayList::new(1);
        
        list.fill_rect(Rect::new(0.0, 0.0, 100.0, 100.0));
        list.fill_rect(Rect::new(50.0, 50.0, 100.0, 100.0));
        
        let bounds = list.bounds();
        assert_eq!(bounds.x, 0.0);
        assert_eq!(bounds.y, 0.0);
        assert_eq!(bounds.width, 150.0);
        assert_eq!(bounds.height, 150.0);
    }
    
    #[test]
    fn test_display_list_cache() {
        let mut cache = DisplayListCache::new();
        
        let mut list = DisplayList::new(1);
        list.fill_rect(Rect::new(0.0, 0.0, 100.0, 100.0));
        
        cache.insert(list);
        
        assert!(cache.get(1).is_some());
        assert!(cache.get(999).is_none());
        assert_eq!(cache.stats().hits, 1);
        assert_eq!(cache.stats().misses, 1);
    }
    
    #[test]
    fn test_display_list_builder() {
        let mut builder = DisplayListBuilder::new();
        
        let id = builder.begin();
        builder.list().set_color(Color::BLACK);
        builder.list().fill_rect(Rect::new(0.0, 0.0, 50.0, 50.0));
        
        let list = builder.finish();
        
        assert_eq!(list.id(), id);
        assert_eq!(list.len(), 2);
    }
}
