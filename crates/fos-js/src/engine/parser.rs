//! JavaScript Parser
//!
//! Parses tokens into AST. Supports ES2023 syntax.

use super::lexer::Lexer;
use super::ast::{Ast, AstNode, AstNodeKind, NodeId, LiteralValue, VarKind, BinaryOp, UnaryOp, LogicalOp, PropertyKind, MethodKind};
use super::token::{Token, TokenKind, Span};

/// Parser error
#[derive(Debug)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

/// JavaScript Parser
pub struct Parser<'src> {
    lexer: Lexer<'src>,
    current: Token,
    previous: Token,
    ast: Ast,
}

impl<'src> Parser<'src> {
    pub fn new(source: &'src str) -> Self {
        let mut lexer = Lexer::new(source);
        let current = lexer.next_token();
        Self {
            lexer,
            current: current.clone(),
            previous: current,
            ast: Ast::new(),
        }
    }
    
    fn advance(&mut self) {
        self.previous = std::mem::replace(&mut self.current, self.lexer.next_token());
    }
    
    fn check(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(&self.current.kind) == std::mem::discriminant(kind)
    }
    
    fn consume(&mut self, kind: TokenKind) -> Result<(), ParseError> {
        if self.check(&kind) {
            self.advance();
            Ok(())
        } else {
            Err(ParseError {
                message: format!("Expected {:?}, got {:?}", kind, self.current.kind),
                span: self.current.span,
            })
        }
    }
    
    /// Parse a complete program
    pub fn parse(mut self) -> Result<Ast, ParseError> {
        let mut body = Vec::new();
        
        while !matches!(self.current.kind, TokenKind::Eof) {
            body.push(self.parse_statement()?);
        }
        
        let span = if body.is_empty() {
            Span::new(0, 0)
        } else {
            let first = self.ast.get(body[0]).unwrap().span;
            let last = self.ast.get(*body.last().unwrap()).unwrap().span;
            first.merge(last)
        };
        
        let root = self.ast.add_node(AstNode::new(
            AstNodeKind::Program { body },
            span,
        ));
        self.ast.set_root(root);
        
        Ok(self.ast)
    }
    
    fn parse_statement(&mut self) -> Result<NodeId, ParseError> {
        match &self.current.kind {
            TokenKind::Let | TokenKind::Const | TokenKind::Var => self.parse_variable_declaration(),
            TokenKind::Function => self.parse_function_declaration(),
            TokenKind::Async => self.parse_async_function(),
            TokenKind::Class => self.parse_class_declaration(),
            TokenKind::If => self.parse_if_statement(),
            TokenKind::While => self.parse_while_statement(),
            TokenKind::Do => self.parse_do_while_statement(),
            TokenKind::For => self.parse_for_statement(),
            TokenKind::Return => self.parse_return_statement(),
            TokenKind::Break => self.parse_break_statement(),
            TokenKind::Continue => self.parse_continue_statement(),
            TokenKind::Switch => self.parse_switch_statement(),
            TokenKind::Try => self.parse_try_statement(),
            TokenKind::Throw => self.parse_throw_statement(),
            TokenKind::LBrace => self.parse_block_statement(),
            TokenKind::Import => self.parse_import_declaration(),
            TokenKind::Export => self.parse_export_declaration(),
            _ => self.parse_expression_statement(),
        }
    }
    
    fn parse_do_while_statement(&mut self) -> Result<NodeId, ParseError> {
        let start = self.current.span;
        self.advance(); // do
        let body = self.parse_statement()?;
        self.consume(TokenKind::While)?;
        self.consume(TokenKind::LParen)?;
        let test = self.parse_expression()?;
        self.consume(TokenKind::RParen)?;
        self.consume(TokenKind::Semicolon)?;
        
        Ok(self.ast.add_node(AstNode::new(
            AstNodeKind::DoWhileStatement { test, body },
            start.merge(self.previous.span),
        )))
    }
    
    fn parse_switch_statement(&mut self) -> Result<NodeId, ParseError> {
        let start = self.current.span;
        self.advance(); // switch
        self.consume(TokenKind::LParen)?;
        let discriminant = self.parse_expression()?;
        self.consume(TokenKind::RParen)?;
        self.consume(TokenKind::LBrace)?;
        
        let mut cases = Vec::new();
        while !self.check(&TokenKind::RBrace) {
            cases.push(self.parse_switch_case()?);
        }
        self.consume(TokenKind::RBrace)?;
        
        Ok(self.ast.add_node(AstNode::new(
            AstNodeKind::SwitchStatement { discriminant, cases },
            start.merge(self.previous.span),
        )))
    }
    
    fn parse_switch_case(&mut self) -> Result<NodeId, ParseError> {
        let start = self.current.span;
        
        let test = if self.check(&TokenKind::Case) {
            self.advance();
            Some(self.parse_expression()?)
        } else if self.check(&TokenKind::Default) {
            self.advance();
            None
        } else {
            return Err(ParseError { message: "Expected case or default".into(), span: self.current.span });
        };
        
        self.consume(TokenKind::Colon)?;
        
        let mut consequent = Vec::new();
        while !self.check(&TokenKind::Case) && !self.check(&TokenKind::Default) && !self.check(&TokenKind::RBrace) {
            consequent.push(self.parse_statement()?);
        }
        
        Ok(self.ast.add_node(AstNode::new(
            AstNodeKind::SwitchCase { test, consequent },
            start.merge(self.previous.span),
        )))
    }
    
    fn parse_try_statement(&mut self) -> Result<NodeId, ParseError> {
        let start = self.current.span;
        self.advance(); // try
        let block = self.parse_block_statement()?;
        
        // Catch clause
        let handler = if self.check(&TokenKind::Catch) {
            self.advance();
            let param = if self.check(&TokenKind::LParen) {
                self.advance();
                let p = self.parse_identifier()?;
                self.consume(TokenKind::RParen)?;
                Some(p)
            } else { None };
            let body = self.parse_block_statement()?;
            Some(self.ast.add_node(AstNode::new(
                AstNodeKind::CatchClause { param, body },
                start.merge(self.previous.span),
            )))
        } else { None };
        
        // Finally clause
        let finalizer = if self.check(&TokenKind::Finally) {
            self.advance();
            Some(self.parse_block_statement()?)
        } else { None };
        
        Ok(self.ast.add_node(AstNode::new(
            AstNodeKind::TryStatement { block, handler, finalizer },
            start.merge(self.previous.span),
        )))
    }
    
    fn parse_throw_statement(&mut self) -> Result<NodeId, ParseError> {
        let start = self.current.span;
        self.advance(); // throw
        let argument = self.parse_expression()?;
        self.consume(TokenKind::Semicolon)?;
        Ok(self.ast.add_node(AstNode::new(
            AstNodeKind::ThrowStatement { argument },
            start.merge(self.previous.span),
        )))
    }
    
    fn parse_class_declaration(&mut self) -> Result<NodeId, ParseError> {
        let start = self.current.span;
        self.advance(); // class
        
        // Class name (optional for expressions)
        let id = if matches!(self.current.kind, TokenKind::Identifier(_)) {
            Some(self.parse_identifier()?)
        } else { None };
        
        // extends clause
        let superclass = if self.check(&TokenKind::Extends) {
            self.advance();
            Some(self.parse_expression()?)
        } else { None };
        
        // Class body
        self.consume(TokenKind::LBrace)?;
        let mut body = Vec::new();
        
        while !self.check(&TokenKind::RBrace) {
            body.push(self.parse_class_member()?);
        }
        
        self.consume(TokenKind::RBrace)?;
        
        let class_body = self.ast.add_node(AstNode::new(
            AstNodeKind::ClassBody { body: body.clone() },
            start.merge(self.previous.span),
        ));
        
        Ok(self.ast.add_node(AstNode::new(
            AstNodeKind::ClassDeclaration { id, superclass, body: class_body },
            start.merge(self.previous.span),
        )))
    }
    
    fn parse_class_member(&mut self) -> Result<NodeId, ParseError> {
        let start = self.current.span;
        
        // Check for static
        let is_static = if self.check_identifier("static") {
            self.advance();
            true
        } else { false };
        
        // Check for getter/setter
        let kind = if self.check_identifier("get") {
            self.advance();
            MethodKind::Get
        } else if self.check_identifier("set") {
            self.advance();
            MethodKind::Set
        } else if self.check_identifier("constructor") {
            MethodKind::Constructor
        } else {
            MethodKind::Method
        };
        
        // Method name
        let key = self.parse_identifier()?;
        
        // Method params and body
        self.consume(TokenKind::LParen)?;
        let params = self.parse_parameters()?;
        self.consume(TokenKind::RParen)?;
        let body = self.parse_block_statement()?;
        
        // Create method function
        let func = self.ast.add_node(AstNode::new(
            AstNodeKind::FunctionExpression {
                id: None, params, body,
                is_async: false, is_generator: false,
            },
            start.merge(self.previous.span),
        ));
        
        Ok(self.ast.add_node(AstNode::new(
            AstNodeKind::MethodDefinition { key, value: func, kind, is_static, computed: false },
            start.merge(self.previous.span),
        )))
    }
    
    fn parse_async_function(&mut self) -> Result<NodeId, ParseError> {
        let start = self.current.span;
        self.advance(); // async
        self.consume(TokenKind::Function)?;
        
        // Check for generator *
        let is_generator = if self.check(&TokenKind::Star) {
            self.advance();
            true
        } else { false };
        
        let id = if matches!(self.current.kind, TokenKind::Identifier(_)) {
            Some(self.parse_identifier()?)
        } else { None };
        
        self.consume(TokenKind::LParen)?;
        let params = self.parse_parameters()?;
        self.consume(TokenKind::RParen)?;
        
        let body = self.parse_block_statement()?;
        
        Ok(self.ast.add_node(AstNode::new(
            AstNodeKind::FunctionDeclaration { id, params, body, is_async: true, is_generator },
            start.merge(self.previous.span),
        )))
    }
    
    fn parse_function_declaration(&mut self) -> Result<NodeId, ParseError> {
        let start = self.current.span;
        self.advance(); // function
        
        // Check for generator *
        let is_generator = if self.check(&TokenKind::Star) {
            self.advance();
            true
        } else { false };
        
        let id = if matches!(self.current.kind, TokenKind::Identifier(_)) {
            Some(self.parse_identifier()?)
        } else { None };
        
        self.consume(TokenKind::LParen)?;
        let params = self.parse_parameters()?;
        self.consume(TokenKind::RParen)?;
        
        let body = self.parse_block_statement()?;
        
        Ok(self.ast.add_node(AstNode::new(
            AstNodeKind::FunctionDeclaration { id, params, body, is_async: false, is_generator },
            start.merge(self.previous.span),
        )))
    }
    
    fn parse_parameters(&mut self) -> Result<Vec<NodeId>, ParseError> {
        let mut params = Vec::new();
        
        if !self.check(&TokenKind::RParen) {
            loop {
                // Check for rest parameter
                if self.check(&TokenKind::DotDotDot) {
                    self.advance();
                    let arg = self.parse_identifier()?;
                    let rest = self.ast.add_node(AstNode::new(
                        AstNodeKind::RestElement { argument: arg },
                        self.ast.get(arg).unwrap().span,
                    ));
                    params.push(rest);
                    break; // Rest must be last
                }
                
                // Check for destructuring in parameters
                let param = if self.check(&TokenKind::LBracket) {
                    self.parse_array_pattern()?
                } else if self.check(&TokenKind::LBrace) {
                    self.parse_object_pattern()?
                } else {
                    let id = self.parse_identifier()?;
                    // Check for default value
                    if self.check(&TokenKind::Eq) {
                        self.advance();
                        let default_val = self.parse_expression()?;
                        self.ast.add_node(AstNode::new(
                            AstNodeKind::AssignmentPattern { left: id, right: default_val },
                            self.ast.get(id).unwrap().span.merge(self.ast.get(default_val).unwrap().span),
                        ))
                    } else {
                        id
                    }
                };
                params.push(param);
                
                if !self.check(&TokenKind::Comma) { break; }
                self.advance();
            }
        }
        
        Ok(params)
    }
    
    fn parse_variable_declaration(&mut self) -> Result<NodeId, ParseError> {
        let start = self.current.span;
        let kind = match &self.current.kind {
            TokenKind::Let => VarKind::Let,
            TokenKind::Const => VarKind::Const,
            _ => VarKind::Var,
        };
        self.advance();
        
        let mut declarations = Vec::new();
        loop {
            // Check for destructuring pattern
            let id = if self.check(&TokenKind::LBracket) {
                // Array destructuring
                self.parse_array_pattern()?
            } else if self.check(&TokenKind::LBrace) {
                // Object destructuring
                self.parse_object_pattern()?
            } else {
                self.parse_identifier()?
            };
            
            let init = if self.check(&TokenKind::Eq) {
                self.advance();
                Some(self.parse_expression()?)
            } else { None };
            
            let span = self.ast.get(id).unwrap().span;
            let decl = self.ast.add_node(AstNode::new(
                AstNodeKind::VariableDeclarator { id, init },
                span,
            ));
            declarations.push(decl);
            
            if !self.check(&TokenKind::Comma) { break; }
            self.advance();
        }
        
        self.consume(TokenKind::Semicolon)?;
        
        Ok(self.ast.add_node(AstNode::new(
            AstNodeKind::VariableDeclaration { kind, declarations },
            start.merge(self.previous.span),
        )))
    }
    
    fn parse_array_pattern(&mut self) -> Result<NodeId, ParseError> {
        let start = self.current.span;
        self.consume(TokenKind::LBracket)?;
        let mut elements = Vec::new();
        
        while !self.check(&TokenKind::RBracket) {
            if self.check(&TokenKind::Comma) {
                // Elision
                elements.push(None);
            } else if self.check(&TokenKind::DotDotDot) {
                // Rest element
                self.advance();
                let arg = self.parse_identifier()?;
                let rest = self.ast.add_node(AstNode::new(
                    AstNodeKind::RestElement { argument: arg },
                    self.ast.get(arg).unwrap().span,
                ));
                elements.push(Some(rest));
                break; // Rest must be last
            } else {
                // Regular element or nested pattern
                let elem = if self.check(&TokenKind::LBracket) {
                    self.parse_array_pattern()?
                } else if self.check(&TokenKind::LBrace) {
                    self.parse_object_pattern()?
                } else {
                    self.parse_identifier()?
                };
                
                // Check for default value
                if self.check(&TokenKind::Eq) {
                    self.advance();
                    let right = self.parse_expression()?;
                    let pattern = self.ast.add_node(AstNode::new(
                        AstNodeKind::AssignmentPattern { left: elem, right },
                        self.ast.get(elem).unwrap().span.merge(self.ast.get(right).unwrap().span),
                    ));
                    elements.push(Some(pattern));
                } else {
                    elements.push(Some(elem));
                }
            }
            
            if !self.check(&TokenKind::RBracket) {
                self.consume(TokenKind::Comma)?;
            }
        }
        
        self.consume(TokenKind::RBracket)?;
        Ok(self.ast.add_node(AstNode::new(
            AstNodeKind::ArrayPattern { elements },
            start.merge(self.previous.span),
        )))
    }
    
    fn parse_object_pattern(&mut self) -> Result<NodeId, ParseError> {
        let start = self.current.span;
        self.consume(TokenKind::LBrace)?;
        let mut properties = Vec::new();
        
        while !self.check(&TokenKind::RBrace) {
            if self.check(&TokenKind::DotDotDot) {
                // Rest element
                self.advance();
                let arg = self.parse_identifier()?;
                let rest = self.ast.add_node(AstNode::new(
                    AstNodeKind::RestElement { argument: arg },
                    self.ast.get(arg).unwrap().span,
                ));
                properties.push(rest);
                break;
            }
            
            // Property pattern
            let key = self.parse_identifier()?;
            let value = if self.check(&TokenKind::Colon) {
                self.advance();
                if self.check(&TokenKind::LBracket) {
                    self.parse_array_pattern()?
                } else if self.check(&TokenKind::LBrace) {
                    self.parse_object_pattern()?
                } else {
                    self.parse_identifier()?
                }
            } else {
                key // Shorthand
            };
            
            // Check for default
            let final_value = if self.check(&TokenKind::Eq) {
                self.advance();
                let right = self.parse_expression()?;
                self.ast.add_node(AstNode::new(
                    AstNodeKind::AssignmentPattern { left: value, right },
                    self.ast.get(value).unwrap().span.merge(self.ast.get(right).unwrap().span),
                ))
            } else {
                value
            };
            
            let prop = self.ast.add_node(AstNode::new(
                AstNodeKind::Property {
                    key, value: final_value,
                    computed: false, shorthand: key == value, kind: PropertyKind::Init,
                },
                self.ast.get(key).unwrap().span,
            ));
            properties.push(prop);
            
            if !self.check(&TokenKind::RBrace) {
                self.consume(TokenKind::Comma)?;
            }
        }
        
        self.consume(TokenKind::RBrace)?;
        Ok(self.ast.add_node(AstNode::new(
            AstNodeKind::ObjectPattern { properties },
            start.merge(self.previous.span),
        )))
    }
    
    fn parse_if_statement(&mut self) -> Result<NodeId, ParseError> {
        let start = self.current.span;
        self.advance();
        self.consume(TokenKind::LParen)?;
        let test = self.parse_expression()?;
        self.consume(TokenKind::RParen)?;
        let consequent = self.parse_statement()?;
        let alternate = if self.check(&TokenKind::Else) {
            self.advance();
            Some(self.parse_statement()?)
        } else { None };
        
        Ok(self.ast.add_node(AstNode::new(
            AstNodeKind::IfStatement { test, consequent, alternate },
            start.merge(self.previous.span),
        )))
    }
    
    fn parse_while_statement(&mut self) -> Result<NodeId, ParseError> {
        let start = self.current.span;
        self.advance();
        self.consume(TokenKind::LParen)?;
        let test = self.parse_expression()?;
        self.consume(TokenKind::RParen)?;
        let body = self.parse_statement()?;
        
        Ok(self.ast.add_node(AstNode::new(
            AstNodeKind::WhileStatement { test, body },
            start.merge(self.previous.span),
        )))
    }
    
    fn parse_for_statement(&mut self) -> Result<NodeId, ParseError> {
        let start = self.current.span;
        self.advance();
        self.consume(TokenKind::LParen)?;
        
        // Check for for-of or for-in
        if matches!(self.current.kind, TokenKind::Let | TokenKind::Const | TokenKind::Var) {
            let var_start = self.current.span;
            let kind = match &self.current.kind {
                TokenKind::Let => VarKind::Let,
                TokenKind::Const => VarKind::Const,
                _ => VarKind::Var,
            };
            self.advance();
            let id = self.parse_identifier()?;
            
            // Check for 'of' or 'in'
            if let TokenKind::Identifier(ref s) = self.current.kind {
                if s.as_ref() == "of" {
                    // for-of loop
                    self.advance();
                    let right = self.parse_expression()?;
                    self.consume(TokenKind::RParen)?;
                    let body = self.parse_statement()?;
                    
                    let declarator = self.ast.add_node(AstNode::new(
                        AstNodeKind::VariableDeclarator { id, init: None },
                        var_start,
                    ));
                    let decl = self.ast.add_node(AstNode::new(
                        AstNodeKind::VariableDeclaration {
                            kind,
                            declarations: vec![declarator],
                        },
                        var_start,
                    ));
                    
                    return Ok(self.ast.add_node(AstNode::new(
                        AstNodeKind::ForOfStatement { left: decl, right, body, is_await: false },
                        start.merge(self.previous.span),
                    )));
                } else if s.as_ref() == "in" {
                    // for-in loop
                    self.advance();
                    let right = self.parse_expression()?;
                    self.consume(TokenKind::RParen)?;
                    let body = self.parse_statement()?;
                    
                    let declarator = self.ast.add_node(AstNode::new(
                        AstNodeKind::VariableDeclarator { id, init: None },
                        var_start,
                    ));
                    let decl = self.ast.add_node(AstNode::new(
                        AstNodeKind::VariableDeclaration {
                            kind,
                            declarations: vec![declarator],
                        },
                        var_start,
                    ));
                    
                    return Ok(self.ast.add_node(AstNode::new(
                        AstNodeKind::ForInStatement { left: decl, right, body },
                        start.merge(self.previous.span),
                    )));
                }
            }
            
            // Regular for loop - parse rest of declaration
            let init_val = if self.check(&TokenKind::Eq) {
                self.advance();
                Some(self.parse_expression()?)
            } else { None };
            
            let decl = self.ast.add_node(AstNode::new(
                AstNodeKind::VariableDeclarator { id, init: init_val },
                var_start,
            ));
            
            let init = self.ast.add_node(AstNode::new(
                AstNodeKind::VariableDeclaration { kind, declarations: vec![decl] },
                var_start,
            ));
            self.consume(TokenKind::Semicolon)?;
            
            // test
            let test = if self.check(&TokenKind::Semicolon) { None }
            else { Some(self.parse_expression()?) };
            self.consume(TokenKind::Semicolon)?;
            
            // update
            let update = if self.check(&TokenKind::RParen) { None }
            else { Some(self.parse_expression()?) };
            self.consume(TokenKind::RParen)?;
            
            let body = self.parse_statement()?;
            
            return Ok(self.ast.add_node(AstNode::new(
                AstNodeKind::ForStatement { init: Some(init), test, update, body },
                start.merge(self.previous.span),
            )));
        }
        
        // init (expression or empty)
        let init = if self.check(&TokenKind::Semicolon) {
            self.advance();
            None
        } else {
            let expr = self.parse_expression()?;
            self.consume(TokenKind::Semicolon)?;
            Some(expr)
        };
        
        // test
        let test = if self.check(&TokenKind::Semicolon) { None }
        else { Some(self.parse_expression()?) };
        self.consume(TokenKind::Semicolon)?;
        
        // update
        let update = if self.check(&TokenKind::RParen) { None }
        else { Some(self.parse_expression()?) };
        self.consume(TokenKind::RParen)?;
        
        let body = self.parse_statement()?;
        
        Ok(self.ast.add_node(AstNode::new(
            AstNodeKind::ForStatement { init, test, update, body },
            start.merge(self.previous.span),
        )))
    }
    
    fn parse_return_statement(&mut self) -> Result<NodeId, ParseError> {
        let start = self.current.span;
        self.advance();
        let argument = if !self.check(&TokenKind::Semicolon) {
            Some(self.parse_expression()?)
        } else { None };
        self.consume(TokenKind::Semicolon)?;
        
        Ok(self.ast.add_node(AstNode::new(
            AstNodeKind::ReturnStatement { argument },
            start.merge(self.previous.span),
        )))
    }
    
    fn parse_break_statement(&mut self) -> Result<NodeId, ParseError> {
        let start = self.current.span;
        self.advance();
        self.consume(TokenKind::Semicolon)?;
        Ok(self.ast.add_node(AstNode::new(AstNodeKind::BreakStatement, start.merge(self.previous.span))))
    }
    
    fn parse_continue_statement(&mut self) -> Result<NodeId, ParseError> {
        let start = self.current.span;
        self.advance();
        self.consume(TokenKind::Semicolon)?;
        Ok(self.ast.add_node(AstNode::new(AstNodeKind::ContinueStatement, start.merge(self.previous.span))))
    }
    
    fn parse_import_declaration(&mut self) -> Result<NodeId, ParseError> {
        let start = self.current.span;
        self.advance(); // import
        
        let mut specifiers = Vec::new();
        
        // import "module" (side-effect)
        if let TokenKind::String(_) = &self.current.kind {
            let source = self.parse_string_literal()?;
            self.consume(TokenKind::Semicolon)?;
            return Ok(self.ast.add_node(AstNode::new(
                AstNodeKind::ImportDeclaration { specifiers, source },
                start.merge(self.previous.span),
            )));
        }
        
        // import * as name from "module"
        if self.check(&TokenKind::Star) {
            self.advance();
            self.expect_identifier("as")?;
            let local = self.parse_identifier()?;
            let spec = self.ast.add_node(AstNode::new(
                AstNodeKind::ImportNamespaceSpecifier { local },
                self.ast.get(local).unwrap().span,
            ));
            specifiers.push(spec);
        }
        // import defaultExport from "module"
        else if matches!(self.current.kind, TokenKind::Identifier(_)) {
            let local = self.parse_identifier()?;
            let spec = self.ast.add_node(AstNode::new(
                AstNodeKind::ImportDefaultSpecifier { local },
                self.ast.get(local).unwrap().span,
            ));
            specifiers.push(spec);
            
            // Check for additional named imports
            if self.check(&TokenKind::Comma) {
                self.advance();
                if self.check(&TokenKind::LBrace) {
                    self.parse_named_imports(&mut specifiers)?;
                }
            }
        }
        // import { named } from "module"
        else if self.check(&TokenKind::LBrace) {
            self.parse_named_imports(&mut specifiers)?;
        }
        
        self.expect_identifier("from")?;
        let source = self.parse_string_literal()?;
        self.consume(TokenKind::Semicolon)?;
        
        Ok(self.ast.add_node(AstNode::new(
            AstNodeKind::ImportDeclaration { specifiers, source },
            start.merge(self.previous.span),
        )))
    }
    
    fn parse_named_imports(&mut self, specifiers: &mut Vec<NodeId>) -> Result<(), ParseError> {
        self.consume(TokenKind::LBrace)?;
        while !self.check(&TokenKind::RBrace) {
            let imported = self.parse_identifier()?;
            let local = if self.check_identifier("as") {
                self.advance();
                self.parse_identifier()?
            } else {
                imported
            };
            let spec = self.ast.add_node(AstNode::new(
                AstNodeKind::ImportSpecifier { imported, local },
                self.ast.get(imported).unwrap().span,
            ));
            specifiers.push(spec);
            if !self.check(&TokenKind::RBrace) {
                self.consume(TokenKind::Comma)?;
            }
        }
        self.consume(TokenKind::RBrace)?;
        Ok(())
    }
    
    fn parse_export_declaration(&mut self) -> Result<NodeId, ParseError> {
        let start = self.current.span;
        self.advance(); // export
        
        // export default
        if self.check(&TokenKind::Default) {
            self.advance();
            let declaration = self.parse_expression()?;
            self.consume(TokenKind::Semicolon)?;
            return Ok(self.ast.add_node(AstNode::new(
                AstNodeKind::ExportDefaultDeclaration { declaration },
                start.merge(self.previous.span),
            )));
        }
        
        // export * from "module"
        if self.check(&TokenKind::Star) {
            self.advance();
            self.expect_identifier("from")?;
            let source = self.parse_string_literal()?;
            self.consume(TokenKind::Semicolon)?;
            return Ok(self.ast.add_node(AstNode::new(
                AstNodeKind::ExportAllDeclaration { source, exported: None },
                start.merge(self.previous.span),
            )));
        }
        
        // export { named }
        if self.check(&TokenKind::LBrace) {
            let mut specifiers = Vec::new();
            self.consume(TokenKind::LBrace)?;
            while !self.check(&TokenKind::RBrace) {
                let local = self.parse_identifier()?;
                let exported = if self.check_identifier("as") {
                    self.advance();
                    self.parse_identifier()?
                } else {
                    local
                };
                let spec = self.ast.add_node(AstNode::new(
                    AstNodeKind::ExportSpecifier { local, exported },
                    self.ast.get(local).unwrap().span,
                ));
                specifiers.push(spec);
                if !self.check(&TokenKind::RBrace) {
                    self.consume(TokenKind::Comma)?;
                }
            }
            self.consume(TokenKind::RBrace)?;
            
            let source = if self.check_identifier("from") {
                self.advance();
                Some(self.parse_string_literal()?)
            } else { None };
            
            self.consume(TokenKind::Semicolon)?;
            return Ok(self.ast.add_node(AstNode::new(
                AstNodeKind::ExportNamedDeclaration { declaration: None, specifiers, source },
                start.merge(self.previous.span),
            )));
        }
        
        // export function/const/let/var
        let declaration = self.parse_statement()?;
        Ok(self.ast.add_node(AstNode::new(
            AstNodeKind::ExportNamedDeclaration { declaration: Some(declaration), specifiers: vec![], source: None },
            start.merge(self.previous.span),
        )))
    }
    
    fn expect_identifier(&mut self, name: &str) -> Result<(), ParseError> {
        if let TokenKind::Identifier(ref s) = self.current.kind {
            if s.as_ref() == name {
                self.advance();
                return Ok(());
            }
        }
        Err(ParseError { message: format!("Expected '{}'", name), span: self.current.span })
    }
    
    fn check_identifier(&self, name: &str) -> bool {
        if let TokenKind::Identifier(ref s) = self.current.kind {
            s.as_ref() == name
        } else { false }
    }
    
    fn parse_string_literal(&mut self) -> Result<NodeId, ParseError> {
        if let TokenKind::String(s) = &self.current.kind {
            let val = s.clone();
            let span = self.current.span;
            self.advance();
            Ok(self.ast.add_node(AstNode::new(
                AstNodeKind::Literal { value: LiteralValue::String(val) },
                span,
            )))
        } else {
            Err(ParseError { message: "Expected string literal".to_string(), span: self.current.span })
        }
    }

    fn parse_block_statement(&mut self) -> Result<NodeId, ParseError> {
        let start = self.current.span;
        self.advance();
        let mut body = Vec::new();
        while !self.check(&TokenKind::RBrace) && !matches!(self.current.kind, TokenKind::Eof) {
            body.push(self.parse_statement()?);
        }
        self.consume(TokenKind::RBrace)?;
        
        Ok(self.ast.add_node(AstNode::new(
            AstNodeKind::BlockStatement { body },
            start.merge(self.previous.span),
        )))
    }
    
    fn parse_expression_statement(&mut self) -> Result<NodeId, ParseError> {
        let start = self.current.span;
        let expr = self.parse_expression()?;
        self.consume(TokenKind::Semicolon)?;
        
        Ok(self.ast.add_node(AstNode::new(
            AstNodeKind::ExpressionStatement { expr },
            start.merge(self.previous.span),
        )))
    }
    
    fn parse_expression(&mut self) -> Result<NodeId, ParseError> {
        self.parse_assignment()
    }
    
    fn parse_assignment(&mut self) -> Result<NodeId, ParseError> {
        let left = self.parse_logical_or()?;
        
        if self.check(&TokenKind::Eq) {
            self.advance();
            let right = self.parse_assignment()?;
            let span = self.ast.get(left).unwrap().span.merge(self.ast.get(right).unwrap().span);
            return Ok(self.ast.add_node(AstNode::new(
                AstNodeKind::AssignmentExpression { operator: super::ast::AssignOp::Assign, left, right },
                span,
            )));
        }
        
        Ok(left)
    }
    
    fn parse_logical_or(&mut self) -> Result<NodeId, ParseError> {
        let mut left = self.parse_logical_and()?;
        
        while self.check(&TokenKind::PipePipe) {
            self.advance();
            let right = self.parse_logical_and()?;
            let span = self.ast.get(left).unwrap().span.merge(self.ast.get(right).unwrap().span);
            left = self.ast.add_node(AstNode::new(
                AstNodeKind::LogicalExpression { operator: LogicalOp::Or, left, right },
                span,
            ));
        }
        Ok(left)
    }
    
    fn parse_logical_and(&mut self) -> Result<NodeId, ParseError> {
        let mut left = self.parse_equality()?;
        
        while self.check(&TokenKind::AmpersandAmpersand) {
            self.advance();
            let right = self.parse_equality()?;
            let span = self.ast.get(left).unwrap().span.merge(self.ast.get(right).unwrap().span);
            left = self.ast.add_node(AstNode::new(
                AstNodeKind::LogicalExpression { operator: LogicalOp::And, left, right },
                span,
            ));
        }
        Ok(left)
    }
    
    fn parse_equality(&mut self) -> Result<NodeId, ParseError> {
        let mut left = self.parse_comparison()?;
        
        while matches!(self.current.kind, TokenKind::EqEq | TokenKind::NotEq | TokenKind::EqEqEq | TokenKind::NotEqEq) {
            let op = match &self.current.kind {
                TokenKind::EqEq => BinaryOp::Equal,
                TokenKind::NotEq => BinaryOp::NotEqual,
                TokenKind::EqEqEq => BinaryOp::StrictEqual,
                _ => BinaryOp::StrictNotEqual,
            };
            self.advance();
            let right = self.parse_comparison()?;
            let span = self.ast.get(left).unwrap().span.merge(self.ast.get(right).unwrap().span);
            left = self.ast.add_node(AstNode::new(
                AstNodeKind::BinaryExpression { operator: op, left, right },
                span,
            ));
        }
        Ok(left)
    }
    
    fn parse_comparison(&mut self) -> Result<NodeId, ParseError> {
        let mut left = self.parse_additive()?;
        
        while matches!(self.current.kind, TokenKind::LessThan | TokenKind::LessThanEq | TokenKind::GreaterThan | TokenKind::GreaterThanEq) {
            let op = match &self.current.kind {
                TokenKind::LessThan => BinaryOp::LessThan,
                TokenKind::LessThanEq => BinaryOp::LessThanEq,
                TokenKind::GreaterThan => BinaryOp::GreaterThan,
                _ => BinaryOp::GreaterThanEq,
            };
            self.advance();
            let right = self.parse_additive()?;
            let span = self.ast.get(left).unwrap().span.merge(self.ast.get(right).unwrap().span);
            left = self.ast.add_node(AstNode::new(
                AstNodeKind::BinaryExpression { operator: op, left, right },
                span,
            ));
        }
        Ok(left)
    }
    
    fn parse_additive(&mut self) -> Result<NodeId, ParseError> {
        let mut left = self.parse_multiplicative()?;
        
        while matches!(self.current.kind, TokenKind::Plus | TokenKind::Minus) {
            let op = match &self.current.kind {
                TokenKind::Plus => BinaryOp::Add,
                _ => BinaryOp::Sub,
            };
            self.advance();
            let right = self.parse_multiplicative()?;
            let span = self.ast.get(left).unwrap().span.merge(self.ast.get(right).unwrap().span);
            left = self.ast.add_node(AstNode::new(
                AstNodeKind::BinaryExpression { operator: op, left, right },
                span,
            ));
        }
        Ok(left)
    }
    
    fn parse_multiplicative(&mut self) -> Result<NodeId, ParseError> {
        let mut left = self.parse_unary()?;
        
        while matches!(self.current.kind, TokenKind::Star | TokenKind::Slash | TokenKind::Percent) {
            let op = match &self.current.kind {
                TokenKind::Star => BinaryOp::Mul,
                TokenKind::Slash => BinaryOp::Div,
                _ => BinaryOp::Mod,
            };
            self.advance();
            let right = self.parse_unary()?;
            let span = self.ast.get(left).unwrap().span.merge(self.ast.get(right).unwrap().span);
            left = self.ast.add_node(AstNode::new(
                AstNodeKind::BinaryExpression { operator: op, left, right },
                span,
            ));
        }
        Ok(left)
    }
    
    fn parse_unary(&mut self) -> Result<NodeId, ParseError> {
        let start = self.current.span;
        
        match &self.current.kind {
            TokenKind::Bang => {
                self.advance();
                let argument = self.parse_unary()?;
                Ok(self.ast.add_node(AstNode::new(
                    AstNodeKind::UnaryExpression { operator: UnaryOp::Not, argument, prefix: true },
                    start.merge(self.ast.get(argument).unwrap().span),
                )))
            }
            TokenKind::Minus => {
                self.advance();
                let argument = self.parse_unary()?;
                Ok(self.ast.add_node(AstNode::new(
                    AstNodeKind::UnaryExpression { operator: UnaryOp::Minus, argument, prefix: true },
                    start.merge(self.ast.get(argument).unwrap().span),
                )))
            }
            TokenKind::Typeof => {
                self.advance();
                let argument = self.parse_unary()?;
                Ok(self.ast.add_node(AstNode::new(
                    AstNodeKind::UnaryExpression { operator: UnaryOp::Typeof, argument, prefix: true },
                    start.merge(self.ast.get(argument).unwrap().span),
                )))
            }
            TokenKind::Await => {
                self.advance();
                let argument = self.parse_unary()?;
                Ok(self.ast.add_node(AstNode::new(
                    AstNodeKind::AwaitExpression { argument },
                    start.merge(self.ast.get(argument).unwrap().span),
                )))
            }
            TokenKind::New => {
                self.advance();
                let callee = self.parse_member()?;
                let arguments = if self.check(&TokenKind::LParen) {
                    self.advance();
                    let args = self.parse_arguments()?;
                    self.consume(TokenKind::RParen)?;
                    args
                } else {
                    vec![]
                };
                Ok(self.ast.add_node(AstNode::new(
                    AstNodeKind::NewExpression { callee, arguments },
                    start.merge(self.previous.span),
                )))
            }
            _ => self.parse_call(),
        }
    }
    
    fn parse_call(&mut self) -> Result<NodeId, ParseError> {
        let mut expr = self.parse_member()?;
        
        loop {
            if self.check(&TokenKind::LParen) {
                self.advance();
                let arguments = self.parse_arguments()?;
                self.consume(TokenKind::RParen)?;
                let span = self.ast.get(expr).unwrap().span.merge(self.previous.span);
                expr = self.ast.add_node(AstNode::new(
                    AstNodeKind::CallExpression { callee: expr, arguments },
                    span,
                ));
            } else if self.check(&TokenKind::LBracket) {
                self.advance();
                let property = self.parse_expression()?;
                self.consume(TokenKind::RBracket)?;
                let span = self.ast.get(expr).unwrap().span.merge(self.previous.span);
                expr = self.ast.add_node(AstNode::new(
                    AstNodeKind::MemberExpression { object: expr, property, computed: true, optional: false },
                    span,
                ));
            } else {
                break;
            }
        }
        
        Ok(expr)
    }
    
    fn parse_member(&mut self) -> Result<NodeId, ParseError> {
        let mut object = self.parse_primary()?;
        
        loop {
            if self.check(&TokenKind::Dot) {
                self.advance();
                let property = self.parse_identifier()?;
                let span = self.ast.get(object).unwrap().span.merge(self.previous.span);
                object = self.ast.add_node(AstNode::new(
                    AstNodeKind::MemberExpression { object, property, computed: false, optional: false },
                    span,
                ));
            } else if self.check(&TokenKind::QuestionDot) {
                // Optional chaining ?.
                self.advance();
                let property = self.parse_identifier()?;
                let span = self.ast.get(object).unwrap().span.merge(self.previous.span);
                object = self.ast.add_node(AstNode::new(
                    AstNodeKind::MemberExpression { object, property, computed: false, optional: true },
                    span,
                ));
            } else if self.check(&TokenKind::LBracket) {
                // Computed access [expr]
                self.advance();
                let property = self.parse_expression()?;
                self.consume(TokenKind::RBracket)?;
                let span = self.ast.get(object).unwrap().span.merge(self.previous.span);
                object = self.ast.add_node(AstNode::new(
                    AstNodeKind::MemberExpression { object, property, computed: true, optional: false },
                    span,
                ));
            } else {
                break;
            }
        }
        
        Ok(object)
    }
    
    fn parse_arguments(&mut self) -> Result<Vec<NodeId>, ParseError> {
        let mut args = Vec::new();
        
        if !self.check(&TokenKind::RParen) {
            args.push(self.parse_expression()?);
            while self.check(&TokenKind::Comma) {
                self.advance();
                args.push(self.parse_expression()?);
            }
        }
        
        Ok(args)
    }
    
    fn parse_primary(&mut self) -> Result<NodeId, ParseError> {
        let span = self.current.span;
        match &self.current.kind {
            TokenKind::Number(n) => {
                let val = *n;
                self.advance();
                Ok(self.ast.add_node(AstNode::new(
                    AstNodeKind::Literal { value: LiteralValue::Number(val) },
                    span,
                )))
            }
            TokenKind::String(s) => {
                let val = s.clone();
                self.advance();
                Ok(self.ast.add_node(AstNode::new(
                    AstNodeKind::Literal { value: LiteralValue::String(val) },
                    span,
                )))
            }
            TokenKind::Boolean(b) => {
                let val = *b;
                self.advance();
                Ok(self.ast.add_node(AstNode::new(
                    AstNodeKind::Literal { value: LiteralValue::Bool(val) },
                    span,
                )))
            }
            TokenKind::Null => {
                self.advance();
                Ok(self.ast.add_node(AstNode::new(
                    AstNodeKind::Literal { value: LiteralValue::Null },
                    span,
                )))
            }
            TokenKind::This => {
                self.advance();
                Ok(self.ast.add_node(AstNode::new(AstNodeKind::ThisExpression, span)))
            }
            TokenKind::Yield => {
                // Yield expression (inside generator)
                self.advance();
                let delegate = if self.check(&TokenKind::Star) {
                    self.advance();
                    true
                } else { false };
                let argument = if !self.check(&TokenKind::Semicolon) && 
                                  !self.check(&TokenKind::RBrace) &&
                                  !self.check(&TokenKind::RParen) {
                    Some(self.parse_expression()?)
                } else { None };
                Ok(self.ast.add_node(AstNode::new(
                    AstNodeKind::YieldExpression { argument, delegate },
                    span.merge(self.previous.span),
                )))
            }
            TokenKind::Identifier(_) => {
                // Check for arrow function: ident => body
                let id = self.parse_identifier()?;
                if self.check(&TokenKind::Arrow) {
                    self.advance();
                    let body = if self.check(&TokenKind::LBrace) {
                        self.parse_block_statement()?
                    } else {
                        self.parse_expression()?
                    };
                    Ok(self.ast.add_node(AstNode::new(
                        AstNodeKind::ArrowFunctionExpression { params: vec![id], body, is_async: false },
                        span.merge(self.previous.span),
                    )))
                } else {
                    Ok(id)
                }
            }
            TokenKind::LParen => {
                // Could be grouped expression or arrow function
                self.advance();
                
                // Empty parens () => is arrow function
                if self.check(&TokenKind::RParen) {
                    self.advance();
                    if self.check(&TokenKind::Arrow) {
                        self.advance();
                        let body = if self.check(&TokenKind::LBrace) {
                            self.parse_block_statement()?
                        } else {
                            self.parse_expression()?
                        };
                        return Ok(self.ast.add_node(AstNode::new(
                            AstNodeKind::ArrowFunctionExpression { params: vec![], body, is_async: false },
                            span.merge(self.previous.span),
                        )));
                    }
                    // Empty parens without arrow - error or return undefined
                    return Err(ParseError { message: "Unexpected ()".into(), span });
                }
                
                // Parse first expression
                let first = self.parse_expression()?;
                
                // Check if it continues to be params
                if self.check(&TokenKind::Comma) || self.check(&TokenKind::RParen) {
                    // Might be arrow function params
                    let mut params = vec![first];
                    while self.check(&TokenKind::Comma) {
                        self.advance();
                        params.push(self.parse_expression()?);
                    }
                    self.consume(TokenKind::RParen)?;
                    
                    if self.check(&TokenKind::Arrow) {
                        self.advance();
                        let body = if self.check(&TokenKind::LBrace) {
                            self.parse_block_statement()?
                        } else {
                            self.parse_expression()?
                        };
                        return Ok(self.ast.add_node(AstNode::new(
                            AstNodeKind::ArrowFunctionExpression { params, body, is_async: false },
                            span.merge(self.previous.span),
                        )));
                    }
                    
                    // Just a grouped expression (or tuple-like for first element)
                    if params.len() == 1 {
                        return Ok(params.into_iter().next().unwrap());
                    }
                    // Multiple expressions in parens without arrow - use first
                    return Ok(first);
                }
                
                self.consume(TokenKind::RParen)?;
                Ok(first)
            }
            TokenKind::LBracket => self.parse_array_literal(),
            TokenKind::LBrace => self.parse_object_literal(),
            TokenKind::Function => self.parse_function_expression(),
            TokenKind::Class => self.parse_class_expression(),
            TokenKind::NoSubstitutionTemplate(s) => {
                // Simple template string without substitutions
                let text = s.clone();
                self.advance();
                let quasi = self.ast.add_node(AstNode::new(
                    AstNodeKind::Literal { value: LiteralValue::String(text) },
                    span,
                ));
                Ok(self.ast.add_node(AstNode::new(
                    AstNodeKind::TemplateLiteral { quasis: vec![quasi], expressions: vec![] },
                    span.merge(self.previous.span),
                )))
            }
            TokenKind::TemplateHead(s) => {
                // Template with substitutions: `text${expr}...`
                let start_span = span;
                let mut quasis = Vec::new();
                let mut expressions = Vec::new();
                
                // First quasi
                let first_text = s.clone();
                self.advance();
                quasis.push(self.ast.add_node(AstNode::new(
                    AstNodeKind::Literal { value: LiteralValue::String(first_text) },
                    start_span,
                )));
                
                // Expression
                expressions.push(self.parse_expression()?);
                
                // Continue with middle/tail
                loop {
                    match &self.current.kind {
                        TokenKind::TemplateMiddle(s) => {
                            let text = s.clone();
                            self.advance();
                            quasis.push(self.ast.add_node(AstNode::new(
                                AstNodeKind::Literal { value: LiteralValue::String(text) },
                                self.previous.span,
                            )));
                            expressions.push(self.parse_expression()?);
                        }
                        TokenKind::TemplateTail(s) => {
                            let text = s.clone();
                            self.advance();
                            quasis.push(self.ast.add_node(AstNode::new(
                                AstNodeKind::Literal { value: LiteralValue::String(text) },
                                self.previous.span,
                            )));
                            break;
                        }
                        _ => break,
                    }
                }
                
                Ok(self.ast.add_node(AstNode::new(
                    AstNodeKind::TemplateLiteral { quasis, expressions },
                    start_span.merge(self.previous.span),
                )))
            }
            _ => Err(ParseError {
                message: format!("Unexpected token: {:?}", self.current.kind),
                span,
            }),
        }
    }
    
    fn parse_class_expression(&mut self) -> Result<NodeId, ParseError> {
        let start = self.current.span;
        self.advance(); // class
        
        let id = if matches!(self.current.kind, TokenKind::Identifier(_)) {
            Some(self.parse_identifier()?)
        } else { None };
        
        let superclass = if self.check(&TokenKind::Extends) {
            self.advance();
            Some(self.parse_expression()?)
        } else { None };
        
        self.consume(TokenKind::LBrace)?;
        let mut body = Vec::new();
        while !self.check(&TokenKind::RBrace) {
            body.push(self.parse_class_member()?);
        }
        self.consume(TokenKind::RBrace)?;
        
        let class_body = self.ast.add_node(AstNode::new(
            AstNodeKind::ClassBody { body: body.clone() },
            start.merge(self.previous.span),
        ));
        
        Ok(self.ast.add_node(AstNode::new(
            AstNodeKind::ClassExpression { id, superclass, body: class_body },
            start.merge(self.previous.span),
        )))
    }
    
    fn parse_array_literal(&mut self) -> Result<NodeId, ParseError> {
        let start = self.current.span;
        self.advance(); // [
        let mut elements = Vec::new();
        
        while !self.check(&TokenKind::RBracket) {
            if self.check(&TokenKind::Comma) {
                elements.push(None); // elision
            } else if self.check(&TokenKind::DotDotDot) {
                // Spread element
                let spread_start = self.current.span;
                self.advance();
                let argument = self.parse_expression()?;
                let spread = self.ast.add_node(AstNode::new(
                    AstNodeKind::SpreadElement { argument },
                    spread_start.merge(self.ast.get(argument).unwrap().span),
                ));
                elements.push(Some(spread));
            } else {
                elements.push(Some(self.parse_expression()?));
            }
            if !self.check(&TokenKind::RBracket) {
                self.consume(TokenKind::Comma)?;
            }
        }
        self.consume(TokenKind::RBracket)?;
        
        Ok(self.ast.add_node(AstNode::new(
            AstNodeKind::ArrayExpression { elements },
            start.merge(self.previous.span),
        )))
    }
    
    fn parse_object_literal(&mut self) -> Result<NodeId, ParseError> {
        let start = self.current.span;
        self.advance(); // {
        let mut properties = Vec::new();
        
        while !self.check(&TokenKind::RBrace) {
            // Spread property
            if self.check(&TokenKind::DotDotDot) {
                let spread_start = self.current.span;
                self.advance();
                let argument = self.parse_expression()?;
                properties.push(self.ast.add_node(AstNode::new(
                    AstNodeKind::SpreadElement { argument },
                    spread_start.merge(self.ast.get(argument).unwrap().span),
                )));
            } else {
                let key = if matches!(self.current.kind, TokenKind::String(_)) {
                    self.parse_primary()?
                } else {
                    self.parse_identifier()?
                };
                
                // Shorthand property { x } instead of { x: x }
                let (value, shorthand) = if self.check(&TokenKind::Colon) {
                    self.advance();
                    (self.parse_expression()?, false)
                } else {
                    // Shorthand - key is also value
                    (key, true)
                };
                
                let span = self.ast.get(key).unwrap().span.merge(self.ast.get(value).unwrap().span);
                properties.push(self.ast.add_node(AstNode::new(
                    AstNodeKind::Property { key, value, computed: false, shorthand, kind: PropertyKind::Init },
                    span,
                )));
            }
            
            if !self.check(&TokenKind::RBrace) {
                self.consume(TokenKind::Comma)?;
            }
        }
        self.consume(TokenKind::RBrace)?;
        
        Ok(self.ast.add_node(AstNode::new(
            AstNodeKind::ObjectExpression { properties },
            start.merge(self.previous.span),
        )))
    }
    
    fn parse_function_expression(&mut self) -> Result<NodeId, ParseError> {
        let start = self.current.span;
        self.advance(); // function
        
        let id = if matches!(self.current.kind, TokenKind::Identifier(_)) {
            Some(self.parse_identifier()?)
        } else { None };
        
        self.consume(TokenKind::LParen)?;
        let params = self.parse_parameters()?;
        self.consume(TokenKind::RParen)?;
        
        let body = self.parse_block_statement()?;
        
        Ok(self.ast.add_node(AstNode::new(
            AstNodeKind::FunctionExpression { id, params, body, is_async: false, is_generator: false },
            start.merge(self.previous.span),
        )))
    }
    
    fn parse_identifier(&mut self) -> Result<NodeId, ParseError> {
        let span = self.current.span;
        if let TokenKind::Identifier(name) = &self.current.kind {
            let name = name.clone();
            self.advance();
            Ok(self.ast.add_node(AstNode::new(
                AstNodeKind::Identifier { name },
                span,
            )))
        } else {
            Err(ParseError { message: "Expected identifier".into(), span })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_function_declaration() {
        let parser = Parser::new("function add(a, b) { return a + b; }");
        let ast = parser.parse().unwrap();
        assert!(ast.root().is_some());
    }
    
    #[test]
    fn test_array_literal() {
        let parser = Parser::new("let arr = [1, 2, 3];");
        let ast = parser.parse().unwrap();
        assert!(ast.root().is_some());
    }
    
    #[test]
    fn test_object_literal() {
        let parser = Parser::new("let obj = { a: 1, b: 2 };");
        let ast = parser.parse().unwrap();
        assert!(ast.root().is_some());
    }
    
    #[test]
    fn test_call_expression() {
        let parser = Parser::new("console.log(1, 2);");
        let ast = parser.parse().unwrap();
        assert!(ast.root().is_some());
    }
    
    #[test]
    fn test_for_loop() {
        let parser = Parser::new("for (let i = 0; i < 10; i = i + 1) { x = x + i; }");
        let ast = parser.parse().unwrap();
        assert!(ast.root().is_some());
    }
}
