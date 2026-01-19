//! JIT Selector Compilation
//!
//! Compile frequently-used CSS selectors to optimized bytecode for fast matching.
//! Uses a custom bytecode VM instead of native code for portability.

use std::collections::HashMap;

// ============================================================================
// Bytecode Instructions
// ============================================================================

/// Bytecode instruction for selector matching
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opcode {
    /// Match element tag name (index into string table)
    MatchTag(u16),
    /// Match element class (index into string table)
    MatchClass(u16),
    /// Match element ID (index into string table)
    MatchId(u16),
    /// Match attribute exists (index into string table)
    MatchAttrExists(u16),
    /// Match attribute value exactly (name index, value index)
    MatchAttrExact(u16, u16),
    /// Match attribute prefix (name index, value index)
    MatchAttrPrefix(u16, u16),
    /// Match attribute suffix (name index, value index)
    MatchAttrSuffix(u16, u16),
    /// Match attribute contains (name index, value index)
    MatchAttrContains(u16, u16),
    /// Match universal selector (always true)
    MatchUniversal,
    /// Check Bloom filter for class (hash value)
    BloomCheckClass(u32),
    /// Check Bloom filter for ID (hash value)
    BloomCheckId(u32),
    /// Check Bloom filter for tag (hash value)
    BloomCheckTag(u32),
    /// Move to parent element
    MoveToParent,
    /// Move to previous sibling
    MoveToPrevSibling,
    /// Branch if match failed (offset)
    BranchIfFail(i16),
    /// Branch unconditionally (offset)
    Branch(i16),
    /// Match succeeded
    Success,
    /// Match failed
    Fail,
    /// Check if at root (fail if so)
    CheckNotRoot,
    /// Save current position for backtracking
    SavePosition,
    /// Restore saved position
    RestorePosition,
    /// Match pseudo-class (index into pseudo-class table)
    MatchPseudoClass(u16),
    /// AND - all following conditions must match until EndAnd
    BeginAnd,
    /// End AND block
    EndAnd,
    /// OR - any of following conditions must match until EndOr
    BeginOr,
    /// End OR block
    EndOr,
}

// ============================================================================
// Compiled Selector
// ============================================================================

/// A compiled selector ready for fast matching
#[derive(Debug, Clone)]
pub struct CompiledSelector {
    /// Bytecode instructions
    pub bytecode: Vec<Opcode>,
    /// String table for tag/class/id/attr names
    pub strings: Vec<Box<str>>,
    /// Pre-computed hash for Bloom filter checks
    pub bloom_hashes: Vec<u32>,
    /// Original selector text (for debugging)
    pub source: Box<str>,
    /// Estimated complexity (for prioritization)
    pub complexity: u32,
    /// Hit count for hot selector detection
    pub hit_count: u64,
}

impl CompiledSelector {
    /// Create an empty compiled selector
    pub fn new(source: &str) -> Self {
        Self {
            bytecode: Vec::new(),
            strings: Vec::new(),
            bloom_hashes: Vec::new(),
            source: source.into(),
            complexity: 0,
            hit_count: 0,
        }
    }
    
    /// Add a string to the string table, return index
    fn intern_string(&mut self, s: &str) -> u16 {
        if let Some(idx) = self.strings.iter().position(|x| x.as_ref() == s) {
            return idx as u16;
        }
        let idx = self.strings.len() as u16;
        self.strings.push(s.into());
        idx
    }
    
    /// Emit an opcode
    fn emit(&mut self, op: Opcode) {
        self.bytecode.push(op);
        self.complexity += 1;
    }
    
    /// Current bytecode position
    fn position(&self) -> usize {
        self.bytecode.len()
    }
    
    /// Patch a branch instruction
    fn patch_branch(&mut self, pos: usize, target: usize) {
        let offset = (target as i32 - pos as i32) as i16;
        match &mut self.bytecode[pos] {
            Opcode::BranchIfFail(o) | Opcode::Branch(o) => {
                *o = offset;
            }
            _ => {}
        }
    }
}

// ============================================================================
// Selector Compiler
// ============================================================================

/// Compile CSS selectors to bytecode
#[derive(Debug)]
pub struct SelectorCompiler {
    /// Compiled selector cache
    cache: HashMap<Box<str>, CompiledSelector>,
    /// Hot selector threshold (compile after N uses)
    hot_threshold: u64,
    /// Statistics
    stats: CompilerStats,
}

/// Compiler statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct CompilerStats {
    pub selectors_compiled: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub total_bytecode_size: usize,
}

impl Default for SelectorCompiler {
    fn default() -> Self {
        Self::new()
    }
}

impl SelectorCompiler {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            hot_threshold: 10,
            stats: CompilerStats::default(),
        }
    }
    
    /// Compile a selector to bytecode
    pub fn compile(&mut self, selector: &str) -> &CompiledSelector {
        // Check cache first
        if self.cache.contains_key(selector) {
            self.stats.cache_hits += 1;
            let compiled = self.cache.get_mut(selector).unwrap();
            compiled.hit_count += 1;
            return self.cache.get(selector).unwrap();
        }
        
        self.stats.cache_misses += 1;
        
        // Compile the selector
        let compiled = self.compile_selector(selector);
        self.stats.selectors_compiled += 1;
        self.stats.total_bytecode_size += compiled.bytecode.len();
        
        self.cache.insert(selector.into(), compiled);
        self.cache.get(selector).unwrap()
    }
    
    /// Internal compilation
    fn compile_selector(&self, selector: &str) -> CompiledSelector {
        let mut compiled = CompiledSelector::new(selector);
        
        // Parse and compile the selector
        let parts = parse_selector_parts(selector);
        
        // Compile right-to-left (subject first)
        for (i, part) in parts.iter().rev().enumerate() {
            if i > 0 {
                // Need to traverse to ancestor/sibling
                match part.combinator {
                    Some(Combinator::Descendant) => {
                        // Loop: move to parent until match or root
                        compiled.emit(Opcode::SavePosition);
                        let loop_start = compiled.position();
                        compiled.emit(Opcode::MoveToParent);
                        compiled.emit(Opcode::CheckNotRoot);
                        let check_pos = compiled.position();
                        compiled.emit(Opcode::BranchIfFail(0)); // Will patch
                        
                        // Try to match
                        self.compile_simple_selectors(&mut compiled, &part.simple);
                        let success_pos = compiled.position();
                        compiled.emit(Opcode::BranchIfFail(0)); // Will patch to loop
                        
                        // Patch branches
                        let end_pos = compiled.position();
                        compiled.patch_branch(check_pos, end_pos + 1);
                        compiled.patch_branch(success_pos, loop_start);
                    }
                    Some(Combinator::Child) => {
                        compiled.emit(Opcode::MoveToParent);
                        compiled.emit(Opcode::CheckNotRoot);
                        self.compile_simple_selectors(&mut compiled, &part.simple);
                    }
                    Some(Combinator::NextSibling) => {
                        compiled.emit(Opcode::MoveToPrevSibling);
                        self.compile_simple_selectors(&mut compiled, &part.simple);
                    }
                    Some(Combinator::SubsequentSibling) => {
                        // Loop: move to prev sibling until match or no more
                        let loop_start = compiled.position();
                        compiled.emit(Opcode::MoveToPrevSibling);
                        let check_pos = compiled.position();
                        compiled.emit(Opcode::BranchIfFail(0));
                        
                        self.compile_simple_selectors(&mut compiled, &part.simple);
                        let match_pos = compiled.position();
                        compiled.emit(Opcode::BranchIfFail(0)); // Loop back
                        
                        let end_pos = compiled.position();
                        compiled.patch_branch(check_pos, end_pos);
                        compiled.patch_branch(match_pos, loop_start);
                    }
                    None => {}
                }
            } else {
                // Subject element - just match
                self.compile_simple_selectors(&mut compiled, &part.simple);
            }
        }
        
        compiled.emit(Opcode::Success);
        compiled
    }
    
    /// Compile simple selectors for a compound selector
    fn compile_simple_selectors(&self, compiled: &mut CompiledSelector, selectors: &[SimpleSelector]) {
        if selectors.len() > 1 {
            compiled.emit(Opcode::BeginAnd);
        }
        
        for sel in selectors {
            match sel {
                SimpleSelector::Universal => {
                    compiled.emit(Opcode::MatchUniversal);
                }
                SimpleSelector::Tag(tag) => {
                    let hash = hash_string(tag);
                    compiled.bloom_hashes.push(hash);
                    compiled.emit(Opcode::BloomCheckTag(hash));
                    let idx = compiled.intern_string(tag);
                    compiled.emit(Opcode::MatchTag(idx));
                }
                SimpleSelector::Class(class) => {
                    let hash = hash_string(class);
                    compiled.bloom_hashes.push(hash);
                    compiled.emit(Opcode::BloomCheckClass(hash));
                    let idx = compiled.intern_string(class);
                    compiled.emit(Opcode::MatchClass(idx));
                }
                SimpleSelector::Id(id) => {
                    let hash = hash_string(id);
                    compiled.bloom_hashes.push(hash);
                    compiled.emit(Opcode::BloomCheckId(hash));
                    let idx = compiled.intern_string(id);
                    compiled.emit(Opcode::MatchId(idx));
                }
                SimpleSelector::Attribute { name, value } => {
                    let name_idx = compiled.intern_string(name);
                    if let Some(val) = value {
                        let val_idx = compiled.intern_string(val);
                        compiled.emit(Opcode::MatchAttrExact(name_idx, val_idx));
                    } else {
                        compiled.emit(Opcode::MatchAttrExists(name_idx));
                    }
                }
                SimpleSelector::PseudoClass(pseudo) => {
                    let idx = compiled.intern_string(pseudo);
                    compiled.emit(Opcode::MatchPseudoClass(idx));
                }
            }
        }
        
        if selectors.len() > 1 {
            compiled.emit(Opcode::EndAnd);
        }
    }
    
    /// Get statistics
    pub fn stats(&self) -> &CompilerStats {
        &self.stats
    }
    
    /// Get hot selectors (most frequently used)
    pub fn hot_selectors(&self, limit: usize) -> Vec<&CompiledSelector> {
        let mut selectors: Vec<_> = self.cache.values().collect();
        selectors.sort_by(|a, b| b.hit_count.cmp(&a.hit_count));
        selectors.truncate(limit);
        selectors
    }
    
    /// Clear the cache
    pub fn clear(&mut self) {
        self.cache.clear();
    }
}

// ============================================================================
// Bytecode VM
// ============================================================================

/// Element context for bytecode execution
pub trait JitMatchContext {
    fn tag_name(&self) -> &str;
    fn has_class(&self, class: &str) -> bool;
    fn id(&self) -> Option<&str>;
    fn get_attribute(&self, name: &str) -> Option<&str>;
    fn matches_pseudo_class(&self, pseudo: &str) -> bool;
    fn parent(&self) -> Option<Self> where Self: Sized;
    fn prev_sibling(&self) -> Option<Self> where Self: Sized;
    fn is_root(&self) -> bool;
    fn check_bloom(&self, hash: u32) -> bool;
}

/// Execute compiled selector against an element
pub fn execute_compiled<E: JitMatchContext + Clone>(
    compiled: &CompiledSelector,
    element: E,
) -> bool {
    let mut vm = BytecodeVm::new(compiled, element);
    vm.execute()
}

/// Bytecode virtual machine
struct BytecodeVm<'a, E> {
    compiled: &'a CompiledSelector,
    current: Option<E>,
    saved: Vec<Option<E>>,
    pc: usize,
    and_depth: usize,
    and_failed: bool,
    or_depth: usize,
    or_succeeded: bool,
}

impl<'a, E: JitMatchContext + Clone> BytecodeVm<'a, E> {
    fn new(compiled: &'a CompiledSelector, element: E) -> Self {
        Self {
            compiled,
            current: Some(element),
            saved: Vec::new(),
            pc: 0,
            and_depth: 0,
            and_failed: false,
            or_depth: 0,
            or_succeeded: false,
        }
    }
    
    fn execute(&mut self) -> bool {
        while self.pc < self.compiled.bytecode.len() {
            let op = self.compiled.bytecode[self.pc];
            self.pc += 1;
            
            match op {
                Opcode::MatchTag(idx) => {
                    let tag = &self.compiled.strings[idx as usize];
                    if !self.current.as_ref().map_or(false, |e| e.tag_name().eq_ignore_ascii_case(tag)) {
                        if self.and_depth > 0 {
                            self.and_failed = true;
                        } else if self.or_depth == 0 {
                            return false;
                        }
                    }
                }
                Opcode::MatchClass(idx) => {
                    let class = &self.compiled.strings[idx as usize];
                    if !self.current.as_ref().map_or(false, |e| e.has_class(class)) {
                        if self.and_depth > 0 {
                            self.and_failed = true;
                        } else if self.or_depth == 0 {
                            return false;
                        }
                    }
                }
                Opcode::MatchId(idx) => {
                    let id = &self.compiled.strings[idx as usize];
                    if !self.current.as_ref().map_or(false, |e| e.id() == Some(id.as_ref())) {
                        if self.and_depth > 0 {
                            self.and_failed = true;
                        } else if self.or_depth == 0 {
                            return false;
                        }
                    }
                }
                Opcode::MatchAttrExists(idx) => {
                    let name = &self.compiled.strings[idx as usize];
                    if !self.current.as_ref().map_or(false, |e| e.get_attribute(name).is_some()) {
                        if self.and_depth > 0 {
                            self.and_failed = true;
                        } else if self.or_depth == 0 {
                            return false;
                        }
                    }
                }
                Opcode::MatchAttrExact(name_idx, val_idx) => {
                    let name = &self.compiled.strings[name_idx as usize];
                    let val = &self.compiled.strings[val_idx as usize];
                    let matches = self.current.as_ref().map_or(false, |e| {
                        e.get_attribute(name) == Some(val.as_ref())
                    });
                    if !matches {
                        if self.and_depth > 0 {
                            self.and_failed = true;
                        } else if self.or_depth == 0 {
                            return false;
                        }
                    }
                }
                Opcode::MatchAttrPrefix(name_idx, val_idx) => {
                    let name = &self.compiled.strings[name_idx as usize];
                    let val = &self.compiled.strings[val_idx as usize];
                    let matches = self.current.as_ref().map_or(false, |e| {
                        e.get_attribute(name).map_or(false, |v| v.starts_with(val.as_ref()))
                    });
                    if !matches {
                        if self.and_depth > 0 { self.and_failed = true; }
                        else if self.or_depth == 0 { return false; }
                    }
                }
                Opcode::MatchAttrSuffix(name_idx, val_idx) => {
                    let name = &self.compiled.strings[name_idx as usize];
                    let val = &self.compiled.strings[val_idx as usize];
                    let matches = self.current.as_ref().map_or(false, |e| {
                        e.get_attribute(name).map_or(false, |v| v.ends_with(val.as_ref()))
                    });
                    if !matches {
                        if self.and_depth > 0 { self.and_failed = true; }
                        else if self.or_depth == 0 { return false; }
                    }
                }
                Opcode::MatchAttrContains(name_idx, val_idx) => {
                    let name = &self.compiled.strings[name_idx as usize];
                    let val = &self.compiled.strings[val_idx as usize];
                    let matches = self.current.as_ref().map_or(false, |e| {
                        e.get_attribute(name).map_or(false, |v| v.contains(val.as_ref()))
                    });
                    if !matches {
                        if self.and_depth > 0 { self.and_failed = true; }
                        else if self.or_depth == 0 { return false; }
                    }
                }
                Opcode::MatchUniversal => {
                    // Always matches
                }
                Opcode::BloomCheckClass(hash) | Opcode::BloomCheckId(hash) | Opcode::BloomCheckTag(hash) => {
                    // Fast rejection via Bloom filter
                    if !self.current.as_ref().map_or(false, |e| e.check_bloom(hash)) {
                        return false;
                    }
                }
                Opcode::MatchPseudoClass(idx) => {
                    let pseudo = &self.compiled.strings[idx as usize];
                    if !self.current.as_ref().map_or(false, |e| e.matches_pseudo_class(pseudo)) {
                        if self.and_depth > 0 { self.and_failed = true; }
                        else if self.or_depth == 0 { return false; }
                    }
                }
                Opcode::MoveToParent => {
                    self.current = self.current.take().and_then(|e| e.parent());
                    if self.current.is_none() {
                        return false;
                    }
                }
                Opcode::MoveToPrevSibling => {
                    self.current = self.current.take().and_then(|e| e.prev_sibling());
                    if self.current.is_none() {
                        return false;
                    }
                }
                Opcode::CheckNotRoot => {
                    if self.current.as_ref().map_or(true, |e| e.is_root()) {
                        return false;
                    }
                }
                Opcode::BranchIfFail(offset) => {
                    if self.and_failed || self.current.is_none() {
                        self.pc = (self.pc as i32 + offset as i32) as usize;
                        self.and_failed = false;
                    }
                }
                Opcode::Branch(offset) => {
                    self.pc = (self.pc as i32 + offset as i32) as usize;
                }
                Opcode::SavePosition => {
                    self.saved.push(self.current.clone());
                }
                Opcode::RestorePosition => {
                    self.current = self.saved.pop().flatten();
                }
                Opcode::BeginAnd => {
                    self.and_depth += 1;
                    self.and_failed = false;
                }
                Opcode::EndAnd => {
                    self.and_depth -= 1;
                    if self.and_failed && self.or_depth == 0 {
                        return false;
                    }
                }
                Opcode::BeginOr => {
                    self.or_depth += 1;
                    self.or_succeeded = false;
                }
                Opcode::EndOr => {
                    self.or_depth -= 1;
                    if !self.or_succeeded {
                        return false;
                    }
                }
                Opcode::Success => {
                    return true;
                }
                Opcode::Fail => {
                    return false;
                }
            }
        }
        
        false
    }
}

// ============================================================================
// Helper Types and Functions
// ============================================================================

/// Parsed selector part
#[derive(Debug, Clone)]
struct SelectorPart {
    simple: Vec<SimpleSelector>,
    combinator: Option<Combinator>,
}

/// Simple selector types
#[derive(Debug, Clone)]
enum SimpleSelector {
    Universal,
    Tag(Box<str>),
    Class(Box<str>),
    Id(Box<str>),
    Attribute { name: Box<str>, value: Option<Box<str>> },
    PseudoClass(Box<str>),
}

/// Combinator types
#[derive(Debug, Clone, Copy)]
enum Combinator {
    Descendant,
    Child,
    NextSibling,
    SubsequentSibling,
}

/// Parse selector into parts
fn parse_selector_parts(selector: &str) -> Vec<SelectorPart> {
    let mut parts = Vec::new();
    let mut current_simple = Vec::new();
    let mut chars = selector.chars().peekable();
    let mut current_token = String::new();
    
    while let Some(c) = chars.next() {
        match c {
            ' ' => {
                if !current_token.is_empty() {
                    add_simple_selector(&current_token, &mut current_simple);
                    current_token.clear();
                }
                
                // Skip extra whitespace
                while chars.peek() == Some(&' ') {
                    chars.next();
                }
                
                // Check for combinator
                let combinator = match chars.peek() {
                    Some('>') => {
                        chars.next();
                        skip_ws(&mut chars);
                        Some(Combinator::Child)
                    }
                    Some('+') => {
                        chars.next();
                        skip_ws(&mut chars);
                        Some(Combinator::NextSibling)
                    }
                    Some('~') => {
                        chars.next();
                        skip_ws(&mut chars);
                        Some(Combinator::SubsequentSibling)
                    }
                    _ => Some(Combinator::Descendant),
                };
                
                if !current_simple.is_empty() {
                    parts.push(SelectorPart {
                        simple: std::mem::take(&mut current_simple),
                        combinator,
                    });
                }
            }
            '>' | '+' | '~' => {
                if !current_token.is_empty() {
                    add_simple_selector(&current_token, &mut current_simple);
                    current_token.clear();
                }
                skip_ws(&mut chars);
                
                let combinator = match c {
                    '>' => Some(Combinator::Child),
                    '+' => Some(Combinator::NextSibling),
                    '~' => Some(Combinator::SubsequentSibling),
                    _ => None,
                };
                
                if !current_simple.is_empty() {
                    parts.push(SelectorPart {
                        simple: std::mem::take(&mut current_simple),
                        combinator,
                    });
                }
            }
            '#' | '.' => {
                if !current_token.is_empty() {
                    add_simple_selector(&current_token, &mut current_simple);
                    current_token.clear();
                }
                current_token.push(c);
            }
            '[' => {
                if !current_token.is_empty() {
                    add_simple_selector(&current_token, &mut current_simple);
                    current_token.clear();
                }
                let attr = parse_attribute(&mut chars);
                current_simple.push(attr);
            }
            ':' if chars.peek() != Some(&':') => {
                if !current_token.is_empty() {
                    add_simple_selector(&current_token, &mut current_simple);
                    current_token.clear();
                }
                let pseudo = collect_pseudo(&mut chars);
                current_simple.push(SimpleSelector::PseudoClass(pseudo.into()));
            }
            _ => {
                current_token.push(c);
            }
        }
    }
    
    if !current_token.is_empty() {
        add_simple_selector(&current_token, &mut current_simple);
    }
    
    if !current_simple.is_empty() {
        parts.push(SelectorPart {
            simple: current_simple,
            combinator: None,
        });
    }
    
    parts
}

fn skip_ws(chars: &mut std::iter::Peekable<std::str::Chars>) {
    while chars.peek() == Some(&' ') {
        chars.next();
    }
}

fn add_simple_selector(token: &str, selectors: &mut Vec<SimpleSelector>) {
    let token = token.trim();
    if token.is_empty() {
        return;
    }
    
    if token == "*" {
        selectors.push(SimpleSelector::Universal);
    } else if let Some(id) = token.strip_prefix('#') {
        selectors.push(SimpleSelector::Id(id.into()));
    } else if let Some(class) = token.strip_prefix('.') {
        selectors.push(SimpleSelector::Class(class.into()));
    } else {
        selectors.push(SimpleSelector::Tag(token.into()));
    }
}

fn parse_attribute(chars: &mut std::iter::Peekable<std::str::Chars>) -> SimpleSelector {
    let mut name = String::new();
    let mut value: Option<String> = None;
    let mut in_value = false;
    let mut quote_char: Option<char> = None;
    
    while let Some(c) = chars.next() {
        if c == ']' && quote_char.is_none() {
            break;
        }
        
        if in_value {
            if let Some(q) = quote_char {
                if c == q {
                    quote_char = None;
                } else if let Some(ref mut v) = value {
                    v.push(c);
                }
            } else if c == '"' || c == '\'' {
                quote_char = Some(c);
            } else if let Some(ref mut v) = value {
                v.push(c);
            }
        } else if c == '=' {
            in_value = true;
            value = Some(String::new());
        } else if !c.is_whitespace() {
            name.push(c);
        }
    }
    
    SimpleSelector::Attribute {
        name: name.into(),
        value: value.map(|v| -> Box<str> { v.into() }),
    }
}

fn collect_pseudo(chars: &mut std::iter::Peekable<std::str::Chars>) -> String {
    let mut name = String::new();
    let mut paren_depth = 0;
    
    while let Some(&c) = chars.peek() {
        if c == '(' {
            paren_depth += 1;
            name.push(c);
            chars.next();
        } else if c == ')' {
            if paren_depth > 0 {
                paren_depth -= 1;
                name.push(c);
                chars.next();
                if paren_depth == 0 {
                    break;
                }
            } else {
                break;
            }
        } else if paren_depth > 0 || c.is_alphanumeric() || c == '-' || c == '_' {
            name.push(c);
            chars.next();
        } else {
            break;
        }
    }
    
    name
}

fn hash_string(s: &str) -> u32 {
    let mut hash = 0x811c9dc5u32;
    for byte in s.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(0x01000193);
    }
    hash
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_compile_simple_tag() {
        let mut compiler = SelectorCompiler::new();
        let compiled = compiler.compile("div");
        
        assert!(!compiled.bytecode.is_empty());
        assert!(compiled.strings.iter().any(|s| s.as_ref() == "div"));
    }
    
    #[test]
    fn test_compile_class() {
        let mut compiler = SelectorCompiler::new();
        let compiled = compiler.compile(".container");
        
        assert!(!compiled.bytecode.is_empty());
        assert!(compiled.strings.iter().any(|s| s.as_ref() == "container"));
    }
    
    #[test]
    fn test_compile_compound() {
        let mut compiler = SelectorCompiler::new();
        let compiled = compiler.compile("div.container#main");
        
        assert!(!compiled.bytecode.is_empty());
        assert_eq!(compiled.strings.len(), 3);
    }
    
    #[test]
    fn test_compile_descendant() {
        let mut compiler = SelectorCompiler::new();
        let compiled = compiler.compile(".foo .bar");
        
        // Should have MoveToParent instruction
        assert!(compiled.bytecode.iter().any(|op| matches!(op, Opcode::MoveToParent)));
    }
    
    #[test]
    fn test_compile_child() {
        let mut compiler = SelectorCompiler::new();
        let compiled = compiler.compile(".foo > .bar");
        
        assert!(compiled.bytecode.iter().any(|op| matches!(op, Opcode::MoveToParent)));
    }
    
    #[test]
    fn test_cache_hit() {
        let mut compiler = SelectorCompiler::new();
        
        compiler.compile(".test");
        compiler.compile(".test");
        
        assert_eq!(compiler.stats().cache_hits, 1);
        assert_eq!(compiler.stats().selectors_compiled, 1);
    }
    
    #[test]
    fn test_hot_selectors() {
        let mut compiler = SelectorCompiler::new();
        
        for _ in 0..100 {
            compiler.compile(".hot");
        }
        for _ in 0..10 {
            compiler.compile(".cold");
        }
        
        let hot = compiler.hot_selectors(1);
        assert_eq!(hot[0].source.as_ref(), ".hot");
    }
}
