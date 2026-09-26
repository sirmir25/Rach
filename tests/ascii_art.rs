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

#[test]
fn sparkline_maps_min_to_lowest_and_max_to_highest() {
    assert_eq!(s("ascii_sparkline([0, 7, 14, 7, 0])"), "▁▅█▅▁", "3.5 of 7 levels rounds up");
    assert_eq!(s("ascii_sparkline([5, 5, 5])"), "▄▄▄", "flat data sits mid-height");
    assert!(run("ascii_sparkline([1, \"x\"])").is_err());
}

#[test]
fn bars_scale_to_the_largest_value_with_eighth_precision() {
    let out = s("ascii_bars([[\"a\", 8], [\"bb\", 4], [\"c\", 1]], width=8)");
    assert_eq!(out, ["a  │████████ 8", "bb │████ 4", "c  │█ 1"].join("\n"));
    let fine = s("ascii_bars([[\"x\", 16], [\"y\", 1]], width=2)");
    assert!(fine.lines().nth(1).unwrap().contains('▏'), "1/16 of 2 cells = one eighth-block: {fine}");
    let from_map = s("ascii_bars({\"b\": 2, \"a\": 1}, width=4)");
    assert!(from_map.starts_with("a │"), "maps list in key order: {from_map}");
    assert!(run("ascii_bars([1, -2])").unwrap_err().contains(">= 0"));
}

#[test]
fn progress_bar() {
    assert_eq!(s("ascii_progress(50, 100, width=10)"), "[█████░░░░░]  50%");
    assert_eq!(s("ascii_progress(250, width=4)"), "[████] 100%", "clamped to 100%");
    assert_eq!(s("ascii_progress(0, 5, width=3)"), "[░░░]   0%");
    assert!(run("ascii_progress(1, 0)").is_err());
}

#[test]
fn plot_draws_a_labelled_braille_line() {
    let out = s("ascii_plot([0, 10], width=5, height=3)");
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 4, "3 chart rows + axis:\n{out}");
    assert!(lines[0].starts_with("10 ┤"), "{out}");
    assert!(lines[2].starts_with(" 0 ┤"), "{out}");
    assert!(lines[3].ends_with("└─────"));
    assert!(out.chars().any(|c| ('\u{2801}'..='\u{28ff}').contains(&c)));
    assert!(run("ascii_plot([1])").unwrap_err().contains("at least 2"));
}
