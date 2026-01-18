//! Tile-Based Rendering with Pool
//!
//! Efficient tile-based rendering using pooled allocation for bitmap buffers.
//! Implements viewport-based priority, tile caching, and dirty rect tracking.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

// ============================================================================
// Parallel Tile Rasterization (Phase 5.1)
// ============================================================================

/// Display list item for rasterization
pub trait DisplayItem: Send + Sync {
    /// Render this item to a pixel buffer
    fn render(&self, buffer: &mut [u8], width: u32, height: u32, offset_x: f64, offset_y: f64);
    
    /// Bounds of this item
    fn bounds(&self) -> (f64, f64, f64, f64); // (x, y, width, height)
}

/// Simple display list for tile rasterization
pub struct DisplayList {
    items: Vec<Box<dyn DisplayItem>>,
}

impl DisplayList {
    /// Create an empty display list
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }
    
    /// Add an item to the display list
    pub fn push(&mut self, item: Box<dyn DisplayItem>) {
        self.items.push(item);
    }
    
    /// Get items that intersect a given bounds
    pub fn items_in_bounds(&self, x: f64, y: f64, width: f64, height: f64) -> Vec<&dyn DisplayItem> {
        self.items.iter()
            .filter(|item| {
                let (ix, iy, iw, ih) = item.bounds();
                ix < x + width && ix + iw > x && iy < y + height && iy + ih > y
            })
            .map(|item| item.as_ref())
            .collect()
    }
    
    /// Number of items
    pub fn len(&self) -> usize {
        self.items.len()
    }
    
    /// Is empty?
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl Default for DisplayList {
    fn default() -> Self {
        Self::new()
    }
}

/// Parallel tile rasterization result
#[derive(Debug)]
pub struct RasterResult {
    /// Tile coordinate
    pub coord: TileCoord,
    /// Rendered pixels (RGBA)
    pub pixels: Vec<u8>,
    /// Time to rasterize in microseconds
    pub raster_time_us: u64,
}

/// Rasterize multiple dirty tiles in parallel
/// 
/// This function distributes tile rasterization across multiple threads,
/// significantly improving render performance on multi-core CPUs.
pub fn rasterize_tiles_parallel<F>(
    tiles: &[TileCoord],
    tile_size: u32,
    rasterize_fn: F,
) -> Vec<RasterResult>
where
    F: Fn(TileCoord, &mut [u8], u32) + Send + Sync,
{
    if tiles.is_empty() {
        return Vec::new();
    }
    
    // For small tile counts, rasterize sequentially
    if tiles.len() < 4 {
        return tiles.iter()
            .map(|&coord| {
                let start = Instant::now();
                let len = (tile_size * tile_size * 4) as usize;
                let mut pixels = vec![0u8; len];
                rasterize_fn(coord, &mut pixels, tile_size);
                RasterResult {
                    coord,
                    pixels,
                    raster_time_us: start.elapsed().as_micros() as u64,
                }
            })
            .collect();
    }
    
    // Parallel rasterization using scoped threads
    let num_threads = std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(4)
        .min(tiles.len());
    
    let chunk_size = (tiles.len() + num_threads - 1) / num_threads;
    
    std::thread::scope(|s| {
        let handles: Vec<_> = tiles
            .chunks(chunk_size)
            .map(|chunk| {
                s.spawn(|| {
                    chunk.iter()
                        .map(|&coord| {
                            let start = Instant::now();
                            let len = (tile_size * tile_size * 4) as usize;
                            let mut pixels = vec![0u8; len];
                            rasterize_fn(coord, &mut pixels, tile_size);
                            RasterResult {
                                coord,
                                pixels,
                                raster_time_us: start.elapsed().as_micros() as u64,
                            }
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        
        handles.into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    })
}

/// Parallel tile rasterizer with integrated thread pool
pub struct ParallelTileRasterizer {
    /// Tile size
    tile_size: u32,
    /// Total tiles rendered
    tiles_rendered: AtomicUsize,
    /// Total rasterization time in microseconds
    total_raster_time_us: AtomicUsize,
}

impl ParallelTileRasterizer {
    /// Create a new parallel rasterizer
    pub fn new(tile_size: u32) -> Self {
        Self {
            tile_size,
            tiles_rendered: AtomicUsize::new(0),
            total_raster_time_us: AtomicUsize::new(0),
        }
    }
    
    /// Rasterize dirty tiles from a grid using a display list
    pub fn rasterize_dirty(&self, grid: &TileGrid, display_list: &DisplayList) -> Vec<RasterResult> {
        let dirty_tiles = grid.get_dirty_tiles();
        
        if dirty_tiles.is_empty() {
            return Vec::new();
        }
        
        let tile_size = self.tile_size;
        let results = rasterize_tiles_parallel(&dirty_tiles, tile_size, |coord, buffer, size| {
            // Calculate tile world position
            let tile_x = coord.col as f64 * size as f64;
            let tile_y = coord.row as f64 * size as f64;
            
            // Get items that intersect this tile
            let items = display_list.items_in_bounds(
                tile_x, tile_y, 
                size as f64, size as f64
            );
            
            // Render each item to the tile buffer
            for item in items {
                item.render(buffer, size, size, tile_x, tile_y);
            }
        });
        
        // Update stats
        let total_time: u64 = results.iter().map(|r| r.raster_time_us).sum();
        self.tiles_rendered.fetch_add(results.len(), Ordering::Relaxed);
        self.total_raster_time_us.fetch_add(total_time as usize, Ordering::Relaxed);
        
        results
    }
    
    /// Get number of tiles rendered
    pub fn tiles_rendered(&self) -> usize {
        self.tiles_rendered.load(Ordering::Relaxed)
    }
    
    /// Get average rasterization time per tile
    pub fn avg_raster_time_us(&self) -> f64 {
        let tiles = self.tiles_rendered.load(Ordering::Relaxed);
        let time = self.total_raster_time_us.load(Ordering::Relaxed);
        if tiles > 0 {
            time as f64 / tiles as f64
        } else {
            0.0
        }
    }
}

/// Tile ID type
pub type TileId = u64;

/// Default tile size in pixels
pub const DEFAULT_TILE_SIZE: u32 = 256;

/// Tile state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileState {
    /// Tile needs rendering
    Dirty,
    /// Tile is being rendered
    Rendering,
    /// Tile is ready for display
    Ready,
    /// Tile is cached but not currently needed
    Cached,
}

/// Tile grid coordinates
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileCoord {
    pub col: u32,
    pub row: u32,
}

impl TileCoord {
    pub fn new(col: u32, row: u32) -> Self {
        Self { col, row }
    }
}

/// Tile priority based on viewport distance
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TilePriority {
    /// In viewport - render immediately
    Immediate = 0,
    /// Adjacent to viewport - render next
    Adjacent = 1,
    /// Near viewport - render when idle
    Near = 2,
    /// Prefetch zone
    Prefetch = 3,
    /// Low priority
    Low = 4,
}

/// A renderable tile
#[derive(Debug)]
pub struct RenderTile {
    /// Tile coordinates
    pub coord: TileCoord,
    /// Tile size
    pub size: u32,
    /// Current state
    pub state: TileState,
    /// Current priority
    pub priority: TilePriority,
    /// Pixel data (RGBA)
    pub pixels: Vec<u8>,
    /// GPU texture handle (if uploaded)
    pub texture_handle: Option<u64>,
    /// Last access time
    pub last_access: Instant,
    /// Generation (for cache invalidation)
    pub generation: u32,
}

impl RenderTile {
    /// Create a new tile
    pub fn new(coord: TileCoord, size: u32) -> Self {
        Self {
            coord,
            size,
            state: TileState::Dirty,
            priority: TilePriority::Low,
            pixels: Vec::new(),
            texture_handle: None,
            last_access: Instant::now(),
            generation: 0,
        }
    }
    
    /// Allocate pixel buffer
    pub fn allocate(&mut self) {
        let len = (self.size * self.size * 4) as usize;
        if self.pixels.len() != len {
            self.pixels = vec![0u8; len];
        }
    }
    
    /// Clear pixel buffer
    pub fn clear(&mut self) {
        self.pixels.fill(0);
    }
    
    /// Get pixel buffer for rendering
    pub fn get_buffer_mut(&mut self) -> &mut [u8] {
        self.allocate();
        &mut self.pixels
    }
    
    /// Mark as dirty
    pub fn invalidate(&mut self) {
        self.state = TileState::Dirty;
        self.generation += 1;
    }
    
    /// World position (x, y)
    pub fn world_position(&self) -> (f64, f64) {
        let x = self.coord.col as f64 * self.size as f64;
        let y = self.coord.row as f64 * self.size as f64;
        (x, y)
    }
    
    /// Memory size in bytes
    pub fn memory_size(&self) -> usize {
        std::mem::size_of::<Self>() + self.pixels.len()
    }
}

/// Tile pool for reusing tile buffers
#[derive(Debug)]
pub struct TilePool {
    /// Free buffers indexed by size
    free_buffers: HashMap<u32, Vec<Vec<u8>>>,
    /// Maximum buffers per size
    max_per_size: usize,
    /// Total pooled bytes
    total_bytes: usize,
    /// Maximum total bytes
    max_bytes: usize,
    /// Stats
    hits: u64,
    misses: u64,
}

impl TilePool {
    /// Create a new tile pool
    pub fn new(max_per_size: usize, max_bytes: usize) -> Self {
        Self {
            free_buffers: HashMap::new(),
            max_per_size,
            total_bytes: 0,
            max_bytes,
            hits: 0,
            misses: 0,
        }
    }
    
    /// Create with default settings (32 per size, 64MB max)
    pub fn with_defaults() -> Self {
        Self::new(32, 64 * 1024 * 1024)
    }
    
    /// Checkout a buffer for the given tile size
    pub fn checkout(&mut self, size: u32) -> Vec<u8> {
        let len = (size * size * 4) as usize;
        
        if let Some(pool) = self.free_buffers.get_mut(&size) {
            if let Some(mut buffer) = pool.pop() {
                self.hits += 1;
                self.total_bytes -= buffer.len();
                buffer.fill(0);
                buffer.resize(len, 0);
                return buffer;
            }
        }
        
        self.misses += 1;
        vec![0u8; len]
    }
    
    /// Return a buffer to the pool
    pub fn checkin(&mut self, size: u32, buffer: Vec<u8>) {
        let bytes = buffer.len();
        
        // Check limits
        if self.total_bytes + bytes > self.max_bytes {
            return; // Drop buffer
        }
        
        let pool = self.free_buffers.entry(size).or_insert_with(Vec::new);
        if pool.len() >= self.max_per_size {
            return; // Drop buffer
        }
        
        self.total_bytes += bytes;
        pool.push(buffer);
    }
    
    /// Clear all pooled buffers
    pub fn clear(&mut self) {
        self.free_buffers.clear();
        self.total_bytes = 0;
    }
    
    /// Get pool stats
    pub fn stats(&self) -> TilePoolStats {
        TilePoolStats {
            total_bytes: self.total_bytes,
            max_bytes: self.max_bytes,
            hits: self.hits,
            misses: self.misses,
            hit_rate: if self.hits + self.misses > 0 {
                self.hits as f64 / (self.hits + self.misses) as f64
            } else {
                0.0
            },
            num_sizes: self.free_buffers.len(),
            num_buffers: self.free_buffers.values().map(|p| p.len()).sum(),
        }
    }
}

impl Default for TilePool {
    fn default() -> Self {
        Self::with_defaults()
    }
}

/// Tile pool statistics
#[derive(Debug, Clone)]
pub struct TilePoolStats {
    pub total_bytes: usize,
    pub max_bytes: usize,
    pub hits: u64,
    pub misses: u64,
    pub hit_rate: f64,
    pub num_sizes: usize,
    pub num_buffers: usize,
}

/// Viewport for tile priority calculation
#[derive(Debug, Clone, Copy, Default)]
pub struct TileViewport {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub scale: f64,
}

impl TileViewport {
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self { x, y, width, height, scale: 1.0 }
    }
    
    /// Check if tile intersects viewport
    pub fn intersects(&self, tile: &RenderTile) -> bool {
        let (tx, ty) = tile.world_position();
        let ts = tile.size as f64;
        
        tx < self.x + self.width &&
        tx + ts > self.x &&
        ty < self.y + self.height &&
        ty + ts > self.y
    }
    
    /// Calculate priority for a tile
    pub fn priority_for(&self, tile: &RenderTile) -> TilePriority {
        let (tx, ty) = tile.world_position();
        let ts = tile.size as f64;
        let tile_cx = tx + ts / 2.0;
        let tile_cy = ty + ts / 2.0;
        
        let vp_cx = self.x + self.width / 2.0;
        let vp_cy = self.y + self.height / 2.0;
        
        // Distance from viewport center
        let dx = (tile_cx - vp_cx).abs();
        let dy = (tile_cy - vp_cy).abs();
        
        let half_w = self.width / 2.0;
        let half_h = self.height / 2.0;
        
        // In viewport
        if dx <= half_w + ts / 2.0 && dy <= half_h + ts / 2.0 {
            return TilePriority::Immediate;
        }
        
        // Adjacent (within 1 tile)
        if dx <= half_w + ts * 1.5 && dy <= half_h + ts * 1.5 {
            return TilePriority::Adjacent;
        }
        
        // Near (within 2 tiles)
        if dx <= half_w + ts * 2.5 && dy <= half_h + ts * 2.5 {
            return TilePriority::Near;
        }
        
        // Prefetch (within 4 tiles)
        if dx <= half_w + ts * 4.5 && dy <= half_h + ts * 4.5 {
            return TilePriority::Prefetch;
        }
        
        TilePriority::Low
    }
}

/// Tile grid for managing tiles
#[derive(Debug)]
pub struct TileGrid {
    /// Tiles indexed by coordinate
    tiles: HashMap<TileCoord, RenderTile>,
    /// Tile size
    tile_size: u32,
    /// Content width
    content_width: u32,
    /// Content height
    content_height: u32,
    /// Current viewport
    viewport: TileViewport,
    /// Generation counter
    generation: u32,
}

impl TileGrid {
    /// Create a new tile grid
    pub fn new(content_width: u32, content_height: u32, tile_size: u32) -> Self {
        Self {
            tiles: HashMap::new(),
            tile_size,
            content_width,
            content_height,
            viewport: TileViewport::default(),
            generation: 0,
        }
    }
    
    /// Create with default tile size
    pub fn with_size(content_width: u32, content_height: u32) -> Self {
        Self::new(content_width, content_height, DEFAULT_TILE_SIZE)
    }
    
    /// Number of columns
    pub fn cols(&self) -> u32 {
        (self.content_width + self.tile_size - 1) / self.tile_size
    }
    
    /// Number of rows
    pub fn rows(&self) -> u32 {
        (self.content_height + self.tile_size - 1) / self.tile_size
    }
    
    /// Update viewport
    pub fn set_viewport(&mut self, viewport: TileViewport) {
        self.viewport = viewport;
        self.update_priorities();
    }
    
    /// Get or create tile at coordinate
    pub fn get_or_create(&mut self, coord: TileCoord) -> &mut RenderTile {
        self.tiles.entry(coord).or_insert_with(|| {
            let mut tile = RenderTile::new(coord, self.tile_size);
            tile.priority = self.viewport.priority_for(&tile);
            tile.generation = self.generation;
            tile
        })
    }
    
    /// Get tile at coordinate
    pub fn get(&self, coord: TileCoord) -> Option<&RenderTile> {
        self.tiles.get(&coord)
    }
    
    /// Get mutable tile
    pub fn get_mut(&mut self, coord: TileCoord) -> Option<&mut RenderTile> {
        self.tiles.get_mut(&coord)
    }
    
    /// Invalidate all tiles
    pub fn invalidate_all(&mut self) {
        self.generation += 1;
        for tile in self.tiles.values_mut() {
            tile.invalidate();
        }
    }
    
    /// Invalidate tiles in a rect
    pub fn invalidate_rect(&mut self, x: f64, y: f64, width: f64, height: f64) {
        let start_col = (x / self.tile_size as f64).floor() as u32;
        let start_row = (y / self.tile_size as f64).floor() as u32;
        let end_col = ((x + width) / self.tile_size as f64).ceil() as u32;
        let end_row = ((y + height) / self.tile_size as f64).ceil() as u32;
        
        for row in start_row..end_row.min(self.rows()) {
            for col in start_col..end_col.min(self.cols()) {
                if let Some(tile) = self.tiles.get_mut(&TileCoord::new(col, row)) {
                    tile.invalidate();
                }
            }
        }
    }
    
    fn update_priorities(&mut self) {
        for tile in self.tiles.values_mut() {
            tile.priority = self.viewport.priority_for(tile);
        }
    }
    
    /// Get tiles that need rendering, sorted by priority
    pub fn get_dirty_tiles(&self) -> Vec<TileCoord> {
        let mut dirty: Vec<_> = self.tiles.iter()
            .filter(|(_, t)| t.state == TileState::Dirty)
            .map(|(c, t)| (*c, t.priority))
            .collect();
        
        dirty.sort_by_key(|(_, p)| *p);
        dirty.into_iter().map(|(c, _)| c).collect()
    }
    
    /// Get visible tiles
    pub fn get_visible_tiles(&self) -> Vec<TileCoord> {
        self.tiles.iter()
            .filter(|(_, t)| self.viewport.intersects(t))
            .map(|(&c, _)| c)
            .collect()
    }
    
    /// Number of tiles
    pub fn len(&self) -> usize {
        self.tiles.len()
    }
    
    /// Is empty?
    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }
    
    /// Total memory usage
    pub fn memory_usage(&self) -> usize {
        self.tiles.values().map(|t| t.memory_size()).sum()
    }
}

/// Tile renderer with integrated pool
#[derive(Debug)]
pub struct TileRenderer {
    /// Tile grid
    grid: TileGrid,
    /// Buffer pool
    pool: TilePool,
    /// Pending render queue
    render_queue: Vec<TileCoord>,
    /// Stats
    tiles_rendered: u64,
    tiles_cached: u64,
}

impl TileRenderer {
    /// Create a new tile renderer
    pub fn new(content_width: u32, content_height: u32) -> Self {
        Self {
            grid: TileGrid::with_size(content_width, content_height),
            pool: TilePool::with_defaults(),
            render_queue: Vec::new(),
            tiles_rendered: 0,
            tiles_cached: 0,
        }
    }
    
    /// Create with custom tile size
    pub fn with_tile_size(content_width: u32, content_height: u32, tile_size: u32) -> Self {
        Self {
            grid: TileGrid::new(content_width, content_height, tile_size),
            pool: TilePool::with_defaults(),
            render_queue: Vec::new(),
            tiles_rendered: 0,
            tiles_cached: 0,
        }
    }
    
    /// Set viewport
    pub fn set_viewport(&mut self, x: f64, y: f64, width: f64, height: f64) {
        self.grid.set_viewport(TileViewport::new(x, y, width, height));
        self.update_render_queue();
    }
    
    /// Invalidate a rect
    pub fn invalidate(&mut self, x: f64, y: f64, width: f64, height: f64) {
        self.grid.invalidate_rect(x, y, width, height);
        self.update_render_queue();
    }
    
    /// Invalidate all tiles
    pub fn invalidate_all(&mut self) {
        self.grid.invalidate_all();
        self.update_render_queue();
    }
    
    fn update_render_queue(&mut self) {
        self.render_queue = self.grid.get_dirty_tiles();
    }
    
    /// Get next tile to render
    pub fn next_tile(&mut self) -> Option<TileCoord> {
        self.render_queue.pop()
    }
    
    /// Prepare tile for rendering (allocates buffer from pool)
    pub fn prepare_tile(&mut self, coord: TileCoord) -> Option<&mut RenderTile> {
        let tile = self.grid.get_or_create(coord);
        
        if tile.pixels.is_empty() {
            tile.pixels = self.pool.checkout(tile.size);
        }
        
        tile.state = TileState::Rendering;
        tile.last_access = Instant::now();
        Some(tile)
    }
    
    /// Mark tile as rendered
    pub fn complete_tile(&mut self, coord: TileCoord) {
        if let Some(tile) = self.grid.get_mut(coord) {
            tile.state = TileState::Ready;
            self.tiles_rendered += 1;
        }
    }
    
    /// Return tile buffer to pool
    pub fn release_tile(&mut self, coord: TileCoord) {
        if let Some(tile) = self.grid.get_mut(coord) {
            if !tile.pixels.is_empty() {
                let buffer = std::mem::take(&mut tile.pixels);
                self.pool.checkin(tile.size, buffer);
                self.tiles_cached += 1;
            }
        }
    }
    
    /// Get tile for compositing
    pub fn get_tile(&self, coord: TileCoord) -> Option<&RenderTile> {
        self.grid.get(coord)
    }
    
    /// Get visible tiles for compositing
    pub fn get_visible_tiles(&self) -> Vec<&RenderTile> {
        self.grid.get_visible_tiles()
            .iter()
            .filter_map(|c| self.grid.get(*c))
            .filter(|t| t.state == TileState::Ready)
            .collect()
    }
    
    /// Has pending tiles to render?
    pub fn has_pending(&self) -> bool {
        !self.render_queue.is_empty()
    }
    
    /// Number of pending tiles
    pub fn pending_count(&self) -> usize {
        self.render_queue.len()
    }
    
    /// Get renderer stats
    pub fn stats(&self) -> TileRendererStats {
        TileRendererStats {
            total_tiles: self.grid.len(),
            tiles_rendered: self.tiles_rendered,
            tiles_cached: self.tiles_cached,
            pending_tiles: self.render_queue.len(),
            memory_usage: self.grid.memory_usage(),
            pool_stats: self.pool.stats(),
        }
    }
}

/// Tile renderer statistics
#[derive(Debug, Clone)]
pub struct TileRendererStats {
    pub total_tiles: usize,
    pub tiles_rendered: u64,
    pub tiles_cached: u64,
    pub pending_tiles: usize,
    pub memory_usage: usize,
    pub pool_stats: TilePoolStats,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_tile_grid() {
        let mut grid = TileGrid::with_size(1024, 768);
        
        assert_eq!(grid.cols(), 4); // 1024/256 = 4
        assert_eq!(grid.rows(), 3); // 768/256 = 3
        
        let tile = grid.get_or_create(TileCoord::new(0, 0));
        assert_eq!(tile.state, TileState::Dirty);
    }
    
    #[test]
    fn test_tile_pool() {
        let mut pool = TilePool::with_defaults();
        
        let buf1 = pool.checkout(256);
        assert_eq!(buf1.len(), 256 * 256 * 4);
        assert_eq!(pool.stats().misses, 1);
        
        pool.checkin(256, buf1);
        
        let _buf2 = pool.checkout(256);
        assert_eq!(pool.stats().hits, 1);
    }
    
    #[test]
    fn test_tile_priority() {
        let viewport = TileViewport::new(100.0, 100.0, 800.0, 600.0);
        
        // Tile in viewport
        let tile1 = RenderTile::new(TileCoord::new(1, 1), 256);
        assert_eq!(viewport.priority_for(&tile1), TilePriority::Immediate);
        
        // Tile far away
        let tile2 = RenderTile::new(TileCoord::new(20, 20), 256);
        assert_eq!(viewport.priority_for(&tile2), TilePriority::Low);
    }
    
    #[test]
    fn test_tile_renderer() {
        let mut renderer = TileRenderer::new(1024, 768);
        
        // Set viewport
        renderer.set_viewport(0.0, 0.0, 800.0, 600.0);
        
        // Prepare and complete a tile (tiles are created on demand)
        let coord = TileCoord::new(0, 0);
        renderer.prepare_tile(coord);
        renderer.complete_tile(coord);
        
        assert_eq!(renderer.stats().tiles_rendered, 1);
        assert_eq!(renderer.stats().total_tiles, 1);
    }
    
    #[test]
    fn test_invalidation() {
        let mut grid = TileGrid::with_size(1024, 768);
        
        // Create some tiles
        grid.get_or_create(TileCoord::new(0, 0)).state = TileState::Ready;
        grid.get_or_create(TileCoord::new(1, 0)).state = TileState::Ready;
        
        // Invalidate a rect
        grid.invalidate_rect(100.0, 0.0, 100.0, 100.0);
        
        // Tile at (0, 0) should be dirty
        assert_eq!(grid.get(TileCoord::new(0, 0)).unwrap().state, TileState::Dirty);
    }
}
