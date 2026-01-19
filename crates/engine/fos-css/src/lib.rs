//! fOS CSS Parser & Style System
//!
//! CSS parsing using lightningcss with style cascade implementation.
//! Designed for memory efficiency with computed style sharing.
//! Includes CSS Custom Properties (variables), calc(), and math functions.
//!
//! ## Features Implemented (CSS Roadmap)
//!
//! - **Phase 1**: Selector performance with 8-hash Bloom filter, ancestor filter, specificity cache
//! - **Phase 2**: Copy-on-write inheritance, flat custom properties
//! - **Phase 3**: :has() selector, CSS Nesting, @layer, @scope, anchor positioning, view transitions
//! - **Phase 4**: Compositor thread animations, keyframe pre-computation
//! - **Phase 5**: Predictive styling, JIT selector compilation

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
pub mod style_sharing;
pub mod selector_match_cache;
pub mod parallel_css_parser;
pub mod parallel_style;
pub mod subtree_isolation;

// Phase 1: Selector Performance
pub mod selector_bloom;
pub mod selector_split;

// Phase 2: Style Computation
pub mod cow_style;

// Phase 3: CSS Features Parity
pub mod has_selector;
pub mod nesting;
pub mod layers;
pub mod scope;
pub mod anchor;
pub mod view_transitions;

// Phase 5: Surpassing Chromium
pub mod predictive;

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
    // Phase 4: Animation performance
    KeyframePrecompute, PrecomputedAnimation, PrecomputedValue,
    CompositorAnimation, CompositorProperty, CompositorValue,
    CompositorAnimationController, CompositorAnimationState,
};
pub use rule_tree::{RuleTree, RuleNode, PackedValue, ColorInterner, RuleSpecificity, CascadeLevel};
pub use inheritance::{InheritanceSnapshot, InheritedProperties, CustomPropertyResolver, OnDemandStyler};
pub use selector_opt::{SelectorIndex, RtlMatcher, HybridSelector, CompiledSelector};
pub use transitions::{
    Transition, ActiveTransition, TransitionEngine,
    TimingFunction, StepPosition, Fixed16 as TransitionFixed16,
};
pub use style_sharing::{
    StyleSharingCache, StyleKey, SharedStyleRef, SharingStats,
    StyleBloomKey, StyleHasher, ElementContext as SharingElementContext,
};
pub use selector_match_cache::{
    SelectorMatchCache, SelectorMatchKey, BloomFilter, MatchCacheStats,
};

// Phase 1 exports
pub use selector_bloom::{
    SelectorBloomFilter as BloomFilter8Hash, AncestorBloom,
    SpecificityCache, Specificity as BloomSpecificity, SelectorId,
    AcceleratedSelectorMatcher, SelectorEntry, MatcherStats,
};
pub use selector_split::{
    SelectorSplitter, SelectorFragment, SimpleSelector as SplitSimpleSelector,
    Combinator as SplitCombinator, parallelize_selector, AttributeMatcher as SplitAttributeMatcher,
};

// Phase 2 exports
pub use cow_style::{
    CowInheritedProps, FlatCustomProperties,
    FontStyle as CowFontStyle, LineHeight as CowLineHeight,
    TextAlign as CowTextAlign, Color as CowColor,
};

// Phase 3 exports
pub use has_selector::{
    HasSelector, HasSelectorId, HasMatcher, HasSelectorCache,
    RelativeSelector, RelativeCombinator, HasMatchContext,
    parse_has_argument,
};
pub use nesting::{
    NestedRule, NestableSelector, FlatRule,
    parse_nested_block, resolve_nested_selectors,
};
pub use layers::{
    LayerId, CascadeLayer, LayerRegistry, LayeredRule,
    LayerStatement, parse_layer_statements, layer_wins,
};
pub use scope::{
    ScopeId, CssScope, ScopeRegistry, ScopeMatcher, ScopeMatch,
    ScopeStatement, parse_scope_statements, resolve_scope_selector,
};
pub use anchor::{
    AnchorName, AnchorSide, AnchorSize, AnchorInfo, AnchorRect,
    AnchorRegistry, PositionFallbackRegistry,
    evaluate_anchor, evaluate_anchor_size, parse_anchor_function,
};
pub use view_transitions::{
    ViewTransitionManager, ViewTransitionGroup, TransitionSnapshot,
    TransitionState, ViewTransitionPseudo,
    parse_view_transition_name,
};

// Phase 5 exports
pub use predictive::{
    PredictiveStyleEngine, PredictedStyleCache, PredictedStyle,
    PredictableState, StateSet, OffScreenPredictor,
};

// JIT Selector module
pub mod jit_selector;

pub use jit_selector::{
    SelectorCompiler, CompiledSelector as JitCompiledSelector, Opcode,
    JitMatchContext, execute_compiled, CompilerStats,
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
