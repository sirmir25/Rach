//! Self-contained fuzzing of the front end. Generates a large number of pseudo-random inputs
//! — biased toward Rach's own token alphabet so the parser is actually exercised — and asserts
//! the `lexer → parser` pipeline never panics. No external fuzzing crate is required, so this
//! runs unchanged in CI on stable Rust.
//!
//! The recursion guards in lexer/parser mean over-nesting yields an `Err`, not a stack
//! overflow, so every input must return `Ok`/`Err` without unwinding. Any panic fails the test
//! and prints the offending input for triage.

use rach::{lexer, parser};
use std::panic::{catch_unwind, AssertUnwindSafe};

/// Tiny deterministic PRNG (xorshift64*). Deterministic so a failure is always reproducible
/// from the seed printed in the assertion message.
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
}

/// Fragments biased toward real Rach syntax so generated programs reach deep into the parser
/// rather than dying at the first byte.
const FRAGMENTS: &[&str] = &[
    "rach ", "main", "(", ")", "0", ":", "\n", "  ", "set ", "x", " = ", "+", "-", "*", "/",
    "[", "]", "{", "}", ",", ".", "..", "..=", "if ", "else", "for ", "in ", "while ", "match ",
    "return", "end", "(end0)", "true", "false", "\"", "'", "\\", "?", "~", "&", "|", "^", "%",
    "not ", "and ", "or ", "import ", "struct ", "=>", "->", "::", "123", "9999999999999999999",
    "💥", " \u{0} ", "#", "_", " ", "\t", "<", ">", "==", "<=", ">=", "!=",
];

fn fuzz_one(src: &str) {
    let owned = src.to_string();
    let result = catch_unwind(AssertUnwindSafe(|| {
        if let Ok(tokens) = lexer::tokenize(&owned) {
            let _ = parser::parse(tokens);
        }
    }));
    assert!(
        result.is_ok(),
        "front end panicked on input ({} bytes): {:?}",
        owned.len(),
        owned
    );
}

#[test]
fn fuzz_random_fragment_programs() {
    // 8 MiB stack so the recursion guards fire before any genuine native overflow.
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
            for _ in 0..20_000 {
                let pieces = 1 + rng.below(40);
                let mut src = String::new();
                for _ in 0..pieces {
                    src.push_str(FRAGMENTS[rng.below(FRAGMENTS.len())]);
                }
                fuzz_one(&src);
            }
        })
        .expect("spawn fuzz thread")
        .join()
        .expect("fuzz thread must not panic");
}

#[test]
fn fuzz_random_raw_bytes() {
    // Arbitrary (valid-UTF-8) byte soup: no structure at all, just must-not-panic.
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let mut rng = Rng(0xDEAD_BEEF_CAFE_F00D);
            for _ in 0..20_000 {
                let len = rng.below(64);
                let mut src = String::new();
                for _ in 0..len {
                    // Printable ASCII plus the occasional control/unicode char.
                    let c = match rng.below(10) {
                        0 => char::from(rng.below(0x20) as u8),
                        1 => char::from_u32(0x80 + rng.below(0x3000) as u32).unwrap_or('?'),
                        _ => char::from(0x20 + rng.below(0x5f) as u8),
                    };
                    src.push(c);
                }
                fuzz_one(&src);
            }
        })
        .expect("spawn fuzz thread")
        .join()
        .expect("fuzz thread must not panic");
}
