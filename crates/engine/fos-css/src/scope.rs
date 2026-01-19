//! CSS @scope
//!
//! Implementation of CSS Scoping specification.
//! Allows scoped styling with proximity-based specificity.

use std::collections::HashMap;

// ============================================================================
// Scope Types
// ============================================================================

/// A CSS scope definition
#[derive(Debug, Clone)]
pub struct CssScope {
    /// Unique scope ID
    pub id: ScopeId,
    /// Scoping root selector (start of scope)
    pub root: Box<str>,
    /// Scoping limit selector (end of scope, optional)
    pub limit: Option<Box<str>>,
    /// Rules within this scope
    pub rules: Vec<ScopedRule>,
}

/// Unique identifier for a scope
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScopeId(pub u32);

/// A rule within a scope
#[derive(Debug, Clone)]
pub struct ScopedRule {
    /// Selector (may contain :scope)
    pub selector: Box<str>,
    /// Declarations
    pub declarations: Vec<ScopedDeclaration>,
    /// Proximity weight (lower = closer to root = higher priority)
    pub proximity: i32,
}

/// Declaration within a scope
#[derive(Debug, Clone)]
pub struct ScopedDeclaration {
    pub property: Box<str>,
    pub value: Box<str>,
    pub important: bool,
}

// ============================================================================
// Scope Registry
// ============================================================================

/// Registry of CSS scopes
#[derive(Debug)]
pub struct ScopeRegistry {
    /// All registered scopes
    scopes: HashMap<ScopeId, CssScope>,
    /// Next scope ID
    next_id: u32,
}

impl Default for ScopeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ScopeRegistry {
    pub fn new() -> Self {
        Self {
            scopes: HashMap::new(),
            next_id: 1,
        }
    }
    
    /// Register a new scope
    pub fn register(
        &mut self,
        root: &str,
        limit: Option<&str>,
    ) -> ScopeId {
        let id = ScopeId(self.next_id);
        self.next_id += 1;
        
        self.scopes.insert(id, CssScope {
            id,
            root: root.into(),
            limit: limit.map(|s| s.into()),
            rules: Vec::new(),
        });
        
        id
    }
    
    /// Add a rule to a scope
    pub fn add_rule(
        &mut self,
        scope_id: ScopeId,
        selector: &str,
        declarations: Vec<ScopedDeclaration>,
    ) {
        if let Some(scope) = self.scopes.get_mut(&scope_id) {
            scope.rules.push(ScopedRule {
                selector: selector.into(),
                declarations,
                proximity: 0, // Will be calculated during matching
            });
        }
    }
    
    /// Get a scope by ID
    pub fn get(&self, id: ScopeId) -> Option<&CssScope> {
        self.scopes.get(&id)
    }
    
    /// Get all scopes
    pub fn all_scopes(&self) -> impl Iterator<Item = &CssScope> {
        self.scopes.values()
    }
    
    /// Number of scopes
    pub fn len(&self) -> usize {
        self.scopes.len()
    }
    
    /// Is empty
    pub fn is_empty(&self) -> bool {
        self.scopes.is_empty()
    }
}

// ============================================================================
// Scope Matching
// ============================================================================

/// Result of scope matching for an element
#[derive(Debug, Clone)]
pub struct ScopeMatch {
    /// The scope that matched
    pub scope_id: ScopeId,
    /// Distance from scoping root (for proximity weighting)
    pub proximity: i32,
    /// The scoping root element
    pub root_element: u32,
}

/// Scope matcher for DOM traversal
#[derive(Debug)]
pub struct ScopeMatcher {
    /// Active scopes during traversal
    active_scopes: Vec<ActiveScope>,
}

/// An active scope during DOM traversal
#[derive(Debug, Clone)]
struct ActiveScope {
    scope_id: ScopeId,
    root_element: u32,
    depth: usize,
}

impl Default for ScopeMatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl ScopeMatcher {
    pub fn new() -> Self {
        Self {
            active_scopes: Vec::new(),
        }
    }
    
    /// Enter a scoping root element
    pub fn enter_scope(&mut self, scope_id: ScopeId, element_id: u32, depth: usize) {
        self.active_scopes.push(ActiveScope {
            scope_id,
            root_element: element_id,
            depth,
        });
    }
    
    /// Exit a scoping root element
    pub fn exit_scope(&mut self, _element_id: u32, depth: usize) {
        // Remove scopes that started at or after this depth
        self.active_scopes.retain(|s| s.depth < depth);
    }
    
    /// Check if an element is within any active scope and calculate proximity
    pub fn get_scope_matches(&self, current_depth: usize) -> Vec<ScopeMatch> {
        self.active_scopes.iter().map(|scope| {
            ScopeMatch {
                scope_id: scope.scope_id,
                proximity: (current_depth - scope.depth) as i32,
                root_element: scope.root_element,
            }
        }).collect()
    }
    
    /// Check if element is within a specific scope
    pub fn is_in_scope(&self, scope_id: ScopeId) -> bool {
        self.active_scopes.iter().any(|s| s.scope_id == scope_id)
    }
    
    /// Get the proximity for a specific scope (or None if not in scope)
    pub fn proximity_for(&self, scope_id: ScopeId, current_depth: usize) -> Option<i32> {
        self.active_scopes.iter()
            .find(|s| s.scope_id == scope_id)
            .map(|s| (current_depth - s.depth) as i32)
    }
    
    /// Clear all active scopes
    pub fn clear(&mut self) {
        self.active_scopes.clear();
    }
}

// ============================================================================
// Scope Parser
// ============================================================================

/// Parsed @scope statement
#[derive(Debug, Clone)]
pub struct ScopeStatement {
    /// Scoping root selector
    pub root: Box<str>,
    /// Scoping limit selector (optional)
    pub limit: Option<Box<str>>,
    /// CSS content within the scope
    pub content: Box<str>,
}

/// Parse @scope statements from CSS
pub fn parse_scope_statements(css: &str) -> Vec<ScopeStatement> {
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
        
        // Check for @scope
        if css[pos..].starts_with("@scope") {
            pos += 6;
            
            // Skip whitespace
            while pos < chars.len() && chars[pos].is_whitespace() {
                pos += 1;
            }
            
            let mut root = String::new();
            let mut limit = None;
            
            // Check for (root) or (root to limit)
            if chars.get(pos) == Some(&'(') {
                pos += 1;
                let mut paren_depth = 1;
                let mut in_to = false;
                let mut current = String::new();
                
                while pos < chars.len() && paren_depth > 0 {
                    let c = chars[pos];
                    
                    if c == '(' {
                        paren_depth += 1;
                        current.push(c);
                    } else if c == ')' {
                        paren_depth -= 1;
                        if paren_depth > 0 {
                            current.push(c);
                        }
                    } else if !in_to && css[pos..].starts_with(" to ") {
                        root = current.trim().to_string();
                        current.clear();
                        in_to = true;
                        pos += 4; // Skip " to "
                        continue;
                    } else {
                        current.push(c);
                    }
                    
                    pos += 1;
                }
                
                if in_to {
                    limit = Some(current.trim().to_string());
                } else {
                    root = current.trim().to_string();
                }
            }
            
            // Skip whitespace
            while pos < chars.len() && chars[pos].is_whitespace() {
                pos += 1;
            }
            
            // Expect {
            if chars.get(pos) == Some(&'{') {
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
                
                statements.push(ScopeStatement {
                    root: root.into(),
                    limit: limit.map(|s| s.into()),
                    content,
                });
            }
        } else {
            // Skip to next @ or end
            while pos < chars.len() && chars[pos] != '@' {
                if chars[pos] == '{' {
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

/// Resolve :scope selector within a scoped rule
pub fn resolve_scope_selector(selector: &str, root_selector: &str) -> String {
    if selector.contains(":scope") {
        selector.replace(":scope", root_selector)
    } else {
        // Implicit :scope at start for descendants
        format!("{} {}", root_selector, selector)
    }
}

// ============================================================================
// Proximity-Based Specificity
// ============================================================================

/// Compare two scoped declarations by proximity
/// Returns true if A wins (closer to scoping root)
pub fn closer_scope_wins(
    a_proximity: i32,
    b_proximity: i32,
) -> bool {
    // Lower proximity = closer to root = wins
    a_proximity < b_proximity
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_scope_registry() {
        let mut registry = ScopeRegistry::new();
        
        let id = registry.register(".card", None);
        assert!(registry.get(id).is_some());
        
        let scope = registry.get(id).unwrap();
        assert_eq!(scope.root.as_ref(), ".card");
        assert!(scope.limit.is_none());
    }
    
    #[test]
    fn test_scope_with_limit() {
        let mut registry = ScopeRegistry::new();
        
        let id = registry.register(".card", Some(".card-footer"));
        let scope = registry.get(id).unwrap();
        
        assert_eq!(scope.root.as_ref(), ".card");
        assert_eq!(scope.limit.as_ref().map(|s| s.as_ref()), Some(".card-footer"));
    }
    
    #[test]
    fn test_scope_matcher() {
        let mut matcher = ScopeMatcher::new();
        
        let scope_id = ScopeId(1);
        
        // Enter scope at depth 2
        matcher.enter_scope(scope_id, 100, 2);
        
        // Check from depth 5
        let matches = matcher.get_scope_matches(5);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].proximity, 3); // 5 - 2 = 3
    }
    
    #[test]
    fn test_parse_scope() {
        let css = "@scope (.card) {
            .title { color: red; }
        }";
        
        let statements = parse_scope_statements(css);
        
        assert_eq!(statements.len(), 1);
        assert_eq!(statements[0].root.as_ref(), ".card");
        assert!(statements[0].limit.is_none());
    }
    
    #[test]
    fn test_parse_scope_with_limit() {
        let css = "@scope (.card to .card-footer) {
            .content { padding: 10px; }
        }";
        
        let statements = parse_scope_statements(css);
        
        assert_eq!(statements.len(), 1);
        assert_eq!(statements[0].root.as_ref(), ".card");
        assert_eq!(statements[0].limit.as_ref().map(|s| s.as_ref()), Some(".card-footer"));
    }
    
    #[test]
    fn test_resolve_scope_selector() {
        let resolved = resolve_scope_selector(":scope > .child", ".parent");
        assert_eq!(resolved, ".parent > .child");
        
        let resolved = resolve_scope_selector(".descendant", ".parent");
        assert_eq!(resolved, ".parent .descendant");
    }
    
    #[test]
    fn test_proximity_comparison() {
        // Closer scope wins
        assert!(closer_scope_wins(1, 3));
        assert!(!closer_scope_wins(3, 1));
        assert!(!closer_scope_wins(2, 2)); // Equal - second wins
    }
}
