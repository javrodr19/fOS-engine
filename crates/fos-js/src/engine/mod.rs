//! Custom JavaScript Engine
//!
//! A complete JavaScript engine implementation for fOS browser.
//!
//! ## Core Components
//! - Lexer: ES2023 tokenization
//! - Parser: AST generation
//! - Compiler: Bytecode compilation
//! - VM: Stack-based execution
//! - GC: Garbage collection
//!
//! ## ES6+ Features
//! - Promise: Async/await
//! - Symbol: Symbol primitive
//! - Collections: Map/Set/WeakMap/WeakSet
//! - TypedArrays: ArrayBuffer/DataView
//! - Proxy/Reflect: Meta-programming
//! - Regex: Pattern matching
//! - JSON: Parse/stringify
//! - Date: Date object

// Core
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

// Optimization
pub mod inline_cache;
pub mod jit;

// DOM
pub mod dom_bindings;

// ES6+ Features
pub mod promise;
pub mod regex;
pub mod json;
pub mod symbol;
pub mod collections;
pub mod date;
pub mod typed_array;
pub mod event_loop;
pub mod proxy;

// Core exports
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

// Optimization exports
pub use inline_cache::{InlineCacheManager, ShapeRegistry, ShapeId};
pub use jit::{BaselineJit, JitTier, JitStats};

// DOM exports
pub use dom_bindings::{DomDocument, DomElement, NodeType};

// ES6+ exports
pub use promise::{JsPromise, PromiseState, AsyncState};
pub use regex::{JsRegex, RegexFlags, RegexMatch};
pub use json::{parse as json_parse, stringify as json_stringify};
pub use symbol::{JsSymbol, SymbolRegistry, WellKnownSymbols};
pub use collections::{JsMap, JsSet, JsWeakMap, JsWeakSet};
pub use date::JsDate;
pub use typed_array::{ArrayBuffer, DataView, TypedArray, TypedArrayKind};
pub use event_loop::EventLoop;
pub use proxy::{JsProxy, ProxyHandler, Reflect};





