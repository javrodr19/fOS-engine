//! Bytecode compiler
//!
//! One pass over each function's syntax tree produces register bytecode.
//!
//! Variables live in registers. A variable captured by a nested function
//! is reached through an upvalue that points at the register while its
//! scope is active and is "closed" (copied out) when the scope exits, as in
//! Lua. Closing at the end of each loop iteration gives `let` its
//! per-iteration bindings for free. Whether a scope needs closing is only
//! known once the scope's code has been compiled, so exits that happen
//! earlier (`break`, `continue`) leave a placeholder that is patched when
//! the scope ends; placeholders that stay unused are stripped at the end.
//!
//! The same trick keeps the temporal dead zone cheap: a `let` binding is
//! only reset to "uninitialized" on scope entry when some access could
//! observe it before its declaration runs.
//!
//! `finally` blocks are compiled inline at every exit from their `try`
//! (normal completion, `break`/`continue`/`return`, and an exception
//! handler that rethrows), so there is no runtime "finally" machinery.

mod expr;
mod stmt;

use std::cell::Cell;
use std::rc::Rc;

use rustc_hash::FxHashMap;

use crate::ast::*;
use crate::bytecode::*;
use crate::gc::Heap;
use crate::lexer::SyntaxError;
use crate::string::{Atom, Atoms, Units, atoms};
use crate::value::Value;

/// Compile a script. Top-level `var` and function declarations become
/// properties of the global object; top-level `let`, `const` and `class`
/// go in the global lexical scope shared by all scripts. The compiled
/// function returns the value of the script's last top-level expression
/// statement.
pub fn compile_script(heap: &Heap, atoms: &mut Atoms, src: &str, program: &Program) -> Result<Rc<FunctionProto>, SyntaxError> {
    let mut c = Compiler { heap, atoms, src, fs: Vec::new(), src_rc: None, lazy_root: None, in_module: false };
    let mut no_fused = false;
    loop {
        match c.script(program, no_fused) {
            Ok(proto) => return Ok(proto),
            Err(CErr::Retry) if !no_fused => {
                c.fs.clear();
                no_fused = true;
            }
            Err(CErr::Retry) => return Err(SyntaxError { message: "function too large".into(), pos: 0 }),
            Err(CErr::Syntax(e)) => return Err(e),
        }
    }
}

/// How a module-scope binding starts out
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleBindingKind {
    /// `var` and function declarations: undefined until set
    Var,
    /// `let`, `const` and `class`: in their dead zone until initialized
    Lexical,
    /// An import: bound to the exporting module's binding when linked
    Import,
    /// `import.meta` and the module's URL: set when linked
    Hidden,
}

/// A compiled module. Its scope is a list of cells (closed upvalues) that
/// the modules importing from it share, which makes imports live
/// bindings and lets modules in an import cycle see each other.
pub struct CompiledModule {
    /// Creates the module's function declarations; run when the module is
    /// linked, so modules in a cycle can call them before this one runs
    pub init: Rc<FunctionProto>,
    /// The module's code (an async function when it uses top-level await)
    pub body: Rc<FunctionProto>,
    /// The module scope: upvalue `i` of `init` and `body` is binding `i`
    pub bindings: Vec<(Name, ModuleBindingKind)>,
}

/// Compile a module
pub fn compile_module(heap: &Heap, atoms: &mut Atoms, src: &str, module: &Module) -> Result<CompiledModule, SyntaxError> {
    let mut scope: Vec<(Name, ModuleBindingKind, BindKind)> = Vec::new();
    let mut add = |name: &Name, kind: ModuleBindingKind, bind: BindKind| -> Result<(), SyntaxError> {
        if let Some(existing) = scope.iter().find(|b| b.0 == *name) {
            if existing.1 == ModuleBindingKind::Var && kind == ModuleBindingKind::Var {
                return Ok(());
            }
            return Err(SyntaxError { message: format!("Identifier '{name}' has already been declared"), pos: 0 });
        }
        scope.push((name.clone(), kind, bind));
        Ok(())
    };
    // `var`s and function declarations
    let mut vars = Vec::new();
    collect_vars(&module.body, &mut vars, false);
    for name in &vars {
        add(name, ModuleBindingKind::Var, BindKind::Var)?;
    }
    for stmt in &module.body {
        match stmt {
            Stmt::Var { kind, decls } if *kind != VarKind::Var => {
                let bind = if *kind == VarKind::Const { BindKind::Const } else { BindKind::Let };
                let mut names = Vec::new();
                for d in decls {
                    pattern_names(&d.target, &mut names);
                }
                for name in &names {
                    add(name, ModuleBindingKind::Lexical, bind)?;
                }
            }
            Stmt::Class(c) => {
                if let Some(name) = &c.name {
                    add(name, ModuleBindingKind::Lexical, BindKind::Let)?;
                }
            }
            _ => {}
        }
    }
    for import in &module.imports {
        add(&import.local, ModuleBindingKind::Import, BindKind::Const)?;
    }
    add(&Rc::from(MODULE_META), ModuleBindingKind::Hidden, BindKind::Const)?;
    add(&Rc::from(MODULE_REFERRER), ModuleBindingKind::Hidden, BindKind::Const)?;
    for export in &module.exports {
        if let ExportEntry::Local { local, .. } = export {
            if !scope.iter().any(|b| b.0 == *local) {
                return Err(SyntaxError { message: format!("Export '{local}' is not defined in module"), pos: 0 });
            }
        }
    }
    if scope.len() > u16::MAX as usize {
        return Err(SyntaxError { message: "too many module bindings".into(), pos: 0 });
    }

    let names: Vec<Name> = scope.iter().map(|b| b.0.clone()).collect();
    let upvals: Vec<UpvalInfo> = scope
        .iter()
        .enumerate()
        .map(|(i, b)| UpvalInfo {
            desc: UpvalDesc { from_parent_reg: false, index: i as u16 },
            checked: matches!(b.1, ModuleBindingKind::Lexical | ModuleBindingKind::Import),
            kind: b.2,
        })
        .collect();
    let mut c = Compiler { heap, atoms, src, fs: Vec::new(), src_rc: None, lazy_root: None, in_module: true };
    let init = c.with_retry(|c, no_fused| c.module_init(module, &upvals, &names, no_fused))?;
    let body = c.with_retry(|c, no_fused| c.module_body(module, &upvals, &names, no_fused))?;
    Ok(CompiledModule { init, body, bindings: scope.into_iter().map(|b| (b.0, b.1)).collect() })
}

/// Compile a function created by the `Function` constructor (its scope is
/// the global scope)
pub fn compile_function_object(heap: &Heap, atoms: &mut Atoms, src: &str, func: &Function) -> Result<Rc<FunctionProto>, SyntaxError> {
    let mut c = Compiler { heap, atoms, src, fs: Vec::new(), src_rc: None, lazy_root: None, in_module: false };
    // An empty script level to resolve names against (all globals)
    c.fs.push(FuncState::new(true, false, func.strict, FunctionKind::Normal, false));
    let name = c.atoms.intern_str(heap, "anonymous");
    match c.function(func, Some(name), false) {
        Ok((proto, _)) => Ok(proto),
        Err(CErr::Retry) => Err(SyntaxError { message: "function too large".into(), pos: 0 }),
        Err(CErr::Syntax(e)) => Err(e),
    }
}

/// Inline caches a function gets before further sites share them by name
const SHARED_ICS_FROM: usize = 49_152;

pub(crate) enum CErr {
    Syntax(SyntaxError),
    /// A short jump overflowed: compile the function again without them
    Retry,
}

pub(crate) type CResult<T> = Result<T, CErr>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum BindKind {
    Var,
    Let,
    Const,
    Class,
    Param,
    This,
    /// `this` of a derived constructor (uninitialized until `super()`)
    ThisUninit,
    /// Own name of a named function expression (read-only)
    Callee,
    /// Hidden compiler bindings (`new.target`, computed field keys...)
    Internal,
}

impl BindKind {
    pub(crate) fn to_u8(self) -> u8 {
        self as u8
    }

    pub(crate) fn from_u8(v: u8) -> BindKind {
        [
            BindKind::Var,
            BindKind::Let,
            BindKind::Const,
            BindKind::Class,
            BindKind::Param,
            BindKind::This,
            BindKind::ThisUninit,
            BindKind::Callee,
            BindKind::Internal,
        ][v as usize]
    }

    fn is_lexical(self) -> bool {
        matches!(self, BindKind::Let | BindKind::Const | BindKind::Class | BindKind::ThisUninit)
    }
}

pub(crate) struct Binding {
    name: Name,
    reg: Reg,
    kind: BindKind,
    /// Textually initialized: later accesses in this function skip the
    /// dead-zone check
    initialized: bool,
    captured: bool,
    /// Placeholder at scope entry, patched to `LoadHole` if an access might
    /// see the binding before its declaration
    hole_patch: Option<u32>,
    /// Placeholder at function entry that fills the binding, patched in
    /// when the binding is used (`LoadCallee`, `LoadNewTarget`)
    init_patch: Option<(u32, Insn)>,
}

struct Scope {
    first_binding: usize,
    reg_start: Reg,
    /// Placeholders at early exits from this scope (become `CloseUpvals`)
    close_patches: Vec<u32>,
    /// `switch` blocks: a case can run without the declarations of earlier
    /// cases, so their bindings always need dead-zone checks
    switch_like: bool,
}

struct Control {
    labels: Vec<Name>,
    is_loop: bool,
    /// Plain `break` targets loops and switches only
    breakable: bool,
    breaks: Vec<u32>,
    continues: Vec<u32>,
    /// Scopes and try entries a `break` leaves (those at and above)
    scope_depth: usize,
    try_depth: usize,
    /// Same for `continue` (loops with per-iteration scopes or iterators
    /// keep some)
    cont_scope_depth: usize,
    cont_try_depth: usize,
}

#[derive(Clone, Copy)]
enum TryKind<'a> {
    Catch,
    Finally(&'a [Stmt]),
    /// A `for-of` loop: leaving it early closes the iterator
    IterClose(Reg),
    /// A `for await` loop: leaving it early calls and awaits `return()`
    AsyncIterClose(Reg),
}

struct TryEntry<'a> {
    kind: TryKind<'a>,
    seg_start: u32,
    /// Indices into `handlers` of this entry's protected ranges
    handler_idxs: Vec<usize>,
    /// Register receiving the exception
    reg: Reg,
}

#[derive(Clone, Copy)]
struct UpvalInfo {
    desc: UpvalDesc,
    checked: bool,
    kind: BindKind,
}

pub(crate) struct FuncState<'a> {
    code: Vec<Insn>,
    consts: Vec<Value>,
    num_consts: FxHashMap<u64, u32>,
    str_consts: FxHashMap<Atom, u32>,
    atoms: Vec<Atom>,
    atom_map: FxHashMap<Atom, u16>,
    ics: Vec<Ic>,
    funcs: Vec<Rc<FunctionProto>>,
    upvals: Vec<UpvalInfo>,
    handlers: Vec<Handler>,
    templates: Vec<TemplateSite>,
    regexps: Vec<RegexLiteral>,
    bindings: Vec<Binding>,
    scopes: Vec<Scope>,
    controls: Vec<Control>,
    tries: Vec<TryEntry<'a>>,
    pending_labels: Vec<Name>,
    /// Jumps to the end of the optional chains being compiled
    chain_exits: Vec<Vec<u32>>,
    next_reg: u16,
    max_reg: u16,
    nparams: u16,
    rest_reg: Option<Reg>,
    arguments_binding: Option<usize>,
    is_script: bool,
    is_arrow: bool,
    strict: bool,
    kind: FunctionKind,
    no_fused: bool,
    uses_this: bool,
    uses_super: bool,
    is_generator: bool,
    is_async: bool,
    /// Script completion value register
    completion: Option<Reg>,
    /// Active `with` statements: (object register, first binding inside)
    withs: Vec<(Reg, usize)>,
    /// Lazily compiled function: names of the precomputed upvalues
    lazy_names: Option<Vec<Name>>,
    /// A module's top-level code (its declarations are module bindings)
    is_module: bool,
    /// Inline caches shared by name once a function has very many
    shared_ics: FxHashMap<Atom, u16>,
}

impl<'a> FuncState<'a> {
    fn new(is_script: bool, is_arrow: bool, strict: bool, kind: FunctionKind, no_fused: bool) -> Self {
        FuncState {
            code: Vec::new(),
            consts: Vec::new(),
            num_consts: FxHashMap::default(),
            str_consts: FxHashMap::default(),
            atoms: Vec::new(),
            atom_map: FxHashMap::default(),
            ics: Vec::new(),
            funcs: Vec::new(),
            upvals: Vec::new(),
            handlers: Vec::new(),
            templates: Vec::new(),
            regexps: Vec::new(),
            bindings: Vec::new(),
            scopes: Vec::new(),
            controls: Vec::new(),
            tries: Vec::new(),
            pending_labels: Vec::new(),
            chain_exits: Vec::new(),
            next_reg: 0,
            max_reg: 0,
            nparams: 0,
            rest_reg: None,
            arguments_binding: None,
            is_script,
            is_arrow,
            strict,
            kind,
            no_fused,
            uses_this: false,
            uses_super: false,
            is_generator: false,
            is_async: false,
            completion: None,
            withs: Vec::new(),
            lazy_names: None,
            is_module: false,
            shared_ics: FxHashMap::default(),
        }
    }
}

/// How a name resolved
#[derive(Clone, Copy)]
pub(crate) enum Res {
    Local(usize),
    Upval(u16),
    Global(Atom),
}

pub(crate) struct Compiler<'a, 'h> {
    heap: &'h Heap,
    atoms: &'h mut Atoms,
    src: &'a str,
    fs: Vec<FuncState<'a>>,
    /// Shared copy of `src` kept by lazy functions
    src_rc: Option<Rc<str>>,
    /// Compiling a lazy function: its upvalues (by name) and the
    /// strictness of its definition
    lazy_root: Option<(Vec<UpvalInfo>, Vec<Name>, bool)>,
    /// Compiling module code
    in_module: bool,
}

/// Compile the body of a lazy function (on its first call)
pub fn compile_lazy(heap: &Heap, atoms: &mut Atoms, proto: &FunctionProto) -> Result<Code, SyntaxError> {
    let info = proto.lazy.as_ref().expect("not a lazy function");
    let reparse = crate::parser::ReparseInfo {
        params_start: info.params_start,
        span_start: proto.source.0,
        kind: info.kind,
        is_async: proto.is_async,
        is_generator: proto.is_generator,
        outer_strict: info.outer_strict,
        in_module: info.in_module,
    };
    let src: &str = &info.source;
    let mut func = crate::parser::reparse_function(src, &reparse)?;
    func.name = info.fn_name.clone();
    let upvals: Vec<UpvalInfo> = proto
        .upvals
        .iter()
        .zip(&info.upval_names)
        .map(|(&desc, (_, checked, kind))| UpvalInfo { desc, checked: *checked, kind: BindKind::from_u8(*kind) })
        .collect();
    let names: Vec<Name> = info.upval_names.iter().map(|(n, _, _)| n.clone()).collect();
    let mut c = Compiler {
        heap,
        atoms,
        src,
        fs: Vec::new(),
        src_rc: Some(info.source.clone()),
        lazy_root: Some((upvals, names, info.outer_strict)),
        in_module: info.in_module,
    };
    // The function borrows from `func`, which lives until the end
    let func: &Function = unsafe { &*(&func as *const Function) };
    match c.function(func, Some(proto.name), info.is_expression) {
        Ok((compiled, _)) => {
            let compiled = Rc::try_unwrap(compiled).ok().expect("fresh function prototype");
            Ok(compiled.compiled.into_inner().expect("compiled"))
        }
        Err(CErr::Retry) => Err(SyntaxError { message: "function too large".into(), pos: 0 }),
        Err(CErr::Syntax(e)) => Err(e),
    }
}

impl<'a, 'h> Compiler<'a, 'h> {
    // ---- errors ----

    fn error<T>(&self, message: impl Into<String>) -> CResult<T> {
        Err(CErr::Syntax(SyntaxError { message: message.into(), pos: 0 }))
    }

    // ---- emission ----

    #[inline]
    fn f(&mut self) -> &mut FuncState<'a> {
        self.fs.last_mut().unwrap()
    }

    #[inline]
    fn fr(&self) -> &FuncState<'a> {
        self.fs.last().unwrap()
    }

    fn emit(&mut self, insn: Insn) -> u32 {
        let f = self.f();
        f.code.push(insn);
        f.code.len() as u32 - 1
    }

    fn pc(&self) -> u32 {
        self.fr().code.len() as u32
    }

    /// Point the jump at `at` to `target`
    fn patch(&mut self, at: u32, target: u32) -> CResult<()> {
        let off = target as i64 - (at as i64 + 1);
        let short = i16::try_from(off).ok();
        let off = off as i32;
        let insn = &mut self.f().code[at as usize];
        let ok = match insn {
            Insn::Jmp { off: o }
            | Insn::JmpTrue { off: o, .. }
            | Insn::JmpFalse { off: o, .. }
            | Insn::JmpNullish { off: o, .. }
            | Insn::JmpNotNullish { off: o, .. }
            | Insn::JmpUndefined { off: o, .. }
            | Insn::JmpNotUndefined { off: o, .. }
            | Insn::JmpHole { off: o, .. }
            | Insn::JmpNotHole { off: o, .. } => {
                *o = off;
                true
            }
            Insn::JmpLt { off: o, .. }
            | Insn::JmpLe { off: o, .. }
            | Insn::JmpGt { off: o, .. }
            | Insn::JmpGe { off: o, .. }
            | Insn::JmpNLt { off: o, .. }
            | Insn::JmpNLe { off: o, .. }
            | Insn::JmpNGt { off: o, .. }
            | Insn::JmpNGe { off: o, .. }
            | Insn::JmpStrictEq { off: o, .. }
            | Insn::JmpStrictNe { off: o, .. } => match short {
                Some(s) => {
                    *o = s;
                    true
                }
                None => false,
            },
            other => panic!("patching a non-jump {other:?}"),
        };
        if ok { Ok(()) } else { Err(CErr::Retry) }
    }

    fn patch_here(&mut self, jumps: Vec<u32>) -> CResult<()> {
        let target = self.pc();
        for j in jumps {
            self.patch(j, target)?;
        }
        Ok(())
    }

    fn jump(&mut self) -> u32 {
        self.emit(Insn::Jmp { off: 0 })
    }

    fn jump_to(&mut self, target: u32) -> CResult<()> {
        let at = self.jump();
        self.patch(at, target)
    }

    // ---- registers ----

    fn alloc(&mut self) -> CResult<Reg> {
        self.alloc_n(1)
    }

    /// `n` consecutive registers
    fn alloc_n(&mut self, n: u16) -> CResult<Reg> {
        let f = self.f();
        let r = f.next_reg;
        match r.checked_add(n) {
            Some(next) if next < u16::MAX - 2 => {
                f.next_reg = next;
                f.max_reg = f.max_reg.max(next);
                Ok(r)
            }
            _ => self.error("function uses too many registers"),
        }
    }

    #[inline]
    fn mark(&self) -> Reg {
        self.fr().next_reg
    }

    #[inline]
    fn release(&mut self, mark: Reg) {
        self.f().next_reg = mark;
    }

    // ---- constants and names ----

    fn intern(&mut self, name: &str) -> Atom {
        self.atoms.intern_str(self.heap, name)
    }

    fn intern_units(&mut self, units: &[u16]) -> Atom {
        self.atoms.intern_units(self.heap, &Units::Utf16(units))
    }

    fn atom_index(&mut self, atom: Atom) -> CResult<u16> {
        let f = self.f();
        if let Some(&i) = f.atom_map.get(&atom) {
            return Ok(i);
        }
        let i = f.atoms.len();
        if i >= u16::MAX as usize {
            return self.error("too many names in one function");
        }
        f.atoms.push(atom);
        f.atom_map.insert(atom, i as u16);
        Ok(i as u16)
    }

    fn name_index(&mut self, name: &str) -> CResult<u16> {
        let atom = self.intern(name);
        self.atom_index(atom)
    }

    fn new_ic(&mut self, atom: Atom) -> CResult<u16> {
        let f = self.f();
        let i = f.ics.len();
        // Huge functions (whole bundles wrapped in one function, mostly
        // run once) share an uncached inline cache per name past this
        // many sites, so an index still fits in an instruction
        if i >= SHARED_ICS_FROM {
            if let Some(&shared) = f.shared_ics.get(&atom) {
                return Ok(shared);
            }
            if i >= u16::MAX as usize {
                return self.error("too many property names in one function");
            }
            f.ics.push(Ic { atom, state: IcState::Megamorphic });
            f.shared_ics.insert(atom, i as u16);
            return Ok(i as u16);
        }
        f.ics.push(Ic { atom, state: IcState::Empty });
        Ok(i as u16)
    }

    fn const_index(&mut self, value: Value) -> CResult<u32> {
        let f = self.f();
        let i = f.consts.len() as u32;
        f.consts.push(value);
        Ok(i)
    }

    fn load_number(&mut self, n: f64, dst: Reg) -> CResult<()> {
        let v = Value::number(n);
        if let Some(i) = v.as_int() {
            self.emit(Insn::LoadInt { dst, value: i });
            return Ok(());
        }
        let bits = v.raw();
        let idx = match self.fr().num_consts.get(&bits) {
            Some(&i) => i,
            None => {
                let i = self.const_index(v)?;
                self.f().num_consts.insert(bits, i);
                i
            }
        };
        self.emit(Insn::LoadConst { dst, idx });
        Ok(())
    }

    /// Load a string constant (interned, so it compares and converts to a
    /// property key quickly)
    fn load_atom_string(&mut self, atom: Atom, dst: Reg) -> CResult<()> {
        let idx = match self.fr().str_consts.get(&atom) {
            Some(&i) => i,
            None => {
                let s = self.atoms.string(atom);
                let i = self.const_index(Value::string(s))?;
                self.f().str_consts.insert(atom, i);
                i
            }
        };
        self.emit(Insn::LoadConst { dst, idx });
        Ok(())
    }

    fn load_string(&mut self, units: &[u16], dst: Reg) -> CResult<()> {
        let atom = self.intern_units(units);
        self.load_atom_string(atom, dst)
    }

    // ---- scopes and bindings ----

    fn push_scope(&mut self, switch_like: bool) {
        let f = self.f();
        let scope = Scope { first_binding: f.bindings.len(), reg_start: f.next_reg, close_patches: Vec::new(), switch_like };
        f.scopes.push(scope);
    }

    /// Leave a scope: close its captured variables and free its registers
    fn pop_scope(&mut self) {
        let f = self.f();
        let scope = f.scopes.pop().unwrap();
        let captured = f.bindings[scope.first_binding..].iter().any(|b| b.captured);
        f.bindings.truncate(scope.first_binding);
        if captured {
            for at in scope.close_patches {
                let from = match f.code[at as usize] {
                    Insn::CloseUpvals { from } => from.min(scope.reg_start),
                    _ => scope.reg_start,
                };
                f.code[at as usize] = Insn::CloseUpvals { from };
            }
            f.code.push(Insn::CloseUpvals { from: scope.reg_start });
        }
        f.next_reg = scope.reg_start;
    }

    /// Declare a binding in the innermost scope
    fn declare(&mut self, name: &Name, kind: BindKind) -> CResult<usize> {
        let first = self.fr().scopes.last().unwrap().first_binding;
        if let Some(existing) = self.fr().bindings[first..].iter().rposition(|b| b.name == *name) {
            let existing = first + existing;
            match (self.fr().bindings[existing].kind, kind) {
                // Implicit bindings are shadowed
                (BindKind::Callee | BindKind::Internal, _) => {}
                // Duplicate parameters (sloppy mode): the last one wins
                (BindKind::Param, BindKind::Param) => {}
                (BindKind::Var | BindKind::Param, BindKind::Var) => return Ok(existing),
                _ => return self.error(format!("Identifier '{name}' has already been declared")),
            }
        }
        let reg = self.alloc()?;
        Ok(self.add_binding(name.clone(), reg, kind))
    }

    fn add_binding(&mut self, name: Name, reg: Reg, kind: BindKind) -> usize {
        let initialized = !kind.is_lexical();
        let f = self.f();
        f.bindings.push(Binding { name, reg, kind, initialized, captured: false, hole_patch: None, init_patch: None });
        f.bindings.len() - 1
    }

    /// Declare a `let`/`const`/`class` binding with a dead-zone placeholder
    fn declare_lexical(&mut self, name: &Name, kind: BindKind) -> CResult<usize> {
        let b = self.declare(name, kind)?;
        let at = self.emit(Insn::Nop);
        self.f().bindings[b].hole_patch = Some(at);
        Ok(b)
    }

    fn find_local(&self, level: usize, name: &str) -> Option<usize> {
        self.fs[level].bindings.iter().rposition(|b| &*b.name == name)
    }

    /// The binding `b` of function `level` may be read before
    /// initialization: make scope entry reset it
    fn need_hole(&mut self, level: usize, b: usize) {
        let f = &mut self.fs[level];
        let binding = &mut f.bindings[b];
        if let Some(at) = binding.hole_patch.take() {
            f.code[at as usize] = Insn::LoadHole { dst: binding.reg };
        }
    }

    fn touch(&mut self, level: usize, b: usize) {
        let f = &mut self.fs[level];
        let binding = &mut f.bindings[b];
        if let Some((at, insn)) = binding.init_patch.take() {
            f.code[at as usize] = insn;
        }
        if matches!(binding.kind, BindKind::This) {
            f.uses_this = true;
        }
    }

    fn resolve(&mut self, name: &str) -> Res {
        let level = self.fs.len() - 1;
        self.resolve_at(level, name)
    }

    fn resolve_at(&mut self, level: usize, name: &str) -> Res {
        if let Some(b) = self.find_local(level, name) {
            self.touch(level, b);
            return Res::Local(b);
        }
        if let Some(names) = &self.fs[level].lazy_names {
            return match names.iter().position(|n| &**n == name) {
                Some(i) => Res::Upval(i as u16),
                None => Res::Global(self.intern(name)),
            };
        }
        if level == 0 || (self.fs[level].is_script) {
            return Res::Global(self.intern(name));
        }
        let (desc, checked, kind) = match self.resolve_at(level - 1, name) {
            Res::Global(a) => return Res::Global(a),
            Res::Local(b) => {
                let parent = &mut self.fs[level - 1];
                let binding = &mut parent.bindings[b];
                binding.captured = true;
                let checked = binding.kind.is_lexical() && !binding.initialized;
                let desc = UpvalDesc { from_parent_reg: true, index: binding.reg };
                let kind = binding.kind;
                if checked {
                    self.need_hole(level - 1, b);
                }
                (desc, checked, kind)
            }
            Res::Upval(i) => {
                let u = self.fs[level - 1].upvals[i as usize];
                (UpvalDesc { from_parent_reg: false, index: i }, u.checked, u.kind)
            }
        };
        let f = &mut self.fs[level];
        if let Some(i) = f.upvals.iter().position(|u| u.desc == desc) {
            // A later capture may need the check an earlier one didn't
            f.upvals[i].checked |= checked;
            return Res::Upval(i as u16);
        }
        f.upvals.push(UpvalInfo { desc, checked, kind });
        Res::Upval(f.upvals.len() as u16 - 1)
    }

    /// Whether a local read needs a dead-zone check
    fn local_needs_check(&self, b: usize) -> bool {
        let binding = &self.fr().bindings[b];
        binding.kind.is_lexical() && !binding.initialized
    }

    /// Register of a local variable that can be read directly (after any
    /// dead-zone check), or None for upvalues and globals
    fn local_reg(&mut self, name: &str) -> CResult<Option<Reg>> {
        if !self.with_objects(name).is_empty() {
            return Ok(None);
        }
        self.local_reg_static(name)
    }

    /// `local_reg` ignoring `with` scopes
    fn local_reg_static(&mut self, name: &str) -> CResult<Option<Reg>> {
        match self.resolve(name) {
            Res::Local(b) => {
                let reg = self.fr().bindings[b].reg;
                if self.local_needs_check(b) {
                    let level = self.fs.len() - 1;
                    self.need_hole(level, b);
                    let name = self.fr().bindings[b].name.clone();
                    let name = self.name_index(&name)?;
                    self.emit(Insn::CheckInit { reg, name });
                }
                Ok(Some(reg))
            }
            _ => Ok(None),
        }
    }

    /// `with` objects that may provide `name`, innermost first
    pub(crate) fn with_objects(&mut self, name: &str) -> Vec<Reg> {
        if self.fr().withs.is_empty() || name == "this" || name == "new.target" || name.starts_with('%') || name.starts_with('#') {
            return Vec::new();
        }
        let binding = match self.resolve(name) {
            Res::Local(b) => Some(b),
            _ => None,
        };
        self.fr()
            .withs
            .iter()
            .rev()
            .filter(|(_, first)| binding.is_none_or(|b| b < *first))
            .map(|(r, _)| *r)
            .collect()
    }

    /// Emit the `with` object checks for `name`: for each object, jump to
    /// its handler if it has the property. Returns (object, jump) pairs.
    pub(crate) fn with_dispatch(&mut self, name: &str, objs: &[Reg]) -> CResult<Vec<(Reg, u32)>> {
        let n = self.name_index(name)?;
        let mut out = Vec::new();
        let mark = self.mark();
        let t = self.alloc()?;
        for &obj in objs {
            self.emit(Insn::WithHas { dst: t, obj, name: n });
            out.push((obj, self.emit(Insn::JmpTrue { cond: t, off: 0 })));
        }
        self.release(mark);
        Ok(out)
    }

    fn load_var(&mut self, name: &str, dst: Reg) -> CResult<()> {
        let objs = self.with_objects(name);
        if !objs.is_empty() {
            let hits = self.with_dispatch(name, &objs)?;
            self.load_var_static(name, dst)?;
            let mut ends = vec![self.jump()];
            let atom = self.intern(name);
            for (obj, j) in hits {
                self.patch_here(vec![j])?;
                let ic = self.new_ic(atom)?;
                self.emit(Insn::GetProp { dst, obj, ic });
                ends.push(self.jump());
            }
            return self.patch_here(ends);
        }
        self.load_var_static(name, dst)
    }

    pub(crate) fn load_var_static(&mut self, name: &str, dst: Reg) -> CResult<()> {
        if let Some(reg) = self.local_reg_static(name)? {
            if reg != dst {
                self.emit(Insn::Mov { dst, src: reg });
            }
            return Ok(());
        }
        match self.resolve(name) {
            Res::Local(_) => unreachable!(),
            Res::Upval(idx) => {
                let u = self.fr().upvals[idx as usize];
                if u.checked {
                    let name = self.name_index(name)?;
                    self.emit(Insn::GetUpvalChecked { dst, idx, name });
                } else {
                    self.emit(Insn::GetUpval { dst, idx });
                }
            }
            Res::Global(atom) => match name {
                "undefined" => {
                    self.emit(Insn::LoadUndef { dst });
                }
                "NaN" => self.load_number(f64::NAN, dst)?,
                "Infinity" => self.load_number(f64::INFINITY, dst)?,
                _ => {
                    let ic = self.new_ic(atom)?;
                    self.emit(Insn::GetGlobal { dst, ic });
                }
            },
        }
        Ok(())
    }

    /// Assign to a variable. `init` marks the declaration's own
    /// initialization (no const or dead-zone checks).
    fn store_var(&mut self, name: &str, src: Reg, init: bool) -> CResult<()> {
        let objs = if init { Vec::new() } else { self.with_objects(name) };
        if !objs.is_empty() {
            let hits = self.with_dispatch(name, &objs)?;
            self.store_var_static(name, src, init)?;
            let mut ends = vec![self.jump()];
            let atom = self.intern(name);
            for (obj, j) in hits {
                self.patch_here(vec![j])?;
                let ic = self.new_ic(atom)?;
                self.emit(Insn::SetProp { obj, src, ic });
                ends.push(self.jump());
            }
            return self.patch_here(ends);
        }
        self.store_var_static(name, src, init)
    }

    fn store_var_static(&mut self, name: &str, src: Reg, init: bool) -> CResult<()> {
        let level = self.fs.len() - 1;
        match self.resolve(name) {
            Res::Local(b) => {
                let (reg, kind) = {
                    let binding = &self.fr().bindings[b];
                    (binding.reg, binding.kind)
                };
                if !init {
                    match kind {
                        BindKind::Const | BindKind::Class => {
                            if self.local_needs_check(b) {
                                self.need_hole(level, b);
                                let n = self.name_index(name)?;
                                self.emit(Insn::CheckInit { reg, name: n });
                            }
                            let n = self.name_index(name)?;
                            self.emit(Insn::ThrowConstAssign { name: n });
                            return Ok(());
                        }
                        BindKind::Callee => {
                            if self.fr().strict {
                                let n = self.name_index(name)?;
                                self.emit(Insn::ThrowConstAssign { name: n });
                            }
                            return Ok(());
                        }
                        _ => {}
                    }
                    if self.local_needs_check(b) {
                        self.need_hole(level, b);
                        let n = self.name_index(name)?;
                        self.emit(Insn::CheckInit { reg, name: n });
                    }
                }
                if reg != src {
                    self.emit(Insn::Mov { dst: reg, src });
                }
                if init && !self.in_switch_scope_of(b) {
                    self.f().bindings[b].initialized = true;
                }
            }
            Res::Upval(idx) if init => {
                // A module-scope declaration initializing its binding
                self.emit(Insn::SetUpval { src, idx });
            }
            Res::Upval(idx) => {
                let u = self.fr().upvals[idx as usize];
                let n = self.name_index(name)?;
                if u.checked {
                    let mark = self.mark();
                    let t = self.alloc()?;
                    self.emit(Insn::GetUpvalChecked { dst: t, idx, name: n });
                    self.release(mark);
                }
                match u.kind {
                    BindKind::Const | BindKind::Class => {
                        self.emit(Insn::ThrowConstAssign { name: n });
                        return Ok(());
                    }
                    BindKind::Callee => {
                        if self.fr().strict {
                            self.emit(Insn::ThrowConstAssign { name: n });
                        }
                        return Ok(());
                    }
                    _ => {}
                }
                self.emit(Insn::SetUpval { src, idx });
            }
            Res::Global(atom) => {
                if init && self.at_script_top() && self.is_global_lexical(name) {
                    let n = self.atom_index(atom)?;
                    self.emit(Insn::InitGlobalLex { src, name: n });
                } else {
                    let ic = self.new_ic(atom)?;
                    self.emit(Insn::SetGlobal { src, ic });
                }
            }
        }
        Ok(())
    }

    fn in_switch_scope_of(&self, b: usize) -> bool {
        let f = self.fr();
        f.scopes.iter().rev().find(|s| s.first_binding <= b).is_some_and(|s| s.switch_like)
    }

    /// Compiling the script's or module's top-level scope (not inside a
    /// block), whose declarations are not registers of the function
    fn at_script_top(&self) -> bool {
        self.fs.len() == 1 && (self.fr().is_script || self.fr().is_module) && self.fr().scopes.len() == 1
    }

    fn is_global_lexical(&self, _name: &str) -> bool {
        // Only called for declarations at the script top level, which are
        // lexical exactly when they are let/const/class: callers pass
        // `init` only for those (a top-level `var` initializer is an
        // assignment)
        true
    }

    // ---- modules ----

    /// Run `f`, again without fused short jumps if one overflowed
    fn with_retry<T>(&mut self, mut f: impl FnMut(&mut Self, bool) -> CResult<T>) -> Result<T, SyntaxError> {
        match f(self, false) {
            Ok(v) => Ok(v),
            Err(CErr::Retry) => {
                self.fs.clear();
                match f(self, true) {
                    Ok(v) => Ok(v),
                    Err(CErr::Retry) => Err(SyntaxError { message: "function too large".into(), pos: 0 }),
                    Err(CErr::Syntax(e)) => Err(e),
                }
            }
            Err(CErr::Syntax(e)) => Err(e),
        }
    }

    /// Start a function whose upvalues are the module scope
    fn push_module_function(&mut self, upvals: &[UpvalInfo], names: &[Name], no_fused: bool) -> CResult<()> {
        self.fs.push(FuncState::new(false, false, true, FunctionKind::Normal, no_fused));
        let f = self.f();
        f.upvals = upvals.to_vec();
        f.lazy_names = Some(names.to_vec());
        f.is_module = true;
        self.push_scope(false);
        // `this` is undefined at the top level of a module
        let this = self.alloc()?;
        self.add_binding(Rc::from("this"), this, BindKind::This);
        Ok(())
    }

    fn module_init(&mut self, module: &'a Module, upvals: &[UpvalInfo], names: &[Name], no_fused: bool) -> CResult<Rc<FunctionProto>> {
        self.push_module_function(upvals, names, no_fused)?;
        for stmt in &module.body {
            let Stmt::Function(func) = stmt else { continue };
            let Some(name) = &func.name else { continue };
            let shown = if &**name == DEFAULT_EXPORT { "default" } else { name };
            let atom = self.intern(shown);
            let mark = self.mark();
            let t = self.alloc()?;
            self.closure(func, Some(atom), t, false)?;
            let idx = names.iter().position(|n| n == name).unwrap_or(0) as u16;
            self.emit(Insn::SetUpval { src: t, idx });
            self.release(mark);
        }
        self.emit(Insn::ReturnUndef);
        let f = self.fs.pop().unwrap();
        Ok(self.finish(f, atoms::empty, 0, (0, self.src.len() as u32)))
    }

    fn module_body(&mut self, module: &'a Module, upvals: &[UpvalInfo], names: &[Name], no_fused: bool) -> CResult<Rc<FunctionProto>> {
        self.push_module_function(upvals, names, no_fused)?;
        self.f().is_async = module.has_await;
        let async_exc = if module.has_await {
            self.emit(Insn::AsyncStart);
            let r = self.alloc()?;
            self.open_try(TryKind::Catch, r);
            Some(r)
        } else {
            None
        };
        for stmt in &module.body {
            self.stmt(stmt)?;
        }
        self.emit_return_undef();
        if let Some(r) = async_exc {
            let entry = self.close_try();
            let target = self.pc();
            self.set_handler_target(&entry, target);
            self.emit(Insn::AsyncThrow { src: r });
        }
        let f = self.fs.pop().unwrap();
        Ok(self.finish(f, atoms::empty, 0, (0, self.src.len() as u32)))
    }

    // ---- functions ----

    fn script(&mut self, program: &'a Program, no_fused: bool) -> CResult<Rc<FunctionProto>> {
        self.fs.push(FuncState::new(true, false, program.strict, FunctionKind::Normal, no_fused));
        self.push_scope(false);
        let this = self.alloc()?;
        let this_name: Name = Rc::from("this");
        self.add_binding(this_name, this, BindKind::This);
        let completion = self.alloc()?;
        self.f().completion = Some(completion);

        // Hoisted declarations
        let mut vars = Vec::new();
        collect_vars(&program.body, &mut vars, !program.strict);
        for name in &vars {
            let n = self.name_index(name)?;
            self.emit(Insn::DeclareGlobalVar { name: n });
        }
        for stmt in &program.body {
            match stmt {
                Stmt::Var { kind, decls } if *kind != VarKind::Var => {
                    let mut names = Vec::new();
                    for d in decls {
                        pattern_names(&d.target, &mut names);
                    }
                    for name in names {
                        let n = self.name_index(&name)?;
                        self.emit(Insn::DeclareGlobalLex { name: n, is_const: *kind == VarKind::Const });
                    }
                }
                Stmt::Class(c) => {
                    if let Some(name) = &c.name {
                        let n = self.name_index(name)?;
                        self.emit(Insn::DeclareGlobalLex { name: n, is_const: false });
                    }
                }
                _ => {}
            }
        }
        for stmt in &program.body {
            if let Stmt::Function(func) = stmt {
                let mark = self.mark();
                let t = self.alloc()?;
                let name = func.name.as_ref().map(|n| self.intern(n));
                self.closure(func, name, t, false)?;
                let n = self.atom_index(name.unwrap_or(atoms::empty))?;
                self.emit(Insn::DeclareGlobalFunc { src: t, name: n });
                self.release(mark);
            }
        }
        // Annex B: functions declared in blocks are also global variables
        self.emit(Insn::LoadUndef { dst: completion });
        for stmt in &program.body {
            self.stmt(stmt)?;
        }
        self.emit(Insn::Return { src: completion });
        let f = self.fs.pop().unwrap();
        Ok(self.finish(f, atoms::empty, 0, (0, self.src.len() as u32)))
    }

    /// Compile a nested function into register `dst` as a closure.
    /// Returns whether it uses `super` (needs a home object).
    fn closure(&mut self, func: &'a Function, name: Option<Atom>, dst: Reg, is_expression: bool) -> CResult<bool> {
        let (proto, uses_super) = self.function(func, name, is_expression)?;
        let idx = self.add_func(proto)?;
        self.emit(Insn::Closure { dst, idx });
        Ok(uses_super)
    }

    /// `return r` (async functions settle their promise; async generators
    /// await the value first)
    pub(crate) fn emit_return(&mut self, r: Reg) {
        let (is_async, is_generator) = (self.fr().is_async, self.fr().is_generator);
        if is_async && is_generator {
            let mark = self.mark();
            if let Ok(t) = self.alloc() {
                self.emit(Insn::Await { dst: t, src: r });
                self.emit(Insn::Return { src: t });
            }
            self.release(mark);
        } else if is_async {
            self.emit(Insn::AsyncReturn { src: r });
        } else {
            self.emit(Insn::Return { src: r });
        }
    }

    pub(crate) fn emit_return_undef(&mut self) {
        if self.fr().is_async && !self.fr().is_generator {
            let mark = self.mark();
            if let Ok(t) = self.alloc() {
                self.emit(Insn::LoadUndef { dst: t });
                self.emit(Insn::AsyncReturn { src: t });
            }
            self.release(mark);
        } else {
            self.emit(Insn::ReturnUndef);
        }
    }

    fn add_func(&mut self, proto: Rc<FunctionProto>) -> CResult<u16> {
        let f = self.f();
        let idx = f.funcs.len();
        if idx >= u16::MAX as usize {
            return self.error("too many nested functions");
        }
        f.funcs.push(proto);
        Ok(idx as u16)
    }

    fn function(&mut self, func: &'a Function, name: Option<Atom>, is_expression: bool) -> CResult<(Rc<FunctionProto>, bool)> {
        let depth = self.fs.len();
        match self.function_attempt(func, name, is_expression, false) {
            Err(CErr::Retry) => {
                self.fs.truncate(depth);
                let r = self.function_attempt(func, name, is_expression, true);
                if r.is_err() {
                    self.fs.truncate(depth);
                }
                r
            }
            Err(e) => {
                self.fs.truncate(depth);
                Err(e)
            }
            ok => ok,
        }
    }

    fn function_attempt(&mut self, func: &'a Function, name: Option<Atom>, is_expression: bool, no_fused: bool) -> CResult<(Rc<FunctionProto>, bool)> {
        let is_arrow = func.kind == FunctionKind::Arrow;
        if let FunctionBody::Lazy(_) = func.body {
            return self.lazy_function(func, name, is_expression);
        }
        let lazy_root = if self.fs.is_empty() { self.lazy_root.clone() } else { None };
        let outer_strict = match &lazy_root {
            Some((_, _, s)) => *s,
            None => self.fr().strict,
        };
        let strict = func.strict || outer_strict;
        self.fs.push(FuncState::new(false, is_arrow, strict, func.kind, no_fused));
        if let Some((upvals, names, _)) = lazy_root {
            self.f().upvals = upvals;
            self.f().lazy_names = Some(names);
        }
        self.push_scope(false);
        let text = self.src.get(func.span.start as usize..func.span.end as usize).unwrap_or("");

        // Register 0: `this`
        let this = self.alloc()?;
        if !is_arrow {
            let kind = if func.kind == FunctionKind::DerivedConstructor { BindKind::ThisUninit } else { BindKind::This };
            self.add_binding(Rc::from("this"), this, kind);
        }

        // Parameters: registers 1..=nparams
        let nparams = func.params.len();
        if nparams > 1000 {
            return self.error("too many parameters");
        }
        let mut param_regs = Vec::with_capacity(nparams);
        for p in &func.params {
            match &p.target {
                Pattern::Ident(n) => {
                    let b = self.declare(n, BindKind::Param)?;
                    param_regs.push(self.fr().bindings[b].reg);
                }
                _ => param_regs.push(self.alloc()?),
            }
        }
        self.f().nparams = nparams as u16;
        let rest_reg = match &func.rest {
            Some(Pattern::Ident(n)) => {
                let b = self.declare(n, BindKind::Param)?;
                Some(self.fr().bindings[b].reg)
            }
            Some(_) => Some(self.alloc()?),
            None => None,
        };
        self.f().rest_reg = rest_reg;

        // Implicit bindings
        let mut init_patches = Vec::new();
        if !is_arrow {
            if text.contains("arguments") && self.find_local(self.fs.len() - 1, "arguments").is_none() {
                let b = self.declare(&Rc::from("arguments"), BindKind::Var)?;
                self.f().arguments_binding = Some(b);
            }
            let derived = func.kind == FunctionKind::DerivedConstructor;
            if derived || text.contains("target") {
                let b = self.declare(&Rc::from("new.target"), BindKind::Internal)?;
                let reg = self.fr().bindings[b].reg;
                init_patches.push((b, Insn::LoadNewTarget { dst: reg }));
            }
            if derived {
                let b = self.declare(&Rc::from("%ctor"), BindKind::Internal)?;
                let reg = self.fr().bindings[b].reg;
                init_patches.push((b, Insn::LoadCallee { dst: reg }));
            }
        }
        if is_expression {
            if let Some(n) = &func.name {
                // Shadowed by parameters and variables of the same name
                let reg = self.alloc()?;
                let f = self.f();
                f.bindings.insert(
                    0,
                    Binding { name: n.clone(), reg, kind: BindKind::Callee, initialized: true, captured: false, hole_patch: None, init_patch: None },
                );
                if let Some(ab) = &mut f.arguments_binding {
                    *ab += 1;
                }
                for (b, _) in &mut init_patches {
                    *b += 1;
                }
                init_patches.push((0, Insn::LoadCallee { dst: reg }));
            }
        }

        // Hoisted variables
        let body: &'a [Stmt] = match &func.body {
            FunctionBody::Block(stmts) => stmts,
            FunctionBody::Expr(_) | FunctionBody::Lazy(_) => &[],
        };
        let mut vars = Vec::new();
        collect_vars(body, &mut vars, !strict);
        for name in &vars {
            self.declare(name, BindKind::Var)?;
        }
        for (b, insn) in init_patches {
            let at = self.emit(Insn::Nop);
            self.f().bindings[b].init_patch = Some((at, insn));
        }

        // Defaults and destructuring
        if !func.simple_params {
            for (p, &reg) in func.params.iter().zip(&param_regs) {
                if let Some(default) = &p.default {
                    let skip = self.emit(Insn::JmpNotUndefined { src: reg, off: 0 });
                    let hint = match &p.target {
                        Pattern::Ident(n) => Some(n.clone()),
                        _ => None,
                    };
                    self.expr_named(default, reg, hint.as_deref())?;
                    self.patch_here(vec![skip])?;
                }
                if !matches!(p.target, Pattern::Ident(_)) {
                    self.declare_pattern(&p.target, BindKind::Param)?;
                    self.bind_pattern(&p.target, reg, Some(BindKind::Param))?;
                }
            }
            if let (Some(rest), Some(reg)) = (&func.rest, rest_reg) {
                if !matches!(rest, Pattern::Ident(_)) {
                    self.declare_pattern(rest, BindKind::Param)?;
                    self.bind_pattern(rest, reg, Some(BindKind::Param))?;
                }
            }
        }

        self.f().is_generator = func.is_generator;
        self.f().is_async = func.is_async;
        // Async functions settle their promise instead of throwing (async
        // generators settle the promise of each call instead)
        let async_exc = if func.is_async && !func.is_generator {
            self.emit(Insn::AsyncStart);
            let r = self.alloc()?;
            self.open_try(TryKind::Catch, r);
            Some(r)
        } else {
            None
        };
        match &func.body {
            FunctionBody::Block(stmts) => {
                self.hoist_block(stmts, true)?;
                if func.is_generator {
                    self.emit(Insn::GenStart);
                }
                for s in stmts {
                    self.stmt(s)?;
                }
                self.emit_return_undef();
            }
            FunctionBody::Expr(e) => {
                let mark = self.mark();
                let r = self.expr_any(e)?;
                self.emit_return(r);
                self.release(mark);
            }
            FunctionBody::Lazy(_) => unreachable!(),
        }
        if let Some(r) = async_exc {
            let entry = self.close_try();
            let target = self.pc();
            self.set_handler_target(&entry, target);
            self.emit(Insn::AsyncThrow { src: r });
        }

        let f = self.fs.pop().unwrap();
        let uses_super = f.uses_super;
        // Arrows see their parent's `super`
        if is_arrow && uses_super && !self.fs.is_empty() {
            self.f().uses_super = true;
        }
        let name = name.or_else(|| func.name.as_ref().map(|n| self.intern(n))).unwrap_or(atoms::empty);
        let length = func.params.iter().take_while(|p| p.default.is_none()).count() as u16;
        let proto = self.finish(f, name, length, (func.span.start, func.span.end));
        Ok((proto, uses_super))
    }

    /// A function compiled on its first call: resolve the names it may
    /// capture now (capturing them from this scope), keep its source
    fn lazy_function(&mut self, func: &'a Function, name: Option<Atom>, is_expression: bool) -> CResult<(Rc<FunctionProto>, bool)> {
        let FunctionBody::Lazy(lb) = &func.body else { unreachable!() };
        let is_arrow = func.kind == FunctionKind::Arrow;
        let outer_strict = self.fr().strict;
        let strict = func.strict || outer_strict;
        self.fs.push(FuncState::new(false, is_arrow, strict, func.kind, false));
        let level = self.fs.len() - 1;
        let mut uses_super = false;
        let mut names: Vec<(Name, u16)> = Vec::new();
        for n in &lb.free {
            if &**n == "super" {
                uses_super = true;
                continue;
            }
            if let Res::Upval(i) = self.resolve_at(level, n) {
                names.push((n.clone(), i));
            }
        }
        let f = self.fs.pop().unwrap();
        if is_arrow && uses_super {
            self.f().uses_super = true;
        }
        // Upvalue order is creation order; map each to its name
        let mut upval_names: Vec<(Name, bool, u8)> = Vec::with_capacity(f.upvals.len());
        for (i, u) in f.upvals.iter().enumerate() {
            let n = names.iter().find(|(_, j)| *j as usize == i).map(|(n, _)| n.clone()).unwrap_or_else(|| Rc::from(""));
            upval_names.push((n, u.checked, u.kind.to_u8()));
        }
        let source = self.src_rc.get_or_insert_with(|| Rc::from(self.src)).clone();
        let name = name.or_else(|| func.name.as_ref().map(|n| self.intern(n))).unwrap_or(atoms::empty);
        let is_constructor = matches!(func.kind, FunctionKind::Normal | FunctionKind::ClassConstructor | FunctionKind::DerivedConstructor)
            && !func.is_generator
            && !func.is_async;
        let proto = FunctionProto {
            name,
            upvals: f.upvals.iter().map(|u| u.desc).collect(),
            nparams: lb.nparams as u16,
            length: lb.length as u16,
            strict,
            is_arrow,
            is_constructor,
            is_class_constructor: matches!(func.kind, FunctionKind::ClassConstructor | FunctionKind::DerivedConstructor),
            is_derived: func.kind == FunctionKind::DerivedConstructor,
            is_generator: func.is_generator,
            is_async: func.is_async,
            source: (func.span.start, func.span.end),
            traced: Cell::new(0),
            lazy: Some(Box::new(LazyInfo {
                source,
                params_start: func.params_start,
                kind: func.kind,
                fn_name: func.name.clone(),
                is_expression,
                outer_strict,
                upval_names,
                in_module: self.in_module,
            })),
            compiled: std::cell::OnceCell::new(),
        };
        Ok((Rc::new(proto), uses_super))
    }

    /// A function whose body the compiler generates (class field
    /// initializers, default constructors, static blocks)
    fn synthetic(
        &mut self,
        kind: FunctionKind,
        name: Atom,
        span: (u32, u32),
        body: &dyn Fn(&mut Self) -> CResult<()>,
    ) -> CResult<(Rc<FunctionProto>, bool)> {
        let depth = self.fs.len();
        let mut no_fused = false;
        loop {
            self.fs.push(FuncState::new(false, false, true, kind, no_fused));
            self.push_scope(false);
            let this = self.alloc()?;
            let this_kind = if kind == FunctionKind::DerivedConstructor { BindKind::ThisUninit } else { BindKind::This };
            self.add_binding(Rc::from("this"), this, this_kind);
            match body(self) {
                Ok(()) => {}
                Err(CErr::Retry) if !no_fused => {
                    self.fs.truncate(depth);
                    no_fused = true;
                    continue;
                }
                Err(e) => {
                    self.fs.truncate(depth);
                    return Err(e);
                }
            }
            self.emit(Insn::ReturnUndef);
            let f = self.fs.pop().unwrap();
            let uses_super = f.uses_super;
            return Ok((self.finish(f, name, 0, span), uses_super));
        }
    }

    fn finish(&mut self, mut f: FuncState<'a>, name: Atom, length: u16, source: (u32, u32)) -> Rc<FunctionProto> {
        strip_nops(&mut f.code, &mut f.handlers);
        let arguments_reg = f.arguments_binding.map(|b| f.bindings.get(b).map(|b| b.reg).unwrap_or(0));
        let arguments_reg = if f.arguments_binding.is_some() { arguments_reg } else { None };
        let is_constructor = matches!(f.kind, FunctionKind::Normal | FunctionKind::ClassConstructor | FunctionKind::DerivedConstructor)
            && !f.is_script
            && !f.is_generator
            && !f.is_async;
        let code = Code {
            code: f.code.into_boxed_slice(),
            consts: f.consts.into_boxed_slice(),
            atoms: f.atoms.into_boxed_slice(),
            funcs: f.funcs.into_boxed_slice(),
            ics: f.ics.into_iter().map(Cell::new).collect(),
            handlers: f.handlers.into_boxed_slice(),
            templates: f.templates.into_boxed_slice(),
            regexps: f.regexps.into_boxed_slice(),
            nregs: f.max_reg.max(1),
            coerce_this: !f.strict && f.uses_this,
            arguments_reg,
            rest_reg: f.rest_reg,
        };
        Rc::new(FunctionProto {
            name,
            upvals: f.upvals.iter().map(|u| u.desc).collect(),
            nparams: f.nparams,
            length,
            strict: f.strict,
            is_arrow: f.is_arrow,
            is_constructor,
            is_class_constructor: matches!(f.kind, FunctionKind::ClassConstructor | FunctionKind::DerivedConstructor),
            is_derived: f.kind == FunctionKind::DerivedConstructor,
            is_generator: f.is_generator,
            is_async: f.is_async,
            source,
            traced: Cell::new(0),
            lazy: None,
            compiled: std::cell::OnceCell::from(code),
        })
    }

    /// Declarations at the start of a block or function body: lexical
    /// bindings (with dead-zone placeholders) and function declarations
    /// (created immediately)
    fn hoist_block(&mut self, stmts: &'a [Stmt], function_top: bool) -> CResult<()> {
        let script_top = self.at_script_top();
        for s in stmts {
            match s {
                Stmt::Var { kind, decls } if *kind != VarKind::Var && !script_top => {
                    let bk = if *kind == VarKind::Const { BindKind::Const } else { BindKind::Let };
                    let mut names = Vec::new();
                    for d in decls {
                        pattern_names(&d.target, &mut names);
                    }
                    for n in names {
                        self.declare_lexical(&n, bk)?;
                    }
                }
                // A class declaration binds like `let`; only the class's
                // own name inside its body is immutable
                Stmt::Class(c) if !script_top => {
                    if let Some(n) = &c.name {
                        self.declare_lexical(n, BindKind::Let)?;
                    }
                }
                Stmt::Function(func) if !function_top && !script_top => {
                    if let Some(n) = &func.name {
                        self.declare(n, BindKind::Let)?;
                    }
                }
                _ => {}
            }
        }
        if script_top {
            return Ok(());
        }
        for s in stmts {
            if let Stmt::Function(func) = s {
                let Some(n) = &func.name else { continue };
                let b = self.find_local(self.fs.len() - 1, n).unwrap();
                let reg = self.fr().bindings[b].reg;
                let atom = self.intern(n);
                self.closure(func, Some(atom), reg, false)?;
                self.f().bindings[b].initialized = true;
                if !function_top && !self.fr().strict {
                    // Annex B: also assign the function-level variable
                    self.annex_b_assign(n, reg)?;
                }
            }
        }
        Ok(())
    }

    /// Sloppy-mode block function: copy into the enclosing var binding
    fn annex_b_assign(&mut self, name: &Name, reg: Reg) -> CResult<()> {
        let level = self.fs.len() - 1;
        if self.fr().is_script {
            let atom = self.intern(name);
            let ic = self.new_ic(atom)?;
            self.emit(Insn::SetGlobal { src: reg, ic });
            return Ok(());
        }
        let first_scope_end = self.fr().scopes.get(1).map(|s| s.first_binding).unwrap_or(self.fr().bindings.len());
        if let Some(b) = self.fs[level].bindings[..first_scope_end].iter().rposition(|b| b.name == *name && b.kind == BindKind::Var) {
            let target = self.fr().bindings[b].reg;
            self.emit(Insn::Mov { dst: target, src: reg });
        }
        Ok(())
    }

    /// Declare every name bound by a pattern
    fn declare_pattern(&mut self, pat: &'a Pattern, kind: BindKind) -> CResult<()> {
        let mut names = Vec::new();
        pattern_names(pat, &mut names);
        for n in names {
            if kind.is_lexical() {
                self.declare_lexical(&n, kind)?;
            } else {
                self.declare(&n, kind)?;
            }
        }
        Ok(())
    }

    // ---- exits through finally blocks ----

    fn open_try(&mut self, kind: TryKind<'a>, reg: Reg) {
        let seg_start = self.pc();
        self.f().tries.push(TryEntry { kind, seg_start, handler_idxs: Vec::new(), reg });
    }

    fn close_segment(&mut self, entry: &mut TryEntry<'a>) {
        let end = self.pc();
        if end > entry.seg_start {
            let f = self.f();
            let finally = !matches!(entry.kind, TryKind::Catch);
            f.handlers.push(Handler { start: entry.seg_start, end, target: 0, reg: entry.reg, finally });
            entry.handler_idxs.push(f.handlers.len() - 1);
        }
        entry.seg_start = end;
    }

    /// End a try entry's protection; returns it for setting the target
    fn close_try(&mut self) -> TryEntry<'a> {
        let mut entry = self.f().tries.pop().unwrap();
        self.close_segment(&mut entry);
        entry
    }

    fn set_handler_target(&mut self, entry: &TryEntry<'a>, target: u32) {
        for &i in &entry.handler_idxs {
            self.f().handlers[i].target = target;
        }
    }

    /// Run the finalizers of the try entries from `depth` up (innermost
    /// first) before a jump out of them. Returns the exited entries, to be
    /// handed back to `reenter_tries` after the jump.
    fn exit_tries(&mut self, depth: usize) -> CResult<Vec<TryEntry<'a>>> {
        let mut exited: Vec<TryEntry<'a>> = self.f().tries.drain(depth..).collect();
        for k in (0..exited.len()).rev() {
            self.close_segment(&mut exited[k]);
            match exited[k].kind {
                TryKind::Catch => {}
                TryKind::IterClose(reg) | TryKind::AsyncIterClose(reg) => {
                    // Still protected by the outer entries
                    let outer: Vec<TryEntry<'a>> = exited.drain(..k).collect();
                    self.f().tries.extend(outer);
                    if matches!(exited[0].kind, TryKind::AsyncIterClose(_)) {
                        let mark = self.mark();
                        let t = self.alloc()?;
                        self.emit(Insn::AsyncIterReturn { dst: t, iter: reg });
                        self.emit(Insn::Await { dst: t, src: t });
                        self.release(mark);
                    } else {
                        self.emit(Insn::IterClose { iter: reg });
                    }
                    let n = self.fr().tries.len();
                    let outer: Vec<TryEntry<'a>> = self.f().tries.drain(n - k..).collect();
                    exited.splice(0..0, outer);
                }
                TryKind::Finally(block) => {
                    let outer: Vec<TryEntry<'a>> = exited.drain(..k).collect();
                    self.f().tries.extend(outer);
                    self.block(block)?;
                    let n = self.fr().tries.len();
                    let outer: Vec<TryEntry<'a>> = self.f().tries.drain(n - k..).collect();
                    exited.splice(0..0, outer);
                }
            }
        }
        Ok(exited)
    }

    fn reenter_tries(&mut self, mut exited: Vec<TryEntry<'a>>) {
        let pc = self.pc();
        for e in &mut exited {
            e.seg_start = pc;
        }
        self.f().tries.extend(exited);
    }

    /// Placeholder closing the upvalues of the scopes an early exit leaves
    fn close_scopes_from(&mut self, depth: usize) {
        if self.fr().scopes.len() <= depth {
            return;
        }
        let at = self.emit(Insn::Nop);
        let f = self.f();
        for s in &mut f.scopes[depth..] {
            s.close_patches.push(at);
        }
    }
}

/// Remove `Nop`s (unused placeholders) and fix up jump offsets and handler
/// ranges
fn strip_nops(code: &mut Vec<Insn>, handlers: &mut [Handler]) {
    if !code.iter().any(|i| matches!(i, Insn::Nop)) {
        return;
    }
    // new_index[old] = position of the first kept instruction at or after old
    let mut new_index = Vec::with_capacity(code.len() + 1);
    let mut n = 0u32;
    for insn in code.iter() {
        new_index.push(n);
        if !matches!(insn, Insn::Nop) {
            n += 1;
        }
    }
    new_index.push(n);
    let mut out = Vec::with_capacity(n as usize);
    for (old, insn) in code.iter().enumerate() {
        if matches!(insn, Insn::Nop) {
            continue;
        }
        let mut insn = *insn;
        let fix = |off: i64| -> i64 {
            let target = (old as i64 + 1 + off) as usize;
            new_index[target] as i64 - (new_index[old] as i64 + 1)
        };
        match &mut insn {
            Insn::Jmp { off }
            | Insn::JmpTrue { off, .. }
            | Insn::JmpFalse { off, .. }
            | Insn::JmpNullish { off, .. }
            | Insn::JmpNotNullish { off, .. }
            | Insn::JmpUndefined { off, .. }
            | Insn::JmpNotUndefined { off, .. }
            | Insn::JmpHole { off, .. }
            | Insn::JmpNotHole { off, .. } => *off = fix(*off as i64) as i32,
            Insn::JmpLt { off, .. }
            | Insn::JmpLe { off, .. }
            | Insn::JmpGt { off, .. }
            | Insn::JmpGe { off, .. }
            | Insn::JmpNLt { off, .. }
            | Insn::JmpNLe { off, .. }
            | Insn::JmpNGt { off, .. }
            | Insn::JmpNGe { off, .. }
            | Insn::JmpStrictEq { off, .. }
            | Insn::JmpStrictNe { off, .. } => *off = fix(*off as i64) as i16,
            _ => {}
        }
        out.push(insn);
    }
    for h in handlers.iter_mut() {
        h.start = new_index[h.start as usize];
        h.end = new_index[h.end as usize];
        h.target = new_index[h.target as usize];
    }
    *code = out;
}

// ---- declaration scans ----

/// Names bound by a pattern
pub(crate) fn pattern_names(pat: &Pattern, out: &mut Vec<Name>) {
    match pat {
        Pattern::Ident(n) => out.push(n.clone()),
        Pattern::Member(_) => {}
        Pattern::Array { elems, rest } => {
            for e in elems.iter().flatten() {
                pattern_names(&e.target, out);
            }
            if let Some(r) = rest {
                pattern_names(r, out);
            }
        }
        Pattern::Object { props, rest } => {
            for p in props {
                pattern_names(&p.target, out);
            }
            if let Some(r) = rest {
                pattern_names(r, out);
            }
        }
    }
}

/// `var` declarations of a function body (not nested functions), plus
/// top-level function declarations and, in sloppy mode, functions declared
/// in blocks (Annex B)
fn collect_vars(stmts: &[Stmt], out: &mut Vec<Name>, annex_b: bool) {
    for s in stmts {
        collect_stmt_vars(s, out, annex_b, true);
    }
}

fn push_unique(out: &mut Vec<Name>, name: Name) {
    if !out.contains(&name) {
        out.push(name);
    }
}

fn collect_stmt_vars(s: &Stmt, out: &mut Vec<Name>, annex_b: bool, top: bool) {
    let mut names = Vec::new();
    match s {
        Stmt::Var { kind: VarKind::Var, decls } => {
            for d in decls {
                pattern_names(&d.target, &mut names);
            }
        }
        Stmt::Function(f) => {
            if top || annex_b {
                if let Some(n) = &f.name {
                    names.push(n.clone());
                }
            }
        }
        Stmt::If { cons, alt, .. } => {
            collect_stmt_vars(cons, out, annex_b, false);
            if let Some(a) = alt {
                collect_stmt_vars(a, out, annex_b, false);
            }
        }
        Stmt::Block(b) => {
            for s in b {
                collect_stmt_vars(s, out, annex_b, false);
            }
        }
        Stmt::For { init, body, .. } => {
            if let Some(ForInit::Var(VarKind::Var, decls)) = init {
                for d in decls {
                    pattern_names(&d.target, &mut names);
                }
            }
            collect_stmt_vars(body, out, annex_b, false);
        }
        Stmt::ForIn { head, body, .. } | Stmt::ForOf { head, body, .. } => {
            if let ForHead::Decl(VarKind::Var, p) = head {
                pattern_names(p, &mut names);
            }
            collect_stmt_vars(body, out, annex_b, false);
        }
        Stmt::While { body, .. } | Stmt::DoWhile { body, .. } | Stmt::Labeled { body, .. } | Stmt::With { body, .. } => {
            collect_stmt_vars(body, out, annex_b, false)
        }
        Stmt::Try { block, handler, finalizer, .. } => {
            for s in block {
                collect_stmt_vars(s, out, annex_b, false);
            }
            for s in handler.iter().flatten() {
                collect_stmt_vars(s, out, annex_b, false);
            }
            for s in finalizer.iter().flatten() {
                collect_stmt_vars(s, out, annex_b, false);
            }
        }
        Stmt::Switch { cases, .. } => {
            for c in cases {
                for s in &c.body {
                    collect_stmt_vars(s, out, annex_b, false);
                }
            }
        }
        _ => {}
    }
    for n in names {
        push_unique(out, n);
    }
}

/// An anonymous function or class, which takes its name from the binding
/// or property it is assigned to
fn is_anonymous_function(e: &Expr) -> bool {
    match e {
        Expr::Function(f) => f.name.is_none(),
        Expr::Class(c) => c.name.is_none(),
        Expr::Paren(inner) => is_anonymous_function(inner),
        _ => false,
    }
}

/// Whether evaluating `e` into a register writes that register only as
/// its final step (so a variable's own register can be the destination
/// even when `e` reads the variable)
fn writes_dst_last(e: &Expr) -> bool {
    match e {
        Expr::Num(_)
        | Expr::Str(_)
        | Expr::Bool(_)
        | Expr::Null
        | Expr::Ident(_)
        | Expr::This
        | Expr::Unary { .. }
        | Expr::Binary { .. }
        | Expr::Call { .. }
        | Expr::New { .. }
        | Expr::Member { .. }
        | Expr::Function(_)
        | Expr::Regex { .. }
        | Expr::TaggedTemplate { .. }
        | Expr::Update { .. } => true,
        Expr::Paren(inner) => writes_dst_last(inner),
        Expr::Cond { cons, alt, .. } => writes_dst_last(cons) && writes_dst_last(alt),
        _ => false,
    }
}

/// Free of side effects that could change a variable (so a variable read
/// on the left of an operator can use the variable's register directly
/// while the right side is evaluated)
fn is_simple(e: &Expr) -> bool {
    match e {
        Expr::Num(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Null | Expr::Ident(_) | Expr::This | Expr::Regex { .. } => true,
        Expr::Paren(inner) => is_simple(inner),
        Expr::Member { object, prop, .. } => {
            is_simple(object)
                && match prop {
                    MemberProp::Computed(k) => is_simple(k),
                    _ => true,
                }
        }
        Expr::Unary { op, arg } => *op != UnaryOp::Delete && is_simple(arg),
        Expr::Binary { left, right, .. } => is_simple(left) && is_simple(right),
        Expr::Logical { left, right, .. } => is_simple(left) && is_simple(right),
        Expr::Cond { test, cons, alt } => is_simple(test) && is_simple(cons) && is_simple(alt),
        Expr::Template(t) => t.exprs.iter().all(is_simple),
        _ => false,
    }
}

#[cfg(test)]
mod tests;
