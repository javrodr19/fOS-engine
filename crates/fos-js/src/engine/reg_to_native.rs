//! Register Bytecode to x86_64 Native Compiler
//!
//! Compiles register-based bytecode to native x86_64 machine code.
//! This is the core JIT compilation pass.

use super::register_vm::{RegBytecode, RegOpcode, RegInstruction};
use super::x64_codegen::{X64Codegen, X64Reg};
use super::executable_jit::ExecutableMemory;
use super::guards::{Guard, GuardType, GuardCompiler};

/// Maps virtual registers (0-255) to x86_64 registers or stack slots
#[derive(Debug, Clone, Copy)]
pub enum RegLocation {
    /// In an x86_64 register
    Register(X64Reg),
    /// On the stack (offset from RBP)
    Stack(i32),
}

/// Register allocator for JIT
pub struct RegAllocator {
    /// Virtual reg -> physical location
    locations: [RegLocation; 256],
    /// Available general purpose registers
    available: Vec<X64Reg>,
    /// Current stack offset for spills
    stack_offset: i32,
}

impl Default for RegAllocator {
    fn default() -> Self { Self::new() }
}

impl RegAllocator {
    pub fn new() -> Self {
        // Use callee-saved + some caller-saved registers
        let available = vec![
            X64Reg::Rbx,
            X64Reg::R12, X64Reg::R13, X64Reg::R14, X64Reg::R15,
            X64Reg::R8, X64Reg::R9, X64Reg::R10, X64Reg::R11,
        ];
        
        let mut locations = [RegLocation::Stack(-8); 256];
        
        // Pre-allocate first 9 virtual regs to physical regs
        for (i, &reg) in available.iter().take(9).enumerate() {
            locations[i] = RegLocation::Register(reg);
        }
        
        Self {
            locations,
            available,
            stack_offset: -64, // Start stack slots below saved registers
        }
    }
    
    /// Get location for virtual register
    pub fn get(&self, vreg: u8) -> RegLocation {
        self.locations[vreg as usize]
    }
    
    /// Allocate a physical register for a virtual one
    pub fn allocate(&mut self, vreg: u8) -> RegLocation {
        if let Some(reg) = self.available.pop() {
            self.locations[vreg as usize] = RegLocation::Register(reg);
            RegLocation::Register(reg)
        } else {
            let offset = self.stack_offset;
            self.stack_offset -= 8;
            self.locations[vreg as usize] = RegLocation::Stack(offset);
            RegLocation::Stack(offset)
        }
    }
}

/// JIT compiler for register bytecode
pub struct RegBytecodeCompiler {
    codegen: X64Codegen,
    allocator: RegAllocator,
    guard_compiler: GuardCompiler,
    /// Label ID for each bytecode offset
    labels: std::collections::HashMap<usize, u32>,
    next_label: u32,
}

impl Default for RegBytecodeCompiler {
    fn default() -> Self { Self::new() }
}

impl RegBytecodeCompiler {
    pub fn new() -> Self {
        Self {
            codegen: X64Codegen::new(),
            allocator: RegAllocator::new(),
            guard_compiler: GuardCompiler::new(),
            labels: std::collections::HashMap::new(),
            next_label: 0,
        }
    }
    
    /// Compile register bytecode to native code
    pub fn compile(mut self, bytecode: &RegBytecode) -> CompiledCode {
        // Function prologue
        self.codegen.prologue();
        
        // Allocate stack space for spills
        // sub rsp, 256 (for spilled registers)
        self.codegen.emit(0x48); // REX.W
        self.codegen.emit(0x81);
        self.codegen.emit(0xEC);
        self.codegen.emit_bytes(&256i32.to_le_bytes());
        
        // Pre-compute labels for jump targets
        for (i, inst) in bytecode.instructions.iter().enumerate() {
            if matches!(inst.opcode, RegOpcode::Jump | RegOpcode::JumpIfFalse | RegOpcode::JumpIfTrue) {
                let offset = inst.imm16() as i32;
                let target = (i as i32 + 1 + offset) as usize;
                if !self.labels.contains_key(&target) {
                    self.labels.insert(target, self.next_label);
                    self.next_label += 1;
                }
            }
        }
        
        // Compile each instruction
        for (i, inst) in bytecode.instructions.iter().enumerate() {
            // Emit label if this is a jump target
            if let Some(&label) = self.labels.get(&i) {
                self.codegen.label(label);
            }
            
            self.compile_instruction(inst, bytecode);
        }
        
        let code = self.codegen.finish();
        
        CompiledCode {
            code,
            guards: self.guard_compiler.guards().to_vec(),
        }
    }
    
    fn compile_instruction(&mut self, inst: &RegInstruction, bytecode: &RegBytecode) {
        match inst.opcode {
            RegOpcode::Move => {
                self.emit_move(inst.a, inst.b);
            }
            RegOpcode::LoadConst => {
                let idx = inst.u16() as usize;
                if let Some(val) = bytecode.constants.get(idx) {
                    let bits = val.to_number().to_bits();
                    self.emit_load_imm(inst.a, bits);
                }
            }
            RegOpcode::LoadInt => {
                let imm = inst.imm16() as i64;
                self.emit_load_imm(inst.a, imm as u64);
            }
            RegOpcode::LoadNull | RegOpcode::LoadUndefined => {
                // Use NaN-boxing null representation
                self.emit_load_imm(inst.a, 0x7FF8_0000_0000_0001);
            }
            RegOpcode::LoadTrue => {
                self.emit_load_imm(inst.a, 0x7FF8_0000_0000_0003);
            }
            RegOpcode::LoadFalse => {
                self.emit_load_imm(inst.a, 0x7FF8_0000_0000_0002);
            }
            
            // Arithmetic
            RegOpcode::Add => self.emit_binary_op(inst.a, inst.b, inst.c, BinaryOp::Add),
            RegOpcode::Sub => self.emit_binary_op(inst.a, inst.b, inst.c, BinaryOp::Sub),
            RegOpcode::Mul => self.emit_binary_op(inst.a, inst.b, inst.c, BinaryOp::Mul),
            RegOpcode::Div => self.emit_binary_op(inst.a, inst.b, inst.c, BinaryOp::Div),
            RegOpcode::Neg => self.emit_unary_op(inst.a, inst.b, UnaryOp::Neg),
            
            // Comparison
            RegOpcode::Lt => self.emit_compare(inst.a, inst.b, inst.c, CompareOp::Lt),
            RegOpcode::Le => self.emit_compare(inst.a, inst.b, inst.c, CompareOp::Le),
            RegOpcode::Gt => self.emit_compare(inst.a, inst.b, inst.c, CompareOp::Gt),
            RegOpcode::Ge => self.emit_compare(inst.a, inst.b, inst.c, CompareOp::Ge),
            RegOpcode::Eq => self.emit_compare(inst.a, inst.b, inst.c, CompareOp::Eq),
            
            // Logical
            RegOpcode::Not => self.emit_unary_op(inst.a, inst.b, UnaryOp::Not),
            
            // Control flow
            RegOpcode::Jump => {
                let offset = inst.imm16() as i32;
                // Calculate target instruction index (relative to next instruction)
                // This is a simplification - real impl needs instruction pointer tracking
                let label = self.next_label;
                self.next_label += 1;
                self.codegen.jmp_label(label);
            }
            RegOpcode::JumpIfFalse => {
                let cond = self.load_to_reg(inst.a, X64Reg::Rax);
                self.codegen.test_reg_reg(X64Reg::Rax, X64Reg::Rax);
                let label = self.next_label;
                self.next_label += 1;
                self.codegen.je_label(label);
            }
            RegOpcode::JumpIfTrue => {
                let cond = self.load_to_reg(inst.a, X64Reg::Rax);
                self.codegen.test_reg_reg(X64Reg::Rax, X64Reg::Rax);
                let label = self.next_label;
                self.next_label += 1;
                self.codegen.jne_label(label);
            }
            
            // Return
            RegOpcode::Return => {
                self.load_to_reg(inst.a, X64Reg::Rax);
                // Deallocate stack space
                self.codegen.emit(0x48);
                self.codegen.emit(0x81);
                self.codegen.emit(0xC4);
                self.codegen.emit_bytes(&256i32.to_le_bytes());
                self.codegen.epilogue();
            }
            
            RegOpcode::Halt => {
                self.codegen.emit(0x48);
                self.codegen.emit(0x81);
                self.codegen.emit(0xC4);
                self.codegen.emit_bytes(&256i32.to_le_bytes());
                self.codegen.epilogue();
            }
            
            _ => {}
        }
    }
    
    fn emit_move(&mut self, dst: u8, src: u8) {
        let src_loc = self.allocator.get(src);
        let dst_loc = self.allocator.get(dst);
        
        match (dst_loc, src_loc) {
            (RegLocation::Register(d), RegLocation::Register(s)) => {
                self.codegen.mov_reg_reg(d, s);
            }
            (RegLocation::Register(d), RegLocation::Stack(off)) => {
                self.codegen.mov_reg_mem(d, X64Reg::Rbp, off);
            }
            (RegLocation::Stack(off), RegLocation::Register(s)) => {
                self.codegen.mov_mem_reg(X64Reg::Rbp, off, s);
            }
            (RegLocation::Stack(dst_off), RegLocation::Stack(src_off)) => {
                self.codegen.mov_reg_mem(X64Reg::Rax, X64Reg::Rbp, src_off);
                self.codegen.mov_mem_reg(X64Reg::Rbp, dst_off, X64Reg::Rax);
            }
        }
    }
    
    fn emit_load_imm(&mut self, dst: u8, imm: u64) {
        match self.allocator.get(dst) {
            RegLocation::Register(reg) => {
                self.codegen.mov_reg_imm64(reg, imm);
            }
            RegLocation::Stack(off) => {
                self.codegen.mov_reg_imm64(X64Reg::Rax, imm);
                self.codegen.mov_mem_reg(X64Reg::Rbp, off, X64Reg::Rax);
            }
        }
    }
    
    fn load_to_reg(&mut self, vreg: u8, target: X64Reg) -> X64Reg {
        match self.allocator.get(vreg) {
            RegLocation::Register(reg) => {
                if reg != target {
                    self.codegen.mov_reg_reg(target, reg);
                }
                target
            }
            RegLocation::Stack(off) => {
                self.codegen.mov_reg_mem(target, X64Reg::Rbp, off);
                target
            }
        }
    }
    
    fn emit_binary_op(&mut self, dst: u8, a: u8, b: u8, op: BinaryOp) {
        // Load operands to XMM registers for float ops
        self.load_to_xmm(a, 0);
        self.load_to_xmm(b, 1);
        
        match op {
            BinaryOp::Add => self.codegen.addsd_xmm_xmm(0, 1),
            BinaryOp::Sub => self.codegen.subsd_xmm_xmm(0, 1),
            BinaryOp::Mul => self.codegen.mulsd_xmm_xmm(0, 1),
            BinaryOp::Div => self.codegen.divsd_xmm_xmm(0, 1),
        }
        
        self.store_from_xmm(0, dst);
    }
    
    fn emit_unary_op(&mut self, dst: u8, src: u8, op: UnaryOp) {
        self.load_to_reg(src, X64Reg::Rax);
        
        match op {
            UnaryOp::Neg => self.codegen.neg_reg(X64Reg::Rax),
            UnaryOp::Not => {
                self.codegen.test_reg_reg(X64Reg::Rax, X64Reg::Rax);
                self.codegen.sete(X64Reg::Rax);
            }
        }
        
        self.store_from_reg(X64Reg::Rax, dst);
    }
    
    fn emit_compare(&mut self, dst: u8, a: u8, b: u8, op: CompareOp) {
        self.load_to_xmm(a, 0);
        self.load_to_xmm(b, 1);
        
        self.codegen.ucomisd_xmm_xmm(0, 1);
        
        match op {
            CompareOp::Lt => self.codegen.setl(X64Reg::Rax),
            CompareOp::Le => self.codegen.setle(X64Reg::Rax),
            CompareOp::Gt => self.codegen.setg(X64Reg::Rax),
            CompareOp::Ge => self.codegen.setge(X64Reg::Rax),
            CompareOp::Eq => self.codegen.sete(X64Reg::Rax),
        }
        
        // Zero-extend to 64-bit
        self.codegen.emit(0x48); self.codegen.emit(0x0F);
        self.codegen.emit(0xB6); self.codegen.emit(0xC0);
        
        self.store_from_reg(X64Reg::Rax, dst);
    }
    
    fn load_to_xmm(&mut self, vreg: u8, xmm: u8) {
        match self.allocator.get(vreg) {
            RegLocation::Register(reg) => {
                // MOVQ xmm, reg
                self.codegen.emit(0x66);
                self.codegen.emit(0x48);
                self.codegen.emit(0x0F);
                self.codegen.emit(0x6E);
                self.codegen.emit(0xC0 | (xmm << 3) | (reg as u8 & 7));
            }
            RegLocation::Stack(off) => {
                // MOVSD xmm, [rbp+off]
                self.codegen.emit(0xF2);
                self.codegen.emit(0x0F);
                self.codegen.emit(0x10);
                if off >= -128 && off <= 127 {
                    self.codegen.emit(0x45 | (xmm << 3));
                    self.codegen.emit(off as u8);
                } else {
                    self.codegen.emit(0x85 | (xmm << 3));
                    self.codegen.emit_bytes(&off.to_le_bytes());
                }
            }
        }
    }
    
    fn store_from_xmm(&mut self, xmm: u8, vreg: u8) {
        match self.allocator.get(vreg) {
            RegLocation::Register(reg) => {
                // MOVQ reg, xmm
                self.codegen.emit(0x66);
                self.codegen.emit(0x48);
                self.codegen.emit(0x0F);
                self.codegen.emit(0x7E);
                self.codegen.emit(0xC0 | (xmm << 3) | (reg as u8 & 7));
            }
            RegLocation::Stack(off) => {
                // MOVSD [rbp+off], xmm
                self.codegen.emit(0xF2);
                self.codegen.emit(0x0F);
                self.codegen.emit(0x11);
                if off >= -128 && off <= 127 {
                    self.codegen.emit(0x45 | (xmm << 3));
                    self.codegen.emit(off as u8);
                } else {
                    self.codegen.emit(0x85 | (xmm << 3));
                    self.codegen.emit_bytes(&off.to_le_bytes());
                }
            }
        }
    }
    
    fn store_from_reg(&mut self, src: X64Reg, vreg: u8) {
        match self.allocator.get(vreg) {
            RegLocation::Register(dst) => {
                if dst != src {
                    self.codegen.mov_reg_reg(dst, src);
                }
            }
            RegLocation::Stack(off) => {
                self.codegen.mov_mem_reg(X64Reg::Rbp, off, src);
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum BinaryOp { Add, Sub, Mul, Div }

#[derive(Debug, Clone, Copy)]
enum UnaryOp { Neg, Not }

#[derive(Debug, Clone, Copy)]
enum CompareOp { Lt, Le, Gt, Ge, Eq }

/// Result of JIT compilation
pub struct CompiledCode {
    pub code: Vec<u8>,
    pub guards: Vec<Guard>,
}

/// Compile register bytecode to native code
pub fn compile_to_native(bytecode: &RegBytecode) -> CompiledCode {
    RegBytecodeCompiler::new().compile(bytecode)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_compile_simple() {
        use super::super::register_vm::RegBytecode;
        
        let mut bc = RegBytecode::new();
        bc.emit(RegOpcode::LoadInt, 0, 10, 0);
        bc.emit(RegOpcode::LoadInt, 1, 5, 0);
        bc.emit(RegOpcode::Add, 2, 0, 1);
        bc.emit(RegOpcode::Return, 2, 0, 0);
        
        let compiled = compile_to_native(&bc);
        
        // Should generate non-empty code
        assert!(!compiled.code.is_empty());
    }
    
    #[test]
    fn test_register_allocator() {
        let alloc = RegAllocator::new();
        
        // First few regs should be in physical registers
        match alloc.get(0) {
            RegLocation::Register(_) => {}
            RegLocation::Stack(_) => panic!("Expected register"),
        }
    }
}
