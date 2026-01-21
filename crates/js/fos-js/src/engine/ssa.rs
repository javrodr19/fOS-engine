//! SSA (Static Single Assignment) Intermediate Representation
//!
//! SSA form for the optimizing JIT compiler. Converts bytecode to SSA
//! for advanced optimization passes like:
//! - Type specialization
//! - Dead code elimination
//! - Common subexpression elimination
//! - Loop invariant code motion
//!
//! In SSA form, each variable is assigned exactly once, making
//! data flow analysis and optimization much simpler.

use std::collections::{HashMap, HashSet, VecDeque};
use super::type_profiler::ObservedType;

/// SSA Value ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SsaValue(u32);

impl SsaValue {
    pub fn new(id: u32) -> Self { Self(id) }
    pub fn id(&self) -> u32 { self.0 }
}

/// Basic Block ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(u32);

impl BlockId {
    pub fn new(id: u32) -> Self { Self(id) }
    pub fn id(&self) -> u32 { self.0 }
}

/// SSA instruction kind
#[derive(Debug, Clone)]
pub enum SsaOp {
    // ===== Constants =====
    /// Load constant number
    ConstNum(f64),
    /// Load constant string
    ConstStr(Box<str>),
    /// Load undefined
    Undefined,
    /// Load null
    Null,
    /// Load boolean
    Bool(bool),
    
    // ===== Parameters =====
    /// Function parameter
    Param(u8),
    
    // ===== Arithmetic =====
    /// Add two values
    Add(SsaValue, SsaValue),
    /// Subtract
    Sub(SsaValue, SsaValue),
    /// Multiply
    Mul(SsaValue, SsaValue),
    /// Divide
    Div(SsaValue, SsaValue),
    /// Modulo
    Mod(SsaValue, SsaValue),
    /// Negate
    Neg(SsaValue),
    
    // ===== Comparison =====
    /// Less than
    Lt(SsaValue, SsaValue),
    /// Less than or equal
    Le(SsaValue, SsaValue),
    /// Greater than
    Gt(SsaValue, SsaValue),
    /// Greater than or equal
    Ge(SsaValue, SsaValue),
    /// Equal (==)
    Eq(SsaValue, SsaValue),
    /// Not equal (!=)
    Ne(SsaValue, SsaValue),
    /// Strict equal (===)
    StrictEq(SsaValue, SsaValue),
    /// Strict not equal (!==)
    StrictNe(SsaValue, SsaValue),
    
    // ===== Logical =====
    /// Logical not
    Not(SsaValue),
    
    // ===== Bitwise =====
    /// Bitwise AND
    BitAnd(SsaValue, SsaValue),
    /// Bitwise OR
    BitOr(SsaValue, SsaValue),
    /// Bitwise XOR
    BitXor(SsaValue, SsaValue),
    /// Bitwise NOT
    BitNot(SsaValue),
    /// Left shift
    Shl(SsaValue, SsaValue),
    /// Right shift (signed)
    Shr(SsaValue, SsaValue),
    /// Right shift (unsigned)
    UShr(SsaValue, SsaValue),
    
    // ===== Property Access =====
    /// Get property by name
    GetProperty(SsaValue, Box<str>),
    /// Set property by name
    SetProperty(SsaValue, Box<str>, SsaValue),
    /// Get element by index
    GetElement(SsaValue, SsaValue),
    /// Set element by index
    SetElement(SsaValue, SsaValue, SsaValue),
    
    // ===== Objects/Arrays =====
    /// Create new object
    NewObject,
    /// Create new array
    NewArray(Vec<SsaValue>),
    
    // ===== Functions =====
    /// Call function
    Call(SsaValue, Vec<SsaValue>),
    /// Call method
    MethodCall(SsaValue, Box<str>, Vec<SsaValue>),
    
    // ===== Control Flow =====
    /// Phi node (merge values from different predecessors)
    Phi(Vec<(BlockId, SsaValue)>),
    
    // ===== Type Operations =====
    /// Type guard (deoptimize if type doesn't match)
    TypeGuard(SsaValue, ObservedType),
    /// Typeof
    Typeof(SsaValue),
    /// Instanceof
    Instanceof(SsaValue, SsaValue),
    
    // ===== Special =====
    /// Load this
    LoadThis,
    /// Load captured variable (closure)
    LoadCaptured(u16),
    /// Store to captured variable
    StoreCaptured(u16, SsaValue),
}

/// SSA instruction with result and source info
#[derive(Debug, Clone)]
pub struct SsaInstr {
    /// Result value (None for void operations)
    pub result: Option<SsaValue>,
    /// Operation
    pub op: SsaOp,
    /// Type hint from profiling
    pub type_hint: Option<ObservedType>,
    /// Original bytecode offset (for debugging/deopt)
    pub bytecode_offset: u32,
}

impl SsaInstr {
    pub fn new(result: Option<SsaValue>, op: SsaOp) -> Self {
        Self {
            result,
            op,
            type_hint: None,
            bytecode_offset: 0,
        }
    }

    pub fn with_type_hint(mut self, hint: ObservedType) -> Self {
        self.type_hint = Some(hint);
        self
    }
}

/// Block terminator instruction
#[derive(Debug, Clone)]
pub enum BlockTerminator {
    /// Unconditional jump
    Jump(BlockId),
    /// Conditional branch
    Branch {
        cond: SsaValue,
        if_true: BlockId,
        if_false: BlockId,
    },
    /// Return value
    Return(Option<SsaValue>),
    /// Unreachable (for dead code)
    Unreachable,
}

/// Basic block in SSA form
#[derive(Debug, Clone)]
pub struct BasicBlock {
    /// Block ID
    pub id: BlockId,
    /// Predecessor blocks
    pub predecessors: Vec<BlockId>,
    /// Successor blocks
    pub successors: Vec<BlockId>,
    /// Phi nodes at block entry
    pub phis: Vec<SsaInstr>,
    /// Instructions in the block
    pub instructions: Vec<SsaInstr>,
    /// Block terminator
    pub terminator: BlockTerminator,
    /// Whether block is a loop header
    pub is_loop_header: bool,
}

impl BasicBlock {
    pub fn new(id: BlockId) -> Self {
        Self {
            id,
            predecessors: Vec::new(),
            successors: Vec::new(),
            phis: Vec::new(),
            instructions: Vec::new(),
            terminator: BlockTerminator::Unreachable,
            is_loop_header: false,
        }
    }
}

/// SSA function representation
#[derive(Debug)]
pub struct SsaFunction {
    /// Function name
    pub name: Option<Box<str>>,
    /// Parameter count
    pub param_count: u8,
    /// Entry block
    pub entry_block: BlockId,
    /// All basic blocks
    pub blocks: HashMap<BlockId, BasicBlock>,
    /// Value counter for SSA names
    next_value: u32,
    /// Block counter
    next_block: u32,
}

impl SsaFunction {
    pub fn new(name: Option<Box<str>>, param_count: u8) -> Self {
        let mut func = Self {
            name,
            param_count,
            entry_block: BlockId::new(0),
            blocks: HashMap::new(),
            next_value: 0,
            next_block: 0,
        };
        
        // Create entry block
        let entry = func.new_block();
        func.entry_block = entry;
        
        func
    }

    /// Create a new SSA value
    pub fn new_value(&mut self) -> SsaValue {
        let v = SsaValue::new(self.next_value);
        self.next_value += 1;
        v
    }

    /// Create a new basic block
    pub fn new_block(&mut self) -> BlockId {
        let id = BlockId::new(self.next_block);
        self.next_block += 1;
        self.blocks.insert(id, BasicBlock::new(id));
        id
    }

    /// Get block by ID
    pub fn get_block(&self, id: BlockId) -> Option<&BasicBlock> {
        self.blocks.get(&id)
    }

    /// Get mutable block by ID
    pub fn get_block_mut(&mut self, id: BlockId) -> Option<&mut BasicBlock> {
        self.blocks.get_mut(&id)
    }

    /// Add instruction to block
    pub fn add_instr(&mut self, block_id: BlockId, instr: SsaInstr) {
        if let Some(block) = self.blocks.get_mut(&block_id) {
            block.instructions.push(instr);
        }
    }

    /// Set block terminator
    pub fn set_terminator(&mut self, block_id: BlockId, term: BlockTerminator) {
        if let Some(block) = self.blocks.get_mut(&block_id) {
            // Update successor info
            block.successors.clear();
            match &term {
                BlockTerminator::Jump(target) => {
                    block.successors.push(*target);
                }
                BlockTerminator::Branch { if_true, if_false, .. } => {
                    block.successors.push(*if_true);
                    block.successors.push(*if_false);
                }
                _ => {}
            }
            block.terminator = term;
        }
    }

    /// Link blocks (update predecessor info)
    pub fn link_blocks(&mut self) {
        let mut preds: HashMap<BlockId, Vec<BlockId>> = HashMap::new();
        
        for (&block_id, block) in &self.blocks {
            for &succ in &block.successors {
                preds.entry(succ).or_default().push(block_id);
            }
        }
        
        for (block_id, pred_list) in preds {
            if let Some(block) = self.blocks.get_mut(&block_id) {
                block.predecessors = pred_list;
            }
        }
    }

    /// Iterate blocks in reverse post-order (good for forward analysis)
    pub fn rpo_order(&self) -> Vec<BlockId> {
        let mut visited = HashSet::new();
        let mut order = Vec::new();
        
        fn visit(
            func: &SsaFunction,
            block_id: BlockId,
            visited: &mut HashSet<BlockId>,
            order: &mut Vec<BlockId>,
        ) {
            if visited.contains(&block_id) {
                return;
            }
            visited.insert(block_id);
            
            if let Some(block) = func.get_block(block_id) {
                for &succ in &block.successors {
                    visit(func, succ, visited, order);
                }
            }
            
            order.push(block_id);
        }
        
        visit(self, self.entry_block, &mut visited, &mut order);
        order.reverse();
        order
    }

    /// Count total instructions
    pub fn instruction_count(&self) -> usize {
        self.blocks.values()
            .map(|b| b.instructions.len() + b.phis.len())
            .sum()
    }
}

/// SSA builder - constructs SSA form from bytecode
#[derive(Debug)]
pub struct SsaBuilder {
    /// Current function being built
    func: SsaFunction,
    /// Current block
    current_block: BlockId,
    /// Variable definitions per block (for phi insertion)
    var_defs: HashMap<BlockId, HashMap<u16, SsaValue>>,
    /// Sealed blocks (all predecessors known)
    sealed_blocks: HashSet<BlockId>,
    /// Incomplete phis to resolve
    incomplete_phis: HashMap<BlockId, HashMap<u16, SsaValue>>,
}

impl SsaBuilder {
    pub fn new(name: Option<Box<str>>, param_count: u8) -> Self {
        let func = SsaFunction::new(name, param_count);
        let entry = func.entry_block;
        
        Self {
            func,
            current_block: entry,
            var_defs: HashMap::new(),
            sealed_blocks: HashSet::new(),
            incomplete_phis: HashMap::new(),
        }
    }

    /// Emit constant number
    pub fn emit_const_num(&mut self, value: f64) -> SsaValue {
        let result = self.func.new_value();
        let instr = SsaInstr::new(Some(result), SsaOp::ConstNum(value));
        self.func.add_instr(self.current_block, instr);
        result
    }

    /// Emit constant string
    pub fn emit_const_str(&mut self, value: &str) -> SsaValue {
        let result = self.func.new_value();
        let instr = SsaInstr::new(Some(result), SsaOp::ConstStr(value.into()));
        self.func.add_instr(self.current_block, instr);
        result
    }

    /// Emit binary operation
    pub fn emit_binop(&mut self, op: SsaOp) -> SsaValue {
        let result = self.func.new_value();
        let instr = SsaInstr::new(Some(result), op);
        self.func.add_instr(self.current_block, instr);
        result
    }

    /// Emit add
    pub fn emit_add(&mut self, left: SsaValue, right: SsaValue) -> SsaValue {
        self.emit_binop(SsaOp::Add(left, right))
    }

    /// Emit sub
    pub fn emit_sub(&mut self, left: SsaValue, right: SsaValue) -> SsaValue {
        self.emit_binop(SsaOp::Sub(left, right))
    }

    /// Emit mul
    pub fn emit_mul(&mut self, left: SsaValue, right: SsaValue) -> SsaValue {
        self.emit_binop(SsaOp::Mul(left, right))
    }

    /// Emit comparison
    pub fn emit_lt(&mut self, left: SsaValue, right: SsaValue) -> SsaValue {
        self.emit_binop(SsaOp::Lt(left, right))
    }

    /// Emit type guard
    pub fn emit_type_guard(&mut self, value: SsaValue, expected: ObservedType) -> SsaValue {
        let result = self.func.new_value();
        let instr = SsaInstr::new(Some(result), SsaOp::TypeGuard(value, expected))
            .with_type_hint(expected);
        self.func.add_instr(self.current_block, instr);
        result
    }

    /// Create new block and return its ID
    pub fn new_block(&mut self) -> BlockId {
        self.func.new_block()
    }

    /// Switch to block
    pub fn switch_block(&mut self, block_id: BlockId) {
        self.current_block = block_id;
    }

    /// Emit jump
    pub fn emit_jump(&mut self, target: BlockId) {
        self.func.set_terminator(self.current_block, BlockTerminator::Jump(target));
    }

    /// Emit conditional branch
    pub fn emit_branch(&mut self, cond: SsaValue, if_true: BlockId, if_false: BlockId) {
        self.func.set_terminator(self.current_block, BlockTerminator::Branch {
            cond,
            if_true,
            if_false,
        });
    }

    /// Emit return
    pub fn emit_return(&mut self, value: Option<SsaValue>) {
        self.func.set_terminator(self.current_block, BlockTerminator::Return(value));
    }

    /// Define variable in current block
    pub fn define_var(&mut self, var: u16, value: SsaValue) {
        self.var_defs
            .entry(self.current_block)
            .or_default()
            .insert(var, value);
    }

    /// Finalize and return SSA function
    pub fn finish(mut self) -> SsaFunction {
        self.func.link_blocks();
        self.func
    }
}

/// Dead code elimination pass
pub fn eliminate_dead_code(func: &mut SsaFunction) -> u32 {
    let mut used: HashSet<SsaValue> = HashSet::new();
    let mut eliminated = 0u32;
    
    // Mark phase: find all used values
    for block in func.blocks.values() {
        // Terminators use values
        match &block.terminator {
            BlockTerminator::Branch { cond, .. } => {
                used.insert(*cond);
            }
            BlockTerminator::Return(Some(v)) => {
                used.insert(*v);
            }
            _ => {}
        }
        
        // Instructions use values
        for instr in &block.instructions {
            mark_uses(&instr.op, &mut used);
        }
    }
    
    // Sweep phase: remove unused instructions
    for block in func.blocks.values_mut() {
        let original_len = block.instructions.len();
        block.instructions.retain(|instr| {
            match instr.result {
                Some(v) => used.contains(&v),
                None => true, // Keep side-effectful instructions
            }
        });
        eliminated += (original_len - block.instructions.len()) as u32;
    }
    
    eliminated
}

/// Mark values used by an operation
fn mark_uses(op: &SsaOp, used: &mut HashSet<SsaValue>) {
    match op {
        SsaOp::Add(a, b) | SsaOp::Sub(a, b) | SsaOp::Mul(a, b) |
        SsaOp::Div(a, b) | SsaOp::Mod(a, b) |
        SsaOp::Lt(a, b) | SsaOp::Le(a, b) | SsaOp::Gt(a, b) | SsaOp::Ge(a, b) |
        SsaOp::Eq(a, b) | SsaOp::Ne(a, b) | SsaOp::StrictEq(a, b) | SsaOp::StrictNe(a, b) |
        SsaOp::BitAnd(a, b) | SsaOp::BitOr(a, b) | SsaOp::BitXor(a, b) |
        SsaOp::Shl(a, b) | SsaOp::Shr(a, b) | SsaOp::UShr(a, b) |
        SsaOp::GetElement(a, b) | SsaOp::Instanceof(a, b) => {
            used.insert(*a);
            used.insert(*b);
        }
        SsaOp::Neg(a) | SsaOp::Not(a) | SsaOp::BitNot(a) | SsaOp::Typeof(a) => {
            used.insert(*a);
        }
        SsaOp::GetProperty(obj, _) => {
            used.insert(*obj);
        }
        SsaOp::SetProperty(obj, _, val) => {
            used.insert(*obj);
            used.insert(*val);
        }
        SsaOp::SetElement(arr, idx, val) => {
            used.insert(*arr);
            used.insert(*idx);
            used.insert(*val);
        }
        SsaOp::Call(func, args) => {
            used.insert(*func);
            for arg in args {
                used.insert(*arg);
            }
        }
        SsaOp::MethodCall(obj, _, args) => {
            used.insert(*obj);
            for arg in args {
                used.insert(*arg);
            }
        }
        SsaOp::NewArray(elements) => {
            for e in elements {
                used.insert(*e);
            }
        }
        SsaOp::Phi(entries) => {
            for (_, v) in entries {
                used.insert(*v);
            }
        }
        SsaOp::TypeGuard(v, _) | SsaOp::StoreCaptured(_, v) => {
            used.insert(*v);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ssa_builder_basic() {
        let mut builder = SsaBuilder::new(Some("test".into()), 0);
        
        let a = builder.emit_const_num(10.0);
        let b = builder.emit_const_num(20.0);
        let sum = builder.emit_add(a, b);
        builder.emit_return(Some(sum));
        
        let func = builder.finish();
        
        assert_eq!(func.blocks.len(), 1);
        assert!(func.get_block(func.entry_block).unwrap().instructions.len() >= 3);
    }

    #[test]
    fn test_ssa_branching() {
        let mut builder = SsaBuilder::new(Some("branch_test".into()), 0);
        
        let cond = builder.emit_const_num(1.0);
        
        let true_block = builder.new_block();
        let false_block = builder.new_block();
        let merge_block = builder.new_block();
        
        builder.emit_branch(cond, true_block, false_block);
        
        builder.switch_block(true_block);
        let true_val = builder.emit_const_num(100.0);
        builder.emit_jump(merge_block);
        
        builder.switch_block(false_block);
        let false_val = builder.emit_const_num(200.0);
        builder.emit_jump(merge_block);
        
        builder.switch_block(merge_block);
        builder.emit_return(Some(true_val));
        
        let func = builder.finish();
        
        assert_eq!(func.blocks.len(), 4);
    }

    #[test]
    fn test_dead_code_elimination() {
        let mut builder = SsaBuilder::new(Some("dce_test".into()), 0);
        
        // Create dead value
        let _dead = builder.emit_const_num(999.0);
        
        // Create used value
        let used = builder.emit_const_num(42.0);
        builder.emit_return(Some(used));
        
        let mut func = builder.finish();
        let before = func.instruction_count();
        
        let eliminated = eliminate_dead_code(&mut func);
        let after = func.instruction_count();
        
        assert!(eliminated > 0);
        assert!(after < before);
    }

    #[test]
    fn test_rpo_order() {
        let mut builder = SsaBuilder::new(None, 0);
        
        let block_a = builder.new_block();
        let block_b = builder.new_block();
        
        builder.emit_jump(block_a);
        
        builder.switch_block(block_a);
        builder.emit_jump(block_b);
        
        builder.switch_block(block_b);
        builder.emit_return(None);
        
        let func = builder.finish();
        let order = func.rpo_order();
        
        // Entry should come first
        assert_eq!(order[0], func.entry_block);
        assert_eq!(order.len(), 3);
    }
}
