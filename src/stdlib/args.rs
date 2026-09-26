//! Argument helpers shared by the byte-oriented stdlib modules (encoding, hashing, ciphers,
//! ASCII rendering). Everything that takes "bytes" accepts text by default, or hex when the
//! caller passes `input="hex"`, and everything that returns raw bytes can hand them back as
//! text (when valid UTF-8) or as hex via `output="hex"` — Rach has no byte-string type, so hex
//! is how binary data round-trips between these functions.

use std::collections::BTreeMap;

use crate::ast::Value;
use crate::interpreter::{Ctx, RuntimeError};

pub type Kwargs = BTreeMap<String, Vec<Value>>;

pub fn str_arg(args: &[Value], n: usize, line: usize, what: &str) -> Result<String, RuntimeError> {
    args.get(n)
        .map(Value::as_str)
        .ok_or_else(|| RuntimeError::new(400, line, format!("{what} requires argument #{}", n + 1)))
}

pub fn opt_str(args: &[Value], n: usize, kwargs: &Kwargs, key: &str) -> Option<String> {
    args.get(n).map(Value::as_str).or_else(|| kw_str(kwargs, key))
}

pub fn int_arg(args: &[Value], n: usize, kwargs: &Kwargs, key: &str, default: i64, line: usize, what: &str) -> Result<i64, RuntimeError> {
    let v = args.get(n).cloned().or_else(|| kwargs.get(key).and_then(|v| v.first().cloned()));
    match v {
        None => Ok(default),
        Some(v) => v.as_f64()
            .filter(|f| f.fract() == 0.0)
            .map(|f| f as i64)
            .ok_or_else(|| RuntimeError::new(400, line, format!("{what}: `{key}` must be an integer, got {}", v.as_str()))),
    }
}

pub fn kw_str(kwargs: &Kwargs, key: &str) -> Option<String> {
    kwargs.get(key).and_then(|v| v.first()).map(Value::as_str)
}

pub fn kw_bool(kwargs: &Kwargs, key: &str) -> bool {
    kwargs.get(key).and_then(|v| v.first()).is_some_and(Value::is_truthy)
}

/// First positional argument as bytes: UTF-8 text, or hex when `input="hex"`.
pub fn input_bytes(args: &[Value], kwargs: &Kwargs, line: usize, what: &str) -> Result<Vec<u8>, RuntimeError> {
    let s = str_arg(args, 0, line, what)?;
    match kw_str(kwargs, "input").as_deref() {
        Some("hex") => hex_to_bytes(&s).map_err(|e| RuntimeError::new(400, line, format!("{what}: {e}"))),
        Some("text") | None => Ok(s.into_bytes()),
        Some(other) => Err(RuntimeError::new(400, line, format!("{what}: input must be \"text\" or \"hex\", got \"{other}\""))),
    }
}

/// Raw bytes back to a Rach value: text when they're valid UTF-8, hex when `output="hex"`.
pub fn output_bytes(bytes: Vec<u8>, kwargs: &Kwargs, line: usize, what: &str) -> Result<String, RuntimeError> {
    match kw_str(kwargs, "output").as_deref() {
        Some("hex") => Ok(bytes_to_hex(&bytes)),
        Some("text") | None => String::from_utf8(bytes).map_err(|_| RuntimeError::new(400, line, format!(
            "{what}: result is binary, not UTF-8 text — pass output=\"hex\" to get it as hex"
        ))),
        Some(other) => Err(RuntimeError::new(400, line, format!("{what}: output must be \"text\" or \"hex\", got \"{other}\""))),
    }
}

pub fn bytes_to_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(DIGITS[(b >> 4) as usize] as char);
        s.push(DIGITS[(b & 0x0f) as usize] as char);
    }
    s
}

pub fn hex_to_bytes(s: &str) -> Result<Vec<u8>, String> {
    let digits: Vec<u8> = s.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if !digits.len().is_multiple_of(2) {
        return Err(format!("hex input has an odd number of digits ({})", digits.len()));
    }
    let nibble = |c: u8| -> Result<u8, String> {
        match c {
            b'0'..=b'9' => Ok(c - b'0'),
            b'a'..=b'f' => Ok(c - b'a' + 10),
            b'A'..=b'F' => Ok(c - b'A' + 10),
            _ => Err(format!("invalid hex digit `{}`", c as char)),
        }
    };
    digits.chunks(2).map(|p| Ok((nibble(p[0])? << 4) | nibble(p[1])?)).collect()
}

/// Print `label: value` + `completed` unless the call's result is being captured into a
/// variable — the same convention `math`/`collections` use.
pub fn emit_text(ctx: &Ctx, label: &str, s: String) -> Value {
    if !ctx.capturing {
        println!("{label}: {s}");
        println!("completed");
    }
    Value::Str(s)
}

/// Multi-line art is printed as-is (no `label:` prefix, which would misalign line 1).
pub fn emit_art(ctx: &Ctx, s: String) -> Value {
    if !ctx.capturing {
        println!("{s}");
        println!("completed");
    }
    Value::Str(s)
}
