//! ASCII/Unicode art rendering: exact glyph shapes, styles, scaling, Cyrillic, shadow.

use rach::ast::Value;
use rach::interpreter::{self, make_ctx};
use rach::{lexer, parser};

fn run(expr: &str) -> Result<Value, String> {
    let src = format!("r = {expr}\n");
    let program = parser::parse(lexer::tokenize(&src).expect("lex")).expect("parse");
    let mut ctx = make_ctx(false, src.clone(), "<test>".into());
    interpreter::run_in_ctx(&program, &mut ctx).map_err(|e| e.message)?;
    Ok(ctx.lookup("r").expect("r bound"))
}

fn s(expr: &str) -> String {
    match run(expr) {
        Ok(Value::Str(s)) => s,
        other => panic!("`{expr}` -> expected a string, got {other:?}"),
    }
}

#[test]
fn text_glyph_is_pixel_exact() {
    assert_eq!(s("ascii_text(\"A\", style=\"ascii\")"), [
        " ###", "#   #", "#   #", "#   #", "#####", "#   #", "#   #",
    ].join("\n"));
    assert_eq!(s("ascii_text(\"Ж\", style=\"ascii\")"), [
        "# # #", "# # #", " ###", "  #", " ###", "# # #", "# # #",
    ].join("\n"), "Cyrillic has its own glyphs");
    assert_eq!(s("ascii_text(\"А\", style=\"ascii\")"), s("ascii_text(\"A\", style=\"ascii\")"), "Cyrillic А reuses Latin A");
    assert_eq!(s("ascii_text(\"ж\", style=\"ascii\")"), s("ascii_text(\"Ж\", style=\"ascii\")"), "lowercase Cyrillic uses the capital");
    assert_eq!(s("ascii_text(\"\u{263A}\", style=\"ascii\")"), s("ascii_text(\"?\", style=\"ascii\")"), "unknown characters fall back to ?");
}

#[test]
fn styles_and_scale() {
    let block = s("ascii_text(\"Hi\")");
    assert!(block.lines().all(|l| l.chars().all(|c| c == '█' || c == ' ')));
    assert_eq!(block.lines().count(), 7);

    let half = s("ascii_text(\"Hi\", style=\"half\")");
    assert_eq!(half.lines().count(), 4, "7 pixel rows → 4 half-block rows");

    let braille = s("ascii_text(\"Hi\", style=\"braille\")");
    assert_eq!(braille.lines().count(), 2, "7 pixel rows → 2 braille rows");
    assert!(braille.chars().any(|c| ('\u{2801}'..='\u{28ff}').contains(&c)));

    let big = s("ascii_text(\"I\", style=\"ascii\", scale=3)");
    assert_eq!(big.lines().count(), 21);
    assert_eq!(s("ascii_text(\"I\", style=\"ascii\", char=\"*\")").replace(['*', ' ', '\n'], ""), "");

    assert!(run("ascii_text(\"x\", style=\"neon\")").unwrap_err().contains("style must be"));
    assert!(run("ascii_text(\"x\", scale=9)").is_err());
}

#[test]
fn shadow_and_multiline() {
    let shadowed = s("ascii_text(\"I\", shadow=true)");
    assert!(shadowed.contains('░'), "shadow drawn with a lighter shade");
    assert_eq!(shadowed.lines().count(), 8, "one extra row for the shadow");
    let two = s("ascii_text(\"A\\nB\", style=\"ascii\")");
    assert_eq!(two.lines().count(), 15, "two 7-row lines + 1 row of spacing");
}
