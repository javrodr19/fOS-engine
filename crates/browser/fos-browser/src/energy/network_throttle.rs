//! Network Throttling for Background Tabs
//!
//! Limits network usage for background tabs to save power and bandwidth.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Network policy for a tab
#[derive(Debug, Clone)]
pub struct BackgroundNetworkPolicy {
    /// Maximum concurrent requests
    pub max_concurrent_requests: usize,
    /// Bandwidth limit in bytes per second (None = unlimited)
    pub bandwidth_limit: Option<u64>,
    /// Request delay for throttling
    pub request_delay: Duration,
    /// Allow critical requests (service workers, etc)
    pub allow_critical: bool,
}

impl Default for BackgroundNetworkPolicy {
    fn default() -> Self {
        Self::focused()
    }
}

impl BackgroundNetworkPolicy {
    /// Policy for focused tabs (no limits)
    pub fn focused() -> Self {
        Self {
            max_concurrent_requests: 10,
            bandwidth_limit: None,
            request_delay: Duration::ZERO,
            allow_critical: true,
        }
    }
    
    /// Policy for visible but unfocused tabs
    pub fn visible() -> Self {
        Self {
            max_concurrent_requests: 6,
            bandwidth_limit: Some(1_000_000), // 1 MB/s
            request_delay: Duration::from_millis(10),
            allow_critical: true,
        }
    }
    
    /// Policy for background tabs
    pub fn background() -> Self {
        Self {
            max_concurrent_requests: 2,
            bandwidth_limit: Some(100_000), // 100 KB/s
            request_delay: Duration::from_millis(100),
            allow_critical: true,
        }
    }
    
    /// Policy for hibernated tabs
    pub fn hibernated() -> Self {
        Self {
            max_concurrent_requests: 0,
            bandwidth_limit: Some(0),
            request_delay: Duration::MAX,
            allow_critical: false,
        }
    }
    
    /// Get policy based on tab state
    pub fn for_tab(focused: bool, visible: bool, hibernated: bool) -> Self {
        if hibernated {
            Self::hibernated()
        } else if focused {
            Self::focused()
        } else if visible {
            Self::visible()
        } else {
            Self::background()
        }
    }
}

/// Request priority for throttling decisions
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RequestPriority {
    /// Low priority (preloads, prefetch)
    Low = 0,
    /// Normal priority (images, stylesheets)
    Normal = 1,
    /// High priority (main document, critical CSS)
    High = 2,
    /// Critical priority (service worker, auth)
    Critical = 3,
}

/// Tab network state
#[derive(Debug)]
struct TabNetworkState {
    /// Current policy
    policy: BackgroundNetworkPolicy,
    /// Active request count
    active_requests: usize,
    /// Bytes transferred this second
    bytes_this_second: u64,
    /// Last bandwidth reset
    last_reset: Instant,
    /// Queued requests
    queued_requests: usize,
}

impl TabNetworkState {
    fn new(policy: BackgroundNetworkPolicy) -> Self {
        Self {
            policy,
            active_requests: 0,
            bytes_this_second: 0,
            last_reset: Instant::now(),
            queued_requests: 0,
        }
    }
    
    fn reset_if_needed(&mut self) {
        if self.last_reset.elapsed() >= Duration::from_secs(1) {
            self.bytes_this_second = 0;
            self.last_reset = Instant::now();
        }
    }
}

/// Network throttler for tabs
#[derive(Debug, Default)]
pub struct NetworkThrottler {
    /// Per-tab network state
    tabs: HashMap<u32, TabNetworkState>,
}

impl NetworkThrottler {
    /// Create a new network throttler
    pub fn new() -> Self {
        Self {
            tabs: HashMap::new(),
        }
    }
    
    /// Register a tab with default focused policy
    pub fn register_tab(&mut self, tab_id: u32) {
        self.tabs.insert(tab_id, TabNetworkState::new(BackgroundNetworkPolicy::focused()));
    }
    
    /// Unregister a tab
    pub fn unregister_tab(&mut self, tab_id: u32) {
        self.tabs.remove(&tab_id);
    }
    
    /// Update policy for a tab
    pub fn set_policy(&mut self, tab_id: u32, policy: BackgroundNetworkPolicy) {
        if let Some(state) = self.tabs.get_mut(&tab_id) {
            state.policy = policy;
        }
    }
    
    /// Update policy based on tab state
    pub fn update_tab_state(&mut self, tab_id: u32, focused: bool, visible: bool, hibernated: bool) {
        let policy = BackgroundNetworkPolicy::for_tab(focused, visible, hibernated);
        self.set_policy(tab_id, policy);
    }
    
    /// Check if a request can proceed
    pub fn can_request(&mut self, tab_id: u32, priority: RequestPriority) -> bool {
        let Some(state) = self.tabs.get_mut(&tab_id) else {
            return true; // Unknown tab, allow
        };
        
        state.reset_if_needed();
        
        // Critical requests always allowed if policy permits
        if priority == RequestPriority::Critical && state.policy.allow_critical {
            return true;
        }
        
        // Check concurrent limit
        if state.active_requests >= state.policy.max_concurrent_requests {
            return false;
        }
        
        true
    }
    
    /// Check if bandwidth limit allows transfer
    pub fn can_transfer(&mut self, tab_id: u32, bytes: u64) -> bool {
        let Some(state) = self.tabs.get_mut(&tab_id) else {
            return true;
        };
        
        state.reset_if_needed();
        
        match state.policy.bandwidth_limit {
            Some(limit) => state.bytes_this_second + bytes <= limit,
            None => true,
        }
    }
    
    /// Record request start
    pub fn request_started(&mut self, tab_id: u32) {
        if let Some(state) = self.tabs.get_mut(&tab_id) {
            state.active_requests += 1;
        }
    }
    
    /// Record request end
    pub fn request_ended(&mut self, tab_id: u32) {
        if let Some(state) = self.tabs.get_mut(&tab_id) {
            state.active_requests = state.active_requests.saturating_sub(1);
        }
    }
    
    /// Record bytes transferred
    pub fn bytes_transferred(&mut self, tab_id: u32, bytes: u64) {
        if let Some(state) = self.tabs.get_mut(&tab_id) {
            state.reset_if_needed();
            state.bytes_this_second += bytes;
        }
    }
    
    /// Get request delay for a tab
    pub fn get_request_delay(&self, tab_id: u32) -> Duration {
        self.tabs
            .get(&tab_id)
            .map(|s| s.policy.request_delay)
            .unwrap_or(Duration::ZERO)
    }
    
    /// Get current policy for a tab
    pub fn get_policy(&self, tab_id: u32) -> Option<&BackgroundNetworkPolicy> {
        self.tabs.get(&tab_id).map(|s| &s.policy)
    }
    
    /// Get statistics
    pub fn stats(&self) -> NetworkThrottleStats {
        let total = self.tabs.len();
        let throttled = self.tabs.values()
            .filter(|s| s.policy.bandwidth_limit.is_some())
            .count();
        let blocked = self.tabs.values()
            .filter(|s| s.policy.max_concurrent_requests == 0)
            .count();
        
        NetworkThrottleStats { total, throttled, blocked }
    }
}

/// Network throttle statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct NetworkThrottleStats {
    pub total: usize,
    pub throttled: usize,
    pub blocked: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_policy_focused() {
        let policy = BackgroundNetworkPolicy::focused();
        assert_eq!(policy.max_concurrent_requests, 10);
        assert!(policy.bandwidth_limit.is_none());
    }
    
    #[test]
    fn test_policy_background() {
        let policy = BackgroundNetworkPolicy::background();
        assert_eq!(policy.max_concurrent_requests, 2);
        assert_eq!(policy.bandwidth_limit, Some(100_000));
    }
    
    #[test]
    fn test_throttler_request_limit() {
        let mut throttler = NetworkThrottler::new();
        throttler.register_tab(1);
        throttler.set_policy(1, BackgroundNetworkPolicy::background());
        
        // First 2 requests should be allowed
        assert!(throttler.can_request(1, RequestPriority::Normal));
        throttler.request_started(1);
        assert!(throttler.can_request(1, RequestPriority::Normal));
        throttler.request_started(1);
        
        // Third should be denied
        assert!(!throttler.can_request(1, RequestPriority::Normal));
        
        // Critical still allowed
        assert!(throttler.can_request(1, RequestPriority::Critical));
    }
    
    #[test]
    fn test_for_tab_policy() {
        assert_eq!(
            BackgroundNetworkPolicy::for_tab(true, true, false).max_concurrent_requests,
            10 // focused
        );
        assert_eq!(
            BackgroundNetworkPolicy::for_tab(false, true, false).max_concurrent_requests,
            6 // visible
        );
        assert_eq!(
            BackgroundNetworkPolicy::for_tab(false, false, false).max_concurrent_requests,
            2 // background
        );
        assert_eq!(
            BackgroundNetworkPolicy::for_tab(false, false, true).max_concurrent_requests,
            0 // hibernated
        );
    }
}
