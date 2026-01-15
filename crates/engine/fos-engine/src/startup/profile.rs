//! Startup Profile
//!
//! Profile-guided initialization based on user patterns.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Frequent origin entry
#[derive(Debug, Clone)]
pub struct FrequentOrigin {
    /// Origin (e.g., "https://example.com")
    pub origin: String,
    /// Visit count
    pub visits: u32,
    /// Last visit timestamp (seconds since epoch)
    pub last_visit: u64,
    /// Average load time
    pub avg_load_time_ms: u32,
}

impl FrequentOrigin {
    pub fn new(origin: &str) -> Self {
        Self {
            origin: origin.to_string(),
            visits: 1,
            last_visit: 0,
            avg_load_time_ms: 0,
        }
    }
    
    /// Record a visit
    pub fn record_visit(&mut self, load_time_ms: u32, timestamp: u64) {
        // Rolling average
        let total = self.avg_load_time_ms as u64 * self.visits as u64 + load_time_ms as u64;
        self.visits += 1;
        self.avg_load_time_ms = (total / self.visits as u64) as u32;
        self.last_visit = timestamp;
    }
    
    /// Calculate priority score (higher = more important to prefetch)
    pub fn priority_score(&self) -> f64 {
        // Weight by recency and frequency
        let frequency_weight = (self.visits as f64).ln().max(1.0);
        frequency_weight
    }
}

/// Startup profile
#[derive(Debug, Clone, Default)]
pub struct StartupProfile {
    /// Frequent origins sorted by priority
    pub frequent_origins: Vec<FrequentOrigin>,
    /// Feature usage stats
    pub feature_usage: HashMap<String, u32>,
    /// Average startup time from previous sessions
    pub avg_startup_ms: u32,
    /// Profile creation timestamp
    pub created: u64,
    /// Last update timestamp
    pub updated: u64,
}

impl StartupProfile {
    /// Create new profile
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Record an origin visit
    pub fn record_visit(&mut self, origin: &str, load_time_ms: u32, timestamp: u64) {
        if let Some(entry) = self.frequent_origins.iter_mut().find(|o| o.origin == origin) {
            entry.record_visit(load_time_ms, timestamp);
        } else {
            let mut entry = FrequentOrigin::new(origin);
            entry.last_visit = timestamp;
            entry.avg_load_time_ms = load_time_ms;
            self.frequent_origins.push(entry);
        }
        
        self.updated = timestamp;
        self.sort_by_priority();
    }
    
    /// Sort origins by priority
    fn sort_by_priority(&mut self) {
        self.frequent_origins.sort_by(|a, b| {
            b.priority_score().partial_cmp(&a.priority_score()).unwrap()
        });
    }
    
    /// Record feature usage
    pub fn record_feature(&mut self, feature: &str) {
        *self.feature_usage.entry(feature.to_string()).or_insert(0) += 1;
    }
    
    /// Get top N origins for prefetch
    pub fn top_origins(&self, n: usize) -> Vec<&str> {
        self.frequent_origins
            .iter()
            .take(n)
            .map(|o| o.origin.as_str())
            .collect()
    }
    
    /// Get frequently used features
    pub fn frequent_features(&self, min_usage: u32) -> Vec<&str> {
        self.feature_usage
            .iter()
            .filter(|&(_, count)| *count >= min_usage)
            .map(|(feature, _)| feature.as_str())
            .collect()
    }
    
    /// Record startup time
    pub fn record_startup(&mut self, startup_ms: u32) {
        // Rolling average
        if self.avg_startup_ms == 0 {
            self.avg_startup_ms = startup_ms;
        } else {
            self.avg_startup_ms = (self.avg_startup_ms * 9 + startup_ms) / 10;
        }
    }
}

/// Profile-guided initializer
#[derive(Debug)]
pub struct ProfileGuidedInit {
    /// Startup profile
    profile: StartupProfile,
    /// DNS prefetch queue
    dns_prefetch_queue: Vec<String>,
    /// Preconnect queue
    preconnect_queue: Vec<String>,
    /// Prefetch started
    prefetch_started: bool,
}

impl ProfileGuidedInit {
    /// Create from profile
    pub fn new(profile: StartupProfile) -> Self {
        Self {
            profile,
            dns_prefetch_queue: Vec::new(),
            preconnect_queue: Vec::new(),
            prefetch_started: false,
        }
    }
    
    /// Create with empty profile
    pub fn empty() -> Self {
        Self::new(StartupProfile::new())
    }
    
    /// Get profile
    pub fn profile(&self) -> &StartupProfile {
        &self.profile
    }
    
    /// Get mutable profile
    pub fn profile_mut(&mut self) -> &mut StartupProfile {
        &mut self.profile
    }
    
    /// Initialize prefetch based on profile
    pub fn init_prefetch(&mut self, max_origins: usize) {
        if self.prefetch_started {
            return;
        }
        
        // Queue DNS prefetch for top origins
        for origin in self.profile.top_origins(max_origins) {
            self.dns_prefetch_queue.push(origin.to_string());
        }
        
        // Queue preconnect for top 3
        for origin in self.profile.top_origins(3) {
            self.preconnect_queue.push(origin.to_string());
        }
        
        self.prefetch_started = true;
    }
    
    /// Get next DNS prefetch origin
    pub fn next_dns_prefetch(&mut self) -> Option<String> {
        self.dns_prefetch_queue.pop()
    }
    
    /// Get next preconnect origin
    pub fn next_preconnect(&mut self) -> Option<String> {
        self.preconnect_queue.pop()
    }
    
    /// Has more prefetch work?
    pub fn has_prefetch_work(&self) -> bool {
        !self.dns_prefetch_queue.is_empty() || !self.preconnect_queue.is_empty()
    }
    
    /// Get DNS prefetch queue length
    pub fn dns_queue_len(&self) -> usize {
        self.dns_prefetch_queue.len()
    }
    
    /// Get preconnect queue length
    pub fn preconnect_queue_len(&self) -> usize {
        self.preconnect_queue.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_frequent_origin() {
        let mut origin = FrequentOrigin::new("https://example.com");
        
        origin.record_visit(100, 1000);
        origin.record_visit(200, 1001);
        
        assert_eq!(origin.visits, 3);
        assert!(origin.avg_load_time_ms > 0);
    }
    
    #[test]
    fn test_startup_profile() {
        let mut profile = StartupProfile::new();
        
        profile.record_visit("https://example.com", 100, 1000);
        profile.record_visit("https://example.com", 150, 1001);
        profile.record_visit("https://google.com", 50, 1002);
        
        let top = profile.top_origins(2);
        assert_eq!(top.len(), 2);
        // example.com should be first (2 visits)
        assert_eq!(top[0], "https://example.com");
    }
    
    #[test]
    fn test_profile_guided_init() {
        let mut profile = StartupProfile::new();
        profile.record_visit("https://a.com", 100, 1000);
        profile.record_visit("https://b.com", 100, 1001);
        profile.record_visit("https://c.com", 100, 1002);
        
        let mut init = ProfileGuidedInit::new(profile);
        init.init_prefetch(5);
        
        assert!(init.dns_queue_len() > 0);
        assert!(init.preconnect_queue_len() > 0);
        
        let dns = init.next_dns_prefetch();
        assert!(dns.is_some());
    }
    
    #[test]
    fn test_feature_usage() {
        let mut profile = StartupProfile::new();
        
        profile.record_feature("devtools");
        profile.record_feature("devtools");
        profile.record_feature("extensions");
        
        let frequent = profile.frequent_features(2);
        assert!(frequent.contains(&"devtools"));
        assert!(!frequent.contains(&"extensions"));
    }
}
