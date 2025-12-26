//! Custom JavaScript Engine
//!
//! A from-scratch JavaScript engine implementation for fOS browser.
//!
//! Components:
//! - Lexer: Tokenizes JavaScript source code (ES2023)
//! - Parser: Builds AST from tokens
//! - Compiler: Generates bytecode from AST
//! - VM: Executes bytecode
//! - GC: Manages memory for JS objects
//! - Builtins: Standard library objects
//! - CustomEngine: JsEngine trait implementation
//! - Integration: fos-engine utilities (StringInterner, Arena, Fixed16, CoW)
//! - DOM Bindings: JavaScript DOM API bindings
//! - Inline Cache: Property access optimization
//! - JIT: Baseline Just-In-Time compiler

pub mod token;
pub mod lexer;
pub mod ast;
pub mod parser;
pub mod bytecode;
pub mod compiler;
pub mod vm;
pub mod gc;
pub mod value;
pub mod object;
pub mod builtins;
pub mod custom_engine;
pub mod integration;
pub mod dom_bindings;
pub mod inline_cache;
pub mod jit;

pub use token::{Token, TokenKind, Span};
pub use lexer::Lexer;
pub use ast::{Ast, AstNode, AstNodeKind};
pub use parser::Parser;
pub use bytecode::{Bytecode, Opcode};
pub use compiler::Compiler;
pub use vm::VirtualMachine;
pub use gc::GarbageCollector;
pub use value::JsVal;
pub use object::JsObject;
pub use custom_engine::{CustomEngine, CustomContext};
pub use integration::{JsInterner, JsFixed, StringInterner, InternedString, Fixed16, Cow};
pub use dom_bindings::{DomDocument, DomElement, NodeType};
pub use inline_cache::{InlineCacheManager, ShapeRegistry, ShapeId};
pub use jit::{BaselineJit, JitTier, JitStats};




