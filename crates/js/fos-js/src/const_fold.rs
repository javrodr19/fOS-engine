//! Constant Folding and Dead Code Elimination (Phase 24.6)
//!
//! Pre-compute constant expressions. Remove unreachable code.
//! Tree shaking at runtime. Smaller active heap.

use std::collections::{HashMap, HashSet};

/// Value ID
pub type ValueId = u32;

/// Function ID
pub type FuncId = u32;

/// Constant value
#[derive(Debug, Clone, PartialEq)]
pub enum ConstValue {
    Undefined,
    Null,
    Boolean(bool),
    Number(f64),
    String(Box<str>),
    /// Not a constant
    Dynamic,
}

impl ConstValue {
    /// Check if definitely truthy
    pub fn is_truthy(&self) -> Option<bool> {
        match self {
            ConstValue::Undefined | ConstValue::Null => Some(false),
            ConstValue::Boolean(b) => Some(*b),
            ConstValue::Number(n) => Some(*n != 0.0 && !n.is_nan()),
            ConstValue::String(s) => Some(!s.is_empty()),
            ConstValue::Dynamic => None,
        }
    }
    
    /// Try to fold binary operation
    pub fn fold_binary(&self, op: BinaryOp, other: &ConstValue) -> ConstValue {
        use ConstValue::*;
        use BinaryOp::*;
        
        match (self, op, other) {
            // Number operations
            (Number(a), Add, Number(b)) => Number(a + b),
            (Number(a), Sub, Number(b)) => Number(a - b),
            (Number(a), Mul, Number(b)) => Number(a * b),
            (Number(a), Div, Number(b)) => Number(a / b),
            (Number(a), Mod, Number(b)) => Number(a % b),
            
            // Comparisons
            (Number(a), Lt, Number(b)) => Boolean(a < b),
            (Number(a), Le, Number(b)) => Boolean(a <= b),
            (Number(a), Gt, Number(b)) => Boolean(a > b),
            (Number(a), Ge, Number(b)) => Boolean(a >= b),
            (Number(a), Eq, Number(b)) => Boolean((a - b).abs() < f64::EPSILON),
            
            // String concat
            (String(a), Add, String(b)) => String(format!("{}{}", a, b).into()),
            
            // Boolean ops
            (Boolean(a), And, Boolean(b)) => Boolean(*a && *b),
            (Boolean(a), Or, Boolean(b)) => Boolean(*a || *b),
            
            _ => Dynamic,
        }
    }
    
    /// Try to fold unary operation
    pub fn fold_unary(&self, op: UnaryOp) -> ConstValue {
        use ConstValue::*;
        use UnaryOp::*;
        
        match (op, self) {
            (Neg, Number(n)) => Number(-n),
            (Not, Boolean(b)) => Boolean(!b),
            (Not, _) => {
                if let Some(truthy) = self.is_truthy() {
                    Boolean(!truthy)
                } else {
                    Dynamic
                }
            }
            (Typeof, Undefined) => String("undefined".into()),
            (Typeof, Null) => String("object".into()),
            (Typeof, Boolean(_)) => String("boolean".into()),
            (Typeof, Number(_)) => String("number".into()),
            (Typeof, String(_)) => String("string".into()),
            _ => Dynamic,
        }
    }
}

/// Binary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add, Sub, Mul, Div, Mod,
    Lt, Le, Gt, Ge, Eq, Ne,
    And, Or,
    BitAnd, BitOr, BitXor,
    Shl, Shr, Ushr,
}

/// Unary operators  
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg, Not, BitNot, Typeof, Void,
}

/// Constant folder
#[derive(Debug)]
pub struct ConstantFolder {
    /// Known constant values
    constants: HashMap<ValueId, ConstValue>,
    /// Statistics
    stats: FoldingStats,
}

/// Folding statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct FoldingStats {
    pub expressions_analyzed: u64,
    pub constants_folded: u64,
    pub branches_eliminated: u64,
    pub bytes_saved: u64,
}

impl FoldingStats {
    pub fn fold_ratio(&self) -> f64 {
        if self.expressions_analyzed == 0 {
            0.0
        } else {
            self.constants_folded as f64 / self.expressions_analyzed as f64
        }
    }
}

impl Default for ConstantFolder {
    fn default() -> Self {
        Self::new()
    }
}

impl ConstantFolder {
    pub fn new() -> Self {
        Self {
            constants: HashMap::new(),
            stats: FoldingStats::default(),
        }
    }
    
    /// Record a constant
    pub fn record_constant(&mut self, id: ValueId, value: ConstValue) {
        self.constants.insert(id, value);
    }
    
    /// Get constant value
    pub fn get_constant(&self, id: ValueId) -> Option<&ConstValue> {
        self.constants.get(&id)
    }
    
    /// Try to fold binary expression
    pub fn fold_binary(&mut self, left: ValueId, op: BinaryOp, right: ValueId) -> Option<ConstValue> {
        self.stats.expressions_analyzed += 1;
        
        let left_val = self.constants.get(&left)?;
        let right_val = self.constants.get(&right)?;
        
        let result = left_val.fold_binary(op, right_val);
        
        if result != ConstValue::Dynamic {
            self.stats.constants_folded += 1;
            Some(result)
        } else {
            None
        }
    }
    
    /// Try to fold unary expression
    pub fn fold_unary(&mut self, op: UnaryOp, operand: ValueId) -> Option<ConstValue> {
        self.stats.expressions_analyzed += 1;
        
        let operand_val = self.constants.get(&operand)?;
        let result = operand_val.fold_unary(op);
        
        if result != ConstValue::Dynamic {
            self.stats.constants_folded += 1;
            Some(result)
        } else {
            None
        }
    }
    
    /// Check if branch can be eliminated
    pub fn can_eliminate_branch(&mut self, condition: ValueId) -> Option<bool> {
        let val = self.constants.get(&condition)?;
        
        if let Some(truthy) = val.is_truthy() {
            self.stats.branches_eliminated += 1;
            Some(truthy)
        } else {
            None
        }
    }
    
    /// Get statistics
    pub fn stats(&self) -> &FoldingStats {
        &self.stats
    }
}

/// Dead code eliminator
#[derive(Debug)]
pub struct DeadCodeEliminator {
    /// Reachable functions
    reachable: HashSet<FuncId>,
    /// Call graph: caller -> callees
    call_graph: HashMap<FuncId, Vec<FuncId>>,
    /// Entry points
    entry_points: Vec<FuncId>,
    /// Statistics
    stats: DceStats,
}

/// DCE statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct DceStats {
    pub total_functions: u64,
    pub reachable_functions: u64,
    pub dead_functions: u64,
    pub bytes_eliminated: u64,
}

impl DceStats {
    pub fn elimination_ratio(&self) -> f64 {
        if self.total_functions == 0 {
            0.0
        } else {
            self.dead_functions as f64 / self.total_functions as f64
        }
    }
}

impl Default for DeadCodeEliminator {
    fn default() -> Self {
        Self::new()
    }
}

impl DeadCodeEliminator {
    pub fn new() -> Self {
        Self {
            reachable: HashSet::new(),
            call_graph: HashMap::new(),
            entry_points: Vec::new(),
            stats: DceStats::default(),
        }
    }
    
    /// Add a function to the call graph
    pub fn add_function(&mut self, func: FuncId, callees: Vec<FuncId>) {
        self.stats.total_functions += 1;
        self.call_graph.insert(func, callees);
    }
    
    /// Mark a function as an entry point
    pub fn add_entry_point(&mut self, func: FuncId) {
        self.entry_points.push(func);
    }
    
    /// Analyze reachability
    pub fn analyze(&mut self) {
        // Mark all reachable from entry points
        let mut worklist: Vec<FuncId> = self.entry_points.clone();
        
        while let Some(func) = worklist.pop() {
            if self.reachable.contains(&func) {
                continue;
            }
            
            self.reachable.insert(func);
            
            if let Some(callees) = self.call_graph.get(&func) {
                for &callee in callees {
                    if !self.reachable.contains(&callee) {
                        worklist.push(callee);
                    }
                }
            }
        }
        
        self.stats.reachable_functions = self.reachable.len() as u64;
        self.stats.dead_functions = self.stats.total_functions - self.stats.reachable_functions;
    }
    
    /// Check if function is reachable
    pub fn is_reachable(&self, func: FuncId) -> bool {
        self.reachable.contains(&func)
    }
    
    /// Get dead functions
    pub fn dead_functions(&self) -> Vec<FuncId> {
        self.call_graph.keys()
            .filter(|f| !self.reachable.contains(f))
            .copied()
            .collect()
    }
    
    /// Get statistics
    pub fn stats(&self) -> &DceStats {
        &self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_constant_folding() {
        let mut folder = ConstantFolder::new();
        
        folder.record_constant(1, ConstValue::Number(2.0));
        folder.record_constant(2, ConstValue::Number(3.0));
        
        let result = folder.fold_binary(1, BinaryOp::Add, 2);
        assert_eq!(result, Some(ConstValue::Number(5.0)));
    }
    
    #[test]
    fn test_string_concat() {
        let mut folder = ConstantFolder::new();
        
        folder.record_constant(1, ConstValue::String("hello".into()));
        folder.record_constant(2, ConstValue::String(" world".into()));
        
        let result = folder.fold_binary(1, BinaryOp::Add, 2);
        assert_eq!(result, Some(ConstValue::String("hello world".into())));
    }
    
    #[test]
    fn test_branch_elimination() {
        let mut folder = ConstantFolder::new();
        
        folder.record_constant(1, ConstValue::Boolean(true));
        
        let can_eliminate = folder.can_eliminate_branch(1);
        assert_eq!(can_eliminate, Some(true));
    }
    
    #[test]
    fn test_dead_code_elimination() {
        let mut dce = DeadCodeEliminator::new();
        
        dce.add_function(0, vec![1, 2]); // main calls f1, f2
        dce.add_function(1, vec![]);     // f1
        dce.add_function(2, vec![]);     // f2
        dce.add_function(3, vec![]);     // f3 (dead)
        dce.add_function(4, vec![]);     // f4 (dead)
        
        dce.add_entry_point(0);
        dce.analyze();
        
        assert!(dce.is_reachable(0));
        assert!(dce.is_reachable(1));
        assert!(dce.is_reachable(2));
        assert!(!dce.is_reachable(3));
        assert!(!dce.is_reachable(4));
        
        assert_eq!(dce.stats().dead_functions, 2);
    }
}
