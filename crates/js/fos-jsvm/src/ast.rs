//! Abstract syntax tree
//!
//! Plain owned trees: the compiler walks each function once, so there is no
//! need for an arena or parent links.

use std::rc::Rc;

use crate::lexer::{Span, Utf16};

/// Identifier name
pub type Name = Rc<str>;

#[derive(Debug)]
pub struct Program {
    pub body: Vec<Stmt>,
    pub strict: bool,
}

/// Binding of a module's default export when it has no name of its own
/// (`export default 1 + 2`, `export default function () {}`)
pub const DEFAULT_EXPORT: &str = "*default*";
/// Hidden module binding holding the `import.meta` object
pub const MODULE_META: &str = "%meta";
/// Hidden module binding holding the module's URL (the referrer of the
/// `import()` calls in it)
pub const MODULE_REFERRER: &str = "%referrer";

/// A module: its code, with `import` declarations removed and `export`
/// keywords stripped from declarations, plus what it imports and exports
#[derive(Debug)]
pub struct Module {
    pub body: Vec<Stmt>,
    /// The modules it imports from, in source order, without duplicates
    pub requests: Vec<ModuleRequest>,
    pub imports: Vec<ImportEntry>,
    pub exports: Vec<ExportEntry>,
    /// Uses `await` outside any function (top-level await)
    pub has_await: bool,
}

/// A module specifier with its import attributes
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleRequest {
    pub specifier: Rc<str>,
    /// `with { type: "json" }`
    pub json: bool,
}

/// `import { import as local } from requests[request]`
#[derive(Debug, Clone)]
pub struct ImportEntry {
    pub request: usize,
    pub import: ImportName,
    pub local: Name,
}

/// What an import or re-export takes from a module
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportName {
    /// An export by name (`default` for default imports)
    Name(Rc<str>),
    /// The module namespace object (`* as ns`)
    Namespace,
}

#[derive(Debug, Clone)]
pub enum ExportEntry {
    /// A binding of this module: `export { local as export }`,
    /// `export let x`, `export default ...`
    Local { export: Rc<str>, local: Name },
    /// A re-export: `export { a as b } from "m"`, `export * as ns from "m"`
    Indirect { export: Rc<str>, request: usize, import: ImportName },
    /// `export * from "m"`
    Star { request: usize },
}

impl ExportEntry {
    /// The name it exports under (None for `export *`)
    pub fn export_name(&self) -> Option<&Rc<str>> {
        match self {
            ExportEntry::Local { export, .. } | ExportEntry::Indirect { export, .. } => Some(export),
            ExportEntry::Star { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarKind {
    Var,
    Let,
    Const,
}

#[derive(Debug)]
pub enum Stmt {
    Expr(Expr),
    Var { kind: VarKind, decls: Vec<VarDecl> },
    Function(Box<Function>),
    Class(Box<Class>),
    Return(Option<Expr>),
    If { test: Expr, cons: Box<Stmt>, alt: Option<Box<Stmt>> },
    Block(Vec<Stmt>),
    For { init: Option<ForInit>, test: Option<Expr>, update: Option<Expr>, body: Box<Stmt> },
    ForIn { head: ForHead, object: Expr, body: Box<Stmt> },
    ForOf { head: ForHead, iterable: Expr, body: Box<Stmt>, is_await: bool },
    While { test: Expr, body: Box<Stmt> },
    DoWhile { body: Box<Stmt>, test: Expr },
    Break(Option<Name>),
    Continue(Option<Name>),
    /// The thrown value and the `throw` keyword's source offset
    Throw(Expr, u32),
    Try { block: Vec<Stmt>, param: Option<Pattern>, handler: Option<Vec<Stmt>>, finalizer: Option<Vec<Stmt>> },
    Switch { discriminant: Expr, cases: Vec<SwitchCase> },
    Labeled { label: Name, body: Box<Stmt> },
    With { object: Expr, body: Box<Stmt> },
    Empty,
    Debugger,
}

#[derive(Debug)]
pub struct SwitchCase {
    /// `None` for `default:`
    pub test: Option<Expr>,
    pub body: Vec<Stmt>,
}

#[derive(Debug)]
pub struct VarDecl {
    pub target: Pattern,
    pub init: Option<Expr>,
}

#[derive(Debug)]
pub enum ForInit {
    Var(VarKind, Vec<VarDecl>),
    Expr(Expr),
}

/// Left side of `for (... in/of ...)`
#[derive(Debug)]
pub enum ForHead {
    /// A declaration: `for (let x of ...)`
    Decl(VarKind, Pattern),
    /// An assignment target: `for (x.y of ...)`
    Target(Pattern),
}

/// Binding or assignment target
#[derive(Debug)]
pub enum Pattern {
    Ident(Name),
    /// Member expression (assignment targets only)
    Member(Box<Expr>),
    Array { elems: Vec<Option<PatternElem>>, rest: Option<Box<Pattern>> },
    Object { props: Vec<PatternProp>, rest: Option<Box<Pattern>> },
}

#[derive(Debug)]
pub struct PatternElem {
    pub target: Pattern,
    pub default: Option<Expr>,
}

#[derive(Debug)]
pub struct PatternProp {
    pub key: PropKey,
    pub target: Pattern,
    pub default: Option<Expr>,
}

#[derive(Debug)]
pub enum PropKey {
    /// Identifier or string key known at compile time
    Name(Utf16),
    Num(f64),
    Computed(Box<Expr>),
    Private(Name),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Plus,
    Not,
    BitNot,
    Typeof,
    Void,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Exp,
    Shl,
    Sar,
    Shr,
    BitAnd,
    BitOr,
    BitXor,
    Eq,
    Ne,
    StrictEq,
    StrictNe,
    Lt,
    Le,
    Gt,
    Ge,
    In,
    Instanceof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicalOp {
    And,
    Or,
    Nullish,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Assign,
    /// Compound arithmetic assignment (`+=` etc.)
    Op(BinaryOp),
    /// `&&=`, `||=`, `??=`
    Logical(LogicalOp),
}

#[derive(Debug)]
pub enum Expr {
    Num(f64),
    Str(Utf16),
    BigInt(Box<str>),
    Bool(bool),
    Null,
    Template(Box<Template>),
    TaggedTemplate { tag: Box<Expr>, template: Rc<Template> },
    Regex { pattern: Box<str>, flags: Box<str> },
    Ident(Name),
    This,
    /// `new.target`
    NewTarget,
    Array(Vec<Option<ArrayElem>>),
    Object(Vec<ObjProp>),
    Function(Box<Function>),
    Class(Box<Class>),
    Unary { op: UnaryOp, arg: Box<Expr> },
    Update { inc: bool, prefix: bool, target: Box<Expr> },
    Binary { op: BinaryOp, left: Box<Expr>, right: Box<Expr> },
    Logical { op: LogicalOp, left: Box<Expr>, right: Box<Expr> },
    Assign { op: AssignOp, target: Box<Pattern>, value: Box<Expr> },
    Cond { test: Box<Expr>, cons: Box<Expr>, alt: Box<Expr> },
    /// `pos`: source offset for stack traces (the callee's property, or the `(`)
    Call { callee: Box<Expr>, args: Vec<ArrayElem>, optional: bool, pos: u32 },
    /// `super(...)`
    SuperCall(Vec<ArrayElem>),
    New { callee: Box<Expr>, args: Vec<ArrayElem>, pos: u32 },
    /// `pos`: source offset of the property (for stack traces)
    Member { object: Box<Expr>, prop: MemberProp, optional: bool, pos: u32 },
    /// `super.x` / `super[x]`
    SuperMember(MemberProp),
    /// An optional chain (`a?.b.c`): short-circuits as a whole
    OptionalChain(Box<Expr>),
    Seq(Vec<Expr>),
    Yield { arg: Option<Box<Expr>>, delegate: bool },
    Await(Box<Expr>),
    /// Parenthesized expression (kept so `(a) = 1` and `(a, b) =>` parse right)
    Paren(Box<Expr>),
    /// Dynamic `import(specifier, options)`
    Import { spec: Box<Expr>, options: Option<Box<Expr>> },
    /// `import.meta`
    ImportMeta,
}

#[derive(Debug)]
pub enum ArrayElem {
    Expr(Expr),
    Spread(Expr),
}

#[derive(Debug)]
pub enum MemberProp {
    Name(Name),
    Computed(Box<Expr>),
    Private(Name),
}

#[derive(Debug)]
pub struct Template {
    /// Cooked strings (`None` where an escape is invalid; tagged only)
    pub cooked: Vec<Option<Utf16>>,
    pub raw: Vec<Box<str>>,
    pub exprs: Vec<Expr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodKind {
    Method,
    Getter,
    Setter,
}

#[derive(Debug)]
pub enum ObjProp {
    KeyValue(PropKey, Expr),
    /// `{ x }`
    Shorthand(Name),
    /// `{ x = 1 }`: only valid when the object becomes a pattern
    CoverInit(Name, Expr),
    Method { key: PropKey, kind: MethodKind, func: Box<Function> },
    Spread(Expr),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionKind {
    Normal,
    Arrow,
    Method,
    Getter,
    Setter,
    ClassConstructor,
    DerivedConstructor,
}

#[derive(Debug)]
pub struct Param {
    pub target: Pattern,
    pub default: Option<Expr>,
}

#[derive(Debug)]
pub enum FunctionBody {
    Block(Vec<Stmt>),
    /// Concise arrow body
    Expr(Box<Expr>),
    /// Not kept: compiled on first call by parsing the source again
    Lazy(Box<LazyBody>),
}

/// What the enclosing code needs to know about a function whose body is
/// compiled later
#[derive(Debug)]
pub struct LazyBody {
    /// Names the function may take from enclosing scopes (a superset),
    /// including the pseudo-names "this", "arguments", "new.target",
    /// "%ctor", "super" and "#private" names
    pub free: Vec<Name>,
    pub nparams: u32,
    /// Function.prototype.length
    pub length: u32,
}

#[derive(Debug)]
pub struct Function {
    pub name: Option<Name>,
    pub params: Vec<Param>,
    pub rest: Option<Pattern>,
    pub body: FunctionBody,
    pub kind: FunctionKind,
    pub is_async: bool,
    pub is_generator: bool,
    pub strict: bool,
    /// Parameters are plain identifiers without defaults or rest
    pub simple_params: bool,
    pub span: Span,
    /// Where parsing resumes to compile a lazy body: the `(` of the
    /// parameters (arrows: the start of the arrow)
    pub params_start: u32,
}

#[derive(Debug)]
pub enum ClassMemberKind {
    Method(MethodKind, Box<Function>),
    Field(Option<Expr>),
    StaticBlock(Vec<Stmt>),
}

#[derive(Debug)]
pub struct ClassMember {
    pub key: PropKey,
    pub is_static: bool,
    pub kind: ClassMemberKind,
}

#[derive(Debug)]
pub struct Class {
    pub name: Option<Name>,
    pub extends: Option<Expr>,
    pub constructor: Option<Box<Function>>,
    pub members: Vec<ClassMember>,
    pub span: Span,
}

// ---- free names (for lazily compiled functions) ----

/// Referenced names, deduplicated as they are collected
#[derive(Default)]
struct Refs {
    set: std::collections::HashSet<Name>,
    list: Vec<Name>,
}

impl Refs {
    fn push(&mut self, n: Name) {
        if self.set.insert(n.clone()) {
            self.list.push(n);
        }
    }

    fn extend(&mut self, names: impl IntoIterator<Item = Name>) {
        for n in names {
            self.push(n);
        }
    }
}

fn pattern_refs(p: &Pattern, out: &mut Refs) {
    match p {
        Pattern::Ident(n) => out.push(n.clone()),
        Pattern::Member(e) => expr_refs(e, out),
        Pattern::Array { elems, rest } => {
            for e in elems.iter().flatten() {
                pattern_refs(&e.target, out);
                if let Some(d) = &e.default {
                    expr_refs(d, out);
                }
            }
            if let Some(r) = rest {
                pattern_refs(r, out);
            }
        }
        Pattern::Object { props, rest } => {
            for p in props {
                key_refs(&p.key, out);
                pattern_refs(&p.target, out);
                if let Some(d) = &p.default {
                    expr_refs(d, out);
                }
            }
            if let Some(r) = rest {
                pattern_refs(r, out);
            }
        }
    }
}

fn key_refs(k: &PropKey, out: &mut Refs) {
    match k {
        PropKey::Computed(e) => expr_refs(e, out),
        PropKey::Private(n) => out.push(Rc::from(format!("#{n}"))),
        _ => {}
    }
}

fn member_refs(p: &MemberProp, out: &mut Refs) {
    match p {
        MemberProp::Computed(e) => expr_refs(e, out),
        MemberProp::Private(n) => out.push(Rc::from(format!("#{n}"))),
        MemberProp::Name(_) => {}
    }
}

fn elems_refs(v: &[ArrayElem], out: &mut Refs) {
    for a in v {
        match a {
            ArrayElem::Expr(e) | ArrayElem::Spread(e) => expr_refs(e, out),
        }
    }
}

fn function_refs(f: &Function, out: &mut Refs) {
    match &f.body {
        FunctionBody::Lazy(l) => out.extend(l.free.iter().cloned()),
        _ => out.extend(function_free_names(f)),
    }
}

fn class_refs(c: &Class, out: &mut Refs) {
    if let Some(e) = &c.extends {
        expr_refs(e, out);
    }
    if let Some(f) = &c.constructor {
        function_refs(f, out);
    }
    for m in &c.members {
        key_refs(&m.key, out);
        match &m.kind {
            ClassMemberKind::Method(_, f) => function_refs(f, out),
            ClassMemberKind::Field(Some(e)) => expr_refs(e, out),
            ClassMemberKind::Field(None) => {}
            ClassMemberKind::StaticBlock(b) => stmts_refs(b, out),
        }
    }
}

fn expr_refs(e: &Expr, out: &mut Refs) {
    match e {
        Expr::Num(_) | Expr::Str(_) | Expr::BigInt(_) | Expr::Bool(_) | Expr::Null | Expr::Regex { .. } => {}
        Expr::Template(t) => t.exprs.iter().for_each(|x| expr_refs(x, out)),
        Expr::TaggedTemplate { tag, template } => {
            expr_refs(tag, out);
            template.exprs.iter().for_each(|x| expr_refs(x, out));
        }
        Expr::Ident(n) => out.push(n.clone()),
        Expr::This => out.push(Rc::from("this")),
        Expr::NewTarget => out.push(Rc::from("new.target")),
        Expr::Array(v) => {
            for a in v.iter().flatten() {
                match a {
                    ArrayElem::Expr(e) | ArrayElem::Spread(e) => expr_refs(e, out),
                }
            }
        }
        Expr::Object(props) => {
            for p in props {
                match p {
                    ObjProp::KeyValue(k, v) => {
                        key_refs(k, out);
                        expr_refs(v, out);
                    }
                    ObjProp::Shorthand(n) => out.push(n.clone()),
                    ObjProp::CoverInit(n, v) => {
                        out.push(n.clone());
                        expr_refs(v, out);
                    }
                    ObjProp::Method { key, func, .. } => {
                        key_refs(key, out);
                        function_refs(func, out);
                    }
                    ObjProp::Spread(e) => expr_refs(e, out),
                }
            }
        }
        Expr::Function(f) => function_refs(f, out),
        Expr::Class(c) => class_refs(c, out),
        Expr::Unary { arg, .. } => expr_refs(arg, out),
        Expr::Update { target, .. } => expr_refs(target, out),
        Expr::Binary { left, right, .. } | Expr::Logical { left, right, .. } => {
            expr_refs(left, out);
            expr_refs(right, out);
        }
        Expr::Assign { target, value, .. } => {
            pattern_refs(target, out);
            expr_refs(value, out);
        }
        Expr::Cond { test, cons, alt } => {
            expr_refs(test, out);
            expr_refs(cons, out);
            expr_refs(alt, out);
        }
        Expr::Call { callee, args, .. } | Expr::New { callee, args, .. } => {
            expr_refs(callee, out);
            elems_refs(args, out);
        }
        Expr::SuperCall(args) => {
            for n in ["%ctor", "new.target", "this"] {
                out.push(Rc::from(n));
            }
            elems_refs(args, out);
        }
        Expr::Member { object, prop, .. } => {
            expr_refs(object, out);
            member_refs(prop, out);
        }
        Expr::SuperMember(p) => {
            out.push(Rc::from("super"));
            out.push(Rc::from("this"));
            member_refs(p, out);
        }
        Expr::OptionalChain(e) | Expr::Await(e) | Expr::Paren(e) => expr_refs(e, out),
        Expr::Import { spec, options } => {
            out.push(Rc::from(MODULE_REFERRER));
            expr_refs(spec, out);
            if let Some(o) = options {
                expr_refs(o, out);
            }
        }
        Expr::ImportMeta => out.push(Rc::from(MODULE_META)),
        Expr::Seq(v) => v.iter().for_each(|x| expr_refs(x, out)),
        Expr::Yield { arg, .. } => {
            if let Some(a) = arg {
                expr_refs(a, out);
            }
        }
    }
}

fn stmts_refs(v: &[Stmt], out: &mut Refs) {
    for s in v {
        stmt_refs(s, out);
    }
}

fn stmt_refs(s: &Stmt, out: &mut Refs) {
    match s {
        Stmt::Expr(e) | Stmt::Throw(e, _) => expr_refs(e, out),
        Stmt::Var { decls, .. } => {
            for d in decls {
                pattern_refs(&d.target, out);
                if let Some(i) = &d.init {
                    expr_refs(i, out);
                }
            }
        }
        Stmt::Function(f) => {
            if let Some(n) = &f.name {
                out.push(n.clone());
            }
            function_refs(f, out);
        }
        Stmt::Class(c) => {
            if let Some(n) = &c.name {
                out.push(n.clone());
            }
            class_refs(c, out);
        }
        Stmt::Return(e) => {
            if let Some(e) = e {
                expr_refs(e, out);
            }
        }
        Stmt::If { test, cons, alt } => {
            expr_refs(test, out);
            stmt_refs(cons, out);
            if let Some(a) = alt {
                stmt_refs(a, out);
            }
        }
        Stmt::Block(b) => stmts_refs(b, out),
        Stmt::For { init, test, update, body } => {
            match init {
                Some(ForInit::Var(_, decls)) => {
                    for d in decls {
                        pattern_refs(&d.target, out);
                        if let Some(i) = &d.init {
                            expr_refs(i, out);
                        }
                    }
                }
                Some(ForInit::Expr(e)) => expr_refs(e, out),
                None => {}
            }
            if let Some(t) = test {
                expr_refs(t, out);
            }
            if let Some(u) = update {
                expr_refs(u, out);
            }
            stmt_refs(body, out);
        }
        Stmt::ForIn { head, object: e, body } | Stmt::ForOf { head, iterable: e, body, .. } => {
            match head {
                ForHead::Decl(_, p) | ForHead::Target(p) => pattern_refs(p, out),
            }
            expr_refs(e, out);
            stmt_refs(body, out);
        }
        Stmt::While { test, body } | Stmt::DoWhile { body, test } => {
            expr_refs(test, out);
            stmt_refs(body, out);
        }
        Stmt::Try { block, param, handler, finalizer } => {
            stmts_refs(block, out);
            if let Some(p) = param {
                pattern_refs(p, out);
            }
            if let Some(h) = handler {
                stmts_refs(h, out);
            }
            if let Some(f) = finalizer {
                stmts_refs(f, out);
            }
        }
        Stmt::Switch { discriminant, cases } => {
            expr_refs(discriminant, out);
            for c in cases {
                if let Some(t) = &c.test {
                    expr_refs(t, out);
                }
                stmts_refs(&c.body, out);
            }
        }
        Stmt::Labeled { body, .. } => stmt_refs(body, out),
        Stmt::With { object, body } => {
            expr_refs(object, out);
            stmt_refs(body, out);
        }
        Stmt::Break(_) | Stmt::Continue(_) | Stmt::Empty | Stmt::Debugger => {}
    }
}

fn bound_names(p: &Pattern, out: &mut Refs) {
    match p {
        Pattern::Ident(n) => out.push(n.clone()),
        Pattern::Member(_) => {}
        Pattern::Array { elems, rest } => {
            for e in elems.iter().flatten() {
                bound_names(&e.target, out);
            }
            if let Some(r) = rest {
                bound_names(r, out);
            }
        }
        Pattern::Object { props, rest } => {
            for p in props {
                bound_names(&p.target, out);
            }
            if let Some(r) = rest {
                bound_names(r, out);
            }
        }
    }
}

/// Free-name collection for a function whose statements are dropped as
/// soon as they are parsed
#[derive(Default)]
pub struct FreeNames {
    refs: Refs,
    own: Refs,
}

impl FreeNames {
    pub fn param(&mut self, p: &Param) {
        pattern_refs(&p.target, &mut self.refs);
        if let Some(d) = &p.default {
            expr_refs(d, &mut self.refs);
        }
        bound_names(&p.target, &mut self.own);
    }

    pub fn rest(&mut self, p: &Pattern) {
        pattern_refs(p, &mut self.refs);
        bound_names(p, &mut self.own);
    }

    /// A top-level statement of the function body
    pub fn stmt(&mut self, s: &Stmt) {
        stmt_refs(s, &mut self.refs);
        match s {
            Stmt::Var { decls, .. } => decls.iter().for_each(|d| bound_names(&d.target, &mut self.own)),
            Stmt::Function(g) => self.own.extend(g.name.clone()),
            Stmt::Class(c) => self.own.extend(c.name.clone()),
            _ => {}
        }
    }

    pub fn expr(&mut self, e: &Expr) {
        expr_refs(e, &mut self.refs);
    }

    pub fn finish(mut self, kind: FunctionKind) -> Vec<Name> {
        if kind != FunctionKind::Arrow {
            for n in ["this", "arguments", "new.target", "%ctor"] {
                self.own.push(Rc::from(n));
            }
        }
        let mut list = self.refs.list;
        list.retain(|n| !self.own.set.contains(n));
        list.shrink_to_fit();
        list
    }
}

/// Names a function may take from its enclosing scopes. A superset:
/// references are only discounted when a parameter or a top-level
/// declaration of the function itself binds them.
pub fn function_free_names(f: &Function) -> Vec<Name> {
    let mut refs = Refs::default();
    for p in &f.params {
        pattern_refs(&p.target, &mut refs);
        if let Some(d) = &p.default {
            expr_refs(d, &mut refs);
        }
    }
    if let Some(r) = &f.rest {
        pattern_refs(r, &mut refs);
    }
    let body: &[Stmt] = match &f.body {
        FunctionBody::Block(b) => {
            stmts_refs(b, &mut refs);
            b
        }
        FunctionBody::Expr(e) => {
            expr_refs(e, &mut refs);
            &[]
        }
        FunctionBody::Lazy(l) => return l.free.clone(),
    };
    // Names bound for the whole function
    let mut own = Refs::default();
    for p in &f.params {
        bound_names(&p.target, &mut own);
    }
    if let Some(r) = &f.rest {
        bound_names(r, &mut own);
    }
    for s in body {
        match s {
            Stmt::Var { decls, .. } => decls.iter().for_each(|d| bound_names(&d.target, &mut own)),
            Stmt::Function(g) => own.extend(g.name.clone()),
            Stmt::Class(c) => own.extend(c.name.clone()),
            _ => {}
        }
    }
    if f.kind != FunctionKind::Arrow {
        for n in ["this", "arguments", "new.target", "%ctor"] {
            own.push(Rc::from(n));
        }
    }
    let mut list = refs.list;
    list.retain(|n| !own.set.contains(n));
    list.shrink_to_fit();
    list
}
