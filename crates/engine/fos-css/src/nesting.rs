//! CSS Nesting
//!
//! Implementation of CSS Nesting specification.
//! Allows nested rule blocks with & selector referencing parent.

use std::collections::HashMap;

// ============================================================================
// Nested Rule Types
// ============================================================================

/// A CSS rule that may contain nested rules
#[derive(Debug, Clone)]
pub struct NestedRule {
    /// Selector for this rule (may contain &)
    pub selector: NestableSelector,
    /// Declarations in this rule
    pub declarations: Vec<Declaration>,
    /// Nested rules
    pub nested_rules: Vec<NestedRule>,
    /// Source location
    pub source_line: u32,
}

/// Property declaration
#[derive(Debug, Clone)]
pub struct Declaration {
    pub property: Box<str>,
    pub value: Box<str>,
    pub important: bool,
}

/// Selector that may contain & nesting references
#[derive(Debug, Clone)]
pub struct NestableSelector {
    /// Original selector text
    pub text: Box<str>,
    /// Parsed parts
    pub parts: Vec<SelectorPart>,
    /// Does this contain &?
    pub has_nesting_selector: bool,
}

/// Part of a nestable selector
#[derive(Debug, Clone)]
pub enum SelectorPart {
    /// Literal text (tag, class, id, etc.)
    Literal(Box<str>),
    /// & - nesting selector
    Nesting,
    /// Combinator (space, >, +, ~)
    Combinator(char),
}

// ============================================================================
// Nesting Parser
// ============================================================================

/// Parse a CSS block that may contain nesting
pub fn parse_nested_block(input: &str) -> Vec<NestedRule> {
    let mut parser = NestingParser::new(input);
    parser.parse_rules()
}

/// Parser for nested CSS
struct NestingParser<'a> {
    input: &'a str,
    pos: usize,
    line: u32,
}

impl<'a> NestingParser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            pos: 0,
            line: 1,
        }
    }
    
    fn parse_rules(&mut self) -> Vec<NestedRule> {
        let mut rules = Vec::new();
        
        while !self.is_eof() {
            self.skip_whitespace_and_comments();
            
            if self.is_eof() {
                break;
            }
            
            if let Some(rule) = self.parse_rule() {
                rules.push(rule);
            }
        }
        
        rules
    }
    
    fn parse_rule(&mut self) -> Option<NestedRule> {
        self.skip_whitespace_and_comments();
        
        // Parse selector
        let selector = self.parse_selector()?;
        
        self.skip_whitespace_and_comments();
        
        // Expect {
        if self.current() != Some('{') {
            return None;
        }
        self.advance();
        
        // Parse declarations and nested rules
        let (declarations, nested_rules) = self.parse_block_contents();
        
        // Expect }
        if self.current() == Some('}') {
            self.advance();
        }
        
        Some(NestedRule {
            selector,
            declarations,
            nested_rules,
            source_line: self.line,
        })
    }
    
    fn parse_selector(&mut self) -> Option<NestableSelector> {
        let start = self.pos;
        let mut parts = Vec::new();
        let mut has_nesting = false;
        let mut current_literal = String::new();
        
        while !self.is_eof() {
            let c = self.current()?;
            
            if c == '{' || c == '}' {
                break;
            }
            
            if c == '&' {
                // Flush current literal
                if !current_literal.is_empty() {
                    parts.push(SelectorPart::Literal(current_literal.clone().into()));
                    current_literal.clear();
                }
                parts.push(SelectorPart::Nesting);
                has_nesting = true;
                self.advance();
            } else if c == ' ' || c == '>' || c == '+' || c == '~' {
                // Flush current literal
                if !current_literal.is_empty() {
                    parts.push(SelectorPart::Literal(current_literal.clone().into()));
                    current_literal.clear();
                }
                
                // Skip whitespace
                self.skip_whitespace();
                
                // Check for explicit combinator
                match self.current() {
                    Some('>') | Some('+') | Some('~') => {
                        let comb = self.current().unwrap();
                        parts.push(SelectorPart::Combinator(comb));
                        self.advance();
                        self.skip_whitespace();
                    }
                    Some('{') | Some('}') => break,
                    _ if c != ' ' => {
                        parts.push(SelectorPart::Combinator(c));
                        self.advance();
                        self.skip_whitespace();
                    }
                    _ => {
                        // Descendant combinator (space)
                        if !parts.is_empty() {
                            parts.push(SelectorPart::Combinator(' '));
                        }
                    }
                }
            } else {
                current_literal.push(c);
                self.advance();
            }
        }
        
        // Flush remaining literal
        if !current_literal.is_empty() {
            parts.push(SelectorPart::Literal(current_literal.into()));
        }
        
        if parts.is_empty() {
            return None;
        }
        
        let text = self.input[start..self.pos].trim().into();
        
        Some(NestableSelector {
            text,
            parts,
            has_nesting_selector: has_nesting,
        })
    }
    
    fn parse_block_contents(&mut self) -> (Vec<Declaration>, Vec<NestedRule>) {
        let mut declarations = Vec::new();
        let mut nested = Vec::new();
        
        loop {
            self.skip_whitespace_and_comments();
            
            if self.is_eof() || self.current() == Some('}') {
                break;
            }
            
            // Determine if this is a declaration or nested rule
            let start = self.pos;
            let is_nested = self.looks_like_nested_rule();
            self.pos = start; // Reset position
            
            if is_nested {
                if let Some(rule) = self.parse_rule() {
                    nested.push(rule);
                }
            } else {
                if let Some(decl) = self.parse_declaration() {
                    declarations.push(decl);
                }
            }
        }
        
        (declarations, nested)
    }
    
    fn looks_like_nested_rule(&mut self) -> bool {
        // Scan ahead to determine if this is a nested rule
        // Nested rules contain { before ; or end of input
        let start = self.pos;
        
        while !self.is_eof() {
            match self.current() {
                Some('{') => {
                    self.pos = start;
                    return true;
                }
                Some(';') | Some('}') => {
                    self.pos = start;
                    return false;
                }
                _ => self.advance(),
            }
        }
        
        self.pos = start;
        false
    }
    
    fn parse_declaration(&mut self) -> Option<Declaration> {
        self.skip_whitespace_and_comments();
        
        // Property name
        let start = self.pos;
        while let Some(c) = self.current() {
            if c == ':' || c == ';' || c == '}' {
                break;
            }
            self.advance();
        }
        
        let property: Box<str> = self.input[start..self.pos].trim().into();
        
        if property.is_empty() {
            return None;
        }
        
        // Expect :
        if self.current() != Some(':') {
            // Not a valid declaration, skip to next ; or }
            while !self.is_eof() && self.current() != Some(';') && self.current() != Some('}') {
                self.advance();
            }
            if self.current() == Some(';') {
                self.advance();
            }
            return None;
        }
        self.advance();
        
        // Value
        let start = self.pos;
        let mut important = false;
        
        while let Some(c) = self.current() {
            if c == ';' || c == '}' {
                break;
            }
            self.advance();
        }
        
        let mut value: Box<str> = self.input[start..self.pos].trim().into();
        
        // Check for !important
        if value.ends_with("!important") {
            important = true;
            value = value[..value.len() - 10].trim().into();
        }
        
        // Skip ;
        if self.current() == Some(';') {
            self.advance();
        }
        
        Some(Declaration {
            property,
            value,
            important,
        })
    }
    
    fn current(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }
    
    fn advance(&mut self) {
        if let Some(c) = self.current() {
            if c == '\n' {
                self.line += 1;
            }
            self.pos += c.len_utf8();
        }
    }
    
    fn is_eof(&self) -> bool {
        self.pos >= self.input.len()
    }
    
    fn skip_whitespace(&mut self) {
        while let Some(c) = self.current() {
            if c.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }
    
    fn skip_whitespace_and_comments(&mut self) {
        loop {
            self.skip_whitespace();
            
            if self.input[self.pos..].starts_with("/*") {
                self.skip_comment();
            } else {
                break;
            }
        }
    }
    
    fn skip_comment(&mut self) {
        if self.input[self.pos..].starts_with("/*") {
            self.pos += 2;
            while !self.is_eof() && !self.input[self.pos..].starts_with("*/") {
                self.advance();
            }
            if self.input[self.pos..].starts_with("*/") {
                self.pos += 2;
            }
        }
    }
}

// ============================================================================
// Selector Resolution
// ============================================================================

/// Resolve nested selectors to flat selectors
pub fn resolve_nested_selectors(
    rules: &[NestedRule],
    parent_selector: Option<&str>,
) -> Vec<FlatRule> {
    let mut flat_rules = Vec::new();
    
    for rule in rules {
        resolve_rule(&mut flat_rules, rule, parent_selector);
    }
    
    flat_rules
}

/// Flat (non-nested) rule
#[derive(Debug, Clone)]
pub struct FlatRule {
    /// Fully resolved selector
    pub selector: Box<str>,
    /// Declarations
    pub declarations: Vec<Declaration>,
}

fn resolve_rule(
    output: &mut Vec<FlatRule>,
    rule: &NestedRule,
    parent_selector: Option<&str>,
) {
    // Resolve the selector
    let resolved_selector = resolve_selector(&rule.selector, parent_selector);
    
    // Add this rule if it has declarations
    if !rule.declarations.is_empty() {
        output.push(FlatRule {
            selector: resolved_selector.clone().into(),
            declarations: rule.declarations.clone(),
        });
    }
    
    // Recursively resolve nested rules
    for nested in &rule.nested_rules {
        resolve_rule(output, nested, Some(&resolved_selector));
    }
}

fn resolve_selector(selector: &NestableSelector, parent: Option<&str>) -> String {
    if !selector.has_nesting_selector {
        // No & reference - prepend parent with descendant combinator
        match parent {
            Some(p) => format!("{} {}", p, selector.text),
            None => selector.text.to_string(),
        }
    } else {
        // Replace & with parent
        let parent = parent.unwrap_or("");
        let mut result = String::new();
        
        for part in &selector.parts {
            match part {
                SelectorPart::Literal(s) => result.push_str(s),
                SelectorPart::Nesting => result.push_str(parent),
                SelectorPart::Combinator(c) => {
                    if *c != ' ' {
                        result.push(' ');
                    }
                    result.push(*c);
                    if *c != ' ' {
                        result.push(' ');
                    }
                }
            }
        }
        
        result.trim().to_string()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_simple_nested() {
        let css = ".parent {
            color: red;
            
            .child {
                color: blue;
            }
        }";
        
        let rules = parse_nested_block(css);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].declarations.len(), 1);
        assert_eq!(rules[0].nested_rules.len(), 1);
    }
    
    #[test]
    fn test_resolve_nested() {
        let css = ".parent {
            color: red;
            
            .child {
                color: blue;
            }
        }";
        
        let rules = parse_nested_block(css);
        let flat = resolve_nested_selectors(&rules, None);
        
        assert_eq!(flat.len(), 2);
        assert_eq!(flat[0].selector.as_ref(), ".parent");
        assert_eq!(flat[1].selector.as_ref(), ".parent .child");
    }
    
    #[test]
    fn test_nesting_selector() {
        let css = ".btn {
            color: blue;
            
            &:hover {
                color: red;
            }
            
            &.active {
                color: green;
            }
        }";
        
        let rules = parse_nested_block(css);
        let flat = resolve_nested_selectors(&rules, None);
        
        assert_eq!(flat.len(), 3);
        assert_eq!(flat[0].selector.as_ref(), ".btn");
        assert_eq!(flat[1].selector.as_ref(), ".btn:hover");
        assert_eq!(flat[2].selector.as_ref(), ".btn.active");
    }
    
    #[test]
    fn test_deep_nesting() {
        let css = ".a {
            .b {
                .c {
                    color: red;
                }
            }
        }";
        
        let rules = parse_nested_block(css);
        let flat = resolve_nested_selectors(&rules, None);
        
        assert_eq!(flat.len(), 1);
        assert_eq!(flat[0].selector.as_ref(), ".a .b .c");
    }
    
    #[test]
    fn test_multiple_declarations() {
        let css = ".test {
            color: red;
            background: blue;
            margin: 10px;
        }";
        
        let rules = parse_nested_block(css);
        assert_eq!(rules[0].declarations.len(), 3);
    }
    
    #[test]
    fn test_important() {
        let css = ".test {
            color: red !important;
        }";
        
        let rules = parse_nested_block(css);
        assert!(rules[0].declarations[0].important);
        assert_eq!(rules[0].declarations[0].value.as_ref(), "red");
    }
}
