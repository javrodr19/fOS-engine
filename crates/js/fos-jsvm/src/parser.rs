//! Parser
//!
//! Recursive descent for statements, precedence climbing for binary
//! operators. Arrow parameters and destructuring assignment use the
//! specification's "cover grammar" approach: they are parsed as expressions
//! and converted to patterns once the following `=>` or `=` shows what they
//! were.

use std::rc::Rc;

use crate::ast::*;
use crate::compiler::pattern_names;
use crate::lexer::{Kw, Lexer, Span, SyntaxError, Tok, Token, Utf16, P};

type PResult<T> = Result<T, SyntaxError>;

/// Parse a script
pub fn parse_script(src: &str) -> PResult<Program> {
    let mut parser = Parser::new(src)?;
    let (body, strict) = parser.parse_body_until_eof()?;
    Ok(Program { body, strict })
}

/// Parse a module
pub fn parse_module(src: &str) -> PResult<Module> {
    parse_module_with(src, false)
}

/// Parse a module keeping only lazy stubs for its functions' bodies
pub fn parse_module_lazy(src: &str) -> PResult<Module> {
    parse_module_with(src, true)
}

fn parse_module_with(src: &str, lazy: bool) -> PResult<Module> {
    let mut p = Parser::new(src)?;
    p.set_lazy(lazy);
    // Module code is strict, and `await` works at its top level
    p.strict = true;
    p.in_async = true;
    p.module = Some(ModuleDecls::default());
    let mut body = Vec::new();
    while p.tok.tok != Tok::Eof {
        p.parse_module_item(&mut body)?;
    }
    let decls = p.module.take().unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    for e in &decls.exports {
        if let Some(name) = e.export_name() {
            if !seen.insert(name.clone()) {
                return Err(SyntaxError { message: format!("Duplicate export of '{name}'"), pos: 0 });
            }
        }
    }
    Ok(Module { body, requests: decls.requests, imports: decls.imports, exports: decls.exports, has_await: p.top_level_await })
}

/// The imports and exports of the module being parsed
#[derive(Default)]
struct ModuleDecls {
    requests: Vec<ModuleRequest>,
    imports: Vec<ImportEntry>,
    exports: Vec<ExportEntry>,
}

/// What re-parsing a lazily compiled function needs to know
pub struct ReparseInfo {
    pub params_start: u32,
    pub span_start: u32,
    pub kind: FunctionKind,
    pub is_async: bool,
    pub is_generator: bool,
    /// Strictness of the enclosing code
    pub outer_strict: bool,
    /// Defined in a module (`import.meta` is valid, `await` is reserved)
    pub in_module: bool,
}

/// Parse a function again from its source (for lazy compilation); its
/// nested functions come back as lazy stubs
pub fn reparse_function(src: &str, info: &ReparseInfo) -> PResult<Function> {
    let mut p = Parser::new("")?;
    p.lexer = Lexer::new(src);
    p.lexer.seek(info.params_start as usize);
    p.tok = p.lexer.next_token()?;
    p.strict = info.outer_strict;
    p.in_function = true;
    p.lazy_depth = Some(1);
    if info.in_module {
        p.module = Some(ModuleDecls::default());
    }
    if info.kind == FunctionKind::Arrow {
        match p.parse_assign()? {
            Expr::Function(f) => Ok(*f),
            _ => Err(SyntaxError { message: "invalid lazy function".into(), pos: info.params_start }),
        }
    } else {
        p.parse_function_rest(None, info.kind, info.is_async, info.is_generator, info.span_start)
    }
}

/// Parse a script keeping only lazy stubs for its functions' bodies
/// (compiled when first called)
pub fn parse_script_lazy(src: &str) -> PResult<Program> {
    let mut parser = Parser::new(src)?;
    parser.set_lazy(true);
    let (body, strict) = parser.parse_body_until_eof()?;
    Ok(Program { body, strict })
}

/// Parse the parameter list and body of a `Function(...)` constructor call
pub fn parse_function_parts(params: &str, body: &str) -> PResult<Function> {
    let src = format!("(function anonymous({}\n) {{\n{}\n}})", params, body);
    let program = parse_script(&src)?;
    match program.body.into_iter().next() {
        Some(Stmt::Expr(Expr::Paren(inner))) => match *inner {
            Expr::Function(f) => Ok(*f),
            _ => Err(SyntaxError { message: "invalid function source".into(), pos: 0 }),
        },
        _ => Err(SyntaxError { message: "invalid function source".into(), pos: 0 }),
    }
}

fn utf16(s: &str) -> Utf16 {
    s.encode_utf16().collect::<Vec<_>>().into()
}

pub struct Parser<'a> {
    lexer: Lexer<'a>,
    tok: Token,
    prev_end: u32,
    strict: bool,
    in_function: bool,
    in_generator: bool,
    in_async: bool,
    /// Disallow `in` as a binary operator (in `for (init; ...)` heads)
    no_in: bool,
    /// `{ a = 1 }` shorthands not yet turned into patterns
    cover_inits: usize,
    /// Function nesting depth
    depth: u32,
    /// Functions finishing at this depth or deeper keep only what lazy
    /// compilation needs (None: keep every body)
    lazy_depth: Option<u32>,
    /// Start of a parenthesized function expression, which is probably
    /// invoked immediately and so compiled eagerly (as V8 does)
    eager_function: Option<u32>,
    /// Interned identifier names (one allocation per distinct name)
    names: std::cell::RefCell<std::collections::HashSet<Name>>,
    /// Parsing a module: its imports and exports so far
    module: Option<ModuleDecls>,
    /// `await` was used outside any function of the module
    top_level_await: bool,
}


impl<'a> Parser<'a> {
    pub fn new(src: &'a str) -> PResult<Self> {
        let mut lexer = Lexer::new(src);
        let tok = lexer.next_token()?;
        Ok(Self {
            lexer,
            tok,
            prev_end: 0,
            strict: false,
            in_function: false,
            in_generator: false,
            in_async: false,
            no_in: false,
            cover_inits: 0,
            depth: 0,
            lazy_depth: None,
            eager_function: None,
            names: Default::default(),
            module: None,
            top_level_await: false,
        })
    }

    fn name(&self, s: &str) -> Name {
        let mut names = self.names.borrow_mut();
        if let Some(n) = names.get(s) {
            return n.clone();
        }
        let n: Name = Rc::from(s);
        names.insert(n.clone());
        n
    }

    /// Keep only lazy stubs for nested functions (see `FunctionBody::Lazy`)
    pub fn set_lazy(&mut self, on: bool) {
        self.lazy_depth = if on { Some(0) } else { None };
    }

    /// Whether a function starting here (at `start`) is pre-parsed: its
    /// statements are checked and dropped, keeping only a lazy stub
    fn preparse_function(&self, start: u32) -> bool {
        self.lazy_depth.is_some_and(|d| self.depth >= d) && self.eager_function != Some(start)
    }

    /// Parse a function body's statements up to (not including) `}`,
    /// either keeping them or (pre-parsing) folding them into free names
    fn parse_body_items(&mut self, pre: Option<&mut FreeNames>) -> PResult<Vec<Stmt>> {
        let mut body = Vec::new();
        self.parse_directives(&mut body)?;
        match pre {
            None => {
                while !self.at(P::RBrace) {
                    if self.tok.tok == Tok::Eof {
                        return self.unexpected("expected '}'");
                    }
                    body.push(self.parse_statement_list_item()?);
                }
            }
            Some(free) => {
                for st in body.drain(..) {
                    free.stmt(&st);
                }
                while !self.at(P::RBrace) {
                    if self.tok.tok == Tok::Eof {
                        return self.unexpected("expected '}'");
                    }
                    let st = self.parse_statement_list_item()?;
                    free.stmt(&st);
                }
            }
        }
        Ok(body)
    }

    // ---- token helpers ----

    fn advance(&mut self) -> PResult<Token> {
        let next = self.lexer.next_token()?;
        self.prev_end = self.tok.span.end;
        Ok(std::mem::replace(&mut self.tok, next))
    }

    fn peek(&self) -> PResult<Token> {
        let mut lexer = Lexer::new(self.lexer.source());
        // Re-scan from the end of the current token
        lexer_seek(&mut lexer, self.tok.span.end as usize);
        lexer.next_token()
    }

    /// Cheap pre-check for one-token lookahead: whether the source after
    /// the current token starts (past spaces and tabs) with `text`. Avoids a
    /// full rescan on every identifier.
    fn next_bytes_are(&self, text: &str) -> bool {
        let src = self.lexer.source().as_bytes();
        let mut i = self.tok.span.end as usize;
        while i < src.len() && (src[i] == b' ' || src[i] == b'\t') {
            i += 1;
        }
        src[i..].starts_with(text.as_bytes())
    }

    fn at(&self, p: P) -> bool {
        self.tok.tok == Tok::Punct(p)
    }

    fn at_kw(&self, kw: Kw) -> bool {
        self.tok.tok == Tok::Keyword(kw)
    }

    fn at_ident(&self, name: &str) -> bool {
        matches!(&self.tok.tok, Tok::Ident(n) if &**n == name) && !self.tok.escaped
    }

    fn eat(&mut self, p: P) -> PResult<bool> {
        if self.at(p) {
            self.advance()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn eat_kw(&mut self, kw: Kw) -> PResult<bool> {
        if self.at_kw(kw) {
            self.advance()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn expect(&mut self, p: P) -> PResult<()> {
        if self.eat(p)? {
            Ok(())
        } else {
            self.unexpected(&format!("expected '{}'", p.as_str()))
        }
    }

    fn expect_kw(&mut self, kw: Kw) -> PResult<()> {
        if self.eat_kw(kw)? {
            Ok(())
        } else {
            self.unexpected(&format!("expected '{}'", kw.as_str()))
        }
    }

    fn error<T>(&self, message: impl Into<String>) -> PResult<T> {
        Err(SyntaxError { message: message.into(), pos: self.tok.span.start })
    }

    fn unexpected<T>(&self, context: &str) -> PResult<T> {
        let found = match &self.tok.tok {
            Tok::Eof => "end of input".to_string(),
            Tok::Ident(n) => format!("identifier '{}'", n),
            Tok::Keyword(k) => format!("'{}'", k.as_str()),
            Tok::Punct(p) => format!("'{}'", p.as_str()),
            Tok::Num(_) | Tok::BigInt(_) => "number".to_string(),
            Tok::Str(_) => "string".to_string(),
            Tok::Template { .. } => "template".to_string(),
            Tok::Regex { .. } => "regular expression".to_string(),
        };
        self.error(format!("unexpected {}: {}", found, context))
    }

    /// Automatic semicolon insertion
    fn consume_semicolon(&mut self) -> PResult<()> {
        if self.eat(P::Semi)? || self.at(P::RBrace) || self.tok.tok == Tok::Eof || self.tok.newline_before {
            Ok(())
        } else {
            self.unexpected("expected ';'")
        }
    }

    fn span_from(&self, start: u32) -> Span {
        Span { start, end: self.prev_end }
    }

    /// Current token as an identifier reference, if it can be one here
    fn ident_reference(&self) -> Option<Name> {
        match &self.tok.tok {
            Tok::Ident(n) => Some(self.name(n)),
            Tok::Keyword(Kw::Let) | Tok::Keyword(Kw::Static) if !self.strict => Some(Rc::from(self.kw_text())),
            Tok::Keyword(Kw::Yield) if !self.in_generator && !self.strict => Some(Rc::from("yield")),
            Tok::Keyword(Kw::Await) if !self.in_async && self.module.is_none() => Some(Rc::from("await")),
            _ => None,
        }
    }

    fn kw_text(&self) -> &'static str {
        match &self.tok.tok {
            Tok::Keyword(k) => k.as_str(),
            _ => "",
        }
    }

    fn parse_binding_identifier(&mut self) -> PResult<Name> {
        match self.ident_reference() {
            Some(name) => {
                if self.strict && matches!(&*name, "eval" | "arguments") {
                    return self.error(format!("'{}' can't be defined in strict mode", name));
                }
                self.advance()?;
                Ok(name)
            }
            None => self.unexpected("expected identifier"),
        }
    }

    /// IdentifierName (any identifier or keyword), e.g. after `.`
    fn parse_identifier_name(&mut self) -> PResult<Name> {
        let name: Name = match &self.tok.tok {
            Tok::Ident(n) => self.name(n),
            Tok::Keyword(k) => self.name(k.as_str()),
            _ => return self.unexpected("expected property name"),
        };
        self.advance()?;
        Ok(name)
    }

    // ---- modules ----

    /// `await` outside any function makes a module asynchronous
    fn note_await(&mut self) {
        if self.depth == 0 && self.module.is_some() {
            self.top_level_await = true;
        }
    }

    fn eat_ident(&mut self, name: &str) -> PResult<bool> {
        if self.at_ident(name) {
            self.advance()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn expect_ident(&mut self, name: &str) -> PResult<()> {
        if self.eat_ident(name)? {
            Ok(())
        } else {
            self.unexpected(&format!("expected '{name}'"))
        }
    }

    fn decls(&mut self) -> &mut ModuleDecls {
        self.module.get_or_insert_with(Default::default)
    }

    /// A module item: an import or export declaration, or a statement
    fn parse_module_item(&mut self, body: &mut Vec<Stmt>) -> PResult<()> {
        if self.at_kw(Kw::Import) {
            let next = self.peek()?;
            if !matches!(next.tok, Tok::Punct(P::LParen) | Tok::Punct(P::Dot)) {
                return self.parse_import_declaration();
            }
        } else if self.at_kw(Kw::Export) {
            return self.parse_export_declaration(body);
        }
        body.push(self.parse_statement_list_item()?);
        Ok(())
    }

    /// An export or import name: an identifier, a keyword, or a string.
    /// Also returns whether it may be a local binding name.
    fn parse_module_export_name(&mut self) -> PResult<(Rc<str>, bool)> {
        if let Tok::Str(units) = &self.tok.tok {
            let name = String::from_utf16(units).map_err(|_| SyntaxError {
                message: "an export name must be valid Unicode".into(),
                pos: self.tok.span.start,
            })?;
            self.advance()?;
            return Ok((Rc::from(name), false));
        }
        let bindable = self.ident_reference().is_some();
        let name = self.parse_identifier_name()?;
        Ok((name, bindable))
    }

    /// `"specifier"` and any `with { type: "json" }` after it
    fn parse_module_request(&mut self) -> PResult<usize> {
        let Tok::Str(units) = &self.tok.tok else { return self.unexpected("expected a module specifier") };
        let specifier: Rc<str> = Rc::from(String::from_utf16_lossy(units));
        self.advance()?;
        let mut json = false;
        // Import attributes (`assert` is their older spelling)
        if self.at_kw(Kw::With) || (self.at_ident("assert") && !self.tok.newline_before) {
            self.advance()?;
            self.expect(P::LBrace)?;
            while !self.eat(P::RBrace)? {
                let (key, _) = self.parse_module_export_name()?;
                self.expect(P::Colon)?;
                let Tok::Str(value) = &self.tok.tok else { return self.unexpected("expected a string") };
                let value = String::from_utf16_lossy(value);
                self.advance()?;
                match (&*key, value.as_str()) {
                    ("type", "json") => json = true,
                    ("type", other) => return self.error(format!("unsupported module type '{other}'")),
                    _ => return self.error(format!("unsupported import attribute '{key}'")),
                }
                if !self.eat(P::Comma)? {
                    self.expect(P::RBrace)?;
                    break;
                }
            }
        }
        let request = ModuleRequest { specifier, json };
        let decls = self.decls();
        Ok(match decls.requests.iter().position(|r| *r == request) {
            Some(i) => i,
            None => {
                decls.requests.push(request);
                decls.requests.len() - 1
            }
        })
    }

    fn parse_import_declaration(&mut self) -> PResult<()> {
        self.expect_kw(Kw::Import)?;
        let mut bindings: Vec<(ImportName, Name)> = Vec::new();
        if !matches!(self.tok.tok, Tok::Str(_)) {
            let mut more = true;
            if !self.at(P::Star) && !self.at(P::LBrace) {
                let local = self.parse_binding_identifier()?;
                bindings.push((ImportName::Name(Rc::from("default")), local));
                more = self.eat(P::Comma)?;
            }
            if more {
                if self.eat(P::Star)? {
                    self.expect_ident("as")?;
                    let local = self.parse_binding_identifier()?;
                    bindings.push((ImportName::Namespace, local));
                } else {
                    self.expect(P::LBrace)?;
                    while !self.eat(P::RBrace)? {
                        let (name, bindable) = self.parse_module_export_name()?;
                        let local = if self.eat_ident("as")? {
                            self.parse_binding_identifier()?
                        } else if bindable {
                            self.name(&name)
                        } else {
                            return self.error(format!("'{name}' cannot be imported without 'as'"));
                        };
                        bindings.push((ImportName::Name(name), local));
                        if !self.eat(P::Comma)? {
                            self.expect(P::RBrace)?;
                            break;
                        }
                    }
                }
            }
            self.expect_ident("from")?;
        }
        let request = self.parse_module_request()?;
        self.consume_semicolon()?;
        let decls = self.decls();
        for (import, local) in bindings {
            decls.imports.push(ImportEntry { request, import, local });
        }
        Ok(())
    }

    fn parse_export_declaration(&mut self, body: &mut Vec<Stmt>) -> PResult<()> {
        self.expect_kw(Kw::Export)?;
        // export * from "m" / export * as ns from "m"
        if self.eat(P::Star)? {
            let export = if self.eat_ident("as")? { Some(self.parse_module_export_name()?.0) } else { None };
            self.expect_ident("from")?;
            let request = self.parse_module_request()?;
            self.consume_semicolon()?;
            let entry = match export {
                Some(export) => ExportEntry::Indirect { export, request, import: ImportName::Namespace },
                None => ExportEntry::Star { request },
            };
            self.decls().exports.push(entry);
            return Ok(());
        }
        let default: Rc<str> = Rc::from("default");
        if self.eat_kw(Kw::Default)? {
            let async_function = self.at_ident("async") && {
                let next = self.peek()?;
                next.tok == Tok::Keyword(Kw::Function) && !next.newline_before
            };
            let local: Name = if self.at_kw(Kw::Function) || async_function {
                if async_function {
                    self.advance()?;
                }
                // A declaration, whose name is optional here
                let mut f = self.parse_function(async_function, false)?;
                let local = f.name.clone().unwrap_or_else(|| self.name(DEFAULT_EXPORT));
                f.name = Some(local.clone());
                body.push(Stmt::Function(Box::new(f)));
                local
            } else if self.at_kw(Kw::Class) {
                let c = self.parse_class(false)?;
                match c.name.clone() {
                    Some(name) => {
                        body.push(Stmt::Class(Box::new(c)));
                        name
                    }
                    None => {
                        let local = self.name(DEFAULT_EXPORT);
                        body.push(Stmt::Var {
                            kind: VarKind::Const,
                            decls: vec![VarDecl { target: Pattern::Ident(local.clone()), init: Some(Expr::Class(Box::new(c))) }],
                        });
                        local
                    }
                }
            } else {
                let value = self.parse_assign()?;
                self.consume_semicolon()?;
                let local = self.name(DEFAULT_EXPORT);
                body.push(Stmt::Var { kind: VarKind::Const, decls: vec![VarDecl { target: Pattern::Ident(local.clone()), init: Some(value) }] });
                local
            };
            self.decls().exports.push(ExportEntry::Local { export: default, local });
            return Ok(());
        }
        // export { a, b as c } [from "m"]
        if self.eat(P::LBrace)? {
            let mut specs: Vec<(Rc<str>, bool, Rc<str>)> = Vec::new();
            while !self.eat(P::RBrace)? {
                let (name, bindable) = self.parse_module_export_name()?;
                let export = if self.eat_ident("as")? { self.parse_module_export_name()?.0 } else { name.clone() };
                specs.push((name, bindable, export));
                if !self.eat(P::Comma)? {
                    self.expect(P::RBrace)?;
                    break;
                }
            }
            if self.eat_ident("from")? {
                let request = self.parse_module_request()?;
                self.consume_semicolon()?;
                for (name, _, export) in specs {
                    self.decls().exports.push(ExportEntry::Indirect { export, request, import: ImportName::Name(name) });
                }
            } else {
                self.consume_semicolon()?;
                for (name, bindable, export) in specs {
                    if !bindable {
                        return self.error(format!("'{name}' is not a local binding to export"));
                    }
                    let local = self.name(&name);
                    self.decls().exports.push(ExportEntry::Local { export, local });
                }
            }
            return Ok(());
        }
        // export var/let/const/function/class
        let declaration = match &self.tok.tok {
            Tok::Keyword(Kw::Var | Kw::Let | Kw::Const | Kw::Function | Kw::Class) => true,
            Tok::Ident(n) if &**n == "async" => true,
            _ => false,
        };
        if !declaration {
            return self.unexpected("expected a declaration to export");
        }
        let stmt = if self.at_kw(Kw::Var) { self.parse_statement()? } else { self.parse_statement_list_item()? };
        let mut names = Vec::new();
        match &stmt {
            Stmt::Var { decls, .. } => {
                for d in decls {
                    pattern_names(&d.target, &mut names);
                }
            }
            Stmt::Function(f) => names.extend(f.name.clone()),
            Stmt::Class(c) => names.extend(c.name.clone()),
            _ => return self.error("expected a declaration to export"),
        }
        body.push(stmt);
        for name in names {
            self.decls().exports.push(ExportEntry::Local { export: name.clone(), local: name });
        }
        Ok(())
    }

    // ---- statements ----

    /// Parse statements up to end of input, with a directive prologue
    fn parse_body_until_eof(&mut self) -> PResult<(Vec<Stmt>, bool)> {
        let mut body = Vec::new();
        self.parse_directives(&mut body)?;
        while self.tok.tok != Tok::Eof {
            body.push(self.parse_statement_list_item()?);
        }
        Ok((body, self.strict))
    }

    /// Parse a directive prologue into `body`, turning on strict mode for
    /// `"use strict"`
    fn parse_directives(&mut self, body: &mut Vec<Stmt>) -> PResult<()> {
        while let Tok::Str(_) = &self.tok.tok {
            let span = self.tok.span;
            let raw = &self.lexer.source()[span.start as usize + 1..span.end as usize - 1];
            let is_use_strict = raw == "use strict";
            let stmt = self.parse_statement()?;
            let is_directive = matches!(&stmt, Stmt::Expr(Expr::Str(_)));
            body.push(stmt);
            if !is_directive {
                break;
            }
            if is_use_strict {
                self.strict = true;
            }
        }
        Ok(())
    }

    fn parse_statement_list_item(&mut self) -> PResult<Stmt> {
        match &self.tok.tok {
            Tok::Keyword(Kw::Function) => Ok(Stmt::Function(Box::new(self.parse_function(false, true)?))),
            Tok::Keyword(Kw::Class) => Ok(Stmt::Class(Box::new(self.parse_class(true)?))),
            Tok::Keyword(Kw::Const) => self.parse_lexical_declaration(),
            Tok::Keyword(Kw::Let) if self.let_starts_declaration()? => self.parse_lexical_declaration(),
            Tok::Ident(n) if &**n == "async" && !self.tok.escaped => {
                let next = self.peek()?;
                if next.tok == Tok::Keyword(Kw::Function) && !next.newline_before {
                    self.advance()?;
                    return Ok(Stmt::Function(Box::new(self.parse_function(true, true)?)));
                }
                self.parse_statement()
            }
            _ => self.parse_statement(),
        }
    }

    fn let_starts_declaration(&self) -> PResult<bool> {
        let next = self.peek()?;
        Ok(match next.tok {
            Tok::Ident(_) | Tok::Punct(P::LBracket) | Tok::Punct(P::LBrace) => true,
            Tok::Keyword(k) => k.is_contextual(),
            _ => false,
        })
    }

    fn parse_lexical_declaration(&mut self) -> PResult<Stmt> {
        let kind = if self.eat_kw(Kw::Const)? {
            VarKind::Const
        } else {
            self.expect_kw(Kw::Let)?;
            VarKind::Let
        };
        let decls = self.parse_declarators(kind)?;
        self.consume_semicolon()?;
        Ok(Stmt::Var { kind, decls })
    }

    fn parse_declarators(&mut self, kind: VarKind) -> PResult<Vec<VarDecl>> {
        let mut decls = Vec::new();
        loop {
            let target = self.parse_binding_target()?;
            let init = if self.eat(P::Eq)? {
                Some(self.parse_assign()?)
            } else {
                if kind == VarKind::Const && !self.at_ident("of") && !self.at_kw(Kw::In) {
                    return self.error("missing initializer in const declaration");
                }
                if !matches!(target, Pattern::Ident(_)) && !self.at_ident("of") && !self.at_kw(Kw::In) {
                    return self.error("missing initializer in destructuring declaration");
                }
                None
            };
            decls.push(VarDecl { target, init });
            if !self.eat(P::Comma)? {
                break;
            }
        }
        Ok(decls)
    }

    fn parse_statement(&mut self) -> PResult<Stmt> {
        match self.tok.tok.clone() {
            Tok::Punct(P::LBrace) => Ok(Stmt::Block(self.parse_block()?)),
            Tok::Punct(P::Semi) => {
                self.advance()?;
                Ok(Stmt::Empty)
            }
            Tok::Keyword(Kw::Var) => {
                self.advance()?;
                let decls = self.parse_declarators(VarKind::Var)?;
                self.consume_semicolon()?;
                Ok(Stmt::Var { kind: VarKind::Var, decls })
            }
            Tok::Keyword(Kw::If) => {
                self.advance()?;
                self.expect(P::LParen)?;
                let test = self.parse_expression()?;
                self.expect(P::RParen)?;
                let cons = Box::new(self.parse_substatement()?);
                let alt = if self.eat_kw(Kw::Else)? { Some(Box::new(self.parse_substatement()?)) } else { None };
                Ok(Stmt::If { test, cons, alt })
            }
            Tok::Keyword(Kw::For) => self.parse_for(),
            Tok::Keyword(Kw::While) => {
                self.advance()?;
                self.expect(P::LParen)?;
                let test = self.parse_expression()?;
                self.expect(P::RParen)?;
                let body = Box::new(self.parse_substatement()?);
                Ok(Stmt::While { test, body })
            }
            Tok::Keyword(Kw::Do) => {
                self.advance()?;
                let body = Box::new(self.parse_substatement()?);
                self.expect_kw(Kw::While)?;
                self.expect(P::LParen)?;
                let test = self.parse_expression()?;
                self.expect(P::RParen)?;
                // A semicolon after do-while is optional
                self.eat(P::Semi)?;
                Ok(Stmt::DoWhile { body, test })
            }
            Tok::Keyword(Kw::Continue) | Tok::Keyword(Kw::Break) => {
                let is_break = self.at_kw(Kw::Break);
                self.advance()?;
                let label = if !self.tok.newline_before {
                    match self.ident_reference() {
                        Some(name) => {
                            self.advance()?;
                            Some(name)
                        }
                        None => None,
                    }
                } else {
                    None
                };
                self.consume_semicolon()?;
                Ok(if is_break { Stmt::Break(label) } else { Stmt::Continue(label) })
            }
            Tok::Keyword(Kw::Return) => {
                if !self.in_function {
                    return self.error("return outside of a function");
                }
                self.advance()?;
                let arg = if self.at(P::Semi) || self.at(P::RBrace) || self.tok.tok == Tok::Eof || self.tok.newline_before {
                    None
                } else {
                    Some(self.parse_expression()?)
                };
                self.consume_semicolon()?;
                Ok(Stmt::Return(arg))
            }
            Tok::Keyword(Kw::Throw) => {
                self.advance()?;
                if self.tok.newline_before {
                    return self.error("line break after throw");
                }
                let arg = self.parse_expression()?;
                self.consume_semicolon()?;
                Ok(Stmt::Throw(arg))
            }
            Tok::Keyword(Kw::Try) => self.parse_try(),
            Tok::Keyword(Kw::Switch) => self.parse_switch(),
            Tok::Keyword(Kw::With) => {
                if self.strict {
                    return self.error("'with' is not allowed in strict mode");
                }
                self.advance()?;
                self.expect(P::LParen)?;
                let object = self.parse_expression()?;
                self.expect(P::RParen)?;
                let body = Box::new(self.parse_substatement()?);
                Ok(Stmt::With { object, body })
            }
            Tok::Keyword(Kw::Debugger) => {
                self.advance()?;
                self.consume_semicolon()?;
                Ok(Stmt::Debugger)
            }
            Tok::Keyword(Kw::Function) => Ok(Stmt::Function(Box::new(self.parse_function(false, true)?))),
            Tok::Keyword(Kw::Class) => Ok(Stmt::Class(Box::new(self.parse_class(true)?))),
            _ => {
                // Labeled statement
                if let Some(name) = self.ident_reference() {
                    if self.next_bytes_are(":") && self.peek()?.tok == Tok::Punct(P::Colon) {
                        self.advance()?;
                        self.advance()?;
                        let body = Box::new(self.parse_labeled_body()?);
                        return Ok(Stmt::Labeled { label: name, body });
                    }
                }
                let expr = self.parse_expression()?;
                self.consume_semicolon()?;
                Ok(Stmt::Expr(expr))
            }
        }
    }

    fn parse_labeled_body(&mut self) -> PResult<Stmt> {
        if self.at_kw(Kw::Function) {
            return Ok(Stmt::Function(Box::new(self.parse_function(false, true)?)));
        }
        self.parse_statement()
    }

    /// A statement in a position that does not allow declarations (the body
    /// of `if`, loops etc.). Function declarations are allowed in sloppy
    /// mode (Annex B).
    fn parse_substatement(&mut self) -> PResult<Stmt> {
        if self.at_kw(Kw::Class) || self.at_kw(Kw::Const) {
            return self.unexpected("declaration not allowed here");
        }
        if self.at_kw(Kw::Let) && self.peek()?.tok == Tok::Punct(P::LBracket) {
            return self.unexpected("lexical declaration not allowed here");
        }
        self.parse_statement()
    }

    fn parse_block(&mut self) -> PResult<Vec<Stmt>> {
        self.expect(P::LBrace)?;
        let mut body = Vec::new();
        while !self.at(P::RBrace) {
            if self.tok.tok == Tok::Eof {
                return self.unexpected("expected '}'");
            }
            body.push(self.parse_statement_list_item()?);
        }
        self.advance()?;
        Ok(body)
    }

    fn parse_for(&mut self) -> PResult<Stmt> {
        self.expect_kw(Kw::For)?;
        let is_await = if self.at_kw(Kw::Await) && self.in_async {
            self.advance()?;
            self.note_await();
            true
        } else {
            false
        };
        self.expect(P::LParen)?;

        // Declaration head
        let decl_kind = match &self.tok.tok {
            Tok::Keyword(Kw::Var) => Some(VarKind::Var),
            Tok::Keyword(Kw::Const) => Some(VarKind::Const),
            Tok::Keyword(Kw::Let) if self.let_starts_declaration()? => Some(VarKind::Let),
            _ => None,
        };
        if let Some(kind) = decl_kind {
            self.advance()?;
            let saved = std::mem::replace(&mut self.no_in, true);
            let decls = self.parse_declarators(kind);
            self.no_in = saved;
            let mut decls = decls?;
            if decls.len() == 1 && decls[0].init.is_none() && (self.at_ident("of") || self.at_kw(Kw::In)) {
                let target = decls.pop().unwrap().target;
                return self.parse_for_in_of(ForHead::Decl(kind, target), is_await);
            }
            // `for (var x = 1 in obj)` (Annex B) is not supported
            self.expect(P::Semi)?;
            return self.parse_for_rest(Some(ForInit::Var(kind, decls)));
        }

        if self.eat(P::Semi)? {
            return self.parse_for_rest(None);
        }

        let saved = std::mem::replace(&mut self.no_in, true);
        let before = self.cover_inits;
        let init = self.parse_expression_cover();
        self.no_in = saved;
        let init = init?;
        if self.at_ident("of") || self.at_kw(Kw::In) {
            self.cover_inits = before;
            let target = self.expr_to_pattern(init, false)?;
            return self.parse_for_in_of(ForHead::Target(target), is_await);
        }
        if self.cover_inits > before {
            return self.error("invalid shorthand property initializer");
        }
        self.expect(P::Semi)?;
        self.parse_for_rest(Some(ForInit::Expr(init)))
    }

    fn parse_for_in_of(&mut self, head: ForHead, is_await: bool) -> PResult<Stmt> {
        if self.eat_kw(Kw::In)? {
            let object = self.parse_expression()?;
            self.expect(P::RParen)?;
            let body = Box::new(self.parse_substatement()?);
            Ok(Stmt::ForIn { head, object, body })
        } else {
            self.advance()?; // `of`
            let iterable = self.parse_assign()?;
            self.expect(P::RParen)?;
            let body = Box::new(self.parse_substatement()?);
            Ok(Stmt::ForOf { head, iterable, body, is_await })
        }
    }

    fn parse_for_rest(&mut self, init: Option<ForInit>) -> PResult<Stmt> {
        let test = if self.at(P::Semi) { None } else { Some(self.parse_expression()?) };
        self.expect(P::Semi)?;
        let update = if self.at(P::RParen) { None } else { Some(self.parse_expression()?) };
        self.expect(P::RParen)?;
        let body = Box::new(self.parse_substatement()?);
        Ok(Stmt::For { init, test, update, body })
    }

    fn parse_try(&mut self) -> PResult<Stmt> {
        self.expect_kw(Kw::Try)?;
        let block = self.parse_block()?;
        let (mut param, mut handler, mut finalizer) = (None, None, None);
        if self.eat_kw(Kw::Catch)? {
            if self.eat(P::LParen)? {
                param = Some(self.parse_binding_target()?);
                self.expect(P::RParen)?;
            }
            handler = Some(self.parse_block()?);
        }
        if self.eat_kw(Kw::Finally)? {
            finalizer = Some(self.parse_block()?);
        }
        if handler.is_none() && finalizer.is_none() {
            return self.unexpected("expected 'catch' or 'finally'");
        }
        Ok(Stmt::Try { block, param, handler, finalizer })
    }

    fn parse_switch(&mut self) -> PResult<Stmt> {
        self.expect_kw(Kw::Switch)?;
        self.expect(P::LParen)?;
        let discriminant = self.parse_expression()?;
        self.expect(P::RParen)?;
        self.expect(P::LBrace)?;
        let mut cases = Vec::new();
        let mut seen_default = false;
        while !self.eat(P::RBrace)? {
            let test = if self.eat_kw(Kw::Case)? {
                Some(self.parse_expression()?)
            } else {
                self.expect_kw(Kw::Default)?;
                if seen_default {
                    return self.error("more than one default clause in switch");
                }
                seen_default = true;
                None
            };
            self.expect(P::Colon)?;
            let mut body = Vec::new();
            while !self.at_kw(Kw::Case) && !self.at_kw(Kw::Default) && !self.at(P::RBrace) {
                if self.tok.tok == Tok::Eof {
                    return self.unexpected("expected '}'");
                }
                body.push(self.parse_statement_list_item()?);
            }
            cases.push(SwitchCase { test, body });
        }
        Ok(Stmt::Switch { discriminant, cases })
    }

    // ---- functions and classes ----

    /// `function` keyword onwards (after any `async`)
    fn parse_function(&mut self, is_async: bool, declaration: bool) -> PResult<Function> {
        let start = self.tok.span.start;
        self.expect_kw(Kw::Function)?;
        let is_generator = self.eat(P::Star)?;
        let name = if declaration || self.ident_reference().is_some() {
            // The name of a generator/async function expression is bound
            // in its own scope, so yield/await rules of the inner body apply
            let saved = (self.in_generator, self.in_async);
            if !declaration {
                self.in_generator = is_generator;
                self.in_async = is_async;
            }
            let name = self.parse_binding_identifier();
            (self.in_generator, self.in_async) = saved;
            Some(name?)
        } else {
            None
        };
        self.parse_function_rest(name, FunctionKind::Normal, is_async, is_generator, start)
    }

    /// Parameters and body
    fn parse_function_rest(
        &mut self,
        name: Option<Name>,
        kind: FunctionKind,
        is_async: bool,
        is_generator: bool,
        start: u32,
    ) -> PResult<Function> {
        let saved = (self.in_function, self.in_generator, self.in_async, self.strict, self.no_in);
        self.in_function = true;
        self.in_generator = is_generator;
        self.in_async = is_async;
        self.no_in = false;
        let params_start = self.tok.span.start;
        let pre = self.preparse_function(start);
        self.depth += 1;
        let result = (|| {
            let (params, rest) = self.parse_params()?;
            self.expect(P::LBrace)?;
            if pre {
                let mut free = FreeNames::default();
                params.iter().for_each(|p| free.param(p));
                if let Some(r) = &rest {
                    free.rest(r);
                }
                self.parse_body_items(Some(&mut free))?;
                self.advance()?;
                let lazy = LazyBody {
                    free: free.finish(kind),
                    nparams: params.len() as u32,
                    length: params.iter().take_while(|p| p.default.is_none()).count() as u32,
                };
                return Ok((Vec::new(), None, FunctionBody::Lazy(Box::new(lazy)), false));
            }
            let body = self.parse_body_items(None)?;
            self.advance()?;
            let simple = rest.is_none() && params.iter().all(|p| p.default.is_none() && matches!(p.target, Pattern::Ident(_)));
            Ok((params, rest, FunctionBody::Block(body), simple))
        })();
        let strict = self.strict;
        self.depth -= 1;
        (self.in_function, self.in_generator, self.in_async, self.strict, self.no_in) = saved;
        let (params, rest, body, simple_params) = result?;
        Ok(Function {
            name,
            params,
            rest,
            body,
            kind,
            is_async,
            is_generator,
            strict,
            simple_params,
            span: self.span_from(start),
            params_start,
        })
    }

    fn parse_params(&mut self) -> PResult<(Vec<Param>, Option<Pattern>)> {
        self.expect(P::LParen)?;
        let mut params = Vec::new();
        let mut rest = None;
        while !self.at(P::RParen) {
            if self.eat(P::Ellipsis)? {
                rest = Some(self.parse_binding_target()?);
                break;
            }
            let target = self.parse_binding_target()?;
            let default = if self.eat(P::Eq)? { Some(self.parse_assign()?) } else { None };
            params.push(Param { target, default });
            if !self.eat(P::Comma)? {
                break;
            }
        }
        self.expect(P::RParen)?;
        Ok((params, rest))
    }

    fn parse_class(&mut self, declaration: bool) -> PResult<Class> {
        let start = self.tok.span.start;
        self.expect_kw(Kw::Class)?;
        let saved_strict = std::mem::replace(&mut self.strict, true);
        let result = (|| {
            let name = if self.ident_reference().is_some() && !self.at_kw(Kw::Let) {
                Some(self.parse_binding_identifier()?)
            } else if declaration {
                return self.unexpected("expected class name");
            } else {
                None
            };
            let extends = if self.eat_kw(Kw::Extends)? { Some(self.parse_lhs()?) } else { None };
            self.expect(P::LBrace)?;
            let mut constructor = None;
            let mut members = Vec::new();
            while !self.eat(P::RBrace)? {
                if self.eat(P::Semi)? {
                    continue;
                }
                let member_start = self.tok.span.start;
                // `static` modifier (unless it is the member's name)
                let mut is_static = false;
                if self.at_kw(Kw::Static) {
                    let next = self.peek()?;
                    if !matches!(next.tok, Tok::Punct(P::LParen) | Tok::Punct(P::Eq) | Tok::Punct(P::Semi) | Tok::Punct(P::RBrace)) {
                        self.advance()?;
                        is_static = true;
                        if self.at(P::LBrace) {
                            let saved = (self.in_function, self.in_async, self.in_generator);
                            self.in_function = false;
                            let block = self.parse_block();
                            (self.in_function, self.in_async, self.in_generator) = saved;
                            members.push(ClassMember { key: PropKey::Name(utf16("")), is_static: true, kind: ClassMemberKind::StaticBlock(block?) });
                            continue;
                        }
                    }
                }
                let (kind, is_async, is_generator, key) = self.parse_method_head(true)?;
                if self.at(P::LParen) {
                    let is_ctor = !is_static && matches!(&key, PropKey::Name(n) if &**n == utf16("constructor").as_ref());
                    let fkind = if is_ctor {
                        if kind != MethodKind::Method || is_async || is_generator {
                            return self.error("class constructor may not be an accessor, generator or async");
                        }
                        if extends.is_some() { FunctionKind::DerivedConstructor } else { FunctionKind::ClassConstructor }
                    } else {
                        match kind {
                            MethodKind::Method => FunctionKind::Method,
                            MethodKind::Getter => FunctionKind::Getter,
                            MethodKind::Setter => FunctionKind::Setter,
                        }
                    };
                    let name = prop_key_name(&key);
                    let func = self.parse_function_rest(name, fkind, is_async, is_generator, member_start)?;
                    if is_ctor {
                        if constructor.is_some() {
                            return self.error("a class may only have one constructor");
                        }
                        constructor = Some(Box::new(func));
                    } else {
                        members.push(ClassMember { key, is_static, kind: ClassMemberKind::Method(kind, Box::new(func)) });
                    }
                } else {
                    // Field
                    if kind != MethodKind::Method || is_async || is_generator {
                        return self.unexpected("expected '('");
                    }
                    let init = if self.eat(P::Eq)? {
                        let saved = (self.in_function, self.in_async, self.in_generator);
                        self.in_function = true;
                        self.in_async = false;
                        self.in_generator = false;
                        let value = self.parse_assign();
                        (self.in_function, self.in_async, self.in_generator) = saved;
                        Some(value?)
                    } else {
                        None
                    };
                    self.consume_semicolon()?;
                    members.push(ClassMember { key, is_static, kind: ClassMemberKind::Field(init) });
                }
            }
            Ok(Class { name, extends, constructor, members, span: self.span_from(start) })
        })();
        self.strict = saved_strict;
        result
    }

    /// Modifiers and key of an object or class method: `get x`, `set x`,
    /// `async x`, `*x`, `async *x`, or a plain key
    fn parse_method_head(&mut self, allow_private: bool) -> PResult<(MethodKind, bool, bool, PropKey)> {
        let mut kind = MethodKind::Method;
        let mut is_async = false;
        let mut is_generator = false;

        if self.at_ident("get") || self.at_ident("set") || self.at_ident("async") {
            let next = self.peek()?;
            let is_modifier = !matches!(
                next.tok,
                Tok::Punct(P::LParen) | Tok::Punct(P::Comma) | Tok::Punct(P::Colon) | Tok::Punct(P::RBrace)
                    | Tok::Punct(P::Eq) | Tok::Punct(P::Semi)
            ) && !(self.at_ident("async") && next.newline_before);
            if is_modifier {
                match &self.tok.tok {
                    Tok::Ident(n) if &**n == "get" => kind = MethodKind::Getter,
                    Tok::Ident(n) if &**n == "set" => kind = MethodKind::Setter,
                    _ => is_async = true,
                }
                self.advance()?;
            }
        }
        if (kind == MethodKind::Method) && self.eat(P::Star)? {
            is_generator = true;
        }
        let key = self.parse_property_key(allow_private)?;
        Ok((kind, is_async, is_generator, key))
    }

    fn parse_property_key(&mut self, allow_private: bool) -> PResult<PropKey> {
        let key = match self.tok.tok.clone() {
            Tok::Ident(n) => PropKey::Name(utf16(&n)),
            Tok::Keyword(k) => PropKey::Name(utf16(k.as_str())),
            Tok::Str(s) => PropKey::Name(s),
            Tok::Num(n) => PropKey::Num(n),
            Tok::BigInt(text) => PropKey::Name(utf16(&text)),
            Tok::Punct(P::LBracket) => {
                self.advance()?;
                let saved = std::mem::replace(&mut self.no_in, false);
                let expr = self.parse_assign();
                self.no_in = saved;
                let expr = expr?;
                self.expect(P::RBracket)?;
                return Ok(PropKey::Computed(Box::new(expr)));
            }
            Tok::Punct(P::Hash) if allow_private => {
                self.advance()?;
                let name = self.parse_identifier_name()?;
                return Ok(PropKey::Private(name));
            }
            _ => return self.unexpected("expected property name"),
        };
        self.advance()?;
        Ok(key)
    }

    // ---- patterns ----

    /// Binding pattern: identifier, `[...]` or `{...}`
    fn parse_binding_target(&mut self) -> PResult<Pattern> {
        match &self.tok.tok {
            Tok::Punct(P::LBracket) => {
                self.advance()?;
                let mut elems = Vec::new();
                let mut rest = None;
                loop {
                    if self.eat(P::RBracket)? {
                        break;
                    }
                    if self.eat(P::Comma)? {
                        elems.push(None);
                        continue;
                    }
                    if self.eat(P::Ellipsis)? {
                        rest = Some(Box::new(self.parse_binding_target()?));
                        self.expect(P::RBracket)?;
                        break;
                    }
                    let target = self.parse_binding_target()?;
                    let default = if self.eat(P::Eq)? { Some(self.parse_assign()?) } else { None };
                    elems.push(Some(PatternElem { target, default }));
                    if !self.at(P::RBracket) {
                        self.expect(P::Comma)?;
                    }
                }
                Ok(Pattern::Array { elems, rest })
            }
            Tok::Punct(P::LBrace) => {
                self.advance()?;
                let mut props = Vec::new();
                let mut rest = None;
                while !self.eat(P::RBrace)? {
                    if self.eat(P::Ellipsis)? {
                        rest = Some(Box::new(Pattern::Ident(self.parse_binding_identifier()?)));
                        self.expect(P::RBrace)?;
                        break;
                    }
                    let shorthand = self.ident_reference();
                    let key = self.parse_property_key(false)?;
                    let target = if self.eat(P::Colon)? {
                        self.parse_binding_target()?
                    } else {
                        match shorthand {
                            Some(name) => {
                                if self.strict && matches!(&*name, "eval" | "arguments") {
                                    return self.error(format!("'{}' can't be defined in strict mode", name));
                                }
                                Pattern::Ident(name)
                            }
                            None => return self.unexpected("expected ':'"),
                        }
                    };
                    let default = if self.eat(P::Eq)? { Some(self.parse_assign()?) } else { None };
                    props.push(PatternProp { key, target, default });
                    if !self.at(P::RBrace) {
                        self.expect(P::Comma)?;
                    }
                }
                Ok(Pattern::Object { props, rest })
            }
            _ => Ok(Pattern::Ident(self.parse_binding_identifier()?)),
        }
    }

    /// Reinterpret an expression as an assignment target (or, with
    /// `binding`, a binding pattern for arrow parameters)
    fn expr_to_pattern(&self, expr: Expr, binding: bool) -> PResult<Pattern> {
        match expr {
            Expr::Ident(name) => {
                if self.strict && matches!(&*name, "eval" | "arguments") {
                    return self.error(format!("can't assign to '{}' in strict mode", name));
                }
                Ok(Pattern::Ident(name))
            }
            Expr::Member { optional: false, .. } | Expr::SuperMember(_) if !binding => Ok(Pattern::Member(Box::new(expr))),
            Expr::Paren(inner) if !binding && matches!(*inner, Expr::Ident(_) | Expr::Member { .. } | Expr::Paren(_)) => {
                self.expr_to_pattern(*inner, binding)
            }
            Expr::Array(items) => {
                let mut elems = Vec::new();
                let mut rest = None;
                let count = items.len();
                for (i, item) in items.into_iter().enumerate() {
                    match item {
                        None => elems.push(None),
                        Some(ArrayElem::Spread(e)) => {
                            if i + 1 != count {
                                return self.error("rest element must be last");
                            }
                            rest = Some(Box::new(self.expr_to_pattern(e, binding)?));
                        }
                        Some(ArrayElem::Expr(e)) => elems.push(Some(self.expr_to_pattern_elem(e, binding)?)),
                    }
                }
                Ok(Pattern::Array { elems, rest })
            }
            Expr::Object(items) => {
                let mut props = Vec::new();
                let mut rest = None;
                let count = items.len();
                for (i, item) in items.into_iter().enumerate() {
                    match item {
                        ObjProp::KeyValue(key, value) => {
                            let PatternElem { target, default } = self.expr_to_pattern_elem(value, binding)?;
                            props.push(PatternProp { key, target, default });
                        }
                        ObjProp::Shorthand(name) => {
                            props.push(PatternProp { key: PropKey::Name(utf16(&name)), target: Pattern::Ident(name), default: None });
                        }
                        ObjProp::CoverInit(name, default) => {
                            props.push(PatternProp { key: PropKey::Name(utf16(&name)), target: Pattern::Ident(name), default: Some(default) });
                        }
                        ObjProp::Spread(e) => {
                            if i + 1 != count {
                                return self.error("rest element must be last");
                            }
                            rest = Some(Box::new(self.expr_to_pattern(e, binding)?));
                        }
                        ObjProp::Method { .. } => return self.error("invalid destructuring target"),
                    }
                }
                Ok(Pattern::Object { props, rest })
            }
            _ => self.error("invalid assignment target"),
        }
    }

    fn expr_to_pattern_elem(&self, expr: Expr, binding: bool) -> PResult<PatternElem> {
        match expr {
            Expr::Assign { op: AssignOp::Assign, target, value } => Ok(PatternElem { target: *target, default: Some(*value) }),
            other => Ok(PatternElem { target: self.expr_to_pattern(other, binding)?, default: None }),
        }
    }

    // ---- expressions ----

    pub fn parse_expression(&mut self) -> PResult<Expr> {
        let before = self.cover_inits;
        let expr = self.parse_expression_cover()?;
        if self.cover_inits > before {
            return self.error("invalid shorthand property initializer");
        }
        Ok(expr)
    }

    /// Comma expression that may still turn into a pattern
    fn parse_expression_cover(&mut self) -> PResult<Expr> {
        let first = self.parse_assign_cover()?;
        if !self.at(P::Comma) {
            return Ok(first);
        }
        let mut exprs = vec![first];
        while self.eat(P::Comma)? {
            exprs.push(self.parse_assign_cover()?);
        }
        Ok(Expr::Seq(exprs))
    }

    fn parse_assign(&mut self) -> PResult<Expr> {
        let before = self.cover_inits;
        let expr = self.parse_assign_cover()?;
        if self.cover_inits > before {
            return self.error("invalid shorthand property initializer");
        }
        Ok(expr)
    }

    /// AssignmentExpression that may contain `{ a = 1 }` shorthands, to be
    /// validated by the caller once it knows whether this is a pattern
    fn parse_assign_cover(&mut self) -> PResult<Expr> {
        if self.at_kw(Kw::Yield) && self.in_generator {
            return self.parse_yield();
        }

        // `x => ...` and `async x => ...`
        if let Some(name) = self.ident_reference().filter(|n| self.next_bytes_are("=>") || &**n == "async") {
            let next = self.peek()?;
            if next.tok == Tok::Punct(P::Arrow) && !next.newline_before {
                let start = self.tok.span.start;
                self.advance()?;
                let params = vec![Param { target: Pattern::Ident(name), default: None }];
                return self.parse_arrow_body(params, None, false, start);
            }
            if &*name == "async" && !self.tok.escaped && !next.newline_before {
                if let Tok::Ident(_) = &next.tok {
                    let start = self.tok.span.start;
                    self.advance()?;
                    let saved = std::mem::replace(&mut self.in_async, true);
                    let param = self.parse_binding_identifier();
                    self.in_async = saved;
                    let param = param?;
                    if !self.at(P::Arrow) || self.tok.newline_before {
                        return self.unexpected("expected '=>'");
                    }
                    let params = vec![Param { target: Pattern::Ident(param), default: None }];
                    return self.parse_arrow_body(params, None, true, start);
                }
            }
        }

        let start = self.tok.span.start;
        let before = self.cover_inits;
        let left = self.parse_conditional()?;

        // `async (a, b) => ...` parsed as a call to `async`
        if self.at(P::Arrow) && !self.tok.newline_before {
            if let Expr::Call { callee, args, optional: false } = left {
                if matches!(&*callee, Expr::Ident(n) if &**n == "async") {
                    self.cover_inits = before;
                    let (params, rest) = self.args_to_params(args)?;
                    return self.parse_arrow_body(params, rest, true, start);
                }
                return self.unexpected("unexpected '=>'");
            }
            return self.unexpected("unexpected '=>'");
        }

        let op = match &self.tok.tok {
            Tok::Punct(p) => match p {
                P::Eq => Some(AssignOp::Assign),
                P::PlusEq => Some(AssignOp::Op(BinaryOp::Add)),
                P::MinusEq => Some(AssignOp::Op(BinaryOp::Sub)),
                P::StarEq => Some(AssignOp::Op(BinaryOp::Mul)),
                P::SlashEq => Some(AssignOp::Op(BinaryOp::Div)),
                P::PercentEq => Some(AssignOp::Op(BinaryOp::Mod)),
                P::StarStarEq => Some(AssignOp::Op(BinaryOp::Exp)),
                P::ShlEq => Some(AssignOp::Op(BinaryOp::Shl)),
                P::SarEq => Some(AssignOp::Op(BinaryOp::Sar)),
                P::ShrEq => Some(AssignOp::Op(BinaryOp::Shr)),
                P::AmpEq => Some(AssignOp::Op(BinaryOp::BitAnd)),
                P::PipeEq => Some(AssignOp::Op(BinaryOp::BitOr)),
                P::CaretEq => Some(AssignOp::Op(BinaryOp::BitXor)),
                P::AmpAmpEq => Some(AssignOp::Logical(LogicalOp::And)),
                P::PipePipeEq => Some(AssignOp::Logical(LogicalOp::Or)),
                P::QuestionQuestionEq => Some(AssignOp::Logical(LogicalOp::Nullish)),
                _ => None,
            },
            _ => None,
        };
        let Some(op) = op else { return Ok(left) };

        let target = if op == AssignOp::Assign {
            self.cover_inits = before;
            self.expr_to_pattern(left, false)?
        } else {
            match left {
                Expr::Ident(_) | Expr::Member { optional: false, .. } | Expr::SuperMember(_) | Expr::Paren(_) => {
                    match self.expr_to_pattern(left, false)? {
                        p @ (Pattern::Ident(_) | Pattern::Member(_)) => p,
                        _ => return self.error("invalid assignment target"),
                    }
                }
                _ => return self.error("invalid assignment target"),
            }
        };
        self.advance()?;
        let value = self.parse_assign()?;
        Ok(Expr::Assign { op, target: Box::new(target), value: Box::new(value) })
    }

    fn args_to_params(&self, args: Vec<ArrayElem>) -> PResult<(Vec<Param>, Option<Pattern>)> {
        let mut params = Vec::new();
        let mut rest = None;
        let count = args.len();
        for (i, arg) in args.into_iter().enumerate() {
            match arg {
                ArrayElem::Expr(e) => {
                    let PatternElem { target, default } = self.expr_to_pattern_elem(e, true)?;
                    params.push(Param { target, default });
                }
                ArrayElem::Spread(e) => {
                    if i + 1 != count {
                        return self.error("rest parameter must be last");
                    }
                    rest = Some(self.expr_to_pattern(e, true)?);
                }
            }
        }
        Ok((params, rest))
    }

    /// After the parameters: `=> body`
    fn parse_arrow_body(&mut self, params: Vec<Param>, rest: Option<Pattern>, is_async: bool, start: u32) -> PResult<Expr> {
        if self.tok.newline_before {
            return self.error("line break before '=>'");
        }
        self.expect(P::Arrow)?;
        let saved = (self.in_function, self.in_generator, self.in_async, self.strict, self.no_in);
        let pre = self.preparse_function(start);
        self.depth += 1;
        self.in_function = true;
        self.in_generator = false;
        self.in_async = is_async;
        let mut free = FreeNames::default();
        if pre {
            params.iter().for_each(|p| free.param(p));
            if let Some(r) = &rest {
                free.rest(r);
            }
        }
        let block_body = self.at(P::LBrace);
        let body = if block_body {
            self.no_in = false;
            (|| {
                self.advance()?;
                // The closing brace is consumed after restoring the flags
                if pre {
                    self.parse_body_items(Some(&mut free))?;
                    Ok(FunctionBody::Block(Vec::new()))
                } else {
                    Ok(FunctionBody::Block(self.parse_body_items(None)?))
                }
            })()
        } else {
            self.parse_assign().map(|e| {
                if pre {
                    free.expr(&e);
                    FunctionBody::Block(Vec::new())
                } else {
                    FunctionBody::Expr(Box::new(e))
                }
            })
        };
        let strict = self.strict;
        self.depth -= 1;
        (self.in_function, self.in_generator, self.in_async, self.strict, self.no_in) = saved;
        let mut body = body?;
        let (mut params, mut rest) = (params, rest);
        if pre {
            let lazy = LazyBody {
                free: free.finish(FunctionKind::Arrow),
                nparams: params.len() as u32,
                length: params.iter().take_while(|p| p.default.is_none()).count() as u32,
            };
            body = FunctionBody::Lazy(Box::new(lazy));
            params = Vec::new();
            rest = None;
        }
        if block_body {
            // Consume `}` with the outer flags restored, so the token after
            // it is lexed in the outer context
            self.expect(P::RBrace)?;
        }
        let simple_params = rest.is_none() && params.iter().all(|p| p.default.is_none() && matches!(p.target, Pattern::Ident(_)));
        Ok(Expr::Function(Box::new(Function {
            name: None,
            params,
            rest,
            body,
            kind: FunctionKind::Arrow,
            is_async,
            is_generator: false,
            strict,
            simple_params,
            span: self.span_from(start),
            params_start: start,
        })))
    }

    fn parse_yield(&mut self) -> PResult<Expr> {
        self.advance()?;
        let delegate = !self.tok.newline_before && self.eat(P::Star)?;
        let has_arg = delegate
            || !(self.tok.newline_before
                || matches!(
                    self.tok.tok,
                    Tok::Eof
                        | Tok::Punct(P::RParen)
                        | Tok::Punct(P::RBracket)
                        | Tok::Punct(P::RBrace)
                        | Tok::Punct(P::Comma)
                        | Tok::Punct(P::Semi)
                        | Tok::Punct(P::Colon)
                ));
        let arg = if has_arg { Some(Box::new(self.parse_assign()?)) } else { None };
        Ok(Expr::Yield { arg, delegate })
    }

    fn parse_conditional(&mut self) -> PResult<Expr> {
        let test = self.parse_binary(1)?;
        if !self.eat(P::Question)? {
            return Ok(test);
        }
        let saved = std::mem::replace(&mut self.no_in, false);
        let cons = self.parse_assign();
        self.no_in = saved;
        let cons = cons?;
        self.expect(P::Colon)?;
        let alt = self.parse_assign()?;
        Ok(Expr::Cond { test: Box::new(test), cons: Box::new(cons), alt: Box::new(alt) })
    }

    /// The binary operator at the current token, with its precedence
    fn binary_op(&self) -> Option<(Result<BinaryOp, LogicalOp>, u8)> {
        let op = match &self.tok.tok {
            Tok::Punct(p) => match p {
                P::QuestionQuestion => (Err(LogicalOp::Nullish), 1),
                P::PipePipe => (Err(LogicalOp::Or), 2),
                P::AmpAmp => (Err(LogicalOp::And), 3),
                P::Pipe => (Ok(BinaryOp::BitOr), 4),
                P::Caret => (Ok(BinaryOp::BitXor), 5),
                P::Amp => (Ok(BinaryOp::BitAnd), 6),
                P::EqEq => (Ok(BinaryOp::Eq), 7),
                P::Ne => (Ok(BinaryOp::Ne), 7),
                P::EqEqEq => (Ok(BinaryOp::StrictEq), 7),
                P::NeEq => (Ok(BinaryOp::StrictNe), 7),
                P::Lt => (Ok(BinaryOp::Lt), 8),
                P::Gt => (Ok(BinaryOp::Gt), 8),
                P::Le => (Ok(BinaryOp::Le), 8),
                P::Ge => (Ok(BinaryOp::Ge), 8),
                P::Shl => (Ok(BinaryOp::Shl), 9),
                P::Sar => (Ok(BinaryOp::Sar), 9),
                P::Shr => (Ok(BinaryOp::Shr), 9),
                P::Plus => (Ok(BinaryOp::Add), 10),
                P::Minus => (Ok(BinaryOp::Sub), 10),
                P::Star => (Ok(BinaryOp::Mul), 11),
                P::Slash => (Ok(BinaryOp::Div), 11),
                P::Percent => (Ok(BinaryOp::Mod), 11),
                P::StarStar => (Ok(BinaryOp::Exp), 12),
                _ => return None,
            },
            Tok::Keyword(Kw::Instanceof) => (Ok(BinaryOp::Instanceof), 8),
            Tok::Keyword(Kw::In) if !self.no_in => (Ok(BinaryOp::In), 8),
            _ => return None,
        };
        Some(op)
    }

    /// Precedence climbing over binary and logical operators: parses
    /// operators binding at least as tightly as `min_prec`
    fn parse_binary(&mut self, min_prec: u8) -> PResult<Expr> {
        let mut left = self.parse_unary()?;
        while let Some((op, prec)) = self.binary_op() {
            if prec < min_prec {
                break;
            }
            if prec == 12 && matches!(left, Expr::Unary { .. } | Expr::Await(_)) {
                return self.error("unparenthesized unary expression can't appear on the left of '**'");
            }
            self.advance()?;
            // `**` is right-associative; everything else is left-associative
            let right = self.parse_binary(if prec == 12 { 12 } else { prec + 1 })?;
            left = match op {
                Ok(op) => Expr::Binary { op, left: Box::new(left), right: Box::new(right) },
                Err(op) => {
                    let mixes = |e: &Expr, other: &[LogicalOp]| matches!(e, Expr::Logical { op, .. } if other.contains(op));
                    if op == LogicalOp::Nullish && (mixes(&left, &[LogicalOp::And, LogicalOp::Or]) || mixes(&right, &[LogicalOp::And, LogicalOp::Or])) {
                        return self.error("'??' can't be mixed with '&&' or '||' without parentheses");
                    }
                    if op != LogicalOp::Nullish && (mixes(&left, &[LogicalOp::Nullish]) || mixes(&right, &[LogicalOp::Nullish])) {
                        return self.error("'??' can't be mixed with '&&' or '||' without parentheses");
                    }
                    Expr::Logical { op, left: Box::new(left), right: Box::new(right) }
                }
            };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> PResult<Expr> {
        let op = match &self.tok.tok {
            Tok::Punct(P::Minus) => Some(UnaryOp::Neg),
            Tok::Punct(P::Plus) => Some(UnaryOp::Plus),
            Tok::Punct(P::Bang) => Some(UnaryOp::Not),
            Tok::Punct(P::Tilde) => Some(UnaryOp::BitNot),
            Tok::Keyword(Kw::Typeof) => Some(UnaryOp::Typeof),
            Tok::Keyword(Kw::Void) => Some(UnaryOp::Void),
            Tok::Keyword(Kw::Delete) => Some(UnaryOp::Delete),
            _ => None,
        };
        if let Some(op) = op {
            self.advance()?;
            let arg = self.parse_unary()?;
            if op == UnaryOp::Delete && self.strict && matches!(arg, Expr::Ident(_)) {
                return self.error("delete of an unqualified identifier in strict mode");
            }
            return Ok(Expr::Unary { op, arg: Box::new(arg) });
        }
        if self.at_kw(Kw::Await) && self.in_async {
            self.advance()?;
            self.note_await();
            let arg = self.parse_unary()?;
            return Ok(Expr::Await(Box::new(arg)));
        }
        if self.at(P::PlusPlus) || self.at(P::MinusMinus) {
            let inc = self.at(P::PlusPlus);
            self.advance()?;
            let target = self.parse_unary()?;
            self.check_update_target(&target)?;
            return Ok(Expr::Update { inc, prefix: true, target: Box::new(target) });
        }
        let expr = self.parse_lhs()?;
        if (self.at(P::PlusPlus) || self.at(P::MinusMinus)) && !self.tok.newline_before {
            let inc = self.at(P::PlusPlus);
            self.check_update_target(&expr)?;
            self.advance()?;
            return Ok(Expr::Update { inc, prefix: false, target: Box::new(expr) });
        }
        Ok(expr)
    }

    fn check_update_target(&self, expr: &Expr) -> PResult<()> {
        match expr {
            Expr::Ident(name) if self.strict && matches!(&**name, "eval" | "arguments") => {
                self.error("invalid update target in strict mode")
            }
            Expr::Ident(_) | Expr::Member { optional: false, .. } | Expr::SuperMember(_) => Ok(()),
            Expr::Paren(inner) => self.check_update_target(inner),
            _ => self.error("invalid update target"),
        }
    }

    /// Left-hand-side expression: member accesses, calls, `new`, optional
    /// chains and tagged templates
    fn parse_lhs(&mut self) -> PResult<Expr> {
        let mut expr = if self.at_kw(Kw::New) {
            self.parse_new()?
        } else if self.at_kw(Kw::Super) {
            self.parse_super()?
        } else if self.at_kw(Kw::Import) {
            self.advance()?;
            if self.eat(P::Dot)? {
                if !self.at_ident("meta") || self.module.is_none() {
                    return self.error("import.meta is only valid in modules");
                }
                self.advance()?;
                Expr::ImportMeta
            } else {
                self.expect(P::LParen)?;
                let spec = self.parse_assign()?;
                let mut options = None;
                if self.eat(P::Comma)? && !self.at(P::RParen) {
                    options = Some(Box::new(self.parse_assign()?));
                    self.eat(P::Comma)?;
                }
                self.expect(P::RParen)?;
                Expr::Import { spec: Box::new(spec), options }
            }
        } else {
            self.parse_primary()?
        };

        let mut in_chain = false;
        loop {
            match &self.tok.tok {
                Tok::Punct(P::Dot) => {
                    self.advance()?;
                    let prop = self.parse_member_name()?;
                    expr = Expr::Member { object: Box::new(expr), prop, optional: false };
                }
                Tok::Punct(P::QuestionDot) => {
                    self.advance()?;
                    in_chain = true;
                    if self.at(P::LParen) {
                        let args = self.parse_arguments()?;
                        expr = Expr::Call { callee: Box::new(expr), args, optional: true };
                    } else if self.eat(P::LBracket)? {
                        let prop = self.parse_bracket_prop()?;
                        expr = Expr::Member { object: Box::new(expr), prop, optional: true };
                    } else if matches!(self.tok.tok, Tok::Template { .. }) {
                        return self.error("tagged template in optional chain");
                    } else {
                        let prop = self.parse_member_name()?;
                        expr = Expr::Member { object: Box::new(expr), prop, optional: true };
                    }
                }
                Tok::Punct(P::LBracket) => {
                    self.advance()?;
                    let prop = self.parse_bracket_prop()?;
                    expr = Expr::Member { object: Box::new(expr), prop, optional: false };
                }
                Tok::Punct(P::LParen) => {
                    let args = self.parse_arguments()?;
                    expr = Expr::Call { callee: Box::new(expr), args, optional: false };
                }
                Tok::Template { .. } => {
                    if in_chain {
                        return self.error("tagged template in optional chain");
                    }
                    let template = self.parse_template(true)?;
                    expr = Expr::TaggedTemplate { tag: Box::new(expr), template: Rc::new(template) };
                }
                _ => break,
            }
        }
        if in_chain {
            expr = Expr::OptionalChain(Box::new(expr));
        }
        Ok(expr)
    }

    fn parse_member_name(&mut self) -> PResult<MemberProp> {
        if self.eat(P::Hash)? {
            return Ok(MemberProp::Private(self.parse_identifier_name()?));
        }
        Ok(MemberProp::Name(self.parse_identifier_name()?))
    }

    /// After `[`: expression and `]`
    fn parse_bracket_prop(&mut self) -> PResult<MemberProp> {
        let saved = std::mem::replace(&mut self.no_in, false);
        let expr = self.parse_expression();
        self.no_in = saved;
        let expr = expr?;
        self.expect(P::RBracket)?;
        Ok(MemberProp::Computed(Box::new(expr)))
    }

    fn parse_new(&mut self) -> PResult<Expr> {
        self.expect_kw(Kw::New)?;
        if self.eat(P::Dot)? {
            if !self.at_ident("target") {
                return self.unexpected("expected 'target'");
            }
            self.advance()?;
            return Ok(Expr::NewTarget);
        }
        // The callee is a member expression: no calls, no optional chains
        let mut callee = if self.at_kw(Kw::New) {
            self.parse_new()?
        } else if self.at_kw(Kw::Super) {
            self.parse_super()?
        } else {
            self.parse_primary()?
        };
        loop {
            match &self.tok.tok {
                Tok::Punct(P::Dot) => {
                    self.advance()?;
                    let prop = self.parse_member_name()?;
                    callee = Expr::Member { object: Box::new(callee), prop, optional: false };
                }
                Tok::Punct(P::LBracket) => {
                    self.advance()?;
                    let prop = self.parse_bracket_prop()?;
                    callee = Expr::Member { object: Box::new(callee), prop, optional: false };
                }
                Tok::Template { .. } => {
                    let template = self.parse_template(true)?;
                    callee = Expr::TaggedTemplate { tag: Box::new(callee), template: Rc::new(template) };
                }
                Tok::Punct(P::QuestionDot) => return self.error("optional chain in 'new' expression"),
                _ => break,
            }
        }
        let args = if self.at(P::LParen) { self.parse_arguments()? } else { Vec::new() };
        Ok(Expr::New { callee: Box::new(callee), args })
    }

    fn parse_super(&mut self) -> PResult<Expr> {
        self.expect_kw(Kw::Super)?;
        match &self.tok.tok {
            Tok::Punct(P::LParen) => Ok(Expr::SuperCall(self.parse_arguments()?)),
            Tok::Punct(P::Dot) => {
                self.advance()?;
                Ok(Expr::SuperMember(MemberProp::Name(self.parse_identifier_name()?)))
            }
            Tok::Punct(P::LBracket) => {
                self.advance()?;
                Ok(Expr::SuperMember(self.parse_bracket_prop()?))
            }
            _ => self.unexpected("'super' must be followed by a call or property access"),
        }
    }

    fn parse_arguments(&mut self) -> PResult<Vec<ArrayElem>> {
        self.expect(P::LParen)?;
        let saved = std::mem::replace(&mut self.no_in, false);
        let result = (|| {
            let mut args = Vec::new();
            while !self.at(P::RParen) {
                if self.eat(P::Ellipsis)? {
                    args.push(ArrayElem::Spread(self.parse_assign_cover()?));
                } else {
                    args.push(ArrayElem::Expr(self.parse_assign_cover()?));
                }
                if !self.eat(P::Comma)? {
                    break;
                }
            }
            self.expect(P::RParen)?;
            Ok(args)
        })();
        self.no_in = saved;
        result
    }

    fn parse_primary(&mut self) -> PResult<Expr> {
        let start = self.tok.span.start;
        match self.tok.tok.clone() {
            Tok::Num(n) => {
                self.advance()?;
                Ok(Expr::Num(n))
            }
            Tok::Str(s) => {
                self.advance()?;
                Ok(Expr::Str(s))
            }
            Tok::BigInt(text) => {
                self.advance()?;
                Ok(Expr::BigInt(text))
            }
            Tok::Template { .. } => Ok(Expr::Template(Box::new(self.parse_template(false)?))),
            Tok::Punct(P::Slash) | Tok::Punct(P::SlashEq) => {
                let token = self.tok.clone();
                self.tok = self.lexer.rescan_regex(&token)?;
                match self.advance()?.tok {
                    Tok::Regex { pattern, flags } => Ok(Expr::Regex { pattern, flags }),
                    _ => unreachable!(),
                }
            }
            Tok::Punct(P::LParen) => self.parse_paren(start),
            Tok::Punct(P::LBracket) => self.parse_array_literal(),
            Tok::Punct(P::LBrace) => self.parse_object_literal(),
            Tok::Keyword(Kw::This) => {
                self.advance()?;
                Ok(Expr::This)
            }
            Tok::Keyword(Kw::Null) => {
                self.advance()?;
                Ok(Expr::Null)
            }
            Tok::Keyword(Kw::True) => {
                self.advance()?;
                Ok(Expr::Bool(true))
            }
            Tok::Keyword(Kw::False) => {
                self.advance()?;
                Ok(Expr::Bool(false))
            }
            Tok::Keyword(Kw::Function) => Ok(Expr::Function(Box::new(self.parse_function(false, false)?))),
            Tok::Keyword(Kw::Class) => Ok(Expr::Class(Box::new(self.parse_class(false)?))),
            Tok::Ident(n) if &*n == "async" && !self.tok.escaped => {
                let next = self.peek()?;
                if next.tok == Tok::Keyword(Kw::Function) && !next.newline_before {
                    self.advance()?;
                    return Ok(Expr::Function(Box::new(self.parse_function(true, false)?)));
                }
                self.advance()?;
                Ok(Expr::Ident(Rc::from("async")))
            }
            _ => match self.ident_reference() {
                Some(name) => {
                    self.advance()?;
                    Ok(Expr::Ident(name))
                }
                None => self.unexpected("expected expression"),
            },
        }
    }

    /// `( ... )`: a parenthesized expression or arrow function parameters
    fn parse_paren(&mut self, start: u32) -> PResult<Expr> {
        let before = self.cover_inits;
        self.expect(P::LParen)?;
        if self.at_kw(Kw::Function) {
            self.eager_function = Some(self.tok.span.start);
        }
        let saved = std::mem::replace(&mut self.no_in, false);
        let result = (|| {
            let mut items = Vec::new();
            let mut rest = None;
            let mut trailing_comma = false;
            while !self.at(P::RParen) {
                if self.eat(P::Ellipsis)? {
                    rest = Some(self.parse_binding_target()?);
                    break;
                }
                items.push(self.parse_assign_cover()?);
                if !self.eat(P::Comma)? {
                    break;
                }
                trailing_comma = self.at(P::RParen);
            }
            self.expect(P::RParen)?;
            Ok((items, rest, trailing_comma))
        })();
        self.no_in = saved;
        let (mut items, rest, trailing_comma) = result?;

        if self.at(P::Arrow) && !self.tok.newline_before {
            let mut params = Vec::new();
            for item in items {
                let PatternElem { target, default } = self.expr_to_pattern_elem(item, true)?;
                params.push(Param { target, default });
            }
            self.cover_inits = before;
            return self.parse_arrow_body(params, rest, false, start);
        }
        if items.is_empty() || rest.is_some() || trailing_comma {
            return self.unexpected("expected '=>'");
        }
        let inner = if items.len() == 1 { items.pop().unwrap() } else { Expr::Seq(items) };
        Ok(Expr::Paren(Box::new(inner)))
    }

    fn parse_array_literal(&mut self) -> PResult<Expr> {
        self.expect(P::LBracket)?;
        let saved = std::mem::replace(&mut self.no_in, false);
        let result = (|| {
            let mut elems = Vec::new();
            loop {
                if self.eat(P::RBracket)? {
                    break;
                }
                if self.eat(P::Comma)? {
                    elems.push(None);
                    continue;
                }
                let elem = if self.eat(P::Ellipsis)? {
                    ArrayElem::Spread(self.parse_assign_cover()?)
                } else {
                    ArrayElem::Expr(self.parse_assign_cover()?)
                };
                elems.push(Some(elem));
                if !self.at(P::RBracket) {
                    self.expect(P::Comma)?;
                }
            }
            Ok(Expr::Array(elems))
        })();
        self.no_in = saved;
        result
    }

    fn parse_object_literal(&mut self) -> PResult<Expr> {
        self.expect(P::LBrace)?;
        let saved = std::mem::replace(&mut self.no_in, false);
        let result = (|| {
            let mut props = Vec::new();
            while !self.eat(P::RBrace)? {
                if self.eat(P::Ellipsis)? {
                    props.push(ObjProp::Spread(self.parse_assign_cover()?));
                } else {
                    let start = self.tok.span.start;
                    let shorthand = self.ident_reference();
                    let (kind, is_async, is_generator, key) = self.parse_method_head(false)?;
                    if self.at(P::LParen) {
                        let fkind = match kind {
                            MethodKind::Method => FunctionKind::Method,
                            MethodKind::Getter => FunctionKind::Getter,
                            MethodKind::Setter => FunctionKind::Setter,
                        };
                        let func = self.parse_function_rest(prop_key_name(&key), fkind, is_async, is_generator, start)?;
                        props.push(ObjProp::Method { key, kind, func: Box::new(func) });
                    } else if kind != MethodKind::Method || is_async || is_generator {
                        return self.unexpected("expected '('");
                    } else if self.eat(P::Colon)? {
                        props.push(ObjProp::KeyValue(key, self.parse_assign_cover()?));
                    } else {
                        let is_plain_name = matches!(&key, PropKey::Name(_));
                        match shorthand {
                            Some(name) if is_plain_name => {
                                if self.eat(P::Eq)? {
                                    let default = self.parse_assign()?;
                                    self.cover_inits += 1;
                                    props.push(ObjProp::CoverInit(name, default));
                                } else {
                                    props.push(ObjProp::Shorthand(name));
                                }
                            }
                            _ => return self.unexpected("expected ':'"),
                        }
                    }
                }
                if !self.at(P::RBrace) {
                    self.expect(P::Comma)?;
                }
            }
            Ok(Expr::Object(props))
        })();
        self.no_in = saved;
        result
    }

    /// Template literal starting at the current template token
    fn parse_template(&mut self, tagged: bool) -> PResult<Template> {
        let mut cooked = Vec::new();
        let mut raw = Vec::new();
        let mut exprs = Vec::new();
        loop {
            let Tok::Template { cooked: c, raw: r, tail } = self.tok.tok.clone() else {
                return self.unexpected("expected template continuation");
            };
            if c.is_none() && !tagged {
                return self.error("invalid escape sequence in template");
            }
            cooked.push(c);
            raw.push(r);
            if tail {
                self.advance()?;
                break;
            }
            self.advance()?;
            let saved = std::mem::replace(&mut self.no_in, false);
            let expr = self.parse_expression();
            self.no_in = saved;
            exprs.push(expr?);
            if !self.at(P::RBrace) {
                return self.unexpected("expected '}' in template");
            }
            let close = self.tok.clone();
            self.tok = self.lexer.rescan_template_continuation(&close)?;
        }
        Ok(Template { cooked, raw, exprs })
    }
}

/// Name of a function defined by a property with this key, where static
fn prop_key_name(key: &PropKey) -> Option<Name> {
    match key {
        PropKey::Name(units) => Some(Rc::from(String::from_utf16_lossy(units).as_str())),
        PropKey::Num(n) => Some(Rc::from(crate::number::number_to_string(*n).as_str())),
        PropKey::Private(name) => Some(Rc::from(format!("#{}", name).as_str())),
        PropKey::Computed(_) => None,
    }
}

/// Position a fresh lexer at a byte offset (for one-token lookahead)
fn lexer_seek(lexer: &mut Lexer<'_>, pos: usize) {
    lexer.seek(pos);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compact S-expression rendering of an expression, for precedence tests
    fn sexpr(e: &Expr) -> String {
        let s16 = |u: &[u16]| String::from_utf16_lossy(u);
        match e {
            Expr::Num(n) => crate::number::number_to_string(*n),
            Expr::Str(s) => format!("{:?}", s16(s)),
            Expr::Ident(n) => n.to_string(),
            Expr::Bool(b) => b.to_string(),
            Expr::Null => "null".into(),
            Expr::This => "this".into(),
            Expr::Paren(e) => sexpr(e),
            Expr::Unary { op, arg } => format!("({:?} {})", op, sexpr(arg)),
            Expr::Update { inc, prefix, target } => {
                format!("({}{} {})", if *prefix { "pre" } else { "post" }, if *inc { "++" } else { "--" }, sexpr(target))
            }
            Expr::Binary { op, left, right } => format!("({:?} {} {})", op, sexpr(left), sexpr(right)),
            Expr::Logical { op, left, right } => format!("({:?} {} {})", op, sexpr(left), sexpr(right)),
            Expr::Cond { test, cons, alt } => format!("(? {} {} {})", sexpr(test), sexpr(cons), sexpr(alt)),
            Expr::Assign { op, target, value } => format!("({:?} {} {})", op, pat(target), sexpr(value)),
            Expr::Call { callee, args, optional } => {
                format!("(call{} {}{})", if *optional { "?" } else { "" }, sexpr(callee), args.iter().map(|a| format!(" {}", elem(a))).collect::<String>())
            }
            Expr::New { callee, args } => format!("(new {}{})", sexpr(callee), args.iter().map(|a| format!(" {}", elem(a))).collect::<String>()),
            Expr::Member { object, prop, optional } => {
                let p = match prop {
                    MemberProp::Name(n) => n.to_string(),
                    MemberProp::Computed(e) => format!("[{}]", sexpr(e)),
                    MemberProp::Private(n) => format!("#{}", n),
                };
                format!("{}{}{}", sexpr(object), if *optional { "?." } else { "." }, p)
            }
            Expr::OptionalChain(e) => format!("(chain {})", sexpr(e)),
            Expr::Seq(es) => format!("(seq {})", es.iter().map(sexpr).collect::<Vec<_>>().join(" ")),
            Expr::Function(f) => format!("(fn {:?} {} params)", f.kind, f.params.len() + f.rest.is_some() as usize),
            Expr::Array(items) => format!("[{}]", items.iter().map(|i| i.as_ref().map_or("_".into(), elem)).collect::<Vec<_>>().join(" ")),
            Expr::Object(props) => format!("{{{} props}}", props.len()),
            Expr::Template(t) => format!("(tpl {} {})", t.cooked.len(), t.exprs.len()),
            Expr::TaggedTemplate { tag, .. } => format!("(tag {})", sexpr(tag)),
            Expr::Regex { pattern, flags } => format!("/{}/{}", pattern, flags),
            Expr::Yield { arg, delegate } => format!("(yield{} {})", if *delegate { "*" } else { "" }, arg.as_ref().map_or("_".into(), |a| sexpr(a))),
            Expr::Await(a) => format!("(await {})", sexpr(a)),
            other => format!("{:?}", other),
        }
    }

    fn elem(e: &ArrayElem) -> String {
        match e {
            ArrayElem::Expr(e) => sexpr(e),
            ArrayElem::Spread(e) => format!("...{}", sexpr(e)),
        }
    }

    fn pat(p: &Pattern) -> String {
        match p {
            Pattern::Ident(n) => n.to_string(),
            Pattern::Member(e) => sexpr(e),
            Pattern::Array { elems, rest } => format!(
                "[{}{}]",
                elems.iter().map(|e| e.as_ref().map_or("_".into(), |e| pat(&e.target) + if e.default.is_some() { "=" } else { "" })).collect::<Vec<_>>().join(" "),
                rest.as_ref().map_or(String::new(), |r| format!(" ...{}", pat(r)))
            ),
            Pattern::Object { props, rest } => format!(
                "{{{}{}}}",
                props.iter().map(|p| pat(&p.target) + if p.default.is_some() { "=" } else { "" }).collect::<Vec<_>>().join(" "),
                rest.as_ref().map_or(String::new(), |r| format!(" ...{}", pat(r)))
            ),
        }
    }

    fn expr(src: &str) -> String {
        let program = parse_script(src).unwrap_or_else(|e| panic!("{src}: {e}"));
        match &program.body[0] {
            Stmt::Expr(e) => sexpr(e),
            other => panic!("not an expression statement: {other:?}"),
        }
    }

    #[test]
    fn test_precedence() {
        assert_eq!(expr("a + b * c - d"), "(Sub (Add a (Mul b c)) d)");
        assert_eq!(expr("a ** b ** c"), "(Exp a (Exp b c))");
        assert_eq!(expr("(-a) ** b"), "(Exp (Neg a) b)");
        assert_eq!(expr("a || b && c | d ^ e & f"), "(Or a (And b (BitOr c (BitXor d (BitAnd e f)))))");
        assert_eq!(expr("a == b < c << d"), "(Eq a (Lt b (Shl c d)))");
        assert_eq!(expr("a ? b : c ? d : e"), "(? a b (? c d e))");
        assert_eq!(expr("a = b += c"), "(Assign a (Op(Add) b c))");
        assert_eq!(expr("a ?? b"), "(Nullish a b)");
        assert_eq!(expr("(a || b) ?? c"), "(Nullish (Or a b) c)");
        assert_eq!(expr("x in y instanceof z"), "(Instanceof (In x y) z)");
        assert_eq!(expr("typeof a.b + !c"), "(Add (Typeof a.b) (Not c))");
        assert_eq!(expr("a, b = 1, c"), "(seq a (Assign b 1) c)");
        assert_eq!(expr("i++ + ++j"), "(Add (post++ i) (pre++ j))");
    }

    #[test]
    fn test_member_call_new() {
        assert_eq!(expr("a.b.c(d)[e]"), "(call a.b.c d).[e]");
        assert_eq!(expr("new Foo(1).bar"), "(new Foo 1).bar");
        assert_eq!(expr("new new X()()"), "(new (new X))");
        assert_eq!(expr("new a.b.C"), "(new a.b.C)");
        assert_eq!(expr("f()()"), "(call (call f))");
        assert_eq!(expr("a?.b.c"), "(chain a?.b.c)");
        assert_eq!(expr("a?.(1)?.[2]"), "(chain (call? a 1)?.[2])");
        assert_eq!(expr("obj.if.class"), "obj.if.class");
        assert_eq!(expr("f(...args, 1)"), "(call f ...args 1)");
        assert_eq!(expr("tag`a${b}c`"), "(tag tag)");
        assert_eq!(expr("`a${b}c${d}e`"), "(tpl 3 2)");
        assert_eq!(expr("/re[/]x/g.test(s)"), "(call /re[/]x/g.test s)");
        assert_eq!(expr("a / b / c"), "(Div (Div a b) c)");
    }

    #[test]
    fn test_arrows_and_patterns() {
        assert_eq!(expr("x => x"), "(fn Arrow 1 params)");
        assert_eq!(expr("(a, b = 1, {c}, [d], ...e) => 0"), "(fn Arrow 5 params)");
        assert_eq!(expr("() => {}"), "(fn Arrow 0 params)");
        assert_eq!(expr("async x => x"), "(fn Arrow 1 params)");
        assert_eq!(expr("async (a, b) => a"), "(fn Arrow 2 params)");
        assert_eq!(expr("async(a, b)"), "(call async a b)");
        assert_eq!(expr("[a, , b = 1, ...c] = d"), "(Assign [a _ b= ...c] d)");
        assert_eq!(expr("({a, b: {c}, d = 1, ...e} = f)"), "(Assign {a {c} d= ...e} f)");
        assert_eq!(expr("[x.y, z[0]] = w"), "(Assign [x.y z.[0]] w)");
        assert_eq!(expr("(a) = 1"), "(Assign a 1)");
        let program = parse_script("for (const [k, v] of map) {} for (x.y in o); for (let i = 0, j; i < 1; i++) ;").unwrap();
        assert!(matches!(&program.body[0], Stmt::ForOf { head: ForHead::Decl(VarKind::Const, Pattern::Array { .. }), .. }));
        assert!(matches!(&program.body[1], Stmt::ForIn { head: ForHead::Target(Pattern::Member(_)), .. }));
        assert!(matches!(&program.body[2], Stmt::For { init: Some(ForInit::Var(VarKind::Let, _)), .. }));
    }

    #[test]
    fn test_asi() {
        let program = parse_script("let a = 1\nlet b = a\n++b\nreturn_label: while (0) break return_label\n").unwrap();
        assert_eq!(program.body.len(), 4);
        // `a \n ++b` is `a; ++b`
        let program = parse_script("a\n++b").unwrap();
        assert_eq!(program.body.len(), 2);
        // `return` followed by a newline returns undefined
        let program = parse_script("function f() { return\n1 }").unwrap();
        match &program.body[0] {
            Stmt::Function(f) => match &f.body {
                FunctionBody::Block(b) => assert!(matches!(b[0], Stmt::Return(None))),
                _ => panic!(),
            },
            _ => panic!(),
        }
        // No ASI inside an expression continued on the next line
        assert_eq!(parse_script("a = b\n(c)").unwrap().body.len(), 1);
        assert!(parse_script("a b").is_err());
    }

    #[test]
    fn test_functions_classes_and_strict() {
        let program = parse_script(r#"
            'use strict';
            function* gen(a, ...rest) { yield a; yield* rest; }
            async function af() { await x; for await (const v of s) {} }
            class A extends B {
                #p = 1;
                static s = 2;
                static { init(); }
                constructor(x) { super(x); this.#p = x; }
                get v() { return this.#p; }
                set v(x) { this.#p = x; }
                static async *m() {}
                ['comp' + 1]() {}
            }
            const o = { a, b: 1, [c]: 2, get d() { return 1 }, set d(v) {}, e() {}, async f() {}, *g() {}, ...h, 'str': 3, 4: 5 };
        "#).unwrap();
        assert!(program.strict);
        assert_eq!(program.body.len(), 5);
        match &program.body[3] {
            Stmt::Class(c) => {
                assert!(c.constructor.is_some());
                assert_eq!(c.members.len(), 7);
                assert_eq!(c.constructor.as_ref().unwrap().kind, FunctionKind::DerivedConstructor);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn test_statements() {
        let src = r#"
            var x = 1, y;
            if (x) y = 2; else { y = 3 }
            do x++; while (x < 10)
            switch (x) { case 1: case 2: y = 0; break; default: y = 1 }
            try { throw new Error('e') } catch ({message}) { } finally { }
            try {} catch {}
            outer: for (;;) { inner: for (;;) { continue outer; } }
            label: { break label; }
            ;;
            debugger;
        "#;
        let program = parse_script(src).unwrap();
        assert_eq!(program.body.len(), 11);
    }

    #[test]
    fn test_syntax_errors() {
        for src in [
            "a +", "(a, b", "-a ** 2", "a ?? b || c", "a || b ?? c", "({a = 1})", "[{a = 1}]",
            "1 = 2", "a++ = 1", "++(a + b)", "const x;", "let [a];", "return 1", "for (const x of y) const z = 1",
            "if (a) class B {}", "`${a`", "'unterminated", "/* unterminated", "x\n=> 1", "(a)\n=> 1", "new a?.b",
            "a?.b = 1", "a?.`x`", "class { }", "function () {}", "'use strict'; with (a) {}",
            "'use strict'; var eval = 1", "switch (a) { default: default: }", "try {}", "throw\n1",
        ] {
            assert!(parse_script(src).is_err(), "should fail: {src}");
        }
    }

    #[test]
    fn test_parses_real_world_code() {
        // Typical minified and modern code
        let src = r#"
            !function(e,t){"object"==typeof exports&&"undefined"!=typeof module?module.exports=t():"function"==typeof define&&define.amd?define(t):(e=e||self).lib=t()}(this,function(){"use strict";var n=function(e){return e&&e.__esModule?e:{default:e}};return{n:n,v:void 0}});
            const debounce = (fn, ms = 100) => { let t; return (...a) => { clearTimeout(t); t = setTimeout(() => fn(...a), ms); }; };
            document.querySelectorAll('.item').forEach((el, i) => el.classList.toggle('odd', i % 2 === 1));
            const { data: { items = [] } = {} } = response ?? {};
            for (let i = 0, n = items.length; i < n; i++) if (items[i]?.id === id) break;
            label: for (const k in obj) if (!Object.hasOwn(obj, k)) continue label;
            x = a ? b ? 1 : 2 : c ? 3 : 4;
            y = typeof z === 'undefined' || z === null ? void 0 : z.w;
        "#;
        let program = parse_script(src).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(program.body.len(), 8);
    }
}
