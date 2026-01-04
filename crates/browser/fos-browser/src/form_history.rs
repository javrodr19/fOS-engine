//! Form History/Autocomplete
//!
//! Field value history storage and privacy-aware suggestions.

use std::collections::HashMap;

/// History entry for a form field
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub value: String,
    pub use_count: u32,
    pub last_used: u64,
    pub created: u64,
}

impl HistoryEntry {
    pub fn new(value: &str) -> Self {
        let now = current_time_ms();
        Self { value: value.into(), use_count: 1, last_used: now, created: now }
    }
    
    pub fn record_use(&mut self) {
        self.use_count += 1;
        self.last_used = current_time_ms();
    }
}

/// Field identifier for history
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct FieldKey {
    pub origin: String,
    pub form_name: Option<String>,
    pub field_name: String,
    pub field_type: String,
}

impl FieldKey {
    pub fn new(origin: &str, field_name: &str, field_type: &str) -> Self {
        Self { origin: origin.into(), form_name: None, field_name: field_name.into(), field_type: field_type.into() }
    }
}

/// Form history manager
#[derive(Debug, Default)]
pub struct FormHistoryManager {
    entries: HashMap<FieldKey, Vec<HistoryEntry>>,
    max_per_field: usize,
    enabled: bool,
    blacklist: Vec<String>,
}

impl FormHistoryManager {
    pub fn new() -> Self {
        Self { entries: HashMap::new(), max_per_field: 100, enabled: true, blacklist: Vec::new() }
    }
    
    pub fn set_enabled(&mut self, enabled: bool) { self.enabled = enabled; }
    
    pub fn add_to_blacklist(&mut self, field_type: &str) { self.blacklist.push(field_type.to_lowercase()); }
    
    fn is_blacklisted(&self, field_type: &str) -> bool {
        let t = field_type.to_lowercase();
        self.blacklist.contains(&t) || matches!(t.as_str(), "password" | "credit-card" | "cc-number" | "cc-csc")
    }
    
    pub fn record(&mut self, key: FieldKey, value: &str) {
        if !self.enabled || value.trim().is_empty() || self.is_blacklisted(&key.field_type) { return; }
        
        let entries = self.entries.entry(key).or_default();
        if let Some(entry) = entries.iter_mut().find(|e| e.value == value) {
            entry.record_use();
        } else {
            entries.push(HistoryEntry::new(value));
            if entries.len() > self.max_per_field {
                entries.sort_by(|a, b| b.last_used.cmp(&a.last_used));
                entries.truncate(self.max_per_field);
            }
        }
    }
    
    pub fn get_suggestions(&self, key: &FieldKey, prefix: &str, limit: usize) -> Vec<&str> {
        if !self.enabled { return Vec::new(); }
        
        let prefix_lower = prefix.to_lowercase();
        self.entries.get(key)
            .map(|entries| {
                let mut matches: Vec<_> = entries.iter()
                    .filter(|e| e.value.to_lowercase().starts_with(&prefix_lower))
                    .collect();
                matches.sort_by(|a, b| b.use_count.cmp(&a.use_count).then(b.last_used.cmp(&a.last_used)));
                matches.into_iter().take(limit).map(|e| e.value.as_str()).collect()
            })
            .unwrap_or_default()
    }
    
    pub fn clear_field(&mut self, key: &FieldKey) { self.entries.remove(key); }
    pub fn clear_origin(&mut self, origin: &str) { self.entries.retain(|k, _| k.origin != origin); }
    pub fn clear_all(&mut self) { self.entries.clear(); }
    
    pub fn remove_value(&mut self, key: &FieldKey, value: &str) {
        if let Some(entries) = self.entries.get_mut(key) {
            entries.retain(|e| e.value != value);
        }
    }
}

fn current_time_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_form_history() {
        let mut manager = FormHistoryManager::new();
        let key = FieldKey::new("https://example.com", "search", "text");
        
        manager.record(key.clone(), "rust programming");
        manager.record(key.clone(), "rust tutorial");
        manager.record(key.clone(), "rust programming");
        
        let suggestions = manager.get_suggestions(&key, "rust", 10);
        assert_eq!(suggestions.len(), 2);
        assert_eq!(suggestions[0], "rust programming"); // Higher use count
    }
    
    #[test]
    fn test_password_blacklist() {
        let mut manager = FormHistoryManager::new();
        let key = FieldKey::new("https://example.com", "pass", "password");
        manager.record(key.clone(), "secret");
        assert!(manager.get_suggestions(&key, "", 10).is_empty());
    }
}
