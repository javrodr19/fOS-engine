//! Element Attributes
//!
//! Attribute manipulation: get, set, remove, has.

use std::collections::HashMap;

/// Named node map (attribute collection)
#[derive(Debug, Clone, Default)]
pub struct NamedNodeMap {
    attributes: Vec<Attr>,
    by_name: HashMap<String, usize>,
}

/// Single attribute
#[derive(Debug, Clone)]
pub struct Attr {
    pub name: String,
    pub value: String,
    pub namespace: Option<String>,
    pub prefix: Option<String>,
    pub local_name: String,
    pub owner_element: Option<u32>,
}

impl Attr {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            local_name: name.clone(),
            name,
            value: value.into(),
            namespace: None,
            prefix: None,
            owner_element: None,
        }
    }
    
    pub fn is_id(&self) -> bool {
        self.name == "id"
    }
}

impl NamedNodeMap {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Get number of attributes
    pub fn length(&self) -> usize {
        self.attributes.len()
    }
    
    /// Get attribute by index
    pub fn item(&self, index: usize) -> Option<&Attr> {
        self.attributes.get(index)
    }
    
    /// Get attribute by name
    pub fn get_named_item(&self, name: &str) -> Option<&Attr> {
        self.by_name.get(name).and_then(|&i| self.attributes.get(i))
    }
    
    /// Get attribute value
    pub fn get_attribute(&self, name: &str) -> Option<&str> {
        self.get_named_item(name).map(|a| a.value.as_str())
    }
    
    /// Set attribute
    pub fn set_named_item(&mut self, attr: Attr) -> Option<Attr> {
        let name = attr.name.clone();
        if let Some(&index) = self.by_name.get(&name) {
            let old = std::mem::replace(&mut self.attributes[index], attr);
            Some(old)
        } else {
            let index = self.attributes.len();
            self.by_name.insert(name, index);
            self.attributes.push(attr);
            None
        }
    }
    
    /// Set attribute by name/value
    pub fn set_attribute(&mut self, name: &str, value: &str) {
        self.set_named_item(Attr::new(name, value));
    }
    
    /// Remove attribute by name
    pub fn remove_named_item(&mut self, name: &str) -> Option<Attr> {
        if let Some(&index) = self.by_name.get(name) {
            self.by_name.remove(name);
            // Update indices for items after removed
            for (_, idx) in self.by_name.iter_mut() {
                if *idx > index {
                    *idx -= 1;
                }
            }
            Some(self.attributes.remove(index))
        } else {
            None
        }
    }
    
    /// Check if attribute exists
    pub fn has_attribute(&self, name: &str) -> bool {
        self.by_name.contains_key(name)
    }
    
    /// Toggle attribute
    pub fn toggle_attribute(&mut self, name: &str, force: Option<bool>) -> bool {
        match force {
            Some(true) => {
                if !self.has_attribute(name) {
                    self.set_attribute(name, "");
                }
                true
            }
            Some(false) => {
                self.remove_named_item(name);
                false
            }
            None => {
                if self.has_attribute(name) {
                    self.remove_named_item(name);
                    false
                } else {
                    self.set_attribute(name, "");
                    true
                }
            }
        }
    }
    
    /// Get attribute names
    pub fn get_attribute_names(&self) -> Vec<&str> {
        self.attributes.iter().map(|a| a.name.as_str()).collect()
    }
    
    /// Iterate over attributes
    pub fn iter(&self) -> impl Iterator<Item = &Attr> {
        self.attributes.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_set_get_attribute() {
        let mut attrs = NamedNodeMap::new();
        attrs.set_attribute("class", "btn");
        attrs.set_attribute("id", "submit");
        
        assert_eq!(attrs.length(), 2);
        assert_eq!(attrs.get_attribute("class"), Some("btn"));
        assert_eq!(attrs.get_attribute("id"), Some("submit"));
    }
    
    #[test]
    fn test_remove_attribute() {
        let mut attrs = NamedNodeMap::new();
        attrs.set_attribute("foo", "bar");
        
        assert!(attrs.has_attribute("foo"));
        attrs.remove_named_item("foo");
        assert!(!attrs.has_attribute("foo"));
    }
    
    #[test]
    fn test_toggle_attribute() {
        let mut attrs = NamedNodeMap::new();
        
        assert!(attrs.toggle_attribute("disabled", None));
        assert!(attrs.has_attribute("disabled"));
        
        assert!(!attrs.toggle_attribute("disabled", None));
        assert!(!attrs.has_attribute("disabled"));
    }
}
