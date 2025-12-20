//! Attribute Access Tracking (Phase 24.2)
//!
//! Track which attributes ever accessed. On re-parse, skip never-accessed.
//! Learn per-site patterns. Adaptive optimization.

use std::collections::{HashMap, HashSet};

/// Tracked attribute access
#[derive(Debug, Clone)]
pub struct TrackedAttribute {
    /// Attribute name hash
    pub name_hash: u32,
    /// Full attribute name
    pub name: Box<str>,
    /// Access count
    pub access_count: u32,
    /// Was ever read
    pub was_read: bool,
    /// Was parsed (for lazy parsing)
    pub was_parsed: bool,
}

/// Access pattern for a site
#[derive(Debug, Clone)]
pub struct SiteAccessPattern {
    /// Domain
    pub domain: Box<str>,
    /// Attributes that are commonly accessed
    pub commonly_accessed: HashSet<u32>,
    /// Attributes rarely accessed (can skip parsing)
    pub rarely_accessed: HashSet<u32>,
    /// Sample count
    pub sample_count: u32,
}

impl SiteAccessPattern {
    pub fn new(domain: &str) -> Self {
        Self {
            domain: domain.into(),
            commonly_accessed: HashSet::new(),
            rarely_accessed: HashSet::new(),
            sample_count: 0,
        }
    }
    
    /// Should skip parsing this attribute?
    pub fn should_skip(&self, name_hash: u32) -> bool {
        self.sample_count > 10 && self.rarely_accessed.contains(&name_hash)
    }
    
    /// Should eagerly parse this attribute?
    pub fn should_parse(&self, name_hash: u32) -> bool {
        self.commonly_accessed.contains(&name_hash)
    }
}

/// Statistics for attribute access
#[derive(Debug, Clone, Copy, Default)]
pub struct AccessStats {
    pub total_attributes: u64,
    pub accessed_attributes: u64,
    pub never_accessed: u64,
    pub parsing_saved: u64,
}

impl AccessStats {
    pub fn access_rate(&self) -> f64 {
        if self.total_attributes == 0 {
            0.0
        } else {
            self.accessed_attributes as f64 / self.total_attributes as f64
        }
    }
    
    pub fn savings_rate(&self) -> f64 {
        if self.total_attributes == 0 {
            0.0
        } else {
            self.never_accessed as f64 / self.total_attributes as f64
        }
    }
}

/// Attribute access tracker
#[derive(Debug)]
pub struct AttributeAccessTracker {
    /// Tracked attributes by name hash
    attributes: HashMap<u32, TrackedAttribute>,
    /// Site patterns
    site_patterns: HashMap<Box<str>, SiteAccessPattern>,
    /// Current domain
    current_domain: Option<Box<str>>,
    /// Statistics
    stats: AccessStats,
    /// Access threshold for "commonly accessed"
    common_threshold: u32,
}

impl Default for AttributeAccessTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl AttributeAccessTracker {
    pub fn new() -> Self {
        Self {
            attributes: HashMap::new(),
            site_patterns: HashMap::new(),
            current_domain: None,
            stats: AccessStats::default(),
            common_threshold: 5,
        }
    }
    
    /// Set current domain
    pub fn set_domain(&mut self, domain: &str) {
        self.current_domain = Some(domain.into());
        self.site_patterns.entry(domain.into())
            .or_insert_with(|| SiteAccessPattern::new(domain));
    }
    
    /// Register an attribute (when parsing)
    pub fn register(&mut self, name_hash: u32, name: &str) {
        self.stats.total_attributes += 1;
        
        self.attributes.entry(name_hash).or_insert_with(|| TrackedAttribute {
            name_hash,
            name: name.into(),
            access_count: 0,
            was_read: false,
            was_parsed: false,
        });
    }
    
    /// Record attribute access
    pub fn record_access(&mut self, name_hash: u32) {
        if let Some(attr) = self.attributes.get_mut(&name_hash) {
            attr.access_count += 1;
            if !attr.was_read {
                attr.was_read = true;
                self.stats.accessed_attributes += 1;
            }
        }
        
        // Update site pattern
        if let Some(ref domain) = self.current_domain {
            if let Some(pattern) = self.site_patterns.get_mut(domain) {
                if self.attributes.get(&name_hash)
                    .map(|a| a.access_count >= self.common_threshold)
                    .unwrap_or(false)
                {
                    pattern.commonly_accessed.insert(name_hash);
                    pattern.rarely_accessed.remove(&name_hash);
                }
            }
        }
    }
    
    /// Record attribute parsed
    pub fn record_parsed(&mut self, name_hash: u32) {
        if let Some(attr) = self.attributes.get_mut(&name_hash) {
            attr.was_parsed = true;
        }
    }
    
    /// Check if should skip parsing (based on learned patterns)
    pub fn should_skip(&self, name_hash: u32) -> bool {
        if let Some(ref domain) = self.current_domain {
            if let Some(pattern) = self.site_patterns.get(domain) {
                return pattern.should_skip(name_hash);
            }
        }
        false
    }
    
    /// Finalize page tracking (update stats)
    pub fn finalize_page(&mut self) {
        // Count never-accessed attributes
        for attr in self.attributes.values() {
            if !attr.was_read {
                self.stats.never_accessed += 1;
            }
        }
        
        // Update site pattern
        if let Some(ref domain) = self.current_domain {
            if let Some(pattern) = self.site_patterns.get_mut(domain) {
                pattern.sample_count += 1;
                
                // Update rarely accessed set
                for attr in self.attributes.values() {
                    if !attr.was_read {
                        pattern.rarely_accessed.insert(attr.name_hash);
                    }
                }
            }
        }
        
        // Clear per-page data
        self.attributes.clear();
    }
    
    /// Get never-accessed attribute names
    pub fn never_accessed(&self) -> Vec<&str> {
        self.attributes.values()
            .filter(|a| !a.was_read)
            .map(|a| a.name.as_ref())
            .collect()
    }
    
    /// Get commonly accessed attribute names
    pub fn commonly_accessed(&self) -> Vec<&str> {
        self.attributes.values()
            .filter(|a| a.access_count >= self.common_threshold)
            .map(|a| a.name.as_ref())
            .collect()
    }
    
    /// Get statistics
    pub fn stats(&self) -> &AccessStats {
        &self.stats
    }
    
    /// Get site pattern
    pub fn get_site_pattern(&self, domain: &str) -> Option<&SiteAccessPattern> {
        self.site_patterns.get(domain)
    }
    
    /// Hash an attribute name
    pub fn hash_name(name: &str) -> u32 {
        let mut hash = 0u32;
        for byte in name.bytes() {
            hash = hash.wrapping_mul(31).wrapping_add(byte as u32);
        }
        hash
    }
}

/// Commonly skipped attribute prefixes
pub const SKIPPABLE_PREFIXES: &[&str] = &[
    "data-",
    "aria-",
    "ng-",
    "v-",
    "x-",
];

/// Check if attribute name is in skippable category
pub fn is_likely_skippable(name: &str) -> bool {
    SKIPPABLE_PREFIXES.iter().any(|p| name.starts_with(p))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_attribute_tracking() {
        let mut tracker = AttributeAccessTracker::new();
        
        let id_hash = AttributeAccessTracker::hash_name("id");
        let data_hash = AttributeAccessTracker::hash_name("data-tracking");
        
        tracker.register(id_hash, "id");
        tracker.register(data_hash, "data-tracking");
        
        // Access id but not data-tracking
        tracker.record_access(id_hash);
        
        assert_eq!(tracker.stats().accessed_attributes, 1);
        
        let never = tracker.never_accessed();
        assert!(never.contains(&"data-tracking"));
        assert!(!never.contains(&"id"));
    }
    
    #[test]
    fn test_site_patterns() {
        let mut tracker = AttributeAccessTracker::new();
        tracker.set_domain("example.com");
        
        let id_hash = AttributeAccessTracker::hash_name("id");
        let data_hash = AttributeAccessTracker::hash_name("data-test");
        
        // Simulate multiple page loads
        for _ in 0..15 {
            tracker.register(id_hash, "id");
            tracker.register(data_hash, "data-test");
            
            // Always access id, never access data-test
            for _ in 0..5 {
                tracker.record_access(id_hash);
            }
            
            tracker.finalize_page();
        }
        
        let pattern = tracker.get_site_pattern("example.com").unwrap();
        assert!(pattern.commonly_accessed.contains(&id_hash));
        assert!(pattern.rarely_accessed.contains(&data_hash));
    }
    
    #[test]
    fn test_skippable_check() {
        assert!(is_likely_skippable("data-id"));
        assert!(is_likely_skippable("aria-label"));
        assert!(is_likely_skippable("ng-click"));
        assert!(!is_likely_skippable("class"));
        assert!(!is_likely_skippable("id"));
    }
}
