//! Rach language crate.
//!
//! Exposes the compiler pipeline (`lexer → parser → interpreter`) as a library so
//! integration tests, fuzz targets, and benchmarks can drive it directly. The `rach`
//! binary (`src/main.rs`) is a thin CLI wrapper over these modules.

pub mod ast;
pub mod interpreter;
pub mod lexer;
pub mod parser;
pub mod repl;
pub mod stdlib;
