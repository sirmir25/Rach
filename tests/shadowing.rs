//! User-defined functions shadow stdlib commands. Regression test: adding `hex_encode` made the
//! parser treat `hex(...)` as a (nonexistent) command, breaking any script defining `hex`.

use rach::ast::Value;
use rach::interpreter::{self, make_ctx, values_equal_pub};
use rach::{lexer, parser};

fn run(src: &str) -> interpreter::Ctx {
    let program = parser::parse(lexer::tokenize(src).expect("lex")).expect("parse");
    let mut ctx = make_ctx(false, src.to_string(), "<test>".into());
    interpreter::run_in_ctx(&program, &mut ctx).expect("run");
    ctx
}

fn assert_var(ctx: &interpreter::Ctx, name: &str, expected: &Value) {
    let got = ctx.lookup(name).unwrap_or_else(|| panic!("`{name}` unbound"));
    assert!(values_equal_pub(&got, expected), "{name}: expected {expected:?}, got {got:?}");
}

#[test]
fn user_fn_named_like_a_command_prefix_wins() {
    let ctx = run("rach hex(n):\n    return n * 16\nend\nrach index(xs, i):\n    return xs[i]\nend\na = hex(2)\nb = index([10, 20], 1)\n");
    assert_var(&ctx, "a", &Value::Int(32));
    assert_var(&ctx, "b", &Value::Int(20));
}

#[test]
fn user_fn_shadows_an_exact_builtin_in_expressions_and_statements() {
    let ctx = run("hits = 0\nrach sha256(x):\n    hits += 1\n    return \"mine\"\nend\nr = sha256(\"abc\")\nsha256(\"abc\")\n");
    assert_var(&ctx, "r", &Value::Str("mine".into()));
    assert_var(&ctx, "hits", &Value::Int(2));
}

#[test]
fn builtins_still_work_without_a_user_definition() {
    let ctx = run("r = sha256(\"\")\nh = hex_encode(\"A\")\n");
    assert_var(&ctx, "r", &Value::Str("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into()));
    assert_var(&ctx, "h", &Value::Str("41".into()));
}
