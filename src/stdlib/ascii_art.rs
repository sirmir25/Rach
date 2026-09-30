//! ASCII / Unicode art: text in a 5×7 bitmap font (full printable ASCII plus Cyrillic) with
//! block, ASCII, half-block and braille styles and an optional drop shadow; more to come on the same canvas.

use crate::ast::Value;
use crate::interpreter::{Ctx, RuntimeError};
use crate::stdlib::args::{emit_art, int_arg, kw_bool, kw_str, str_arg, Kwargs};
use crate::stdlib::canvas::{Canvas, Render};

/// Classic 5×7 LCD font for U+0020..=U+007E. Column-major: one byte per column, bit 0 = top row.
const FONT_ASCII: [[u8; 5]; 95] = [
    [0x00, 0x00, 0x00, 0x00, 0x00], [0x00, 0x00, 0x5f, 0x00, 0x00], [0x00, 0x07, 0x00, 0x07, 0x00], [0x14, 0x7f, 0x14, 0x7f, 0x14],
    [0x24, 0x2a, 0x7f, 0x2a, 0x12], [0x23, 0x13, 0x08, 0x64, 0x62], [0x36, 0x49, 0x55, 0x22, 0x50], [0x00, 0x05, 0x03, 0x00, 0x00],
    [0x00, 0x1c, 0x22, 0x41, 0x00], [0x00, 0x41, 0x22, 0x1c, 0x00], [0x08, 0x2a, 0x1c, 0x2a, 0x08], [0x08, 0x08, 0x3e, 0x08, 0x08],
    [0x00, 0x50, 0x30, 0x00, 0x00], [0x08, 0x08, 0x08, 0x08, 0x08], [0x00, 0x60, 0x60, 0x00, 0x00], [0x20, 0x10, 0x08, 0x04, 0x02],
    [0x3e, 0x51, 0x49, 0x45, 0x3e], [0x00, 0x42, 0x7f, 0x40, 0x00], [0x42, 0x61, 0x51, 0x49, 0x46], [0x21, 0x41, 0x45, 0x4b, 0x31],
    [0x18, 0x14, 0x12, 0x7f, 0x10], [0x27, 0x45, 0x45, 0x45, 0x39], [0x3c, 0x4a, 0x49, 0x49, 0x30], [0x01, 0x71, 0x09, 0x05, 0x03],
    [0x36, 0x49, 0x49, 0x49, 0x36], [0x06, 0x49, 0x49, 0x29, 0x1e], [0x00, 0x36, 0x36, 0x00, 0x00], [0x00, 0x56, 0x36, 0x00, 0x00],
    [0x08, 0x14, 0x22, 0x41, 0x00], [0x14, 0x14, 0x14, 0x14, 0x14], [0x00, 0x41, 0x22, 0x14, 0x08], [0x02, 0x01, 0x51, 0x09, 0x06],
    [0x32, 0x49, 0x79, 0x41, 0x3e], [0x7e, 0x11, 0x11, 0x11, 0x7e], [0x7f, 0x49, 0x49, 0x49, 0x36], [0x3e, 0x41, 0x41, 0x41, 0x22],
    [0x7f, 0x41, 0x41, 0x22, 0x1c], [0x7f, 0x49, 0x49, 0x49, 0x41], [0x7f, 0x09, 0x09, 0x01, 0x01], [0x3e, 0x41, 0x41, 0x51, 0x32],
    [0x7f, 0x08, 0x08, 0x08, 0x7f], [0x00, 0x41, 0x7f, 0x41, 0x00], [0x20, 0x40, 0x41, 0x3f, 0x01], [0x7f, 0x08, 0x14, 0x22, 0x41],
    [0x7f, 0x40, 0x40, 0x40, 0x40], [0x7f, 0x02, 0x04, 0x02, 0x7f], [0x7f, 0x04, 0x08, 0x10, 0x7f], [0x3e, 0x41, 0x41, 0x41, 0x3e],
    [0x7f, 0x09, 0x09, 0x09, 0x06], [0x3e, 0x41, 0x51, 0x21, 0x5e], [0x7f, 0x09, 0x19, 0x29, 0x46], [0x46, 0x49, 0x49, 0x49, 0x31],
    [0x01, 0x01, 0x7f, 0x01, 0x01], [0x3f, 0x40, 0x40, 0x40, 0x3f], [0x1f, 0x20, 0x40, 0x20, 0x1f], [0x7f, 0x20, 0x18, 0x20, 0x7f],
    [0x63, 0x14, 0x08, 0x14, 0x63], [0x03, 0x04, 0x78, 0x04, 0x03], [0x61, 0x51, 0x49, 0x45, 0x43], [0x00, 0x7f, 0x41, 0x41, 0x00],
    [0x02, 0x04, 0x08, 0x10, 0x20], [0x00, 0x41, 0x41, 0x7f, 0x00], [0x04, 0x02, 0x01, 0x02, 0x04], [0x40, 0x40, 0x40, 0x40, 0x40],
    [0x00, 0x01, 0x02, 0x04, 0x00], [0x20, 0x54, 0x54, 0x54, 0x78], [0x7f, 0x48, 0x44, 0x44, 0x38], [0x38, 0x44, 0x44, 0x44, 0x20],
    [0x38, 0x44, 0x44, 0x48, 0x7f], [0x38, 0x54, 0x54, 0x54, 0x18], [0x08, 0x7e, 0x09, 0x01, 0x02], [0x08, 0x14, 0x54, 0x54, 0x3c],
    [0x7f, 0x08, 0x04, 0x04, 0x78], [0x00, 0x44, 0x7d, 0x40, 0x00], [0x20, 0x40, 0x44, 0x3d, 0x00], [0x00, 0x7f, 0x10, 0x28, 0x44],
    [0x00, 0x41, 0x7f, 0x40, 0x00], [0x7c, 0x04, 0x18, 0x04, 0x78], [0x7c, 0x08, 0x04, 0x04, 0x78], [0x38, 0x44, 0x44, 0x44, 0x38],
    [0x7c, 0x14, 0x14, 0x14, 0x08], [0x08, 0x14, 0x14, 0x18, 0x7c], [0x7c, 0x08, 0x04, 0x04, 0x08], [0x48, 0x54, 0x54, 0x54, 0x20],
    [0x04, 0x3f, 0x44, 0x40, 0x20], [0x3c, 0x40, 0x40, 0x20, 0x7c], [0x1c, 0x20, 0x40, 0x20, 0x1c], [0x3c, 0x40, 0x30, 0x40, 0x3c],
    [0x44, 0x28, 0x10, 0x28, 0x44], [0x0c, 0x50, 0x50, 0x50, 0x3c], [0x44, 0x64, 0x54, 0x4c, 0x44], [0x00, 0x08, 0x36, 0x41, 0x00],
    [0x00, 0x00, 0x7f, 0x00, 0x00], [0x00, 0x41, 0x36, 0x08, 0x00], [0x02, 0x01, 0x02, 0x04, 0x02],
];

/// Cyrillic capitals that don't share a shape with a Latin letter (А В Е К М Н О Р С Т Х reuse
/// the Latin glyphs). Lowercase Cyrillic renders with the capital's glyph.
const FONT_CYRILLIC: &[(char, [u8; 5])] = &[
    ('Б', [0x7f, 0x49, 0x49, 0x49, 0x31]),
    ('Г', [0x7f, 0x01, 0x01, 0x01, 0x01]),
    ('Д', [0x60, 0x3e, 0x21, 0x3f, 0x60]),
    ('Ж', [0x63, 0x14, 0x7f, 0x14, 0x63]),
    ('З', [0x22, 0x41, 0x49, 0x49, 0x36]),
    ('И', [0x7f, 0x10, 0x08, 0x04, 0x7f]),
    ('Й', [0x7e, 0x11, 0x08, 0x05, 0x7e]),
    ('Л', [0x40, 0x3e, 0x01, 0x01, 0x7f]),
    ('П', [0x7f, 0x01, 0x01, 0x01, 0x7f]),
    ('У', [0x27, 0x48, 0x48, 0x48, 0x3f]),
    ('Ф', [0x0c, 0x12, 0x7f, 0x12, 0x0c]),
    ('Ц', [0x3f, 0x20, 0x20, 0x3f, 0x60]),
    ('Ч', [0x07, 0x08, 0x08, 0x08, 0x7f]),
    ('Ш', [0x7f, 0x40, 0x7f, 0x40, 0x7f]),
    ('Щ', [0x3f, 0x20, 0x3f, 0x20, 0x7f]),
    ('Ъ', [0x01, 0x7f, 0x48, 0x48, 0x30]),
    ('Ы', [0x7f, 0x48, 0x48, 0x30, 0x7f]),
    ('Ь', [0x7f, 0x48, 0x48, 0x48, 0x30]),
    ('Э', [0x22, 0x41, 0x49, 0x49, 0x3e]),
    ('Ю', [0x7f, 0x08, 0x3e, 0x41, 0x3e]),
    ('Я', [0x46, 0x29, 0x19, 0x09, 0x7f]),
    ('Ё', [0x7e, 0x4b, 0x4a, 0x4b, 0x42]),
];

fn glyph(c: char) -> [u8; 5] {
    let latin_twin = |c: char| match c {
        'А' => Some('A'), 'В' => Some('B'), 'Е' => Some('E'), 'К' => Some('K'), 'М' => Some('M'),
        'Н' => Some('H'), 'О' => Some('O'), 'Р' => Some('P'), 'С' => Some('C'), 'Т' => Some('T'),
        'Х' => Some('X'),
        _ => None,
    };
    if (' '..='~').contains(&c) {
        return FONT_ASCII[c as usize - 0x20];
    }
    let upper = c.to_uppercase().next().unwrap_or(c);
    if let Some(l) = latin_twin(upper) {
        return FONT_ASCII[l as usize - 0x20];
    }
    FONT_CYRILLIC.iter().find(|(k, _)| *k == upper).map_or(FONT_ASCII['?' as usize - 0x20], |(_, g)| *g)
}

const GLYPH_W: usize = 5;
const GLYPH_H: usize = 7;

/// Rasterize text (multi-line allowed) at `scale` canvas pixels per font pixel, one font pixel of
/// spacing between letters and lines, and `pad` blank pixels of margin (room for a shadow).
fn rasterize(text: &str, scale: usize, pad: usize) -> Canvas {
    let lines: Vec<Vec<char>> = text.split('\n').map(|l| l.chars().collect()).collect();
    let widest = lines.iter().map(Vec::len).max().unwrap_or(0);
    let w = (widest * (GLYPH_W + 1)).saturating_sub(1) * scale + pad;
    let h = (lines.len() * (GLYPH_H + 1)).saturating_sub(1) * scale + pad;
    let mut canvas = Canvas::new(w, h);
    for (ln, chars) in lines.iter().enumerate() {
        for (i, &c) in chars.iter().enumerate() {
            let g = glyph(c);
            for (col, bits) in g.iter().enumerate() {
                for row in 0..GLYPH_H {
                    if bits >> row & 1 == 0 { continue; }
                    let x0 = (i * (GLYPH_W + 1) + col) * scale;
                    let y0 = (ln * (GLYPH_H + 1) + row) * scale;
                    for dy in 0..scale {
                        for dx in 0..scale { canvas.set((x0 + dx) as i64, (y0 + dy) as i64, 1.0); }
                    }
                }
            }
        }
    }
    canvas
}

/// Drop shadow one pixel down-right, drawn at half intensity under the lit pixels.
fn with_shadow(src: &Canvas) -> Canvas {
    let mut out = Canvas::new(src.w, src.h);
    for y in 0..src.h {
        for x in 0..src.w {
            if src.get(x, y) >= 0.5 {
                out.set(x as i64 + 1, y as i64 + 1, 0.5);
            }
        }
    }
    for y in 0..src.h {
        for x in 0..src.w {
            if src.get(x, y) >= 0.5 { out.set(x as i64, y as i64, 1.0); }
        }
    }
    out
}

pub fn ascii_text(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let what = "ascii_text";
    let text = str_arg(args, 0, line, what)?;
    let style = kw_str(kwargs, "style").unwrap_or_else(|| "block".into());
    let scale = int_arg(args, 1, kwargs, "scale", 1, line, what)?;
    let scale = usize::try_from(scale).ok().filter(|s| (1..=8).contains(s))
        .ok_or_else(|| RuntimeError::new(400, line, format!("{what}: scale must be 1..=8")))?;
    let shadow = kw_bool(kwargs, "shadow");
    let fill = kw_str(kwargs, "char").and_then(|s| s.chars().next());

    let mut canvas = rasterize(&text, scale, usize::from(shadow));
    if shadow { canvas = with_shadow(&canvas); }
    let mode = match style.as_str() {
        "block" => Render::Blocks { fill: fill.unwrap_or('█'), half: '░', width: 2 },
        "ascii" => Render::Blocks { fill: fill.unwrap_or('#'), half: '.', width: 1 },
        "half" => Render::HalfBlocks,
        "braille" => Render::Braille,
        other => return Err(RuntimeError::new(400, line, format!(
            "{what}: style must be block, ascii, half or braille; got \"{other}\""
        ))),
    };
    Ok(emit_art(ctx, canvas.render(mode)))
}

// ---------------- charts ----------------

const EIGHTHS: [char; 8] = ['▏', '▎', '▍', '▌', '▋', '▊', '▉', '█'];
const SPARKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

fn numbers(v: &Value, line: usize, what: &str) -> Result<Vec<f64>, RuntimeError> {
    let Value::List(items) = v else {
        return Err(RuntimeError::new(400, line, format!("{what}: expected a list of numbers, got {}", v.as_str())));
    };
    items.iter().map(|x| match x {
        Value::Int(_) | Value::Float(_) => x.as_f64().filter(|f| f.is_finite()),
        _ => None,
    }.ok_or_else(|| RuntimeError::new(400, line, format!("{what}: `{}` is not a finite number", x.as_str())))).collect()
}

/// Integers print without a decimal point; everything else with up to two decimals.
fn fmt_num(x: f64) -> String {
    if x.fract() == 0.0 && x.abs() < 1e15 {
        format!("{}", x as i64)
    } else {
        let s = format!("{x:.2}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// A bar `fraction` (0..=1) of `width` cells long, with 1/8-cell precision.
fn bar(fraction: f64, width: usize) -> String {
    let eighths = (fraction.clamp(0.0, 1.0) * (width * 8) as f64).round() as usize;
    let mut s = "█".repeat(eighths / 8);
    if !eighths.is_multiple_of(8) { s.push(EIGHTHS[eighths % 8 - 1]); }
    s
}

fn width_arg(args: &[Value], n: usize, kwargs: &Kwargs, key: &str, default: i64, line: usize, what: &str) -> Result<usize, RuntimeError> {
    let w = int_arg(args, n, kwargs, key, default, line, what)?;
    usize::try_from(w).ok().filter(|w| (1..=1000).contains(w))
        .ok_or_else(|| RuntimeError::new(400, line, format!("{what}: {key} must be 1..=1000")))
}

pub fn ascii_sparkline(args: &[Value], _kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let what = "ascii_sparkline";
    let xs = numbers(args.first().unwrap_or(&Value::Nil), line, what)?;
    let (lo, hi) = xs.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), x| (lo.min(*x), hi.max(*x)));
    let s: String = xs.iter().map(|x| {
        if hi > lo { SPARKS[(((x - lo) / (hi - lo)) * 7.0).round() as usize] } else { SPARKS[3] }
    }).collect();
    Ok(emit_art(ctx, s))
}

/// `data` is a map (label → value), a list of numbers (labelled 1..n), or a list of
/// `[label, value]` pairs (keeps your order).
fn labelled(data: &Value, line: usize, what: &str) -> Result<Vec<(String, f64)>, RuntimeError> {
    let bad = |v: &Value| RuntimeError::new(400, line, format!("{what}: `{}` is not a finite number", v.as_str()));
    let num = |v: &Value| v.as_f64().filter(|f| f.is_finite() && matches!(v, Value::Int(_) | Value::Float(_))).ok_or_else(|| bad(v));
    match data {
        Value::Map(m) => m.iter().map(|(k, v)| Ok((k.clone(), num(v)?))).collect(),
        Value::List(items) => items.iter().enumerate().map(|(i, item)| match item {
            Value::List(pair) if pair.len() == 2 => Ok((pair[0].as_str(), num(&pair[1])?)),
            other => Ok(((i + 1).to_string(), num(other)?)),
        }).collect(),
        other => Err(RuntimeError::new(400, line, format!("{what}: expected a map or list, got {}", other.as_str()))),
    }
}

pub fn ascii_bars(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let what = "ascii_bars";
    let rows = labelled(args.first().unwrap_or(&Value::Nil), line, what)?;
    let width = width_arg(args, 1, kwargs, "width", 40, line, what)?;
    if rows.iter().any(|(_, v)| *v < 0.0) {
        return Err(RuntimeError::new(400, line, format!("{what}: values must be >= 0")));
    }
    let max = rows.iter().map(|(_, v)| *v).fold(0.0, f64::max);
    let label_w = rows.iter().map(|(l, _)| l.chars().count()).max().unwrap_or(0);
    let lines: Vec<String> = rows.iter().map(|(label, v)| {
        let pad = " ".repeat(label_w - label.chars().count());
        let b = if max > 0.0 { bar(v / max, width) } else { String::new() };
        format!("{label}{pad} │{b} {}", fmt_num(*v))
    }).collect();
    Ok(emit_art(ctx, lines.join("\n")))
}

pub fn ascii_progress(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let what = "ascii_progress";
    let value = args.first().and_then(Value::as_f64)
        .ok_or_else(|| RuntimeError::new(400, line, format!("{what}: first argument must be a number")))?;
    let total = args.get(1).cloned().or_else(|| kwargs.get("total").and_then(|v| v.first().cloned()))
        .map_or(Some(100.0), |v| v.as_f64())
        .filter(|t| *t > 0.0)
        .ok_or_else(|| RuntimeError::new(400, line, format!("{what}: total must be a positive number")))?;
    let width = width_arg(args, 2, kwargs, "width", 30, line, what)?;
    let fraction = (value / total).clamp(0.0, 1.0);
    let filled = bar(fraction, width);
    let empty = "░".repeat(width - filled.chars().count());
    Ok(emit_art(ctx, format!("[{filled}{empty}] {:>3}%", (fraction * 100.0).round() as i64)))
}

/// Line chart in braille (2×4 dots per character) with min/max labels on the left axis.
pub fn ascii_plot(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let what = "ascii_plot";
    let ys = numbers(args.first().unwrap_or(&Value::Nil), line, what)?;
    if ys.len() < 2 { return Err(RuntimeError::new(400, line, format!("{what}: need at least 2 points"))); }
    let width = width_arg(args, 1, kwargs, "width", 60, line, what)?;
    let height = width_arg(args, 2, kwargs, "height", 12, line, what)?;
    let (pw, ph) = (width * 2, height * 4);
    let (lo, hi) = ys.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), y| (lo.min(*y), hi.max(*y)));
    let span = if hi > lo { hi - lo } else { 1.0 };
    let to_px = |i: usize, y: f64| -> (i64, i64) {
        let x = (i as f64 * (pw - 1) as f64 / (ys.len() - 1) as f64).round() as i64;
        let py = ((hi - y) / span * (ph - 1) as f64).round() as i64;
        (x, py)
    };
    let mut canvas = Canvas::new(pw, ph);
    for i in 1..ys.len() {
        let (x0, y0) = to_px(i - 1, ys[i - 1]);
        let (x1, y1) = to_px(i, ys[i]);
        canvas.line(x0, y0, x1, y1);
    }
    let chart = canvas.render(Render::Braille);
    let (top, bottom) = (fmt_num(hi), fmt_num(lo));
    let label_w = top.chars().count().max(bottom.chars().count());
    let rows: Vec<&str> = chart.split('\n').collect();
    let mut out: Vec<String> = Vec::with_capacity(height + 1);
    for r in 0..height {
        let body = rows.get(r).copied().unwrap_or("");
        let (label, tick) = match r {
            0 => (top.as_str(), '┤'),
            _ if r == height - 1 => (bottom.as_str(), '┤'),
            _ => ("", '│'),
        };
        out.push(format!("{label:>label_w$} {tick}{body}").trim_end().to_string());
    }
    out.push(format!("{} └{}", " ".repeat(label_w), "─".repeat(width)));
    Ok(emit_art(ctx, out.join("\n")))
}

// ---------------- trees, fractals, shapes ----------------

fn tree_lines(value: &Value, prefix: &str, out: &mut Vec<String>) {
    let children: Vec<(String, &Value)> = match value {
        Value::Map(m) => m.iter().map(|(k, v)| (k.clone(), v)).collect(),
        Value::List(items) => items.iter().enumerate().map(|(i, v)| (format!("[{i}]"), v)).collect(),
        _ => return,
    };
    let list_parent = matches!(value, Value::List(_));
    for (i, (label, child)) in children.iter().enumerate() {
        let last = i + 1 == children.len();
        let (branch, next) = if last { ("└── ", "    ") } else { ("├── ", "│   ") };
        let text = match child {
            Value::Map(_) | Value::List(_) => label.clone(),
            scalar if list_parent => scalar.as_str(),
            scalar => format!("{label}: {}", scalar.as_str()),
        };
        out.push(format!("{prefix}{branch}{text}"));
        tree_lines(child, &format!("{prefix}{next}"), out);
    }
}

pub fn ascii_tree(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let what = "ascii_tree";
    let value = args.first().ok_or_else(|| RuntimeError::new(400, line, format!("{what} requires a map or list")))?;
    if !matches!(value, Value::Map(_) | Value::List(_)) {
        return Err(RuntimeError::new(400, line, format!("{what}: expected a map or list, got {}", value.as_str())));
    }
    let mut out = vec![kw_str(kwargs, "root").unwrap_or_else(|| ".".into())];
    tree_lines(value, "", &mut out);
    Ok(emit_art(ctx, out.join("\n")))
}

fn render_style(style: &str, line: usize, what: &str) -> Result<Render, RuntimeError> {
    match style {
        "ascii" => Ok(Render::Ramp(crate::stdlib::canvas::RAMP_ASCII)),
        "shade" => Ok(Render::Ramp(crate::stdlib::canvas::RAMP_SHADE)),
        "braille" => Ok(Render::Braille),
        "half" => Ok(Render::HalfBlocks),
        other => Err(RuntimeError::new(400, line, format!("{what}: style must be ascii, shade, braille or half; got \"{other}\""))),
    }
}

fn float_kw(kwargs: &Kwargs, key: &str, default: f64, line: usize, what: &str) -> Result<f64, RuntimeError> {
    match kwargs.get(key).and_then(|v| v.first()) {
        None => Ok(default),
        Some(v) => v.as_f64().filter(|f| f.is_finite())
            .ok_or_else(|| RuntimeError::new(400, line, format!("{what}: `{key}` must be a number"))),
    }
}

/// Escape-time Mandelbrot with smooth (fractional) iteration counts. 1-bit styles (braille,
/// half) are Floyd–Steinberg dithered so the glow around the set still shows.
pub fn ascii_mandelbrot(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let what = "ascii_mandelbrot";
    let width = width_arg(args, 0, kwargs, "width", 78, line, what)?;
    let height = width_arg(args, 1, kwargs, "height", 30, line, what)?;
    let max_iter = int_arg(args, 2, kwargs, "iterations", 80, line, what)?;
    let max_iter = u32::try_from(max_iter).ok().filter(|n| (1..=10_000).contains(n))
        .ok_or_else(|| RuntimeError::new(400, line, format!("{what}: iterations must be 1..=10000")))?;
    let mode = render_style(&kw_str(kwargs, "style").unwrap_or_else(|| "ascii".into()), line, what)?;
    let (cx, cy) = (float_kw(kwargs, "x", -0.5, line, what)?, float_kw(kwargs, "y", 0.0, line, what)?);
    let zoom = float_kw(kwargs, "zoom", 1.0, line, what)?;
    if zoom <= 0.0 { return Err(RuntimeError::new(400, line, format!("{what}: zoom must be > 0"))); }

    let one_bit = matches!(mode, Render::Braille | Render::HalfBlocks);
    let (cell_w, cell_h) = mode.cell();
    let (pw, ph) = (width * cell_w, height * cell_h);
    let step_x = 3.2 / zoom / pw as f64;
    let step_y = step_x * mode.pixel_aspect();
    let mut canvas = Canvas::new(pw, ph);
    for py in 0..ph {
        for px in 0..pw {
            // Pixel centres, so the image is exactly symmetric about (x, y).
            let c_re = cx + (px as f64 + 0.5 - pw as f64 / 2.0) * step_x;
            let c_im = cy + (py as f64 + 0.5 - ph as f64 / 2.0) * step_y;
            let (mut zr, mut zi, mut n) = (0.0f64, 0.0f64, 0u32);
            while n < max_iter && zr * zr + zi * zi <= 256.0 {
                (zr, zi) = (zr * zr - zi * zi + c_re, 2.0 * zr * zi + c_im);
                n += 1;
            }
            let v = if n == max_iter {
                1.0
            } else {
                let smooth = f64::from(n) + 1.0 - (zr * zr + zi * zi).ln().ln() / std::f64::consts::LN_2;
                let t = (smooth.max(0.0) / f64::from(max_iter)).sqrt();
                // 1-bit renderers: drop the faint far-field glow, which dithering would otherwise
                // scatter as noise across the whole picture, and stretch what's left.
                if one_bit { ((t - 0.3) / 0.7).max(0.0) } else { t }
            };
            canvas.set(px as i64, py as i64, v as f32);
        }
    }
    if one_bit { canvas.dither(); }
    Ok(emit_art(ctx, canvas.render(mode)))
}

/// A circle that looks round: radii are corrected for the renderer's pixel aspect ratio.
pub fn ascii_circle(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let what = "ascii_circle";
    let r = width_arg(args, 0, kwargs, "radius", 8, line, what)? as f64;
    let fill = kw_bool(kwargs, "fill");
    let style = kw_str(kwargs, "style").unwrap_or_else(|| "braille".into());
    let mode = match style.as_str() {
        "ascii" => Render::Blocks { fill: kw_str(kwargs, "char").and_then(|c| c.chars().next()).unwrap_or('#'), half: '#', width: 1 },
        other => render_style(other, line, what)?,
    };
    // Sample at pixel centres, as a rasterizer does: pixel (x, y) covers [x, x+1) × [y, y+1).
    let (rx, ry) = (r, r / mode.pixel_aspect());
    let (w, h) = ((2.0 * rx).ceil().max(1.0) as usize, (2.0 * ry).ceil().max(1.0) as usize);
    let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
    let mut canvas = Canvas::new(w, h);
    if fill {
        for y in 0..h {
            for x in 0..w {
                let (dx, dy) = ((x as f64 + 0.5 - cx) / rx, (y as f64 + 0.5 - cy) / ry);
                if dx * dx + dy * dy <= 1.0 { canvas.set(x as i64, y as i64, 1.0); }
            }
        }
    } else {
        let steps = ((rx.max(ry) * 32.0).ceil() as usize).max(64);
        for i in 0..steps {
            let t = i as f64 / steps as f64 * std::f64::consts::TAU;
            let x = ((cx + (rx - 0.5) * t.cos()).floor() as i64).clamp(0, w as i64 - 1);
            let y = ((cy + (ry - 0.5) * t.sin()).floor() as i64).clamp(0, h as i64 - 1);
            canvas.set(x, y, 1.0);
        }
    }
    Ok(emit_art(ctx, canvas.render(mode)))
}

// ---------------- images ----------------

/// BMP / PBM / PGM / PPM file → art. Bright pixels become dense characters (lit dots), which
/// reads correctly on a dark terminal; `invert=true` for light backgrounds. 1-bit styles
/// (braille, half) are dithered unless `dither=false`.
pub fn ascii_image(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let what = "ascii_image";
    let path = str_arg(args, 0, line, what)?;
    let width = width_arg(args, 1, kwargs, "width", 80, line, what)?;
    let mode = render_style(&kw_str(kwargs, "style").unwrap_or_else(|| "ascii".into()), line, what)?;
    let bytes = std::fs::read(&path).map_err(|e| RuntimeError::new(404, line, format!("{what}: cannot read `{path}`: {e}")))?;
    let img = crate::stdlib::image::decode(&bytes).map_err(|e| RuntimeError::new(400, line, format!("{what}: `{path}`: {e}")))?;

    let (cell_w, _) = mode.cell();
    let pw = width * cell_w;
    let ph = ((img.h as f64 / img.w as f64) * pw as f64 / mode.pixel_aspect()).round().max(1.0) as usize;
    let mut px = crate::stdlib::image::resample(&img, pw, ph);
    if kw_bool(kwargs, "invert") { for v in &mut px { *v = 1.0 - *v; } }

    let mut canvas = Canvas::new(pw, ph);
    for (i, v) in px.iter().enumerate() { canvas.set((i % pw) as i64, (i / pw) as i64, *v); }
    let one_bit = matches!(mode, Render::Braille | Render::HalfBlocks);
    let dither = kwargs.get("dither").and_then(|v| v.first()).map_or(one_bit, Value::is_truthy);
    if dither { canvas.dither(); }
    Ok(emit_art(ctx, canvas.render(mode)))
}
