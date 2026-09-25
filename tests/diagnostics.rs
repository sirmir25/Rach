//! Lex and parse errors carry the 1-based column of the offending token.

use rach::{lexer, parser};

#[test]
fn lex_error_has_column() {
    let err = lexer::tokenize("print(\"ok\")\nx = 1 @ 2\n").expect_err("`@` is not a token");
    assert_eq!((err.line, err.col), (2, 7));
}

#[test]
fn unterminated_string_points_at_opening_quote() {
    let err = lexer::tokenize("x = \"abc\n").expect_err("unterminated");
    assert_eq!((err.line, err.col), (1, 5));
}

#[test]
fn parse_error_has_column() {
    let tokens = lexer::tokenize("x = (1 + \n").expect("lexes");
    let err = parser::parse(tokens).expect_err("incomplete expression");
    assert_eq!((err.line, err.col), (1, 10));
}
