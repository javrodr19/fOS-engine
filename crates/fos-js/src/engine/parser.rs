//! JavaScript Parser (stub)
//!
//! Parses tokens into AST.

use super::lexer::Lexer;
use super::ast::{Ast, AstNode, AstNodeKind, NodeId, LiteralValue, VarKind, BinaryOp};
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
                message: format!("Expected {:?}", kind),
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
            TokenKind::If => self.parse_if_statement(),
            TokenKind::While => self.parse_while_statement(),
            TokenKind::Return => self.parse_return_statement(),
            TokenKind::LBrace => self.parse_block_statement(),
            _ => self.parse_expression_statement(),
        }
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
            let id = self.parse_identifier()?;
            let init = if self.check(&TokenKind::Eq) {
                self.advance();
                Some(self.parse_expression()?)
            } else {
                None
            };
            
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
    
    fn parse_if_statement(&mut self) -> Result<NodeId, ParseError> {
        let start = self.current.span;
        self.advance(); // if
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
        self.parse_additive()
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
        let mut left = self.parse_primary()?;
        
        while matches!(self.current.kind, TokenKind::Star | TokenKind::Slash | TokenKind::Percent) {
            let op = match &self.current.kind {
                TokenKind::Star => BinaryOp::Mul,
                TokenKind::Slash => BinaryOp::Div,
                _ => BinaryOp::Mod,
            };
            self.advance();
            let right = self.parse_primary()?;
            let span = self.ast.get(left).unwrap().span.merge(self.ast.get(right).unwrap().span);
            left = self.ast.add_node(AstNode::new(
                AstNodeKind::BinaryExpression { operator: op, left, right },
                span,
            ));
        }
        Ok(left)
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
            TokenKind::Identifier(_) => self.parse_identifier(),
            TokenKind::LParen => {
                self.advance();
                let expr = self.parse_expression()?;
                self.consume(TokenKind::RParen)?;
                Ok(expr)
            }
            _ => Err(ParseError {
                message: format!("Unexpected token: {:?}", self.current.kind),
                span,
            }),
        }
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
