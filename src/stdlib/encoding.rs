//! Binary-to-text encodings, all hand-rolled (no crates): base64 / base64url (RFC 4648),
//! base32 (RFC 4648), base58 (Bitcoin alphabet), Ascii85 (Adobe), hex, binary, and URL
//! percent-encoding (RFC 3986).
//!
//! Encoders take text (or hex bytes with `input="hex"`); decoders return text, or hex with
//! `output="hex"` when the decoded bytes aren't UTF-8 — see `stdlib::args`.

use crate::ast::Value;
use crate::interpreter::{Ctx, RuntimeError};
use crate::stdlib::args::{bytes_to_hex, emit_text, hex_to_bytes, input_bytes, kw_bool, output_bytes, str_arg, Kwargs};

const B64_STD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const B64_URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
const B32: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
const B58: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

// ---------------- pure codecs ----------------

pub fn base64_encode_bytes(data: &[u8], url_safe: bool) -> String {
    let alphabet = if url_safe { B64_URL } else { B64_STD };
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], chunk.get(1).copied().unwrap_or(0), chunk.get(2).copied().unwrap_or(0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        let emitted = chunk.len() + 1;
        for i in 0..4 {
            if i < emitted {
                out.push(alphabet[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else if !url_safe {
                // base64url is conventionally unpadded (JWT, RFC 7515); standard keeps `=`.
                out.push('=');
            }
        }
    }
    out
}

/// Accepts both the standard and URL-safe alphabets, with or without `=` padding, and
/// ignores whitespace (so wrapped MIME-style base64 decodes too).
pub fn base64_decode_bytes(s: &str) -> Result<Vec<u8>, String> {
    let chars: Vec<u8> = s.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    let mut end = chars.len();
    while end > 0 && chars[end - 1] == b'=' { end -= 1; }
    let mut vals = Vec::with_capacity(end);
    for &c in &chars[..end] {
        vals.push(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return Err(format!("invalid base64 character `{}`", c as char)),
        });
    }
    if vals.len() % 4 == 1 {
        return Err("truncated base64 input (a single leftover character carries no full byte)".into());
    }
    let mut out = Vec::with_capacity(vals.len() * 3 / 4);
    for chunk in vals.chunks(4) {
        let mut n = 0u32;
        for (i, v) in chunk.iter().enumerate() { n |= u32::from(*v) << (18 - 6 * i); }
        for i in 0..chunk.len() * 6 / 8 { out.push((n >> (16 - 8 * i)) as u8); }
    }
    Ok(out)
}

pub fn base32_encode_bytes(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(5) * 8);
    for chunk in data.chunks(5) {
        let mut buf = [0u8; 5];
        buf[..chunk.len()].copy_from_slice(chunk);
        let n = buf.iter().fold(0u64, |acc, b| (acc << 8) | u64::from(*b));
        let emitted = (chunk.len() * 8).div_ceil(5);
        for i in 0..8 {
            out.push(if i < emitted { B32[((n >> (35 - 5 * i)) & 31) as usize] as char } else { '=' });
        }
    }
    out
}

pub fn base32_decode_bytes(s: &str) -> Result<Vec<u8>, String> {
    let mut vals = Vec::new();
    for c in s.bytes().filter(|b| !b.is_ascii_whitespace() && *b != b'=') {
        vals.push(match c.to_ascii_uppercase() {
            u @ b'A'..=b'Z' => u - b'A',
            d @ b'2'..=b'7' => d - b'2' + 26,
            _ => return Err(format!("invalid base32 character `{}`", c as char)),
        });
    }
    let mut out = Vec::with_capacity(vals.len() * 5 / 8);
    for chunk in vals.chunks(8) {
        if matches!(chunk.len(), 1 | 3 | 6) {
            return Err(format!("truncated base32 input (final group of {} characters is not a valid length)", chunk.len()));
        }
        let mut n = 0u64;
        for (i, v) in chunk.iter().enumerate() { n |= u64::from(*v) << (35 - 5 * i); }
        for i in 0..chunk.len() * 5 / 8 { out.push((n >> (32 - 8 * i)) as u8); }
    }
    Ok(out)
}

pub fn base58_encode_bytes(data: &[u8]) -> String {
    let zeros = data.iter().take_while(|b| **b == 0).count();
    // Little-endian base-58 digits of the big-endian number `data`.
    let mut digits: Vec<u8> = Vec::new();
    for &byte in &data[zeros..] {
        let mut carry = u32::from(byte);
        for d in &mut digits {
            carry += u32::from(*d) << 8;
            *d = (carry % 58) as u8;
            carry /= 58;
        }
        while carry > 0 {
            digits.push((carry % 58) as u8);
            carry /= 58;
        }
    }
    let mut out = "1".repeat(zeros);
    out.extend(digits.iter().rev().map(|d| B58[*d as usize] as char));
    out
}

pub fn base58_decode_bytes(s: &str) -> Result<Vec<u8>, String> {
    let s = s.trim();
    let zeros = s.bytes().take_while(|c| *c == b'1').count();
    let mut bytes: Vec<u8> = Vec::new(); // little-endian base-256
    for c in s.bytes().skip(zeros) {
        let v = B58.iter().position(|a| *a == c)
            .ok_or_else(|| format!("invalid base58 character `{}` (base58 omits 0, O, I and l)", c as char))?;
        let mut carry = v as u32;
        for b in &mut bytes {
            carry += u32::from(*b) * 58;
            *b = (carry & 0xff) as u8;
            carry >>= 8;
        }
        while carry > 0 {
            bytes.push((carry & 0xff) as u8);
            carry >>= 8;
        }
    }
    let mut out = vec![0u8; zeros];
    out.extend(bytes.iter().rev());
    Ok(out)
}

/// Adobe Ascii85 without the `<~ ~>` delimiters (matches Python's `base64.a85encode`),
/// with the `z` shorthand for an all-zero 4-byte group.
pub fn ascii85_encode_bytes(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(4) * 5);
    for chunk in data.chunks(4) {
        let mut buf = [0u8; 4];
        buf[..chunk.len()].copy_from_slice(chunk);
        let n = u32::from_be_bytes(buf);
        if chunk.len() == 4 && n == 0 {
            out.push('z');
            continue;
        }
        let mut enc = [0u8; 5];
        let mut v = n;
        for slot in enc.iter_mut().rev() {
            *slot = (v % 85) as u8 + b'!';
            v /= 85;
        }
        out.extend(enc[..=chunk.len()].iter().map(|b| *b as char));
    }
    out
}

pub fn ascii85_decode_bytes(s: &str) -> Result<Vec<u8>, String> {
    let mut body: &str = s.trim();
    if let Some(rest) = body.strip_prefix("<~") { body = rest; }
    if let Some(rest) = body.strip_suffix("~>") { body = rest; }
    let mut out = Vec::new();
    let mut group: Vec<u8> = Vec::with_capacity(5);
    let flush = |group: &[u8], out: &mut Vec<u8>| -> Result<(), String> {
        let mut padded = [b'u'; 5];
        padded[..group.len()].copy_from_slice(group);
        let n = padded.iter().try_fold(0u64, |acc, c| Ok::<u64, String>(acc * 85 + u64::from(c - b'!')))?;
        let n = u32::try_from(n).map_err(|_| "ascii85 group overflows 32 bits".to_string())?;
        out.extend_from_slice(&n.to_be_bytes()[..group.len() - 1]);
        Ok(())
    };
    for c in body.bytes().filter(|b| !b.is_ascii_whitespace()) {
        match c {
            b'z' if group.is_empty() => out.extend_from_slice(&[0, 0, 0, 0]),
            b'z' => return Err("ascii85 `z` shorthand is only valid between groups".into()),
            b'!'..=b'u' => {
                group.push(c);
                if group.len() == 5 {
                    flush(&group, &mut out)?;
                    group.clear();
                }
            }
            _ => return Err(format!("invalid ascii85 character `{}`", c as char)),
        }
    }
    match group.len() {
        0 => {}
        1 => return Err("truncated ascii85 input (a single leftover character carries no full byte)".into()),
        _ => flush(&group, &mut out)?,
    }
    Ok(out)
}

pub fn binary_encode_bytes(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:08b}")).collect::<Vec<_>>().join(" ")
}

pub fn binary_decode_bytes(s: &str) -> Result<Vec<u8>, String> {
    let bits: Vec<u8> = s.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if !bits.len().is_multiple_of(8) {
        return Err(format!("binary input has {} bits, not a multiple of 8", bits.len()));
    }
    bits.chunks(8).map(|octet| {
        octet.iter().try_fold(0u8, |acc, bit| match bit {
            b'0' => Ok(acc << 1),
            b'1' => Ok((acc << 1) | 1),
            other => Err(format!("invalid binary digit `{}`", *other as char)),
        })
    }).collect()
}

/// RFC 3986: everything except the unreserved set `A-Z a-z 0-9 - _ . ~` is escaped.
pub fn url_encode_bytes(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len());
    for &b in data {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `form=true` also turns `+` into a space (application/x-www-form-urlencoded, i.e. query
/// strings); plain RFC 3986 leaves `+` alone.
pub fn url_decode_bytes(s: &str, form: bool) -> Result<Vec<u8>, String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                let hex = bytes.get(i + 1..i + 3)
                    .ok_or_else(|| format!("truncated percent-escape at byte {i}"))?;
                let decoded = hex_to_bytes(std::str::from_utf8(hex).map_err(|_| format!("bad percent-escape at byte {i}"))?)
                    .map_err(|e| format!("bad percent-escape at byte {i}: {e}"))?;
                out.extend(decoded);
                i += 3;
            }
            b'+' if form => { out.push(b' '); i += 1; }
            b => { out.push(b); i += 1; }
        }
    }
    Ok(out)
}

// ---------------- Rach-facing commands ----------------

fn decoded(result: Result<Vec<u8>, String>, kwargs: &Kwargs, line: usize, what: &str, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let bytes = result.map_err(|e| RuntimeError::new(400, line, format!("{what}: {e}")))?;
    Ok(emit_text(ctx, what, output_bytes(bytes, kwargs, line, what)?))
}

pub fn base64_encode(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let data = input_bytes(args, kwargs, line, "base64_encode")?;
    Ok(emit_text(ctx, "base64_encode", base64_encode_bytes(&data, kw_bool(kwargs, "url"))))
}

pub fn base64_decode(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let s = str_arg(args, 0, line, "base64_decode")?;
    decoded(base64_decode_bytes(&s), kwargs, line, "base64_decode", ctx)
}

pub fn base32_encode(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let data = input_bytes(args, kwargs, line, "base32_encode")?;
    Ok(emit_text(ctx, "base32_encode", base32_encode_bytes(&data)))
}

pub fn base32_decode(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let s = str_arg(args, 0, line, "base32_decode")?;
    decoded(base32_decode_bytes(&s), kwargs, line, "base32_decode", ctx)
}

pub fn base58_encode(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let data = input_bytes(args, kwargs, line, "base58_encode")?;
    Ok(emit_text(ctx, "base58_encode", base58_encode_bytes(&data)))
}

pub fn base58_decode(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let s = str_arg(args, 0, line, "base58_decode")?;
    decoded(base58_decode_bytes(&s), kwargs, line, "base58_decode", ctx)
}

pub fn ascii85_encode(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let data = input_bytes(args, kwargs, line, "ascii85_encode")?;
    Ok(emit_text(ctx, "ascii85_encode", ascii85_encode_bytes(&data)))
}

pub fn ascii85_decode(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let s = str_arg(args, 0, line, "ascii85_decode")?;
    decoded(ascii85_decode_bytes(&s), kwargs, line, "ascii85_decode", ctx)
}

pub fn hex_encode(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let data = input_bytes(args, kwargs, line, "hex_encode")?;
    Ok(emit_text(ctx, "hex_encode", bytes_to_hex(&data)))
}

pub fn hex_decode(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let s = str_arg(args, 0, line, "hex_decode")?;
    decoded(hex_to_bytes(&s), kwargs, line, "hex_decode", ctx)
}

pub fn binary_encode(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let data = input_bytes(args, kwargs, line, "binary_encode")?;
    Ok(emit_text(ctx, "binary_encode", binary_encode_bytes(&data)))
}

pub fn binary_decode(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let s = str_arg(args, 0, line, "binary_decode")?;
    decoded(binary_decode_bytes(&s), kwargs, line, "binary_decode", ctx)
}

pub fn url_encode(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let data = input_bytes(args, kwargs, line, "url_encode")?;
    Ok(emit_text(ctx, "url_encode", url_encode_bytes(&data)))
}

pub fn url_decode(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let s = str_arg(args, 0, line, "url_decode")?;
    decoded(url_decode_bytes(&s, kw_bool(kwargs, "form")), kwargs, line, "url_decode", ctx)
}
