//! Preload Scanner
//!
//! Speculative resource loading and hints.

use std::collections::{HashSet, VecDeque};

/// Preload scanner
#[derive(Debug, Default)]
pub struct PreloadScanner {
    discovered: VecDeque<PreloadResource>,
    loaded: HashSet<String>,
}

/// Preload resource
#[derive(Debug, Clone)]
pub struct PreloadResource {
    pub url: String,
    pub resource_type: ResourceType,
    pub priority: Priority,
    pub crossorigin: Option<String>,
    pub integrity: Option<String>,
}

/// Resource type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceType {
    Script,
    Style,
    Image,
    Font,
    Fetch,
    Document,
    Audio,
    Video,
}

/// Loading priority
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Low,
    Medium,
    High,
    Critical,
}

impl PreloadScanner {
    pub fn new() -> Self { Self::default() }
    
    /// Scan HTML for resources
    pub fn scan(&mut self, html: &str) {
        // Scan for link[rel=preload]
        self.scan_preload_links(html);
        
        // Scan for scripts
        self.scan_scripts(html);
        
        // Scan for stylesheets
        self.scan_stylesheets(html);
        
        // Scan for images
        self.scan_images(html);
    }
    
    fn scan_preload_links(&mut self, html: &str) {
        // Simple regex-like scanning for <link rel="preload"
        for line in html.lines() {
            if line.contains("<link") && line.contains("preload") {
                if let Some(href) = self.extract_attr(line, "href") {
                    let as_type = self.extract_attr(line, "as")
                        .map(|s| self.parse_resource_type(&s))
                        .unwrap_or(ResourceType::Fetch);
                    
                    self.add_resource(href, as_type, Priority::High);
                }
            }
        }
    }
    
    fn scan_scripts(&mut self, html: &str) {
        for line in html.lines() {
            if line.contains("<script") && line.contains("src=") {
                if let Some(src) = self.extract_attr(line, "src") {
                    let priority = if line.contains("async") || line.contains("defer") {
                        Priority::Low
                    } else {
                        Priority::High
                    };
                    self.add_resource(src, ResourceType::Script, priority);
                }
            }
        }
    }
    
    fn scan_stylesheets(&mut self, html: &str) {
        for line in html.lines() {
            if line.contains("<link") && line.contains("stylesheet") {
                if let Some(href) = self.extract_attr(line, "href") {
                    self.add_resource(href, ResourceType::Style, Priority::Critical);
                }
            }
        }
    }
    
    fn scan_images(&mut self, html: &str) {
        for line in html.lines() {
            if line.contains("<img") && line.contains("src=") {
                if let Some(src) = self.extract_attr(line, "src") {
                    let priority = if line.contains("loading=\"lazy\"") {
                        Priority::Low
                    } else {
                        Priority::Medium
                    };
                    self.add_resource(src, ResourceType::Image, priority);
                }
            }
        }
    }
    
    fn extract_attr(&self, line: &str, attr: &str) -> Option<String> {
        let pattern = format!("{}=\"", attr);
        if let Some(start) = line.find(&pattern) {
            let rest = &line[start + pattern.len()..];
            if let Some(end) = rest.find('"') {
                return Some(rest[..end].to_string());
            }
        }
        None
    }
    
    fn parse_resource_type(&self, s: &str) -> ResourceType {
        match s {
            "script" => ResourceType::Script,
            "style" => ResourceType::Style,
            "image" => ResourceType::Image,
            "font" => ResourceType::Font,
            "audio" => ResourceType::Audio,
            "video" => ResourceType::Video,
            "document" => ResourceType::Document,
            _ => ResourceType::Fetch,
        }
    }
    
    fn add_resource(&mut self, url: String, resource_type: ResourceType, priority: Priority) {
        if !self.loaded.contains(&url) {
            self.discovered.push_back(PreloadResource {
                url,
                resource_type,
                priority,
                crossorigin: None,
                integrity: None,
            });
        }
    }
    
    /// Get next resource to preload
    pub fn next(&mut self) -> Option<PreloadResource> {
        // Sort by priority
        let mut resources: Vec<_> = self.discovered.drain(..).collect();
        resources.sort_by(|a, b| b.priority.cmp(&a.priority));
        
        for r in resources.into_iter().skip(1) {
            self.discovered.push_back(r);
        }
        
        self.discovered.pop_front()
    }
    
    /// Mark as loaded
    pub fn mark_loaded(&mut self, url: &str) {
        self.loaded.insert(url.to_string());
    }
    
    /// Get all discovered resources
    pub fn get_discovered(&self) -> &VecDeque<PreloadResource> {
        &self.discovered
    }
}

/// Resource hints
#[derive(Debug, Clone)]
pub struct ResourceHint {
    pub hint_type: HintType,
    pub url: String,
}

/// Hint type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HintType {
    DnsPrefetch,
    Preconnect,
    Prefetch,
    Preload,
    Prerender,
}

/// Hint processor
#[derive(Debug, Default)]
pub struct HintProcessor {
    hints: Vec<ResourceHint>,
    dns_prefetched: HashSet<String>,
    preconnected: HashSet<String>,
}

impl HintProcessor {
    pub fn new() -> Self { Self::default() }
    
    /// Add hint
    pub fn add_hint(&mut self, hint: ResourceHint) {
        self.hints.push(hint);
    }
    
    /// Process hints
    pub fn process(&mut self) -> Vec<HintAction> {
        let mut actions = Vec::new();
        
        for hint in &self.hints {
            match hint.hint_type {
                HintType::DnsPrefetch => {
                    if !self.dns_prefetched.contains(&hint.url) {
                        actions.push(HintAction::ResolveDns(hint.url.clone()));
                        self.dns_prefetched.insert(hint.url.clone());
                    }
                }
                HintType::Preconnect => {
                    if !self.preconnected.contains(&hint.url) {
                        actions.push(HintAction::Connect(hint.url.clone()));
                        self.preconnected.insert(hint.url.clone());
                    }
                }
                HintType::Prefetch => {
                    actions.push(HintAction::Fetch(hint.url.clone(), Priority::Low));
                }
                HintType::Preload => {
                    actions.push(HintAction::Fetch(hint.url.clone(), Priority::High));
                }
                HintType::Prerender => {
                    actions.push(HintAction::Prerender(hint.url.clone()));
                }
            }
        }
        
        self.hints.clear();
        actions
    }
}

/// Hint action
#[derive(Debug, Clone)]
pub enum HintAction {
    ResolveDns(String),
    Connect(String),
    Fetch(String, Priority),
    Prerender(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_preload_scanner() {
        let mut scanner = PreloadScanner::new();
        let html = r#"
            <link rel="preload" href="/style.css" as="style">
            <script src="/app.js"></script>
            <img src="/hero.png">
        "#;
        
        scanner.scan(html);
        assert!(scanner.discovered.len() >= 2);
    }
    
    #[test]
    fn test_hint_processor() {
        let mut processor = HintProcessor::new();
        processor.add_hint(ResourceHint {
            hint_type: HintType::DnsPrefetch,
            url: "https://cdn.example.com".into(),
        });
        
        let actions = processor.process();
        assert_eq!(actions.len(), 1);
    }
}
