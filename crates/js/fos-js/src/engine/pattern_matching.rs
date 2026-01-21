//! Pattern Matching Implementation
//!
//! Implements TC39 Pattern Matching proposal:
//! - Literal patterns
//! - Binding patterns
//! - Array patterns
//! - Object patterns
//! - Guard expressions
//! - Or patterns
//! - Wildcard patterns

use std::collections::HashMap;

// =============================================================================
// Pattern Types
// =============================================================================

/// Pattern node in match expression
#[derive(Debug, Clone)]
pub enum Pattern {
    /// Wildcard: matches anything, binds nothing
    /// `case _:`
    Wildcard,
    
    /// Literal match
    /// `case 42:`, `case "hello":`, `case true:`
    Literal(PatternLiteral),
    
    /// Binding pattern: matches anything, binds to identifier
    /// `case let x:`, `case const y:`
    Binding {
        name: String,
        mutable: bool, // let vs const
    },
    
    /// Array pattern
    /// `case [a, b, c]:`, `case [first, ...rest]:`
    Array {
        elements: Vec<ArrayPatternElement>,
        rest: Option<String>, // ...rest binding
    },
    
    /// Object pattern
    /// `case { x, y }:`, `case { type: "point", ...rest }:`
    Object {
        properties: Vec<ObjectPatternProperty>,
        rest: Option<String>,
    },
    
    /// Constructor/class pattern
    /// `case Point { x, y }:`
    Constructor {
        constructor: String,
        pattern: Box<Pattern>,
    },
    
    /// Or pattern (alternatives)
    /// `case 1 | 2 | 3:`
    Or(Vec<Pattern>),
    
    /// And pattern (intersection)
    /// `case let x and Number:`
    And(Vec<Pattern>),
    
    /// Guard pattern (with condition)
    /// `case let x if x > 0:`
    Guard {
        pattern: Box<Pattern>,
        guard: GuardExpression,
    },
    
    /// Type pattern
    /// `case Number:`, `case String:`
    Type(TypePattern),
    
    /// Range pattern
    /// `case 1..10:`, `case 'a'..'z':`
    Range {
        start: PatternLiteral,
        end: PatternLiteral,
        inclusive: bool, // .. vs ..=
    },
    
    /// Pinned value (reference to variable)
    /// `case ^existingVar:`
    Pin(String),
    
    /// Regex pattern
    /// `case /\d+/:`
    Regex {
        pattern: String,
        flags: String,
        captures: Option<Vec<String>>,
    },
}

/// Literal pattern values
#[derive(Debug, Clone, PartialEq)]
pub enum PatternLiteral {
    Null,
    Undefined,
    Boolean(bool),
    Number(f64),
    BigInt(String), // Stored as string for precision
    String(String),
    Symbol(String),
}

/// Array pattern element
#[derive(Debug, Clone)]
pub enum ArrayPatternElement {
    /// Normal element pattern
    Pattern(Pattern),
    /// Hole (empty slot)
    Hole,
}

/// Object pattern property
#[derive(Debug, Clone)]
pub struct ObjectPatternProperty {
    /// Property key
    pub key: PropertyKey,
    /// Pattern to match value
    pub pattern: Pattern,
    /// Shorthand (key == binding name)
    pub shorthand: bool,
}

/// Property key
#[derive(Debug, Clone)]
pub enum PropertyKey {
    Identifier(String),
    String(String),
    Number(f64),
    Computed(Box<Pattern>), // [expr]: pattern
}

/// Type pattern
#[derive(Debug, Clone)]
pub enum TypePattern {
    Number,
    String,
    Boolean,
    BigInt,
    Symbol,
    Object,
    Function,
    Array,
    Null,
    Undefined,
    Custom(String), // instanceof check
}

/// Guard expression (simplified AST)
#[derive(Debug, Clone)]
pub enum GuardExpression {
    /// Simple comparison: x > 0
    Compare {
        left: GuardOperand,
        op: CompareOp,
        right: GuardOperand,
    },
    /// Logical and: a && b
    And(Box<GuardExpression>, Box<GuardExpression>),
    /// Logical or: a || b
    Or(Box<GuardExpression>, Box<GuardExpression>),
    /// Logical not: !a
    Not(Box<GuardExpression>),
    /// Function call: isValid(x)
    Call {
        function: String,
        args: Vec<GuardOperand>,
    },
    /// Property access: x.length
    Property {
        object: GuardOperand,
        property: String,
    },
    /// Binding reference
    Binding(String),
    /// Literal
    Literal(PatternLiteral),
}

/// Guard operand
#[derive(Debug, Clone)]
pub enum GuardOperand {
    Binding(String),
    Literal(PatternLiteral),
    Expression(Box<GuardExpression>),
}

/// Comparison operator
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp {
    Eq,       // ==
    StrictEq, // ===
    Ne,       // !=
    StrictNe, // !==
    Lt,       // <
    Le,       // <=
    Gt,       // >
    Ge,       // >=
    In,       // in
    InstanceOf,
}

// =============================================================================
// Match Expression
// =============================================================================

/// Match expression
#[derive(Debug, Clone)]
pub struct MatchExpression {
    /// Subject expression to match
    pub subject: u32, // AST node ID
    /// Match arms
    pub arms: Vec<MatchArm>,
    /// Is exhaustive
    pub exhaustive: bool,
}

/// Match arm
#[derive(Debug, Clone)]
pub struct MatchArm {
    /// Pattern to match
    pub pattern: Pattern,
    /// Guard expression (optional)
    pub guard: Option<GuardExpression>,
    /// Body expression
    pub body: u32, // AST node ID
}

// =============================================================================
// Pattern Compiler
// =============================================================================

/// Compiled pattern for efficient matching
#[derive(Debug)]
pub struct CompiledPattern {
    /// Match instructions
    pub instructions: Vec<MatchInstruction>,
    /// Bindings to create
    pub bindings: Vec<String>,
    /// Jump targets for failure
    pub fail_targets: Vec<usize>,
}

/// Pattern matching instruction
#[derive(Debug, Clone)]
pub enum MatchInstruction {
    /// Check if subject is null
    CheckNull,
    /// Check if subject is undefined
    CheckUndefined,
    /// Check if subject equals literal
    CheckLiteral(PatternLiteral),
    /// Check type of subject
    CheckType(TypePattern),
    /// Check instanceof
    CheckInstanceOf(String),
    /// Check if array
    CheckArray,
    /// Check array length
    CheckArrayLength { min: usize, exact: bool },
    /// Check if object
    CheckObject,
    /// Check property exists
    CheckProperty(String),
    /// Get array element (pushes to stack)
    GetElement(usize),
    /// Get property (pushes to stack)
    GetProperty(String),
    /// Get rest elements (from index)
    GetRestElements(usize),
    /// Get rest properties
    GetRestProperties(Vec<String>), // Excluded keys
    /// Bind current value to name
    Bind(String),
    /// Push subject to stack (save for later)
    PushSubject,
    /// Pop subject from stack
    PopSubject,
    /// Jump if match fails
    JumpIfFail(usize),
    /// Jump unconditionally
    Jump(usize),
    /// Evaluate guard
    EvalGuard(GuardExpression),
    /// Match succeeded
    Success,
    /// Match failed
    Fail,
    /// Check range
    CheckRange {
        start: PatternLiteral,
        end: PatternLiteral,
        inclusive: bool,
    },
    /// Regex test
    RegexTest { pattern: String, flags: String },
    /// Fork for or-pattern (try multiple patterns)
    Fork(Vec<usize>), // Targets to try
}

/// Pattern compiler
#[derive(Debug, Default)]
pub struct PatternCompiler {
    /// Current instruction list
    instructions: Vec<MatchInstruction>,
    /// Bindings collected
    bindings: Vec<String>,
    /// Label counter
    label_counter: usize,
}

impl PatternCompiler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Compile pattern to instructions
    pub fn compile(&mut self, pattern: &Pattern) -> CompiledPattern {
        self.instructions.clear();
        self.bindings.clear();
        
        self.compile_pattern(pattern);
        self.instructions.push(MatchInstruction::Success);
        
        CompiledPattern {
            instructions: self.instructions.clone(),
            bindings: self.bindings.clone(),
            fail_targets: Vec::new(),
        }
    }

    fn compile_pattern(&mut self, pattern: &Pattern) {
        match pattern {
            Pattern::Wildcard => {
                // Matches anything, do nothing
            }
            
            Pattern::Literal(lit) => {
                self.instructions.push(MatchInstruction::CheckLiteral(lit.clone()));
            }
            
            Pattern::Binding { name, .. } => {
                self.bindings.push(name.clone());
                self.instructions.push(MatchInstruction::Bind(name.clone()));
            }
            
            Pattern::Array { elements, rest } => {
                self.instructions.push(MatchInstruction::CheckArray);
                
                let min_len = elements.iter()
                    .filter(|e| !matches!(e, ArrayPatternElement::Hole))
                    .count();
                let exact = rest.is_none();
                
                self.instructions.push(MatchInstruction::CheckArrayLength { 
                    min: min_len, 
                    exact 
                });
                
                for (i, element) in elements.iter().enumerate() {
                    if let ArrayPatternElement::Pattern(p) = element {
                        self.instructions.push(MatchInstruction::PushSubject);
                        self.instructions.push(MatchInstruction::GetElement(i));
                        self.compile_pattern(p);
                        self.instructions.push(MatchInstruction::PopSubject);
                    }
                }
                
                if let Some(rest_name) = rest {
                    self.bindings.push(rest_name.clone());
                    self.instructions.push(MatchInstruction::GetRestElements(elements.len()));
                    self.instructions.push(MatchInstruction::Bind(rest_name.clone()));
                }
            }
            
            Pattern::Object { properties, rest } => {
                self.instructions.push(MatchInstruction::CheckObject);
                
                let mut keys = Vec::new();
                
                for prop in properties {
                    let key = match &prop.key {
                        PropertyKey::Identifier(s) | PropertyKey::String(s) => s.clone(),
                        PropertyKey::Number(n) => n.to_string(),
                        PropertyKey::Computed(_) => continue, // Handle separately
                    };
                    
                    keys.push(key.clone());
                    self.instructions.push(MatchInstruction::CheckProperty(key.clone()));
                    self.instructions.push(MatchInstruction::PushSubject);
                    self.instructions.push(MatchInstruction::GetProperty(key));
                    self.compile_pattern(&prop.pattern);
                    self.instructions.push(MatchInstruction::PopSubject);
                }
                
                if let Some(rest_name) = rest {
                    self.bindings.push(rest_name.clone());
                    self.instructions.push(MatchInstruction::GetRestProperties(keys));
                    self.instructions.push(MatchInstruction::Bind(rest_name.clone()));
                }
            }
            
            Pattern::Or(patterns) => {
                let targets: Vec<_> = (0..patterns.len())
                    .map(|_| self.new_label())
                    .collect();
                
                self.instructions.push(MatchInstruction::Fork(targets.clone()));
                
                for (i, p) in patterns.iter().enumerate() {
                    // Each branch starts at its target label
                    self.compile_pattern(p);
                    if i < patterns.len() - 1 {
                        // Jump to success after match
                        self.instructions.push(MatchInstruction::Jump(0)); // Placeholder
                    }
                }
            }
            
            Pattern::And(patterns) => {
                for p in patterns {
                    self.compile_pattern(p);
                }
            }
            
            Pattern::Guard { pattern, guard } => {
                self.compile_pattern(pattern);
                self.instructions.push(MatchInstruction::EvalGuard(guard.clone()));
            }
            
            Pattern::Type(ty) => {
                self.instructions.push(MatchInstruction::CheckType(ty.clone()));
            }
            
            Pattern::Range { start, end, inclusive } => {
                self.instructions.push(MatchInstruction::CheckRange {
                    start: start.clone(),
                    end: end.clone(),
                    inclusive: *inclusive,
                });
            }
            
            Pattern::Constructor { constructor, pattern } => {
                self.instructions.push(MatchInstruction::CheckInstanceOf(constructor.clone()));
                self.compile_pattern(pattern);
            }
            
            Pattern::Pin(name) => {
                // Would need to look up variable value at runtime
                self.instructions.push(MatchInstruction::CheckLiteral(
                    PatternLiteral::String(format!("__pin_{}", name))
                ));
            }
            
            Pattern::Regex { pattern, flags, captures: _ } => {
                self.instructions.push(MatchInstruction::RegexTest {
                    pattern: pattern.clone(),
                    flags: flags.clone(),
                });
            }
        }
    }

    fn new_label(&mut self) -> usize {
        let label = self.label_counter;
        self.label_counter += 1;
        label
    }
}

// =============================================================================
// Pattern Matcher Runtime
// =============================================================================

/// Pattern match runtime result
#[derive(Debug)]
pub struct MatchResult {
    /// Whether pattern matched
    pub matched: bool,
    /// Bindings created
    pub bindings: HashMap<String, u32>, // Name -> value ID
}

impl MatchResult {
    pub fn success(bindings: HashMap<String, u32>) -> Self {
        Self { matched: true, bindings }
    }

    pub fn failure() -> Self {
        Self { matched: false, bindings: HashMap::new() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wildcard_pattern() {
        let mut compiler = PatternCompiler::new();
        let compiled = compiler.compile(&Pattern::Wildcard);
        
        assert_eq!(compiled.instructions.len(), 1); // Just Success
        assert!(compiled.bindings.is_empty());
    }

    #[test]
    fn test_literal_pattern() {
        let mut compiler = PatternCompiler::new();
        let pattern = Pattern::Literal(PatternLiteral::Number(42.0));
        let compiled = compiler.compile(&pattern);
        
        assert!(matches!(
            compiled.instructions[0],
            MatchInstruction::CheckLiteral(PatternLiteral::Number(n)) if n == 42.0
        ));
    }

    #[test]
    fn test_binding_pattern() {
        let mut compiler = PatternCompiler::new();
        let pattern = Pattern::Binding {
            name: "x".to_string(),
            mutable: false,
        };
        let compiled = compiler.compile(&pattern);
        
        assert_eq!(compiled.bindings, vec!["x".to_string()]);
        assert!(matches!(
            compiled.instructions[0],
            MatchInstruction::Bind(ref s) if s == "x"
        ));
    }

    #[test]
    fn test_array_pattern() {
        let mut compiler = PatternCompiler::new();
        let pattern = Pattern::Array {
            elements: vec![
                ArrayPatternElement::Pattern(Pattern::Binding {
                    name: "a".to_string(),
                    mutable: false,
                }),
                ArrayPatternElement::Pattern(Pattern::Binding {
                    name: "b".to_string(),
                    mutable: false,
                }),
            ],
            rest: None,
        };
        let compiled = compiler.compile(&pattern);
        
        assert!(matches!(compiled.instructions[0], MatchInstruction::CheckArray));
        assert_eq!(compiled.bindings, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn test_object_pattern() {
        let mut compiler = PatternCompiler::new();
        let pattern = Pattern::Object {
            properties: vec![
                ObjectPatternProperty {
                    key: PropertyKey::Identifier("x".to_string()),
                    pattern: Pattern::Binding {
                        name: "x".to_string(),
                        mutable: false,
                    },
                    shorthand: true,
                },
            ],
            rest: None,
        };
        let compiled = compiler.compile(&pattern);
        
        assert!(matches!(compiled.instructions[0], MatchInstruction::CheckObject));
    }

    #[test]
    fn test_guard_pattern() {
        let mut compiler = PatternCompiler::new();
        let pattern = Pattern::Guard {
            pattern: Box::new(Pattern::Binding {
                name: "x".to_string(),
                mutable: false,
            }),
            guard: GuardExpression::Compare {
                left: GuardOperand::Binding("x".to_string()),
                op: CompareOp::Gt,
                right: GuardOperand::Literal(PatternLiteral::Number(0.0)),
            },
        };
        let compiled = compiler.compile(&pattern);
        
        assert!(compiled.instructions.iter().any(|i| matches!(i, MatchInstruction::EvalGuard(_))));
    }

    #[test]
    fn test_or_pattern() {
        let mut compiler = PatternCompiler::new();
        let pattern = Pattern::Or(vec![
            Pattern::Literal(PatternLiteral::Number(1.0)),
            Pattern::Literal(PatternLiteral::Number(2.0)),
            Pattern::Literal(PatternLiteral::Number(3.0)),
        ]);
        let compiled = compiler.compile(&pattern);
        
        assert!(compiled.instructions.iter().any(|i| matches!(i, MatchInstruction::Fork(_))));
    }
}
