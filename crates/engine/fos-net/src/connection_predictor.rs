//! Connection Predictor
//!
//! Predictive connection management using Bloom filters and navigation models.
//! Enables hover-based preconnect and viewport-aware speculative fetch.

use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

/// Bloom filter size (64 bytes = 512 bits)
const BLOOM_SIZE: usize = 512;

/// Number of hash functions for Bloom filter
const BLOOM_HASHES: usize = 3;

/// Maximum prediction entries
const MAX_PREDICTIONS: usize = 100;

/// Bloom filter for probable host detection
#[derive(Debug, Clone)]
pub struct BloomFilter<const N: usize> {
    bits: [u64; N],
    count: usize,
}

impl<const N: usize> Default for BloomFilter<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> BloomFilter<N> {
    /// Create a new Bloom filter
    pub const fn new() -> Self {
        Self {
            bits: [0; N],
            count: 0,
        }
    }
    
    /// Add an item to the filter
    pub fn insert<T: Hash>(&mut self, item: &T) {
        let hashes = self.compute_hashes(item);
        for h in hashes {
            let idx = (h as usize) / 64;
            let bit = h % 64;
            if idx < N {
                self.bits[idx] |= 1 << bit;
            }
        }
        self.count += 1;
    }
    
    /// Check if item might be in the filter
    pub fn contains<T: Hash>(&self, item: &T) -> bool {
        let hashes = self.compute_hashes(item);
        for h in hashes {
            let idx = (h as usize) / 64;
            let bit = h % 64;
            if idx < N {
                if (self.bits[idx] & (1 << bit)) == 0 {
                    return false;
                }
            }
        }
        true
    }
    
    /// Get approximate count
    pub fn count(&self) -> usize {
        self.count
    }
    
    /// Clear the filter
    pub fn clear(&mut self) {
        self.bits = [0; N];
        self.count = 0;
    }
    
    fn compute_hashes<T: Hash>(&self, item: &T) -> [u64; BLOOM_HASHES] {
        use std::collections::hash_map::DefaultHasher;
        
        let mut hasher = DefaultHasher::new();
        item.hash(&mut hasher);
        let h1 = hasher.finish();
        
        let mut hasher = DefaultHasher::new();
        h1.hash(&mut hasher);
        let h2 = hasher.finish();
        
        let total_bits = (N * 64) as u64;
        [
            h1 % total_bits,
            h2 % total_bits,
            (h1.wrapping_add(h2)) % total_bits,
        ]
    }
}

/// Navigation model using simple Markov chain
#[derive(Debug, Default)]
pub struct NavigationModel {
    /// Transition counts: from -> to -> count
    transitions: HashMap<String, HashMap<String, u32>>,
    /// Total transitions from each state
    totals: HashMap<String, u32>,
    /// Maximum states to track
    max_states: usize,
}

impl NavigationModel {
    /// Create a new navigation model
    pub fn new(max_states: usize) -> Self {
        Self {
            transitions: HashMap::new(),
            totals: HashMap::new(),
            max_states,
        }
    }
    
    /// Record a navigation from one URL to another
    pub fn record(&mut self, from: &str, to: &str) {
        // Normalize URLs
        let from = Self::normalize_url(from);
        let to = Self::normalize_url(to);
        
        // Enforce state limit
        if self.transitions.len() >= self.max_states 
            && !self.transitions.contains_key(&from) {
            return;
        }
        
        *self.transitions
            .entry(from.clone())
            .or_default()
            .entry(to)
            .or_insert(0) += 1;
        
        *self.totals.entry(from).or_insert(0) += 1;
    }
    
    /// Predict likely next navigations from current URL
    pub fn predict(&self, current: &str, min_probability: f64) -> Vec<Prediction> {
        let current = Self::normalize_url(current);
        
        let Some(transitions) = self.transitions.get(&current) else {
            return Vec::new();
        };
        
        let Some(&total) = self.totals.get(&current) else {
            return Vec::new();
        };
        
        if total == 0 {
            return Vec::new();
        }
        
        let mut predictions: Vec<_> = transitions
            .iter()
            .filter_map(|(to, count)| {
                let prob = *count as f64 / total as f64;
                if prob >= min_probability {
                    Some(Prediction {
                        url: to.clone(),
                        probability: prob,
                    })
                } else {
                    None
                }
            })
            .collect();
        
        predictions.sort_by(|a, b| b.probability.partial_cmp(&a.probability).unwrap());
        predictions
    }
    
    /// Get the most likely next URL
    pub fn most_likely(&self, current: &str) -> Option<Prediction> {
        self.predict(current, 0.0).into_iter().next()
    }
    
    /// Clear all learned patterns
    pub fn clear(&mut self) {
        self.transitions.clear();
        self.totals.clear();
    }
    
    fn normalize_url(url: &str) -> String {
        // Remove query params and fragments for pattern matching
        let url = url.split('?').next().unwrap_or(url);
        let url = url.split('#').next().unwrap_or(url);
        url.to_lowercase()
    }
}

/// A predicted navigation
#[derive(Debug, Clone)]
pub struct Prediction {
    /// Predicted URL
    pub url: String,
    /// Probability (0.0 - 1.0)
    pub probability: f64,
}

/// Preconnect hint
#[derive(Debug, Clone)]
pub struct PreconnectHint {
    /// Host to preconnect to
    pub host: String,
    /// Port
    pub port: u16,
    /// Whether to use HTTPS
    pub secure: bool,
    /// Source of the hint
    pub source: HintSource,
    /// When the hint was created
    pub created: Instant,
}

/// Source of a preconnect hint
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HintSource {
    /// From link hover
    Hover,
    /// From navigation prediction
    Prediction,
    /// From link rel=preconnect
    LinkHeader,
    /// From DNS prefetch
    DnsPrefetch,
    /// From viewport analysis
    Viewport,
}

/// Connection predictor for intelligent preconnection
#[derive(Debug)]
pub struct ConnectionPredictor {
    /// Bloom filter for likely hosts
    host_filter: BloomFilter<8>,
    
    /// Navigation model
    nav_model: NavigationModel,
    
    /// Pending preconnects
    pending_preconnects: VecDeque<PreconnectHint>,
    
    /// Active connections being warmed
    warming: HashSet<String>,
    
    /// Hover preconnect enabled
    hover_preconnect: bool,
    
    /// Viewport prefetch enabled  
    viewport_prefetch: bool,
    
    /// Minimum probability for predictions
    min_probability: f64,
    
    /// Statistics
    stats: PredictorStats,
    
    /// Current page URL
    current_url: Option<String>,
    
    /// Viewport URLs (for prefetch)
    viewport_urls: Vec<ViewportUrl>,
}

/// A URL visible in the viewport
#[derive(Debug, Clone)]
pub struct ViewportUrl {
    /// URL
    pub url: String,
    /// Position from top (0.0 = top, 1.0 = bottom of viewport)
    pub position: f64,
    /// Whether it's a navigation link
    pub is_navigation: bool,
}

/// Predictor statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct PredictorStats {
    /// Predictions made
    pub predictions: u64,
    /// Predictions that were correct
    pub hits: u64,
    /// Preconnects initiated
    pub preconnects: u64,
    /// Preconnects that were used
    pub preconnects_used: u64,
    /// Prefetches initiated
    pub prefetches: u64,
    /// Prefetches that were used
    pub prefetches_used: u64,
}

impl PredictorStats {
    /// Get prediction accuracy
    pub fn accuracy(&self) -> f64 {
        if self.predictions == 0 {
            0.0
        } else {
            self.hits as f64 / self.predictions as f64
        }
    }
    
    /// Get preconnect utilization
    pub fn preconnect_utilization(&self) -> f64 {
        if self.preconnects == 0 {
            0.0
        } else {
            self.preconnects_used as f64 / self.preconnects as f64
        }
    }
}

impl Default for ConnectionPredictor {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectionPredictor {
    /// Create a new connection predictor
    pub fn new() -> Self {
        Self {
            host_filter: BloomFilter::new(),
            nav_model: NavigationModel::new(MAX_PREDICTIONS),
            pending_preconnects: VecDeque::new(),
            warming: HashSet::new(),
            hover_preconnect: true,
            viewport_prefetch: true,
            min_probability: 0.3,
            stats: PredictorStats::default(),
            current_url: None,
            viewport_urls: Vec::new(),
        }
    }
    
    /// Enable/disable hover preconnect
    pub fn set_hover_preconnect(&mut self, enabled: bool) {
        self.hover_preconnect = enabled;
    }
    
    /// Enable/disable viewport prefetch
    pub fn set_viewport_prefetch(&mut self, enabled: bool) {
        self.viewport_prefetch = enabled;
    }
    
    /// Set minimum prediction probability
    pub fn set_min_probability(&mut self, prob: f64) {
        self.min_probability = prob.clamp(0.0, 1.0);
    }
    
    /// Record a page navigation
    pub fn on_navigate(&mut self, url: &str) {
        // Record transition
        if let Some(ref from) = self.current_url {
            self.nav_model.record(from, url);
            
            // Check if prediction was correct
            if self.host_filter.contains(&Self::extract_host(url)) {
                self.stats.hits += 1;
            }
        }
        
        // Add host to bloom filter
        let host = Self::extract_host(url);
        self.host_filter.insert(&host);
        
        self.current_url = Some(url.to_string());
        self.viewport_urls.clear();
    }
    
    /// On link hover - trigger preconnect
    pub fn on_link_hover(&mut self, url: &str) -> Option<PreconnectHint> {
        if !self.hover_preconnect {
            return None;
        }
        
        let host = Self::extract_host(url);
        let port = Self::extract_port(url);
        let secure = url.starts_with("https://");
        
        // Don't preconnect if already warming
        if self.warming.contains(&host) {
            return None;
        }
        
        self.warming.insert(host.clone());
        self.stats.preconnects += 1;
        
        Some(PreconnectHint {
            host,
            port,
            secure,
            source: HintSource::Hover,
            created: Instant::now(),
        })
    }
    
    /// Update viewport URLs
    pub fn update_viewport(&mut self, urls: Vec<ViewportUrl>) {
        self.viewport_urls = urls;
    }
    
    /// Get speculative prefetch hints based on viewport
    pub fn get_viewport_hints(&self) -> Vec<PreconnectHint> {
        if !self.viewport_prefetch {
            return Vec::new();
        }
        
        let mut hints = Vec::new();
        
        for vurl in &self.viewport_urls {
            // Prioritize links near top of viewport
            if vurl.position < 0.5 && vurl.is_navigation {
                let host = Self::extract_host(&vurl.url);
                let port = Self::extract_port(&vurl.url);
                let secure = vurl.url.starts_with("https://");
                
                hints.push(PreconnectHint {
                    host,
                    port,
                    secure,
                    source: HintSource::Viewport,
                    created: Instant::now(),
                });
            }
        }
        
        hints
    }
    
    /// Get predicted navigations from current URL
    pub fn get_predictions(&mut self) -> Vec<Prediction> {
        let Some(ref current) = self.current_url else {
            return Vec::new();
        };
        
        self.stats.predictions += 1;
        self.nav_model.predict(current, self.min_probability)
    }
    
    /// Check if a host is likely to be needed
    pub fn is_likely_host(&self, host: &str) -> bool {
        self.host_filter.contains(&host.to_lowercase())
    }
    
    /// Record that a preconnect was used
    pub fn on_preconnect_used(&mut self, host: &str) {
        if self.warming.remove(host) {
            self.stats.preconnects_used += 1;
        }
    }
    
    /// Record that a prefetch was used
    pub fn on_prefetch_used(&mut self) {
        self.stats.prefetches_used += 1;
    }
    
    /// Get statistics
    pub fn stats(&self) -> &PredictorStats {
        &self.stats
    }
    
    /// Clear all predictions and history
    pub fn clear(&mut self) {
        self.host_filter.clear();
        self.nav_model.clear();
        self.pending_preconnects.clear();
        self.warming.clear();
        self.current_url = None;
        self.viewport_urls.clear();
    }
    
    /// Pop next pending preconnect hint
    pub fn pop_preconnect(&mut self) -> Option<PreconnectHint> {
        self.pending_preconnects.pop_front()
    }
    
    fn extract_host(url: &str) -> String {
        let url = url.trim_start_matches("https://")
            .trim_start_matches("http://");
        
        url.split('/')
            .next()
            .unwrap_or(url)
            .split(':')
            .next()
            .unwrap_or(url)
            .to_lowercase()
    }
    
    fn extract_port(url: &str) -> u16 {
        let url = url.trim_start_matches("https://")
            .trim_start_matches("http://");
        
        let host_part = url.split('/').next().unwrap_or(url);
        
        if let Some(port_str) = host_part.split(':').nth(1) {
            port_str.parse().unwrap_or_else(|_| {
                if url.starts_with("https") { 443 } else { 80 }
            })
        } else if url.contains("https://") {
            443
        } else {
            80
        }
    }
}

/// DNS prefetch manager
#[derive(Debug, Default)]
pub struct DnsPrefetcher {
    /// Prefetched hosts
    prefetched: HashSet<String>,
    /// Pending prefetch requests
    pending: VecDeque<String>,
    /// Maximum pending
    max_pending: usize,
}

impl DnsPrefetcher {
    /// Create a new DNS prefetcher
    pub fn new() -> Self {
        Self {
            prefetched: HashSet::new(),
            pending: VecDeque::new(),
            max_pending: 50,
        }
    }
    
    /// Queue a host for DNS prefetch
    pub fn prefetch(&mut self, host: &str) -> bool {
        let host = host.to_lowercase();
        
        if self.prefetched.contains(&host) {
            return false;
        }
        
        if self.pending.len() >= self.max_pending {
            return false;
        }
        
        self.pending.push_back(host);
        true
    }
    
    /// Get next host to prefetch
    pub fn pop(&mut self) -> Option<String> {
        self.pending.pop_front()
    }
    
    /// Mark host as prefetched
    pub fn mark_done(&mut self, host: &str) {
        self.prefetched.insert(host.to_lowercase());
    }
    
    /// Check if host was prefetched
    pub fn is_prefetched(&self, host: &str) -> bool {
        self.prefetched.contains(&host.to_lowercase())
    }
    
    /// Clear all prefetch state
    pub fn clear(&mut self) {
        self.prefetched.clear();
        self.pending.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_bloom_filter() {
        let mut filter: BloomFilter<8> = BloomFilter::new();
        
        filter.insert(&"example.com");
        filter.insert(&"test.com");
        
        assert!(filter.contains(&"example.com"));
        assert!(filter.contains(&"test.com"));
        // May have false positives but should be rare
        assert_eq!(filter.count(), 2);
    }
    
    #[test]
    fn test_navigation_model() {
        let mut model = NavigationModel::new(100);
        
        model.record("https://example.com/", "https://example.com/about");
        model.record("https://example.com/", "https://example.com/about");
        model.record("https://example.com/", "https://example.com/contact");
        
        let predictions = model.predict("https://example.com/", 0.0);
        assert!(!predictions.is_empty());
        
        // About should be most likely (2 vs 1)
        let most_likely = model.most_likely("https://example.com/");
        assert!(most_likely.is_some());
        assert!(most_likely.unwrap().url.contains("about"));
    }
    
    #[test]
    fn test_connection_predictor() {
        let mut predictor = ConnectionPredictor::new();
        
        predictor.on_navigate("https://example.com/");
        predictor.on_navigate("https://example.com/about");
        predictor.on_navigate("https://example.com/");
        predictor.on_navigate("https://example.com/about");
        
        assert!(predictor.is_likely_host("example.com"));
    }
    
    #[test]
    fn test_hover_preconnect() {
        let mut predictor = ConnectionPredictor::new();
        predictor.on_navigate("https://example.com/");
        
        let hint = predictor.on_link_hover("https://other.com/page");
        assert!(hint.is_some());
        
        let hint = hint.unwrap();
        assert_eq!(hint.host, "other.com");
        assert!(hint.secure);
        assert_eq!(hint.source, HintSource::Hover);
    }
    
    #[test]
    fn test_viewport_hints() {
        let mut predictor = ConnectionPredictor::new();
        
        predictor.update_viewport(vec![
            ViewportUrl {
                url: "https://example.com/link1".into(),
                position: 0.2,
                is_navigation: true,
            },
            ViewportUrl {
                url: "https://example.com/link2".into(),
                position: 0.8,
                is_navigation: true,
            },
        ]);
        
        let hints = predictor.get_viewport_hints();
        assert_eq!(hints.len(), 1); // Only link1 is near top
    }
    
    #[test]
    fn test_dns_prefetcher() {
        let mut prefetcher = DnsPrefetcher::new();
        
        assert!(prefetcher.prefetch("example.com"));
        assert!(prefetcher.prefetch("test.com"));
        
        let host = prefetcher.pop().unwrap();
        prefetcher.mark_done(&host);
        
        assert!(prefetcher.is_prefetched(&host));
        assert!(!prefetcher.prefetch(&host)); // Already done
    }
    
    #[test]
    fn test_extract_host() {
        assert_eq!(
            ConnectionPredictor::extract_host("https://example.com/path"),
            "example.com"
        );
        assert_eq!(
            ConnectionPredictor::extract_host("http://test.com:8080/"),
            "test.com"
        );
    }
}
