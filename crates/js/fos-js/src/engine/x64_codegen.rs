//! x86_64 Code Generator
//!
//! Generates native x86_64 machine code from bytecode.
//! Uses a simple one-pass compilation strategy.

/// x86_64 Register names
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum X64Reg {
    Rax = 0, Rcx = 1, Rdx = 2, Rbx = 3,
    Rsp = 4, Rbp = 5, Rsi = 6, Rdi = 7,
    R8 = 8, R9 = 9, R10 = 10, R11 = 11,
    R12 = 12, R13 = 13, R14 = 14, R15 = 15,
}

/// x86_64 machine code builder
pub struct X64Codegen {
    code: Vec<u8>,
    labels: std::collections::HashMap<u32, usize>,
    fixups: Vec<(usize, u32)>,  // (code offset, label id)
}

impl Default for X64Codegen {
    fn default() -> Self { Self::new() }
}

impl X64Codegen {
    pub fn new() -> Self {
        Self {
            code: Vec::new(),
            labels: std::collections::HashMap::new(),
            fixups: Vec::new(),
        }
    }
    
    /// Get generated code
    pub fn finish(mut self) -> Vec<u8> {
        // Apply fixups
        for (offset, label_id) in &self.fixups {
            if let Some(&target) = self.labels.get(label_id) {
                let rel = (target as i32) - (*offset as i32 + 4);
                let bytes = rel.to_le_bytes();
                self.code[*offset..*offset + 4].copy_from_slice(&bytes);
            }
        }
        self.code
    }
    
    /// Define a label at current position
    pub fn label(&mut self, id: u32) {
        self.labels.insert(id, self.code.len());
    }
    
    /// Emit raw byte
    pub fn emit(&mut self, byte: u8) {
        self.code.push(byte);
    }
    
    /// Emit raw bytes
    pub fn emit_bytes(&mut self, bytes: &[u8]) {
        self.code.extend_from_slice(bytes);
    }
    
    // === x86_64 Instructions ===
    
    /// Push register
    pub fn push_reg(&mut self, reg: X64Reg) {
        if (reg as u8) >= 8 {
            self.emit(0x41); // REX.B
        }
        self.emit(0x50 + (reg as u8 & 7));
    }
    
    /// Pop register
    pub fn pop_reg(&mut self, reg: X64Reg) {
        if (reg as u8) >= 8 {
            self.emit(0x41);
        }
        self.emit(0x58 + (reg as u8 & 7));
    }
    
    /// MOV reg, imm64
    pub fn mov_reg_imm64(&mut self, reg: X64Reg, imm: u64) {
        let r = reg as u8;
        if r >= 8 {
            self.emit(0x49); // REX.WB
        } else {
            self.emit(0x48); // REX.W
        }
        self.emit(0xB8 + (r & 7));
        self.emit_bytes(&imm.to_le_bytes());
    }
    
    /// MOV reg, reg
    pub fn mov_reg_reg(&mut self, dst: X64Reg, src: X64Reg) {
        let d = dst as u8;
        let s = src as u8;
        let mut rex = 0x48u8; // REX.W
        if d >= 8 { rex |= 0x04; } // REX.R
        if s >= 8 { rex |= 0x01; } // REX.B
        self.emit(rex);
        self.emit(0x89);
        self.emit(0xC0 | ((s & 7) << 3) | (d & 7));
    }
    
    /// ADD dst, src
    pub fn add_reg_reg(&mut self, dst: X64Reg, src: X64Reg) {
        let d = dst as u8;
        let s = src as u8;
        let mut rex = 0x48u8;
        if d >= 8 { rex |= 0x01; }
        if s >= 8 { rex |= 0x04; }
        self.emit(rex);
        self.emit(0x01);
        self.emit(0xC0 | ((s & 7) << 3) | (d & 7));
    }
    
    /// SUB dst, src
    pub fn sub_reg_reg(&mut self, dst: X64Reg, src: X64Reg) {
        let d = dst as u8;
        let s = src as u8;
        let mut rex = 0x48u8;
        if d >= 8 { rex |= 0x01; }
        if s >= 8 { rex |= 0x04; }
        self.emit(rex);
        self.emit(0x29);
        self.emit(0xC0 | ((s & 7) << 3) | (d & 7));
    }
    
    /// IMUL dst, src
    pub fn imul_reg_reg(&mut self, dst: X64Reg, src: X64Reg) {
        let d = dst as u8;
        let s = src as u8;
        let mut rex = 0x48u8;
        if d >= 8 { rex |= 0x04; }
        if s >= 8 { rex |= 0x01; }
        self.emit(rex);
        self.emit(0x0F);
        self.emit(0xAF);
        self.emit(0xC0 | ((d & 7) << 3) | (s & 7));
    }
    
    /// CMP reg, reg
    pub fn cmp_reg_reg(&mut self, a: X64Reg, b: X64Reg) {
        let ar = a as u8;
        let br = b as u8;
        let mut rex = 0x48u8;
        if ar >= 8 { rex |= 0x01; }
        if br >= 8 { rex |= 0x04; }
        self.emit(rex);
        self.emit(0x39);
        self.emit(0xC0 | ((br & 7) << 3) | (ar & 7));
    }
    
    /// JMP rel32 (forward reference with fixup)
    pub fn jmp_label(&mut self, label_id: u32) {
        self.emit(0xE9);
        self.fixups.push((self.code.len(), label_id));
        self.emit_bytes(&[0, 0, 0, 0]);
    }
    
    /// JE rel32
    pub fn je_label(&mut self, label_id: u32) {
        self.emit(0x0F);
        self.emit(0x84);
        self.fixups.push((self.code.len(), label_id));
        self.emit_bytes(&[0, 0, 0, 0]);
    }
    
    /// JNE rel32
    pub fn jne_label(&mut self, label_id: u32) {
        self.emit(0x0F);
        self.emit(0x85);
        self.fixups.push((self.code.len(), label_id));
        self.emit_bytes(&[0, 0, 0, 0]);
    }
    
    /// JL rel32 (less than, signed)
    pub fn jl_label(&mut self, label_id: u32) {
        self.emit(0x0F);
        self.emit(0x8C);
        self.fixups.push((self.code.len(), label_id));
        self.emit_bytes(&[0, 0, 0, 0]);
    }
    
    /// JGE rel32 (greater or equal, signed)
    pub fn jge_label(&mut self, label_id: u32) {
        self.emit(0x0F);
        self.emit(0x8D);
        self.fixups.push((self.code.len(), label_id));
        self.emit_bytes(&[0, 0, 0, 0]);
    }
    
    /// RET
    pub fn ret(&mut self) {
        self.emit(0xC3);
    }
    
    /// CALL reg
    pub fn call_reg(&mut self, reg: X64Reg) {
        if (reg as u8) >= 8 {
            self.emit(0x41);
        }
        self.emit(0xFF);
        self.emit(0xD0 | (reg as u8 & 7));
    }
    
    /// NOP
    pub fn nop(&mut self) {
        self.emit(0x90);
    }
    
    /// Generate function prologue
    pub fn prologue(&mut self) {
        self.push_reg(X64Reg::Rbp);
        self.mov_reg_reg(X64Reg::Rbp, X64Reg::Rsp);
        // Save callee-saved registers
        self.push_reg(X64Reg::Rbx);
        self.push_reg(X64Reg::R12);
        self.push_reg(X64Reg::R13);
        self.push_reg(X64Reg::R14);
        self.push_reg(X64Reg::R15);
    }
    
    /// Generate function epilogue
    pub fn epilogue(&mut self) {
        self.pop_reg(X64Reg::R15);
        self.pop_reg(X64Reg::R14);
        self.pop_reg(X64Reg::R13);
        self.pop_reg(X64Reg::R12);
        self.pop_reg(X64Reg::Rbx);
        self.pop_reg(X64Reg::Rbp);
        self.ret();
    }
    
    // === SSE Floating Point Operations ===
    
    /// MOVSD xmm, xmm (move scalar double)
    pub fn movsd_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit(0xF2);
        if dst >= 8 || src >= 8 {
            let mut rex = 0x40u8;
            if dst >= 8 { rex |= 0x04; }
            if src >= 8 { rex |= 0x01; }
            self.emit(rex);
        }
        self.emit(0x0F);
        self.emit(0x10);
        self.emit(0xC0 | ((dst & 7) << 3) | (src & 7));
    }
    
    /// ADDSD xmm, xmm (add scalar double)
    pub fn addsd_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit(0xF2);
        if dst >= 8 || src >= 8 {
            let mut rex = 0x40u8;
            if dst >= 8 { rex |= 0x04; }
            if src >= 8 { rex |= 0x01; }
            self.emit(rex);
        }
        self.emit(0x0F);
        self.emit(0x58);
        self.emit(0xC0 | ((dst & 7) << 3) | (src & 7));
    }
    
    /// SUBSD xmm, xmm (subtract scalar double)
    pub fn subsd_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit(0xF2);
        if dst >= 8 || src >= 8 {
            let mut rex = 0x40u8;
            if dst >= 8 { rex |= 0x04; }
            if src >= 8 { rex |= 0x01; }
            self.emit(rex);
        }
        self.emit(0x0F);
        self.emit(0x5C);
        self.emit(0xC0 | ((dst & 7) << 3) | (src & 7));
    }
    
    /// MULSD xmm, xmm (multiply scalar double)
    pub fn mulsd_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit(0xF2);
        if dst >= 8 || src >= 8 {
            let mut rex = 0x40u8;
            if dst >= 8 { rex |= 0x04; }
            if src >= 8 { rex |= 0x01; }
            self.emit(rex);
        }
        self.emit(0x0F);
        self.emit(0x59);
        self.emit(0xC0 | ((dst & 7) << 3) | (src & 7));
    }
    
    /// DIVSD xmm, xmm (divide scalar double)
    pub fn divsd_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit(0xF2);
        if dst >= 8 || src >= 8 {
            let mut rex = 0x40u8;
            if dst >= 8 { rex |= 0x04; }
            if src >= 8 { rex |= 0x01; }
            self.emit(rex);
        }
        self.emit(0x0F);
        self.emit(0x5E);
        self.emit(0xC0 | ((dst & 7) << 3) | (src & 7));
    }
    
    /// UCOMISD xmm, xmm (compare scalar double, set flags)
    pub fn ucomisd_xmm_xmm(&mut self, a: u8, b: u8) {
        self.emit(0x66);
        if a >= 8 || b >= 8 {
            let mut rex = 0x40u8;
            if a >= 8 { rex |= 0x04; }
            if b >= 8 { rex |= 0x01; }
            self.emit(rex);
        }
        self.emit(0x0F);
        self.emit(0x2E);
        self.emit(0xC0 | ((a & 7) << 3) | (b & 7));
    }
    
    /// CVTSI2SD xmm, reg (convert int to double)
    pub fn cvtsi2sd_xmm_reg(&mut self, xmm: u8, reg: X64Reg) {
        self.emit(0xF2);
        let r = reg as u8;
        let mut rex = 0x48u8;
        if xmm >= 8 { rex |= 0x04; }
        if r >= 8 { rex |= 0x01; }
        self.emit(rex);
        self.emit(0x0F);
        self.emit(0x2A);
        self.emit(0xC0 | ((xmm & 7) << 3) | (r & 7));
    }
    
    /// CVTTSD2SI reg, xmm (convert double to int, truncate)
    pub fn cvttsd2si_reg_xmm(&mut self, reg: X64Reg, xmm: u8) {
        self.emit(0xF2);
        let r = reg as u8;
        let mut rex = 0x48u8;
        if r >= 8 { rex |= 0x04; }
        if xmm >= 8 { rex |= 0x01; }
        self.emit(rex);
        self.emit(0x0F);
        self.emit(0x2C);
        self.emit(0xC0 | ((r & 7) << 3) | (xmm & 7));
    }
    
    // === Memory Access ===
    
    /// MOV reg, [reg + offset]
    pub fn mov_reg_mem(&mut self, dst: X64Reg, base: X64Reg, offset: i32) {
        let d = dst as u8;
        let b = base as u8;
        let mut rex = 0x48u8;
        if d >= 8 { rex |= 0x04; }
        if b >= 8 { rex |= 0x01; }
        self.emit(rex);
        self.emit(0x8B);
        
        if offset == 0 && (b & 7) != 5 {
            self.emit(((d & 7) << 3) | (b & 7));
        } else if offset >= -128 && offset <= 127 {
            self.emit(0x40 | ((d & 7) << 3) | (b & 7));
            self.emit(offset as u8);
        } else {
            self.emit(0x80 | ((d & 7) << 3) | (b & 7));
            self.emit_bytes(&offset.to_le_bytes());
        }
    }
    
    /// MOV [reg + offset], reg
    pub fn mov_mem_reg(&mut self, base: X64Reg, offset: i32, src: X64Reg) {
        let s = src as u8;
        let b = base as u8;
        let mut rex = 0x48u8;
        if s >= 8 { rex |= 0x04; }
        if b >= 8 { rex |= 0x01; }
        self.emit(rex);
        self.emit(0x89);
        
        if offset == 0 && (b & 7) != 5 {
            self.emit(((s & 7) << 3) | (b & 7));
        } else if offset >= -128 && offset <= 127 {
            self.emit(0x40 | ((s & 7) << 3) | (b & 7));
            self.emit(offset as u8);
        } else {
            self.emit(0x80 | ((s & 7) << 3) | (b & 7));
            self.emit_bytes(&offset.to_le_bytes());
        }
    }
    
    // === Additional Jumps ===
    
    /// JG rel32 (greater than, signed)
    pub fn jg_label(&mut self, label_id: u32) {
        self.emit(0x0F);
        self.emit(0x8F);
        self.fixups.push((self.code.len(), label_id));
        self.emit_bytes(&[0, 0, 0, 0]);
    }
    
    /// JLE rel32 (less or equal, signed)
    pub fn jle_label(&mut self, label_id: u32) {
        self.emit(0x0F);
        self.emit(0x8E);
        self.fixups.push((self.code.len(), label_id));
        self.emit_bytes(&[0, 0, 0, 0]);
    }
    
    /// JA rel32 (above, unsigned)
    pub fn ja_label(&mut self, label_id: u32) {
        self.emit(0x0F);
        self.emit(0x87);
        self.fixups.push((self.code.len(), label_id));
        self.emit_bytes(&[0, 0, 0, 0]);
    }
    
    /// JB rel32 (below, unsigned)
    pub fn jb_label(&mut self, label_id: u32) {
        self.emit(0x0F);
        self.emit(0x82);
        self.fixups.push((self.code.len(), label_id));
        self.emit_bytes(&[0, 0, 0, 0]);
    }
    
    // === Bitwise Operations ===
    
    /// AND reg, reg
    pub fn and_reg_reg(&mut self, dst: X64Reg, src: X64Reg) {
        let d = dst as u8;
        let s = src as u8;
        let mut rex = 0x48u8;
        if d >= 8 { rex |= 0x01; }
        if s >= 8 { rex |= 0x04; }
        self.emit(rex);
        self.emit(0x21);
        self.emit(0xC0 | ((s & 7) << 3) | (d & 7));
    }
    
    /// OR reg, reg
    pub fn or_reg_reg(&mut self, dst: X64Reg, src: X64Reg) {
        let d = dst as u8;
        let s = src as u8;
        let mut rex = 0x48u8;
        if d >= 8 { rex |= 0x01; }
        if s >= 8 { rex |= 0x04; }
        self.emit(rex);
        self.emit(0x09);
        self.emit(0xC0 | ((s & 7) << 3) | (d & 7));
    }
    
    /// XOR reg, reg
    pub fn xor_reg_reg(&mut self, dst: X64Reg, src: X64Reg) {
        let d = dst as u8;
        let s = src as u8;
        let mut rex = 0x48u8;
        if d >= 8 { rex |= 0x01; }
        if s >= 8 { rex |= 0x04; }
        self.emit(rex);
        self.emit(0x31);
        self.emit(0xC0 | ((s & 7) << 3) | (d & 7));
    }
    
    /// NEG reg (two's complement negation)
    pub fn neg_reg(&mut self, reg: X64Reg) {
        let r = reg as u8;
        let mut rex = 0x48u8;
        if r >= 8 { rex |= 0x01; }
        self.emit(rex);
        self.emit(0xF7);
        self.emit(0xD8 | (r & 7));
    }
    
    /// NOT reg (bitwise not)
    pub fn not_reg(&mut self, reg: X64Reg) {
        let r = reg as u8;
        let mut rex = 0x48u8;
        if r >= 8 { rex |= 0x01; }
        self.emit(rex);
        self.emit(0xF7);
        self.emit(0xD0 | (r & 7));
    }
    
    /// INC reg
    pub fn inc_reg(&mut self, reg: X64Reg) {
        let r = reg as u8;
        let mut rex = 0x48u8;
        if r >= 8 { rex |= 0x01; }
        self.emit(rex);
        self.emit(0xFF);
        self.emit(0xC0 | (r & 7));
    }
    
    /// DEC reg
    pub fn dec_reg(&mut self, reg: X64Reg) {
        let r = reg as u8;
        let mut rex = 0x48u8;
        if r >= 8 { rex |= 0x01; }
        self.emit(rex);
        self.emit(0xFF);
        self.emit(0xC8 | (r & 7));
    }
    
    /// TEST reg, reg (AND without storing, just set flags)
    pub fn test_reg_reg(&mut self, a: X64Reg, b: X64Reg) {
        let ar = a as u8;
        let br = b as u8;
        let mut rex = 0x48u8;
        if ar >= 8 { rex |= 0x01; }
        if br >= 8 { rex |= 0x04; }
        self.emit(rex);
        self.emit(0x85);
        self.emit(0xC0 | ((br & 7) << 3) | (ar & 7));
    }
    
    /// SETCC - Set byte based on condition
    pub fn sete(&mut self, reg: X64Reg) { self.setcc(0x94, reg); }
    pub fn setne(&mut self, reg: X64Reg) { self.setcc(0x95, reg); }
    pub fn setl(&mut self, reg: X64Reg) { self.setcc(0x9C, reg); }
    pub fn setg(&mut self, reg: X64Reg) { self.setcc(0x9F, reg); }
    pub fn setle(&mut self, reg: X64Reg) { self.setcc(0x9E, reg); }
    pub fn setge(&mut self, reg: X64Reg) { self.setcc(0x9D, reg); }
    
    fn setcc(&mut self, opcode: u8, reg: X64Reg) {
        let r = reg as u8;
        if r >= 8 {
            self.emit(0x41);
        }
        self.emit(0x0F);
        self.emit(opcode);
        self.emit(0xC0 | (r & 7));
    }
    
    /// Get current code offset
    pub fn current_offset(&self) -> usize {
        self.code.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_mov_imm() {
        let mut cg = X64Codegen::new();
        cg.mov_reg_imm64(X64Reg::Rax, 0x123456789ABCDEF0);
        let code = cg.finish();
        assert_eq!(code[0], 0x48); // REX.W
        assert_eq!(code[1], 0xB8); // MOV RAX, imm64
    }
    
    #[test]
    fn test_prologue_epilogue() {
        let mut cg = X64Codegen::new();
        cg.prologue();
        cg.mov_reg_imm64(X64Reg::Rax, 42);
        cg.epilogue();
        let code = cg.finish();
        assert!(!code.is_empty());
    }
}
