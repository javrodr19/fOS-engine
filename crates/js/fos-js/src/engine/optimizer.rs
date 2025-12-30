//! Bytecode Optimizer
//!
//! Peephole optimization, dead store elimination, and
//! common subexpression elimination for bytecode.

use super::bytecode::{Bytecode, Opcode};
use std::collections::HashMap;

/// Bytecode optimizer
#[derive(Debug, Default)]
pub struct BytecodeOptimizer {
    stats: OptimizerStats,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct OptimizerStats {
    pub dead_stores_removed: u32,
    pub redundant_loads_removed: u32,
    pub peephole_optimizations: u32,
    pub instructions_removed: u32,
}

impl BytecodeOptimizer {
    pub fn new() -> Self { Self::default() }
    
    /// Optimize bytecode
    pub fn optimize(&mut self, bytecode: &mut Bytecode) {
        self.peephole_optimize(bytecode);
        self.remove_dead_stores(bytecode);
        self.eliminate_redundant_loads(bytecode);
    }
    
    /// Peephole optimization - combine adjacent instructions
    fn peephole_optimize(&mut self, bytecode: &mut Bytecode) {
        let code = &mut bytecode.code;
        let mut i = 0;
        
        while i + 1 < code.len() {
            let optimized = match (code.get(i), code.get(i + 1)) {
                // LoadZero + Add -> just keep the value (x + 0 = x)
                (Some(&op), Some(&add)) 
                    if op == Opcode::LoadZero as u8 && add == Opcode::Add as u8 => {
                    code.remove(i);  // Remove LoadZero
                    code.remove(i);  // Remove Add
                    self.stats.peephole_optimizations += 1;
                    self.stats.instructions_removed += 2;
                    true
                }
                // LoadOne + Mul -> just keep the value (x * 1 = x)
                (Some(&op), Some(&mul))
                    if op == Opcode::LoadOne as u8 && mul == Opcode::Mul as u8 => {
                    code.remove(i);
                    code.remove(i);
                    self.stats.peephole_optimizations += 1;
                    self.stats.instructions_removed += 2;
                    true
                }
                // LoadZero + Mul -> LoadZero (x * 0 = 0)
                (Some(&op), Some(&mul))
                    if op == Opcode::LoadZero as u8 && mul == Opcode::Mul as u8 => {
                    code.remove(i + 1);  // Remove Mul, keep LoadZero
                    self.stats.peephole_optimizations += 1;
                    self.stats.instructions_removed += 1;
                    true
                }
                // Push + Pop -> nothing
                (Some(&push), Some(&pop))
                    if pop == Opcode::Pop as u8 && is_push_op(push) => {
                    code.remove(i);
                    code.remove(i);
                    self.stats.peephole_optimizations += 1;
                    self.stats.instructions_removed += 2;
                    true
                }
                // Neg + Neg -> nothing (double negation)
                (Some(&neg1), Some(&neg2))
                    if neg1 == Opcode::Neg as u8 && neg2 == Opcode::Neg as u8 => {
                    code.remove(i);
                    code.remove(i);
                    self.stats.peephole_optimizations += 1;
                    self.stats.instructions_removed += 2;
                    true
                }
                // Not + Not -> nothing (double negation)
                (Some(&not1), Some(&not2))
                    if not1 == Opcode::Not as u8 && not2 == Opcode::Not as u8 => {
                    code.remove(i);
                    code.remove(i);
                    self.stats.peephole_optimizations += 1;
                    self.stats.instructions_removed += 2;
                    true
                }
                _ => false,
            };
            
            if !optimized {
                i += 1;
            }
        }
    }
    
    /// Remove dead stores (SetLocal followed by another SetLocal to same slot)
    fn remove_dead_stores(&mut self, bytecode: &mut Bytecode) {
        // Simplified: track last SetLocal for each slot
        // If same slot is set again before being read, first is dead
        let code = &mut bytecode.code;
        let mut i = 0;
        
        while i + 4 < code.len() {
            // Look for SetLocal patterns
            if code[i] == Opcode::SetLocal as u8 {
                let slot1 = u16::from_le_bytes([code[i + 1], code[i + 2]]);
                
                // Check if next instruction is also SetLocal to same slot
                if i + 5 < code.len() && code[i + 3] == Opcode::SetLocal as u8 {
                    let slot2 = u16::from_le_bytes([code[i + 4], code[i + 5]]);
                    if slot1 == slot2 {
                        // Remove first SetLocal (dead store)
                        for _ in 0..3 { code.remove(i); }
                        self.stats.dead_stores_removed += 1;
                        self.stats.instructions_removed += 3;
                        continue;
                    }
                }
            }
            i += 1;
        }
    }
    
    /// Eliminate redundant loads (GetLocal immediately after SetLocal same slot)
    fn eliminate_redundant_loads(&mut self, bytecode: &mut Bytecode) {
        let code = &mut bytecode.code;
        let mut i = 0;
        
        while i + 5 < code.len() {
            // SetLocal slot, GetLocal slot -> SetLocal slot, Dup
            if code[i] == Opcode::SetLocal as u8 && code[i + 3] == Opcode::GetLocal as u8 {
                let slot1 = u16::from_le_bytes([code[i + 1], code[i + 2]]);
                let slot2 = u16::from_le_bytes([code[i + 4], code[i + 5]]);
                
                if slot1 == slot2 {
                    // Replace GetLocal with Dup (value is already on stack)
                    code[i + 3] = Opcode::Dup as u8;
                    code.remove(i + 4);
                    code.remove(i + 4);
                    self.stats.redundant_loads_removed += 1;
                    self.stats.instructions_removed += 2;
                    continue;
                }
            }
            i += 1;
        }
    }
    
    pub fn stats(&self) -> OptimizerStats { self.stats }
}

/// Check if opcode pushes a value onto stack
fn is_push_op(op: u8) -> bool {
    matches!(op, 
        x if x == Opcode::LoadConst as u8 ||
             x == Opcode::LoadNull as u8 ||
             x == Opcode::LoadUndefined as u8 ||
             x == Opcode::LoadTrue as u8 ||
             x == Opcode::LoadFalse as u8 ||
             x == Opcode::LoadZero as u8 ||
             x == Opcode::LoadOne as u8 ||
             x == Opcode::GetLocal as u8 ||
             x == Opcode::GetGlobal as u8
    )
}

/// Optimize bytecode (convenience function)
pub fn optimize_bytecode(bytecode: &mut Bytecode) -> OptimizerStats {
    let mut optimizer = BytecodeOptimizer::new();
    optimizer.optimize(bytecode);
    optimizer.stats()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_double_negation() {
        let mut bytecode = Bytecode::new();
        bytecode.emit(Opcode::LoadOne);
        bytecode.emit(Opcode::Neg);
        bytecode.emit(Opcode::Neg);
        
        let stats = optimize_bytecode(&mut bytecode);
        assert_eq!(stats.peephole_optimizations, 1);
        assert_eq!(bytecode.code.len(), 1); // Just LoadOne
    }
    
    #[test]
    fn test_multiply_by_one() {
        let mut bytecode = Bytecode::new();
        bytecode.emit(Opcode::LoadConst);
        bytecode.emit_u16(0);
        bytecode.emit(Opcode::LoadOne);
        bytecode.emit(Opcode::Mul);
        
        let stats = optimize_bytecode(&mut bytecode);
        assert!(stats.peephole_optimizations >= 1);
    }
}
