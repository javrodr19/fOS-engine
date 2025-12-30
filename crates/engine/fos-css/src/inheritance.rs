//! Style Inheritance Optimization
//!
//! Efficient handling of CSS inheritance with snapshots and hoisting.

use std::collections::HashMap;
use std::sync::Arc;
use super::rule_tree::{PropertyId, PackedValue, OptPropertyMask};

/// Inheritance snapshot for a cascade level
#[derive(Debug, Clone)]
pub struct InheritanceSnapshot {
    /// Inherited values
    values: HashMap<PropertyId, PackedValue>,
    /// Properties that are set
    mask: OptPropertyMask,
    /// Parent snapshot
    parent: Option<Arc<InheritanceSnapshot>>,
}

impl InheritanceSnapshot {
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
            mask: OptPropertyMask::new(),
            parent: None,
        }
    }
    
    pub fn with_parent(parent: Arc<InheritanceSnapshot>) -> Self {
        Self {
            values: HashMap::new(),
            mask: OptPropertyMask::new(),
            parent: Some(parent),
        }
    }
    
    /// Set a value
    pub fn set(&mut self, property: PropertyId, value: PackedValue) {
        self.values.insert(property, value);
        self.mask.set(property);
    }
    
    /// Get a value, walking up parent chain
    pub fn get(&self, property: PropertyId) -> Option<PackedValue> {
        if let Some(v) = self.values.get(&property) {
            return Some(*v);
        }
        
        if let Some(parent) = &self.parent {
            return parent.get(property);
        }
        
        None
    }
    
    /// Check if property is set
    pub fn has(&self, property: PropertyId) -> bool {
        self.mask.get(property) || self.parent.as_ref().map(|p| p.has(property)).unwrap_or(false)
    }
}

impl Default for InheritanceSnapshot {
    fn default() -> Self {
        Self::new()
    }
}

/// Properties that inherit by default
pub fn inherits_by_default(property: PropertyId) -> bool {
    matches!(property,
        PropertyId::COLOR |
        PropertyId::FONT_SIZE |
        PropertyId::FONT_WEIGHT |
        PropertyId::FONT_FAMILY
        // Add more inheriting properties
    )
}

/// Inherited-only property storage
/// Stores inheriting properties once per cascade level
#[derive(Debug, Default)]
pub struct InheritedProperties {
    /// Values by property
    values: HashMap<PropertyId, PackedValue>,
}

impl InheritedProperties {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn set(&mut self, property: PropertyId, value: PackedValue) {
        if inherits_by_default(property) {
            self.values.insert(property, value);
        }
    }
    
    pub fn get(&self, property: PropertyId) -> Option<PackedValue> {
        self.values.get(&property).copied()
    }
    
    /// Create a child that inherits from this
    pub fn inherit(&self) -> Self {
        Self {
            values: self.values.clone(),
        }
    }
}

/// CSS Custom Property hoisting
/// Pre-resolves custom property dependencies
#[derive(Debug, Default)]
pub struct CustomPropertyResolver {
    /// Resolved custom properties
    resolved: HashMap<String, String>,
    /// Dependency order (topologically sorted)
    order: Vec<String>,
}

impl CustomPropertyResolver {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Register a custom property
    pub fn register(&mut self, name: &str, value: &str) {
        // Check for var() references
        let deps = self.extract_dependencies(value);
        
        // Resolve dependencies first
        let resolved = self.resolve_references(value);
        self.resolved.insert(name.to_string(), resolved);
        self.order.push(name.to_string());
    }
    
    /// Extract var() dependencies from value
    fn extract_dependencies(&self, value: &str) -> Vec<String> {
        let mut deps = Vec::new();
        let mut rest = value;
        
        while let Some(start) = rest.find("var(") {
            if let Some(end) = rest[start..].find(')') {
                let content = &rest[start + 4..start + end];
                if let Some(comma) = content.find(',') {
                    let name = content[..comma].trim().trim_start_matches("--");
                    deps.push(name.to_string());
                } else {
                    let name = content.trim().trim_start_matches("--");
                    deps.push(name.to_string());
                }
                rest = &rest[start + end + 1..];
            } else {
                break;
            }
        }
        
        deps
    }
    
    /// Resolve var() references in value
    fn resolve_references(&self, value: &str) -> String {
        let mut result = value.to_string();
        
        for (name, resolved) in &self.resolved {
            let pattern = format!("var(--{})", name);
            result = result.replace(&pattern, resolved);
            
            // Also try with whitespace
            let pattern2 = format!("var( --{} )", name);
            result = result.replace(&pattern2, resolved);
        }
        
        result
    }
    
    /// Get resolved value
    pub fn get(&self, name: &str) -> Option<&str> {
        self.resolved.get(name).map(|s| s.as_str())
    }
    
    /// Hoist all properties (pre-resolve all)
    pub fn hoist(&mut self) {
        // Re-resolve in dependency order
        let order = self.order.clone();
        for name in order {
            if let Some(value) = self.resolved.get(&name).cloned() {
                let resolved = self.resolve_references(&value);
                self.resolved.insert(name, resolved);
            }
        }
    }
}

/// On-demand style calculation
/// Only calculates styles for visible elements
#[derive(Debug, Default)]
pub struct OnDemandStyler {
    /// Cached computed styles
    cache: HashMap<u64, Arc<ComputedStyleSnapshot>>,
    /// Pending calculations
    pending: Vec<u64>,
}

/// Computed style snapshot
#[derive(Debug, Clone)]
pub struct ComputedStyleSnapshot {
    /// Property values
    values: HashMap<PropertyId, PackedValue>,
    /// Is this for a hidden element
    hidden: bool,
}

impl OnDemandStyler {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Request style calculation for element
    pub fn request(&mut self, element_id: u64) {
        if !self.cache.contains_key(&element_id) {
            self.pending.push(element_id);
        }
    }
    
    /// Get cached style
    pub fn get(&self, element_id: u64) -> Option<Arc<ComputedStyleSnapshot>> {
        self.cache.get(&element_id).cloned()
    }
    
    /// Mark element as hidden (skip calculation)
    pub fn mark_hidden(&mut self, element_id: u64) {
        self.cache.insert(element_id, Arc::new(ComputedStyleSnapshot {
            values: HashMap::new(),
            hidden: true,
        }));
    }
    
    /// Clear all cached styles
    pub fn invalidate_all(&mut self) {
        self.cache.clear();
        self.pending.clear();
    }
    
    /// Invalidate specific element
    pub fn invalidate(&mut self, element_id: u64) {
        self.cache.remove(&element_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_inheritance_snapshot() {
        let mut snapshot = InheritanceSnapshot::new();
        snapshot.set(PropertyId::COLOR, PackedValue::color_index(1));
        
        assert!(snapshot.has(PropertyId::COLOR));
        assert!(!snapshot.has(PropertyId::WIDTH));
    }
    
    #[test]
    fn test_inherited_properties() {
        let mut props = InheritedProperties::new();
        props.set(PropertyId::COLOR, PackedValue::color_index(1));
        
        let child = props.inherit();
        assert!(child.get(PropertyId::COLOR).is_some());
    }
    
    #[test]
    fn test_custom_property_resolver() {
        let mut resolver = CustomPropertyResolver::new();
        resolver.register("primary", "#ff0000");
        resolver.register("secondary", "var(--primary)");
        resolver.hoist();
        
        assert_eq!(resolver.get("primary"), Some("#ff0000"));
    }
    
    #[test]
    fn test_on_demand_styler() {
        let mut styler = OnDemandStyler::new();
        styler.request(1);
        styler.mark_hidden(2);
        
        assert!(styler.get(2).is_some());
        assert!(styler.get(2).unwrap().hidden);
    }
}
