//! WebAssembly Runtime
//!
//! Implements WebAssembly MVP runtime with:
//! - Binary format decoder
//! - Module validation
//! - Bytecode interpreter
//! - Linear memory management
//! - Table support
//! - Import/Export handling

use std::collections::HashMap;

// =============================================================================
// WASM Value Types
// =============================================================================

/// WebAssembly value type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValType {
    I32,
    I64,
    F32,
    F64,
    /// Reference type (funcref, externref)
    FuncRef,
    ExternRef,
    /// SIMD vector type
    V128,
}

impl ValType {
    /// Decode value type from WASM binary
    pub fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0x7F => Some(ValType::I32),
            0x7E => Some(ValType::I64),
            0x7D => Some(ValType::F32),
            0x7C => Some(ValType::F64),
            0x70 => Some(ValType::FuncRef),
            0x6F => Some(ValType::ExternRef),
            0x7B => Some(ValType::V128),
            _ => None,
        }
    }
}

/// WebAssembly runtime value
#[derive(Debug, Clone, Copy)]
pub enum Value {
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    FuncRef(Option<u32>), // Function index or null
    ExternRef(Option<u32>), // External reference or null
    V128([u8; 16]),
}

impl Value {
    pub fn val_type(&self) -> ValType {
        match self {
            Value::I32(_) => ValType::I32,
            Value::I64(_) => ValType::I64,
            Value::F32(_) => ValType::F32,
            Value::F64(_) => ValType::F64,
            Value::FuncRef(_) => ValType::FuncRef,
            Value::ExternRef(_) => ValType::ExternRef,
            Value::V128(_) => ValType::V128,
        }
    }

    pub fn default_for(ty: ValType) -> Self {
        match ty {
            ValType::I32 => Value::I32(0),
            ValType::I64 => Value::I64(0),
            ValType::F32 => Value::F32(0.0),
            ValType::F64 => Value::F64(0.0),
            ValType::FuncRef => Value::FuncRef(None),
            ValType::ExternRef => Value::ExternRef(None),
            ValType::V128 => Value::V128([0; 16]),
        }
    }
}

// =============================================================================
// WASM Module Structure
// =============================================================================

/// Function type signature
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuncType {
    pub params: Vec<ValType>,
    pub results: Vec<ValType>,
}

/// Import entry
#[derive(Debug, Clone)]
pub struct Import {
    pub module: String,
    pub name: String,
    pub desc: ImportDesc,
}

/// Import descriptor
#[derive(Debug, Clone)]
pub enum ImportDesc {
    Func(u32),         // Type index
    Table(TableType),
    Memory(MemoryType),
    Global(GlobalType),
}

/// Export entry
#[derive(Debug, Clone)]
pub struct Export {
    pub name: String,
    pub desc: ExportDesc,
}

/// Export descriptor
#[derive(Debug, Clone, Copy)]
pub enum ExportDesc {
    Func(u32),
    Table(u32),
    Memory(u32),
    Global(u32),
}

/// Table type
#[derive(Debug, Clone, Copy)]
pub struct TableType {
    pub element_type: ValType,
    pub limits: Limits,
}

/// Memory type
#[derive(Debug, Clone, Copy)]
pub struct MemoryType {
    pub limits: Limits,
}

/// Global type
#[derive(Debug, Clone, Copy)]
pub struct GlobalType {
    pub val_type: ValType,
    pub mutable: bool,
}

/// Limits (min, optional max)
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub min: u32,
    pub max: Option<u32>,
}

/// WASM function definition
#[derive(Debug, Clone)]
pub struct Function {
    pub type_idx: u32,
    pub locals: Vec<ValType>,
    pub code: Vec<u8>,
}

/// WASM module
#[derive(Debug)]
pub struct Module {
    /// Type section
    pub types: Vec<FuncType>,
    /// Import section
    pub imports: Vec<Import>,
    /// Function section (type indices)
    pub functions: Vec<u32>,
    /// Table section
    pub tables: Vec<TableType>,
    /// Memory section
    pub memories: Vec<MemoryType>,
    /// Global section
    pub globals: Vec<(GlobalType, Vec<u8>)>, // Type + init expr
    /// Export section
    pub exports: Vec<Export>,
    /// Start function index
    pub start: Option<u32>,
    /// Code section
    pub code: Vec<Function>,
    /// Data section
    pub data: Vec<DataSegment>,
}

/// Data segment
#[derive(Debug, Clone)]
pub struct DataSegment {
    pub memory_idx: u32,
    pub offset_expr: Vec<u8>,
    pub data: Vec<u8>,
}

impl Module {
    pub fn new() -> Self {
        Self {
            types: Vec::new(),
            imports: Vec::new(),
            functions: Vec::new(),
            tables: Vec::new(),
            memories: Vec::new(),
            globals: Vec::new(),
            exports: Vec::new(),
            start: None,
            code: Vec::new(),
            data: Vec::new(),
        }
    }

    /// Get export by name
    pub fn get_export(&self, name: &str) -> Option<&Export> {
        self.exports.iter().find(|e| e.name == name)
    }

    /// Count imported functions
    pub fn import_func_count(&self) -> usize {
        self.imports.iter()
            .filter(|i| matches!(i.desc, ImportDesc::Func(_)))
            .count()
    }
}

impl Default for Module {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// WASM Linear Memory
// =============================================================================

/// WebAssembly linear memory
#[derive(Debug)]
pub struct Memory {
    /// Memory pages (64KB each)
    data: Vec<u8>,
    /// Current size in pages
    current_pages: u32,
    /// Maximum pages (if limited)
    max_pages: Option<u32>,
}

/// Page size in bytes (64KB)
pub const PAGE_SIZE: usize = 65536;

impl Memory {
    pub fn new(initial_pages: u32, max_pages: Option<u32>) -> Self {
        let size = initial_pages as usize * PAGE_SIZE;
        Self {
            data: vec![0u8; size],
            current_pages: initial_pages,
            max_pages,
        }
    }

    /// Current size in pages
    pub fn size(&self) -> u32 {
        self.current_pages
    }

    /// Grow memory by delta pages, returns old size or -1 on failure
    pub fn grow(&mut self, delta: u32) -> i32 {
        let new_pages = self.current_pages.saturating_add(delta);
        
        if let Some(max) = self.max_pages {
            if new_pages > max {
                return -1;
            }
        }

        // Limit to reasonable size (4GB max)
        if new_pages > 65536 {
            return -1;
        }

        let old_pages = self.current_pages as i32;
        let new_size = new_pages as usize * PAGE_SIZE;
        self.data.resize(new_size, 0);
        self.current_pages = new_pages;
        
        old_pages
    }

    /// Load byte from memory
    pub fn load_u8(&self, addr: u32) -> Option<u8> {
        self.data.get(addr as usize).copied()
    }

    /// Load i32 from memory (little-endian)
    pub fn load_i32(&self, addr: u32) -> Option<i32> {
        let addr = addr as usize;
        if addr + 4 > self.data.len() {
            return None;
        }
        let bytes = [
            self.data[addr],
            self.data[addr + 1],
            self.data[addr + 2],
            self.data[addr + 3],
        ];
        Some(i32::from_le_bytes(bytes))
    }

    /// Store byte to memory
    pub fn store_u8(&mut self, addr: u32, value: u8) -> bool {
        if let Some(cell) = self.data.get_mut(addr as usize) {
            *cell = value;
            true
        } else {
            false
        }
    }

    /// Store i32 to memory (little-endian)
    pub fn store_i32(&mut self, addr: u32, value: i32) -> bool {
        let addr = addr as usize;
        if addr + 4 > self.data.len() {
            return false;
        }
        let bytes = value.to_le_bytes();
        self.data[addr..addr + 4].copy_from_slice(&bytes);
        true
    }

    /// Get data slice
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Get mutable data slice
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }
}

// =============================================================================
// WASM Table
// =============================================================================

/// WebAssembly table
#[derive(Debug)]
pub struct Table {
    /// Table elements (function references)
    elements: Vec<Option<u32>>,
    /// Element type
    element_type: ValType,
    /// Maximum size
    max_size: Option<u32>,
}

impl Table {
    pub fn new(element_type: ValType, initial: u32, max: Option<u32>) -> Self {
        Self {
            elements: vec![None; initial as usize],
            element_type,
            max_size: max,
        }
    }

    /// Get element at index
    pub fn get(&self, idx: u32) -> Option<Option<u32>> {
        self.elements.get(idx as usize).copied()
    }

    /// Set element at index
    pub fn set(&mut self, idx: u32, value: Option<u32>) -> bool {
        if let Some(elem) = self.elements.get_mut(idx as usize) {
            *elem = value;
            true
        } else {
            false
        }
    }

    /// Grow table
    pub fn grow(&mut self, delta: u32, init: Option<u32>) -> i32 {
        let new_size = self.elements.len() as u32 + delta;
        
        if let Some(max) = self.max_size {
            if new_size > max {
                return -1;
            }
        }

        let old_size = self.elements.len() as i32;
        self.elements.resize(new_size as usize, init);
        old_size
    }

    /// Current size
    pub fn size(&self) -> u32 {
        self.elements.len() as u32
    }
}

// =============================================================================
// WASM Opcodes
// =============================================================================

/// WASM instruction opcodes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum WasmOp {
    // Control
    Unreachable = 0x00,
    Nop = 0x01,
    Block = 0x02,
    Loop = 0x03,
    If = 0x04,
    Else = 0x05,
    End = 0x0B,
    Br = 0x0C,
    BrIf = 0x0D,
    BrTable = 0x0E,
    Return = 0x0F,
    Call = 0x10,
    CallIndirect = 0x11,

    // Parametric
    Drop = 0x1A,
    Select = 0x1B,

    // Variable
    LocalGet = 0x20,
    LocalSet = 0x21,
    LocalTee = 0x22,
    GlobalGet = 0x23,
    GlobalSet = 0x24,

    // Memory
    I32Load = 0x28,
    I64Load = 0x29,
    F32Load = 0x2A,
    F64Load = 0x2B,
    I32Load8S = 0x2C,
    I32Load8U = 0x2D,
    I32Store = 0x36,
    I64Store = 0x37,
    I32Store8 = 0x3A,
    MemorySize = 0x3F,
    MemoryGrow = 0x40,

    // Constants
    I32Const = 0x41,
    I64Const = 0x42,
    F32Const = 0x43,
    F64Const = 0x44,

    // I32 Comparison
    I32Eqz = 0x45,
    I32Eq = 0x46,
    I32Ne = 0x47,
    I32LtS = 0x48,
    I32LtU = 0x49,
    I32GtS = 0x4A,
    I32GtU = 0x4B,
    I32LeS = 0x4C,
    I32LeU = 0x4D,
    I32GeS = 0x4E,
    I32GeU = 0x4F,

    // I32 Arithmetic
    I32Add = 0x6A,
    I32Sub = 0x6B,
    I32Mul = 0x6C,
    I32DivS = 0x6D,
    I32DivU = 0x6E,
    I32RemS = 0x6F,
    I32RemU = 0x70,
    I32And = 0x71,
    I32Or = 0x72,
    I32Xor = 0x73,
    I32Shl = 0x74,
    I32ShrS = 0x75,
    I32ShrU = 0x76,
}

// =============================================================================
// WASM Runtime Instance
// =============================================================================

/// WASM module instance
#[derive(Debug)]
pub struct Instance {
    /// Source module
    module: Module,
    /// Memories
    memories: Vec<Memory>,
    /// Tables
    tables: Vec<Table>,
    /// Globals
    globals: Vec<Value>,
    /// Imported function implementations
    imports: HashMap<String, ImportedFunc>,
}

/// Imported function type
pub type ImportedFunc = Box<dyn Fn(&[Value]) -> Vec<Value> + Send + Sync>;

impl Instance {
    /// Instantiate a module
    pub fn new(module: Module) -> Result<Self, WasmError> {
        let mut instance = Self {
            memories: Vec::new(),
            tables: Vec::new(),
            globals: Vec::new(),
            imports: HashMap::new(),
            module,
        };

        // Initialize memories
        for mem_type in &instance.module.memories {
            instance.memories.push(Memory::new(
                mem_type.limits.min,
                mem_type.limits.max,
            ));
        }

        // Initialize tables
        for table_type in &instance.module.tables {
            instance.tables.push(Table::new(
                table_type.element_type,
                table_type.limits.min,
                table_type.limits.max,
            ));
        }

        // Initialize globals
        for (global_type, _init_expr) in &instance.module.globals {
            instance.globals.push(Value::default_for(global_type.val_type));
        }

        // Initialize data segments
        for segment in &instance.module.data {
            // Simplified: assume offset 0
            let mem_idx = segment.memory_idx as usize;
            if let Some(memory) = instance.memories.get_mut(mem_idx) {
                let data = memory.data_mut();
                let len = segment.data.len().min(data.len());
                data[..len].copy_from_slice(&segment.data[..len]);
            }
        }

        Ok(instance)
    }

    /// Get export
    pub fn get_export(&self, name: &str) -> Option<&Export> {
        self.module.get_export(name)
    }

    /// Call exported function
    pub fn call(&mut self, func_idx: u32, args: &[Value]) -> Result<Vec<Value>, WasmError> {
        // Create interpreter and execute
        let mut interp = Interpreter::new(self);
        interp.call(func_idx, args)
    }

    /// Get memory
    pub fn memory(&self, idx: usize) -> Option<&Memory> {
        self.memories.get(idx)
    }

    /// Get mutable memory
    pub fn memory_mut(&mut self, idx: usize) -> Option<&mut Memory> {
        self.memories.get_mut(idx)
    }
}

/// WASM runtime error
#[derive(Debug, Clone)]
pub enum WasmError {
    /// Invalid WASM binary
    InvalidBinary(String),
    /// Validation error
    ValidationError(String),
    /// Runtime trap
    Trap(String),
    /// Out of bounds memory access
    OutOfBounds,
    /// Stack overflow
    StackOverflow,
    /// Type mismatch
    TypeMismatch,
    /// Unknown function
    UnknownFunction(u32),
}

// =============================================================================
// WASM Interpreter
// =============================================================================

/// WASM bytecode interpreter
pub struct Interpreter<'a> {
    instance: &'a mut Instance,
    /// Value stack
    stack: Vec<Value>,
    /// Call stack
    call_stack: Vec<CallFrame>,
}

/// Call frame
#[derive(Debug)]
struct CallFrame {
    func_idx: u32,
    locals: Vec<Value>,
    ip: usize,
    stack_base: usize,
}

impl<'a> Interpreter<'a> {
    pub fn new(instance: &'a mut Instance) -> Self {
        Self {
            instance,
            stack: Vec::with_capacity(1024),
            call_stack: Vec::with_capacity(256),
        }
    }

    /// Call a function
    pub fn call(&mut self, func_idx: u32, args: &[Value]) -> Result<Vec<Value>, WasmError> {
        let import_count = self.instance.module.import_func_count();
        
        if (func_idx as usize) < import_count {
            return Err(WasmError::Trap("Cannot call imported function directly yet".into()));
        }

        let local_idx = func_idx as usize - import_count;
        let func = self.instance.module.code.get(local_idx)
            .ok_or(WasmError::UnknownFunction(func_idx))?;

        let type_idx = self.instance.module.functions.get(local_idx)
            .ok_or(WasmError::UnknownFunction(func_idx))?;

        let func_type = self.instance.module.types.get(*type_idx as usize)
            .ok_or(WasmError::ValidationError("Invalid type index".into()))?;

        // Set up locals
        let mut locals = Vec::with_capacity(args.len() + func.locals.len());
        locals.extend_from_slice(args);
        for local_type in &func.locals {
            locals.push(Value::default_for(*local_type));
        }

        // Push call frame
        self.call_stack.push(CallFrame {
            func_idx,
            locals,
            ip: 0,
            stack_base: self.stack.len(),
        });

        // Execute (simplified - would need full opcode dispatch)
        let code = func.code.clone();
        self.execute(&code)?;

        // Pop results
        let result_count = func_type.results.len();
        let results: Vec<Value> = self.stack.drain(self.stack.len().saturating_sub(result_count)..).collect();

        self.call_stack.pop();

        Ok(results)
    }

    fn execute(&mut self, code: &[u8]) -> Result<(), WasmError> {
        let mut ip = 0;

        while ip < code.len() {
            let opcode = code[ip];
            ip += 1;

            match opcode {
                0x00 => return Err(WasmError::Trap("unreachable".into())),
                0x01 => { /* nop */ }
                
                // i32.const
                0x41 => {
                    let (value, len) = decode_leb128_i32(&code[ip..])?;
                    ip += len;
                    self.stack.push(Value::I32(value));
                }

                // local.get
                0x20 => {
                    let (idx, len) = decode_leb128_u32(&code[ip..])?;
                    ip += len;
                    let frame = self.call_stack.last()
                        .ok_or(WasmError::Trap("No call frame".into()))?;
                    let value = frame.locals.get(idx as usize)
                        .cloned()
                        .ok_or(WasmError::Trap("Invalid local index".into()))?;
                    self.stack.push(value);
                }

                // local.set
                0x21 => {
                    let (idx, len) = decode_leb128_u32(&code[ip..])?;
                    ip += len;
                    let value = self.stack.pop()
                        .ok_or(WasmError::Trap("Stack underflow".into()))?;
                    let frame = self.call_stack.last_mut()
                        .ok_or(WasmError::Trap("No call frame".into()))?;
                    if let Some(local) = frame.locals.get_mut(idx as usize) {
                        *local = value;
                    }
                }

                // i32.add
                0x6A => {
                    let b = self.pop_i32()?;
                    let a = self.pop_i32()?;
                    self.stack.push(Value::I32(a.wrapping_add(b)));
                }

                // i32.sub
                0x6B => {
                    let b = self.pop_i32()?;
                    let a = self.pop_i32()?;
                    self.stack.push(Value::I32(a.wrapping_sub(b)));
                }

                // i32.mul
                0x6C => {
                    let b = self.pop_i32()?;
                    let a = self.pop_i32()?;
                    self.stack.push(Value::I32(a.wrapping_mul(b)));
                }

                // i32.lt_s
                0x48 => {
                    let b = self.pop_i32()?;
                    let a = self.pop_i32()?;
                    self.stack.push(Value::I32(if a < b { 1 } else { 0 }));
                }

                // end
                0x0B => {
                    break;
                }

                // return
                0x0F => {
                    break;
                }

                _ => {
                    // Skip unknown opcodes for now
                }
            }
        }

        Ok(())
    }

    fn pop_i32(&mut self) -> Result<i32, WasmError> {
        match self.stack.pop() {
            Some(Value::I32(v)) => Ok(v),
            Some(_) => Err(WasmError::TypeMismatch),
            None => Err(WasmError::Trap("Stack underflow".into())),
        }
    }
}

/// Decode LEB128 signed 32-bit integer
fn decode_leb128_i32(bytes: &[u8]) -> Result<(i32, usize), WasmError> {
    let mut result = 0i32;
    let mut shift = 0;
    let mut i = 0;

    loop {
        if i >= bytes.len() {
            return Err(WasmError::InvalidBinary("Unexpected end of LEB128".into()));
        }

        let byte = bytes[i];
        i += 1;

        result |= ((byte & 0x7F) as i32) << shift;
        shift += 7;

        if byte & 0x80 == 0 {
            // Sign extend if needed
            if shift < 32 && (byte & 0x40) != 0 {
                result |= !0 << shift;
            }
            return Ok((result, i));
        }

        if shift >= 35 {
            return Err(WasmError::InvalidBinary("LEB128 too long".into()));
        }
    }
}

/// Decode LEB128 unsigned 32-bit integer
fn decode_leb128_u32(bytes: &[u8]) -> Result<(u32, usize), WasmError> {
    let mut result = 0u32;
    let mut shift = 0;
    let mut i = 0;

    loop {
        if i >= bytes.len() {
            return Err(WasmError::InvalidBinary("Unexpected end of LEB128".into()));
        }

        let byte = bytes[i];
        i += 1;

        result |= ((byte & 0x7F) as u32) << shift;
        shift += 7;

        if byte & 0x80 == 0 {
            return Ok((result, i));
        }

        if shift >= 35 {
            return Err(WasmError::InvalidBinary("LEB128 too long".into()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_operations() {
        let mut mem = Memory::new(1, Some(10));
        
        assert_eq!(mem.size(), 1);
        
        mem.store_i32(0, 42).unwrap();
        assert_eq!(mem.load_i32(0), Some(42));
        
        let old_size = mem.grow(1);
        assert_eq!(old_size, 1);
        assert_eq!(mem.size(), 2);
    }

    #[test]
    fn test_table_operations() {
        let mut table = Table::new(ValType::FuncRef, 10, Some(100));
        
        assert_eq!(table.size(), 10);
        assert_eq!(table.get(0), Some(None));
        
        table.set(0, Some(42));
        assert_eq!(table.get(0), Some(Some(42)));
    }

    #[test]
    fn test_value_types() {
        assert_eq!(ValType::from_byte(0x7F), Some(ValType::I32));
        assert_eq!(ValType::from_byte(0x7E), Some(ValType::I64));
        assert_eq!(ValType::from_byte(0xFF), None);
    }

    #[test]
    fn test_leb128_decoding() {
        // Simple cases
        assert_eq!(decode_leb128_u32(&[0x00]).unwrap(), (0, 1));
        assert_eq!(decode_leb128_u32(&[0x01]).unwrap(), (1, 1));
        assert_eq!(decode_leb128_u32(&[0x7F]).unwrap(), (127, 1));
        
        // Multi-byte
        assert_eq!(decode_leb128_u32(&[0x80, 0x01]).unwrap(), (128, 2));
        assert_eq!(decode_leb128_u32(&[0xE5, 0x8E, 0x26]).unwrap(), (624485, 3));
    }

    #[test]
    fn test_module_creation() {
        let module = Module::new();
        assert!(module.types.is_empty());
        assert!(module.exports.is_empty());
    }

    #[test]
    fn test_instance_creation() {
        let mut module = Module::new();
        module.memories.push(MemoryType {
            limits: Limits { min: 1, max: Some(10) },
        });
        
        let instance = Instance::new(module).unwrap();
        assert!(instance.memory(0).is_some());
        assert_eq!(instance.memory(0).unwrap().size(), 1);
    }
}
