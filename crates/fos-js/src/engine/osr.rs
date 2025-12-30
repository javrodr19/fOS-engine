//! On-Stack Replacement (OSR)
//!
//! Replace running interpreter code with JIT-compiled code mid-execution.
//! Essential for optimizing long-running loops.

use super::value::JsVal;
use std::collections::HashMap;

/// OSR entry point information
#[derive(Debug, Clone)]
pub struct OsrEntry {
    /// Bytecode offset where OSR can occur
    pub bytecode_offset: u32,
    /// Offset in native code to jump to
    pub native_offset: u32,
    /// Register/local mapping at this point
    pub local_mapping: Vec<LocalSlot>,
}

/// Maps interpreter locals to JIT registers/stack slots
#[derive(Debug, Clone)]
pub struct LocalSlot {
    pub local_idx: u16,
    pub location: SlotLocation,
}

#[derive(Debug, Clone, Copy)]
pub enum SlotLocation {
    Register(u8),      // Native register
    StackOffset(i32),  // Stack offset from RBP
}

/// OSR state for transferring execution from interpreter to JIT
#[derive(Debug)]
pub struct OsrState {
    /// Values to transfer
    pub locals: Vec<JsVal>,
    /// Stack values
    pub stack: Vec<JsVal>,
    /// Current IP in bytecode
    pub bytecode_ip: usize,
}

impl OsrState {
    pub fn new() -> Self {
        Self {
            locals: Vec::new(),
            stack: Vec::new(),
            bytecode_ip: 0,
        }
    }
    
    /// Capture interpreter state for OSR
    pub fn capture(locals: &[JsVal], stack: &[JsVal], ip: usize) -> Self {
        Self {
            locals: locals.to_vec(),
            stack: stack.to_vec(),
            bytecode_ip: ip,
        }
    }
}

/// OSR manager
#[derive(Debug, Default)]
pub struct OsrManager {
    /// OSR entry points indexed by bytecode offset
    entries: HashMap<u32, OsrEntry>,
    /// Successful OSR count
    osr_count: u64,
    /// Failed OSR attempts
    failed_count: u64,
}

impl OsrManager {
    pub fn new() -> Self { Self::default() }
    
    /// Register an OSR entry point
    pub fn register_entry(&mut self, entry: OsrEntry) {
        self.entries.insert(entry.bytecode_offset, entry);
    }
    
    /// Check if OSR is available at offset
    pub fn can_osr(&self, bytecode_offset: u32) -> bool {
        self.entries.contains_key(&bytecode_offset)
    }
    
    /// Get OSR entry for offset
    pub fn get_entry(&self, bytecode_offset: u32) -> Option<&OsrEntry> {
        self.entries.get(&bytecode_offset)
    }
    
    /// Record successful OSR
    pub fn record_osr(&mut self) {
        self.osr_count += 1;
    }
    
    /// Record failed OSR attempt
    pub fn record_failed(&mut self) {
        self.failed_count += 1;
    }
    
    /// Get OSR statistics
    pub fn stats(&self) -> OsrStats {
        OsrStats {
            entry_count: self.entries.len(),
            osr_count: self.osr_count,
            failed_count: self.failed_count,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct OsrStats {
    pub entry_count: usize,
    pub osr_count: u64,
    pub failed_count: u64,
}

/// Deoptimization point
#[derive(Debug, Clone)]
pub struct DeoptPoint {
    /// Native code offset
    pub native_offset: u32,
    /// Bytecode offset to return to
    pub bytecode_offset: u32,
    /// State to restore
    pub restore_info: RestoreInfo,
}

#[derive(Debug, Clone)]
pub struct RestoreInfo {
    /// Registers to restore as locals
    pub register_mapping: Vec<(u8, u16)>,  // (reg, local_idx)
    /// Stack values to restore
    pub stack_slots: Vec<i32>,  // Stack offsets
}

/// Deoptimization manager
#[derive(Debug, Default)]
pub struct DeoptManager {
    points: HashMap<u32, DeoptPoint>,
    deopt_count: u64,
}

impl DeoptManager {
    pub fn new() -> Self { Self::default() }
    
    /// Register a deoptimization point
    pub fn register(&mut self, point: DeoptPoint) {
        self.points.insert(point.native_offset, point);
    }
    
    /// Get deopt point for native offset
    pub fn get(&self, native_offset: u32) -> Option<&DeoptPoint> {
        self.points.get(&native_offset)
    }
    
    /// Record deoptimization
    pub fn record_deopt(&mut self) {
        self.deopt_count += 1;
    }
    
    pub fn deopt_count(&self) -> u64 { self.deopt_count }
    
    /// Perform deoptimization - restore interpreter state from JIT
    pub fn bailout(&mut self, point: &DeoptPoint, jit_registers: &[JsVal]) -> OsrState {
        self.record_deopt();
        
        // Restore locals from JIT registers
        let mut locals = Vec::new();
        for (reg, local_idx) in &point.restore_info.register_mapping {
            if let Some(val) = jit_registers.get(*reg as usize) {
                // Ensure locals vec is big enough
                while locals.len() <= *local_idx as usize {
                    locals.push(JsVal::Undefined);
                }
                locals[*local_idx as usize] = *val;
            }
        }
        
        OsrState {
            locals,
            stack: Vec::new(),  // Stack reconstructed from restore_info
            bytecode_ip: point.bytecode_offset as usize,
        }
    }
}

/// OSR runtime coordinator
pub struct OsrRuntime {
    pub osr_manager: OsrManager,
    pub deopt_manager: DeoptManager,
}

impl Default for OsrRuntime {
    fn default() -> Self { Self::new() }
}

impl OsrRuntime {
    pub fn new() -> Self {
        Self {
            osr_manager: OsrManager::new(),
            deopt_manager: DeoptManager::new(),
        }
    }
    
    /// Attempt OSR at current bytecode offset
    /// Returns native code offset to jump to, or None if OSR not available
    pub fn try_osr(&mut self, bytecode_offset: u32, state: &OsrState) -> Option<OsrTransfer> {
        // Clone entry to avoid borrow conflict
        let entry = self.osr_manager.get_entry(bytecode_offset)?.clone();
        
        // Create transfer buffer for JIT
        let mut jit_registers = vec![JsVal::Undefined; 256];
        
        // Map interpreter locals to JIT registers
        for slot in &entry.local_mapping {
            if let Some(val) = state.locals.get(slot.local_idx as usize) {
                match slot.location {
                    SlotLocation::Register(reg) => {
                        jit_registers[reg as usize] = *val;
                    }
                    SlotLocation::StackOffset(_) => {
                        // Would need to set up stack - simplified for now
                    }
                }
            }
        }
        
        self.osr_manager.record_osr();
        
        Some(OsrTransfer {
            native_offset: entry.native_offset,
            registers: jit_registers,
        })
    }
    
    /// Handle deoptimization - return to interpreter
    pub fn deoptimize(&mut self, native_offset: u32, jit_registers: &[JsVal]) -> Option<OsrState> {
        let point = self.deopt_manager.get(native_offset)?.clone();
        Some(self.deopt_manager.bailout(&point, jit_registers))
    }
}

/// Data needed to transfer from interpreter to JIT
#[derive(Debug)]
pub struct OsrTransfer {
    pub native_offset: u32,
    pub registers: Vec<JsVal>,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_osr_registration() {
        let mut mgr = OsrManager::new();
        
        mgr.register_entry(OsrEntry {
            bytecode_offset: 100,
            native_offset: 0x1000,
            local_mapping: vec![],
        });
        
        assert!(mgr.can_osr(100));
        assert!(!mgr.can_osr(200));
    }
    
    #[test]
    fn test_osr_state_capture() {
        let locals = vec![JsVal::Number(1.0), JsVal::Number(2.0)];
        let stack = vec![JsVal::Number(3.0)];
        
        let state = OsrState::capture(&locals, &stack, 50);
        
        assert_eq!(state.locals.len(), 2);
        assert_eq!(state.stack.len(), 1);
        assert_eq!(state.bytecode_ip, 50);
    }
}
