//! int / float / str / bool / type_of.

use rach::ast::Value;
use rach::interpreter::{self, make_ctx, values_equal_pub};
use rach::{lexer, parser};

fn run(src: &str) -> Result<Value, String> {
    let src = format!("{src}\n");
    let program = parser::parse(lexer::tokenize(&src).expect("lex")).expect("parse");
    let mut ctx = make_ctx(false, src.clone(), "<test>".into());
    interpreter::run_in_ctx(&program, &mut ctx).map_err(|e| e.message)?;
    Ok(ctx.lookup("r").expect("r bound"))
}

fn is(expr: &str, expected: Value) {
    let got = run(&format!("r = {expr}")).unwrap_or_else(|e| panic!("{expr}: {e}"));
    assert!(values_equal_pub(&got, &expected) && std::mem::discriminant(&got) == std::mem::discriminant(&expected),
        "{expr}: expected {expected:?}, got {got:?}");
}

#[test]
fn int_truncates_toward_zero_and_parses_strings() {
    is("int(3.9)", Value::Int(3));
    is("int(-3.9)", Value::Int(-3));
    is("int(\"42\")", Value::Int(42));
    is("int(\" -7.5 \")", Value::Int(-7));
    is("int(true)", Value::Int(1));
    is("int(sqrt(16))", Value::Int(4));
    for bad in ["int(\"abc\")", "int([1])", "int(10.0 ^ 300)", "int(nil)"] {
        assert!(run(&format!("r = {bad}")).is_err(), "{bad} should fail, not saturate");
    }
}

#[test]
fn float_str_bool_type_of() {
    is("float(2)", Value::Float(2.0));
    is("float(\"2.5\")", Value::Float(2.5));
    is("str(42)", Value::Str("42".into()));
    is("str(1.5)", Value::Str("1.5".into()));
    is("bool(0)", Value::Bool(false));
    is("bool(\"x\")", Value::Bool(true));
    is("bool([])", Value::Bool(false));
    for (expr, t) in [("1", "int"), ("1.0", "float"), ("\"s\"", "str"), ("true", "bool"), ("[1]", "list"),
                      ("{\"a\": 1}", "map"), ("nil", "nil"), ("fn(x) -> x", "fn")] {
        is(&format!("type_of({expr})"), Value::Str(t.into()));
    }
    let r = run("struct Point { x, y }\nr = type_of(Point { x: 1, y: 2 })").unwrap();
    assert!(values_equal_pub(&r, &Value::Str("Point".into())), "a struct reports its own name");
    assert!(run("r = float([1])").is_err());
}
