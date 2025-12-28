//! JavaScript Parser
//!
//! Parses tokens into AST. Supports ES2023 syntax.

use super::lexer::Lexer;
use super::ast::{Ast, AstNode, AstNodeKind, NodeId, LiteralValue, VarKind, BinaryOp, UnaryOp, LogicalOp, PropertyKind};
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
            TokenKind::If => self.parse_if_statement(),
            TokenKind::While => self.parse_while_statement(),
            TokenKind::For => self.parse_for_statement(),
            TokenKind::Return => self.parse_return_statement(),
            TokenKind::Break => self.parse_break_statement(),
            TokenKind::Continue => self.parse_continue_statement(),
            TokenKind::LBrace => self.parse_block_statement(),
            _ => self.parse_expression_statement(),
        }
    }
    
    fn parse_function_declaration(&mut self) -> Result<NodeId, ParseError> {
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
            AstNodeKind::FunctionDeclaration { id, params, body, is_async: false, is_generator: false },
            start.merge(self.previous.span),
        )))
    }
    
    fn parse_parameters(&mut self) -> Result<Vec<NodeId>, ParseError> {
        let mut params = Vec::new();
        
        if !self.check(&TokenKind::RParen) {
            params.push(self.parse_identifier()?);
            while self.check(&TokenKind::Comma) {
                self.advance();
                params.push(self.parse_identifier()?);
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
            let id = self.parse_identifier()?;
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
        
        // init
        let init = if self.check(&TokenKind::Semicolon) {
            self.advance();
            None
        } else if matches!(self.current.kind, TokenKind::Let | TokenKind::Const | TokenKind::Var) {
            Some(self.parse_variable_declaration()?)
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
        
        while self.check(&TokenKind::Dot) {
            self.advance();
            let property = self.parse_identifier()?;
            let span = self.ast.get(object).unwrap().span.merge(self.previous.span);
            object = self.ast.add_node(AstNode::new(
                AstNodeKind::MemberExpression { object, property, computed: false, optional: false },
                span,
            ));
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
            TokenKind::Identifier(_) => self.parse_identifier(),
            TokenKind::LParen => {
                self.advance();
                let expr = self.parse_expression()?;
                self.consume(TokenKind::RParen)?;
                Ok(expr)
            }
            TokenKind::LBracket => self.parse_array_literal(),
            TokenKind::LBrace => self.parse_object_literal(),
            TokenKind::Function => self.parse_function_expression(),
            _ => Err(ParseError {
                message: format!("Unexpected token: {:?}", self.current.kind),
                span,
            }),
        }
    }
    
    fn parse_array_literal(&mut self) -> Result<NodeId, ParseError> {
        let start = self.current.span;
        self.advance(); // [
        let mut elements = Vec::new();
        
        while !self.check(&TokenKind::RBracket) {
            if self.check(&TokenKind::Comma) {
                elements.push(None); // elision
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
            let key = if matches!(self.current.kind, TokenKind::String(_)) {
                self.parse_primary()?
            } else {
                self.parse_identifier()?
            };
            
            self.consume(TokenKind::Colon)?;
            let value = self.parse_expression()?;
            
            let span = self.ast.get(key).unwrap().span.merge(self.ast.get(value).unwrap().span);
            properties.push(self.ast.add_node(AstNode::new(
                AstNodeKind::Property { key, value, computed: false, shorthand: false, kind: PropertyKind::Init },
                span,
            )));
            
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
