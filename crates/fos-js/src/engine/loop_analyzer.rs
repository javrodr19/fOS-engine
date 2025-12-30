//! Loop Analyzer
//!
//! Identifies and optimizes loop patterns in bytecode.
//! Supports loop peeling, unrolling hints, and trip count estimation.

use super::bytecode::{Bytecode, Opcode};

/// Loop information
#[derive(Debug, Clone)]
pub struct LoopInfo {
    pub start_offset: usize,
    pub end_offset: usize,
    pub back_edge: usize,      // Jump back instruction
    pub exit_jumps: Vec<usize>, // Break/exit jumps
    pub is_counted: bool,       // Has known trip count
    pub estimated_trips: Option<u32>,
}

/// Loop analyzer
#[derive(Debug, Default)]
pub struct LoopAnalyzer {
    loops: Vec<LoopInfo>,
}

impl LoopAnalyzer {
    pub fn new() -> Self { Self::default() }
    
    /// Analyze bytecode for loops
    pub fn analyze(&mut self, bytecode: &Bytecode) {
        self.loops.clear();
        let code = &bytecode.code;
        let mut i = 0;
        
        while i < code.len() {
            // Look for backward jumps (loop back edges)
            if code[i] == Opcode::Jump as u8 && i + 2 < code.len() {
                let offset = i16::from_le_bytes([code[i + 1], code[i + 2]]);
                if offset < 0 {
                    // This is a backward jump - found a loop
                    let loop_start = (i as i32 + 3 + offset as i32) as usize;
                    self.loops.push(LoopInfo {
                        start_offset: loop_start,
                        end_offset: i + 3,
                        back_edge: i,
                        exit_jumps: Vec::new(),
                        is_counted: false,
                        estimated_trips: None,
                    });
                }
            }
            i += self.instruction_size(code, i);
        }
        
        // Analyze each loop for optimization opportunities
        for i in 0..self.loops.len() {
            Self::analyze_loop_static(&bytecode.code, &mut self.loops[i]);
        }
    }
    
    fn analyze_loop_static(code: &[u8], loop_info: &mut LoopInfo) {
        // Look for counted loop pattern:
        // GetLocal, LoadConst, Lt/Le, JumpIfFalse
        let start = loop_info.start_offset;
        let end = loop_info.end_offset.min(code.len());
        
        if start + 8 < code.len() && start < end {
            // Check for comparison at loop start
            let has_compare = code[start..end].iter()
                .any(|&op| op == Opcode::Lt as u8 || op == Opcode::Le as u8);
            
            if has_compare {
                loop_info.is_counted = true;
            }
        }
    }
    
    fn instruction_size(&self, code: &[u8], offset: usize) -> usize {
        if offset >= code.len() { return 1; }
        
        match code[offset] {
            x if x == Opcode::LoadConst as u8 => 3,
            x if x == Opcode::GetLocal as u8 => 3,
            x if x == Opcode::SetLocal as u8 => 3,
            x if x == Opcode::GetGlobal as u8 => 3,
            x if x == Opcode::SetGlobal as u8 => 3,
            x if x == Opcode::Jump as u8 => 3,
            x if x == Opcode::JumpIfFalse as u8 => 3,
            x if x == Opcode::JumpIfTrue as u8 => 3,
            x if x == Opcode::Call as u8 => 2,
            x if x == Opcode::NewArray as u8 => 3,
            x if x == Opcode::GetProperty as u8 => 3,
            x if x == Opcode::SetProperty as u8 => 3,
            _ => 1,
        }
    }
    
    /// Get all loops
    pub fn loops(&self) -> &[LoopInfo] { &self.loops }
    
    /// Get counted loops (optimization candidates)
    pub fn counted_loops(&self) -> impl Iterator<Item = &LoopInfo> {
        self.loops.iter().filter(|l| l.is_counted)
    }
    
    /// Check if offset is inside a loop
    pub fn is_in_loop(&self, offset: usize) -> bool {
        self.loops.iter().any(|l| offset >= l.start_offset && offset < l.end_offset)
    }
}

/// Apply loop peeling optimization
/// Executes first iteration separately to enable further optimizations
pub fn peel_first_iteration(bytecode: &mut Bytecode, loop_info: &LoopInfo) {
    // Copy first iteration code before loop
    // This allows type specialization for the common case
    let loop_body: Vec<u8> = bytecode.code[loop_info.start_offset..loop_info.back_edge].to_vec();
    
    // Insert peeled iteration before loop
    // Note: This is a simplified version - full implementation would need
    // to handle jump targets and local variable offsets
    let _ = loop_body; // Placeholder for actual implementation
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_find_loops() {
        let mut bytecode = Bytecode::new();
        
        // Simple loop: for (i = 0; i < 10; i++)
        let loop_start = bytecode.code.len();
        bytecode.emit(Opcode::GetLocal);
        bytecode.emit_u16(0);
        bytecode.emit(Opcode::LoadConst);
        bytecode.emit_u16(0);
        bytecode.emit(Opcode::Lt);
        bytecode.emit(Opcode::JumpIfFalse);
        bytecode.emit_i16(10);  // Exit jump
        
        // Loop body
        bytecode.emit(Opcode::Inc);
        
        // Back edge
        let offset = -((bytecode.code.len() - loop_start + 3) as i16);
        bytecode.emit(Opcode::Jump);
        bytecode.emit_i16(offset);
        
        let mut analyzer = LoopAnalyzer::new();
        analyzer.analyze(&bytecode);
        
        assert_eq!(analyzer.loops().len(), 1);
    }
}
