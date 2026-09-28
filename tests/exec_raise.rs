//! `exec` returns a command's outcome instead of failing on a non-zero exit; `raise` lets
//! scripts throw their own catchable errors.

use rach::interpreter::{self, make_ctx};
use rach::{lexer, parser};

/// Without `RACH_STRICT`, a failing top-level command is printed and skipped; `strict`
/// makes it surface here so the test can inspect it.
fn run_with(src: &str, strict: bool) -> Result<(), (i64, String)> {
    let program = parser::parse(lexer::tokenize(src).expect("lex")).expect("parse");
    let mut ctx = make_ctx(strict, src.to_string(), "<test>".into());
    interpreter::run_in_ctx(&program, &mut ctx).map_err(|e| (e.code, e.message))
}

fn run(src: &str) -> Result<(), (i64, String)> {
    run_with(src, true)
}

#[cfg(unix)]
#[test]
fn exec_reports_code_and_streams_without_failing() {
    run("r = exec(\"echo out; echo err >&2; exit 3\", quiet=true)\n\
         assert(r.code == 3)\n\
         assert(not r.ok)\n\
         assert(r.stdout == \"out\\n\")\n\
         assert(r.stderr == \"err\\n\")\n")
        .expect("a non-zero exit must come back as data, not an error");
}

#[cfg(unix)]
#[test]
fn exec_passes_input_env_and_cwd() {
    run("r = exec(\"cat; echo $RACH_T; pwd\", input=\"in\\n\", env={\"RACH_T\": \"v\"}, cwd=\"/\", quiet=true)\n\
         assert(r.ok)\n\
         assert(r.stdout == \"in\\nv\\n/\\n\")\n")
        .expect("input, env and cwd must reach the child");
}

#[cfg(unix)]
#[test]
fn exec_check_fails_fast() {
    let (code, msg) = run("exec(\"exit 2\", check=true, quiet=true)\n").expect_err("check=true must fail");
    assert_eq!(code, 402);
    assert!(msg.contains("exited with 2"), "got: {msg}");
}

#[test]
fn exec_rejects_non_map_env() {
    let (_, msg) = run("exec(\"true\", env=\"X=1\", quiet=true)\n").expect_err("env must be a map");
    assert!(msg.contains("must be a map"), "got: {msg}");
}

#[test]
fn raise_is_caught_by_rescue() {
    run("try:\n    raise(\"boom\", code=42)\nrescue as e:\n    assert(e.code == 42)\n    assert(e.message == \"boom\")\n")
        .expect("rescue must catch a raised error with its code and message");
}

#[test]
fn raise_uncaught_aborts_even_when_not_strict() {
    let (code, msg) = run_with("raise(\"nope\")\nassert(false, \"kept running\")\n", false)
        .expect_err("uncaught raise must stop the script");
    assert_eq!((code, msg.as_str()), (500, "nope"));
}

#[test]
fn raise_rejects_signal_codes() {
    let (code, msg) = run("raise(\"x\", code=-1)\n").expect_err("negative codes are reserved");
    assert_eq!(code, 400);
    assert!(msg.contains("must be positive"), "got: {msg}");
}
