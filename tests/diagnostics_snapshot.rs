//! Locks the exact rustc-style diagnostic text `render_pretty` produces — line/col header,
//! source-context window, and caret placement — so a future edit that reflows this rendering
//! shows up as a diff here instead of silently changing what users see in error output.

use rach::interpreter::render_pretty;

#[test]
fn lex_error_snapshot() {
    let src = "x = 1 +\n";
    let out = render_pretty("lex", 3, "<test>", 1, 8, "unexpected end of expression", Some(src), false);
    insta::assert_snapshot!(out, @r"
    error[3]: unexpected end of expression
      --> <test>:1:8
      |
    1 | > x = 1 +
      |          ^
      |
    // lex error 3 string 1
    ");
}

#[test]
fn parse_error_snapshot_with_context_lines() {
    let src = "rach f(x):\n    return x +\nend\n";
    let out = render_pretty("parse", 4, "<test>", 2, 15, "expected expression", Some(src), false);
    insta::assert_snapshot!(out, @r"
    error[4]: expected expression
      --> <test>:2:15
      |
    1 |   rach f(x):
    2 | >     return x +
      |                 ^
    3 |   end
      |
    // parse error 4 string 2
    ");
}

#[test]
fn runtime_error_snapshot_without_a_line_number() {
    let out = render_pretty("runtime", 500, "<test>", 0, 0, "no `main` function defined", None, false);
    insta::assert_snapshot!(out, @r"
    error[500]: no `main` function defined
      --> <test>
    // runtime error 500 string 0
    ");
}
