//! `struct` + `impl` methods: dispatch, default params, and `self`-mutation write-back.

use rach::ast::Value;
use rach::interpreter::{self, make_ctx, values_equal_pub};
use rach::{lexer, parser};

fn run(src: &str) -> interpreter::Ctx {
    let tokens = lexer::tokenize(src).expect("lex");
    let program = parser::parse(tokens).expect("parse");
    let mut ctx = make_ctx(false, src.to_string(), "<test>".into());
    interpreter::run_in_ctx(&program, &mut ctx).expect("run");
    ctx
}

fn field(v: &Value, name: &str) -> Value {
    match v {
        Value::Struct { fields, .. } => fields.get(name).cloned().expect("field present"),
        other => panic!("expected struct, got {other:?}"),
    }
}

/// `Value` has no `PartialEq` (deliberately — float/int equality goes through the
/// language's own numeric-coercion rules), so tests compare via the same helper the
/// interpreter uses for `==`.
fn assert_value_eq(actual: &Value, expected: &Value) {
    assert!(
        values_equal_pub(actual, expected),
        "expected {expected:?}, got {actual:?}"
    );
}

#[test]
fn method_call_returns_a_value() {
    let ctx = run(
        "struct Point { x, y }\n\
         impl Point:\n    \
             rach dist(self):\n        \
                 return self.x * self.x + self.y * self.y\n    \
             end\n\
         end\n\
         p = Point { x: 3, y: 4 }\n\
         d = p.dist()\n",
    );
    assert_value_eq(&ctx.lookup("d").expect("d defined"), &Value::Int(25));
}

#[test]
fn method_mutation_writes_back_through_a_variable() {
    let ctx = run(
        "struct Point { x, y }\n\
         impl Point:\n    \
             rach move(self, dx, dy):\n        \
                 self.x = self.x + dx\n        \
                 self.y = self.y + dy\n    \
             end\n\
         end\n\
         p = Point { x: 1, y: 1 }\n\
         p.move(2, 3)\n",
    );
    let p = ctx.lookup("p").expect("p defined");
    assert_value_eq(&field(&p, "x"), &Value::Int(3));
    assert_value_eq(&field(&p, "y"), &Value::Int(4));
}

#[test]
fn method_mutation_writes_back_through_a_list_index_but_not_siblings() {
    let ctx = run(
        "struct Point { x, y }\n\
         impl Point:\n    \
             rach move(self, dx):\n        \
                 self.x = self.x + dx\n    \
             end\n\
         end\n\
         points = [Point { x: 0, y: 0 }, Point { x: 1, y: 1 }]\n\
         points[0].move(5)\n",
    );
    let points = ctx.lookup("points").expect("points defined");
    let Value::List(items) = points else { panic!("expected list") };
    assert_value_eq(&field(&items[0], "x"), &Value::Int(5));
    assert_value_eq(&field(&items[1], "x"), &Value::Int(1));
}

#[test]
fn method_uses_default_parameter() {
    let ctx = run(
        "struct Point { x }\n\
         impl Point:\n    \
             rach label(self, tag = \"pt\"):\n        \
                 return tag\n    \
             end\n\
         end\n\
         p = Point { x: 1 }\n\
         a = p.label()\n\
         b = p.label(\"custom\")\n",
    );
    assert_value_eq(&ctx.lookup("a").expect("a defined"), &Value::Str("pt".into()));
    assert_value_eq(&ctx.lookup("b").expect("b defined"), &Value::Str("custom".into()));
}

#[test]
fn calling_an_undefined_method_is_a_clean_runtime_error_not_a_panic() {
    let tokens = lexer::tokenize("struct Point { x }\np = Point { x: 1 }\np.nope()\n").expect("lex");
    let program = parser::parse(tokens).expect("parse");
    let mut ctx = make_ctx(false, String::new(), "<test>".into());
    let err = interpreter::run_in_ctx(&program, &mut ctx).expect_err("undefined method must error");
    assert!(err.message.contains("nope"), "expected the method name in the error, got: {}", err.message);
}
