//! Datalist Support
//!
//! HTML5 datalist element processing and suggestion management.

use std::collections::HashMap;

/// Datalist option
#[derive(Debug, Clone)]
pub struct DatalistOption {
    pub value: String,
    pub label: Option<String>,
    pub disabled: bool,
}

impl DatalistOption {
    pub fn new(value: &str) -> Self {
        Self {
            value: value.to_string(),
            label: None,
            disabled: false,
        }
    }
    
    pub fn with_label(value: &str, label: &str) -> Self {
        Self {
            value: value.to_string(),
            label: Some(label.to_string()),
            disabled: false,
        }
    }
    
    /// Get display text (label or value)
    pub fn display_text(&self) -> &str {
        self.label.as_deref().unwrap_or(&self.value)
    }
}

/// Datalist element representation
#[derive(Debug, Clone, Default)]
pub struct Datalist {
    pub id: String,
    pub options: Vec<DatalistOption>,
}

impl Datalist {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            options: Vec::new(),
        }
    }
    
    /// Add an option
    pub fn add_option(&mut self, option: DatalistOption) {
        self.options.push(option);
    }
    
    /// Add a simple value
    pub fn add_value(&mut self, value: &str) {
        self.options.push(DatalistOption::new(value));
    }
    
    /// Get all enabled options
    pub fn enabled_options(&self) -> Vec<&DatalistOption> {
        self.options.iter().filter(|o| !o.disabled).collect()
    }
    
    /// Filter options by prefix
    pub fn filter(&self, prefix: &str) -> Vec<&DatalistOption> {
        let prefix_lower = prefix.to_lowercase();
        self.options.iter()
            .filter(|o| !o.disabled)
            .filter(|o| {
                o.value.to_lowercase().starts_with(&prefix_lower) ||
                o.label.as_ref().map(|l| l.to_lowercase().starts_with(&prefix_lower)).unwrap_or(false)
            })
            .collect()
    }
    
    /// Filter options containing substring
    pub fn filter_contains(&self, query: &str) -> Vec<&DatalistOption> {
        let query_lower = query.to_lowercase();
        self.options.iter()
            .filter(|o| !o.disabled)
            .filter(|o| {
                o.value.to_lowercase().contains(&query_lower) ||
                o.label.as_ref().map(|l| l.to_lowercase().contains(&query_lower)).unwrap_or(false)
            })
            .collect()
    }
    
    /// Fuzzy match options
    pub fn fuzzy_match(&self, query: &str) -> Vec<(&DatalistOption, f64)> {
        let query_lower = query.to_lowercase();
        let mut matches: Vec<_> = self.options.iter()
            .filter(|o| !o.disabled)
            .filter_map(|o| {
                let score = fuzzy_score(&query_lower, &o.value.to_lowercase());
                if score > 0.0 {
                    Some((o, score))
                } else {
                    o.label.as_ref().and_then(|l| {
                        let label_score = fuzzy_score(&query_lower, &l.to_lowercase());
                        if label_score > 0.0 {
                            Some((o, label_score))
                        } else {
                            None
                        }
                    })
                }
            })
            .collect();
        
        matches.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        matches
    }
}

/// Simple fuzzy matching score (0.0 to 1.0)
fn fuzzy_score(query: &str, target: &str) -> f64 {
    if query.is_empty() {
        return 1.0;
    }
    if target.is_empty() {
        return 0.0;
    }
    
    // Exact match
    if target == query {
        return 1.0;
    }
    
    // Prefix match
    if target.starts_with(query) {
        return 0.9;
    }
    
    // Substring match
    if target.contains(query) {
        return 0.7;
    }
    
    // Character sequence match
    let mut query_chars = query.chars().peekable();
    let mut matched = 0;
    
    for c in target.chars() {
        if query_chars.peek() == Some(&c) {
            query_chars.next();
            matched += 1;
        }
    }
    
    if matched == query.len() {
        0.5 * (matched as f64 / target.len() as f64)
    } else {
        0.0
    }
}

/// Datalist registry - manages all datalists on a page
#[derive(Debug, Default)]
pub struct DatalistRegistry {
    datalists: HashMap<String, Datalist>,
    input_bindings: HashMap<u64, String>, // input element ID -> datalist ID
}

impl DatalistRegistry {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Register a datalist
    pub fn register(&mut self, datalist: Datalist) {
        self.datalists.insert(datalist.id.clone(), datalist);
    }
    
    /// Unregister a datalist
    pub fn unregister(&mut self, id: &str) {
        self.datalists.remove(id);
    }
    
    /// Bind input to datalist
    pub fn bind_input(&mut self, input_id: u64, datalist_id: &str) {
        self.input_bindings.insert(input_id, datalist_id.to_string());
    }
    
    /// Unbind input
    pub fn unbind_input(&mut self, input_id: u64) {
        self.input_bindings.remove(&input_id);
    }
    
    /// Get datalist for input
    pub fn get_for_input(&self, input_id: u64) -> Option<&Datalist> {
        self.input_bindings.get(&input_id)
            .and_then(|id| self.datalists.get(id))
    }
    
    /// Get datalist by ID
    pub fn get(&self, id: &str) -> Option<&Datalist> {
        self.datalists.get(id)
    }
    
    /// Get mutable datalist
    pub fn get_mut(&mut self, id: &str) -> Option<&mut Datalist> {
        self.datalists.get_mut(id)
    }
    
    /// Get suggestions for an input
    pub fn get_suggestions(&self, input_id: u64, value: &str) -> Vec<&DatalistOption> {
        self.get_for_input(input_id)
            .map(|dl| dl.filter(value))
            .unwrap_or_default()
    }
    
    /// Clear all datalists
    pub fn clear(&mut self) {
        self.datalists.clear();
        self.input_bindings.clear();
    }
}

/// Suggestion popup state
#[derive(Debug, Clone)]
pub struct SuggestionPopup {
    pub visible: bool,
    pub input_id: u64,
    pub suggestions: Vec<String>,
    pub selected_index: Option<usize>,
    pub position: PopupPosition,
}

/// Popup position
#[derive(Debug, Clone, Default)]
pub struct PopupPosition {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub max_height: f64,
}

impl SuggestionPopup {
    pub fn new(input_id: u64) -> Self {
        Self {
            visible: false,
            input_id,
            suggestions: Vec::new(),
            selected_index: None,
            position: PopupPosition::default(),
        }
    }
    
    /// Show with suggestions
    pub fn show(&mut self, suggestions: Vec<String>, position: PopupPosition) {
        self.suggestions = suggestions;
        self.position = position;
        self.selected_index = None;
        self.visible = !self.suggestions.is_empty();
    }
    
    /// Hide popup
    pub fn hide(&mut self) {
        self.visible = false;
        self.selected_index = None;
    }
    
    /// Select next suggestion
    pub fn select_next(&mut self) {
        if self.suggestions.is_empty() {
            return;
        }
        
        self.selected_index = Some(match self.selected_index {
            Some(i) => (i + 1) % self.suggestions.len(),
            None => 0,
        });
    }
    
    /// Select previous suggestion
    pub fn select_prev(&mut self) {
        if self.suggestions.is_empty() {
            return;
        }
        
        self.selected_index = Some(match self.selected_index {
            Some(0) => self.suggestions.len() - 1,
            Some(i) => i - 1,
            None => self.suggestions.len() - 1,
        });
    }
    
    /// Get selected value
    pub fn selected_value(&self) -> Option<&str> {
        self.selected_index
            .and_then(|i| self.suggestions.get(i))
            .map(|s| s.as_str())
    }
    
    /// Select by index
    pub fn select_at(&mut self, index: usize) {
        if index < self.suggestions.len() {
            self.selected_index = Some(index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_datalist_filter() {
        let mut dl = Datalist::new("browsers");
        dl.add_value("Chrome");
        dl.add_value("Firefox");
        dl.add_value("Safari");
        dl.add_value("Edge");
        
        let results = dl.filter("F");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].value, "Firefox");
    }
    
    #[test]
    fn test_datalist_filter_contains() {
        let mut dl = Datalist::new("fruits");
        dl.add_value("Apple");
        dl.add_value("Pineapple");
        dl.add_value("Banana");
        
        let results = dl.filter_contains("apple");
        assert_eq!(results.len(), 2);
    }
    
    #[test]
    fn test_fuzzy_score() {
        assert_eq!(fuzzy_score("test", "test"), 1.0);
        assert!(fuzzy_score("te", "test") > 0.8);
        assert!(fuzzy_score("est", "test") > 0.5);
        assert_eq!(fuzzy_score("xyz", "test"), 0.0);
    }
    
    #[test]
    fn test_registry() {
        let mut registry = DatalistRegistry::new();
        
        let mut dl = Datalist::new("colors");
        dl.add_value("Red");
        dl.add_value("Green");
        dl.add_value("Blue");
        registry.register(dl);
        
        registry.bind_input(1, "colors");
        
        let suggestions = registry.get_suggestions(1, "R");
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].value, "Red");
    }
    
    #[test]
    fn test_suggestion_popup() {
        let mut popup = SuggestionPopup::new(1);
        popup.show(vec!["A".into(), "B".into(), "C".into()], PopupPosition::default());
        
        assert!(popup.visible);
        assert_eq!(popup.selected_index, None);
        
        popup.select_next();
        assert_eq!(popup.selected_value(), Some("A"));
        
        popup.select_next();
        assert_eq!(popup.selected_value(), Some("B"));
        
        popup.select_prev();
        assert_eq!(popup.selected_value(), Some("A"));
    }
}
