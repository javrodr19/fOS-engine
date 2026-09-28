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
    Throw(Expr),
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
    Call { callee: Box<Expr>, args: Vec<ArrayElem>, optional: bool },
    /// `super(...)`
    SuperCall(Vec<ArrayElem>),
    New { callee: Box<Expr>, args: Vec<ArrayElem> },
    Member { object: Box<Expr>, prop: MemberProp, optional: bool },
    /// `super.x` / `super[x]`
    SuperMember(MemberProp),
    /// An optional chain (`a?.b.c`): short-circuits as a whole
    OptionalChain(Box<Expr>),
    Seq(Vec<Expr>),
    Yield { arg: Option<Box<Expr>>, delegate: bool },
    Await(Box<Expr>),
    /// Parenthesized expression (kept so `(a) = 1` and `(a, b) =>` parse right)
    Paren(Box<Expr>),
    /// Dynamic `import(specifier)`
    Import(Box<Expr>),
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
