//! Optimization Integration
//!
//! Integrates fos-engine optimization features: string interning, memory stats.

use fos_engine::{
    StringInterner, InternedString, Viewport,
};

/// Optimization manager for the browser
pub struct OptimizationManager {
    /// String interner for deduplication
    interner: StringInterner,
    /// Viewport for visibility
    viewport: Viewport,
    /// Stats
    stats: OptimizationStats,
}

/// Optimization statistics
#[derive(Debug, Clone, Default)]
pub struct OptimizationStats {
    pub strings_interned: usize,
    pub strings_deduplicated: usize,
    pub visibility_culled: usize,
    pub visibility_visible: usize,
}

impl OptimizationManager {
    /// Create new optimization manager
    pub fn new() -> Self {
        Self {
            interner: StringInterner::new(),
            viewport: Viewport {
                x: 0.0,
                y: 0.0,
                width: 1920.0,
                height: 1080.0,
            },
            stats: OptimizationStats::default(),
        }
    }
    
    // === String Interning ===
    
    /// Intern a string (deduplicate)
    pub fn intern(&mut self, s: &str) -> InternedString {
        let before = self.interner.len();
        let interned = self.interner.intern(s);
        let after = self.interner.len();
        
        self.stats.strings_interned += 1;
        if before == after {
            self.stats.strings_deduplicated += 1;
        }
        
        interned
    }
    
    /// Get interned string count
    pub fn interned_count(&self) -> usize {
        self.interner.len()
    }
    
    // === Visibility Culling ===
    
    /// Update viewport
    pub fn set_viewport(&mut self, x: f32, y: f32, width: f32, height: f32) {
        self.viewport = Viewport { x, y, width, height };
    }
    
    /// Check if element is visible in viewport
    pub fn is_visible(&mut self, x: f32, y: f32, width: f32, height: f32) -> bool {
        let visible = !(x + width < self.viewport.x 
            || x > self.viewport.x + self.viewport.width
            || y + height < self.viewport.y 
            || y > self.viewport.y + self.viewport.height);
        
        if visible {
            self.stats.visibility_visible += 1;
        } else {
            self.stats.visibility_culled += 1;
        }
        
        visible
    }
    
    /// Get current viewport
    pub fn viewport(&self) -> &Viewport {
        &self.viewport
    }
    
    // === Stats ===
    
    /// Get optimization stats
    pub fn stats(&self) -> &OptimizationStats {
        &self.stats
    }
    
    /// Get summary
    pub fn summary(&self) -> OptimizationSummary {
        OptimizationSummary {
            interned_strings: self.interner.len(),
            deduplicated_strings: self.stats.strings_deduplicated,
            culled_elements: self.stats.visibility_culled,
            visible_elements: self.stats.visibility_visible,
        }
    }
    
    /// Reset stats
    pub fn reset_stats(&mut self) {
        self.stats = OptimizationStats::default();
    }
}

impl Default for OptimizationManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Optimization summary
#[derive(Debug, Clone)]
pub struct OptimizationSummary {
    pub interned_strings: usize,
    pub deduplicated_strings: usize,
    pub culled_elements: usize,
    pub visible_elements: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_optimization_creation() {
        let manager = OptimizationManager::new();
        assert_eq!(manager.interned_count(), 0);
    }
    
    #[test]
    fn test_string_interning() {
        let mut manager = OptimizationManager::new();
        
        let s1 = manager.intern("hello");
        let s2 = manager.intern("hello");
        let s3 = manager.intern("world");
        
        assert_eq!(manager.interned_count(), 2); // "hello" and "world"
        assert_eq!(manager.stats().strings_deduplicated, 1);
    }
    
    #[test]
    fn test_visibility() {
        let mut manager = OptimizationManager::new();
        manager.set_viewport(0.0, 0.0, 100.0, 100.0);
        
        assert!(manager.is_visible(10.0, 10.0, 20.0, 20.0)); // Inside
        assert!(!manager.is_visible(200.0, 200.0, 20.0, 20.0)); // Outside
        
        assert_eq!(manager.stats().visibility_visible, 1);
        assert_eq!(manager.stats().visibility_culled, 1);
    }
}
