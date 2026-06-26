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

/// Stack size for the thread that runs the interpreter. Rach is a tree-walking interpreter,
/// so each user-function frame costs a large slice of native stack; a generous reserve lets
/// genuinely deep (but bounded) recursion run while [`interpreter`]'s call-depth guard still
/// catches true runaway recursion. Threads reserve address space lazily, so this is cheap.
pub const INTERP_STACK_SIZE: usize = 256 * 1024 * 1024;

