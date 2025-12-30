//! Abstract Syntax Tree
//!
//! AST node definitions for JavaScript.
//! Uses arena-style allocation with Vec<AstNode> and NodeId indices.

use super::token::Span;

/// AST Node ID - Compact reference (4 bytes)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub u32);

/// Abstract Syntax Tree container
/// Pre-allocates space for typical source files.
#[derive(Debug)]
pub struct Ast {
    nodes: Vec<AstNode>,
    root: Option<NodeId>,
}

impl Default for Ast {
    fn default() -> Self {
        Self {
            // Pre-allocate for ~500 nodes (typical medium-sized file)
            nodes: Vec::with_capacity(512),
            root: None,
        }
    }
}

impl Ast {
    pub fn new() -> Self { Self::default() }
    
    /// Create with specific capacity for large files
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            nodes: Vec::with_capacity(capacity),
            root: None,
        }
    }
    
    #[inline]
    pub fn add_node(&mut self, node: AstNode) -> NodeId {
        let id = NodeId(self.nodes.len() as u32);
        self.nodes.push(node);
        id
    }
    
    #[inline]
    pub fn get(&self, id: NodeId) -> Option<&AstNode> { self.nodes.get(id.0 as usize) }
    
    #[inline]
    pub fn set_root(&mut self, id: NodeId) { self.root = Some(id); }
    
    #[inline]
    pub fn root(&self) -> Option<NodeId> { self.root }
    
    #[inline]
    pub fn len(&self) -> usize { self.nodes.len() }
    
    #[inline]
    pub fn is_empty(&self) -> bool { self.nodes.is_empty() }
    
    /// Memory usage in bytes (approximate)
    pub fn memory_usage(&self) -> usize {
        std::mem::size_of::<Self>() + self.nodes.capacity() * std::mem::size_of::<AstNode>()
    }
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
    // Statements
    Program { body: Vec<NodeId> },
    ExpressionStatement { expr: NodeId },
    BlockStatement { body: Vec<NodeId> },
    EmptyStatement,
    DebuggerStatement,
    ReturnStatement { argument: Option<NodeId> },
    IfStatement { test: NodeId, consequent: NodeId, alternate: Option<NodeId> },
    WhileStatement { test: NodeId, body: NodeId },
    DoWhileStatement { test: NodeId, body: NodeId },
    ForStatement { init: Option<NodeId>, test: Option<NodeId>, update: Option<NodeId>, body: NodeId },
    ForInStatement { left: NodeId, right: NodeId, body: NodeId },
    ForOfStatement { left: NodeId, right: NodeId, body: NodeId, is_await: bool },
    BreakStatement,
    ContinueStatement,
    SwitchStatement { discriminant: NodeId, cases: Vec<NodeId> },
    SwitchCase { test: Option<NodeId>, consequent: Vec<NodeId> },
    LabeledStatement { label: NodeId, body: NodeId },
    WithStatement { object: NodeId, body: NodeId },
    
    // Declarations
    VariableDeclaration { kind: VarKind, declarations: Vec<NodeId> },
    VariableDeclarator { id: NodeId, init: Option<NodeId> },
    FunctionDeclaration { id: Option<NodeId>, params: Vec<NodeId>, body: NodeId, is_async: bool, is_generator: bool },
    
    // Expressions
    Identifier { name: Box<str> },
    Literal { value: LiteralValue },
    RegExpLiteral { pattern: Box<str>, flags: Box<str> },
    ArrayExpression { elements: Vec<Option<NodeId>> },
    ObjectExpression { properties: Vec<NodeId> },
    Property { key: NodeId, value: NodeId, computed: bool, shorthand: bool, kind: PropertyKind },
    FunctionExpression { id: Option<NodeId>, params: Vec<NodeId>, body: NodeId, is_async: bool, is_generator: bool },
    ArrowFunctionExpression { params: Vec<NodeId>, body: NodeId, is_async: bool },
    UnaryExpression { operator: UnaryOp, argument: NodeId, prefix: bool },
    UpdateExpression { operator: UpdateOp, argument: NodeId, prefix: bool },
    BinaryExpression { operator: BinaryOp, left: NodeId, right: NodeId },
    LogicalExpression { operator: LogicalOp, left: NodeId, right: NodeId },
    AssignmentExpression { operator: AssignOp, left: NodeId, right: NodeId },
    ConditionalExpression { test: NodeId, consequent: NodeId, alternate: NodeId },
    CallExpression { callee: NodeId, arguments: Vec<NodeId> },
    NewExpression { callee: NodeId, arguments: Vec<NodeId> },
    MemberExpression { object: NodeId, property: NodeId, computed: bool, optional: bool },
    SequenceExpression { expressions: Vec<NodeId> },
    ThisExpression,
    YieldExpression { argument: Option<NodeId>, delegate: bool },
    SpreadElement { argument: NodeId },
    AwaitExpression { argument: NodeId },
    TemplateLiteral { quasis: Vec<NodeId>, expressions: Vec<NodeId> },
    TaggedTemplateExpression { tag: NodeId, quasi: NodeId },
    
    // Destructuring
    ArrayPattern { elements: Vec<Option<NodeId>> },
    ObjectPattern { properties: Vec<NodeId> },
    AssignmentPattern { left: NodeId, right: NodeId },
    RestElement { argument: NodeId },
    
    // Error handling
    TryStatement { block: NodeId, handler: Option<NodeId>, finalizer: Option<NodeId> },
    CatchClause { param: Option<NodeId>, body: NodeId },
    ThrowStatement { argument: NodeId },
    
    // Classes
    ClassDeclaration { id: Option<NodeId>, superclass: Option<NodeId>, body: NodeId },
    ClassExpression { id: Option<NodeId>, superclass: Option<NodeId>, body: NodeId },
    ClassBody { body: Vec<NodeId> },
    MethodDefinition { key: NodeId, value: NodeId, kind: MethodKind, is_static: bool, computed: bool },
    PropertyDefinition { key: NodeId, value: Option<NodeId>, is_static: bool, computed: bool },
    SuperExpression,
    
    // Modules
    ImportDeclaration { specifiers: Vec<NodeId>, source: NodeId },
    ImportSpecifier { imported: NodeId, local: NodeId },
    ImportDefaultSpecifier { local: NodeId },
    ImportNamespaceSpecifier { local: NodeId },
    ExportNamedDeclaration { declaration: Option<NodeId>, specifiers: Vec<NodeId>, source: Option<NodeId> },
    ExportDefaultDeclaration { declaration: NodeId },
    ExportAllDeclaration { source: NodeId, exported: Option<NodeId> },
    ExportSpecifier { local: NodeId, exported: NodeId },
    
    // Meta
    MetaProperty { meta: NodeId, property: NodeId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarKind { Var, Let, Const }

#[derive(Debug, Clone)]
pub enum LiteralValue { Null, Bool(bool), Number(f64), String(Box<str>), BigInt(Box<str>) }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp { Minus, Plus, Not, BitwiseNot, Typeof, Void, Delete }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateOp { Increment, Decrement }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add, Sub, Mul, Div, Mod, Pow, LessThan, LessThanEq, GreaterThan, GreaterThanEq,
    Equal, NotEqual, StrictEqual, StrictNotEqual, LeftShift, RightShift, UnsignedRightShift,
    BitwiseAnd, BitwiseOr, BitwiseXor, In, Instanceof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicalOp { And, Or, NullishCoalescing }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp { Assign, AddAssign, SubAssign, MulAssign, DivAssign, ModAssign,
    PowAssign, LeftShiftAssign, RightShiftAssign, UnsignedRightShiftAssign,
    BitwiseAndAssign, BitwiseOrAssign, BitwiseXorAssign, AndAssign, OrAssign, NullishAssign }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodKind { Constructor, Method, Get, Set }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyKind { Init, Get, Set }


