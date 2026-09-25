//! String literal lexing: plain `"…"` is literal (braces allowed, e.g. embedded C/JSON);
//! only `f"…"` interpolates `{expr}`.

use rach::lexer::{self, StrPart, Tok};

fn single_str(src: &str) -> Vec<StrPart> {
    let tokens = lexer::tokenize(src).expect("must lex");
    match tokens.into_iter().map(|t| t.tok).find(|t| matches!(t, Tok::Str(_))) {
        Some(Tok::Str(parts)) => parts,
        _ => panic!("no string token in {src:?}"),
    }
}

#[test]
fn plain_string_keeps_braces_and_escaped_quotes() {
    // Regression: examples/native.rach — C source inside a plain string.
    let parts = single_str(r#"run_c("int main(){ printf(\"hi %d\\n\", 42); }")"#);
    match parts.as_slice() {
        [StrPart::Lit(s)] => assert_eq!(s, "int main(){ printf(\"hi %d\\n\", 42); }"),
        other => panic!("expected one literal part, got {other:?}"),
    }
}

#[test]
fn fstring_interpolates() {
    let parts = single_str(r#"print(f"a{x}b")"#);
    assert!(matches!(parts.as_slice(),
        [StrPart::Lit(a), StrPart::Expr(e), StrPart::Lit(b)] if a == "a" && e == "x" && b == "b"));
}
