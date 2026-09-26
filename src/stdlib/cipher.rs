//! Classical ciphers, hand-rolled. Historical and educational — every one of these falls to
//! pen-and-paper cryptanalysis (see `vigenere_crack` for the method Charles Babbage used on
//! Vigenère in the 1850s). Use `stdlib::modern` for anything that needs to stay secret.
//!
//! Substitution ciphers work over an alphabet chosen with `alphabet="en"` (default, 26
//! letters) or `alphabet="ru"` (33 letters, Ё included). Case is preserved and characters
//! outside the alphabet pass through untouched without consuming key letters.

use std::collections::BTreeMap;

use crate::ast::Value;
use crate::interpreter::{Ctx, RuntimeError};
use crate::stdlib::args::{emit_text, int_arg, kw_str, str_arg, Kwargs};

pub struct Alphabet {
    upper: Vec<char>,
    lower: Vec<char>,
    /// Relative letter frequencies of the language, in alphabet order (for cryptanalysis).
    freq: &'static [f64],
}

/// English letter frequencies (Lewand, *Cryptological Mathematics*).
const EN_FREQ: [f64; 26] = [
    0.08167, 0.01492, 0.02782, 0.04253, 0.12702, 0.02228, 0.02015, 0.06094, 0.06966, 0.00153,
    0.00772, 0.04025, 0.02406, 0.06749, 0.07507, 0.01929, 0.00095, 0.05987, 0.06327, 0.09056,
    0.02758, 0.00978, 0.02360, 0.00150, 0.01974, 0.00074,
];

/// Russian letter frequencies (Национальный корпус русского языка), А..Я with Ё after Е.
const RU_FREQ: [f64; 33] = [
    0.0801, 0.0159, 0.0454, 0.0170, 0.0298, 0.0845, 0.0004, 0.0094, 0.0165, 0.0735, 0.0121,
    0.0349, 0.0440, 0.0321, 0.0670, 0.1097, 0.0281, 0.0473, 0.0547, 0.0626, 0.0262, 0.0026,
    0.0097, 0.0048, 0.0144, 0.0073, 0.0036, 0.0004, 0.0190, 0.0174, 0.0032, 0.0064, 0.0201,
];

impl Alphabet {
    pub fn english() -> Self {
        Alphabet { upper: ('A'..='Z').collect(), lower: ('a'..='z').collect(), freq: &EN_FREQ }
    }

    pub fn russian() -> Self {
        Alphabet {
            upper: "АБВГДЕЁЖЗИЙКЛМНОПРСТУФХЦЧШЩЪЫЬЭЮЯ".chars().collect(),
            lower: "абвгдеёжзийклмнопрстуфхцчшщъыьэюя".chars().collect(),
            freq: &RU_FREQ,
        }
    }

    pub fn from_kwargs(kwargs: &Kwargs, line: usize, what: &str) -> Result<Self, RuntimeError> {
        match kw_str(kwargs, "alphabet").as_deref() {
            None | Some("en") => Ok(Self::english()),
            Some("ru") => Ok(Self::russian()),
            Some(other) => Err(RuntimeError::new(400, line, format!("{what}: alphabet must be \"en\" or \"ru\", got \"{other}\""))),
        }
    }

    pub fn size(&self) -> usize { self.upper.len() }

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
            let out = abc.letter(f(i, n) % abc.size(), upper);
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
    let s = modulo(shift, abc.size());
    map_letters(text, abc, |i, _| i + s)
}

pub fn atbash(text: &str, abc: &Alphabet) -> String {
    let n = abc.size();
    map_letters(text, abc, |i, _| n - 1 - i)
}

fn gcd(a: usize, b: usize) -> usize { if b == 0 { a } else { gcd(b, a % b) } }

fn mod_inverse(a: usize, m: usize) -> Option<usize> {
    (1..m).find(|x| (a * x) % m == 1)
}

pub fn affine(text: &str, a: i64, b: i64, decrypt: bool, abc: &Alphabet) -> Result<String, String> {
    let n = abc.size();
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
    let n = abc.size();
    map_letters(text, abc, |i, pos| {
        let k = key[pos % key.len()];
        if decrypt { i + n - k } else { i + k }
    })
}

/// Beaufort (c = k − p) is its own inverse: the same call encrypts and decrypts.
pub fn beaufort(text: &str, key: &[usize], abc: &Alphabet) -> String {
    let n = abc.size();
    map_letters(text, abc, |i, pos| key[pos % key.len()] + n - i)
}

/// Autokey: the keystream is the key followed by the plaintext itself, so there's no
/// repeating period for Kasiski/Babbage to find.
pub fn autokey(text: &str, key: &[usize], decrypt: bool, abc: &Alphabet) -> String {
    let n = abc.size();
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

// ---------------- polygraphic, fractionating & transposition ciphers ----------------

const SQUARE25: &str = "ABCDEFGHIKLMNOPQRSTUVWXYZ";
const SQUARE36: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

/// Keyed Polybius square: key characters first (deduplicated), then the rest of the alphabet.
/// The 5×5 variant folds J into I, as Wheatstone's Playfair and Delastelle's Bifid do.
fn keyed_square(key: &str, alphabet: &str) -> Vec<char> {
    let fold = alphabet.len() == 25;
    let mut sq: Vec<char> = Vec::with_capacity(alphabet.len());
    for c in key.chars().flat_map(char::to_uppercase).chain(alphabet.chars()) {
        let c = if fold && c == 'J' { 'I' } else { c };
        if alphabet.contains(c) && !sq.contains(&c) { sq.push(c); }
    }
    sq
}

fn square_letters(text: &str) -> Vec<char> {
    text.chars().flat_map(char::to_uppercase)
        .filter(char::is_ascii_alphabetic)
        .map(|c| if c == 'J' { 'I' } else { c })
        .collect()
}

fn pos5(sq: &[char], c: char) -> (usize, usize) {
    let i = sq.iter().position(|x| *x == c).expect("letter is in the square");
    (i / 5, i % 5)
}

/// Playfair digraphs: a doubled letter is split with X (Q when the letter is X itself), and an
/// odd trailing letter is padded the same way. Decryption can't know which X's were filler, so
/// they stay in the output.
pub fn playfair(text: &str, key: &str, decrypt: bool) -> Result<String, String> {
    let sq = keyed_square(key, SQUARE25);
    let t = square_letters(text);
    let filler = |a: char| if a == 'X' { 'Q' } else { 'X' };
    let mut pairs: Vec<(char, char)> = Vec::new();
    if decrypt {
        if t.len() % 2 == 1 { return Err("ciphertext must have an even number of letters".into()); }
        pairs.extend(t.chunks(2).map(|p| (p[0], p[1])));
    } else {
        let mut i = 0;
        while i < t.len() {
            match t.get(i + 1) {
                Some(&b) if b != t[i] => { pairs.push((t[i], b)); i += 2; }
                _ => { pairs.push((t[i], filler(t[i]))); i += 1; }
            }
        }
    }
    let step = if decrypt { 4 } else { 1 };
    let mut out = String::with_capacity(pairs.len() * 2);
    for (a, b) in pairs {
        let ((r1, c1), (r2, c2)) = (pos5(&sq, a), pos5(&sq, b));
        let (x, y) = if r1 == r2 {
            ((r1, (c1 + step) % 5), (r2, (c2 + step) % 5))
        } else if c1 == c2 {
            (((r1 + step) % 5, c1), ((r2 + step) % 5, c2))
        } else {
            ((r1, c2), (r2, c1))
        };
        out.push(sq[x.0 * 5 + x.1]);
        out.push(sq[y.0 * 5 + y.1]);
    }
    Ok(out)
}

/// Zig-zag row index for each position of a `len`-character message on `rails` rails.
fn rail_pattern(len: usize, rails: usize) -> Vec<usize> {
    let cycle = 2 * (rails - 1);
    (0..len).map(|i| if rails == 1 { 0 } else { let p = i % cycle; if p < rails { p } else { cycle - p } }).collect()
}

pub fn rail_fence(text: &str, rails: usize, decrypt: bool) -> String {
    let chars: Vec<char> = text.chars().collect();
    let pattern = rail_pattern(chars.len(), rails);
    // Positions in the order the ciphertext lists them: rail by rail, left to right.
    let mut order: Vec<usize> = (0..chars.len()).collect();
    order.sort_by_key(|&i| (pattern[i], i));
    let mut out = vec![' '; chars.len()];
    for (k, &i) in order.iter().enumerate() {
        if decrypt { out[i] = chars[k]; } else { out[k] = chars[i]; }
    }
    out.into_iter().collect()
}

/// Irregular columnar transposition: write row by row under the key, read the columns in
/// alphabetical key order (ties broken left to right). No padding.
pub fn columnar(text: &str, key: &str, decrypt: bool) -> String {
    let chars: Vec<char> = text.chars().collect();
    let key: Vec<char> = key.chars().flat_map(char::to_uppercase).collect();
    let k = key.len();
    let mut order: Vec<usize> = (0..k).collect();
    order.sort_by_key(|&i| (key[i], i));
    // Grid position (row-major index) of every ciphertext character, in ciphertext order.
    let positions: Vec<usize> = order.iter().flat_map(|&col| (col..chars.len()).step_by(k)).collect();
    let mut out = vec![' '; chars.len()];
    for (n, &p) in positions.iter().enumerate() {
        if decrypt { out[p] = chars[n]; } else { out[n] = chars[p]; }
    }
    out.into_iter().collect()
}

pub fn polybius_encode(text: &str, key: &str) -> String {
    let sq = keyed_square(key, SQUARE25);
    let mut tokens: Vec<String> = Vec::new();
    for c in text.chars().flat_map(char::to_uppercase) {
        if c.is_ascii_alphabetic() {
            let (r, col) = pos5(&sq, if c == 'J' { 'I' } else { c });
            tokens.push(format!("{}{}", r + 1, col + 1));
        } else if c.is_whitespace() && tokens.last().is_some_and(|t| t != "/") {
            tokens.push("/".into());
        }
    }
    while tokens.last().is_some_and(|t| t == "/") { tokens.pop(); }
    tokens.join(" ")
}

pub fn polybius_decode(text: &str, key: &str) -> Result<String, String> {
    let sq = keyed_square(key, SQUARE25);
    let mut out = String::new();
    for tok in text.split_whitespace() {
        if tok == "/" { out.push(' '); continue; }
        let d: Vec<u32> = tok.chars().map(|c| c.to_digit(10).unwrap_or(0)).collect();
        match d.as_slice() {
            [r @ 1..=5, c @ 1..=5] => out.push(sq[((r - 1) * 5 + (c - 1)) as usize]),
            _ => return Err(format!("`{tok}` is not a row/column pair 11..55")),
        }
    }
    Ok(out)
}

/// Delastelle's Bifid (1901) over the whole message: write each letter's row and column
/// coordinates on two lines, then read the coordinates back in pairs.
pub fn bifid(text: &str, key: &str, decrypt: bool) -> String {
    let sq = keyed_square(key, SQUARE25);
    let t = square_letters(text);
    let coords: Vec<(usize, usize)> = t.iter().map(|&c| pos5(&sq, c)).collect();
    let n = coords.len();
    if decrypt {
        let flat: Vec<usize> = coords.iter().flat_map(|&(r, c)| [r, c]).collect();
        (0..n).map(|i| sq[flat[i] * 5 + flat[n + i]]).collect()
    } else {
        let flat: Vec<usize> = coords.iter().map(|p| p.0).chain(coords.iter().map(|p| p.1)).collect();
        flat.chunks(2).map(|p| sq[p[0] * 5 + p[1]]).collect()
    }
}

const ADFGVX: [char; 6] = ['A', 'D', 'F', 'G', 'V', 'X'];

/// The German Army's ADFGVX field cipher (Nebel, 1918): a keyed 6×6 substitution onto the
/// letters A D F G V X, followed by a columnar transposition under a second key.
pub fn adfgvx(text: &str, square_key: &str, transposition_key: &str, decrypt: bool) -> Result<String, String> {
    let sq = keyed_square(square_key, SQUARE36);
    if decrypt {
        let frac = columnar(&text.chars().filter(|c| !c.is_whitespace()).collect::<String>(), transposition_key, true);
        let labels: Vec<char> = frac.chars().collect();
        if labels.len() % 2 == 1 { return Err("ciphertext must have an even number of letters".into()); }
        labels.chunks(2).map(|p| {
            let r = ADFGVX.iter().position(|l| *l == p[0].to_ascii_uppercase());
            let c = ADFGVX.iter().position(|l| *l == p[1].to_ascii_uppercase());
            match (r, c) {
                (Some(r), Some(c)) => Ok(sq[r * 6 + c].to_ascii_lowercase()),
                _ => Err(format!("`{}{}` isn't an ADFGVX pair", p[0], p[1])),
            }
        }).collect()
    } else {
        let frac: String = text.chars().flat_map(char::to_uppercase)
            .filter_map(|c| sq.iter().position(|x| *x == c))
            .flat_map(|i| [ADFGVX[i / 6], ADFGVX[i % 6]])
            .collect();
        Ok(columnar(&frac, transposition_key, false))
    }
}

/// Francis Bacon's biliteral cipher (1605), 26-letter variant: each letter as five A/B symbols.
pub fn bacon_encode(text: &str) -> String {
    text.chars().filter(char::is_ascii_alphabetic).map(|c| {
        let n = c.to_ascii_uppercase() as u8 - b'A';
        (0..5).rev().map(|bit| if n >> bit & 1 == 1 { 'B' } else { 'A' }).collect::<String>()
    }).collect::<Vec<_>>().join(" ")
}

pub fn bacon_decode(text: &str) -> Result<String, String> {
    let symbols: Vec<u8> = text.chars().filter_map(|c| match c.to_ascii_uppercase() {
        'A' => Some(0), 'B' => Some(1), _ => None,
    }).collect();
    if !symbols.len().is_multiple_of(5) {
        return Err(format!("{} A/B symbols is not a multiple of 5", symbols.len()));
    }
    symbols.chunks(5).map(|g| {
        let n = g.iter().fold(0u8, |acc, b| (acc << 1) | b);
        if n < 26 { Ok(char::from(b'A' + n)) } else { Err(format!("group value {n} is past Z")) }
    }).collect()
}

const MORSE_LATIN: &[(char, &str)] = &[
    ('A', ".-"), ('B', "-..."), ('C', "-.-."), ('D', "-.."), ('E', "."), ('F', "..-."), ('G', "--."),
    ('H', "...."), ('I', ".."), ('J', ".---"), ('K', "-.-"), ('L', ".-.."), ('M', "--"), ('N', "-."),
    ('O', "---"), ('P', ".--."), ('Q', "--.-"), ('R', ".-."), ('S', "..."), ('T', "-"), ('U', "..-"),
    ('V', "...-"), ('W', ".--"), ('X', "-..-"), ('Y', "-.--"), ('Z', "--.."),
    ('0', "-----"), ('1', ".----"), ('2', "..---"), ('3', "...--"), ('4', "....-"),
    ('5', "....."), ('6', "-...."), ('7', "--..."), ('8', "---.."), ('9', "----."),
    ('.', ".-.-.-"), (',', "--..--"), ('?', "..--.."), ('\'', ".----."), ('!', "-.-.--"),
    ('/', "-..-."), ('(', "-.--."), (')', "-.--.-"), ('&', ".-..."), (':', "---..."),
    (';', "-.-.-."), ('=', "-...-"), ('+', ".-.-."), ('-', "-....-"), ('_', "..--.-"),
    ('"', ".-..-."), ('$', "...-..-"), ('@', ".--.-."),
];

/// Russian Morse; Ё is sent as Е, so it decodes back to Е.
const MORSE_RUSSIAN: &[(char, &str)] = &[
    ('А', ".-"), ('Б', "-..."), ('В', ".--"), ('Г', "--."), ('Д', "-.."), ('Е', "."), ('Ж', "...-"),
    ('З', "--.."), ('И', ".."), ('Й', ".---"), ('К', "-.-"), ('Л', ".-.."), ('М', "--"), ('Н', "-."),
    ('О', "---"), ('П', ".--."), ('Р', ".-."), ('С', "..."), ('Т', "-"), ('У', "..-"), ('Ф', "..-."),
    ('Х', "...."), ('Ц', "-.-."), ('Ч', "---."), ('Ш', "----"), ('Щ', "--.-"), ('Ъ', "--.--"),
    ('Ы', "-.--"), ('Ь', "-..-"), ('Э', "..-.."), ('Ю', "..--"), ('Я', ".-.-"),
    ('0', "-----"), ('1', ".----"), ('2', "..---"), ('3', "...--"), ('4', "....-"),
    ('5', "....."), ('6', "-...."), ('7', "--..."), ('8', "---.."), ('9', "----."),
    ('.', "......"), (',', ".-.-.-"), ('?', "..--.."), ('!', "--..--"), ('-', "-....-"),
];

fn morse_table(kwargs: &Kwargs, line: usize, what: &str) -> Result<&'static [(char, &'static str)], RuntimeError> {
    match kw_str(kwargs, "alphabet").as_deref() {
        None | Some("en") => Ok(MORSE_LATIN),
        Some("ru") => Ok(MORSE_RUSSIAN),
        Some(other) => Err(RuntimeError::new(400, line, format!("{what}: alphabet must be \"en\" or \"ru\", got \"{other}\""))),
    }
}

pub fn morse_encode_text(text: &str, table: &[(char, &str)]) -> Result<String, String> {
    let mut words: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        let codes: Result<Vec<&str>, String> = word.chars().flat_map(char::to_uppercase).map(|c| {
            let c = if c == 'Ё' { 'Е' } else { c };
            table.iter().find(|(k, _)| *k == c).map(|(_, v)| *v)
                .ok_or_else(|| format!("no Morse code for `{c}` in this alphabet"))
        }).collect();
        words.push(codes?.join(" "));
    }
    Ok(words.join(" / "))
}

pub fn morse_decode_text(text: &str, table: &[(char, &str)]) -> Result<String, String> {
    let normalized: String = text.chars().map(|c| match c {
        '·' | '•' => '.',
        '−' | '–' | '—' | '_' => '-',
        other => other,
    }).collect();
    let words: Result<Vec<String>, String> = normalized.split('/').map(|word| {
        word.split_whitespace().map(|code| {
            table.iter().find(|(_, v)| *v == code).map(|(k, _)| *k)
                .ok_or_else(|| format!("`{code}` is not a Morse code in this alphabet"))
        }).collect()
    }).collect();
    Ok(words?.join(" "))
}

// ---------------- Rach-facing commands ----------------

fn text_key(args: &[Value], line: usize, what: &str) -> Result<(String, String), RuntimeError> {
    Ok((str_arg(args, 0, line, what)?, str_arg(args, 1, line, what)?))
}

fn lifted(r: Result<String, String>, line: usize, what: &str, ctx: &Ctx) -> Result<Value, RuntimeError> {
    r.map(|s| emit_text(ctx, what, s)).map_err(|e| RuntimeError::new(400, line, format!("{what}: {e}")))
}

pub fn playfair_encrypt(args: &[Value], _kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let (t, k) = text_key(args, line, "playfair_encrypt")?;
    lifted(playfair(&t, &k, false), line, "playfair_encrypt", ctx)
}

pub fn playfair_decrypt(args: &[Value], _kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let (t, k) = text_key(args, line, "playfair_decrypt")?;
    lifted(playfair(&t, &k, true), line, "playfair_decrypt", ctx)
}

fn rails_arg(args: &[Value], kwargs: &Kwargs, line: usize, what: &str) -> Result<usize, RuntimeError> {
    let n = int_arg(args, 1, kwargs, "rails", 3, line, what)?;
    usize::try_from(n).ok().filter(|n| *n >= 1)
        .ok_or_else(|| RuntimeError::new(400, line, format!("{what}: rails must be >= 1")))
}

pub fn rail_fence_encrypt(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let t = str_arg(args, 0, line, "rail_fence_encrypt")?;
    let n = rails_arg(args, kwargs, line, "rail_fence_encrypt")?;
    Ok(emit_text(ctx, "rail_fence_encrypt", rail_fence(&t, n, false)))
}

pub fn rail_fence_decrypt(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let t = str_arg(args, 0, line, "rail_fence_decrypt")?;
    let n = rails_arg(args, kwargs, line, "rail_fence_decrypt")?;
    Ok(emit_text(ctx, "rail_fence_decrypt", rail_fence(&t, n, true)))
}

fn nonempty_key(key: &str, line: usize, what: &str) -> Result<(), RuntimeError> {
    if key.is_empty() { Err(RuntimeError::new(400, line, format!("{what}: key must not be empty"))) } else { Ok(()) }
}

pub fn columnar_encrypt(args: &[Value], _kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let (t, k) = text_key(args, line, "columnar_encrypt")?;
    nonempty_key(&k, line, "columnar_encrypt")?;
    Ok(emit_text(ctx, "columnar_encrypt", columnar(&t, &k, false)))
}

pub fn columnar_decrypt(args: &[Value], _kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let (t, k) = text_key(args, line, "columnar_decrypt")?;
    nonempty_key(&k, line, "columnar_decrypt")?;
    Ok(emit_text(ctx, "columnar_decrypt", columnar(&t, &k, true)))
}

pub fn polybius_encrypt(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let t = str_arg(args, 0, line, "polybius_encrypt")?;
    let key = args.get(1).map(Value::as_str).or_else(|| kw_str(kwargs, "key")).unwrap_or_default();
    Ok(emit_text(ctx, "polybius_encrypt", polybius_encode(&t, &key)))
}

pub fn polybius_decrypt(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let t = str_arg(args, 0, line, "polybius_decrypt")?;
    let key = args.get(1).map(Value::as_str).or_else(|| kw_str(kwargs, "key")).unwrap_or_default();
    lifted(polybius_decode(&t, &key), line, "polybius_decrypt", ctx)
}

pub fn bifid_encrypt(args: &[Value], _kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let (t, k) = text_key(args, line, "bifid_encrypt")?;
    Ok(emit_text(ctx, "bifid_encrypt", bifid(&t, &k, false)))
}

pub fn bifid_decrypt(args: &[Value], _kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let (t, k) = text_key(args, line, "bifid_decrypt")?;
    Ok(emit_text(ctx, "bifid_decrypt", bifid(&t, &k, true)))
}

fn adfgvx_cmd(what: &str, decrypt: bool, args: &[Value], line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let t = str_arg(args, 0, line, what)?;
    let sq = str_arg(args, 1, line, what)?;
    let tk = str_arg(args, 2, line, what)?;
    nonempty_key(&tk, line, what)?;
    lifted(adfgvx(&t, &sq, &tk, decrypt), line, what, ctx)
}

pub fn adfgvx_encrypt(args: &[Value], _kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    adfgvx_cmd("adfgvx_encrypt", false, args, line, ctx)
}

pub fn adfgvx_decrypt(args: &[Value], _kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    adfgvx_cmd("adfgvx_decrypt", true, args, line, ctx)
}

pub fn bacon_encrypt(args: &[Value], _kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let t = str_arg(args, 0, line, "bacon_encrypt")?;
    Ok(emit_text(ctx, "bacon_encrypt", bacon_encode(&t)))
}

pub fn bacon_decrypt(args: &[Value], _kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let t = str_arg(args, 0, line, "bacon_decrypt")?;
    lifted(bacon_decode(&t), line, "bacon_decrypt", ctx)
}

pub fn morse_encode(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let t = str_arg(args, 0, line, "morse_encode")?;
    let table = morse_table(kwargs, line, "morse_encode")?;
    lifted(morse_encode_text(&t, table), line, "morse_encode", ctx)
}

pub fn morse_decode(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let t = str_arg(args, 0, line, "morse_decode")?;
    let table = morse_table(kwargs, line, "morse_decode")?;
    lifted(morse_decode_text(&t, table), line, "morse_decode", ctx)
}

// ---------------- cryptanalysis ----------------
//
// Babbage broke Vigenère around 1854 (never published; Kasiski did independently in 1863):
// repeated fragments in the ciphertext give away the key length, and once you know it, each
// key letter is just a Caesar shift you can find by frequency analysis. We find the length the
// statistically tidier way (Friedman's index of coincidence, 1922) and each shift by
// chi-squared against the language's letter frequencies.

fn letter_indices(text: &str, abc: &Alphabet) -> Vec<usize> {
    text.chars().filter_map(|c| abc.index(c).map(|(i, _)| i)).collect()
}

pub fn index_of_coincidence_of(letters: &[usize], n: usize) -> f64 {
    let total = letters.len();
    if total < 2 { return 0.0; }
    let mut counts = vec![0usize; n];
    for &l in letters { counts[l] += 1; }
    let pairs: usize = counts.iter().map(|c| c * c.saturating_sub(1)).sum();
    pairs as f64 / (total * (total - 1)) as f64
}

/// Shift that makes `letters` look most like the language (lowest chi-squared).
fn best_shift(letters: &[usize], abc: &Alphabet) -> usize {
    let n = abc.size();
    let total = letters.len().max(1) as f64;
    let mut counts = vec![0usize; n];
    for &l in letters { counts[l] += 1; }
    (0..n).min_by(|&a, &b| {
        let chi = |shift: usize| -> f64 {
            (0..n).map(|plain| {
                let observed = counts[(plain + shift) % n] as f64;
                let expected = abc.freq[plain] * total;
                (observed - expected).powi(2) / expected
            }).sum()
        };
        chi(a).total_cmp(&chi(b))
    }).unwrap_or(0)
}

/// Most likely key length: the shortest length whose columns look like natural language
/// (average IC within 10% of the best seen). Taking the *shortest* stops multiples of the true
/// length — which score just as well — from winning.
fn guess_key_length(letters: &[usize], n: usize, max_len: usize) -> usize {
    let scores: Vec<(usize, f64)> = (1..=max_len)
        .filter(|l| letters.len() / l >= 2)
        .map(|l| {
            let avg = (0..l).map(|col| {
                let column: Vec<usize> = letters.iter().skip(col).step_by(l).copied().collect();
                index_of_coincidence_of(&column, n)
            }).sum::<f64>() / l as f64;
            (l, avg)
        })
        .collect();
    let best = scores.iter().map(|(_, s)| *s).fold(0.0, f64::max);
    scores.iter().find(|(_, s)| *s >= best * 0.9).map_or(1, |(l, _)| *l)
}

/// A key that repeats itself ("KEYKEY") enciphers exactly like its period ("KEY"), so reduce
/// to the shortest one — this also absorbs the case where short texts make a multiple of the
/// true length score best.
fn minimal_period(key: &[usize]) -> Vec<usize> {
    let n = key.len();
    let p = (1..=n).find(|p| n.is_multiple_of(*p) && (0..n).all(|i| key[i] == key[i % p])).unwrap_or(n);
    key[..p].to_vec()
}

pub fn crack_vigenere(text: &str, abc: &Alphabet, max_len: usize) -> Vec<usize> {
    let letters = letter_indices(text, abc);
    let len = guess_key_length(&letters, abc.size(), max_len);
    let key: Vec<usize> = (0..len).map(|col| {
        let column: Vec<usize> = letters.iter().skip(col).step_by(len).copied().collect();
        best_shift(&column, abc)
    }).collect();
    minimal_period(&key)
}

fn result_map(ctx: &Ctx, what: &str, entries: Vec<(&str, Value)>) -> Value {
    let map: BTreeMap<String, Value> = entries.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    let value = Value::Map(map);
    if !ctx.capturing {
        println!("{what}: {}", value.as_str());
        println!("completed");
    }
    value
}

fn enough_letters(text: &str, abc: &Alphabet, min: usize, line: usize, what: &str) -> Result<(), RuntimeError> {
    let n = letter_indices(text, abc).len();
    if n < min {
        return Err(RuntimeError::new(400, line, format!(
            "{what}: only {n} letters — frequency analysis needs at least {min} to say anything"
        )));
    }
    Ok(())
}

pub fn vigenere_crack(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let text = str_arg(args, 0, line, "vigenere_crack")?;
    let abc = Alphabet::from_kwargs(kwargs, line, "vigenere_crack")?;
    let max_len = int_arg(args, 1, kwargs, "max_key_length", 20, line, "vigenere_crack")?;
    let max_len = usize::try_from(max_len).ok().filter(|n| *n >= 1)
        .ok_or_else(|| RuntimeError::new(400, line, "vigenere_crack: max_key_length must be >= 1"))?;
    enough_letters(&text, &abc, 20, line, "vigenere_crack")?;
    let key = crack_vigenere(&text, &abc, max_len);
    let key_text: String = key.iter().map(|&i| abc.letter(i, true)).collect();
    let plaintext = vigenere(&text, &key, true, &abc);
    Ok(result_map(ctx, "vigenere_crack", vec![
        ("key", Value::Str(key_text)),
        ("key_length", Value::Int(key.len() as i64)),
        ("plaintext", Value::Str(plaintext)),
    ]))
}

pub fn caesar_crack(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let text = str_arg(args, 0, line, "caesar_crack")?;
    let abc = Alphabet::from_kwargs(kwargs, line, "caesar_crack")?;
    enough_letters(&text, &abc, 10, line, "caesar_crack")?;
    let shift = best_shift(&letter_indices(&text, &abc), &abc);
    Ok(result_map(ctx, "caesar_crack", vec![
        ("shift", Value::Int(shift as i64)),
        ("plaintext", Value::Str(caesar(&text, -(shift as i64), &abc))),
    ]))
}

pub fn index_of_coincidence(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let text = str_arg(args, 0, line, "index_of_coincidence")?;
    let abc = Alphabet::from_kwargs(kwargs, line, "index_of_coincidence")?;
    let ic = index_of_coincidence_of(&letter_indices(&text, &abc), abc.size());
    if !ctx.capturing { println!("index_of_coincidence: {ic:.4}"); println!("completed"); }
    Ok(Value::Float(ic))
}

pub fn letter_frequencies(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let text = str_arg(args, 0, line, "letter_frequencies")?;
    let abc = Alphabet::from_kwargs(kwargs, line, "letter_frequencies")?;
    let mut counts = vec![0i64; abc.size()];
    for i in letter_indices(&text, &abc) { counts[i] += 1; }
    let entries: Vec<(String, Value)> = counts.iter().enumerate()
        .filter(|(_, c)| **c > 0)
        .map(|(i, c)| (abc.letter(i, true).to_string(), Value::Int(*c)))
        .collect();
    let value = Value::Map(entries.into_iter().collect());
    if !ctx.capturing { println!("letter_frequencies: {}", value.as_str()); println!("completed"); }
    Ok(value)
}
