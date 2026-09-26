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

### Fixed after integration (branch `fix/integrate`)

- **`examples/native.rach` lex error** — fixed by merging `feat/binary-installers`
  (interpolation is opt-in via `f"…"`; plain strings are literal). Regression tests in
  `tests/strings.rs`.
- **WebDriver never started with geckodriver ≥0.34** — `Host` header lacked the port, so
  every session failed with `Invalid Host header` and fell back to the OS browser.
- Examples: DuckDuckGo search field is now `name="q"`; `login.rach` opens `/login`.
- Build warnings 3 → 0.

### Done on `feat/diagnostics`

- **P1 span diagnostics** — `LexError`/`ParseError` carry `col`; `report_pretty` prints
  `file:line:col` and a `^` caret (tab-aware). Tests: `tests/diagnostics.rs`.
- **Clippy (default lints)** — clean under `-D warnings`; dead `resolve_call` removed.
- **CI** — `.github/workflows/ci.yml`: build/test/clippy on Linux/macOS/Windows, plus the
  offline examples under `RACH_STRICT=1`. `cargo fmt --check` is deliberately not gated
  (the codebase uses a compact one-line style that rustfmt would rewrite wholesale).

### Done on `feat/diagnostics` (continued — network available, deps added)

- **`thiserror` for error types** (P1-c) — `LexError`/`ParseError`/`RuntimeError` derive
  `thiserror::Error` (`#[error("... at {line}:{col}: {message}")]` /
  `#[error("runtime error {code} at line {line}: {message}")]`); they're now real
  `std::error::Error` types usable with `?` and trait objects by anyone embedding `rach` as a
  library, on top of the hand-rolled `report_pretty` rendering (unaffected — it doesn't use
  `Display`).
- **`insta` snapshot tests** (Phase 4) — `render_pretty` (the formatting half of
  `report_pretty`, split out so it returns a `String` instead of only `eprintln!`-ing) is
  snapshot-tested in `tests/diagnostics_snapshot.rs`: a lex error with a caret, a multi-line
  parse error, and a line-less runtime error. Locks the exact rustc-style rendering — header,
  source window, caret column — against silent drift.
- **`criterion` benches** (Phase 6) — `benches/pipeline.rs` (`cargo bench`): lex+parse
  throughput on a small script, `fib(18)` (recursive user-function call overhead), and a
  `struct`/`impl`-method loop (method dispatch + `self`-mutation write-back cost). Not wired
  into CI (benches are slow and noisy on shared runners); run locally when tuning hot paths.
- **OOP: structs + methods** — `impl StructName: rach method(self, ...): ... end` attaches
  methods to a `struct`. First param must be named `self`; the interpreter binds the receiver
  to it and, after the call, writes any `self.field = ...` mutation back to the call site when
  it's an assignable place (a variable, or a field/index chain reachable from one) — a
  temporary's mutation is simply discarded, matching pass-by-value semantics. Supports default
  params like regular functions; dispatch is by struct name, so two structs may each define a
  same-named method. New statement-level grammar: `p.method(args)` as a bare statement (was
  previously only parseable as an expression via `set x = p.method(args)`). Tests:
  `tests/oop.rs` (5). Docs: README "Structs and methods", REFERENCE.md §2.4 + formal grammar
  §8. Example: `examples/oop.rach` (wired into CI's offline-examples list).

### Done — follow-up pass (edge-case tests, interpreter fuzzing, clippy::pedantic)

- **OOP edge-case coverage**: `tests/oop.rs` grew from 5 to 9 tests — a method calling another
  method on `self` (from within the same `impl`), `self`-mutation write-back through a *nested*
  struct field (`line.a.move(...)`, where `a` is itself a struct), two structs defining a
  same-named method without colliding, and calling a method on a non-struct value failing
  cleanly. `examples/oop_composition.rach` demonstrates all of it; wired into CI.
- **Interpreter-level fuzzing** (previously flagged as needing a generator): `tests/fuzz_interp.rs`
  generates programs from a fixed, always-parseable skeleton with randomized expression holes
  (literals, indices, field/method calls, arithmetic) instead of random text, so it actually
  reaches `eval_expr`/`eval_binary`/stdlib dispatch — `tests/fuzz.rs`'s random text almost never
  gets past the parser. **Found and fixed three real crash bugs on the first pass**: `-i64::MIN`
  and `abs(i64::MIN)` both panicked (checked negation/abs has no positive counterpart for that
  value — reachable from ordinary `x * x` on large ints, since `eval_binary`'s `f64` round trip
  saturates to exactly `i64::MIN`); both fixed by promoting to float on overflow. `for i in <huge
  non-negative int>:` built an N-element `Vec<Value>` eagerly, so `for i in 999999999999:` OOM-
  aborted the process (not even catchable by `try/rescue`) before running a single iteration;
  fixed by iterating a lazy `Range` instead. Regression test in `tests/runtime.rs` for the first;
  the fuzzer itself exercises all three every run.
- **`clippy::pedantic`**: `cargo clippy --fix -- -W clippy::pedantic` plus a hand-verified batch
  of the `let...else`/`format_push_string`/`assigning_clones` suggestions clippy marks
  correct-but-not-machine-applicable (had to hand-fix one `--fix` output that used an ugly
  fully-qualified path, and hand-complete one `unnecessary_wraps` suggestion whose two-part fix
  — signature *and* body — I'd applied only half of, plus two `write!` call sites that needed a
  `use std::fmt::Write` clippy's suggestion didn't add). Net: ~1500 → ~600 occurrences. What's
  left is a deliberate, documented `#![allow(...)]` in `src/lib.rs`/`src/main.rs` — see the
  comment there for why each category (missing_errors_doc, the `cast_*` family,
  needless_continue, too_many_lines, and a few judgement-call lints) isn't worth chasing further
  in this codebase. Default `cargo clippy -- -D warnings` (the CI gate) was clean before and
  after; build/tests unaffected throughout.

### Left (needs direction / external deps)

- **CI workflow** (Phase 8) fmt gate: `cargo fmt --check` is still deliberately not gated (see
  the CI section above — the codebase's compact one-line style predates this session and
  rustfmt would rewrite it wholesale). No change here.
- **Corpus fuzzing beyond well-formed programs**: `tests/fuzz_interp.rs` generates programs from
  one fixed skeleton with randomized leaves — good at finding numeric/dispatch edge cases, but
  it never varies control-flow *shape* (no nested user-defined blocks, no varying struct/impl
  counts). A true random-AST generator would cover more ground; not attempted this pass.

### Next step

The audit's outstanding P1/P2 items (diagnostics spans, tests, thiserror, insta, criterion,
interpreter fuzzing, OOP edge cases, clippy::pedantic) are now all closed. Candidates for a
future pass: extend OOP with a second struct-like construct if a concrete need shows up (kept
deliberately minimal — no inheritance/traits — since nothing in the existing stdlib or examples
asked for one yet); a random-AST-shaped fuzzer (see above); and the small `cast_possible_wrap`/
`cast_possible_truncation` residue left in `tests/`/`benches/` (not part of the lib/bin crates,
so the `#![allow]` doesn't reach them — harmless either way since they're pedantic-only).
