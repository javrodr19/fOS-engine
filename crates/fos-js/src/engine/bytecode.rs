//! Bytecode definitions
//!
//! Stack-based bytecode for the JavaScript VM.

/// Bytecode instruction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Opcode {
    // Stack ops
    Pop = 0,
    Dup = 1,
    
    // Constants
    LoadConst = 10,      // idx: u16
    LoadNull = 11,
    LoadUndefined = 12,
    LoadTrue = 13,
    LoadFalse = 14,
    LoadZero = 15,
    LoadOne = 16,
    
    // Variables
    GetLocal = 20,       // idx: u16
    SetLocal = 21,       // idx: u16
    GetGlobal = 22,      // name_idx: u16
    SetGlobal = 23,      // name_idx: u16
    GetUpvalue = 24,     // idx: u16
    SetUpvalue = 25,     // idx: u16
    CloseUpvalue = 26,   // Close upvalue at stack top
    
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
    
    // Functions
    Call = 90,           // argc: u8
    Return = 91,
    Closure = 92,        // const_idx: u16, upvalue_count: u8, then upvalue_info
    
    // Objects
    NewObject = 100,
    NewArray = 101,      // count: u16
    
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
