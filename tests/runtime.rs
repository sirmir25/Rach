//! Runtime robustness: the interpreter must turn runaway recursion into a catchable error
//! rather than overflowing the native stack and aborting the process.

use rach::{interpreter, lexer, parser};

/// Run a program end-to-end on an 8 MiB worker thread (matching the CLI's main-thread stack),
/// returning the runtime result. A stack overflow here would abort the process, failing the
/// test by construction.
fn run(src: &str) -> Result<(), (i64, String)> {
    let owned = src.to_string();
    std::thread::Builder::new()
        .stack_size(rach::INTERP_STACK_SIZE)
        .spawn(move || {
            let tokens = lexer::tokenize(&owned).map_err(|e| (e.line as i64, e.message))?;
            let program = parser::parse(tokens).map_err(|e| (e.line as i64, e.message))?;
            interpreter::run(&program, &owned, "<test>").map_err(|e| (e.code, e.message))
        })
        .expect("spawn interpreter thread")
        .join()
        .expect("interpreter thread must not panic or overflow")
}

#[test]
fn infinite_recursion_errors_not_overflow() {
    let src = "rach boom(0)\n    boom()\nreturn(end)\n(end0)\n\nrach main(0)\n    boom()\nreturn(end)\n(end0)\n";
    let (_code, msg) = run(src).expect_err("infinite recursion must surface a runtime error");
    assert!(msg.contains("call stack"), "expected a call-stack diagnostic, got: {msg}");
}

#[test]
fn bounded_recursion_still_runs() {
    let src = "rach fact(n)\n    if n <= 1:\n        return 1\n    return n * fact(n - 1)\nreturn(end)\n(end0)\n\nrach main(0)\n    set _ = fact(10)\nreturn(end)\n(end0)\n";
    run(src).expect("a bounded recursive program must complete");
}

/// `-i64::MIN` has no `i64` representation, so a plain `-n` would abort the process
/// (checked negation panics in a debug build) instead of surfacing as a Rach-level value.
/// Found by fuzzing arithmetic that lands exactly on `i64::MIN` via a large multiply.
#[test]
fn negating_i64_min_does_not_panic() {
    let src = "x = 100000000000000 * -100000000000000\ny = -x\n";
    run(src).expect("negating i64::MIN must promote to float, not panic");
}

/// Without `RACH_STRICT` a failing command is printed and skipped, but the run as a whole
/// must still fail — otherwise CI and cron see a broken script as a success.
#[test]
fn skipped_command_failure_fails_the_run() {
    let (code, msg) = run("read(\"/nonexistent/rach-test\")\nx = 1\n").expect_err("a skipped failure must fail the run");
    assert_eq!(code, 1);
    assert!(msg.contains("1 command(s) failed"), "got: {msg}");
}

#[test]
fn caught_failure_does_not_fail_the_run() {
    run("try:\n    read(\"/nonexistent/rach-test\")\nrescue:\n    x = 1\n").expect("a rescued failure is handled");
}
