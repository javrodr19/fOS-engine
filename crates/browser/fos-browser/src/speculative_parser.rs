//! Speculative Parsing
//!
//! Look-ahead parsing to discover preloadable resources before needed.

use std::collections::VecDeque;

/// Speculative parser for resource discovery
#[derive(Debug)]
pub struct SpeculativeParser {
    /// Discovered hints
    hints: VecDeque<SpeculativeHint>,
    /// Parse position
    pos: usize,
    /// Statistics
    stats: SpeculativeStats,
}

/// Hint for preloadable resource
#[derive(Debug, Clone)]
pub struct SpeculativeHint {
    pub resource_type: ResourceType,
    pub url: String,
    pub priority: Priority,
    pub discovered_at: usize,
}

/// Resource type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceType {
    Script,
    Stylesheet,
    Image,
    Font,
    Fetch,
    Preconnect,
}

/// Priority level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

/// Statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct SpeculativeStats {
    pub bytes_scanned: usize,
    pub scripts_found: usize,
    pub styles_found: usize,
    pub images_found: usize,
    pub preloads_initiated: usize,
}

impl Default for SpeculativeParser {
    fn default() -> Self { Self::new() }
}

impl SpeculativeParser {
    pub fn new() -> Self {
        Self { hints: VecDeque::new(), pos: 0, stats: SpeculativeStats::default() }
    }
    
    /// Scan HTML for preloadable resources
    pub fn scan(&mut self, html: &str) {
        self.stats.bytes_scanned += html.len();
        
        let mut i = 0;
        let bytes = html.as_bytes();
        
        while i < bytes.len() {
            if bytes[i] == b'<' {
                // Check for script/link/img tags
                if let Some((hint, advance)) = self.try_parse_tag(&html[i..]) {
                    self.hints.push_back(hint);
                    i += advance;
                    continue;
                }
            }
            i += 1;
        }
        
        self.pos += html.len();
    }
    
    fn try_parse_tag(&mut self, html: &str) -> Option<(SpeculativeHint, usize)> {
        let lower = html.to_ascii_lowercase();
        
        // Script tag
        if lower.starts_with("<script") {
            if let Some(src) = extract_attr(html, "src") {
                self.stats.scripts_found += 1;
                let priority = if html.contains("async") || html.contains("defer") {
                    Priority::Normal
                } else {
                    Priority::High
                };
                return Some((SpeculativeHint {
                    resource_type: ResourceType::Script,
                    url: src,
                    priority,
                    discovered_at: self.pos,
                }, find_tag_end(html)));
            }
        }
        
        // Link tag (stylesheet)
        if lower.starts_with("<link") && lower.contains("stylesheet") {
            if let Some(href) = extract_attr(html, "href") {
                self.stats.styles_found += 1;
                return Some((SpeculativeHint {
                    resource_type: ResourceType::Stylesheet,
                    url: href,
                    priority: Priority::High,
                    discovered_at: self.pos,
                }, find_tag_end(html)));
            }
        }
        
        // Image tag
        if lower.starts_with("<img") {
            if let Some(src) = extract_attr(html, "src") {
                self.stats.images_found += 1;
                let priority = if html.contains("loading=\"lazy\"") {
                    Priority::Low
                } else {
                    Priority::Normal
                };
                return Some((SpeculativeHint {
                    resource_type: ResourceType::Image,
                    url: src,
                    priority,
                    discovered_at: self.pos,
                }, find_tag_end(html)));
            }
        }
        
        None
    }
    
    /// Get next hint
    pub fn next_hint(&mut self) -> Option<SpeculativeHint> {
        self.hints.pop_front()
    }
    
    /// Get all hints sorted by priority
    pub fn drain_by_priority(&mut self) -> Vec<SpeculativeHint> {
        let mut hints: Vec<_> = self.hints.drain(..).collect();
        hints.sort_by(|a, b| b.priority.cmp(&a.priority));
        hints
    }
    
    /// Has hints
    pub fn has_hints(&self) -> bool { !self.hints.is_empty() }
    
    /// Get stats
    pub fn stats(&self) -> &SpeculativeStats { &self.stats }
}

/// Extract attribute value
fn extract_attr(html: &str, attr: &str) -> Option<String> {
    let pattern = format!("{}=\"", attr);
    if let Some(start) = html.find(&pattern) {
        let value_start = start + pattern.len();
        if let Some(end) = html[value_start..].find('"') {
            return Some(html[value_start..value_start + end].to_string());
        }
    }
    
    // Try single quotes
    let pattern = format!("{}='", attr);
    if let Some(start) = html.find(&pattern) {
        let value_start = start + pattern.len();
        if let Some(end) = html[value_start..].find('\'') {
            return Some(html[value_start..value_start + end].to_string());
        }
    }
    
    None
}

/// Find end of tag
fn find_tag_end(html: &str) -> usize {
    html.find('>').map(|i| i + 1).unwrap_or(html.len())
}

/// Preload queue for managing speculative loads
#[derive(Debug, Default)]
pub struct PreloadQueue {
    queued: VecDeque<SpeculativeHint>,
    in_flight: usize,
    max_concurrent: usize,
    completed: usize,
}

impl PreloadQueue {
    pub fn new() -> Self {
        Self { queued: VecDeque::new(), in_flight: 0, max_concurrent: 6, completed: 0 }
    }
    
    pub fn set_max_concurrent(&mut self, max: usize) { self.max_concurrent = max; }
    
    pub fn enqueue(&mut self, hint: SpeculativeHint) {
        // Insert by priority
        let pos = self.queued.iter()
            .position(|h| h.priority < hint.priority)
            .unwrap_or(self.queued.len());
        self.queued.insert(pos, hint);
    }
    
    pub fn next(&mut self) -> Option<SpeculativeHint> {
        if self.in_flight >= self.max_concurrent { return None; }
        self.queued.pop_front().map(|h| { self.in_flight += 1; h })
    }
    
    pub fn complete(&mut self) {
        self.in_flight = self.in_flight.saturating_sub(1);
        self.completed += 1;
    }
    
    pub fn queued_count(&self) -> usize { self.queued.len() }
    pub fn in_flight_count(&self) -> usize { self.in_flight }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_scan_script() {
        let mut parser = SpeculativeParser::new();
        parser.scan(r#"<html><script src="app.js"></script></html>"#);
        
        let hint = parser.next_hint().unwrap();
        assert_eq!(hint.resource_type, ResourceType::Script);
        assert_eq!(hint.url, "app.js");
    }
    
    #[test]
    fn test_scan_stylesheet() {
        let mut parser = SpeculativeParser::new();
        parser.scan(r#"<link rel="stylesheet" href="style.css">"#);
        
        let hint = parser.next_hint().unwrap();
        assert_eq!(hint.resource_type, ResourceType::Stylesheet);
        assert_eq!(hint.url, "style.css");
    }
    
    #[test]
    fn test_scan_image() {
        let mut parser = SpeculativeParser::new();
        parser.scan(r#"<img src="photo.jpg" loading="lazy">"#);
        
        let hint = parser.next_hint().unwrap();
        assert_eq!(hint.resource_type, ResourceType::Image);
        assert_eq!(hint.priority, Priority::Low);
    }
    
    #[test]
    fn test_preload_queue() {
        let mut queue = PreloadQueue::new();
        queue.set_max_concurrent(2);
        
        queue.enqueue(SpeculativeHint { resource_type: ResourceType::Script, url: "a.js".into(), priority: Priority::Normal, discovered_at: 0 });
        queue.enqueue(SpeculativeHint { resource_type: ResourceType::Script, url: "b.js".into(), priority: Priority::High, discovered_at: 0 });
        
        // High priority should come first
        let first = queue.next().unwrap();
        assert_eq!(first.url, "b.js");
    }
}
