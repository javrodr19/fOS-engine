//! Bytecode Compiler
//!
//! Compiles AST to bytecode.

use super::ast::{Ast, AstNode, AstNodeKind, NodeId, LiteralValue, BinaryOp, VarKind};
use super::bytecode::{Bytecode, Opcode, Constant};
use std::collections::HashMap;

/// Compiler
pub struct Compiler {
    bytecode: Bytecode,
    locals: Vec<Local>,
    scope_depth: u32,
}

struct Local {
    name: Box<str>,
    depth: u32,
    slot: u16,
}

impl Default for Compiler {
    fn default() -> Self { Self::new() }
}

impl Compiler {
    pub fn new() -> Self {
        Self { bytecode: Bytecode::new(), locals: Vec::new(), scope_depth: 0 }
    }
    
    pub fn compile(mut self, ast: &Ast) -> Result<Bytecode, String> {
        if let Some(root) = ast.root() {
            self.compile_node(ast, root)?;
        }
        self.bytecode.emit(Opcode::Halt);
        Ok(self.bytecode)
    }
    
    /// Compile a statement, optionally not popping expression values (for REPL)
    fn compile_statement(&mut self, ast: &Ast, id: NodeId, is_last: bool) -> Result<(), String> {
        let node = ast.get(id).ok_or("Invalid node")?;
        match &node.kind {
            AstNodeKind::ExpressionStatement { expr } => {
                self.compile_node(ast, *expr)?;
                // For the last expression in a program, don't pop - return it as result
                if !is_last {
                    self.bytecode.emit(Opcode::Pop);
                }
            }
            _ => {
                // For other statements, compile normally
                self.compile_node(ast, id)?;
            }
        }
        Ok(())
    }
    
    fn compile_node(&mut self, ast: &Ast, id: NodeId) -> Result<(), String> {
        let node = ast.get(id).ok_or("Invalid node")?;
        match &node.kind {
            AstNodeKind::Program { body } => {
                // For each statement except the last, compile normally
                // For the last expression statement, don't pop its value
                let len = body.len();
                for (i, stmt) in body.iter().enumerate() {
                    let is_last = i == len - 1;
                    self.compile_statement(ast, *stmt, is_last)?;
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
                self.compile_node(ast, *test)?;
                let exit_jump = self.emit_jump(Opcode::JumpIfFalse);
                self.bytecode.emit(Opcode::Pop);
                self.compile_node(ast, *body)?;
                self.emit_loop(loop_start);
                self.patch_jump(exit_jump);
                self.bytecode.emit(Opcode::Pop);
            }
            _ => {}
        }
        Ok(())
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
        self.locals.push(Local { name, depth: self.scope_depth, slot });
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
