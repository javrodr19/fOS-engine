//! CSS Custom Properties (Variables) Module
//!
//! Implements CSS Custom Properties (--var) and the var() function.

use std::collections::HashMap;

/// A CSS custom property value
#[derive(Debug, Clone, PartialEq)]
pub enum CustomPropertyValue {
    /// Raw token string (unparsed)
    Tokens(String),
    /// Resolved value
    Resolved(ResolvedValue),
    /// Invalid/unset
    Invalid,
}

/// A resolved CSS value
#[derive(Debug, Clone, PartialEq)]
pub enum ResolvedValue {
    /// Length in pixels
    Length(f32),
    /// Percentage
    Percentage(f32),
    /// Number
    Number(f32),
    /// Color (r, g, b, a)
    Color(u8, u8, u8, u8),
    /// String value
    String(String),
}

impl ResolvedValue {
    /// Convert to f32 if numeric
    pub fn as_f32(&self) -> Option<f32> {
        match self {
            Self::Length(v) | Self::Percentage(v) | Self::Number(v) => Some(*v),
            _ => None,
        }
    }
}

/// CSS Variable scope (cascading)
#[derive(Debug, Clone, Default)]
pub struct VariableScope {
    /// Variable definitions (--name -> value)
    variables: HashMap<String, CustomPropertyValue>,
    /// Parent scope for cascading
    parent: Option<Box<VariableScope>>,
}

impl VariableScope {
    /// Create a new empty scope
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Create a child scope
    pub fn child(&self) -> Self {
        Self {
            variables: HashMap::new(),
            parent: Some(Box::new(self.clone())),
        }
    }
    
    /// Set a custom property
    pub fn set(&mut self, name: impl Into<String>, value: impl Into<String>) {
        let name = name.into();
        let value = value.into();
        self.variables.insert(name, CustomPropertyValue::Tokens(value));
    }
    
    /// Set a resolved value
    pub fn set_resolved(&mut self, name: impl Into<String>, value: ResolvedValue) {
        self.variables.insert(name.into(), CustomPropertyValue::Resolved(value));
    }
    
    /// Get a custom property (searches parent scopes)
    pub fn get(&self, name: &str) -> Option<&CustomPropertyValue> {
        self.variables.get(name).or_else(|| {
            self.parent.as_ref().and_then(|p| p.get(name))
        })
    }
    
    /// Resolve var() references in a value string
    pub fn resolve_var(&self, value: &str) -> String {
        let mut result = value.to_string();
        
        // Simple regex-free var() resolution
        while let Some(start) = result.find("var(") {
            if let Some(relative_end) = result[start..].find(')') {
                let end = start + relative_end + 1;
                let var_expr = &result[start..end];
                let resolved = self.resolve_var_expression(var_expr);
                result = format!("{}{}{}", &result[..start], resolved, &result[end..]);
            } else {
                break; // Malformed var()
            }
        }
        
        result
    }
    
    /// Resolve a single var(--name) or var(--name, fallback) expression
    fn resolve_var_expression(&self, expr: &str) -> String {
        // Parse "var(--name)" or "var(--name, fallback)"
        let inner = expr.trim_start_matches("var(").trim_end_matches(')');
        let parts: Vec<&str> = inner.splitn(2, ',').collect();
        
        let var_name = parts[0].trim();
        let fallback = parts.get(1).map(|s| s.trim());
        
        if let Some(value) = self.get(var_name) {
            match value {
                CustomPropertyValue::Tokens(s) => self.resolve_var(s),
                CustomPropertyValue::Resolved(r) => format!("{:?}", r),
                CustomPropertyValue::Invalid => fallback.unwrap_or("").to_string(),
            }
        } else {
            fallback.unwrap_or("").to_string()
        }
    }
}

/// CSS calc() expression evaluator
#[derive(Debug, Clone)]
pub struct CalcExpression {
    tokens: Vec<CalcToken>,
}

#[derive(Debug, Clone, PartialEq)]
enum CalcToken {
    Number(f32),
    Length(f32),      // px
    Percentage(f32),  // %
    Add,
    Sub,
    Mul,
    Div,
    LParen,
    RParen,
}

impl CalcExpression {
    /// Parse a calc() expression
    pub fn parse(expr: &str) -> Option<Self> {
        let inner = expr.trim()
            .strip_prefix("calc(")?
            .strip_suffix(')')?;
        
        let tokens = Self::tokenize(inner)?;
        Some(Self { tokens })
    }
    
    /// Tokenize calc expression
    fn tokenize(expr: &str) -> Option<Vec<CalcToken>> {
        let mut tokens = Vec::new();
        let mut chars = expr.chars().peekable();
        
        while let Some(&c) = chars.peek() {
            match c {
                ' ' | '\t' | '\n' => { chars.next(); }
                '+' => { chars.next(); tokens.push(CalcToken::Add); }
                '-' => { 
                    chars.next();
                    // Check if it's a negative number or subtraction
                    if let Some(&next) = chars.peek() {
                        if next.is_ascii_digit() || next == '.' {
                            let num = Self::read_number(&mut chars, true)?;
                            tokens.push(num);
                            continue;
                        }
                    }
                    tokens.push(CalcToken::Sub);
                }
                '*' => { chars.next(); tokens.push(CalcToken::Mul); }
                '/' => { chars.next(); tokens.push(CalcToken::Div); }
                '(' => { chars.next(); tokens.push(CalcToken::LParen); }
                ')' => { chars.next(); tokens.push(CalcToken::RParen); }
                '0'..='9' | '.' => {
                    let num = Self::read_number(&mut chars, false)?;
                    tokens.push(num);
                }
                _ => { chars.next(); } // Skip unknown
            }
        }
        
        Some(tokens)
    }
    
    /// Read a number with optional unit
    fn read_number(chars: &mut std::iter::Peekable<std::str::Chars>, negative: bool) -> Option<CalcToken> {
        let mut num_str = String::new();
        
        while let Some(&c) = chars.peek() {
            if c.is_ascii_digit() || c == '.' {
                num_str.push(c);
                chars.next();
            } else {
                break;
            }
        }
        
        let mut value: f32 = num_str.parse().ok()?;
        if negative {
            value = -value;
        }
        
        // Check for unit
        let mut unit = String::new();
        while let Some(&c) = chars.peek() {
            if c.is_ascii_alphabetic() || c == '%' {
                unit.push(c);
                chars.next();
            } else {
                break;
            }
        }
        
        Some(match unit.as_str() {
            "px" | "PX" => CalcToken::Length(value),
            "%" => CalcToken::Percentage(value),
            "em" => CalcToken::Length(value * 16.0), // Assume 16px base
            "rem" => CalcToken::Length(value * 16.0),
            _ => CalcToken::Number(value),
        })
    }
    
    /// Evaluate the expression
    pub fn evaluate(&self, percentage_base: f32) -> Option<f32> {
        let mut values = self.tokens.clone();
        
        // Convert percentages to absolute values
        for token in &mut values {
            if let CalcToken::Percentage(p) = token {
                *token = CalcToken::Number(*p / 100.0 * percentage_base);
            }
            if let CalcToken::Length(l) = token {
                *token = CalcToken::Number(*l);
            }
        }
        
        // Simple evaluation (no parentheses support for now)
        self.eval_tokens(&values)
    }
    
    /// Evaluate tokens with proper operator precedence
    fn eval_tokens(&self, tokens: &[CalcToken]) -> Option<f32> {
        if tokens.is_empty() {
            return None;
        }
        
        // First pass: handle * and /
        let mut result: Vec<CalcToken> = Vec::new();
        let mut i = 0;
        
        while i < tokens.len() {
            match &tokens[i] {
                CalcToken::Mul | CalcToken::Div => {
                    let left = match result.pop()? {
                        CalcToken::Number(n) => n,
                        _ => return None,
                    };
                    let right = match tokens.get(i + 1)? {
                        CalcToken::Number(n) => *n,
                        _ => return None,
                    };
                    let val = if matches!(tokens[i], CalcToken::Mul) {
                        left * right
                    } else {
                        if right == 0.0 { return None; }
                        left / right
                    };
                    result.push(CalcToken::Number(val));
                    i += 2;
                }
                token => {
                    result.push(token.clone());
                    i += 1;
                }
            }
        }
        
        // Second pass: handle + and -
        let mut final_result = 0.0;
        let mut current_op = CalcToken::Add;
        
        for token in result {
            match token {
                CalcToken::Number(n) => {
                    match current_op {
                        CalcToken::Add => final_result += n,
                        CalcToken::Sub => final_result -= n,
                        _ => {}
                    }
                }
                CalcToken::Add | CalcToken::Sub => {
                    current_op = token;
                }
                _ => {}
            }
        }
        
        Some(final_result)
    }
}

/// min(), max(), clamp() functions
pub fn css_min(values: &[f32]) -> f32 {
    values.iter().copied().fold(f32::INFINITY, f32::min)
}

pub fn css_max(values: &[f32]) -> f32 {
    values.iter().copied().fold(f32::NEG_INFINITY, f32::max)
}

pub fn css_clamp(min: f32, preferred: f32, max: f32) -> f32 {
    preferred.max(min).min(max)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_variable_scope() {
        let mut scope = VariableScope::new();
        scope.set("--primary", "#ff0000");
        
        assert!(matches!(scope.get("--primary"), Some(CustomPropertyValue::Tokens(s)) if s == "#ff0000"));
    }
    
    #[test]
    fn test_variable_inheritance() {
        let mut parent = VariableScope::new();
        parent.set("--color", "blue");
        
        let child = parent.child();
        assert!(child.get("--color").is_some());
    }
    
    #[test]
    fn test_var_resolution() {
        let mut scope = VariableScope::new();
        scope.set("--size", "16px");
        
        let result = scope.resolve_var("font-size: var(--size)");
        assert_eq!(result, "font-size: 16px");
    }
    
    #[test]
    fn test_var_fallback() {
        let scope = VariableScope::new();
        let result = scope.resolve_var("color: var(--undefined, red)");
        assert_eq!(result, "color: red");
    }
    
    #[test]
    fn test_calc_simple() {
        let expr = CalcExpression::parse("calc(100px + 50px)").unwrap();
        let result = expr.evaluate(0.0);
        assert_eq!(result, Some(150.0));
    }
    
    #[test]
    fn test_calc_multiply() {
        let expr = CalcExpression::parse("calc(10 * 5)").unwrap();
        let result = expr.evaluate(0.0);
        assert_eq!(result, Some(50.0));
    }
    
    #[test]
    fn test_calc_percentage() {
        let expr = CalcExpression::parse("calc(50% + 20px)").unwrap();
        let result = expr.evaluate(200.0); // 50% of 200 = 100
        assert_eq!(result, Some(120.0));
    }
    
    #[test]
    fn test_clamp() {
        assert_eq!(css_clamp(10.0, 5.0, 20.0), 10.0);
        assert_eq!(css_clamp(10.0, 15.0, 20.0), 15.0);
        assert_eq!(css_clamp(10.0, 25.0, 20.0), 20.0);
    }
}
