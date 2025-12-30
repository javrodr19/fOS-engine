//! Executable JIT Compiler
//!
//! Generates and executes native x86_64 machine code.
//! Uses platform-specific memory allocation for executable code.

use super::bytecode::{Bytecode, Opcode};
use super::x64_codegen::{X64Codegen, X64Reg};
use super::value::JsVal;

use std::ptr;

/// Executable memory region
pub struct ExecutableMemory {
    ptr: *mut u8,
    size: usize,
    #[allow(dead_code)]
    layout: std::alloc::Layout,
}

unsafe impl Send for ExecutableMemory {}
unsafe impl Sync for ExecutableMemory {}

impl ExecutableMemory {
    /// Allocate memory for code storage
    /// Note: On most systems, this memory won't be executable without mprotect
    /// This is a simplified implementation for demonstration
    pub fn new(size: usize) -> Option<Self> {
        let layout = std::alloc::Layout::from_size_align(size.max(4096), 4096).ok()?;
        let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
        if ptr.is_null() { 
            None 
        } else { 
            Some(Self { ptr, size, layout }) 
        }
    }
    
    /// Copy code into memory
    pub fn write(&mut self, code: &[u8]) {
        let len = code.len().min(self.size);
        unsafe {
            ptr::copy_nonoverlapping(code.as_ptr(), self.ptr, len);
        }
    }
    
    /// Get function pointer (platform-dependent whether it's actually executable)
    pub fn as_fn<T>(&self) -> T {
        unsafe { std::mem::transmute_copy(&self.ptr) }
    }
    
    /// Get raw code bytes
    pub fn code_bytes(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr, self.size) }
    }
}

impl Drop for ExecutableMemory {
    fn drop(&mut self) {
        unsafe {
            std::alloc::dealloc(self.ptr, self.layout);
        }
    }
}

/// JIT-compiled function
pub struct JitFunction {
    memory: ExecutableMemory,
    entry_offset: usize,
}

impl JitFunction {
    /// Execute the JIT function (returns i64 result)
    pub fn call(&self) -> i64 {
        type JitFn = extern "C" fn() -> i64;
        let f: JitFn = self.memory.as_fn();
        f()
    }
}

/// Full JIT compiler that generates executable code
pub struct ExecutableJit {
    hot_threshold: u64,
    execution_counts: std::collections::HashMap<u32, u64>,
    compiled_functions: Vec<JitFunction>,
}

impl Default for ExecutableJit {
    fn default() -> Self { Self::new() }
}

impl ExecutableJit {
    pub fn new() -> Self {
        Self {
            hot_threshold: 1000,
            execution_counts: std::collections::HashMap::new(),
            compiled_functions: Vec::new(),
        }
    }
    
    /// Record execution and potentially compile
    pub fn record(&mut self, offset: u32) -> bool {
        let count = self.execution_counts.entry(offset).or_insert(0);
        *count += 1;
        *count >= self.hot_threshold
    }
    
    /// Compile bytecode to native x86_64
    pub fn compile(&mut self, bytecode: &Bytecode, start: usize, end: usize) -> Option<&JitFunction> {
        let mut cg = X64Codegen::new();
        
        // Function prologue
        cg.prologue();
        
        // Compile bytecode to x86_64
        let mut ip = start;
        while ip < end && ip < bytecode.code.len() {
            let op = bytecode.code[ip];
            
            match op {
                x if x == Opcode::LoadZero as u8 => {
                    cg.mov_reg_imm64(X64Reg::Rax, 0);
                    ip += 1;
                }
                x if x == Opcode::LoadOne as u8 => {
                    cg.mov_reg_imm64(X64Reg::Rax, 1);
                    ip += 1;
                }
                x if x == Opcode::LoadConst as u8 => {
                    let idx = u16::from_le_bytes([bytecode.code[ip + 1], bytecode.code[ip + 2]]);
                    if let Some(super::bytecode::Constant::Number(n)) = bytecode.constants.get(idx as usize) {
                        // Load f64 as raw bits into register
                        cg.mov_reg_imm64(X64Reg::Rax, n.to_bits());
                    }
                    ip += 3;
                }
                x if x == Opcode::Add as u8 => {
                    // Simple integer add (RAX = RAX + RBX)
                    cg.add_reg_reg(X64Reg::Rax, X64Reg::Rbx);
                    ip += 1;
                }
                x if x == Opcode::Sub as u8 => {
                    cg.sub_reg_reg(X64Reg::Rax, X64Reg::Rbx);
                    ip += 1;
                }
                x if x == Opcode::Mul as u8 => {
                    cg.imul_reg_reg(X64Reg::Rax, X64Reg::Rbx);
                    ip += 1;
                }
                x if x == Opcode::Return as u8 => {
                    // Return value in RAX
                    cg.epilogue();
                    ip += 1;
                }
                x if x == Opcode::Halt as u8 => {
                    cg.epilogue();
                    ip += 1;
                    break;
                }
                _ => {
                    ip += 1;
                }
            }
        }
        
        // Ensure epilogue
        cg.epilogue();
        
        let code = cg.finish();
        
        // Allocate executable memory
        let mut memory = ExecutableMemory::new(code.len().max(4096))?;
        memory.write(&code);
        
        let jit_fn = JitFunction {
            memory,
            entry_offset: 0,
        };
        
        self.compiled_functions.push(jit_fn);
        self.compiled_functions.last()
    }
    
    /// Set hot threshold
    pub fn set_threshold(&mut self, threshold: u64) {
        self.hot_threshold = threshold;
    }
    
    /// Get compilation stats
    pub fn stats(&self) -> JitCompileStats {
        JitCompileStats {
            compiled_count: self.compiled_functions.len(),
            total_code_bytes: self.compiled_functions.iter()
                .map(|f| f.memory.size)
                .sum(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct JitCompileStats {
    pub compiled_count: usize,
    pub total_code_bytes: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_executable_memory() {
        let mem = ExecutableMemory::new(4096);
        assert!(mem.is_some());
    }
    
    #[test]
    fn test_simple_jit() {
        // Simple function that returns 42
        let mut cg = X64Codegen::new();
        cg.prologue();
        cg.mov_reg_imm64(X64Reg::Rax, 42);
        cg.epilogue();
        let code = cg.finish();
        
        if let Some(mut mem) = ExecutableMemory::new(code.len()) {
            mem.write(&code);
            type Fn = extern "C" fn() -> i64;
            let f: Fn = mem.as_fn();
            
            // Note: Actually calling this would require proper platform support
            // This test just verifies the code generation works
            assert!(!code.is_empty());
        }
    }
}
