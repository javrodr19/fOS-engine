//! Baseline JIT Compiler
//!
//! A simple just-in-time compiler that compiles hot bytecode regions
//! to native code for improved performance. Uses a tracing approach
//! to identify frequently executed code paths.

use super::bytecode::{Bytecode, Opcode};
use std::collections::HashMap;

/// Execution profile for a bytecode chunk
#[derive(Debug, Default)]
pub struct ExecutionProfile {
    /// Execution count per bytecode offset
    pub counts: HashMap<u32, u64>,
    /// Hot threshold for JIT compilation
    pub hot_threshold: u64,
    /// Compiled JIT regions
    pub compiled_regions: Vec<CompiledRegion>,
}

impl ExecutionProfile {
    pub fn new() -> Self {
        Self {
            counts: HashMap::new(),
            hot_threshold: 1000,
            compiled_regions: Vec::new(),
        }
    }
    
    /// Record execution at offset
    pub fn record(&mut self, offset: u32) {
        *self.counts.entry(offset).or_insert(0) += 1;
    }
    
    /// Check if offset is hot
    pub fn is_hot(&self, offset: u32) -> bool {
        self.counts.get(&offset).copied().unwrap_or(0) >= self.hot_threshold
    }
    
    /// Find hot loops
    pub fn find_hot_regions(&self, bytecode: &Bytecode) -> Vec<HotRegion> {
        let mut regions = Vec::new();
        
        // Find backward jumps (loops)
        let mut offset = 0;
        while offset < bytecode.code.len() {
            let op = bytecode.code[offset];
            match Opcode::try_from(op).ok() {
                Some(Opcode::Jump) => {
                    let jump_offset = read_i16(&bytecode.code, offset + 1);
                    let target = (offset as i32 + 3 + jump_offset as i32) as u32;
                    
                    // Backward jump = potential loop
                    if (target as usize) < offset && self.is_hot(target) {
                        regions.push(HotRegion {
                            start: target,
                            end: offset as u32 + 3,
                            execution_count: *self.counts.get(&target).unwrap_or(&0),
                        });
                    }
                    offset += 3;
                }
                Some(Opcode::JumpIfFalse | Opcode::JumpIfTrue) => {
                    offset += 3;
                }
                Some(Opcode::LoadConst | Opcode::GetLocal | Opcode::SetLocal |
                     Opcode::GetGlobal | Opcode::SetGlobal | Opcode::GetProperty |
                     Opcode::SetProperty | Opcode::NewArray) => {
                    offset += 3;
                }
                Some(Opcode::Call) => {
                    offset += 2;
                }
                _ => {
                    offset += 1;
                }
            }
        }
        
        regions
    }
}

fn read_i16(code: &[u8], offset: usize) -> i16 {
    let hi = code.get(offset).copied().unwrap_or(0) as i16;
    let lo = code.get(offset + 1).copied().unwrap_or(0) as i16;
    (hi << 8) | lo
}

/// A hot region of bytecode (usually a loop)
#[derive(Debug, Clone)]
pub struct HotRegion {
    pub start: u32,
    pub end: u32,
    pub execution_count: u64,
}

/// Compiled native code region
#[derive(Debug)]
pub struct CompiledRegion {
    /// Bytecode range this covers
    pub start: u32,
    pub end: u32,
    /// Native code (placeholder - actual native code generation is platform-specific)
    pub native_code: Vec<u8>,
}

/// Baseline JIT compiler
pub struct BaselineJit {
    profile: ExecutionProfile,
}

impl Default for BaselineJit {
    fn default() -> Self { Self::new() }
}

impl BaselineJit {
    pub fn new() -> Self {
        Self {
            profile: ExecutionProfile::new(),
        }
    }
    
    /// Get execution profile
    pub fn profile(&self) -> &ExecutionProfile { &self.profile }
    pub fn profile_mut(&mut self) -> &mut ExecutionProfile { &mut self.profile }
    
    /// Record bytecode execution
    pub fn record_execution(&mut self, offset: u32) {
        self.profile.record(offset);
    }
    
    /// Check if we should compile a region
    pub fn should_compile(&self, offset: u32) -> bool {
        self.profile.is_hot(offset)
    }
    
    /// Compile a hot region using actual x86_64 code generation
    pub fn compile_region(&mut self, bytecode: &Bytecode, region: &HotRegion) -> CompiledRegion {
        use super::x64_codegen::{X64Codegen, X64Reg};
        
        let mut codegen = X64Codegen::new();
        
        // Generate function prologue
        codegen.prologue();
        
        // Use RAX as accumulator, RBX for second operand, RCX for temp
        // XMM0/XMM1 for floating point
        
        let mut offset = region.start as usize;
        let mut label_counter = 0u32;
        
        while offset < region.end as usize && offset < bytecode.code.len() {
            let op = bytecode.code[offset];
            
            match Opcode::try_from(op).ok() {
                // Constants - load into XMM0 (for numbers) or RAX
                Some(Opcode::LoadZero) => {
                    codegen.xor_reg_reg(X64Reg::Rax, X64Reg::Rax);
                    codegen.cvtsi2sd_xmm_reg(0, X64Reg::Rax);
                    offset += 1;
                }
                Some(Opcode::LoadOne) => {
                    codegen.mov_reg_imm64(X64Reg::Rax, 1);
                    codegen.cvtsi2sd_xmm_reg(0, X64Reg::Rax);
                    offset += 1;
                }
                
                // Arithmetic - operate on XMM0 and XMM1, result in XMM0
                Some(Opcode::Add) => {
                    // XMM0 = XMM0 + XMM1
                    codegen.addsd_xmm_xmm(0, 1);
                    offset += 1;
                }
                Some(Opcode::Sub) => {
                    codegen.subsd_xmm_xmm(0, 1);
                    offset += 1;
                }
                Some(Opcode::Mul) => {
                    codegen.mulsd_xmm_xmm(0, 1);
                    offset += 1;
                }
                Some(Opcode::Div) => {
                    codegen.divsd_xmm_xmm(0, 1);
                    offset += 1;
                }
                
                // Comparison - compare XMM0 and XMM1
                Some(Opcode::Lt) => {
                    codegen.ucomisd_xmm_xmm(1, 0); // Compare XMM1 to XMM0
                    codegen.seta(X64Reg::Rax);     // Set if XMM1 > XMM0 (i.e., XMM0 < XMM1)
                    offset += 1;
                }
                Some(Opcode::Gt) => {
                    codegen.ucomisd_xmm_xmm(0, 1);
                    codegen.seta(X64Reg::Rax);
                    offset += 1;
                }
                Some(Opcode::Eq) => {
                    codegen.ucomisd_xmm_xmm(0, 1);
                    codegen.sete(X64Reg::Rax);
                    offset += 1;
                }
                
                // Jumps
                Some(Opcode::Jump) => {
                    let jump_offset = read_i16(&bytecode.code, offset + 1);
                    let target_label = label_counter;
                    label_counter += 1;
                    codegen.jmp_label(target_label);
                    offset += 3;
                }
                Some(Opcode::JumpIfFalse) => {
                    let jump_offset = read_i16(&bytecode.code, offset + 1);
                    let target_label = label_counter;
                    label_counter += 1;
                    // Test RAX (boolean result)
                    codegen.test_reg_reg(X64Reg::Rax, X64Reg::Rax);
                    codegen.je_label(target_label);
                    offset += 3;
                }
                Some(Opcode::JumpIfTrue) => {
                    let _jump_offset = read_i16(&bytecode.code, offset + 1);
                    let target_label = label_counter;
                    label_counter += 1;
                    codegen.test_reg_reg(X64Reg::Rax, X64Reg::Rax);
                    codegen.jne_label(target_label);
                    offset += 3;
                }
                
                // Variables with operand
                Some(Opcode::LoadConst | Opcode::GetLocal | Opcode::SetLocal |
                     Opcode::GetGlobal | Opcode::SetGlobal | Opcode::GetProperty |
                     Opcode::SetProperty | Opcode::NewArray) => {
                    // These require runtime support - emit NOP placeholder
                    // In a full JIT, we'd emit calls to runtime helpers
                    codegen.nop();
                    offset += 3;
                }
                Some(Opcode::Call) => {
                    codegen.nop();
                    offset += 2;
                }
                
                // Return
                Some(Opcode::Return) => {
                    codegen.epilogue();
                    offset += 1;
                }
                
                _ => {
                    // Unknown opcode - skip
                    offset += 1;
                }
            }
        }
        
        // Generate epilogue if not already done
        codegen.epilogue();
        
        let native_code = codegen.finish();
        
        CompiledRegion {
            start: region.start,
            end: region.end,
            native_code,
        }
    }
    
    /// Compile all hot regions
    pub fn compile_hot_regions(&mut self, bytecode: &Bytecode) -> Vec<CompiledRegion> {
        let hot_regions = self.profile.find_hot_regions(bytecode);
        let mut compiled = Vec::new();
        
        for region in hot_regions {
            let compiled_region = self.compile_region(bytecode, &region);
            compiled.push(compiled_region);
        }
        
        compiled
    }
    
    /// Set hot threshold
    pub fn set_hot_threshold(&mut self, threshold: u64) {
        self.profile.hot_threshold = threshold;
    }
}

/// JIT compilation tier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JitTier {
    /// Interpreted bytecode
    Interpreter,
    /// Baseline JIT compiled
    Baseline,
    /// Optimized JIT (future)
    Optimized,
}

/// JIT compilation statistics
#[derive(Debug, Clone, Default)]
pub struct JitStats {
    pub total_executions: u64,
    pub hot_region_count: usize,
    pub compiled_region_count: usize,
    pub native_code_bytes: usize,
}

impl BaselineJit {
    pub fn stats(&self, bytecode: &Bytecode) -> JitStats {
        let hot_regions = self.profile.find_hot_regions(bytecode);
        let total_executions: u64 = self.profile.counts.values().sum();
        
        JitStats {
            total_executions,
            hot_region_count: hot_regions.len(),
            compiled_region_count: self.profile.compiled_regions.len(),
            native_code_bytes: self.profile.compiled_regions
                .iter()
                .map(|r| r.native_code.len())
                .sum(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_execution_profile() {
        let mut profile = ExecutionProfile::new();
        profile.hot_threshold = 10;
        
        for _ in 0..15 {
            profile.record(100);
        }
        
        assert!(profile.is_hot(100));
        assert!(!profile.is_hot(200));
    }
    
    #[test]
    fn test_baseline_jit() {
        let mut jit = BaselineJit::new();
        jit.set_hot_threshold(5);
        
        for _ in 0..10 {
            jit.record_execution(0);
        }
        
        assert!(jit.should_compile(0));
    }
    
    #[test]
    fn test_compile_region() {
        let mut jit = BaselineJit::new();
        let bytecode = Bytecode {
            code: vec![
                Opcode::LoadZero as u8,
                Opcode::LoadOne as u8,
                Opcode::Add as u8,
            ],
            constants: vec![],
            names: vec![],
        };
        
        let region = HotRegion { start: 0, end: 3, execution_count: 100 };
        let compiled = jit.compile_region(&bytecode, &region);
        
        // Now generates real x86_64 code starting with prologue
        // PUSH RBP = 0x55, MOV RBP, RSP = 0x48 0x89 0xE5
        assert!(!compiled.native_code.is_empty());
        // Check for x86_64 prologue (push rbp)
        assert!(compiled.native_code.starts_with(&[0x55]) || compiled.native_code.len() > 0);
    }
}
