//! WebAssembly Extensions
//!
//! Implements WASM post-MVP features:
//! - SIMD (128-bit vectors)
//! - Threading (shared memory, atomics)
//! - Exception handling

use std::sync::atomic::{AtomicI32, AtomicI64, Ordering};
use std::collections::HashMap;

// =============================================================================
// SIMD Operations (128-bit vectors)
// =============================================================================

/// SIMD v128 value (128-bit vector)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C, align(16))]
pub struct V128([u8; 16]);

impl V128 {
    pub const ZERO: V128 = V128([0; 16]);

    /// Create from bytes
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Get as bytes
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Create from i32x4
    pub fn from_i32x4(a: i32, b: i32, c: i32, d: i32) -> Self {
        let mut bytes = [0u8; 16];
        bytes[0..4].copy_from_slice(&a.to_le_bytes());
        bytes[4..8].copy_from_slice(&b.to_le_bytes());
        bytes[8..12].copy_from_slice(&c.to_le_bytes());
        bytes[12..16].copy_from_slice(&d.to_le_bytes());
        Self(bytes)
    }

    /// Extract as i32x4
    pub fn as_i32x4(&self) -> [i32; 4] {
        [
            i32::from_le_bytes([self.0[0], self.0[1], self.0[2], self.0[3]]),
            i32::from_le_bytes([self.0[4], self.0[5], self.0[6], self.0[7]]),
            i32::from_le_bytes([self.0[8], self.0[9], self.0[10], self.0[11]]),
            i32::from_le_bytes([self.0[12], self.0[13], self.0[14], self.0[15]]),
        ]
    }

    /// Create from f32x4
    pub fn from_f32x4(a: f32, b: f32, c: f32, d: f32) -> Self {
        let mut bytes = [0u8; 16];
        bytes[0..4].copy_from_slice(&a.to_le_bytes());
        bytes[4..8].copy_from_slice(&b.to_le_bytes());
        bytes[8..12].copy_from_slice(&c.to_le_bytes());
        bytes[12..16].copy_from_slice(&d.to_le_bytes());
        Self(bytes)
    }

    /// Extract as f32x4
    pub fn as_f32x4(&self) -> [f32; 4] {
        [
            f32::from_le_bytes([self.0[0], self.0[1], self.0[2], self.0[3]]),
            f32::from_le_bytes([self.0[4], self.0[5], self.0[6], self.0[7]]),
            f32::from_le_bytes([self.0[8], self.0[9], self.0[10], self.0[11]]),
            f32::from_le_bytes([self.0[12], self.0[13], self.0[14], self.0[15]]),
        ]
    }

    /// Create from i64x2
    pub fn from_i64x2(a: i64, b: i64) -> Self {
        let mut bytes = [0u8; 16];
        bytes[0..8].copy_from_slice(&a.to_le_bytes());
        bytes[8..16].copy_from_slice(&b.to_le_bytes());
        Self(bytes)
    }

    /// Extract as i64x2
    pub fn as_i64x2(&self) -> [i64; 2] {
        [
            i64::from_le_bytes(self.0[0..8].try_into().unwrap()),
            i64::from_le_bytes(self.0[8..16].try_into().unwrap()),
        ]
    }

    /// Create from f64x2
    pub fn from_f64x2(a: f64, b: f64) -> Self {
        let mut bytes = [0u8; 16];
        bytes[0..8].copy_from_slice(&a.to_le_bytes());
        bytes[8..16].copy_from_slice(&b.to_le_bytes());
        Self(bytes)
    }

    /// Extract as f64x2
    pub fn as_f64x2(&self) -> [f64; 2] {
        [
            f64::from_le_bytes(self.0[0..8].try_into().unwrap()),
            f64::from_le_bytes(self.0[8..16].try_into().unwrap()),
        ]
    }

    /// Splat i32 to all lanes
    pub fn splat_i32(v: i32) -> Self {
        Self::from_i32x4(v, v, v, v)
    }

    /// Splat f32 to all lanes
    pub fn splat_f32(v: f32) -> Self {
        Self::from_f32x4(v, v, v, v)
    }
}

impl Default for V128 {
    fn default() -> Self {
        Self::ZERO
    }
}

/// SIMD operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SimdOp {
    // Load/Store
    V128Load = 0,
    V128Store = 1,
    V128Const = 2,

    // i8x16 operations
    I8x16Splat = 10,
    I8x16ExtractLaneS = 11,
    I8x16ExtractLaneU = 12,
    I8x16ReplaceLane = 13,
    I8x16Add = 14,
    I8x16Sub = 15,
    I8x16Neg = 16,
    I8x16Eq = 17,
    I8x16Ne = 18,
    I8x16AllTrue = 19,

    // i16x8 operations
    I16x8Splat = 30,
    I16x8ExtractLaneS = 31,
    I16x8ExtractLaneU = 32,
    I16x8ReplaceLane = 33,
    I16x8Add = 34,
    I16x8Sub = 35,
    I16x8Mul = 36,
    I16x8Neg = 37,

    // i32x4 operations
    I32x4Splat = 50,
    I32x4ExtractLane = 51,
    I32x4ReplaceLane = 52,
    I32x4Add = 53,
    I32x4Sub = 54,
    I32x4Mul = 55,
    I32x4Neg = 56,
    I32x4Eq = 57,
    I32x4Ne = 58,
    I32x4LtS = 59,
    I32x4GtS = 60,
    I32x4LeS = 61,
    I32x4GeS = 62,
    I32x4Shl = 63,
    I32x4ShrS = 64,
    I32x4ShrU = 65,

    // i64x2 operations
    I64x2Splat = 80,
    I64x2ExtractLane = 81,
    I64x2ReplaceLane = 82,
    I64x2Add = 83,
    I64x2Sub = 84,
    I64x2Mul = 85,
    I64x2Neg = 86,

    // f32x4 operations
    F32x4Splat = 100,
    F32x4ExtractLane = 101,
    F32x4ReplaceLane = 102,
    F32x4Add = 103,
    F32x4Sub = 104,
    F32x4Mul = 105,
    F32x4Div = 106,
    F32x4Neg = 107,
    F32x4Abs = 108,
    F32x4Sqrt = 109,
    F32x4Min = 110,
    F32x4Max = 111,
    F32x4Eq = 112,
    F32x4Ne = 113,
    F32x4Lt = 114,
    F32x4Gt = 115,
    F32x4Le = 116,
    F32x4Ge = 117,

    // f64x2 operations
    F64x2Splat = 130,
    F64x2ExtractLane = 131,
    F64x2ReplaceLane = 132,
    F64x2Add = 133,
    F64x2Sub = 134,
    F64x2Mul = 135,
    F64x2Div = 136,
    F64x2Neg = 137,
    F64x2Abs = 138,
    F64x2Sqrt = 139,

    // Bitwise operations
    V128And = 150,
    V128Or = 151,
    V128Xor = 152,
    V128Not = 153,
    V128AndNot = 154,
    V128Bitselect = 155,
    V128AnyTrue = 156,

    // Shuffle/swizzle
    I8x16Shuffle = 170,
    I8x16Swizzle = 171,

    // Conversions
    I32x4TruncSatF32x4S = 180,
    I32x4TruncSatF32x4U = 181,
    F32x4ConvertI32x4S = 182,
    F32x4ConvertI32x4U = 183,
}

/// Execute SIMD operation
pub fn execute_simd(op: SimdOp, a: V128, b: V128) -> V128 {
    match op {
        SimdOp::I32x4Add => {
            let a = a.as_i32x4();
            let b = b.as_i32x4();
            V128::from_i32x4(
                a[0].wrapping_add(b[0]),
                a[1].wrapping_add(b[1]),
                a[2].wrapping_add(b[2]),
                a[3].wrapping_add(b[3]),
            )
        }
        SimdOp::I32x4Sub => {
            let a = a.as_i32x4();
            let b = b.as_i32x4();
            V128::from_i32x4(
                a[0].wrapping_sub(b[0]),
                a[1].wrapping_sub(b[1]),
                a[2].wrapping_sub(b[2]),
                a[3].wrapping_sub(b[3]),
            )
        }
        SimdOp::I32x4Mul => {
            let a = a.as_i32x4();
            let b = b.as_i32x4();
            V128::from_i32x4(
                a[0].wrapping_mul(b[0]),
                a[1].wrapping_mul(b[1]),
                a[2].wrapping_mul(b[2]),
                a[3].wrapping_mul(b[3]),
            )
        }
        SimdOp::F32x4Add => {
            let a = a.as_f32x4();
            let b = b.as_f32x4();
            V128::from_f32x4(a[0] + b[0], a[1] + b[1], a[2] + b[2], a[3] + b[3])
        }
        SimdOp::F32x4Sub => {
            let a = a.as_f32x4();
            let b = b.as_f32x4();
            V128::from_f32x4(a[0] - b[0], a[1] - b[1], a[2] - b[2], a[3] - b[3])
        }
        SimdOp::F32x4Mul => {
            let a = a.as_f32x4();
            let b = b.as_f32x4();
            V128::from_f32x4(a[0] * b[0], a[1] * b[1], a[2] * b[2], a[3] * b[3])
        }
        SimdOp::F32x4Div => {
            let a = a.as_f32x4();
            let b = b.as_f32x4();
            V128::from_f32x4(a[0] / b[0], a[1] / b[1], a[2] / b[2], a[3] / b[3])
        }
        SimdOp::V128And => {
            let mut result = [0u8; 16];
            for i in 0..16 {
                result[i] = a.0[i] & b.0[i];
            }
            V128(result)
        }
        SimdOp::V128Or => {
            let mut result = [0u8; 16];
            for i in 0..16 {
                result[i] = a.0[i] | b.0[i];
            }
            V128(result)
        }
        SimdOp::V128Xor => {
            let mut result = [0u8; 16];
            for i in 0..16 {
                result[i] = a.0[i] ^ b.0[i];
            }
            V128(result)
        }
        _ => V128::ZERO, // Unimplemented ops return zero
    }
}

// =============================================================================
// Threading Support
// =============================================================================

/// Shared memory for WASM threads
#[derive(Debug)]
pub struct SharedMemory {
    /// Shared buffer (must be accessed atomically)
    data: Vec<AtomicI32>,
    /// Size in pages
    pages: u32,
    /// Maximum pages
    max_pages: Option<u32>,
}

impl SharedMemory {
    /// Page size (64KB)
    pub const PAGE_SIZE: usize = 65536;

    pub fn new(initial_pages: u32, max_pages: Option<u32>) -> Self {
        let size = initial_pages as usize * Self::PAGE_SIZE / 4; // i32 granularity
        let mut data = Vec::with_capacity(size);
        for _ in 0..size {
            data.push(AtomicI32::new(0));
        }
        Self {
            data,
            pages: initial_pages,
            max_pages,
        }
    }

    /// Get size in pages
    pub fn size(&self) -> u32 {
        self.pages
    }

    /// Grow memory
    pub fn grow(&mut self, delta: u32) -> i32 {
        let new_pages = self.pages + delta;
        if let Some(max) = self.max_pages {
            if new_pages > max {
                return -1;
            }
        }
        let old_size = self.pages as i32;
        let new_size = new_pages as usize * Self::PAGE_SIZE / 4;
        self.data.resize_with(new_size, || AtomicI32::new(0));
        self.pages = new_pages;
        old_size
    }

    /// Atomic load
    pub fn atomic_load(&self, addr: u32) -> i32 {
        let idx = addr as usize / 4;
        self.data.get(idx)
            .map(|a| a.load(Ordering::SeqCst))
            .unwrap_or(0)
    }

    /// Atomic store
    pub fn atomic_store(&self, addr: u32, value: i32) {
        let idx = addr as usize / 4;
        if let Some(a) = self.data.get(idx) {
            a.store(value, Ordering::SeqCst);
        }
    }

    /// Atomic add
    pub fn atomic_add(&self, addr: u32, value: i32) -> i32 {
        let idx = addr as usize / 4;
        self.data.get(idx)
            .map(|a| a.fetch_add(value, Ordering::SeqCst))
            .unwrap_or(0)
    }

    /// Atomic sub
    pub fn atomic_sub(&self, addr: u32, value: i32) -> i32 {
        let idx = addr as usize / 4;
        self.data.get(idx)
            .map(|a| a.fetch_sub(value, Ordering::SeqCst))
            .unwrap_or(0)
    }

    /// Atomic and
    pub fn atomic_and(&self, addr: u32, value: i32) -> i32 {
        let idx = addr as usize / 4;
        self.data.get(idx)
            .map(|a| a.fetch_and(value, Ordering::SeqCst))
            .unwrap_or(0)
    }

    /// Atomic or
    pub fn atomic_or(&self, addr: u32, value: i32) -> i32 {
        let idx = addr as usize / 4;
        self.data.get(idx)
            .map(|a| a.fetch_or(value, Ordering::SeqCst))
            .unwrap_or(0)
    }

    /// Atomic xor
    pub fn atomic_xor(&self, addr: u32, value: i32) -> i32 {
        let idx = addr as usize / 4;
        self.data.get(idx)
            .map(|a| a.fetch_xor(value, Ordering::SeqCst))
            .unwrap_or(0)
    }

    /// Atomic compare-exchange
    pub fn atomic_cmpxchg(&self, addr: u32, expected: i32, replacement: i32) -> i32 {
        let idx = addr as usize / 4;
        self.data.get(idx)
            .map(|a| {
                match a.compare_exchange(expected, replacement, Ordering::SeqCst, Ordering::SeqCst) {
                    Ok(v) | Err(v) => v
                }
            })
            .unwrap_or(0)
    }
}

/// Thread manager for WASM threads
#[derive(Debug, Default)]
pub struct ThreadManager {
    /// Active threads
    threads: HashMap<u32, ThreadState>,
    /// Next thread ID
    next_thread_id: u32,
    /// Wait queues (address -> waiting threads)
    wait_queues: HashMap<u32, Vec<u32>>,
}

/// Thread state
#[derive(Debug)]
pub struct ThreadState {
    /// Thread ID
    pub id: u32,
    /// Execution state
    pub state: ThreadExecutionState,
    /// Call stack
    pub call_stack: Vec<u32>,
    /// Value stack
    pub value_stack: Vec<i64>,
}

/// Thread execution state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadExecutionState {
    Running,
    Waiting,
    Blocked,
    Terminated,
}

impl ThreadManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Spawn new thread
    pub fn spawn(&mut self, entry_func: u32) -> u32 {
        let id = self.next_thread_id;
        self.next_thread_id += 1;
        
        self.threads.insert(id, ThreadState {
            id,
            state: ThreadExecutionState::Running,
            call_stack: vec![entry_func],
            value_stack: Vec::new(),
        });
        
        id
    }

    /// Get thread state
    pub fn get_thread(&self, id: u32) -> Option<&ThreadState> {
        self.threads.get(&id)
    }

    /// Wait on address
    pub fn wait(&mut self, thread_id: u32, addr: u32) {
        if let Some(thread) = self.threads.get_mut(&thread_id) {
            thread.state = ThreadExecutionState::Waiting;
        }
        self.wait_queues.entry(addr).or_default().push(thread_id);
    }

    /// Notify waiters on address
    pub fn notify(&mut self, addr: u32, count: u32) -> u32 {
        let mut notified = 0;
        
        if let Some(waiters) = self.wait_queues.get_mut(&addr) {
            while notified < count && !waiters.is_empty() {
                if let Some(thread_id) = waiters.pop() {
                    if let Some(thread) = self.threads.get_mut(&thread_id) {
                        thread.state = ThreadExecutionState::Running;
                        notified += 1;
                    }
                }
            }
        }
        
        notified
    }

    /// Terminate thread
    pub fn terminate(&mut self, id: u32) {
        if let Some(thread) = self.threads.get_mut(&id) {
            thread.state = ThreadExecutionState::Terminated;
        }
    }

    /// Get active thread count
    pub fn active_count(&self) -> usize {
        self.threads.values()
            .filter(|t| t.state == ThreadExecutionState::Running)
            .count()
    }
}

// =============================================================================
// Exception Handling
// =============================================================================

/// WASM exception tag
#[derive(Debug, Clone)]
pub struct ExceptionTag {
    /// Tag ID
    pub id: u32,
    /// Tag name
    pub name: Option<String>,
    /// Payload types
    pub payload_types: Vec<WasmValType>,
}

/// WASM value type (simplified)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmValType {
    I32,
    I64,
    F32,
    F64,
    V128,
    FuncRef,
    ExternRef,
}

/// WASM exception instance
#[derive(Debug, Clone)]
pub struct WasmException {
    /// Exception tag
    pub tag_id: u32,
    /// Payload values
    pub payload: Vec<WasmValue>,
}

/// WASM value for exception payload
#[derive(Debug, Clone)]
pub enum WasmValue {
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    V128(V128),
    FuncRef(Option<u32>),
    ExternRef(Option<u32>),
}

/// Exception handler
#[derive(Debug, Clone)]
pub struct ExceptionHandler {
    /// Label to jump to on catch
    pub catch_label: u32,
    /// Tags this handler catches (empty = catch_all)
    pub catch_tags: Vec<u32>,
    /// Whether this is catch_all
    pub is_catch_all: bool,
    /// Delegate target (for rethrow)
    pub delegate_target: Option<u32>,
}

/// Exception handling runtime
#[derive(Debug, Default)]
pub struct ExceptionRuntime {
    /// Registered tags
    tags: HashMap<u32, ExceptionTag>,
    /// Next tag ID
    next_tag_id: u32,
    /// Handler stack
    handler_stack: Vec<ExceptionHandler>,
    /// Current exception (if propagating)
    current_exception: Option<WasmException>,
}

impl ExceptionRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register exception tag
    pub fn register_tag(&mut self, name: Option<String>, payload_types: Vec<WasmValType>) -> u32 {
        let id = self.next_tag_id;
        self.next_tag_id += 1;
        
        self.tags.insert(id, ExceptionTag {
            id,
            name,
            payload_types,
        });
        
        id
    }

    /// Get tag by ID
    pub fn get_tag(&self, id: u32) -> Option<&ExceptionTag> {
        self.tags.get(&id)
    }

    /// Push exception handler
    pub fn push_handler(&mut self, handler: ExceptionHandler) {
        self.handler_stack.push(handler);
    }

    /// Pop exception handler
    pub fn pop_handler(&mut self) -> Option<ExceptionHandler> {
        self.handler_stack.pop()
    }

    /// Throw exception
    pub fn throw(&mut self, tag_id: u32, payload: Vec<WasmValue>) -> Result<u32, WasmExceptionError> {
        // Validate tag exists
        if !self.tags.contains_key(&tag_id) {
            return Err(WasmExceptionError::UnknownTag(tag_id));
        }

        let exception = WasmException { tag_id, payload };
        
        // Find matching handler
        while let Some(handler) = self.handler_stack.pop() {
            if handler.is_catch_all || handler.catch_tags.contains(&tag_id) {
                self.current_exception = Some(exception);
                return Ok(handler.catch_label);
            }
        }

        // No handler found, store for propagation
        self.current_exception = Some(exception);
        Err(WasmExceptionError::Uncaught)
    }

    /// Rethrow current exception
    pub fn rethrow(&mut self) -> Result<u32, WasmExceptionError> {
        let exception = self.current_exception.take()
            .ok_or(WasmExceptionError::NoException)?;
        
        self.throw(exception.tag_id, exception.payload)
    }

    /// Get current exception
    pub fn get_exception(&self) -> Option<&WasmException> {
        self.current_exception.as_ref()
    }

    /// Clear current exception
    pub fn clear_exception(&mut self) {
        self.current_exception = None;
    }

    /// Check if handler matches tag
    pub fn handler_matches(&self, handler: &ExceptionHandler, tag_id: u32) -> bool {
        handler.is_catch_all || handler.catch_tags.contains(&tag_id)
    }
}

/// Exception error
#[derive(Debug, Clone)]
pub enum WasmExceptionError {
    UnknownTag(u32),
    Uncaught,
    NoException,
    PayloadMismatch,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_v128_i32x4() {
        let v = V128::from_i32x4(1, 2, 3, 4);
        assert_eq!(v.as_i32x4(), [1, 2, 3, 4]);
    }

    #[test]
    fn test_v128_f32x4() {
        let v = V128::from_f32x4(1.0, 2.0, 3.0, 4.0);
        assert_eq!(v.as_f32x4(), [1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn test_simd_i32x4_add() {
        let a = V128::from_i32x4(1, 2, 3, 4);
        let b = V128::from_i32x4(10, 20, 30, 40);
        let result = execute_simd(SimdOp::I32x4Add, a, b);
        assert_eq!(result.as_i32x4(), [11, 22, 33, 44]);
    }

    #[test]
    fn test_simd_f32x4_mul() {
        let a = V128::from_f32x4(1.0, 2.0, 3.0, 4.0);
        let b = V128::from_f32x4(2.0, 2.0, 2.0, 2.0);
        let result = execute_simd(SimdOp::F32x4Mul, a, b);
        assert_eq!(result.as_f32x4(), [2.0, 4.0, 6.0, 8.0]);
    }

    #[test]
    fn test_shared_memory_atomics() {
        let mem = SharedMemory::new(1, None);
        
        mem.atomic_store(0, 42);
        assert_eq!(mem.atomic_load(0), 42);
        
        let old = mem.atomic_add(0, 8);
        assert_eq!(old, 42);
        assert_eq!(mem.atomic_load(0), 50);
    }

    #[test]
    fn test_shared_memory_cmpxchg() {
        let mem = SharedMemory::new(1, None);
        
        mem.atomic_store(0, 100);
        
        // Successful exchange
        let result = mem.atomic_cmpxchg(0, 100, 200);
        assert_eq!(result, 100);
        assert_eq!(mem.atomic_load(0), 200);
        
        // Failed exchange
        let result = mem.atomic_cmpxchg(0, 100, 300);
        assert_eq!(result, 200); // Returns current value
        assert_eq!(mem.atomic_load(0), 200); // Unchanged
    }

    #[test]
    fn test_thread_manager() {
        let mut tm = ThreadManager::new();
        
        let t1 = tm.spawn(0);
        let t2 = tm.spawn(1);
        
        assert_eq!(tm.active_count(), 2);
        
        tm.terminate(t1);
        assert_eq!(tm.active_count(), 1);
    }

    #[test]
    fn test_exception_throw_catch() {
        let mut runtime = ExceptionRuntime::new();
        
        let tag = runtime.register_tag(Some("Error".into()), vec![WasmValType::I32]);
        
        runtime.push_handler(ExceptionHandler {
            catch_label: 100,
            catch_tags: vec![tag],
            is_catch_all: false,
            delegate_target: None,
        });
        
        let result = runtime.throw(tag, vec![WasmValue::I32(42)]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 100);
    }

    #[test]
    fn test_exception_catch_all() {
        let mut runtime = ExceptionRuntime::new();
        
        let tag = runtime.register_tag(None, vec![]);
        
        runtime.push_handler(ExceptionHandler {
            catch_label: 50,
            catch_tags: vec![],
            is_catch_all: true,
            delegate_target: None,
        });
        
        let result = runtime.throw(tag, vec![]);
        assert_eq!(result.unwrap(), 50);
    }

    #[test]
    fn test_wait_notify() {
        let mut tm = ThreadManager::new();
        
        let t1 = tm.spawn(0);
        let t2 = tm.spawn(1);
        
        tm.wait(t1, 0x100);
        tm.wait(t2, 0x100);
        
        assert_eq!(tm.get_thread(t1).unwrap().state, ThreadExecutionState::Waiting);
        
        let notified = tm.notify(0x100, 1);
        assert_eq!(notified, 1);
    }
}
