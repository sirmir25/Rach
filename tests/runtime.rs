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
