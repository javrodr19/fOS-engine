//! Bytecode
//!
//! Register-based instructions (8 bytes each). A function's registers are
//! a window of the VM stack: register 0 holds `this`, registers
//! 1..=nparams the parameters, then locals and temporaries.
//!
//! Calls pass the callee in register `func`, `this` in `func + 1` and the
//! arguments right after; the callee's register window starts at
//! `func + 1`, so arguments are never copied.

use std::cell::Cell;
use std::rc::Rc;

use crate::gc::{Gc, Tracer};
use crate::object::JsObject;
use crate::shape::ShapeId;
use crate::string::Atom;
use crate::value::Value;

pub type Reg = u16;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Insn {
    Nop,
    Mov { dst: Reg, src: Reg },
    LoadInt { dst: Reg, value: i32 },
    /// Number or string constant
    LoadConst { dst: Reg, idx: u32 },
    LoadUndef { dst: Reg },
    LoadNull { dst: Reg },
    LoadTrue { dst: Reg },
    LoadFalse { dst: Reg },
    /// Uninitialized `let`/`const` (temporal dead zone)
    LoadHole { dst: Reg },
    /// ReferenceError if `reg` is still uninitialized; `name` indexes atoms
    CheckInit { reg: Reg, name: u16 },
    /// TypeError: assignment to constant `name`
    ThrowConstAssign { name: u16 },

    GetUpval { dst: Reg, idx: u16 },
    /// Like GetUpval, with a TDZ check
    GetUpvalChecked { dst: Reg, idx: u16, name: u16 },
    SetUpval { src: Reg, idx: u16 },
    /// Close upvalues pointing at registers >= `from`
    CloseUpvals { from: Reg },

    /// Global variable read (ReferenceError if undeclared)
    GetGlobal { dst: Reg, ic: u16 },
    /// `typeof` of a global (undefined if undeclared)
    TypeofGlobal { dst: Reg, ic: u16 },
    /// Global variable write (ReferenceError in strict mode if undeclared)
    SetGlobal { src: Reg, ic: u16 },
    /// Script-level `var`/function declaration
    DeclareGlobalVar { name: u16 },
    DeclareGlobalFunc { src: Reg, name: u16 },
    /// Script-level `let`/`const`/`class` (uninitialized until InitGlobalLex)
    DeclareGlobalLex { name: u16, is_const: bool },
    InitGlobalLex { src: Reg, name: u16 },

    Add { dst: Reg, a: Reg, b: Reg },
    Sub { dst: Reg, a: Reg, b: Reg },
    Mul { dst: Reg, a: Reg, b: Reg },
    Div { dst: Reg, a: Reg, b: Reg },
    Mod { dst: Reg, a: Reg, b: Reg },
    Exp { dst: Reg, a: Reg, b: Reg },
    BitAnd { dst: Reg, a: Reg, b: Reg },
    BitOr { dst: Reg, a: Reg, b: Reg },
    BitXor { dst: Reg, a: Reg, b: Reg },
    Shl { dst: Reg, a: Reg, b: Reg },
    Sar { dst: Reg, a: Reg, b: Reg },
    Shr { dst: Reg, a: Reg, b: Reg },
    /// dst = a + imm (numbers fast path, ToPrimitive otherwise)
    AddImm { dst: Reg, a: Reg, imm: i16 },
    /// dst = a - imm (numeric)
    SubImm { dst: Reg, a: Reg, imm: i16 },
    Eq { dst: Reg, a: Reg, b: Reg },
    Ne { dst: Reg, a: Reg, b: Reg },
    StrictEq { dst: Reg, a: Reg, b: Reg },
    StrictNe { dst: Reg, a: Reg, b: Reg },
    Lt { dst: Reg, a: Reg, b: Reg },
    Le { dst: Reg, a: Reg, b: Reg },
    Gt { dst: Reg, a: Reg, b: Reg },
    Ge { dst: Reg, a: Reg, b: Reg },
    In { dst: Reg, a: Reg, b: Reg },
    Instanceof { dst: Reg, a: Reg, b: Reg },
    Neg { dst: Reg, src: Reg },
    /// ToNumber
    Plus { dst: Reg, src: Reg },
    /// ToNumeric (postfix update: the old value)
    ToNumeric { dst: Reg, src: Reg },
    Not { dst: Reg, src: Reg },
    BitNot { dst: Reg, src: Reg },
    Typeof { dst: Reg, src: Reg },
    Inc { dst: Reg, src: Reg },
    Dec { dst: Reg, src: Reg },
    /// ToString (template literals)
    ToStr { dst: Reg, src: Reg },
    ToPropertyKey { dst: Reg, src: Reg },

    Jmp { off: i32 },
    JmpTrue { cond: Reg, off: i32 },
    JmpFalse { cond: Reg, off: i32 },
    JmpNullish { src: Reg, off: i32 },
    JmpNotNullish { src: Reg, off: i32 },
    JmpUndefined { src: Reg, off: i32 },
    JmpNotUndefined { src: Reg, off: i32 },
    /// Jump if the value is the internal hole (iteration done)
    JmpHole { src: Reg, off: i32 },
    JmpNotHole { src: Reg, off: i32 },
    /// Fused compare-and-branch (loop conditions, compiled at the bottom
    /// of the loop so the jump is backward with a known offset)
    JmpLt { a: Reg, b: Reg, off: i16 },
    JmpLe { a: Reg, b: Reg, off: i16 },
    JmpGt { a: Reg, b: Reg, off: i16 },
    JmpGe { a: Reg, b: Reg, off: i16 },
    /// Negated forms (jump unless the comparison holds; NaN jumps)
    JmpNLt { a: Reg, b: Reg, off: i16 },
    JmpNLe { a: Reg, b: Reg, off: i16 },
    JmpNGt { a: Reg, b: Reg, off: i16 },
    JmpNGe { a: Reg, b: Reg, off: i16 },
    JmpStrictEq { a: Reg, b: Reg, off: i16 },
    JmpStrictNe { a: Reg, b: Reg, off: i16 },

    NewObject { dst: Reg },
    NewArray { dst: Reg, cap: u16 },
    GetProp { dst: Reg, obj: Reg, ic: u16 },
    SetProp { obj: Reg, src: Reg, ic: u16 },
    GetElem { dst: Reg, obj: Reg, key: Reg },
    SetElem { obj: Reg, key: Reg, src: Reg },
    /// Define an own enumerable data property (object literals)
    DefineProp { obj: Reg, src: Reg, ic: u16 },
    DefineElem { obj: Reg, key: Reg, src: Reg },
    /// Define a non-enumerable method (classes); `key` holds the key value
    DefineMethod { obj: Reg, key: Reg, func: Reg },
    /// Define an accessor half; `enumerable` for object literals
    DefineGetter { obj: Reg, key: Reg, func: Reg },
    DefineSetter { obj: Reg, key: Reg, func: Reg },
    DefineGetterHidden { obj: Reg, key: Reg, func: Reg },
    DefineSetterHidden { obj: Reg, key: Reg, func: Reg },
    ArrayPush { arr: Reg, src: Reg },
    ArrayPushHole { arr: Reg },
    ArraySpread { arr: Reg, src: Reg },
    /// Object spread: copy own enumerable properties of src into obj
    CopyDataProps { obj: Reg, src: Reg },
    /// Object rest: copy props of src into a new object in dst, excluding
    /// the keys in the array `excluded`
    CopyRest { dst: Reg, src: Reg, excluded: Reg },
    DeleteProp { dst: Reg, obj: Reg, name: u16 },
    DeleteElem { dst: Reg, obj: Reg, key: Reg },
    /// `__proto__: value` in an object literal
    SetProtoLiteral { obj: Reg, src: Reg },
    /// Throw TypeError if the value can't be converted to an object
    RequireObjectCoercible { src: Reg },

    /// Create a closure from nested function `idx`
    Closure { dst: Reg, idx: u16 },
    /// `import(spec)`: a promise for the module namespace; `referrer` is
    /// the importing module's URL (undefined in scripts)
    DynamicImport { dst: Reg, spec: Reg, referrer: Reg },
    /// Set a function's home object (methods that use `super`)
    SetHomeObject { func: Reg, obj: Reg },
    /// Create a class: dst = constructor from closure `ctor` (already
    /// created), with prototype object `proto` and parent `parent`
    MakeClass { ctor: Reg, proto: Reg, parent: Reg },
    SetClassFields { ctor: Reg, init: Reg },
    Call { dst: Reg, func: Reg, argc: u16 },
    /// Arguments in an array at func + 2
    CallSpread { dst: Reg, func: Reg },
    New { dst: Reg, func: Reg, argc: u16 },
    NewSpread { dst: Reg, func: Reg },
    /// super(...) in a derived constructor: `func` holds the running class
    /// constructor (its prototype is the parent), `func + 1` new.target,
    /// arguments from `func + 2`
    SuperCall { dst: Reg, func: Reg, argc: u16 },
    /// Like SuperCall with the arguments in an array at `func + 2`
    SuperCallSpread { dst: Reg, func: Reg },
    /// dst = the home object's prototype (for super.x)
    GetSuperBase { dst: Reg },
    /// dst = new.target
    LoadNewTarget { dst: Reg },
    /// dst = the running function (named function expressions)
    LoadCallee { dst: Reg },
    Return { src: Reg },
    ReturnUndef,
    Throw { src: Reg },
    /// Throw a TypeError/ReferenceError with a message constant
    ThrowError { kind: u8, msg: u32 },

    ForInInit { dst: Reg, obj: Reg },
    /// dst = next key or hole
    ForInNext { dst: Reg, iter: Reg },
    GetIterator { dst: Reg, src: Reg },
    /// dst = next value or hole when done
    IterNext { dst: Reg, iter: Reg },
    /// dst = next value, undefined when done (destructuring)
    IterValue { dst: Reg, iter: Reg },
    /// dst = array of the remaining values
    IterRest { dst: Reg, iter: Reg },
    IterClose { iter: Reg },
    /// dst = iterator record for `for await` / async `yield*` (sync
    /// iterables are wrapped)
    GetAsyncIterator { dst: Reg, src: Reg },
    /// Leaving a `for await` early: dst = result of the iterator's
    /// `return()` (to be awaited), or undefined if it has none
    AsyncIterReturn { dst: Reg, iter: Reg },

    /// dst = template strings array for tagged template `idx`
    TemplateObject { dst: Reg, idx: u16 },
    RegExp { dst: Reg, idx: u16 },
    /// dst = a fresh private name (class `#x`); `name` indexes atoms
    NewPrivateName { dst: Reg, name: u16 },

    /// `with` lookup: dst = whether `obj` has property `name` (and it is not
    /// blocked by @@unscopables)
    WithHas { dst: Reg, obj: Reg, name: u16 },
    /// Generator functions: suspend right after argument binding and
    /// return the generator object
    GenStart,
    /// Suspend with `src`; on resumption dst receives the sent value
    Yield { dst: Reg, src: Reg },
    /// `yield*` step: dst = iterator result object of iter.next(val)
    IterSend { dst: Reg, iter: Reg, val: Reg },
    /// Async functions: create the result promise
    AsyncStart,
    /// Suspend until `src` settles; dst receives the value (or the reason
    /// is thrown)
    Await { dst: Reg, src: Reg },
    /// Settle the async function's promise and return it
    AsyncReturn { src: Reg },
    AsyncThrow { src: Reg },
    Debugger,
}

impl Insn {
    /// The register the instruction writes its result to, if any
    pub fn dst(self) -> Option<Reg> {
        match self {
            Insn::Mov { dst, .. } | Insn::LoadInt { dst, .. } | Insn::LoadConst { dst, .. } | Insn::LoadUndef { dst, .. } | Insn::LoadNull { dst, .. } | Insn::LoadTrue { dst, .. } | Insn::LoadFalse { dst, .. } | Insn::LoadHole { dst, .. } | Insn::GetUpval { dst, .. } | Insn::GetUpvalChecked { dst, .. } | Insn::GetGlobal { dst, .. } | Insn::TypeofGlobal { dst, .. } | Insn::Add { dst, .. } | Insn::Sub { dst, .. } | Insn::Mul { dst, .. } | Insn::Div { dst, .. } | Insn::Mod { dst, .. } | Insn::Exp { dst, .. } | Insn::BitAnd { dst, .. } | Insn::BitOr { dst, .. } | Insn::BitXor { dst, .. } | Insn::Shl { dst, .. } | Insn::Sar { dst, .. } | Insn::Shr { dst, .. } | Insn::AddImm { dst, .. } | Insn::SubImm { dst, .. } | Insn::Eq { dst, .. } | Insn::Ne { dst, .. } | Insn::StrictEq { dst, .. } | Insn::StrictNe { dst, .. } | Insn::Lt { dst, .. } | Insn::Le { dst, .. } | Insn::Gt { dst, .. } | Insn::Ge { dst, .. } | Insn::In { dst, .. } | Insn::Instanceof { dst, .. } | Insn::Neg { dst, .. } | Insn::Plus { dst, .. } | Insn::ToNumeric { dst, .. } | Insn::Not { dst, .. } | Insn::BitNot { dst, .. } | Insn::Typeof { dst, .. } | Insn::Inc { dst, .. } | Insn::Dec { dst, .. } | Insn::ToStr { dst, .. } | Insn::ToPropertyKey { dst, .. } | Insn::NewObject { dst, .. } | Insn::NewArray { dst, .. } | Insn::GetProp { dst, .. } | Insn::GetElem { dst, .. } | Insn::CopyRest { dst, .. } | Insn::DeleteProp { dst, .. } | Insn::DeleteElem { dst, .. } | Insn::Closure { dst, .. } | Insn::DynamicImport { dst, .. } | Insn::Call { dst, .. } | Insn::CallSpread { dst, .. } | Insn::New { dst, .. } | Insn::NewSpread { dst, .. } | Insn::SuperCall { dst, .. } | Insn::SuperCallSpread { dst, .. } | Insn::GetSuperBase { dst, .. } | Insn::LoadNewTarget { dst, .. } | Insn::LoadCallee { dst, .. } | Insn::ForInInit { dst, .. } | Insn::ForInNext { dst, .. } | Insn::GetIterator { dst, .. } | Insn::IterNext { dst, .. } | Insn::IterValue { dst, .. } | Insn::IterRest { dst, .. } | Insn::GetAsyncIterator { dst, .. } | Insn::AsyncIterReturn { dst, .. } | Insn::TemplateObject { dst, .. } | Insn::RegExp { dst, .. } | Insn::NewPrivateName { dst, .. } | Insn::WithHas { dst, .. } | Insn::Yield { dst, .. } | Insn::IterSend { dst, .. } | Insn::Await { dst, .. } => Some(dst),
            _ => None,
        }
    }
}

/// Kinds for `ThrowError`
pub const ERR_TYPE: u8 = 0;
pub const ERR_REFERENCE: u8 = 1;
pub const ERR_SYNTAX: u8 = 2;
pub const ERR_RANGE: u8 = 3;

/// Where a nested function's captured variable comes from
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UpvalDesc {
    /// A register of the enclosing function (else one of its upvalues)
    pub from_parent_reg: bool,
    pub index: u16,
}

/// Exception handler: pcs in [start, end) jump to `target` with the
/// exception in `reg`
#[derive(Clone, Copy, Debug)]
pub struct Handler {
    pub start: u32,
    pub end: u32,
    pub target: u32,
    pub reg: Reg,
    /// A `finally` (or iterator-closing) handler, which also runs when a
    /// generator is closed with `return()`; catch handlers don't
    pub finally: bool,
}

/// Inline cache state for a property or global access site
#[derive(Clone, Copy, Debug)]
pub struct Ic {
    pub atom: Atom,
    pub state: IcState,
}

#[derive(Clone, Copy, Debug)]
pub enum IcState {
    Empty,
    /// Own property of objects with `shape`, at `slot`
    Own { shape: ShapeId, slot: u32 },
    /// Found on `holder` (in the prototype chain) at `slot`, for receivers
    /// with `shape` and prototype `proto`, valid while no prototype has
    /// changed shape (`epoch`)
    Proto { shape: ShapeId, proto: Gc<JsObject>, holder: Gc<JsObject>, slot: u32, epoch: u32 },
    /// Add property: shape `from` becomes `to`, value at `slot`, for
    /// receivers with prototype `proto`, while no prototype changed
    Add { from: ShapeId, to: ShapeId, slot: u32, proto: Option<Gc<JsObject>>, epoch: u32 },
    /// `length` of arrays
    ArrayLength,
    /// `length` of primitive strings
    StringLength,
    /// Global lexical (`let`/`const`) binding at `slot`
    GlobalLex { slot: u32 },
    /// Too many shapes seen: always use the slow path
    Megamorphic,
}

/// Tagged template site data
#[derive(Debug)]
pub struct TemplateSite {
    pub cooked: Vec<Option<Box<[u16]>>>,
    pub raw: Vec<Box<[u16]>>,
}

/// A regular expression literal (compiled on first evaluation)
pub struct RegexLiteral {
    pub pattern: Box<[u16]>,
    pub flags: Box<str>,
    pub compiled: std::cell::OnceCell<Rc<crate::regex::Regex>>,
}

/// A function: header known when its definition is compiled, code
/// compiled then or (lazy functions) on first call
pub struct FunctionProto {
    pub name: Atom,
    pub upvals: Box<[UpvalDesc]>,
    pub nparams: u16,
    /// Function.prototype.length
    pub length: u16,
    pub strict: bool,
    pub is_arrow: bool,
    /// Methods, accessors and arrows are not constructors
    pub is_constructor: bool,
    pub is_class_constructor: bool,
    pub is_derived: bool,
    pub is_generator: bool,
    pub is_async: bool,
    /// Source text range (for Function.prototype.toString)
    pub source: (u32, u32),
    /// Collection number this was last traced in
    pub traced: Cell<u32>,
    /// How to compile the body later (lazy functions)
    pub lazy: Option<Box<LazyInfo>>,
    pub compiled: std::cell::OnceCell<Code>,
    /// The script or module defining this function (stack traces)
    pub script: Option<Rc<Script>>,
}

/// A script or module: its name (URL) and source, shared by its functions
pub struct Script {
    pub name: Box<str>,
    pub source: Rc<str>,
    /// Byte offsets of line starts, built on first use
    line_starts: std::cell::OnceCell<Box<[u32]>>,
}

impl Script {
    pub fn new(name: &str, source: Rc<str>) -> Rc<Script> {
        Rc::new(Script { name: name.into(), source, line_starts: std::cell::OnceCell::new() })
    }

    /// 1-based line and column (in UTF-16 units, as browsers count) of a
    /// byte offset into the source
    pub fn line_col(&self, offset: u32) -> (u32, u32) {
        let src = &*self.source;
        let starts = self.line_starts.get_or_init(|| {
            let b = src.as_bytes();
            let mut starts = vec![0u32];
            let mut i = 0;
            while i < b.len() {
                match b[i] {
                    b'\n' => starts.push(i as u32 + 1),
                    b'\r' if b.get(i + 1) != Some(&b'\n') => starts.push(i as u32 + 1),
                    // U+2028 and U+2029
                    0xE2 if b.get(i + 1) == Some(&0x80) && matches!(b.get(i + 2), Some(0xA8 | 0xA9)) => {
                        starts.push(i as u32 + 3);
                        i += 2;
                    }
                    _ => {}
                }
                i += 1;
            }
            starts.into_boxed_slice()
        });
        let offset = offset.min(src.len() as u32);
        let line = starts.partition_point(|&s| s <= offset).max(1);
        let start = starts[line - 1] as usize;
        let mut end = offset as usize;
        while !src.is_char_boundary(end) {
            end -= 1;
        }
        let col: usize = src[start..end].chars().map(char::len_utf16).sum();
        (line as u32, col as u32 + 1)
    }
}

/// Encode (pc, source offset) pairs, in pc order, as deltas: a varint pc
/// step and a zigzag varint offset step per entry
pub fn encode_positions(entries: &[(u32, u32)]) -> Box<[u8]> {
    fn varint(out: &mut Vec<u8>, mut v: u64) {
        while v >= 0x80 {
            out.push(v as u8 | 0x80);
            v >>= 7;
        }
        out.push(v as u8);
    }
    let mut out = Vec::with_capacity(entries.len() * 3);
    let (mut pc, mut pos) = (0u32, 0i64);
    for &(p, q) in entries {
        varint(&mut out, (p - pc) as u64);
        let d = q as i64 - pos;
        varint(&mut out, ((d << 1) ^ (d >> 63)) as u64);
        pc = p;
        pos = q as i64;
    }
    out.into_boxed_slice()
}

/// A compiled function body
pub struct Code {
    pub code: Box<[Insn]>,
    pub consts: Box<[Value]>,
    pub atoms: Box<[Atom]>,
    pub funcs: Box<[Rc<FunctionProto>]>,
    pub ics: Box<[Cell<Ic>]>,
    pub handlers: Box<[Handler]>,
    pub templates: Box<[TemplateSite]>,
    pub regexps: Box<[RegexLiteral]>,
    pub nregs: u16,
    /// Sloppy functions that use `this` coerce it to an object
    pub coerce_this: bool,
    /// Register receiving the `arguments` object, if used
    pub arguments_reg: Option<Reg>,
    /// Register receiving the rest parameter array, if any
    pub rest_reg: Option<Reg>,
    /// Source offsets of instructions that can throw (see `encode_positions`)
    pub positions: Box<[u8]>,
}

impl Code {
    /// Source offset of the instruction at `pc`: the nearest recorded at
    /// or before it
    pub fn position_at(&self, pc: u32) -> Option<u32> {
        fn read(b: &[u8], i: &mut usize) -> u64 {
            let (mut v, mut shift) = (0u64, 0);
            while *i < b.len() {
                let byte = b[*i];
                *i += 1;
                v |= ((byte & 0x7F) as u64) << shift;
                if byte & 0x80 == 0 {
                    break;
                }
                shift += 7;
            }
            v
        }
        let b = &*self.positions;
        let (mut i, mut cur_pc, mut pos, mut found) = (0usize, 0u32, 0i64, None);
        while i < b.len() {
            cur_pc += read(b, &mut i) as u32;
            let z = read(b, &mut i);
            pos += ((z >> 1) as i64) ^ -((z & 1) as i64);
            if cur_pc > pc {
                break;
            }
            found = Some(pos as u32);
        }
        found
    }
}

/// Source and scope information for compiling a lazy function
pub struct LazyInfo {
    pub params_start: u32,
    pub kind: crate::ast::FunctionKind,
    pub fn_name: Option<crate::ast::Name>,
    /// Compiled as a function expression (binds its own name)
    pub is_expression: bool,
    pub outer_strict: bool,
    /// Names reachable through upvalues, parallel to `upvals`, with
    /// dead-zone check and binding kind flags
    pub upval_names: Vec<(crate::ast::Name, bool, u8)>,
    /// Defined in a module
    pub in_module: bool,
}

impl FunctionProto {
    /// Mark the constants and inline-cache objects of this function and
    /// its nested functions (once per collection)
    pub fn trace(&self, tracer: &mut Tracer) {
        if self.traced.get() == tracer.epoch() {
            return;
        }
        self.traced.set(tracer.epoch());
        let Some(code) = self.compiled.get() else { return };
        tracer.mark_values(&code.consts);
        for ic in code.ics.iter() {
            match ic.get().state {
                IcState::Proto { proto, holder, .. } => {
                    tracer.mark(proto);
                    tracer.mark(holder);
                }
                IcState::Add { proto: Some(p), .. } => tracer.mark(p),
                _ => {}
            }
        }
        for f in code.funcs.iter() {
            f.trace(tracer);
        }
    }
}

impl std::ops::Deref for FunctionProto {
    type Target = Code;

    /// The compiled body (callers make sure it is compiled)
    #[inline(always)]
    fn deref(&self) -> &Code {
        match self.compiled.get() {
            Some(c) => c,
            None => panic!("function used before compilation"),
        }
    }
}

impl std::fmt::Debug for FunctionProto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FunctionProto")
            .field("name", &self.name)
            .field("nparams", &self.nparams)
            .field("compiled", &self.compiled.get().is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instruction_size() {
        assert_eq!(std::mem::size_of::<Insn>(), 8, "instructions must stay 8 bytes");
    }
}
