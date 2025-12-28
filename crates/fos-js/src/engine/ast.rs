//! Abstract Syntax Tree
//!
//! AST node definitions for JavaScript.

use super::token::Span;

/// AST Node ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub u32);

/// Abstract Syntax Tree container
#[derive(Debug, Default)]
pub struct Ast {
    nodes: Vec<AstNode>,
    root: Option<NodeId>,
}

impl Ast {
    pub fn new() -> Self { Self::default() }
    
    pub fn add_node(&mut self, node: AstNode) -> NodeId {
        let id = NodeId(self.nodes.len() as u32);
        self.nodes.push(node);
        id
    }
    
    pub fn get(&self, id: NodeId) -> Option<&AstNode> { self.nodes.get(id.0 as usize) }
    pub fn set_root(&mut self, id: NodeId) { self.root = Some(id); }
    pub fn root(&self) -> Option<NodeId> { self.root }
    pub fn len(&self) -> usize { self.nodes.len() }
    pub fn is_empty(&self) -> bool { self.nodes.is_empty() }
}

/// AST Node
#[derive(Debug, Clone)]
pub struct AstNode {
    pub kind: AstNodeKind,
    pub span: Span,
}

impl AstNode {
    pub fn new(kind: AstNodeKind, span: Span) -> Self { Self { kind, span } }
}

/// AST Node kinds
#[derive(Debug, Clone)]
pub enum AstNodeKind {
    Program { body: Vec<NodeId> },
    ExpressionStatement { expr: NodeId },
    BlockStatement { body: Vec<NodeId> },
    ReturnStatement { argument: Option<NodeId> },
    IfStatement { test: NodeId, consequent: NodeId, alternate: Option<NodeId> },
    WhileStatement { test: NodeId, body: NodeId },
    ForStatement { init: Option<NodeId>, test: Option<NodeId>, update: Option<NodeId>, body: NodeId },
    BreakStatement,
    ContinueStatement,
    VariableDeclaration { kind: VarKind, declarations: Vec<NodeId> },
    VariableDeclarator { id: NodeId, init: Option<NodeId> },
    FunctionDeclaration { id: Option<NodeId>, params: Vec<NodeId>, body: NodeId, is_async: bool },
    Identifier { name: Box<str> },
    Literal { value: LiteralValue },
    ArrayExpression { elements: Vec<Option<NodeId>> },
    ObjectExpression { properties: Vec<NodeId> },
    Property { key: NodeId, value: NodeId, computed: bool },
    FunctionExpression { id: Option<NodeId>, params: Vec<NodeId>, body: NodeId, is_async: bool },
    ArrowFunctionExpression { params: Vec<NodeId>, body: NodeId, is_async: bool },
    UnaryExpression { operator: UnaryOp, argument: NodeId },
    BinaryExpression { operator: BinaryOp, left: NodeId, right: NodeId },
    LogicalExpression { operator: LogicalOp, left: NodeId, right: NodeId },
    AssignmentExpression { operator: AssignOp, left: NodeId, right: NodeId },
    ConditionalExpression { test: NodeId, consequent: NodeId, alternate: NodeId },
    CallExpression { callee: NodeId, arguments: Vec<NodeId> },
    NewExpression { callee: NodeId, arguments: Vec<NodeId> },
    MemberExpression { object: NodeId, property: NodeId, computed: bool },
    ThisExpression,
    SpreadElement { argument: NodeId },
    AwaitExpression { argument: NodeId },
    TemplateLiteral { quasis: Vec<NodeId>, expressions: Vec<NodeId> },
    
    // Error handling
    TryStatement { block: NodeId, handler: Option<NodeId>, finalizer: Option<NodeId> },
    CatchClause { param: Option<NodeId>, body: NodeId },
    ThrowStatement { argument: NodeId },
    
    // Classes
    ClassDeclaration { id: Option<NodeId>, superclass: Option<NodeId>, body: NodeId },
    ClassExpression { id: Option<NodeId>, superclass: Option<NodeId>, body: NodeId },
    ClassBody { body: Vec<NodeId> },
    MethodDefinition { key: NodeId, value: NodeId, kind: MethodKind, is_static: bool, computed: bool },
    SuperExpression,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarKind { Var, Let, Const }

#[derive(Debug, Clone)]
pub enum LiteralValue { Null, Bool(bool), Number(f64), String(Box<str>) }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp { Minus, Plus, Not, BitwiseNot, Typeof, Void, Delete }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add, Sub, Mul, Div, Mod, Pow, LessThan, LessThanEq, GreaterThan, GreaterThanEq,
    Equal, NotEqual, StrictEqual, StrictNotEqual, LeftShift, RightShift, UnsignedRightShift,
    BitwiseAnd, BitwiseOr, BitwiseXor, In, Instanceof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicalOp { And, Or, NullishCoalescing }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp { Assign, AddAssign, SubAssign, MulAssign, DivAssign, ModAssign }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodKind { Constructor, Method, Get, Set }

