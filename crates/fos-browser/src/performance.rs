//! Performance Timing API
//!
//! Navigation timing, resource timing, and user timing.

use std::collections::HashMap;

/// Navigation timing
#[derive(Debug, Clone, Default)]
pub struct NavigationTiming {
    pub navigation_start: f64,
    pub unload_event_start: f64,
    pub unload_event_end: f64,
    pub redirect_start: f64,
    pub redirect_end: f64,
    pub fetch_start: f64,
    pub domain_lookup_start: f64,
    pub domain_lookup_end: f64,
    pub connect_start: f64,
    pub connect_end: f64,
    pub secure_connection_start: f64,
    pub request_start: f64,
    pub response_start: f64,
    pub response_end: f64,
    pub dom_loading: f64,
    pub dom_interactive: f64,
    pub dom_content_loaded_event_start: f64,
    pub dom_content_loaded_event_end: f64,
    pub dom_complete: f64,
    pub load_event_start: f64,
    pub load_event_end: f64,
}

/// Resource timing entry
#[derive(Debug, Clone)]
pub struct ResourceTiming {
    pub name: String,
    pub entry_type: String,
    pub start_time: f64,
    pub duration: f64,
    pub initiator_type: String,
    pub next_hop_protocol: String,
    pub redirect_start: f64,
    pub redirect_end: f64,
    pub fetch_start: f64,
    pub domain_lookup_start: f64,
    pub domain_lookup_end: f64,
    pub connect_start: f64,
    pub connect_end: f64,
    pub secure_connection_start: f64,
    pub request_start: f64,
    pub response_start: f64,
    pub response_end: f64,
    pub transfer_size: u64,
    pub encoded_body_size: u64,
    pub decoded_body_size: u64,
}

/// User timing mark
#[derive(Debug, Clone)]
pub struct PerformanceMark {
    pub name: String,
    pub entry_type: String,
    pub start_time: f64,
    pub duration: f64,
    pub detail: Option<String>,
}

/// User timing measure
#[derive(Debug, Clone)]
pub struct PerformanceMeasure {
    pub name: String,
    pub entry_type: String,
    pub start_time: f64,
    pub duration: f64,
    pub detail: Option<String>,
}

/// Performance API
#[derive(Debug, Default)]
pub struct PerformanceApi {
    navigation_timing: NavigationTiming,
    resource_entries: Vec<ResourceTiming>,
    marks: Vec<PerformanceMark>,
    measures: Vec<PerformanceMeasure>,
    time_origin: f64,
}

impl PerformanceApi {
    pub fn new(time_origin: f64) -> Self {
        Self {
            navigation_timing: NavigationTiming::default(),
            resource_entries: Vec::new(),
            marks: Vec::new(),
            measures: Vec::new(),
            time_origin,
        }
    }
    
    /// Get high resolution time
    pub fn now(&self) -> f64 {
        // In real implementation, use std::time::Instant
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64() * 1000.0)
            .unwrap_or(0.0);
        now - self.time_origin
    }
    
    /// Create a mark
    pub fn mark(&mut self, name: &str, detail: Option<String>) {
        self.marks.push(PerformanceMark {
            name: name.to_string(),
            entry_type: "mark".to_string(),
            start_time: self.now(),
            duration: 0.0,
            detail,
        });
    }
    
    /// Create a measure between marks
    pub fn measure(&mut self, name: &str, start_mark: Option<&str>, end_mark: Option<&str>) {
        let start = start_mark
            .and_then(|n| self.marks.iter().find(|m| m.name == n))
            .map(|m| m.start_time)
            .unwrap_or(0.0);
        
        let end = end_mark
            .and_then(|n| self.marks.iter().find(|m| m.name == n))
            .map(|m| m.start_time)
            .unwrap_or_else(|| self.now());
        
        self.measures.push(PerformanceMeasure {
            name: name.to_string(),
            entry_type: "measure".to_string(),
            start_time: start,
            duration: end - start,
            detail: None,
        });
    }
    
    /// Clear marks
    pub fn clear_marks(&mut self, name: Option<&str>) {
        if let Some(n) = name {
            self.marks.retain(|m| m.name != n);
        } else {
            self.marks.clear();
        }
    }
    
    /// Clear measures
    pub fn clear_measures(&mut self, name: Option<&str>) {
        if let Some(n) = name {
            self.measures.retain(|m| m.name != n);
        } else {
            self.measures.clear();
        }
    }
    
    /// Get marks by name
    pub fn get_entries_by_name(&self, name: &str) -> Vec<&PerformanceMark> {
        self.marks.iter().filter(|m| m.name == name).collect()
    }
    
    /// Add resource timing
    pub fn add_resource(&mut self, entry: ResourceTiming) {
        self.resource_entries.push(entry);
        
        // Limit buffer size
        if self.resource_entries.len() > 250 {
            self.resource_entries.remove(0);
        }
    }
    
    /// Get resource timings
    pub fn get_resources(&self) -> &[ResourceTiming] {
        &self.resource_entries
    }
    
    /// Clear resource timings
    pub fn clear_resource_timings(&mut self) {
        self.resource_entries.clear();
    }
    
    /// Set navigation timing
    pub fn set_navigation_timing(&mut self, timing: NavigationTiming) {
        self.navigation_timing = timing;
    }
    
    /// Get navigation timing
    pub fn get_navigation_timing(&self) -> &NavigationTiming {
        &self.navigation_timing
    }
    
    /// Get all entries
    pub fn get_entries(&self) -> Vec<PerformanceEntry> {
        let mut entries = Vec::new();
        
        for mark in &self.marks {
            entries.push(PerformanceEntry {
                name: mark.name.clone(),
                entry_type: "mark".to_string(),
                start_time: mark.start_time,
                duration: mark.duration,
            });
        }
        
        for measure in &self.measures {
            entries.push(PerformanceEntry {
                name: measure.name.clone(),
                entry_type: "measure".to_string(),
                start_time: measure.start_time,
                duration: measure.duration,
            });
        }
        
        entries.sort_by(|a, b| a.start_time.partial_cmp(&b.start_time).unwrap());
        entries
    }
}

/// Generic performance entry
#[derive(Debug, Clone)]
pub struct PerformanceEntry {
    pub name: String,
    pub entry_type: String,
    pub start_time: f64,
    pub duration: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_performance_marks() {
        let mut perf = PerformanceApi::new(0.0);
        
        perf.mark("start", None);
        // Simulate work
        perf.mark("end", None);
        
        perf.measure("duration", Some("start"), Some("end"));
        
        assert_eq!(perf.measures.len(), 1);
        assert!(perf.measures[0].duration >= 0.0);
    }
}
