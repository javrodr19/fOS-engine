//! fOS JavaScript engine core
//!
//! Parser, bytecode compiler, register-based virtual machine with inline
//! caches, and a precise garbage collector.

pub mod ast;
pub mod builtins;
pub mod bytecode;
pub mod compiler;
pub mod gc;
pub mod lexer;
pub mod number;
pub mod object;
pub mod parser;
pub mod regex;
pub mod shape;
pub mod string;
pub mod value;
pub mod vm;

pub use value::Value;
pub use vm::{JsResult, Vm};
