//! Bytecode definitions
//!
//! Stack-based bytecode for the JavaScript VM.
//! Uses compact encoding for common operations.

/// Bytecode instruction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Opcode {
    // Stack ops
    Pop = 0,
    Dup = 1,
    
    // === COMPACT OPCODES (single byte, no operands) ===
    // Small integers 0-7 (inline in opcode)
    LoadSmallInt0 = 2,
    LoadSmallInt1 = 3,
    LoadSmallInt2 = 4,
    LoadSmallInt3 = 5,
    LoadSmallInt4 = 6,
    LoadSmallInt5 = 7,
    LoadSmallInt6 = 8,
    LoadSmallInt7 = 9,
    
    // Constants
    LoadConst = 10,      // idx: u16
    LoadNull = 11,
    LoadUndefined = 12,
    LoadTrue = 13,
    LoadFalse = 14,
    LoadZero = 15,       // Alias for LoadSmallInt0
    LoadOne = 16,        // Alias for LoadSmallInt1
    LoadMinusOne = 17,   // -1 (common)
    
    // === FAST LOCAL ACCESS (single byte for locals 0-3) ===
    GetLocal0 = 18,
    GetLocal1 = 19,
    SetLocal0 = 20,      // Note: reusing old GetLocal slot
    SetLocal1 = 21,      // Note: reusing old SetLocal slot
    
    // Variables (with operand)
    GetLocal = 22,       // idx: u16 (for locals >= 4)
    SetLocal = 23,       // idx: u16
    GetGlobal = 24,      // name_idx: u16
    SetGlobal = 25,      // name_idx: u16
    GetUpvalue = 26,     // idx: u16
    SetUpvalue = 27,     // idx: u16
    CloseUpvalue = 28,   // Close upvalue at stack top
    
    // Properties
    GetProperty = 30,    // name_idx: u16
    SetProperty = 31,    // name_idx: u16
    GetIndex = 32,
    SetIndex = 33,
    
    // Arithmetic
    Add = 40,
    Sub = 41,
    Mul = 42,
    Div = 43,
    Mod = 44,
    Pow = 45,
    Neg = 46,
    Inc = 47,            // Increment (+1, common)
    Dec = 48,            // Decrement (-1, common)
    
    // Bitwise
    BitAnd = 50,
    BitOr = 51,
    BitXor = 52,
    BitNot = 53,
    Shl = 54,
    Shr = 55,
    UShr = 56,
    
    // Comparison
    Eq = 60,
    Ne = 61,
    StrictEq = 62,
    StrictNe = 63,
    Lt = 64,
    Le = 65,
    Gt = 66,
    Ge = 67,
    
    // Logical
    Not = 70,
    
    // Jumps
    Jump = 80,           // offset: i16
    JumpIfFalse = 81,    // offset: i16
    JumpIfTrue = 82,     // offset: i16
    JumpIfFalseOrPop = 83, // Short-circuit AND
    JumpIfTrueOrPop = 84,  // Short-circuit OR
    
    // Functions
    Call = 90,           // argc: u8
    Call0 = 91,          // No args (common)
    Call1 = 92,          // 1 arg (common)
    Return = 93,
    ReturnUndefined = 94, // Common: return without value
    Closure = 95,        // const_idx: u16, upvalue_count: u8, then upvalue_info
    
    // Objects
    NewObject = 100,
    NewArray = 101,      // count: u16
    NewArray0 = 102,     // Empty array (common)
    
    // Special
    Typeof = 110,
    Instanceof = 111,
    In = 112,
    
    // Error handling
    Throw = 120,
    TryStart = 121,      // catch_offset: i16 - jump to catch block if error
    TryEnd = 122,        // Pop try handler
    
    // Prototype
    GetPrototype = 130,
    SetPrototype = 131,
    
    // Iteration
    GetIterator = 140,   // Get Symbol.iterator from object
    IteratorNext = 141,  // Call iterator.next()
    IteratorDone = 142,  // Check if iterator is done
    ForOfInit = 143,     // Initialize for-of loop
    ForInInit = 144,     // Initialize for-in loop (get keys)
    
    Halt = 255,
}

/// Compiled bytecode chunk
#[derive(Debug, Clone, Default)]
pub struct Bytecode {
    pub code: Vec<u8>,
    pub constants: Vec<Constant>,
    pub names: Vec<Box<str>>,
}

/// Constant pool entry
#[derive(Debug, Clone)]
pub enum Constant {
    Number(f64),
    String(Box<str>),
    Function(Box<CompiledFunction>),
}

/// Compiled function
#[derive(Debug, Clone)]
pub struct CompiledFunction {
    pub name: Option<Box<str>>,
    pub arity: u8,
    pub locals_count: u16,
    pub upvalue_count: u8,
    pub upvalues: Vec<UpvalueInfo>,
    pub bytecode: Bytecode,
}

/// Upvalue capture info
#[derive(Debug, Clone, Copy)]
pub struct UpvalueInfo {
    pub index: u16,
    pub is_local: bool, // true = capture local, false = capture parent upvalue
}

impl CompiledFunction {
    pub fn new(name: Option<Box<str>>, arity: u8) -> Self {
        Self {
            name,
            arity,
            locals_count: 0,
            upvalue_count: 0,
            upvalues: Vec::new(),
            bytecode: Bytecode::new(),
        }
    }
}

impl Bytecode {
    pub fn new() -> Self { Self::default() }
    
    pub fn emit(&mut self, op: Opcode) { self.code.push(op as u8); }
    
    pub fn emit_u8(&mut self, val: u8) { self.code.push(val); }
    
    pub fn emit_u16(&mut self, val: u16) {
        self.code.push((val >> 8) as u8);
        self.code.push(val as u8);
    }
    
    pub fn emit_i16(&mut self, val: i16) { self.emit_u16(val as u16); }
    
    pub fn add_constant(&mut self, c: Constant) -> u16 {
        // Deduplicate number and string constants
        for (i, existing) in self.constants.iter().enumerate() {
            match (&c, existing) {
                (Constant::Number(a), Constant::Number(b)) if a.to_bits() == b.to_bits() => {
                    return i as u16;
                }
                (Constant::String(a), Constant::String(b)) if a == b => {
                    return i as u16;
                }
                _ => {}
            }
        }
        let idx = self.constants.len() as u16;
        self.constants.push(c);
        idx
    }
    
    pub fn add_name(&mut self, name: &str) -> u16 {
        if let Some(idx) = self.names.iter().position(|n| &**n == name) {
            return idx as u16;
        }
        let idx = self.names.len() as u16;
        self.names.push(name.into());
        idx
    }
    
    pub fn len(&self) -> usize { self.code.len() }
    pub fn is_empty(&self) -> bool { self.code.is_empty() }
}
