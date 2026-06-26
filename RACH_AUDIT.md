# RACH_AUDIT.md

_Phase 1 recon — 2026-06-26. Read-only pass; no source changed._

## Architecture (one paragraph)

Rach is a tree-walking interpreter in the classic **lexer → parser → interpreter**
pipeline, driven by `main.rs::run_file`. `lexer::tokenize` (`src/lexer.rs`) turns source
into `Vec<Token>`, each token carrying `line` **and** `col` (`lexer.rs:69-70`).
`parser::parse` (`src/parser.rs`, recursive descent) produces a `Program` AST
(`src/ast.rs`: `imports`, `structs`, `functions`, indentation-sensitive blocks).
`interpreter::run` (`src/interpreter.rs`) walks the AST through a mutable `Ctx`
(scopes, vars, functions) starting at `main`. The standard library is a set of native
Rust modules under `src/stdlib/`, dispatched **by function name** in a single giant match
in `stdlib/mod.rs`. The REPL lives in `repl.rs`. Errors are three structs —
`LexError`, `ParseError`, `RuntimeError` — each holding `{ line, message }` (+ `code` for
runtime), rendered by `interpreter::report_pretty` (`interpreter.rs:97`) in a rustc-ish
style with a source-context window.

## Toolchain status

| Check | Result |
|-------|--------|
| `cargo build` | ✅ compiles, **18 warnings** |
| `cargo test` | ✅ but **0 tests exist** |
| `cargo clippy -- -W clippy::pedantic` | **626 warnings** |

## Weak spots (each cites file + reason)

### P0 — Correctness / never-panic violations

1. **Stack overflow (SIGABRT, rc=134) on deeply nested input.**
   `parser.rs` recursive descent (`parse_expr` and friends) and the interpreter's
   recursive `eval_expr` have **no depth guard**. Empirically: ~200 nested parens parse
   fine, **~500 aborts the process** (`set x = (((…1…)))`). Garbage/malicious input
   crashes the binary instead of emitting an error. Directly violates the "never panic on
   bad code" rule.

2. **`log` stdlib name collision — silent wrong dispatch.** `stdlib/mod.rs:262` maps
   `"log"` → `math::log`; `stdlib/mod.rs:280` maps `"log"` → `logging::log`, which is
   **unreachable** (clippy "unreachable pattern"). Calling `log(...)` always hits math;
   the logging entry point is dead. Real behavioral bug, not just a warning.

### P1 — Diagnostics quality

3. **No column / caret in any error.** Tokens carry `col` (`lexer.rs:70`) but
   `LexError`/`ParseError`/`RuntimeError` discard it — only `line` survives.
   `report_pretty` (`interpreter.rs:97-131`) prints the offending line but cannot draw a
   `^^^` span under the exact column. Falls short of the rustc-style target.

4. **Error envelope is ad-hoc.** Codes are HTTP-flavored (400/404/422/500) and the
   trailer line `// {stage} error {code} string {line}` (`interpreter.rs:130`) is cryptic.
   Consistent but not self-explanatory.

### P1 — Test coverage (everything is a gap)

5. **Zero automated tests.** `cargo test` runs 0. No coverage on any hot path:
   - lexer: tokenize, sentinel handling, number/string literals (`lexer.rs`)
   - parser: indentation blocks (`parse_block`), `parse_expr` precedence, match patterns,
     C-style for (`parser.rs`)
   - interpreter: `eval_expr`, `eval_binop` (int/float promotion, div0, bitwise),
     assignment targets, `mutate_place` (`interpreter.rs`)
   - stdlib dispatch (`stdlib/mod.rs`)
   No regression net exists for any future change.

### P2 — Readability / clippy / tech debt

6. **`pub fn parse_expr` leaks a private type.** `parser.rs:1067` is `pub` but takes
   `&mut P` where `P` is `pub(self)` — "type `P` is more private than the item" warning.

7. **Dead code carried in the AST/Ctx.** `line` fields never read in `ast.rs`
   (`ExprStmt`, `Switch`, `DoWhile`, ~13 occurrences); `Ctx.imports` never read
   (`interpreter.rs:32`); unused `use std::path::Path` (`interpreter.rs:399`); unused
   binding `line` (`interpreter.rs:616`).

8. **3× guarded `unreachable!()`** in `eval_binop` (`interpreter.rs:1269,1284,1301`).
   Currently safe (each sits behind a `matches!` filter) but fragile to future edits.

9. **626 clippy::pedantic warnings**, dominated by: inline-able `format!` args (279),
   redundant closures (60), `map().unwrap_or()` (24), lossy/wrapping `as` casts (~60
   across f64↔i64↔usize — worth a careful look in `math.rs`/`interpreter.rs` since these
   can silently corrupt numeric results), `let...else` opportunities (15), redundant
   `continue` (15).

10. **Large modules.** `parser.rs` 1504 LOC, `interpreter.rs` 1338 LOC — navigable now
    but trending toward unwieldy; several "function has too many lines" warnings.

## Proposed priority plan (Phase 2 — awaiting "go")

Ordered by the session's stated criteria (correctness → diagnostics → tests →
readability → perf):

- **P0-a** Add recursion-depth guard in parser (and interpreter eval) → return a
  `ParseError`/`RuntimeError` ("expression nested too deeply") instead of aborting.
  TDD: failing test feeds 10 000 nested parens, expects a clean error + non-134 exit.
- **P0-b** Fix the `log` dispatch collision (`stdlib/mod.rs`) — decide whether `log` =
  natural log or logging, and disambiguate the other (needs a one-line sign-off on which
  name wins, since it touches public surface).
- **P1-a** Thread `col` from tokens into `ParseError`/`LexError` and render a `^` caret in
  `report_pretty`. Runtime errors get col where an expr span is available.
- **P1-b** Stand up a test harness: `tests/` integration tests driving `engine`-level
  `tokenize`/`parse`/`run`, plus unit tests on `eval_binop`. Lock current behavior first
  (characterization tests), then TDD new fixes.
- **P1-c** Wire `thiserror` for the three error types (currently hand-rolled).
- **P2** Clear clippy: start with the numeric-cast warnings (potential correctness),
  then the mechanical `format!`/closure lints. Remove dead `line`/`imports` fields or
  start using them (the diagnostics work in P1-a may consume them).

## Status

Branch: `harden/parser-robustness`.

### Done (5 atomic commits, all tests green)

- **Library target** (`src/lib.rs`) — pipeline importable by tests/fuzz/benches; `main.rs`
  is now a thin CLI. Prerequisite for the rest of the arc.
- **P0 parser stack overflow** — `MAX_PARSE_DEPTH` (128) guard at every recursive-descent
  re-entry; ~500 nested parens now yields a clean `ParseError` instead of SIGABRT.
- **P0 runtime recursion overflow** — `MAX_CALL_DEPTH` guard + large-stack interpreter
  thread (`INTERP_STACK_SIZE` = 256 MiB); runaway recursion → catchable error, while
  bounded deep recursion (cap 2000) runs. _(decision: large-stack thread.)_
- **P0 `log` collision** — `log` = math natural log; logger moved to `log_message`; dead
  arm + unreachable-pattern warning gone. _(decision: log = natural log.)_
- **Front-end fuzzer** (`tests/fuzz.rs`) — 40k seeded random inputs through lexer→parser,
  zero panics. Self-contained (no nightly / no external crate), CI-portable.
- Tests: `tests/robustness.rs` (7), `tests/runtime.rs` (2), `tests/stdlib.rs` (1),
  `tests/fuzz.rs` (2). Build warnings 18 → 3.

### Known bug found, not yet fixed

- **Lexer rejects `\` in `examples/native.rach:18`** (`unexpected character '\'`). Graceful
  error, not a panic — but the example doesn't run. Corpus/triage candidate (escape
  handling in string interpolation). _(pre-existing, unrelated to the recursion work.)_

### Left (needs direction / external deps)

- **P1 diagnostics with spans** (Phase 5): thread token `col` into errors + caret rendering.
  Dep choice: `ariadne` vs `codespan-reporting` vs hand-rolled (current `report_pretty` is
  close). Needs network to add a crate.
- **Corpus + `insta` snapshot tests** (Phase 4): needs `insta` dep.
- **`criterion` benches + profiling** (Phase 6): needs `criterion` dep.
- **CI workflow** (Phase 8): `build`/`test`/`clippy -D warnings`/`fmt --check`. Blocked on
  clearing the ~620 remaining clippy::pedantic warnings first (mostly mechanical:
  inline `format!` args, redundant closures, lossy casts).
- **`thiserror` for error types** (P1-c): needs network.

### Next step

Proceed to **P1 span diagnostics** (highest remaining priority) — recommend hand-rolling
the caret into the existing `report_pretty` to avoid a new dependency, since it already does
the source-window rendering. Confirm dependency policy (is adding crates from crates.io OK
in this environment?) before the `insta`/`criterion`/`thiserror`/`ariadne` phases.
