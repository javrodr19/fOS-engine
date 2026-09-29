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

    /// dst = template strings array for tagged template `idx`
    TemplateObject { dst: Reg, idx: u16 },
    RegExp { dst: Reg, idx: u16 },
    /// dst = a fresh private name (class `#x`); `name` indexes atoms
    NewPrivateName { dst: Reg, name: u16 },
    Debugger,
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

/// Compiled function
pub struct FunctionProto {
    pub name: Atom,
    pub code: Box<[Insn]>,
    pub consts: Box<[Value]>,
    pub atoms: Box<[Atom]>,
    pub funcs: Box<[Rc<FunctionProto>]>,
    pub ics: Box<[Cell<Ic>]>,
    pub upvals: Box<[UpvalDesc]>,
    pub handlers: Box<[Handler]>,
    pub templates: Box<[TemplateSite]>,
    pub regexps: Box<[(Box<str>, Box<str>)]>,
    pub nregs: u16,
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
    /// Sloppy functions that use `this` coerce it to an object
    pub coerce_this: bool,
    /// Register receiving the `arguments` object, if used
    pub arguments_reg: Option<Reg>,
    /// Register receiving the rest parameter array, if any
    pub rest_reg: Option<Reg>,
    /// Source text range (for Function.prototype.toString)
    pub source: (u32, u32),
    /// Collection number this was last traced in
    pub traced: Cell<u32>,
}

impl FunctionProto {
    /// Mark the constants and inline-cache objects of this function and
    /// its nested functions (once per collection)
    pub fn trace(&self, tracer: &mut Tracer) {
        if self.traced.get() == tracer.epoch() {
            return;
        }
        self.traced.set(tracer.epoch());
        tracer.mark_values(&self.consts);
        for ic in self.ics.iter() {
            match ic.get().state {
                IcState::Proto { proto, holder, .. } => {
                    tracer.mark(proto);
                    tracer.mark(holder);
                }
                IcState::Add { proto: Some(p), .. } => tracer.mark(p),
                _ => {}
            }
        }
        for f in self.funcs.iter() {
            f.trace(tracer);
        }
    }
}

impl std::fmt::Debug for FunctionProto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FunctionProto")
            .field("name", &self.name)
            .field("nregs", &self.nregs)
            .field("nparams", &self.nparams)
            .field("code", &self.code)
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
