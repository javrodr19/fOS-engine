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
    
    // Objects
    NewObject = 100,
    NewArray = 101,      // count: u16
    
    // Special
    Typeof = 110,
    Instanceof = 111,
    In = 112,
    
    // Control
    Throw = 120,
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
    pub bytecode: Bytecode,
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
