//! Bytecode Compiler
//!
//! Compiles AST to bytecode.

use super::ast::{Ast, AstNode, AstNodeKind, NodeId, LiteralValue, BinaryOp, VarKind, UnaryOp, LogicalOp, AssignOp};
use super::bytecode::{Bytecode, Opcode, Constant, CompiledFunction, UpvalueInfo};

/// Compiler
pub struct Compiler {
    bytecode: Bytecode,
    locals: Vec<Local>,
    upvalues: Vec<CompilerUpvalue>,
    scope_depth: u32,
    loop_start: Option<usize>,
    loop_exit: Vec<usize>,
}

struct Local {
    name: Box<str>,
    depth: u32,
    slot: u16,
    is_captured: bool,
}

/// Upvalue during compilation
#[derive(Clone)]
struct CompilerUpvalue {
    index: u16,
    is_local: bool,
}

impl Default for Compiler {
    fn default() -> Self { Self::new() }
}

impl Compiler {
    pub fn new() -> Self {
        Self {
            bytecode: Bytecode::new(),
            locals: Vec::new(),
            upvalues: Vec::new(),
            scope_depth: 0,
            loop_start: None,
            loop_exit: Vec::new(),
        }
    }
    
    pub fn compile(mut self, ast: &Ast) -> Result<Bytecode, String> {
        if let Some(root) = ast.root() {
            self.compile_node(ast, root)?;
        }
        self.bytecode.emit(Opcode::Halt);
        Ok(self.bytecode)
    }
    
    fn compile_statement(&mut self, ast: &Ast, id: NodeId, is_last: bool) -> Result<(), String> {
        let node = ast.get(id).ok_or("Invalid node")?;
        match &node.kind {
            AstNodeKind::ExpressionStatement { expr } => {
                self.compile_node(ast, *expr)?;
                if !is_last { self.bytecode.emit(Opcode::Pop); }
            }
            _ => { self.compile_node(ast, id)?; }
        }
        Ok(())
    }
    
    fn compile_node(&mut self, ast: &Ast, id: NodeId) -> Result<(), String> {
        let node = ast.get(id).ok_or("Invalid node")?;
        match &node.kind {
            AstNodeKind::Program { body } => {
                let len = body.len();
                for (i, stmt) in body.iter().enumerate() {
                    self.compile_statement(ast, *stmt, i == len - 1)?;
                }
            }
            AstNodeKind::ExpressionStatement { expr } => {
                self.compile_node(ast, *expr)?;
                self.bytecode.emit(Opcode::Pop);
            }
            AstNodeKind::BlockStatement { body } => {
                self.scope_depth += 1;
                for stmt in body { self.compile_node(ast, *stmt)?; }
                self.end_scope();
            }
            AstNodeKind::VariableDeclaration { declarations, .. } => {
                for decl in declarations { self.compile_node(ast, *decl)?; }
            }
            AstNodeKind::VariableDeclarator { id: name_id, init } => {
                if let Some(init) = init { self.compile_node(ast, *init)?; }
                else { self.bytecode.emit(Opcode::LoadUndefined); }
                
                if let Some(name_node) = ast.get(*name_id) {
                    if let AstNodeKind::Identifier { name } = &name_node.kind {
                        self.define_local(name.clone());
                    }
                }
            }
            AstNodeKind::Literal { value } => {
                match value {
                    LiteralValue::Null => self.bytecode.emit(Opcode::LoadNull),
                    LiteralValue::Bool(true) => self.bytecode.emit(Opcode::LoadTrue),
                    LiteralValue::Bool(false) => self.bytecode.emit(Opcode::LoadFalse),
                    LiteralValue::Number(n) => {
                        if *n == 0.0 { self.bytecode.emit(Opcode::LoadZero); }
                        else if *n == 1.0 { self.bytecode.emit(Opcode::LoadOne); }
                        else {
                            let idx = self.bytecode.add_constant(Constant::Number(*n));
                            self.bytecode.emit(Opcode::LoadConst);
                            self.bytecode.emit_u16(idx);
                        }
                    }
                    LiteralValue::String(s) => {
                        let idx = self.bytecode.add_constant(Constant::String(s.clone()));
                        self.bytecode.emit(Opcode::LoadConst);
                        self.bytecode.emit_u16(idx);
                    }
                }
            }
            AstNodeKind::Identifier { name } => {
                if let Some(slot) = self.resolve_local(name) {
                    self.bytecode.emit(Opcode::GetLocal);
                    self.bytecode.emit_u16(slot);
                } else {
                    let idx = self.bytecode.add_name(name);
                    self.bytecode.emit(Opcode::GetGlobal);
                    self.bytecode.emit_u16(idx);
                }
            }
            AstNodeKind::ThisExpression => {
                self.bytecode.emit(Opcode::LoadUndefined); // TODO: Proper this binding
            }
            AstNodeKind::BinaryExpression { operator, left, right } => {
                self.compile_node(ast, *left)?;
                self.compile_node(ast, *right)?;
                match operator {
                    BinaryOp::Add => self.bytecode.emit(Opcode::Add),
                    BinaryOp::Sub => self.bytecode.emit(Opcode::Sub),
                    BinaryOp::Mul => self.bytecode.emit(Opcode::Mul),
                    BinaryOp::Div => self.bytecode.emit(Opcode::Div),
                    BinaryOp::Mod => self.bytecode.emit(Opcode::Mod),
                    BinaryOp::LessThan => self.bytecode.emit(Opcode::Lt),
                    BinaryOp::LessThanEq => self.bytecode.emit(Opcode::Le),
                    BinaryOp::GreaterThan => self.bytecode.emit(Opcode::Gt),
                    BinaryOp::GreaterThanEq => self.bytecode.emit(Opcode::Ge),
                    BinaryOp::Equal => self.bytecode.emit(Opcode::Eq),
                    BinaryOp::NotEqual => self.bytecode.emit(Opcode::Ne),
                    BinaryOp::StrictEqual => self.bytecode.emit(Opcode::StrictEq),
                    BinaryOp::StrictNotEqual => self.bytecode.emit(Opcode::StrictNe),
                    _ => {}
                }
            }
            AstNodeKind::UnaryExpression { operator, argument } => {
                self.compile_node(ast, *argument)?;
                match operator {
                    UnaryOp::Minus => self.bytecode.emit(Opcode::Neg),
                    UnaryOp::Not => self.bytecode.emit(Opcode::Not),
                    UnaryOp::BitwiseNot => self.bytecode.emit(Opcode::BitNot),
                    UnaryOp::Typeof => self.bytecode.emit(Opcode::Typeof),
                    _ => {}
                }
            }
            AstNodeKind::LogicalExpression { operator, left, right } => {
                self.compile_node(ast, *left)?;
                let jump = match operator {
                    LogicalOp::And => self.emit_jump(Opcode::JumpIfFalse),
                    LogicalOp::Or => self.emit_jump(Opcode::JumpIfTrue),
                    LogicalOp::NullishCoalescing => self.emit_jump(Opcode::JumpIfTrue), // Simplified
                };
                self.bytecode.emit(Opcode::Pop);
                self.compile_node(ast, *right)?;
                self.patch_jump(jump);
            }
            AstNodeKind::AssignmentExpression { left, right, .. } => {
                self.compile_node(ast, *right)?;
                self.bytecode.emit(Opcode::Dup);
                if let Some(left_node) = ast.get(*left) {
                    if let AstNodeKind::Identifier { name } = &left_node.kind {
                        if let Some(slot) = self.resolve_local(name) {
                            self.bytecode.emit(Opcode::SetLocal);
                            self.bytecode.emit_u16(slot);
                        } else {
                            let idx = self.bytecode.add_name(name);
                            self.bytecode.emit(Opcode::SetGlobal);
                            self.bytecode.emit_u16(idx);
                        }
                    }
                }
            }
            AstNodeKind::CallExpression { callee, arguments } => {
                self.compile_node(ast, *callee)?;
                for arg in arguments { self.compile_node(ast, *arg)?; }
                self.bytecode.emit(Opcode::Call);
                self.bytecode.emit_u8(arguments.len() as u8);
            }
            AstNodeKind::MemberExpression { object, property, computed } => {
                self.compile_node(ast, *object)?;
                if *computed {
                    self.compile_node(ast, *property)?;
                    self.bytecode.emit(Opcode::GetIndex);
                } else if let Some(prop_node) = ast.get(*property) {
                    if let AstNodeKind::Identifier { name } = &prop_node.kind {
                        let idx = self.bytecode.add_name(name);
                        self.bytecode.emit(Opcode::GetProperty);
                        self.bytecode.emit_u16(idx);
                    }
                }
            }
            AstNodeKind::ArrayExpression { elements } => {
                for elem in elements.iter().rev() {
                    if let Some(e) = elem { self.compile_node(ast, *e)?; }
                    else { self.bytecode.emit(Opcode::LoadUndefined); }
                }
                self.bytecode.emit(Opcode::NewArray);
                self.bytecode.emit_u16(elements.len() as u16);
            }
            AstNodeKind::ObjectExpression { properties } => {
                self.bytecode.emit(Opcode::NewObject);
                for prop in properties {
                    if let Some(prop_node) = ast.get(*prop) {
                        if let AstNodeKind::Property { key, value, .. } = &prop_node.kind {
                            self.bytecode.emit(Opcode::Dup);
                            if let Some(key_node) = ast.get(*key) {
                                if let AstNodeKind::Identifier { name } = &key_node.kind {
                                    let idx = self.bytecode.add_name(name);
                                    self.compile_node(ast, *value)?;
                                    self.bytecode.emit(Opcode::SetProperty);
                                    self.bytecode.emit_u16(idx);
                                }
                            }
                            self.bytecode.emit(Opcode::Pop);
                        }
                    }
                }
            }
            AstNodeKind::ReturnStatement { argument } => {
                if let Some(arg) = argument { self.compile_node(ast, *arg)?; }
                else { self.bytecode.emit(Opcode::LoadUndefined); }
                self.bytecode.emit(Opcode::Return);
            }
            AstNodeKind::IfStatement { test, consequent, alternate } => {
                self.compile_node(ast, *test)?;
                let jump_false = self.emit_jump(Opcode::JumpIfFalse);
                self.bytecode.emit(Opcode::Pop);
                self.compile_node(ast, *consequent)?;
                
                if let Some(alt) = alternate {
                    let jump_end = self.emit_jump(Opcode::Jump);
                    self.patch_jump(jump_false);
                    self.bytecode.emit(Opcode::Pop);
                    self.compile_node(ast, *alt)?;
                    self.patch_jump(jump_end);
                } else {
                    self.patch_jump(jump_false);
                }
            }
            AstNodeKind::WhileStatement { test, body } => {
                let loop_start = self.bytecode.len();
                let old_start = self.loop_start.replace(loop_start);
                
                self.compile_node(ast, *test)?;
                let exit_jump = self.emit_jump(Opcode::JumpIfFalse);
                self.bytecode.emit(Opcode::Pop);
                self.compile_node(ast, *body)?;
                self.emit_loop(loop_start);
                self.patch_jump(exit_jump);
                self.bytecode.emit(Opcode::Pop);
                
                // Patch break statements
                for exit in std::mem::take(&mut self.loop_exit) {
                    self.patch_jump(exit);
                }
                self.loop_start = old_start;
            }
            AstNodeKind::ForStatement { init, test, update, body } => {
                self.scope_depth += 1;
                if let Some(init) = init { self.compile_node(ast, *init)?; }
                
                let loop_start = self.bytecode.len();
                let old_start = self.loop_start.replace(loop_start);
                
                let exit_jump = if let Some(test) = test {
                    self.compile_node(ast, *test)?;
                    let j = self.emit_jump(Opcode::JumpIfFalse);
                    self.bytecode.emit(Opcode::Pop);
                    Some(j)
                } else { None };
                
                self.compile_node(ast, *body)?;
                if let Some(update) = update {
                    self.compile_node(ast, *update)?;
                    self.bytecode.emit(Opcode::Pop);
                }
                
                self.emit_loop(loop_start);
                if let Some(j) = exit_jump { self.patch_jump(j); self.bytecode.emit(Opcode::Pop); }
                
                for exit in std::mem::take(&mut self.loop_exit) {
                    self.patch_jump(exit);
                }
                self.loop_start = old_start;
                self.end_scope();
            }
            AstNodeKind::BreakStatement => {
                let jump = self.emit_jump(Opcode::Jump);
                self.loop_exit.push(jump);
            }
            AstNodeKind::ContinueStatement => {
                if let Some(start) = self.loop_start {
                    self.emit_loop(start);
                }
            }
            AstNodeKind::FunctionDeclaration { id, params, body, .. } |
            AstNodeKind::FunctionExpression { id, params, body, .. } => {
                // Get function name if available
                let name = id.and_then(|n| {
                    ast.get(n).and_then(|node| {
                        if let AstNodeKind::Identifier { name } = &node.kind {
                            Some(name.clone())
                        } else { None }
                    })
                });
                
                // Compile function body with new compiler
                let func = self.compile_function(ast, name, params, *body)?;
                
                // Add compiled function to constants
                let const_idx = self.bytecode.add_constant(Constant::Function(Box::new(func)));
                
                // Emit LoadConst to push the function onto the stack
                self.bytecode.emit(Opcode::LoadConst);
                self.bytecode.emit_u16(const_idx);
                
                // If this is a function declaration (not expression), bind it to a name
                if let AstNodeKind::FunctionDeclaration { id: Some(name_id), .. } = &node.kind {
                    if let Some(name_node) = ast.get(*name_id) {
                        if let AstNodeKind::Identifier { name } = &name_node.kind {
                            // Bind to local or global
                            if self.scope_depth > 0 {
                                self.define_local(name.clone());
                            } else {
                                let name_idx = self.bytecode.add_name(name);
                                self.bytecode.emit(Opcode::SetGlobal);
                                self.bytecode.emit_u16(name_idx);
                                self.bytecode.emit(Opcode::Pop);
                            }
                        }
                    }
                }
            }
            AstNodeKind::ArrowFunctionExpression { params, body, .. } => {
                // Compile arrow function body
                let func = self.compile_function(ast, None, params, *body)?;
                let const_idx = self.bytecode.add_constant(Constant::Function(Box::new(func)));
                self.bytecode.emit(Opcode::LoadConst);
                self.bytecode.emit_u16(const_idx);
            }
            
            // Try/Catch/Throw
            AstNodeKind::TryStatement { block, handler, finalizer } => {
                // Emit TryStart - if error, jumps to catch handler
                let catch_jump = self.emit_jump(Opcode::TryStart);
                
                // Compile try block
                self.compile_node(ast, *block)?;
                
                // Emit TryEnd to pop handler
                self.bytecode.emit(Opcode::TryEnd);
                
                // Jump over catch block
                let finally_jump = self.emit_jump(Opcode::Jump);
                
                // Patch catch jump
                self.patch_jump(catch_jump);
                
                // Compile catch handler if present
                if let Some(handler_id) = handler {
                    self.compile_node(ast, *handler_id)?;
                }
                
                // Patch finally jump
                self.patch_jump(finally_jump);
                
                // Compile finalizer if present
                if let Some(finalizer_id) = finalizer {
                    self.compile_node(ast, *finalizer_id)?;
                }
            }
            AstNodeKind::CatchClause { param, body } => {
                // If param present, create binding for error
                if let Some(param_id) = param {
                    if let Some(param_node) = ast.get(*param_id) {
                        if let AstNodeKind::Identifier { name } = &param_node.kind {
                            self.define_local(name.clone());
                        }
                    }
                }
                self.compile_node(ast, *body)?;
            }
            AstNodeKind::ThrowStatement { argument } => {
                self.compile_node(ast, *argument)?;
                self.bytecode.emit(Opcode::Throw);
            }
            
            // Classes
            AstNodeKind::ClassDeclaration { id, superclass, body } |
            AstNodeKind::ClassExpression { id, superclass, body } => {
                // Compile superclass if present
                if let Some(super_id) = superclass {
                    self.compile_node(ast, *super_id)?;
                } else {
                    self.bytecode.emit(Opcode::LoadNull);
                }
                
                // Create class object
                self.bytecode.emit(Opcode::NewObject);
                
                // Compile class body (methods)
                self.compile_node(ast, *body)?;
                
                // If class declaration, bind to name
                if let AstNodeKind::ClassDeclaration { id: Some(name_id), .. } = &node.kind {
                    if let Some(name_node) = ast.get(*name_id) {
                        if let AstNodeKind::Identifier { name } = &name_node.kind {
                            if self.scope_depth > 0 {
                                self.define_local(name.clone());
                            } else {
                                let name_idx = self.bytecode.add_name(name);
                                self.bytecode.emit(Opcode::SetGlobal);
                                self.bytecode.emit_u16(name_idx);
                            }
                        }
                    }
                }
            }
            AstNodeKind::ClassBody { body } => {
                for member in body {
                    self.compile_node(ast, *member)?;
                }
            }
            AstNodeKind::MethodDefinition { key, value, kind, is_static, .. } => {
                // Get method name
                if let Some(key_node) = ast.get(*key) {
                    if let AstNodeKind::Identifier { name } = &key_node.kind {
                        // Compile method function
                        self.compile_node(ast, *value)?;
                        
                        // Set property on class/prototype
                        let name_idx = self.bytecode.add_name(name);
                        self.bytecode.emit(Opcode::SetProperty);
                        self.bytecode.emit_u16(name_idx);
                    }
                }
            }
            AstNodeKind::SuperExpression => {
                // TODO: Implement proper super reference
                self.bytecode.emit(Opcode::LoadUndefined);
            }
            
            _ => {}
        }
        Ok(())
    }
    
    /// Compile a function body into a CompiledFunction
    fn compile_function(&mut self, ast: &Ast, name: Option<Box<str>>, params: &[NodeId], body: NodeId) -> Result<CompiledFunction, String> {
        use super::bytecode::CompiledFunction;
        
        // Create a new compiler for the function body
        let mut func_compiler = Compiler::new();
        func_compiler.scope_depth = 1; // Function starts at scope depth 1
        
        // Add parameters as locals
        for param in params {
            if let Some(param_node) = ast.get(*param) {
                if let AstNodeKind::Identifier { name } = &param_node.kind {
                    func_compiler.define_local(name.clone());
                }
            }
        }
        
        // Compile function body
        func_compiler.compile_node(ast, body)?;
        
        // Ensure function returns something
        func_compiler.bytecode.emit(Opcode::LoadUndefined);
        func_compiler.bytecode.emit(Opcode::Return);
        
        Ok(CompiledFunction {
            name,
            arity: params.len() as u8,
            locals_count: func_compiler.locals.len() as u16,
            upvalue_count: 0, // TODO: Track upvalues
            upvalues: Vec::new(),
            bytecode: func_compiler.bytecode,
        })
    }
    
    fn emit_jump(&mut self, op: Opcode) -> usize {
        self.bytecode.emit(op);
        self.bytecode.emit_u16(0);
        self.bytecode.len() - 2
    }
    
    fn patch_jump(&mut self, offset: usize) {
        let jump = (self.bytecode.len() - offset - 2) as i16;
        self.bytecode.code[offset] = (jump >> 8) as u8;
        self.bytecode.code[offset + 1] = jump as u8;
    }
    
    fn emit_loop(&mut self, loop_start: usize) {
        self.bytecode.emit(Opcode::Jump);
        let offset = -((self.bytecode.len() - loop_start + 2) as i16);
        self.bytecode.emit_i16(offset);
    }
    
    fn define_local(&mut self, name: Box<str>) {
        let slot = self.locals.len() as u16;
        self.locals.push(Local { name, depth: self.scope_depth, slot, is_captured: false });
    }
    
    fn resolve_local(&self, name: &str) -> Option<u16> {
        self.locals.iter().rev().find(|l| &*l.name == name).map(|l| l.slot)
    }
    
    fn end_scope(&mut self) {
        while self.locals.last().map(|l| l.depth == self.scope_depth).unwrap_or(false) {
            self.locals.pop();
            self.bytecode.emit(Opcode::Pop);
        }
        self.scope_depth -= 1;
    }
}
