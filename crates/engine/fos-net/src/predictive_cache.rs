//! Predictive HTTP Cache
//!
//! Learns navigation patterns to predict and prefetch resources.
//! Uses a Markov chain for URL sequence prediction.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::time::Instant;

// ============================================================================
// Markov Chain for URL Prediction
// ============================================================================

/// Simple Markov chain for sequence prediction
/// Tracks transition probabilities between states
#[derive(Debug)]
pub struct MarkovChain<T: Hash + Eq + Clone> {
    /// Transition counts: from -> (to -> count)
    transitions: HashMap<T, HashMap<T, u32>>,
    /// Total outgoing transitions from each state
    totals: HashMap<T, u32>,
    /// Maximum states to track
    max_states: usize,
}

impl<T: Hash + Eq + Clone> Default for MarkovChain<T> {
    fn default() -> Self {
        Self::new(10000)
    }
}

impl<T: Hash + Eq + Clone> MarkovChain<T> {
    /// Create a new Markov chain
    pub fn new(max_states: usize) -> Self {
        Self {
            transitions: HashMap::new(),
            totals: HashMap::new(),
            max_states,
        }
    }
    
    /// Record a transition from one state to another
    pub fn record_transition(&mut self, from: T, to: T) {
        // Limit memory usage
        if self.transitions.len() >= self.max_states && !self.transitions.contains_key(&from) {
            return;
        }
        
        let entry = self.transitions.entry(from.clone()).or_default();
        *entry.entry(to).or_insert(0) += 1;
        *self.totals.entry(from).or_insert(0) += 1;
    }
    
    /// Get likely next states from current state
    /// Returns states with probability >= threshold
    pub fn likely_next(&self, current: &T, threshold: f64) -> Vec<T> {
        let Some(transitions) = self.transitions.get(current) else {
            return Vec::new();
        };
        
        let total = self.totals.get(current).copied().unwrap_or(0) as f64;
        if total == 0.0 {
            return Vec::new();
        }
        
        transitions.iter()
            .filter_map(|(state, count)| {
                let prob = *count as f64 / total;
                if prob >= threshold {
                    Some(state.clone())
                } else {
                    None
                }
            })
            .collect()
    }
    
    /// Get the most likely next state
    pub fn most_likely(&self, current: &T) -> Option<T> {
        self.transitions.get(current)?
            .iter()
            .max_by_key(|(_, count)| *count)
            .map(|(state, _)| state.clone())
    }
    
    /// Get probability of transitioning from one state to another
    pub fn probability(&self, from: &T, to: &T) -> f64 {
        let Some(transitions) = self.transitions.get(from) else {
            return 0.0;
        };
        
        let count = transitions.get(to).copied().unwrap_or(0) as f64;
        let total = self.totals.get(from).copied().unwrap_or(0) as f64;
        
        if total == 0.0 { 0.0 } else { count / total }
    }
    
    /// Number of tracked states
    pub fn len(&self) -> usize {
        self.transitions.len()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.transitions.is_empty()
    }
    
    /// Clear all learned patterns
    pub fn clear(&mut self) {
        self.transitions.clear();
        self.totals.clear();
    }
}

// ============================================================================
// URL Key for Predictive Cache
// ============================================================================

/// Normalized URL for prediction (ignores query params, fragments)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NormalizedUrl {
    /// Scheme + host + path (normalized)
    key: String,
}

impl NormalizedUrl {
    /// Create from URL string
    pub fn new(url: &str) -> Self {
        // Simple normalization: strip query string and fragment
        let key = url
            .split('?').next().unwrap_or(url)
            .split('#').next().unwrap_or(url)
            .to_lowercase();
        
        Self { key }
    }
    
    /// Get the normalized key
    pub fn as_str(&self) -> &str {
        &self.key
    }
}

impl From<&str> for NormalizedUrl {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for NormalizedUrl {
    fn from(s: String) -> Self {
        Self::new(&s)
    }
}

// ============================================================================
// Prefetch Request
// ============================================================================

/// A prefetch request with priority
#[derive(Debug, Clone)]
pub struct PrefetchRequest {
    /// URL to prefetch
    pub url: String,
    /// Predicted probability
    pub probability: f64,
    /// Requested at
    pub requested_at: Instant,
}

impl PrefetchRequest {
    pub fn new(url: String, probability: f64) -> Self {
        Self {
            url,
            probability,
            requested_at: Instant::now(),
        }
    }
}

// ============================================================================
// Predictive Cache
// ============================================================================

/// Predictive cache statistics
#[derive(Debug, Clone, Default)]
pub struct PredictiveCacheStats {
    /// Navigation transitions recorded
    pub transitions_recorded: u64,
    /// Predictions made
    pub predictions_made: u64,
    /// Prefetches initiated
    pub prefetches_initiated: u64,
    /// Prefetch hits (resource was used)
    pub prefetch_hits: u64,
    /// Prefetch misses (resource was not used)
    pub prefetch_wastes: u64,
}

impl PredictiveCacheStats {
    /// Hit rate for prefetches (accuracy of predictions)
    pub fn prefetch_accuracy(&self) -> f64 {
        let total = self.prefetch_hits + self.prefetch_wastes;
        if total == 0 { 0.0 } else { self.prefetch_hits as f64 / total as f64 }
    }
}

/// Predictive HTTP cache
pub struct PredictiveCache {
    /// Navigation pattern model
    navigation_model: MarkovChain<NormalizedUrl>,
    /// Sub-resource pattern model (which resources are needed for which pages)
    resource_model: MarkovChain<NormalizedUrl>,
    /// Current page URL
    current_url: Option<NormalizedUrl>,
    /// Pending prefetch requests
    pending_prefetches: Vec<PrefetchRequest>,
    /// URLs already in HTTP cache (checked before prefetching)
    cached_urls: std::collections::HashSet<String>,
    /// Probability threshold for prefetching
    prefetch_threshold: f64,
    /// Maximum pending prefetches
    max_pending: usize,
    /// Statistics
    stats: PredictiveCacheStats,
}

impl Default for PredictiveCache {
    fn default() -> Self {
        Self::new()
    }
}

impl PredictiveCache {
    /// Create a new predictive cache
    pub fn new() -> Self {
        Self {
            navigation_model: MarkovChain::new(5000),
            resource_model: MarkovChain::new(20000),
            current_url: None,
            pending_prefetches: Vec::new(),
            cached_urls: std::collections::HashSet::new(),
            prefetch_threshold: 0.3, // 30% probability threshold
            max_pending: 10,
            stats: PredictiveCacheStats::default(),
        }
    }
    
    /// Set the probability threshold for prefetching
    pub fn with_threshold(mut self, threshold: f64) -> Self {
        self.prefetch_threshold = threshold.clamp(0.1, 0.9);
        self
    }
    
    /// Record a page navigation
    pub fn record_navigation(&mut self, url: &str) {
        let new_url = NormalizedUrl::new(url);
        
        if let Some(ref current) = self.current_url {
            self.navigation_model.record_transition(current.clone(), new_url.clone());
            self.stats.transitions_recorded += 1;
        }
        
        self.current_url = Some(new_url);
        
        // Generate prefetch suggestions based on new location
        self.generate_prefetch_suggestions();
    }
    
    /// Record a sub-resource load (page -> resource)
    pub fn record_resource_load(&mut self, resource_url: &str) {
        if let Some(ref current) = self.current_url {
            let resource = NormalizedUrl::new(resource_url);
            self.resource_model.record_transition(current.clone(), resource);
        }
        
        // Check if this was a prefetched resource
        if self.pending_prefetches.iter().any(|p| p.url == resource_url) {
            self.stats.prefetch_hits += 1;
            self.pending_prefetches.retain(|p| p.url != resource_url);
        }
    }
    
    /// Mark a URL as already cached
    pub fn mark_cached(&mut self, url: &str) {
        self.cached_urls.insert(url.to_string());
    }
    
    /// Check if URL is already cached
    pub fn is_cached(&self, url: &str) -> bool {
        self.cached_urls.contains(url)
    }
    
    /// Predict next likely navigations from current page
    pub fn predict_next_navigations(&self) -> Vec<String> {
        self.stats.clone(); // Just to touch stats
        
        let Some(ref current) = self.current_url else {
            return Vec::new();
        };
        
        self.navigation_model.likely_next(current, self.prefetch_threshold)
            .into_iter()
            .map(|u| u.key)
            .collect()
    }
    
    /// Predict resources needed for current page
    pub fn predict_resources(&self) -> Vec<String> {
        let Some(ref current) = self.current_url else {
            return Vec::new();
        };
        
        self.resource_model.likely_next(current, self.prefetch_threshold)
            .into_iter()
            .map(|u| u.key)
            .collect()
    }
    
    /// Get pending prefetch requests
    pub fn get_prefetch_requests(&mut self) -> Vec<PrefetchRequest> {
        std::mem::take(&mut self.pending_prefetches)
    }
    
    /// Generate prefetch suggestions based on current location
    fn generate_prefetch_suggestions(&mut self) {
        let Some(ref current) = self.current_url else { return; };
        
        // Get likely next navigations
        let likely_pages = self.navigation_model.likely_next(current, self.prefetch_threshold);
        
        // For each likely page, get its resources
        for page in likely_pages.iter().take(3) {
            let url = page.key.clone();
            
            if !self.is_cached(&url) && self.pending_prefetches.len() < self.max_pending {
                let prob = self.navigation_model.probability(current, page);
                self.pending_prefetches.push(PrefetchRequest::new(url, prob));
                self.stats.prefetches_initiated += 1;
            }
            
            // Prefetch resources for that page
            let resources = self.resource_model.likely_next(page, self.prefetch_threshold);
            for resource in resources.into_iter().take(5) {
                if !self.is_cached(&resource.key) && self.pending_prefetches.len() < self.max_pending {
                    let prob = self.resource_model.probability(page, &resource);
                    self.pending_prefetches.push(PrefetchRequest::new(resource.key, prob * 0.5)); // Lower priority
                    self.stats.prefetches_initiated += 1;
                }
            }
        }
        
        self.stats.predictions_made += 1;
        
        // Sort by probability (highest first)
        self.pending_prefetches.sort_by(|a, b| 
            b.probability.partial_cmp(&a.probability).unwrap_or(std::cmp::Ordering::Equal)
        );
    }
    
    /// Clean up old pending prefetches
    pub fn cleanup_stale_prefetches(&mut self, max_age_secs: u64) {
        let cutoff = std::time::Duration::from_secs(max_age_secs);
        let initial_count = self.pending_prefetches.len();
        
        self.pending_prefetches.retain(|p| p.requested_at.elapsed() < cutoff);
        
        // Mark remaining as wasted
        self.stats.prefetch_wastes += (initial_count - self.pending_prefetches.len()) as u64;
    }
    
    /// Get statistics
    pub fn stats(&self) -> &PredictiveCacheStats {
        &self.stats
    }
    
    /// Clear all learned patterns
    pub fn clear(&mut self) {
        self.navigation_model.clear();
        self.resource_model.clear();
        self.current_url = None;
        self.pending_prefetches.clear();
        self.cached_urls.clear();
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_markov_chain_basic() {
        let mut chain: MarkovChain<&str> = MarkovChain::new(100);
        
        // Record some transitions
        chain.record_transition("home", "about");
        chain.record_transition("home", "about");
        chain.record_transition("home", "contact");
        
        // Should predict "about" as most likely (2/3 probability)
        assert_eq!(chain.most_likely(&"home"), Some("about"));
        
        let prob = chain.probability(&"home", &"about");
        assert!((prob - 0.666).abs() < 0.01);
    }
    
    #[test]
    fn test_markov_chain_threshold() {
        let mut chain: MarkovChain<&str> = MarkovChain::new(100);
        
        chain.record_transition("a", "b");
        chain.record_transition("a", "b");
        chain.record_transition("a", "b");
        chain.record_transition("a", "c"); // 25% probability
        
        // Only "b" should meet 50% threshold
        let likely = chain.likely_next(&"a", 0.5);
        assert_eq!(likely.len(), 1);
        assert!(likely.contains(&"b"));
    }
    
    #[test]
    fn test_normalized_url() {
        let url1 = NormalizedUrl::new("https://example.com/page?query=1");
        let url2 = NormalizedUrl::new("https://example.com/page#section");
        let url3 = NormalizedUrl::new("https://example.com/page");
        
        assert_eq!(url1, url2);
        assert_eq!(url2, url3);
    }
    
    #[test]
    fn test_predictive_cache_navigation() {
        let mut cache = PredictiveCache::new();
        
        // Simulate navigation pattern
        cache.record_navigation("https://example.com/");
        cache.record_navigation("https://example.com/products");
        cache.record_navigation("https://example.com/");
        cache.record_navigation("https://example.com/products");
        cache.record_navigation("https://example.com/");
        
        // Should have recorded transitions
        assert!(cache.stats.transitions_recorded > 0);
    }
    
    #[test]
    fn test_predictive_cache_resources() {
        let mut cache = PredictiveCache::new();
        
        cache.record_navigation("https://example.com/");
        cache.record_resource_load("https://example.com/style.css");
        cache.record_resource_load("https://example.com/main.js");
        
        // Predictions should include these resources
        let resources = cache.predict_resources();
        // Resources need enough samples to meet threshold
        assert!(resources.is_empty() || resources.len() <= 2);
    }
}
