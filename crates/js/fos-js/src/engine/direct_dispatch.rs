//! Direct Threading Dispatch & Superinstructions
//!
//! Optimized dispatch mechanisms for the bytecode interpreter.
//! 
//! ## Direct Threading
//! Uses a table of handler pointers instead of a switch statement.
//! Each handler knows where to jump next without going through dispatch.
//!
//! ## Superinstructions
//! Common instruction sequences are fused into single instructions:
//! - LoadLocal + Add → LoadLocalAdd
//! - GetProperty + Call → GetPropertyCall
//! - LoadConst + Return → ReturnConst
//!
//! ## Stack Caching
//! Keeps top-of-stack values in registers to avoid memory access.

use super::bytecode::Opcode;
use std::collections::HashMap;

/// Superinstruction opcode
/// These combine common instruction sequences for better performance
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SuperOpcode {
    // ===== Load + Arithmetic =====
    /// LoadLocal + Add: load local and add to TOS
    LoadLocalAdd = 0,
    /// LoadLocal + Sub: load local and subtract from TOS
    LoadLocalSub = 1,
    /// LoadLocal + Mul: load local and multiply with TOS
    LoadLocalMul = 2,
    
    // ===== Load + Compare =====
    /// LoadLocal + Lt: load local and compare less than
    LoadLocalLt = 10,
    /// LoadLocal + Le: load local and compare less than or equal
    LoadLocalLe = 11,
    /// LoadLocal + Gt: load local and compare greater than
    LoadLocalGt = 12,
    /// LoadLocal + Eq: load local and compare equal
    LoadLocalEq = 13,
    
    // ===== Get + Call =====
    /// GetProperty + Call0: get property and call with no args
    GetPropertyCall0 = 20,
    /// GetProperty + Call1: get property and call with 1 arg
    GetPropertyCall1 = 21,
    /// GetProperty + Call: get property and call with N args
    GetPropertyCall = 22,
    
    // ===== Load + Store =====
    /// LoadConst + SetLocal: load constant directly to local
    LoadConstSetLocal = 30,
    /// LoadLocal + SetLocal: copy local to local
    CopyLocal = 31,
    
    // ===== Branch Combinations =====
    /// LoadLocal + JumpIfFalse: test local and jump
    LoadLocalJumpIfFalse = 40,
    /// LoadLocal + JumpIfTrue: test local and jump
    LoadLocalJumpIfTrue = 41,
    /// Compare + JumpIfFalse: compare and conditional jump
    CompareJumpIfFalse = 42,
    /// Compare + JumpIfTrue: compare and conditional jump
    CompareJumpIfTrue = 43,
    
    // ===== Return Combinations =====
    /// LoadConst + Return: return constant value
    ReturnConst = 50,
    /// LoadLocal + Return: return local variable
    ReturnLocal = 51,
    /// LoadUndefined + Return: return undefined (common)
    ReturnUndefined = 52,
    
    // ===== Increment/Decrement =====
    /// GetLocal + Inc + SetLocal: increment local in place
    IncLocal = 60,
    /// GetLocal + Dec + SetLocal: decrement local in place
    DecLocal = 61,
    
    // ===== Property Access =====
    /// GetProperty + GetProperty: chain property access
    GetPropertyChain = 70,
    /// LoadThis + GetProperty: access property on this
    ThisGetProperty = 71,
}

/// Pattern matcher for superinstruction detection
#[derive(Debug, Clone)]
pub struct SuperInstructionPattern {
    /// Opcodes that form this pattern
    pub opcodes: Vec<Opcode>,
    /// Resulting superinstruction
    pub super_opcode: SuperOpcode,
    /// Total bytes consumed by pattern
    pub bytes_consumed: usize,
}

/// Superinstruction transformer
#[derive(Debug, Default)]
pub struct SuperInstructionTransformer {
    /// Known patterns
    patterns: Vec<SuperInstructionPattern>,
    /// Stats
    transformations: u64,
    bytes_saved: u64,
}

impl SuperInstructionTransformer {
    pub fn new() -> Self {
        let mut transformer = Self::default();
        transformer.register_default_patterns();
        transformer
    }

    /// Register default superinstruction patterns
    fn register_default_patterns(&mut self) {
        // LoadLocal + Add
        self.patterns.push(SuperInstructionPattern {
            opcodes: vec![Opcode::GetLocal, Opcode::Add],
            super_opcode: SuperOpcode::LoadLocalAdd,
            bytes_consumed: 4, // GetLocal(3) + Add(1)
        });

        // LoadLocal + Sub
        self.patterns.push(SuperInstructionPattern {
            opcodes: vec![Opcode::GetLocal, Opcode::Sub],
            super_opcode: SuperOpcode::LoadLocalSub,
            bytes_consumed: 4,
        });

        // LoadLocal + Lt
        self.patterns.push(SuperInstructionPattern {
            opcodes: vec![Opcode::GetLocal, Opcode::Lt],
            super_opcode: SuperOpcode::LoadLocalLt,
            bytes_consumed: 4,
        });

        // LoadLocal + JumpIfFalse
        self.patterns.push(SuperInstructionPattern {
            opcodes: vec![Opcode::GetLocal, Opcode::JumpIfFalse],
            super_opcode: SuperOpcode::LoadLocalJumpIfFalse,
            bytes_consumed: 6, // GetLocal(3) + JumpIfFalse(3)
        });

        // GetProperty + Call0
        self.patterns.push(SuperInstructionPattern {
            opcodes: vec![Opcode::GetProperty, Opcode::Call0],
            super_opcode: SuperOpcode::GetPropertyCall0,
            bytes_consumed: 4, // GetProperty(3) + Call0(1)
        });

        // GetProperty + Call1
        self.patterns.push(SuperInstructionPattern {
            opcodes: vec![Opcode::GetProperty, Opcode::Call1],
            super_opcode: SuperOpcode::GetPropertyCall1,
            bytes_consumed: 4,
        });

        // LoadConst + Return
        self.patterns.push(SuperInstructionPattern {
            opcodes: vec![Opcode::LoadConst, Opcode::Return],
            super_opcode: SuperOpcode::ReturnConst,
            bytes_consumed: 4, // LoadConst(3) + Return(1)
        });

        // LoadLocal + Return
        self.patterns.push(SuperInstructionPattern {
            opcodes: vec![Opcode::GetLocal, Opcode::Return],
            super_opcode: SuperOpcode::ReturnLocal,
            bytes_consumed: 4,
        });
    }

    /// Find matching pattern at current position
    pub fn find_pattern(&self, code: &[u8], pos: usize) -> Option<&SuperInstructionPattern> {
        for pattern in &self.patterns {
            if self.matches_pattern(code, pos, pattern) {
                return Some(pattern);
            }
        }
        None
    }

    /// Check if pattern matches at position
    fn matches_pattern(&self, code: &[u8], pos: usize, pattern: &SuperInstructionPattern) -> bool {
        if pos + pattern.bytes_consumed > code.len() {
            return false;
        }

        let mut offset = pos;
        for opcode in &pattern.opcodes {
            if code.get(offset) != Some(&(*opcode as u8)) {
                return false;
            }
            // Skip operands based on opcode
            offset += self.opcode_size(*opcode);
        }

        true
    }

    /// Get size of opcode including operands
    fn opcode_size(&self, opcode: Opcode) -> usize {
        match opcode {
            // 3-byte opcodes (opcode + u16)
            Opcode::GetLocal | Opcode::SetLocal |
            Opcode::GetGlobal | Opcode::SetGlobal |
            Opcode::GetProperty | Opcode::SetProperty |
            Opcode::LoadConst | Opcode::GetUpvalue | Opcode::SetUpvalue => 3,
            
            // 3-byte opcodes (opcode + i16)
            Opcode::Jump | Opcode::JumpIfFalse | Opcode::JumpIfTrue => 3,
            
            // 2-byte opcodes (opcode + u8)
            Opcode::Call => 2,
            
            // 1-byte opcodes
            _ => 1,
        }
    }

    /// Get transformation statistics
    pub fn stats(&self) -> SuperInstructionStats {
        SuperInstructionStats {
            transformations: self.transformations,
            bytes_saved: self.bytes_saved,
        }
    }
}

/// Superinstruction statistics
#[derive(Debug, Clone, Default)]
pub struct SuperInstructionStats {
    pub transformations: u64,
    pub bytes_saved: u64,
}

/// Stack caching state
/// Tracks which values are in "registers" vs on the stack
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackCacheState {
    /// Stack cache is empty, TOS is in memory
    Empty,
    /// One value cached (in virtual register R0)
    One,
    /// Two values cached (in R0 and R1)
    Two,
}

impl Default for StackCacheState {
    fn default() -> Self {
        Self::Empty
    }
}

/// Stack cache manager
#[derive(Debug, Default)]
pub struct StackCache {
    state: StackCacheState,
    /// Cached top-of-stack value (when state >= One)
    r0: Option<CachedValue>,
    /// Second cached value (when state == Two)
    r1: Option<CachedValue>,
}

/// Cached stack value
#[derive(Debug, Clone, Copy)]
pub enum CachedValue {
    Integer(i64),
    Float(f64),
    Boolean(bool),
    /// Reference to object/array/function by ID
    Reference(u32),
}

impl StackCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a value onto the cache
    pub fn push(&mut self, value: CachedValue) -> bool {
        match self.state {
            StackCacheState::Empty => {
                self.r0 = Some(value);
                self.state = StackCacheState::One;
                true
            }
            StackCacheState::One => {
                self.r1 = self.r0.take();
                self.r0 = Some(value);
                self.state = StackCacheState::Two;
                true
            }
            StackCacheState::Two => {
                // Cache full, need to spill
                false
            }
        }
    }

    /// Pop a value from the cache
    pub fn pop(&mut self) -> Option<CachedValue> {
        match self.state {
            StackCacheState::Empty => None,
            StackCacheState::One => {
                self.state = StackCacheState::Empty;
                self.r0.take()
            }
            StackCacheState::Two => {
                self.state = StackCacheState::One;
                let result = self.r0.take();
                self.r0 = self.r1.take();
                result
            }
        }
    }

    /// Peek at top value without popping
    pub fn peek(&self) -> Option<&CachedValue> {
        self.r0.as_ref()
    }

    /// Get current cache state
    pub fn state(&self) -> StackCacheState {
        self.state
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.state == StackCacheState::Empty
    }

    /// Spill all cached values to memory
    pub fn spill(&mut self) -> Vec<CachedValue> {
        let mut values = Vec::new();
        while let Some(v) = self.pop() {
            values.push(v);
        }
        values.reverse();
        values
    }
}

/// Direct threading handler type
/// Each handler is a function that executes an opcode and returns the next IP
pub type DirectHandler = fn(
    code: &[u8],
    ip: usize,
    stack: &mut Vec<u64>,
    cache: &mut StackCache,
) -> usize;

/// Direct threading dispatch table
#[derive(Debug)]
pub struct DirectDispatch {
    /// Handler table indexed by opcode
    handlers: [Option<DirectHandler>; 256],
    /// Number of registered handlers
    handler_count: usize,
}

impl Default for DirectDispatch {
    fn default() -> Self {
        Self::new()
    }
}

impl DirectDispatch {
    pub fn new() -> Self {
        Self {
            handlers: [None; 256],
            handler_count: 0,
        }
    }

    /// Register a handler for an opcode
    pub fn register(&mut self, opcode: u8, handler: DirectHandler) {
        if self.handlers[opcode as usize].is_none() {
            self.handler_count += 1;
        }
        self.handlers[opcode as usize] = Some(handler);
    }

    /// Get handler for opcode
    pub fn get(&self, opcode: u8) -> Option<DirectHandler> {
        self.handlers[opcode as usize]
    }

    /// Dispatch to handler
    #[inline(always)]
    pub fn dispatch(
        &self,
        code: &[u8],
        ip: usize,
        stack: &mut Vec<u64>,
        cache: &mut StackCache,
    ) -> Option<usize> {
        let opcode = *code.get(ip)?;
        let handler = self.handlers[opcode as usize]?;
        Some(handler(code, ip, stack, cache))
    }

    /// Count of registered handlers
    pub fn handler_count(&self) -> usize {
        self.handler_count
    }
}

/// Dispatch statistics for profiling
#[derive(Debug, Clone, Default)]
pub struct DispatchStats {
    /// Count per opcode
    pub opcode_counts: HashMap<u8, u64>,
    /// Total dispatches
    pub total_dispatches: u64,
    /// Cache hits (TOS in register)
    pub cache_hits: u64,
    /// Cache misses (had to access memory)
    pub cache_misses: u64,
}

impl DispatchStats {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a dispatch
    pub fn record(&mut self, opcode: u8) {
        *self.opcode_counts.entry(opcode).or_insert(0) += 1;
        self.total_dispatches += 1;
    }

    /// Record cache hit
    pub fn cache_hit(&mut self) {
        self.cache_hits += 1;
    }

    /// Record cache miss
    pub fn cache_miss(&mut self) {
        self.cache_misses += 1;
    }

    /// Get cache hit rate
    pub fn cache_hit_rate(&self) -> f64 {
        let total = self.cache_hits + self.cache_misses;
        if total == 0 {
            0.0
        } else {
            self.cache_hits as f64 / total as f64
        }
    }

    /// Get top N most frequent opcodes
    pub fn top_opcodes(&self, n: usize) -> Vec<(u8, u64)> {
        let mut counts: Vec<_> = self.opcode_counts.iter()
            .map(|(&k, &v)| (k, v))
            .collect();
        counts.sort_by(|a, b| b.1.cmp(&a.1));
        counts.truncate(n);
        counts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_super_instruction_transformer() {
        let transformer = SuperInstructionTransformer::new();
        assert!(!transformer.patterns.is_empty());
    }

    #[test]
    fn test_stack_cache_push_pop() {
        let mut cache = StackCache::new();
        
        assert!(cache.is_empty());
        
        cache.push(CachedValue::Integer(42));
        assert_eq!(cache.state(), StackCacheState::One);
        
        cache.push(CachedValue::Integer(100));
        assert_eq!(cache.state(), StackCacheState::Two);
        
        // Cache full
        assert!(!cache.push(CachedValue::Integer(200)));
        
        // Pop in LIFO order
        match cache.pop() {
            Some(CachedValue::Integer(100)) => {}
            _ => panic!("Expected 100"),
        }
        
        match cache.pop() {
            Some(CachedValue::Integer(42)) => {}
            _ => panic!("Expected 42"),
        }
        
        assert!(cache.is_empty());
    }

    #[test]
    fn test_stack_cache_spill() {
        let mut cache = StackCache::new();
        cache.push(CachedValue::Integer(1));
        cache.push(CachedValue::Integer(2));
        
        let spilled = cache.spill();
        assert_eq!(spilled.len(), 2);
        assert!(cache.is_empty());
    }

    #[test]
    fn test_direct_dispatch() {
        let mut dispatch = DirectDispatch::new();
        
        // Dummy handler
        fn nop_handler(_: &[u8], ip: usize, _: &mut Vec<u64>, _: &mut StackCache) -> usize {
            ip + 1
        }
        
        dispatch.register(0, nop_handler);
        assert_eq!(dispatch.handler_count(), 1);
        assert!(dispatch.get(0).is_some());
        assert!(dispatch.get(1).is_none());
    }

    #[test]
    fn test_dispatch_stats() {
        let mut stats = DispatchStats::new();
        
        stats.record(10);
        stats.record(10);
        stats.record(20);
        
        assert_eq!(stats.total_dispatches, 3);
        assert_eq!(stats.opcode_counts.get(&10), Some(&2));
        
        let top = stats.top_opcodes(2);
        assert_eq!(top[0], (10, 2));
        assert_eq!(top[1], (20, 1));
    }

    #[test]
    fn test_super_opcode_enum() {
        // Ensure enum values are distinct
        assert_ne!(SuperOpcode::LoadLocalAdd as u8, SuperOpcode::LoadLocalSub as u8);
        assert_ne!(SuperOpcode::GetPropertyCall0 as u8, SuperOpcode::ReturnConst as u8);
    }
}
