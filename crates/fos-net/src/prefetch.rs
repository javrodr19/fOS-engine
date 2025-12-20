//! Predictive Prefetch (Phase 24.3)
//!
//! Lightweight model predicts next click. Pre-render likely link targets.
//! Pre-fetch nearby resources. Instant perceived navigation.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Link ID for tracking
pub type LinkId = u32;

/// Link prediction model
#[derive(Debug)]
pub struct PredictionModel {
    /// Click counts per link
    click_counts: HashMap<LinkId, u32>,
    /// Transition counts (from_link -> to_link -> count)
    transitions: HashMap<LinkId, HashMap<LinkId, u32>>,
    /// Last clicked link
    last_clicked: Option<LinkId>,
    /// Click timestamps for recency weighting
    click_times: HashMap<LinkId, Instant>,
    /// Total clicks
    total_clicks: u32,
    /// Decay factor for old clicks
    decay_factor: f32,
}

impl Default for PredictionModel {
    fn default() -> Self {
        Self::new()
    }
}

impl PredictionModel {
    pub fn new() -> Self {
        Self {
            click_counts: HashMap::new(),
            transitions: HashMap::new(),
            last_clicked: None,
            click_times: HashMap::new(),
            total_clicks: 0,
            decay_factor: 0.9,
        }
    }
    
    /// Record a link click
    pub fn record_click(&mut self, link_id: LinkId) {
        // Update click count
        *self.click_counts.entry(link_id).or_insert(0) += 1;
        self.click_times.insert(link_id, Instant::now());
        self.total_clicks += 1;
        
        // Update transition
        if let Some(prev) = self.last_clicked {
            let transitions = self.transitions.entry(prev).or_insert_with(HashMap::new);
            *transitions.entry(link_id).or_insert(0) += 1;
        }
        
        self.last_clicked = Some(link_id);
    }
    
    /// Predict next click based on current context
    pub fn predict_next(&self, current: Option<LinkId>) -> Vec<(LinkId, f32)> {
        let mut predictions: HashMap<LinkId, f32> = HashMap::new();
        
        // Base probability from overall click frequency
        let base_weight = 0.3;
        for (&link, &count) in &self.click_counts {
            let prob = (count as f32 / self.total_clicks.max(1) as f32) * base_weight;
            *predictions.entry(link).or_insert(0.0) += prob;
        }
        
        // Transition probability (if we know current link)
        if let Some(curr) = current {
            if let Some(transitions) = self.transitions.get(&curr) {
                let total: u32 = transitions.values().sum();
                let trans_weight = 0.5;
                
                for (&to_link, &count) in transitions {
                    let prob = (count as f32 / total.max(1) as f32) * trans_weight;
                    *predictions.entry(to_link).or_insert(0.0) += prob;
                }
            }
        }
        
        // Recency boost
        let now = Instant::now();
        let recency_weight = 0.2;
        for (&link, &time) in &self.click_times {
            let age = now.duration_since(time).as_secs_f32();
            let boost = (1.0 / (1.0 + age / 60.0)) * recency_weight; // Decay over minutes
            *predictions.entry(link).or_insert(0.0) += boost;
        }
        
        // Sort by probability
        let mut result: Vec<_> = predictions.into_iter().collect();
        result.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        result
    }
    
    /// Get top N predictions
    pub fn top_predictions(&self, current: Option<LinkId>, n: usize) -> Vec<(LinkId, f32)> {
        self.predict_next(current).into_iter().take(n).collect()
    }
    
    /// Apply decay to old data (call periodically)
    pub fn apply_decay(&mut self) {
        for count in self.click_counts.values_mut() {
            *count = (*count as f32 * self.decay_factor) as u32;
        }
        
        for transitions in self.transitions.values_mut() {
            for count in transitions.values_mut() {
                *count = (*count as f32 * self.decay_factor) as u32;
            }
        }
        
        self.total_clicks = (self.total_clicks as f32 * self.decay_factor) as u32;
    }
}

/// Resource URL for prefetching
pub type ResourceUrl = String;

/// Prefetch priority
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    /// Background prefetch (lowest)
    Low = 0,
    /// User might navigate soon
    Medium = 1,
    /// User hovering or likely to click
    High = 2,
    /// User initiated (e.g., explicit prefetch)
    Critical = 3,
}

/// Prefetch request
#[derive(Debug, Clone)]
pub struct PrefetchRequest {
    pub url: ResourceUrl,
    pub priority: Priority,
    pub probability: f32,
    pub created_at: Instant,
}

/// Prefetch result
#[derive(Debug)]
pub enum PrefetchResult {
    /// Successfully prefetched
    Success { url: ResourceUrl, size: usize, duration: Duration },
    /// Failed to prefetch
    Failed { url: ResourceUrl, error: String },
    /// Already cached
    AlreadyCached { url: ResourceUrl },
    /// Cancelled
    Cancelled { url: ResourceUrl },
}

/// Prefetch state for a URL
#[derive(Debug, Clone)]
pub enum PrefetchState {
    /// Pending prefetch
    Pending,
    /// Currently fetching
    Fetching,
    /// Completed
    Completed { size: usize, fetched_at: Instant },
    /// Failed
    Failed { error: String },
}

/// Prefetch manager
#[derive(Debug)]
pub struct PrefetchManager {
    /// Prediction model
    model: PredictionModel,
    /// Link ID to URL mapping
    link_urls: HashMap<LinkId, ResourceUrl>,
    /// Prefetch states
    states: HashMap<ResourceUrl, PrefetchState>,
    /// Pending requests
    pending: Vec<PrefetchRequest>,
    /// Maximum concurrent prefetches
    max_concurrent: usize,
    /// Currently fetching count
    current_fetching: usize,
    /// Statistics
    stats: PrefetchStats,
    /// Prefetch threshold probability
    threshold: f32,
}

/// Prefetch statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct PrefetchStats {
    pub predictions_made: u64,
    pub prefetches_started: u64,
    pub prefetches_completed: u64,
    pub prefetches_failed: u64,
    pub prefetches_used: u64,
    pub bytes_prefetched: u64,
    pub time_saved_ms: u64,
}

impl PrefetchStats {
    pub fn hit_rate(&self) -> f64 {
        if self.prefetches_completed == 0 {
            0.0
        } else {
            self.prefetches_used as f64 / self.prefetches_completed as f64
        }
    }
}

impl Default for PrefetchManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PrefetchManager {
    pub fn new() -> Self {
        Self {
            model: PredictionModel::new(),
            link_urls: HashMap::new(),
            states: HashMap::new(),
            pending: Vec::new(),
            max_concurrent: 2,
            current_fetching: 0,
            stats: PrefetchStats::default(),
            threshold: 0.1,
        }
    }
    
    /// Set prefetch threshold
    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.threshold = threshold;
        self
    }
    
    /// Set max concurrent prefetches
    pub fn with_max_concurrent(mut self, max: usize) -> Self {
        self.max_concurrent = max;
        self
    }
    
    /// Register a link
    pub fn register_link(&mut self, id: LinkId, url: ResourceUrl) {
        self.link_urls.insert(id, url);
    }
    
    /// Record a link click and update predictions
    pub fn on_click(&mut self, link_id: LinkId) {
        self.model.record_click(link_id);
        
        // Check if this was a prefetched resource
        if let Some(url) = self.link_urls.get(&link_id) {
            if let Some(PrefetchState::Completed { .. }) = self.states.get(url) {
                self.stats.prefetches_used += 1;
            }
        }
        
        // Generate new prefetch predictions
        self.update_prefetches(Some(link_id));
    }
    
    /// On hover - boost prediction for this link
    pub fn on_hover(&mut self, link_id: LinkId) {
        // Immediately queue high-priority prefetch
        if let Some(url) = self.link_urls.get(&link_id).cloned() {
            self.queue_prefetch(url, Priority::High, 0.8);
        }
    }
    
    /// Update prefetch queue based on predictions
    fn update_prefetches(&mut self, current: Option<LinkId>) {
        let predictions = self.model.top_predictions(current, 5);
        self.stats.predictions_made += 1;
        
        for (link_id, prob) in predictions {
            if prob >= self.threshold {
                if let Some(url) = self.link_urls.get(&link_id).cloned() {
                    let priority = if prob > 0.5 {
                        Priority::Medium
                    } else {
                        Priority::Low
                    };
                    self.queue_prefetch(url, priority, prob);
                }
            }
        }
    }
    
    /// Queue a prefetch request
    pub fn queue_prefetch(&mut self, url: ResourceUrl, priority: Priority, probability: f32) {
        // Skip if already fetched or fetching
        if let Some(state) = self.states.get(&url) {
            match state {
                PrefetchState::Completed { .. } |
                PrefetchState::Fetching |
                PrefetchState::Pending => return,
                PrefetchState::Failed { .. } => {} // Can retry
            }
        }
        
        // Add to pending
        self.pending.push(PrefetchRequest {
            url: url.clone(),
            priority,
            probability,
            created_at: Instant::now(),
        });
        
        // Sort by priority
        self.pending.sort_by(|a, b| {
            b.priority.cmp(&a.priority)
                .then_with(|| b.probability.partial_cmp(&a.probability).unwrap_or(std::cmp::Ordering::Equal))
        });
        
        self.states.insert(url, PrefetchState::Pending);
    }
    
    /// Get next prefetch request to execute
    pub fn next_request(&mut self) -> Option<PrefetchRequest> {
        if self.current_fetching >= self.max_concurrent {
            return None;
        }
        
        let request = self.pending.pop()?;
        self.states.insert(request.url.clone(), PrefetchState::Fetching);
        self.current_fetching += 1;
        self.stats.prefetches_started += 1;
        
        Some(request)
    }
    
    /// Complete a prefetch
    pub fn complete_prefetch(&mut self, result: PrefetchResult) {
        self.current_fetching = self.current_fetching.saturating_sub(1);
        
        match result {
            PrefetchResult::Success { url, size, .. } => {
                self.states.insert(url, PrefetchState::Completed {
                    size,
                    fetched_at: Instant::now(),
                });
                self.stats.prefetches_completed += 1;
                self.stats.bytes_prefetched += size as u64;
            }
            PrefetchResult::Failed { url, error } => {
                self.states.insert(url, PrefetchState::Failed { error });
                self.stats.prefetches_failed += 1;
            }
            PrefetchResult::AlreadyCached { url } => {
                self.states.insert(url, PrefetchState::Completed {
                    size: 0,
                    fetched_at: Instant::now(),
                });
            }
            PrefetchResult::Cancelled { url } => {
                self.states.remove(&url);
            }
        }
    }
    
    /// Check if URL is prefetched
    pub fn is_prefetched(&self, url: &str) -> bool {
        matches!(self.states.get(url), Some(PrefetchState::Completed { .. }))
    }
    
    /// Get prefetch state
    pub fn state(&self, url: &str) -> Option<&PrefetchState> {
        self.states.get(url)
    }
    
    /// Get statistics
    pub fn stats(&self) -> &PrefetchStats {
        &self.stats
    }
    
    /// Clear old prefetches
    pub fn cleanup(&mut self, max_age: Duration) {
        let now = Instant::now();
        
        self.states.retain(|_, state| {
            match state {
                PrefetchState::Completed { fetched_at, .. } => {
                    now.duration_since(*fetched_at) < max_age
                }
                _ => true,
            }
        });
        
        self.pending.retain(|req| {
            now.duration_since(req.created_at) < max_age
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_prediction_model() {
        let mut model = PredictionModel::new();
        
        // Record some clicks
        model.record_click(1);
        model.record_click(2);
        model.record_click(1);
        model.record_click(1);
        
        // Link 1 should be predicted higher
        let predictions = model.top_predictions(None, 3);
        assert!(!predictions.is_empty());
        assert_eq!(predictions[0].0, 1);
    }
    
    #[test]
    fn test_transitions() {
        let mut model = PredictionModel::new();
        
        // Create a pattern: 1 -> 2 -> 3
        model.record_click(1);
        model.record_click(2);
        model.record_click(1);
        model.record_click(2);
        
        // After clicking 1, should predict 2
        let predictions = model.top_predictions(Some(1), 1);
        assert!(!predictions.is_empty());
        // 2 should be in top predictions after 1
    }
    
    #[test]
    fn test_prefetch_manager() {
        let mut manager = PrefetchManager::new().with_threshold(0.0);
        
        manager.register_link(1, "https://example.com/page1".into());
        manager.register_link(2, "https://example.com/page2".into());
        
        manager.on_click(1);
        manager.on_hover(2);
        
        // Should have pending prefetches
        let request = manager.next_request();
        assert!(request.is_some());
    }
    
    #[test]
    fn test_prefetch_completion() {
        let mut manager = PrefetchManager::new();
        
        manager.queue_prefetch("https://test.com".into(), Priority::High, 0.9);
        
        let request = manager.next_request().unwrap();
        assert_eq!(request.url, "https://test.com");
        
        manager.complete_prefetch(PrefetchResult::Success {
            url: request.url,
            size: 1000,
            duration: Duration::from_millis(50),
        });
        
        assert!(manager.is_prefetched("https://test.com"));
    }
}
