//! ASCII-art generation: banners, boxes, pyramids, diamonds, tables, mirrors, borders.
//!
//! Surface API (used by dispatch in stdlib/mod.rs):
//!   ascii banner("HELLO")
//!   ascii box("text", title="WARN", style="rounded")
//!   ascii pyramid("X")
//!   ascii diamond("X")
//!   ascii border("text", style="double")
//!   ascii mirror("text")
//!   ascii table(headers="A,B,C", rows="1,2,3;4,5,6")

use std::collections::BTreeMap;

use crate::ast::Value;
use crate::interpreter::RuntimeError;

fn first_str(args: &[Value], line: usize, what: &str) -> Result<String, RuntimeError> {
    args.first()
        .map(|v| v.as_str())
        .ok_or_else(|| RuntimeError::new(400, line, format!("{} requires text argument", what)))
}

fn kw_str(kwargs: &BTreeMap<String, Vec<Value>>, key: &str) -> Option<String> {
    kwargs.get(key).and_then(|v| v.first()).map(|v| v.as_str())
}

fn print_lines(lines: &[String]) {
    for l in lines { println!("{}", l); }
    println!("completed");
}

// ---------------- banner ----------------

pub fn banner(args: &[Value], _kwargs: &BTreeMap<String, Vec<Value>>, line: usize) -> Result<Value, RuntimeError> {
    let text = first_str(args, line, "ascii_banner")?.to_uppercase();
    let mut rows: [String; 5] = Default::default();
    let mut first = true;
    for ch in text.chars() {
        let glyph = glyph_for(ch);
        if !first {
            for r in rows.iter_mut() { r.push(' '); }
        }
        for (i, line_) in glyph.iter().enumerate() {
            rows[i].push_str(line_);
        }
        first = false;
    }
    let lines: Vec<String> = rows.iter().cloned().collect();
    let out = lines.join("\n");
    print_lines(&lines);
    Ok(Value::Str(out))
}

/// 5-line glyphs for A-Z, 0-9, and a couple of symbols. Anything else → blank pad.
fn glyph_for(ch: char) -> [&'static str; 5] {
    match ch.to_ascii_uppercase() {
        'A' => [" █████ ", "██   ██", "███████", "██   ██", "██   ██"],
        'B' => ["██████ ", "██   ██", "██████ ", "██   ██", "██████ "],
        'C' => [" ██████", "██     ", "██     ", "██     ", " ██████"],
        'D' => ["██████ ", "██   ██", "██   ██", "██   ██", "██████ "],
        'E' => ["███████", "██     ", "█████  ", "██     ", "███████"],
        'F' => ["███████", "██     ", "█████  ", "██     ", "██     "],
        'G' => [" ██████", "██     ", "██  ███", "██   ██", " ██████"],
        'H' => ["██   ██", "██   ██", "███████", "██   ██", "██   ██"],
        'I' => ["██", "██", "██", "██", "██"],
        'J' => ["     ██", "     ██", "     ██", "██   ██", " █████ "],
        'K' => ["██   ██", "██  ██ ", "█████  ", "██  ██ ", "██   ██"],
        'L' => ["██     ", "██     ", "██     ", "██     ", "███████"],
        'M' => ["███    ███", "████  ████", "██ ████ ██", "██  ██  ██", "██      ██"],
        'N' => ["███   ██", "████  ██", "██ ██ ██", "██  ████", "██   ███"],
        'O' => [" ██████ ", "██    ██", "██    ██", "██    ██", " ██████ "],
        'P' => ["██████ ", "██   ██", "██████ ", "██     ", "██     "],
        'Q' => [" ██████ ", "██    ██", "██    ██", "██  ████", " ██████ "],
        'R' => ["██████ ", "██   ██", "██████ ", "██   ██", "██   ██"],
        'S' => [" ██████", "██     ", " █████ ", "     ██", "██████ "],
        'T' => ["████████", "   ██   ", "   ██   ", "   ██   ", "   ██   "],
        'U' => ["██   ██", "██   ██", "██   ██", "██   ██", " █████ "],
        'V' => ["██   ██", "██   ██", "██   ██", " ██ ██ ", "  ███  "],
        'W' => ["██     ██", "██  █  ██", "██ ███ ██", "████ ████", "███   ███"],
        'X' => ["██   ██", " ██ ██ ", "  ███  ", " ██ ██ ", "██   ██"],
        'Y' => ["██   ██", " ██ ██ ", "  ███  ", "   █   ", "   █   "],
        'Z' => ["███████", "    ██ ", "  ███  ", " ██    ", "███████"],
        '0' => [" █████ ", "██   ██", "██   ██", "██   ██", " █████ "],
        '1' => ["  ██  ", "  ██  ", "  ██  ", "  ██  ", "  ██  "],
        '2' => ["█████ ", "    ██", " ████ ", "██    ", "██████"],
        '3' => ["█████ ", "    ██", " ████ ", "    ██", "█████ "],
        '4' => ["██  ██", "██  ██", "██████", "    ██", "    ██"],
        '5' => ["██████", "██    ", "█████ ", "    ██", "█████ "],
        '6' => [" █████", "██    ", "██████", "██   ██", " █████ "],
        '7' => ["██████", "    ██", "   ██ ", "  ██  ", "  ██  "],
        '8' => [" █████ ", "██   ██", " █████ ", "██   ██", " █████ "],
        '9' => [" █████ ", "██   ██", " ██████", "    ██ ", " ████  "],
        ' ' => ["    ", "    ", "    ", "    ", "    "],
        '!' => ["██", "██", "██", "  ", "██"],
        '?' => ["█████", "    ██", "  ███ ", "      ", "  █   "],
        '.' => ["  ", "  ", "  ", "  ", "██"],
        ',' => ["  ", "  ", "  ", "██", "█ "],
        '-' => ["    ", "    ", "████", "    ", "    "],
        _   => ["?? ", "?? ", "?? ", "?? ", "?? "],
    }
}

// ---------------- borders / box ----------------

/// 8-tuple: (top-left, top, top-right, left, right, bottom-left, bottom, bottom-right)
fn border_glyphs(style: &str) -> (&'static str, &'static str, &'static str, &'static str, &'static str, &'static str, &'static str, &'static str) {
    match style {
        "double"  => ("╔", "═", "╗", "║", "║", "╚", "═", "╝"),
        "bold"    => ("┏", "━", "┓", "┃", "┃", "┗", "━", "┛"),
        "rounded" => ("╭", "─", "╮", "│", "│", "╰", "─", "╯"),
        "ascii"   => ("+", "-", "+", "|", "|", "+", "-", "+"),
        "stars"   => ("*", "*", "*", "*", "*", "*", "*", "*"),
        "hash"    => ("#", "#", "#", "#", "#", "#", "#", "#"),
        _         => ("┌", "─", "┐", "│", "│", "└", "─", "┘"), // single
    }
}

pub fn box_around(args: &[Value], kwargs: &BTreeMap<String, Vec<Value>>, line: usize) -> Result<Value, RuntimeError> {
    let text = first_str(args, line, "ascii_box")?;
    let title = kw_str(kwargs, "title");
    let style = kw_str(kwargs, "style").unwrap_or_else(|| "single".into());
    let body_lines: Vec<&str> = text.split('\n').collect();
    let max_w = body_lines.iter().map(|s| s.chars().count()).max().unwrap_or(0)
        .max(title.as_deref().map(|t| t.chars().count() + 4).unwrap_or(0));
    let (tl, t, tr, l, r, bl, b, br) = border_glyphs(&style);
    let mut out = Vec::new();
    let top_bar = if let Some(tt) = &title {
        let pad = max_w.saturating_sub(tt.chars().count() + 2);
        format!("{}{} {} {}{}", tl, t, tt, t.repeat(pad), tr)
    } else {
        format!("{}{}{}", tl, t.repeat(max_w + 2), tr)
    };
    out.push(top_bar);
    for ln in body_lines {
        let pad = max_w - ln.chars().count();
        out.push(format!("{} {}{} {}", l, ln, " ".repeat(pad), r));
    }
    out.push(format!("{}{}{}", bl, b.repeat(max_w + 2), br));
    let s = out.join("\n");
    print_lines(&out);
    Ok(Value::Str(s))
}

pub fn border(args: &[Value], kwargs: &BTreeMap<String, Vec<Value>>, line: usize) -> Result<Value, RuntimeError> {
    box_around(args, kwargs, line)
}

// ---------------- pyramid / diamond ----------------

pub fn pyramid(args: &[Value], line: usize) -> Result<Value, RuntimeError> {
    let s = first_str(args, line, "ascii_pyramid")?;
    let height = if s.is_empty() { 5 } else { s.chars().count().max(3) };
    let ch = s.chars().next().unwrap_or('*');
    let mut lines = Vec::new();
    for i in 0..height {
        let pad = " ".repeat(height - 1 - i);
        let row = std::iter::repeat(ch).take(2 * i + 1).collect::<String>();
        lines.push(format!("{}{}", pad, row));
    }
    let out = lines.join("\n");
    print_lines(&lines);
    Ok(Value::Str(out))
}

pub fn diamond(args: &[Value], line: usize) -> Result<Value, RuntimeError> {
    let s = first_str(args, line, "ascii_diamond")?;
    let n = if s.is_empty() { 5 } else { s.chars().count().max(3) };
    let ch = s.chars().next().unwrap_or('*');
    let mut lines = Vec::new();
    for i in 0..n {
        let pad = " ".repeat(n - 1 - i);
        lines.push(format!("{}{}", pad, std::iter::repeat(ch).take(2 * i + 1).collect::<String>()));
    }
    for i in (0..n - 1).rev() {
        let pad = " ".repeat(n - 1 - i);
        lines.push(format!("{}{}", pad, std::iter::repeat(ch).take(2 * i + 1).collect::<String>()));
    }
    let out = lines.join("\n");
    print_lines(&lines);
    Ok(Value::Str(out))
}

// ---------------- mirror ----------------

pub fn mirror(args: &[Value], line: usize) -> Result<Value, RuntimeError> {
    let s = first_str(args, line, "ascii_mirror")?;
    let mirrored: String = s.chars().rev().collect();
    let out = format!("{} | {}", s, mirrored);
    println!("{}", out);
    println!("completed");
    Ok(Value::Str(out))
}

// ---------------- table ----------------

pub fn table(_args: &[Value], kwargs: &BTreeMap<String, Vec<Value>>, line: usize) -> Result<Value, RuntimeError> {
    let headers_raw = kw_str(kwargs, "headers").unwrap_or_default();
    let rows_raw = kw_str(kwargs, "rows").or_else(|| kw_str(kwargs, "data")).unwrap_or_default();
    if headers_raw.is_empty() {
        return Err(RuntimeError::new(400, line, "ascii_table requires headers=\"a,b,c\""));
    }

    let headers: Vec<String> = headers_raw.split(',').map(|s| s.trim().to_string()).collect();
    let rows: Vec<Vec<String>> = if rows_raw.is_empty() {
        Vec::new()
    } else {
        rows_raw.split(';').map(|r| r.split(',').map(|c| c.trim().to_string()).collect()).collect()
    };

    let mut widths: Vec<usize> = headers.iter().map(|h| h.chars().count()).collect();
    for r in &rows {
        for (i, c) in r.iter().enumerate() {
            if i >= widths.len() { widths.push(c.chars().count()); }
            else { widths[i] = widths[i].max(c.chars().count()); }
        }
    }

    let sep = format!("+{}+",
        widths.iter().map(|w| "-".repeat(w + 2)).collect::<Vec<_>>().join("+"));
    let render_row = |r: &[String]| -> String {
        let mut out = String::from("|");
        for (i, cell) in r.iter().enumerate() {
            let w = widths.get(i).copied().unwrap_or(0);
            let pad = w - cell.chars().count();
            out.push_str(&format!(" {}{} |", cell, " ".repeat(pad)));
        }
        out
    };

    let mut lines = vec![sep.clone(), render_row(&headers), sep.clone()];
    for r in &rows { lines.push(render_row(r)); }
    if !rows.is_empty() { lines.push(sep); }

    let out = lines.join("\n");
    print_lines(&lines);
    Ok(Value::Str(out))
}
