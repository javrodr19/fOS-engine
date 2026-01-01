//! CSS Advanced Selectors Module
//!
//! Implements pseudo-elements, pseudo-classes, and advanced selector matching.

use std::collections::HashMap;

/// Pseudo-element type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PseudoElement {
    /// ::before - content before element
    Before,
    /// ::after - content after element
    After,
    /// ::first-line - first line of text
    FirstLine,
    /// ::first-letter - first letter of text
    FirstLetter,
    /// ::marker - list marker
    Marker,
    /// ::selection - selected text
    Selection,
    /// ::placeholder - input placeholder
    Placeholder,
    /// ::backdrop - fullscreen backdrop
    Backdrop,
}

impl PseudoElement {
    /// Parse from string
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "before" | "::before" => Some(Self::Before),
            "after" | "::after" => Some(Self::After),
            "first-line" | "::first-line" => Some(Self::FirstLine),
            "first-letter" | "::first-letter" => Some(Self::FirstLetter),
            "marker" | "::marker" => Some(Self::Marker),
            "selection" | "::selection" => Some(Self::Selection),
            "placeholder" | "::placeholder" => Some(Self::Placeholder),
            "backdrop" | "::backdrop" => Some(Self::Backdrop),
            _ => None,
        }
    }
    
    /// Check if this pseudo-element requires generated content
    pub fn requires_content(&self) -> bool {
        matches!(self, Self::Before | Self::After)
    }
}

/// Pseudo-class type
#[derive(Debug, Clone, PartialEq)]
pub enum PseudoClass {
    // Link pseudo-classes
    Link,
    Visited,
    
    // User action pseudo-classes
    Hover,
    Active,
    Focus,
    FocusVisible,
    FocusWithin,
    
    // Input pseudo-classes
    Enabled,
    Disabled,
    Checked,
    Indeterminate,
    Required,
    Optional,
    Valid,
    Invalid,
    ReadOnly,
    ReadWrite,
    PlaceholderShown,
    Default,
    
    // Tree-structural pseudo-classes
    Root,
    Empty,
    FirstChild,
    LastChild,
    OnlyChild,
    FirstOfType,
    LastOfType,
    OnlyOfType,
    NthChild(NthExpression),
    NthLastChild(NthExpression),
    NthOfType(NthExpression),
    NthLastOfType(NthExpression),
    
    // Logical pseudo-classes
    Not(Box<SelectorComponent>),
    Is(Vec<SelectorComponent>),
    Where(Vec<SelectorComponent>),
    Has(Vec<SelectorComponent>),
    
    // Scope pseudo-class (matches the element the selector is scoped to)
    Scope,
    
    // Other
    Target,
    Lang(String),
    Dir(Direction),
}

/// Direction for :dir() pseudo-class
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Ltr,
    Rtl,
}

/// An+B expression for :nth-* selectors
#[derive(Debug, Clone, PartialEq)]
pub struct NthExpression {
    /// Coefficient (A in An+B)
    pub a: i32,
    /// Offset (B in An+B)
    pub b: i32,
}

impl NthExpression {
    /// Create "odd" expression (2n+1)
    pub fn odd() -> Self {
        Self { a: 2, b: 1 }
    }
    
    /// Create "even" expression (2n)
    pub fn even() -> Self {
        Self { a: 2, b: 0 }
    }
    
    /// Create a simple index (0n+b)
    pub fn index(n: i32) -> Self {
        Self { a: 0, b: n }
    }
    
    /// Create An+B expression
    pub fn new(a: i32, b: i32) -> Self {
        Self { a, b }
    }
    
    /// Parse from string like "2n+1", "odd", "even", "3"
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().to_lowercase();
        
        match s.as_str() {
            "odd" => return Some(Self::odd()),
            "even" => return Some(Self::even()),
            _ => {}
        }
        
        // Try to parse as simple number
        if let Ok(n) = s.parse::<i32>() {
            return Some(Self::index(n));
        }
        
        // Parse An+B format
        let s = s.replace(" ", "");
        
        if let Some(n_pos) = s.find('n') {
            let a_str = &s[..n_pos];
            let a = if a_str.is_empty() || a_str == "+" {
                1
            } else if a_str == "-" {
                -1
            } else {
                a_str.parse().ok()?
            };
            
            let rest = &s[n_pos + 1..];
            let b = if rest.is_empty() {
                0
            } else {
                rest.parse().ok()?
            };
            
            return Some(Self::new(a, b));
        }
        
        None
    }
    
    /// Check if index n (1-based) matches this expression
    pub fn matches(&self, n: i32) -> bool {
        if self.a == 0 {
            return n == self.b;
        }
        
        let diff = n - self.b;
        if self.a > 0 {
            diff >= 0 && diff % self.a == 0
        } else {
            diff <= 0 && diff % self.a == 0
        }
    }
}

/// A component of a selector
#[derive(Debug, Clone, PartialEq)]
pub enum SelectorComponent {
    /// Universal selector *
    Universal,
    /// Type selector (tag name)
    Type(String),
    /// ID selector #id
    Id(String),
    /// Class selector .class
    Class(String),
    /// Attribute selector [attr], [attr=value], etc.
    Attribute(AttributeSelector),
    /// Pseudo-class :hover, :nth-child(), etc.
    PseudoClass(PseudoClass),
    /// Pseudo-element ::before, ::after
    PseudoElement(PseudoElement),
}

/// Attribute selector
#[derive(Debug, Clone, PartialEq)]
pub struct AttributeSelector {
    pub name: String,
    pub matcher: Option<AttributeMatcher>,
    pub case_insensitive: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AttributeMatcher {
    /// [attr=value] - exact match
    Exact(String),
    /// [attr~=value] - whitespace-separated list contains
    Contains(String),
    /// [attr|=value] - exact or prefix with hyphen
    DashMatch(String),
    /// [attr^=value] - starts with
    Prefix(String),
    /// [attr$=value] - ends with
    Suffix(String),
    /// [attr*=value] - contains substring
    Substring(String),
}

impl AttributeSelector {
    /// Check if an attribute value matches
    pub fn matches(&self, value: Option<&str>) -> bool {
        match (&self.matcher, value) {
            (None, Some(_)) => true, // [attr] - just check existence
            (None, None) => false,
            (Some(_), None) => false,
            (Some(matcher), Some(val)) => {
                let val = if self.case_insensitive {
                    val.to_lowercase()
                } else {
                    val.to_string()
                };
                
                match matcher {
                    AttributeMatcher::Exact(expected) => {
                        let expected = if self.case_insensitive {
                            expected.to_lowercase()
                        } else {
                            expected.clone()
                        };
                        val == expected
                    }
                    AttributeMatcher::Contains(expected) => {
                        val.split_whitespace().any(|w| {
                            if self.case_insensitive {
                                w.to_lowercase() == expected.to_lowercase()
                            } else {
                                w == expected
                            }
                        })
                    }
                    AttributeMatcher::DashMatch(expected) => {
                        val == *expected || val.starts_with(&format!("{}-", expected))
                    }
                    AttributeMatcher::Prefix(expected) => val.starts_with(expected),
                    AttributeMatcher::Suffix(expected) => val.ends_with(expected),
                    AttributeMatcher::Substring(expected) => val.contains(expected),
                }
            }
        }
    }
}

/// Element context for selector matching
pub struct ElementContext<'a> {
    /// Tag name
    pub tag_name: &'a str,
    /// ID attribute
    pub id: Option<&'a str>,
    /// Class list
    pub classes: &'a [String],
    /// Attributes
    pub attributes: &'a HashMap<String, String>,
    /// Index among siblings (1-based)
    pub sibling_index: usize,
    /// Total siblings count
    pub sibling_count: usize,
    /// Index among same-type siblings (1-based)
    pub type_index: usize,
    /// Total same-type siblings count
    pub type_count: usize,
    /// Element states
    pub states: ElementStates,
}

/// Element interaction states
#[derive(Debug, Clone, Copy, Default)]
pub struct ElementStates {
    pub hover: bool,
    pub active: bool,
    pub focus: bool,
    pub focus_visible: bool,
    pub visited: bool,
    pub checked: bool,
    pub disabled: bool,
    pub required: bool,
    pub valid: bool,
    pub read_only: bool,
    pub placeholder_shown: bool,
    pub is_target: bool,
    pub is_root: bool,
    pub is_empty: bool,
    pub is_scope: bool,
}

/// Match a selector component against an element
pub fn match_component(component: &SelectorComponent, element: &ElementContext) -> bool {
    match component {
        SelectorComponent::Universal => true,
        SelectorComponent::Type(tag) => element.tag_name.eq_ignore_ascii_case(tag),
        SelectorComponent::Id(id) => element.id == Some(id.as_str()),
        SelectorComponent::Class(class) => element.classes.iter().any(|c| c == class),
        SelectorComponent::Attribute(attr) => {
            attr.matches(element.attributes.get(&attr.name).map(|s| s.as_str()))
        }
        SelectorComponent::PseudoClass(pseudo) => match_pseudo_class(pseudo, element),
        SelectorComponent::PseudoElement(_) => true, // Pseudo-elements are handled separately
    }
}

/// Match a pseudo-class against an element
pub fn match_pseudo_class(pseudo: &PseudoClass, element: &ElementContext) -> bool {
    match pseudo {
        // Link pseudo-classes
        PseudoClass::Link => {
            element.tag_name.eq_ignore_ascii_case("a") && 
            element.attributes.contains_key("href")
        }
        PseudoClass::Visited => element.states.visited,
        
        // User action pseudo-classes
        PseudoClass::Hover => element.states.hover,
        PseudoClass::Active => element.states.active,
        PseudoClass::Focus => element.states.focus,
        PseudoClass::FocusVisible => element.states.focus_visible,
        PseudoClass::FocusWithin => element.states.focus, // Simplified
        
        // Input pseudo-classes
        PseudoClass::Enabled => !element.states.disabled,
        PseudoClass::Disabled => element.states.disabled,
        PseudoClass::Checked => element.states.checked,
        PseudoClass::Indeterminate => false, // Simplified
        PseudoClass::Required => element.states.required,
        PseudoClass::Optional => !element.states.required,
        PseudoClass::Valid => element.states.valid,
        PseudoClass::Invalid => !element.states.valid,
        PseudoClass::ReadOnly => element.states.read_only,
        PseudoClass::ReadWrite => !element.states.read_only,
        PseudoClass::PlaceholderShown => element.states.placeholder_shown,
        PseudoClass::Default => false, // Simplified
        
        // Tree-structural pseudo-classes
        PseudoClass::Root => element.states.is_root,
        PseudoClass::Empty => element.states.is_empty,
        PseudoClass::FirstChild => element.sibling_index == 1,
        PseudoClass::LastChild => element.sibling_index == element.sibling_count,
        PseudoClass::OnlyChild => element.sibling_count == 1,
        PseudoClass::FirstOfType => element.type_index == 1,
        PseudoClass::LastOfType => element.type_index == element.type_count,
        PseudoClass::OnlyOfType => element.type_count == 1,
        PseudoClass::NthChild(expr) => expr.matches(element.sibling_index as i32),
        PseudoClass::NthLastChild(expr) => {
            let from_end = element.sibling_count - element.sibling_index + 1;
            expr.matches(from_end as i32)
        }
        PseudoClass::NthOfType(expr) => expr.matches(element.type_index as i32),
        PseudoClass::NthLastOfType(expr) => {
            let from_end = element.type_count - element.type_index + 1;
            expr.matches(from_end as i32)
        }
        
        // Logical pseudo-classes
        PseudoClass::Not(selector) => !match_component(selector, element),
        PseudoClass::Is(selectors) | PseudoClass::Where(selectors) => {
            selectors.iter().any(|s| match_component(s, element))
        }
        PseudoClass::Has(_) => false, // Complex - requires checking descendants
        
        // Scope pseudo-class
        PseudoClass::Scope => element.states.is_scope,
        
        // Other
        PseudoClass::Target => element.states.is_target,
        PseudoClass::Lang(lang) => {
            element.attributes.get("lang")
                .map(|l| l.starts_with(lang))
                .unwrap_or(false)
        }
        PseudoClass::Dir(dir) => {
            let attr_dir = element.attributes.get("dir");
            match dir {
                Direction::Ltr => attr_dir.map(|d| d == "ltr").unwrap_or(true),
                Direction::Rtl => attr_dir.map(|d| d == "rtl").unwrap_or(false),
            }
        }
    }
}

/// Bloom filter for fast selector rejection
/// 
/// Uses a 256-bit (32-byte) bloom filter to quickly reject
/// selectors that cannot possibly match an element's ancestors.
#[derive(Clone, Default)]
pub struct SelectorBloomFilter {
    bits: [u64; 4],
}

impl SelectorBloomFilter {
    /// Create a new empty bloom filter
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Hash a string to filter indices
    fn hash(s: &str) -> (usize, usize) {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut h1 = DefaultHasher::new();
        s.hash(&mut h1);
        let hash1 = h1.finish();
        
        let mut h2 = DefaultHasher::new();
        s.len().hash(&mut h2);
        s.hash(&mut h2);
        let hash2 = h2.finish();
        
        ((hash1 as usize) % 256, (hash2 as usize) % 256)
    }
    
    /// Insert an element identifier (tag, class, or id)
    pub fn insert(&mut self, s: &str) {
        let (i1, i2) = Self::hash(s);
        self.set_bit(i1);
        self.set_bit(i2);
    }
    
    /// Insert an element's tag name
    pub fn insert_tag(&mut self, tag: &str) {
        self.insert(&tag.to_lowercase());
    }
    
    /// Insert an element's id
    pub fn insert_id(&mut self, id: &str) {
        self.insert(&format!("#{}", id));
    }
    
    /// Insert an element's class
    pub fn insert_class(&mut self, class: &str) {
        self.insert(&format!(".{}", class));
    }
    
    /// Check if a selector might match (may return false positives)
    pub fn might_match(&self, s: &str) -> bool {
        let (i1, i2) = Self::hash(s);
        self.get_bit(i1) && self.get_bit(i2)
    }
    
    /// Check if a tag might be in ancestors
    pub fn might_have_tag(&self, tag: &str) -> bool {
        self.might_match(&tag.to_lowercase())
    }
    
    /// Check if an id might be in ancestors
    pub fn might_have_id(&self, id: &str) -> bool {
        self.might_match(&format!("#{}", id))
    }
    
    /// Check if a class might be in ancestors
    pub fn might_have_class(&self, class: &str) -> bool {
        self.might_match(&format!(".{}", class))
    }
    
    /// Set a bit in the filter
    fn set_bit(&mut self, index: usize) {
        let word = index / 64;
        let bit = index % 64;
        self.bits[word] |= 1 << bit;
    }
    
    /// Get a bit from the filter
    fn get_bit(&self, index: usize) -> bool {
        let word = index / 64;
        let bit = index % 64;
        (self.bits[word] & (1 << bit)) != 0
    }
    
    /// Clear the filter
    pub fn clear(&mut self) {
        self.bits = [0; 4];
    }
    
    /// Create a copy with additional element
    pub fn with_element(&self, tag: &str, id: Option<&str>, classes: &[String]) -> Self {
        let mut filter = self.clone();
        filter.insert_tag(tag);
        if let Some(id) = id {
            filter.insert_id(id);
        }
        for class in classes {
            filter.insert_class(class);
        }
        filter
    }
}

// ============================================================================
// Forgiving Selector Parsing
// ============================================================================

/// Parse a forgiving selector list (for :is() and :where())
/// 
/// Unlike normal selector parsing, this silently ignores invalid selectors
/// rather than failing the entire parse. This matches CSS Selectors Level 4
/// behavior for :is() and :where() pseudo-classes.
pub fn parse_forgiving_selector_list(input: &str) -> Vec<SelectorComponent> {
    let mut results = Vec::new();
    
    for selector_str in input.split(',') {
        let selector_str = selector_str.trim();
        if selector_str.is_empty() {
            continue;
        }
        
        // Try to parse each selector, ignore failures
        if let Some(component) = parse_simple_selector(selector_str) {
            results.push(component);
        }
        // Invalid selectors are silently ignored (forgiving behavior)
    }
    
    results
}

/// Parse a simple selector (type, class, id, or attribute)
pub fn parse_simple_selector(input: &str) -> Option<SelectorComponent> {
    let input = input.trim();
    
    if input.is_empty() {
        return None;
    }
    
    // Universal selector
    if input == "*" {
        return Some(SelectorComponent::Universal);
    }
    
    // ID selector
    if let Some(id) = input.strip_prefix('#') {
        if !id.is_empty() && is_valid_ident(id) {
            return Some(SelectorComponent::Id(id.to_string()));
        }
        return None;
    }
    
    // Class selector
    if let Some(class) = input.strip_prefix('.') {
        if !class.is_empty() && is_valid_ident(class) {
            return Some(SelectorComponent::Class(class.to_string()));
        }
        return None;
    }
    
    // Attribute selector
    if input.starts_with('[') && input.ends_with(']') {
        return parse_attribute_selector(&input[1..input.len()-1]);
    }
    
    // Pseudo-class
    if let Some(pseudo) = input.strip_prefix(':') {
        if let Some(pseudo) = input.strip_prefix("::") {
            if let Some(pe) = PseudoElement::parse(pseudo) {
                return Some(SelectorComponent::PseudoElement(pe));
            }
        } else if let Some(pc) = parse_pseudo_class(pseudo) {
            return Some(SelectorComponent::PseudoClass(pc));
        }
        return None;
    }
    
    // Type selector (tag name)
    if is_valid_ident(input) {
        return Some(SelectorComponent::Type(input.to_lowercase()));
    }
    
    None
}

/// Check if string is a valid CSS identifier
fn is_valid_ident(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    
    let mut chars = s.chars();
    
    // First character must be a letter, underscore, or hyphen followed by non-digit
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' || c == '-' => {}
        _ => return false,
    }
    
    // Rest can be letters, digits, underscores, hyphens
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Parse an attribute selector content (without brackets)
fn parse_attribute_selector(content: &str) -> Option<SelectorComponent> {
    let content = content.trim();
    
    // Case-insensitive flag
    let (content, case_insensitive) = if content.ends_with(" i") || content.ends_with(" I") {
        (&content[..content.len()-2], true)
    } else {
        (content, false)
    };
    
    // Check for different matchers
    for (op, matcher_fn) in [
        ("~=", AttributeMatcher::Contains as fn(String) -> AttributeMatcher),
        ("|=", AttributeMatcher::DashMatch as fn(String) -> AttributeMatcher),
        ("^=", AttributeMatcher::Prefix as fn(String) -> AttributeMatcher),
        ("$=", AttributeMatcher::Suffix as fn(String) -> AttributeMatcher),
        ("*=", AttributeMatcher::Substring as fn(String) -> AttributeMatcher),
        ("=", AttributeMatcher::Exact as fn(String) -> AttributeMatcher),
    ] {
        if let Some(pos) = content.find(op) {
            let name = content[..pos].trim().to_string();
            let value = content[pos + op.len()..].trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_string();
            
            return Some(SelectorComponent::Attribute(AttributeSelector {
                name,
                matcher: Some(matcher_fn(value)),
                case_insensitive,
            }));
        }
    }
    
    // Just attribute presence [attr]
    Some(SelectorComponent::Attribute(AttributeSelector {
        name: content.to_string(),
        matcher: None,
        case_insensitive,
    }))
}

/// Parse a pseudo-class name
fn parse_pseudo_class(input: &str) -> Option<PseudoClass> {
    // Handle functional pseudo-classes
    if let Some(paren_pos) = input.find('(') {
        let name = &input[..paren_pos];
        let arg = input[paren_pos+1..].trim_end_matches(')');
        
        return match name {
            "nth-child" => NthExpression::parse(arg).map(PseudoClass::NthChild),
            "nth-last-child" => NthExpression::parse(arg).map(PseudoClass::NthLastChild),
            "nth-of-type" => NthExpression::parse(arg).map(PseudoClass::NthOfType),
            "nth-last-of-type" => NthExpression::parse(arg).map(PseudoClass::NthLastOfType),
            "not" => parse_simple_selector(arg).map(|s| PseudoClass::Not(Box::new(s))),
            "is" => Some(PseudoClass::Is(parse_forgiving_selector_list(arg))),
            "where" => Some(PseudoClass::Where(parse_forgiving_selector_list(arg))),
            "has" => Some(PseudoClass::Has(parse_forgiving_selector_list(arg))),
            "lang" => Some(PseudoClass::Lang(arg.to_string())),
            "dir" => match arg {
                "ltr" => Some(PseudoClass::Dir(Direction::Ltr)),
                "rtl" => Some(PseudoClass::Dir(Direction::Rtl)),
                _ => None,
            },
            _ => None,
        };
    }
    
    // Simple pseudo-classes
    match input {
        "hover" => Some(PseudoClass::Hover),
        "active" => Some(PseudoClass::Active),
        "focus" => Some(PseudoClass::Focus),
        "focus-visible" => Some(PseudoClass::FocusVisible),
        "focus-within" => Some(PseudoClass::FocusWithin),
        "link" => Some(PseudoClass::Link),
        "visited" => Some(PseudoClass::Visited),
        "enabled" => Some(PseudoClass::Enabled),
        "disabled" => Some(PseudoClass::Disabled),
        "checked" => Some(PseudoClass::Checked),
        "indeterminate" => Some(PseudoClass::Indeterminate),
        "required" => Some(PseudoClass::Required),
        "optional" => Some(PseudoClass::Optional),
        "valid" => Some(PseudoClass::Valid),
        "invalid" => Some(PseudoClass::Invalid),
        "read-only" => Some(PseudoClass::ReadOnly),
        "read-write" => Some(PseudoClass::ReadWrite),
        "placeholder-shown" => Some(PseudoClass::PlaceholderShown),
        "default" => Some(PseudoClass::Default),
        "root" => Some(PseudoClass::Root),
        "empty" => Some(PseudoClass::Empty),
        "first-child" => Some(PseudoClass::FirstChild),
        "last-child" => Some(PseudoClass::LastChild),
        "only-child" => Some(PseudoClass::OnlyChild),
        "first-of-type" => Some(PseudoClass::FirstOfType),
        "last-of-type" => Some(PseudoClass::LastOfType),
        "only-of-type" => Some(PseudoClass::OnlyOfType),
        "target" => Some(PseudoClass::Target),
        "scope" => Some(PseudoClass::Scope),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_nth_expression_odd() {
        let expr = NthExpression::odd();
        assert!(expr.matches(1));
        assert!(!expr.matches(2));
        assert!(expr.matches(3));
        assert!(!expr.matches(4));
        assert!(expr.matches(5));
    }
    
    #[test]
    fn test_nth_expression_even() {
        let expr = NthExpression::even();
        assert!(!expr.matches(1));
        assert!(expr.matches(2));
        assert!(!expr.matches(3));
        assert!(expr.matches(4));
    }
    
    #[test]
    fn test_nth_expression_parse() {
        assert_eq!(NthExpression::parse("odd"), Some(NthExpression::odd()));
        assert_eq!(NthExpression::parse("even"), Some(NthExpression::even()));
        assert_eq!(NthExpression::parse("3"), Some(NthExpression::index(3)));
        assert_eq!(NthExpression::parse("2n"), Some(NthExpression::new(2, 0)));
        assert_eq!(NthExpression::parse("2n+1"), Some(NthExpression::new(2, 1)));
        assert_eq!(NthExpression::parse("-n+3"), Some(NthExpression::new(-1, 3)));
    }
    
    #[test]
    fn test_attribute_selector_exact() {
        let sel = AttributeSelector {
            name: "type".to_string(),
            matcher: Some(AttributeMatcher::Exact("text".to_string())),
            case_insensitive: false,
        };
        
        assert!(sel.matches(Some("text")));
        assert!(!sel.matches(Some("TEXT")));
        assert!(!sel.matches(Some("password")));
        assert!(!sel.matches(None));
    }
    
    #[test]
    fn test_attribute_selector_prefix() {
        let sel = AttributeSelector {
            name: "class".to_string(),
            matcher: Some(AttributeMatcher::Prefix("btn-".to_string())),
            case_insensitive: false,
        };
        
        assert!(sel.matches(Some("btn-primary")));
        assert!(sel.matches(Some("btn-secondary")));
        assert!(!sel.matches(Some("button")));
    }
    
    #[test]
    fn test_pseudo_element_parse() {
        assert_eq!(PseudoElement::parse("::before"), Some(PseudoElement::Before));
        assert_eq!(PseudoElement::parse("after"), Some(PseudoElement::After));
        assert_eq!(PseudoElement::parse("::first-line"), Some(PseudoElement::FirstLine));
    }
}
