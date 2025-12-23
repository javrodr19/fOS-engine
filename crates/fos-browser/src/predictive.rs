//! Predictive networking
//!
//! DNS prefetch, preconnect, and resource prediction.

use std::collections::{HashSet, VecDeque};

/// Predictive networking manager
#[derive(Debug, Default)]
pub struct PredictiveNetwork {
    /// Recently visited URLs for prediction
    recent_urls: VecDeque<String>,
    /// Known link targets from current page
    page_links: HashSet<String>,
    /// Preconnected hosts
    preconnected: HashSet<String>,
    /// Prefetch queue
    prefetch_queue: Vec<String>,
}

impl PredictiveNetwork {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Record a page visit for prediction learning
    pub fn record_visit(&mut self, url: &str) {
        if self.recent_urls.len() >= 100 {
            self.recent_urls.pop_front();
        }
        self.recent_urls.push_back(url.to_string());
        
        // Extract and prefetch likely next hosts
        if let Ok(parsed) = url::Url::parse(url) {
            if let Some(host) = parsed.host_str() {
                self.prefetch_dns(host);
            }
        }
    }
    
    /// Update links found on current page
    pub fn update_page_links(&mut self, links: Vec<String>) {
        self.page_links.clear();
        for link in links {
            if let Ok(parsed) = url::Url::parse(&link) {
                if let Some(host) = parsed.host_str() {
                    self.page_links.insert(host.to_string());
                    self.prefetch_dns(host);
                }
            }
        }
    }
    
    /// Prefetch DNS for a host
    fn prefetch_dns(&mut self, _host: &str) {
        // In a real implementation, trigger DNS lookup
        log::debug!("DNS prefetch: {}", _host);
    }
    
    /// Preconnect to a host (TCP + TLS handshake)
    pub fn preconnect(&mut self, host: &str) {
        if !self.preconnected.contains(host) {
            self.preconnected.insert(host.to_string());
            log::debug!("Preconnected to {}", host);
        }
    }
    
    /// Prefetch a resource
    pub fn prefetch(&mut self, url: &str) {
        self.prefetch_queue.push(url.to_string());
    }
    
    /// Get predicted next navigations
    pub fn predict_next(&self) -> Vec<String> {
        self.page_links.iter().take(5).cloned().collect()
    }
    
    /// Check if host is preconnected
    pub fn is_preconnected(&self, host: &str) -> bool {
        self.preconnected.contains(host)
    }
    
    /// Get DNS prefetch suggestions
    pub fn get_dns_suggestions(&self) -> Vec<String> {
        self.page_links.iter().take(10).cloned().collect()
    }
    
    /// Process prefetch queue
    pub fn process_queue(&mut self) -> Vec<String> {
        std::mem::take(&mut self.prefetch_queue)
    }
    
    /// Clear prefetch data (e.g., for privacy)
    pub fn clear(&mut self) {
        self.recent_urls.clear();
        self.page_links.clear();
        self.preconnected.clear();
    }
}

/// Resource hints from HTML
#[derive(Debug, Clone)]
pub struct ResourceHint {
    pub url: String,
    pub hint_type: HintType,
    pub crossorigin: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HintType {
    DnsPrefetch,
    Preconnect,
    Prefetch,
    Preload,
    Prerender,
    Modulepreload,
}

impl ResourceHint {
    /// Parse from <link> attributes
    pub fn from_link(rel: &str, href: &str, crossorigin: bool) -> Option<Self> {
        let hint_type = match rel {
            "dns-prefetch" => HintType::DnsPrefetch,
            "preconnect" => HintType::Preconnect,
            "prefetch" => HintType::Prefetch,
            "preload" => HintType::Preload,
            "prerender" => HintType::Prerender,
            "modulepreload" => HintType::Modulepreload,
            _ => return None,
        };
        
        Some(Self {
            url: href.to_string(),
            hint_type,
            crossorigin,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_predictive_network() {
        let mut net = PredictiveNetwork::new();
        
        net.record_visit("https://example.com/page1");
        net.update_page_links(vec![
            "https://example.com/page2".to_string(),
            "https://other.com/".to_string(),
        ]);
        
        assert!(net.page_links.contains("example.com"));
        assert!(net.page_links.contains("other.com"));
    }
}
