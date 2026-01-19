//! CSS Cascade Layers (@layer)
//!
//! Implementation of CSS Cascade Layers specification.
//! Allows explicit control over the cascade order of rules.

use std::collections::HashMap;

// ============================================================================
// Layer Types
// ============================================================================

/// Unique identifier for a cascade layer
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LayerId(pub u32);

impl LayerId {
    /// The implicit outer layer (unlayered styles)
    pub const UNLAYERED: LayerId = LayerId(0);
}

/// A cascade layer
#[derive(Debug, Clone)]
pub struct CascadeLayer {
    /// Layer ID
    pub id: LayerId,
    /// Layer name (dot-separated for nested)
    pub name: Option<Box<str>>,
    /// Parent layer ID (for nested layers)
    pub parent: Option<LayerId>,
    /// Order in which this layer was declared
    pub order: u32,
    /// Child layers (for nested layer lookup)
    pub children: Vec<LayerId>,
}

/// A rule within a layer
#[derive(Debug, Clone)]
pub struct LayeredRule {
    /// Which layer this rule belongs to
    pub layer_id: LayerId,
    /// Selector
    pub selector: Box<str>,
    /// Declarations
    pub declarations: Vec<LayerDeclaration>,
    /// Source order within the layer
    pub source_order: u32,
}

/// Declaration within a layer
#[derive(Debug, Clone)]
pub struct LayerDeclaration {
    pub property: Box<str>,
    pub value: Box<str>,
    pub important: bool,
}

// ============================================================================
// Layer Registry
// ============================================================================

/// Registry of cascade layers
#[derive(Debug)]
pub struct LayerRegistry {
    /// All registered layers
    layers: HashMap<LayerId, CascadeLayer>,
    /// Name to ID mapping
    name_to_id: HashMap<Box<str>, LayerId>,
    /// Ordered list of layer IDs (by declaration order)
    order: Vec<LayerId>,
    /// Next layer ID
    next_id: u32,
    /// Rules by layer
    rules: HashMap<LayerId, Vec<LayeredRule>>,
}

impl Default for LayerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl LayerRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            layers: HashMap::new(),
            name_to_id: HashMap::new(),
            order: Vec::new(),
            next_id: 1,
            rules: HashMap::new(),
        };
        
        // Register the implicit unlayered layer (always last in order)
        registry.layers.insert(LayerId::UNLAYERED, CascadeLayer {
            id: LayerId::UNLAYERED,
            name: None,
            parent: None,
            order: u32::MAX, // Unlayered styles come last
            children: Vec::new(),
        });
        
        registry
    }
    
    /// Declare layer(s) by name, establishing order
    /// Names can be dot-separated for nested layers: "framework.reset"
    pub fn declare_layers(&mut self, names: &[&str]) {
        for name in names {
            self.get_or_create_layer(name);
        }
    }
    
    /// Get or create a layer by name
    pub fn get_or_create_layer(&mut self, name: &str) -> LayerId {
        // Check if already exists
        if let Some(&id) = self.name_to_id.get(name) {
            return id;
        }
        
        // Handle nested layers (e.g., "framework.reset")
        let parts: Vec<&str> = name.split('.').collect();
        
        let mut parent_id = None;
        let mut full_name = String::new();
        
        for (i, part) in parts.iter().enumerate() {
            if i > 0 {
                full_name.push('.');
            }
            full_name.push_str(part);
            
            if let Some(&id) = self.name_to_id.get(full_name.as_str()) {
                parent_id = Some(id);
            } else {
                // Create this layer
                let id = LayerId(self.next_id);
                self.next_id += 1;
                
                let order = self.order.len() as u32;
                
                let layer = CascadeLayer {
                    id,
                    name: Some(full_name.clone().into()),
                    parent: parent_id,
                    order,
                    children: Vec::new(),
                };
                
                // Add to parent's children
                if let Some(pid) = parent_id {
                    if let Some(parent) = self.layers.get_mut(&pid) {
                        parent.children.push(id);
                    }
                }
                
                self.layers.insert(id, layer);
                self.name_to_id.insert(full_name.clone().into(), id);
                self.order.push(id);
                
                parent_id = Some(id);
            }
        }
        
        parent_id.unwrap_or(LayerId::UNLAYERED)
    }
    
    /// Get a layer by name
    pub fn get_layer(&self, name: &str) -> Option<LayerId> {
        self.name_to_id.get(name).copied()
    }
    
    /// Get layer info
    pub fn get_layer_info(&self, id: LayerId) -> Option<&CascadeLayer> {
        self.layers.get(&id)
    }
    
    /// Add a rule to a layer
    pub fn add_rule(&mut self, layer_id: LayerId, selector: &str, declarations: Vec<LayerDeclaration>) {
        let rules = self.rules.entry(layer_id).or_default();
        let source_order = rules.len() as u32;
        
        rules.push(LayeredRule {
            layer_id,
            selector: selector.into(),
            declarations,
            source_order,
        });
    }
    
    /// Get rules for a layer
    pub fn get_rules(&self, layer_id: LayerId) -> Option<&[LayeredRule]> {
        self.rules.get(&layer_id).map(|v| v.as_slice())
    }
    
    /// Compare layer orders for cascade
    /// Returns Ordering for layer priority (lower = earlier in cascade = lower priority)
    pub fn layer_order(&self, a: LayerId, b: LayerId) -> std::cmp::Ordering {
        let order_a = self.layers.get(&a).map(|l| l.order).unwrap_or(u32::MAX);
        let order_b = self.layers.get(&b).map(|l| l.order).unwrap_or(u32::MAX);
        order_a.cmp(&order_b)
    }
    
    /// Check if layer A comes before layer B in cascade
    pub fn layer_precedes(&self, a: LayerId, b: LayerId) -> bool {
        self.layer_order(a, b) == std::cmp::Ordering::Less
    }
    
    /// Get all layers in order
    pub fn layers_in_order(&self) -> Vec<LayerId> {
        let mut layers: Vec<LayerId> = self.layers.keys().copied().collect();
        layers.sort_by(|a, b| self.layer_order(*a, *b));
        layers
    }
    
    /// Get all rules in cascade order (by layer, then source order)
    pub fn all_rules_in_order(&self) -> Vec<&LayeredRule> {
        let mut all_rules = Vec::new();
        
        for layer_id in self.layers_in_order() {
            if let Some(rules) = self.rules.get(&layer_id) {
                all_rules.extend(rules.iter());
            }
        }
        
        all_rules
    }
    
    /// Number of layers
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }
    
    /// Clear all layers and rules (except unlayered)
    pub fn clear(&mut self) {
        self.layers.retain(|id, _| *id == LayerId::UNLAYERED);
        self.name_to_id.clear();
        self.order.clear();
        self.rules.clear();
        self.next_id = 1;
    }
}

// ============================================================================
// Layer Statement Parser
// ============================================================================

/// Parsed @layer statement
#[derive(Debug, Clone)]
pub enum LayerStatement {
    /// @layer name; - declaration only
    Declaration(Vec<Box<str>>),
    /// @layer name { ... } - layer block with rules
    Block {
        name: Option<Box<str>>,
        content: Box<str>,
    },
}

/// Parse @layer statements from CSS
pub fn parse_layer_statements(css: &str) -> Vec<LayerStatement> {
    let mut statements = Vec::new();
    let mut pos = 0;
    let chars: Vec<char> = css.chars().collect();
    
    while pos < chars.len() {
        // Skip whitespace
        while pos < chars.len() && chars[pos].is_whitespace() {
            pos += 1;
        }
        
        if pos >= chars.len() {
            break;
        }
        
        // Check for @layer
        if css[pos..].starts_with("@layer") {
            pos += 6;
            
            // Skip whitespace
            while pos < chars.len() && chars[pos].is_whitespace() {
                pos += 1;
            }
            
            // Collect layer names or block
            let mut names = Vec::new();
            let mut current_name = String::new();
            
            while pos < chars.len() {
                let c = chars[pos];
                
                if c == '{' {
                    // Block statement
                    let name = if current_name.is_empty() {
                        None
                    } else {
                        Some(current_name.trim().into())
                    };
                    
                    // Find matching }
                    let block_start = pos + 1;
                    let mut depth = 1;
                    pos += 1;
                    
                    while pos < chars.len() && depth > 0 {
                        if chars[pos] == '{' {
                            depth += 1;
                        } else if chars[pos] == '}' {
                            depth -= 1;
                        }
                        pos += 1;
                    }
                    
                    let content: Box<str> = chars[block_start..pos - 1].iter().collect::<String>().into();
                    
                    statements.push(LayerStatement::Block { name, content });
                    break;
                } else if c == ';' {
                    // Declaration statement
                    if !current_name.is_empty() {
                        names.push(current_name.trim().into());
                    }
                    
                    statements.push(LayerStatement::Declaration(names));
                    pos += 1;
                    break;
                } else if c == ',' {
                    if !current_name.is_empty() {
                        names.push(current_name.trim().into());
                        current_name.clear();
                    }
                    pos += 1;
                } else {
                    current_name.push(c);
                    pos += 1;
                }
            }
        } else {
            // Skip to next @ or end
            while pos < chars.len() && chars[pos] != '@' {
                if chars[pos] == '{' {
                    // Skip block
                    let mut depth = 1;
                    pos += 1;
                    while pos < chars.len() && depth > 0 {
                        if chars[pos] == '{' {
                            depth += 1;
                        } else if chars[pos] == '}' {
                            depth -= 1;
                        }
                        pos += 1;
                    }
                } else {
                    pos += 1;
                }
            }
        }
    }
    
    statements
}

// ============================================================================
// Layer-Aware Cascade
// ============================================================================

/// Compare two declarations considering layer order
/// Returns true if declaration A wins over declaration B
pub fn layer_wins(
    registry: &LayerRegistry,
    a_layer: LayerId,
    a_important: bool,
    b_layer: LayerId,
    b_important: bool,
) -> bool {
    // Important declarations reverse layer order
    match (a_important, b_important) {
        (true, true) => {
            // Both important - earlier layer wins (reversed order)
            registry.layer_precedes(a_layer, b_layer)
        }
        (true, false) => true,  // Important always wins
        (false, true) => false, // Important always wins
        (false, false) => {
            // Both normal - later layer wins
            !registry.layer_precedes(a_layer, b_layer)
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_layer_declaration() {
        let mut registry = LayerRegistry::new();
        
        registry.declare_layers(&["reset", "base", "components"]);
        
        assert!(registry.get_layer("reset").is_some());
        assert!(registry.get_layer("base").is_some());
        assert!(registry.get_layer("components").is_some());
        
        // Order should be maintained
        let reset = registry.get_layer("reset").unwrap();
        let base = registry.get_layer("base").unwrap();
        let components = registry.get_layer("components").unwrap();
        
        assert!(registry.layer_precedes(reset, base));
        assert!(registry.layer_precedes(base, components));
    }
    
    #[test]
    fn test_nested_layers() {
        let mut registry = LayerRegistry::new();
        
        let id = registry.get_or_create_layer("framework.reset");
        
        assert!(registry.get_layer("framework").is_some());
        assert!(registry.get_layer("framework.reset").is_some());
        
        let parent = registry.get_layer_info(registry.get_layer("framework").unwrap()).unwrap();
        assert!(parent.children.contains(&id));
    }
    
    #[test]
    fn test_layer_order() {
        let mut registry = LayerRegistry::new();
        
        registry.declare_layers(&["first", "second", "third"]);
        
        let first = registry.get_layer("first").unwrap();
        let second = registry.get_layer("second").unwrap();
        let third = registry.get_layer("third").unwrap();
        
        // Normal declarations: later layer wins
        assert!(!layer_wins(&registry, first, false, second, false));
        assert!(!layer_wins(&registry, second, false, third, false));
        
        // Important declarations: earlier layer wins
        assert!(layer_wins(&registry, first, true, second, true));
    }
    
    #[test]
    fn test_parse_layer_declaration() {
        let css = "@layer reset, base, components;";
        let statements = parse_layer_statements(css);
        
        assert_eq!(statements.len(), 1);
        assert!(matches!(&statements[0], LayerStatement::Declaration(names) if names.len() == 3));
    }
    
    #[test]
    fn test_parse_layer_block() {
        let css = "@layer reset {
            * { margin: 0; }
        }";
        
        let statements = parse_layer_statements(css);
        
        assert_eq!(statements.len(), 1);
        assert!(matches!(&statements[0], LayerStatement::Block { name: Some(_), .. }));
    }
    
    #[test]
    fn test_unlayered_last() {
        let mut registry = LayerRegistry::new();
        
        registry.declare_layers(&["base"]);
        
        let base = registry.get_layer("base").unwrap();
        
        // Unlayered should come after all named layers
        assert!(registry.layer_precedes(base, LayerId::UNLAYERED));
    }
    
    #[test]
    fn test_add_rules() {
        let mut registry = LayerRegistry::new();
        
        let layer_id = registry.get_or_create_layer("base");
        
        registry.add_rule(layer_id, ".test", vec![
            LayerDeclaration {
                property: "color".into(),
                value: "red".into(),
                important: false,
            },
        ]);
        
        let rules = registry.get_rules(layer_id).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].selector.as_ref(), ".test");
    }
}
