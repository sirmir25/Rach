//! Robustness tests: the front end must never panic or overflow the stack on adversarial
//! input. Every case here feeds malformed or pathological source through the real
//! `lexer → parser` pipeline and asserts a clean `Result`, never a crash.

use rach::{lexer, parser};

/// Parse source the way the CLI does. Returns `Ok` on a valid program and `Err` on any
/// lex/parse error. A panic or stack overflow here is a test failure by definition.
///
/// Runs on a worker thread with an 8 MiB stack — the same size as the process main thread
/// the CLI uses — so the depth-guard diagnostic fires before the native stack is exhausted,
/// independent of the (smaller) default stack `cargo test` gives its harness threads.
fn try_parse(src: &str) -> Result<(), String> {
    let owned = src.to_string();
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(move || {
            let tokens = lexer::tokenize(&owned).map_err(|e| e.message)?;
            parser::parse(tokens).map(|_| ()).map_err(|e| e.message)
        })
        .expect("spawn parser thread")
        .join()
        .expect("parser thread must not panic or overflow")
}

#[test]
fn deeply_nested_parens_error_not_overflow() {
    let src = format!("rach main():\n  set x = {}1{}\n", "(".repeat(5000), ")".repeat(5000));
    let err = try_parse(&src).expect_err("deep nesting must be rejected, not parsed");
    assert!(err.contains("deep"), "expected a depth diagnostic, got: {err}");
}

#[test]
fn deeply_nested_blocks_error_not_overflow() {
    // 2000 nested `if` blocks, each indented one space deeper.
    let mut src = String::from("rach main():\n");
    for i in 0..2000 {
        src.push_str(&" ".repeat(i + 2));
        src.push_str("if true:\n");
    }
    src.push_str(&" ".repeat(2002));
    src.push_str("echo(1)\n");
    // Must terminate with a diagnostic rather than overflowing the stack.
    let _ = try_parse(&src);
}

#[test]
fn long_unary_chain_does_not_overflow() {
    let src = format!("rach main():\n  set x = {}1\n", "-".repeat(20000));
    let _ = try_parse(&src);
}

#[test]
fn long_power_chain_does_not_overflow() {
    let src = format!("rach main():\n  set x = {}2\n", "2^".repeat(20000));
    let _ = try_parse(&src);
}

#[test]
fn long_not_chain_does_not_overflow() {
    let src = format!("rach main():\n  set x = {}true\n", "not ".repeat(20000));
    let _ = try_parse(&src);
}

#[test]
fn garbage_inputs_never_panic() {
    let cases = [
        "",
        "((((((",
        "))))))",
        "rach main():",
        "rach main():\n  set x =",
        "rach main():\n  set x = [1, 2, 3",
        "rach main():\n  if :\n echo(1)",
        "\"unterminated string",
        "rach main():\n  set m = {a:}",
        "match",
        "rach\0main():\n  echo(1)",
        "💥🔥 рандом юникод",
        "rach main():\n  set x = 999999999999999999999999999999\n",
    ];
    for case in cases {
        // The contract is "no panic"; either Ok or Err is acceptable.
        let _ = try_parse(case);
    }
}

#[test]
fn valid_program_still_parses() {
    let src = "rach main():\n  set x = (1 + 2) * 3\n  print(x)\nend\n";
    try_parse(src).expect("a well-formed program must parse");
}
