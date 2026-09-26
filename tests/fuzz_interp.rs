//! Fuzzes the *interpreter*, not just the front end. `tests/fuzz.rs` throws random text at
//! `lexer -> parser` and almost never reaches `eval` — the parser is strict, so pure token
//! soup dies before it gets there. This generates programs from a fixed, always-parseable
//! skeleton with randomized expression holes (literals, variables, indices, field/method
//! calls, arithmetic), so every run actually exercises `eval_expr`/`eval_binary`/stdlib
//! dispatch — including plenty of runtime-error paths (undefined vars, bad indices, div by
//! zero, wrong arg counts) — without ever failing to parse.
//!
//! Motivating catches, all found by an early version of this generator and fixed alongside it:
//! - `-i64::MIN` and `abs(i64::MIN)` both panicked (checked negation/abs has no positive
//!   counterpart for that one value) — see `tests/runtime.rs::negating_i64_min_does_not_panic`.
//!   This generator deliberately produces huge/negative int literals, and products of two such
//!   values, to keep landing on exactly that value.
//! - `for i in <huge non-negative int>:` built an N-element `Vec<Value>` eagerly, so a plain
//!   `for i in 999999999999:` aborted the process on the allocation (an OOM abort isn't even a
//!   catchable panic) before running a single iteration — fixed by iterating a lazy `Range`
//!   instead. `gen_iter_expr` below deliberately keeps its own counts small so *this test*
//!   doesn't just trade a crash for a multi-hour hang.

use rach::{interpreter, lexer, parser};
use std::panic::{catch_unwind, AssertUnwindSafe};

/// Tiny deterministic PRNG (xorshift64*), matching `tests/fuzz.rs` — deterministic so a
/// failure is always reproducible from the seed printed in the assertion message. Duplicated
/// rather than shared: each fuzz test is meant to stand alone with no test-support crate.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
    fn bool(&mut self) -> bool {
        self.below(2) == 0
    }
    /// Skewed toward small values most of the time, but occasionally huge or negative —
    /// exactly the range that overflow/panic bugs hide in.
    fn int_literal(&mut self) -> i64 {
        match self.below(6) {
            0 => 0,
            1 => -(self.below(1_000_000) as i64),
            2 => self.below(1_000_000) as i64,
            3 => i64::MAX,
            4 => i64::MIN,
            _ => {
                // Large-but-plausible values whose product with another such value is
                // exactly what drove `x * x` to saturate to `i64::MIN` in the motivating bug.
                let mag = 100_000_000_000_i64 + self.below(1_000_000_000) as i64;
                if self.bool() { mag } else { -mag }
            }
        }
    }
}

const VARS: &[&str] = &["x", "y", "s", "i", "err", "undefined_var", "z", "self"];
const BINOPS: &[&str] = &["+", "-", "*", "/", "%", "^", "==", "!=", "<", ">", "<=", ">=", "and", "or"];
const FIELDS: &[&str] = &["a", "b", "x", "dist", "nope", "m"];
const FUNCS: &[&str] = &["len", "sqrt", "upper", "abs", "sorted", "not_a_real_function", "print"];

fn gen_expr(rng: &mut Rng, depth: u32) -> String {
    if depth == 0 || rng.below(3) == 0 {
        return match rng.below(8) {
            0 => rng.int_literal().to_string(),
            1 => format!("{:.3}", (rng.below(2_000_000) as f64 / 7.0) - 100_000.0),
            2 => "\"fuzz\"".to_string(),
            3 => if rng.bool() { "true".into() } else { "false".into() },
            4 => "nil".to_string(),
            5 => "\"\"".to_string(),
            // A magnitude whose square (or product with another such value) overflows
            // `i64` and saturates when `eval_binary` casts the `f64` result back — the
            // exact shape that made `-i64::MIN` reachable in the motivating bug. Weighted
            // in deliberately, since two independent random large ints landing in the
            // same expression is otherwise a low-probability coincidence.
            6 => {
                let a = 100_000_000_000_i64 + rng.below(900_000_000_000) as i64;
                let b = if rng.bool() { a } else { -a };
                format!("({a} * {b})")
            }
            _ => VARS[rng.below(VARS.len())].to_string(),
        };
    }
    let d = depth - 1;
    match rng.below(8) {
        0 => format!("({} {} {})", gen_expr(rng, d), BINOPS[rng.below(BINOPS.len())], gen_expr(rng, d)),
        1 => format!("-({})", gen_expr(rng, d)),
        2 => format!("not ({})", gen_expr(rng, d)),
        3 => format!("[{}, {}, {}]", gen_expr(rng, d), gen_expr(rng, d), gen_expr(rng, d)),
        4 => format!("({})[{}]", gen_expr(rng, d), gen_expr(rng, d)),
        5 => format!("({}).{}", gen_expr(rng, d), FIELDS[rng.below(FIELDS.len())]),
        6 => format!("({}).{}({})", gen_expr(rng, d), FIELDS[rng.below(FIELDS.len())], gen_expr(rng, d)),
        _ => format!("{}({}, {})", FUNCS[rng.below(FUNCS.len())], gen_expr(rng, d), gen_expr(rng, d)),
    }
}

/// The `for` header specifically needs its own, bounded leaf set: `gen_expr`'s huge int
/// magnitudes are exactly what a real bug (found by this fuzzer) turned into an instant
/// OOM abort via `for i in <huge int>:` eagerly building an N-element `Vec`. That's now
/// fixed (the interpreter iterates a `Range` lazily instead), so a giant count here is no
/// longer a crash — but it would still spin the interpreter for a very long time, which
/// would make this test hang rather than fail fast. Keep the iterable itself small.
fn gen_iter_expr(rng: &mut Rng) -> String {
    match rng.below(4) {
        0 => rng.below(6).to_string(),
        1 => "[1, 2, 3]".to_string(),
        2 => VARS[rng.below(VARS.len())].to_string(),
        _ => "\"a,b,c\"".to_string(),
    }
}

/// A single always-parseable program: a struct with one method, then top-level statements
/// (assignment, list/map/struct literals, `if`, `for`, `try/rescue`) whose expression
/// positions are filled by `gen_expr`. The skeleton's shape never changes — only the leaves —
/// so every generated program reaches `interpreter::run` instead of dying at parse time.
fn gen_program(rng: &mut Rng) -> String {
    let e: Vec<String> = (0..6).map(|_| gen_expr(rng, 3)).collect();
    let cond = gen_expr(rng, 2);
    let iter = gen_iter_expr(rng);
    format!(
        "struct S {{ a, b }}\n\
         impl S:\n    \
             rach m(self, n):\n        \
                 return self.a + n\n    \
             end\n\
         end\n\
         x = {e0}\n\
         y = {e1}\n\
         s = S {{ a: {e2}, b: {e3} }}\n\
         lst = [{e4}, {e5}]\n\
         mp = {{\"k\": {e0}}}\n\
         if {cond}:\n    \
             z = {e1}\n\
         for i in {iter}:\n    \
             w = {e2}\n\
         try:\n    \
             v = {e3}\n\
         rescue as err:\n    \
             u = {e4}\n\
         print(x)\n",
        e0 = e[0], e1 = e[1], e2 = e[2], e3 = e[3], e4 = e[4], e5 = e[5],
        cond = cond, iter = iter,
    )
}

/// Runs one generated program end-to-end on a large-stack thread (matching the CLI's), and
/// fails only on a panic. Any `Err` — lex, parse (shouldn't happen, but no guarantee is
/// absolute), or runtime — is a normal, expected outcome for randomized input.
fn fuzz_one(src: String) {
    let result = catch_unwind(AssertUnwindSafe(|| {
        if let Ok(tokens) = lexer::tokenize(&src) {
            if let Ok(program) = parser::parse(tokens) {
                let _ = interpreter::run(&program, &src, "<fuzz>");
            }
        }
    }));
    assert!(result.is_ok(), "interpreter panicked on generated program:\n{src}");
}

#[test]
fn fuzz_random_but_well_formed_programs() {
    std::thread::Builder::new()
        .stack_size(rach::INTERP_STACK_SIZE)
        .spawn(|| {
            let mut rng = Rng(0xABCD_EF01_2345_6789);
            for _ in 0..8_000 {
                let src = gen_program(&mut rng);
                fuzz_one(src);
            }
        })
        .expect("spawn fuzz thread")
        .join()
        .expect("fuzz thread must not panic");
}
