//! Rach language crate.
//!
//! Exposes the compiler pipeline (`lexer → parser → interpreter`) as a library so
//! integration tests, fuzz targets, and benchmarks can drive it directly. The `rach`
//! binary (`src/main.rs`) is a thin CLI wrapper over these modules.

// `clippy::pedantic` is *checked* (see RACH_AUDIT.md) but not gated in CI, and most of it
// has already been applied — see the "style(clippy)" commits. What's left is deliberately
// left on:
//   - missing_errors_doc / missing_panics_doc: this crate isn't a published API surface;
//     hundreds of internal `Result`-returning functions would each need a boilerplate
//     "# Errors" section that restates the return type, not real documentation.
//   - cast_possible_truncation / cast_possible_wrap / cast_sign_loss / cast_precision_loss:
//     the interpreter's numeric model deliberately coerces between int/float/usize with
//     saturating/truncating `as` casts (see `eval_binary`, `eval_unary`'s `checked_neg`
//     fallback, `math::abs`) — that's the coercion mechanism, not an oversight, and blanket
//     `try_from`-ing every site would trade this lint for real risk of new panics.
//   - needless_continue: the hand-written recursive-descent parser uses an explicit
//     `continue`/`break` at the end of loop-body match arms throughout, even where it's a
//     no-op, because it makes the loop's control flow legible at a glance. Fighting that
//     idiom at 30 call sites for no behavior change isn't worth the review risk.
//   - too_many_lines: `parser.rs`/`interpreter.rs` are large; splitting them is a deliberate
//     refactor (tracked in RACH_AUDIT.md), not a mechanical lint fix.
//   - match_same_arms / case_sensitive_file_extension_comparisons / many_single_char_names /
//     needless_pass_by_value / used_underscore_binding / float_cmp: each remaining hit is a
//     judgement call (e.g. `result == result.trunc()` in `eval_binary` *means* exact
//     integer-valued float, not "close enough") rather than a mechanical rewrite.
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::needless_continue,
    clippy::too_many_lines,
    clippy::match_same_arms,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::many_single_char_names,
    clippy::needless_pass_by_value,
    clippy::used_underscore_binding,
    clippy::float_cmp
)]

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

