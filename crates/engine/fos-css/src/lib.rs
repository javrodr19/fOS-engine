//! fOS CSS Parser & Style System
//!
//! CSS parsing using lightningcss with style cascade implementation.
//! Designed for memory efficiency with computed style sharing.
//! Includes CSS Custom Properties (variables), calc(), and math functions.

mod parser;
mod cascade;
pub mod properties;
pub mod computed;
pub mod variables;
pub mod selectors;
pub mod style_cache;
pub mod container;
pub mod mask;
pub mod web_animations;
pub mod rule_tree;
pub mod inheritance;
pub mod selector_opt;
pub mod transitions;

pub use parser::CssParser;
pub use cascade::StyleResolver;
pub use properties::{PropertyId, PropertyValue};
pub use computed::ComputedStyle;
pub use computed::PropertyMask;
pub use variables::{
    VariableScope, CustomPropertyValue, ResolvedValue,
    CalcExpression, css_min, css_max, css_clamp,
    CssVarInterner, InternedVarName,
    Fixed16 as CalcFixed16, css_min_fixed, css_max_fixed, css_clamp_fixed,
    CalcExpressionFixed,
};
pub use selectors::{
    PseudoElement, PseudoClass, NthExpression, SelectorComponent,
    AttributeSelector, AttributeMatcher, ElementContext, ElementStates,
    match_component, match_pseudo_class, SelectorBloomFilter, Direction,
    parse_forgiving_selector_list, parse_simple_selector,
};
pub use style_cache::{StyleCache, StyleCacheKey, SharedStyle, CacheStats};
pub use container::{ContainerContext, ContainerQuery, ContainerRegistry};
pub use mask::{Mask, MaskLayer, MaskImage, Isolation, MaskComposite, MaskMode};
pub use web_animations::{
    Animation, AnimationEffect, Keyframe, PlayState, DocumentAnimations,
    Fixed16 as AnimationFixed16, DeterministicTiming,
};
pub use rule_tree::{RuleTree, RuleNode, PackedValue, ColorInterner, RuleSpecificity, CascadeLevel};
pub use inheritance::{InheritanceSnapshot, InheritedProperties, CustomPropertyResolver, OnDemandStyler};
pub use selector_opt::{SelectorIndex, RtlMatcher, HybridSelector, CompiledSelector};
pub use transitions::{
    Transition, ActiveTransition, TransitionEngine,
    TimingFunction, StepPosition, Fixed16 as TransitionFixed16,
};

/// Parse a CSS stylesheet
pub fn parse_stylesheet(css: &str) -> Result<Stylesheet, CssError> {
    CssParser::new().parse(css)
}

/// Parsed stylesheet
#[derive(Debug, Default)]
pub struct Stylesheet {
    pub rules: Vec<Rule>,
}

impl Stylesheet {
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }
    
    /// Number of rules
    pub fn len(&self) -> usize {
        self.rules.len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

/// CSS rule (selector list + declarations)
#[derive(Debug)]
pub struct Rule {
    pub selectors: Vec<Selector>,
    pub declarations: Vec<Declaration>,
}

/// CSS selector with parsed components
#[derive(Debug, Clone)]
pub struct Selector {
    /// Original selector text
    pub text: String,
    /// Specificity (id, class, type)
    pub specificity: Specificity,
    /// Parsed selector parts
    pub parts: Vec<SelectorPart>,
}

/// Part of a compound selector
#[derive(Debug, Clone)]
pub enum SelectorPart {
    /// Type selector (div, span, etc)
    Type(String),
    /// Class selector (.class)
    Class(String),
    /// ID selector (#id)
    Id(String),
    /// Universal selector (*)
    Universal,
    /// Attribute selector ([attr=value])
    Attribute { name: String, op: AttrOp, value: String },
    /// Pseudo-class (:hover, :first-child)
    PseudoClass(String),
    /// Pseudo-element (::before, ::after)
    PseudoElement(String),
    /// Combinator
    Combinator(Combinator),
}

/// Attribute selector operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttrOp {
    Exists,     // [attr]
    Equals,     // [attr=value]
    Contains,   // [attr*=value]
    StartsWith, // [attr^=value]
    EndsWith,   // [attr$=value]
    Includes,   // [attr~=value]
    DashMatch,  // [attr|=value]
}

/// Selector combinators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Combinator {
    /// Descendant (space)
    Descendant,
    /// Direct child (>)
    Child,
    /// Adjacent sibling (+)
    NextSibling,
    /// General sibling (~)
    SubsequentSibling,
}

/// Selector specificity (a, b, c) where:
/// a = ID selectors
/// b = class, attribute, pseudo-class
/// c = type, pseudo-element
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Specificity(pub u32, pub u32, pub u32);

impl Specificity {
    pub fn new(ids: u32, classes: u32, types: u32) -> Self {
        Self(ids, classes, types)
    }
    
    /// Add another specificity to this one
    pub fn add(&mut self, other: Specificity) {
        self.0 += other.0;
        self.1 += other.1;
        self.2 += other.2;
    }
}

/// CSS declaration (property: value)
#[derive(Debug, Clone)]
pub struct Declaration {
    pub property: PropertyId,
    pub value: PropertyValue,
    pub important: bool,
}

/// CSS parsing error
#[derive(Debug, thiserror::Error)]
pub enum CssError {
    #[error("Parse error at line {line}: {message}")]
    ParseError { line: u32, message: String },
    
    #[error("Invalid property: {0}")]
    InvalidProperty(String),
    
    #[error("Invalid value for {property}: {value}")]
    InvalidValue { property: String, value: String },
}
