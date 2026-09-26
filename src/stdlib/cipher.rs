//! Classical ciphers, hand-rolled. Historical and educational — every one of these falls to
//! pen-and-paper cryptanalysis (see `vigenere_crack` for the method Charles Babbage used on
//! Vigenère in the 1850s). Use `stdlib::modern` for anything that needs to stay secret.
//!
//! Substitution ciphers work over an alphabet chosen with `alphabet="en"` (default, 26
//! letters) or `alphabet="ru"` (33 letters, Ё included). Case is preserved and characters
//! outside the alphabet pass through untouched without consuming key letters.

use crate::ast::Value;
use crate::interpreter::{Ctx, RuntimeError};
use crate::stdlib::args::{emit_text, int_arg, kw_str, str_arg, Kwargs};

pub struct Alphabet {
    upper: Vec<char>,
    lower: Vec<char>,
}

impl Alphabet {
    pub fn english() -> Self {
        Alphabet { upper: ('A'..='Z').collect(), lower: ('a'..='z').collect() }
    }

    pub fn russian() -> Self {
        Alphabet {
            upper: "АБВГДЕЁЖЗИЙКЛМНОПРСТУФХЦЧШЩЪЫЬЭЮЯ".chars().collect(),
            lower: "абвгдеёжзийклмнопрстуфхцчшщъыьэюя".chars().collect(),
        }
    }

    pub fn from_kwargs(kwargs: &Kwargs, line: usize, what: &str) -> Result<Self, RuntimeError> {
        match kw_str(kwargs, "alphabet").as_deref() {
            None | Some("en") => Ok(Self::english()),
            Some("ru") => Ok(Self::russian()),
            Some(other) => Err(RuntimeError::new(400, line, format!("{what}: alphabet must be \"en\" or \"ru\", got \"{other}\""))),
        }
    }

    pub fn len(&self) -> usize { self.upper.len() }

    /// `(index, is_uppercase)` for a letter of this alphabet.
    pub fn index(&self, c: char) -> Option<(usize, bool)> {
        self.upper.iter().position(|u| *u == c).map(|i| (i, true))
            .or_else(|| self.lower.iter().position(|l| *l == c).map(|i| (i, false)))
    }

    pub fn letter(&self, i: usize, upper: bool) -> char {
        if upper { self.upper[i] } else { self.lower[i] }
    }

    /// Key letters as alphabet indices, ignoring anything that isn't a letter.
    fn key_indices(&self, key: &str, line: usize, what: &str) -> Result<Vec<usize>, RuntimeError> {
        let k: Vec<usize> = key.chars().filter_map(|c| self.index(c).map(|(i, _)| i)).collect();
        if k.is_empty() {
            return Err(RuntimeError::new(400, line, format!("{what}: key must contain at least one letter of the alphabet")));
        }
        Ok(k)
    }
}

/// Apply `f(letter_index, position_among_letters)` to every alphabet letter, keeping case and
/// passing everything else through.
fn map_letters(text: &str, abc: &Alphabet, mut f: impl FnMut(usize, usize) -> usize) -> String {
    let mut n = 0;
    text.chars().map(|c| match abc.index(c) {
        Some((i, upper)) => {
            let out = abc.letter(f(i, n) % abc.len(), upper);
            n += 1;
            out
        }
        None => c,
    }).collect()
}

fn modulo(x: i64, m: usize) -> usize {
    x.rem_euclid(m as i64) as usize
}

pub fn caesar(text: &str, shift: i64, abc: &Alphabet) -> String {
    let s = modulo(shift, abc.len());
    map_letters(text, abc, |i, _| i + s)
}

pub fn atbash(text: &str, abc: &Alphabet) -> String {
    let n = abc.len();
    map_letters(text, abc, |i, _| n - 1 - i)
}

fn gcd(a: usize, b: usize) -> usize { if b == 0 { a } else { gcd(b, a % b) } }

fn mod_inverse(a: usize, m: usize) -> Option<usize> {
    (1..m).find(|x| (a * x) % m == 1)
}

pub fn affine(text: &str, a: i64, b: i64, decrypt: bool, abc: &Alphabet) -> Result<String, String> {
    let n = abc.len();
    let a = modulo(a, n);
    let b = modulo(b, n);
    if gcd(a, n) != 1 {
        return Err(format!("`a` must be coprime with the alphabet size {n} (got {a}), otherwise the cipher can't be inverted"));
    }
    if decrypt {
        let inv = mod_inverse(a, n).expect("coprime implies invertible");
        Ok(map_letters(text, abc, |i, _| inv * (i + n - b)))
    } else {
        Ok(map_letters(text, abc, |i, _| a * i + b))
    }
}

pub fn vigenere(text: &str, key: &[usize], decrypt: bool, abc: &Alphabet) -> String {
    let n = abc.len();
    map_letters(text, abc, |i, pos| {
        let k = key[pos % key.len()];
        if decrypt { i + n - k } else { i + k }
    })
}

/// Beaufort (c = k − p) is its own inverse: the same call encrypts and decrypts.
pub fn beaufort(text: &str, key: &[usize], abc: &Alphabet) -> String {
    let n = abc.len();
    map_letters(text, abc, |i, pos| key[pos % key.len()] + n - i)
}

/// Autokey: the keystream is the key followed by the plaintext itself, so there's no
/// repeating period for Kasiski/Babbage to find.
pub fn autokey(text: &str, key: &[usize], decrypt: bool, abc: &Alphabet) -> String {
    let n = abc.len();
    let mut stream: Vec<usize> = key.to_vec();
    map_letters(text, abc, |i, pos| {
        let k = stream[pos];
        let (out, plain) = if decrypt { let p = (i + n - k) % n; (p, p) } else { ((i + k) % n, i) };
        stream.push(plain);
        out
    })
}

pub fn rot47(text: &str) -> String {
    text.chars().map(|c| match c {
        '!'..='~' => char::from(b'!' + (c as u8 - b'!' + 47) % 94),
        _ => c,
    }).collect()
}

// ---------------- Rach-facing commands ----------------

pub fn caesar_encrypt(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let text = str_arg(args, 0, line, "caesar_encrypt")?;
    let shift = int_arg(args, 1, kwargs, "shift", 3, line, "caesar_encrypt")?;
    let abc = Alphabet::from_kwargs(kwargs, line, "caesar_encrypt")?;
    Ok(emit_text(ctx, "caesar_encrypt", caesar(&text, shift, &abc)))
}

pub fn caesar_decrypt(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let text = str_arg(args, 0, line, "caesar_decrypt")?;
    let shift = int_arg(args, 1, kwargs, "shift", 3, line, "caesar_decrypt")?;
    let abc = Alphabet::from_kwargs(kwargs, line, "caesar_decrypt")?;
    Ok(emit_text(ctx, "caesar_decrypt", caesar(&text, -shift, &abc)))
}

pub fn rot13(args: &[Value], _kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let text = str_arg(args, 0, line, "rot13")?;
    Ok(emit_text(ctx, "rot13", caesar(&text, 13, &Alphabet::english())))
}

pub fn rot47_cmd(args: &[Value], _kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let text = str_arg(args, 0, line, "rot47")?;
    Ok(emit_text(ctx, "rot47", rot47(&text)))
}

pub fn atbash_cmd(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let text = str_arg(args, 0, line, "atbash")?;
    let abc = Alphabet::from_kwargs(kwargs, line, "atbash")?;
    Ok(emit_text(ctx, "atbash", atbash(&text, &abc)))
}

fn affine_cmd(what: &str, decrypt: bool, args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let text = str_arg(args, 0, line, what)?;
    let a = int_arg(args, 1, kwargs, "a", 5, line, what)?;
    let b = int_arg(args, 2, kwargs, "b", 8, line, what)?;
    let abc = Alphabet::from_kwargs(kwargs, line, what)?;
    let out = affine(&text, a, b, decrypt, &abc).map_err(|e| RuntimeError::new(400, line, format!("{what}: {e}")))?;
    Ok(emit_text(ctx, what, out))
}

pub fn affine_encrypt(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    affine_cmd("affine_encrypt", false, args, kwargs, line, ctx)
}

pub fn affine_decrypt(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    affine_cmd("affine_decrypt", true, args, kwargs, line, ctx)
}

fn keyed_cmd(
    what: &str,
    args: &[Value],
    kwargs: &Kwargs,
    line: usize,
    ctx: &Ctx,
    f: impl Fn(&str, &[usize], &Alphabet) -> String,
) -> Result<Value, RuntimeError> {
    let text = str_arg(args, 0, line, what)?;
    let key = str_arg(args, 1, line, what)?;
    let abc = Alphabet::from_kwargs(kwargs, line, what)?;
    let k = abc.key_indices(&key, line, what)?;
    Ok(emit_text(ctx, what, f(&text, &k, &abc)))
}

pub fn vigenere_encrypt(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    keyed_cmd("vigenere_encrypt", args, kwargs, line, ctx, |t, k, a| vigenere(t, k, false, a))
}

pub fn vigenere_decrypt(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    keyed_cmd("vigenere_decrypt", args, kwargs, line, ctx, |t, k, a| vigenere(t, k, true, a))
}

pub fn beaufort_cmd(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    keyed_cmd("beaufort", args, kwargs, line, ctx, beaufort)
}

pub fn autokey_encrypt(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    keyed_cmd("autokey_encrypt", args, kwargs, line, ctx, |t, k, a| autokey(t, k, false, a))
}

pub fn autokey_decrypt(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    keyed_cmd("autokey_decrypt", args, kwargs, line, ctx, |t, k, a| autokey(t, k, true, a))
}
