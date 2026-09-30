//! Enigma I / M3 simulator: rotors I–V, reflectors B and C, ring settings (Ringstellung),
//! start positions (Grundstellung), plugboard (Steckerbrett), and the middle rotor's
//! double-step anomaly. Encryption and decryption are the same operation.
//!
//! Verified against the canonical `AAAAA → BDZGO` vector and a real 1941 Operation
//! Barbarossa message (see tests/enigma.rs).

use crate::ast::Value;
use crate::interpreter::{Ctx, RuntimeError};
use crate::stdlib::args::{emit_text, kw_str, str_arg, Kwargs};

const ROTORS: &[(&str, &str, u8)] = &[
    ("I", "EKMFLGDQVZNTOWYHXUSPAIBRCJ", b'Q'),
    ("II", "AJDKSIRUXBLHWTMCQGZNPYFVOE", b'E'),
    ("III", "BDFHJLCPRTXVZNYEIWGAKMQOUS", b'V'),
    ("IV", "ESOVPZJAYQUIRHXLNFTGKDCMWB", b'J'),
    ("V", "VZBRGITYUPSDNHLXAWMJQOFECK", b'Z'),
];

const REFLECTORS: &[(&str, &str)] = &[
    ("B", "YRUHQSLDPXNGOKMIEBFZCWVJAT"),
    ("C", "FVPJIAOYEDRZXWGCTKUQSBNMHL"),
];

struct Rotor {
    forward: [u8; 26],
    backward: [u8; 26],
    notch: u8,
    ring: u8,
    pos: u8,
}

impl Rotor {
    fn new(name: &str, ring: u8, pos: u8) -> Result<Self, String> {
        let (_, wiring, notch) = ROTORS.iter().find(|(n, _, _)| n.eq_ignore_ascii_case(name))
            .ok_or_else(|| format!("unknown rotor `{name}` (I, II, III, IV, V)"))?;
        let mut forward = [0u8; 26];
        let mut backward = [0u8; 26];
        for (i, c) in wiring.bytes().enumerate() {
            forward[i] = c - b'A';
            backward[(c - b'A') as usize] = i as u8;
        }
        Ok(Rotor { forward, backward, notch: notch - b'A', ring, pos })
    }

    fn at_notch(&self) -> bool { self.pos == self.notch }

    fn step(&mut self) { self.pos = (self.pos + 1) % 26; }

    fn pass(&self, x: u8, table: &[u8; 26]) -> u8 {
        let shift = (self.pos + 26 - self.ring) % 26;
        (table[((x + shift) % 26) as usize] + 26 - shift) % 26
    }
}

pub struct Enigma {
    rotors: [Rotor; 3], // left, middle, right
    reflector: [u8; 26],
    plugboard: [u8; 26],
}

/// Settings given as letters ("BUL") or 1-based numbers ("02 21 12").
fn parse_triple(s: &str, what: &str) -> Result<[u8; 3], String> {
    let vals: Vec<u8> = if s.chars().any(|c| c.is_ascii_digit()) {
        s.split(|c: char| c.is_whitespace() || c == ',' || c == '-')
            .filter(|t| !t.is_empty())
            .map(|t| t.parse::<u8>().ok().filter(|n| (1..=26).contains(n)).map(|n| n - 1)
                .ok_or_else(|| format!("{what}: `{t}` is not a number 1..26")))
            .collect::<Result<_, _>>()?
    } else {
        s.chars().filter(|c| !c.is_whitespace()).map(|c| {
            let u = c.to_ascii_uppercase();
            if u.is_ascii_uppercase() { Ok(u as u8 - b'A') } else { Err(format!("{what}: `{c}` is not a letter")) }
        }).collect::<Result<_, _>>()?
    };
    <[u8; 3]>::try_from(vals).map_err(|v| format!("{what} needs exactly 3 values, got {}", v.len()))
}

impl Enigma {
    pub fn new(rotors: &str, reflector: &str, rings: &str, positions: &str, plugboard: &str) -> Result<Self, String> {
        let names: Vec<&str> = rotors.split(|c: char| c.is_whitespace() || c == ',' || c == '-').filter(|t| !t.is_empty()).collect();
        if names.len() != 3 {
            return Err(format!("rotors needs 3 names left-to-right, e.g. \"I II III\"; got {}", names.len()));
        }
        let mut seen = names.clone();
        seen.sort_unstable();
        seen.dedup();
        if seen.len() != 3 { return Err("each rotor can only be used once".into()); }
        let rings = parse_triple(rings, "rings")?;
        let pos = parse_triple(positions, "positions")?;
        let rotors = [
            Rotor::new(names[0], rings[0], pos[0])?,
            Rotor::new(names[1], rings[1], pos[1])?,
            Rotor::new(names[2], rings[2], pos[2])?,
        ];
        let (_, refl) = REFLECTORS.iter().find(|(n, _)| n.eq_ignore_ascii_case(reflector))
            .ok_or_else(|| format!("unknown reflector `{reflector}` (B, C)"))?;
        let mut reflector = [0u8; 26];
        for (i, c) in refl.bytes().enumerate() { reflector[i] = c - b'A'; }

        let mut plug: [u8; 26] = std::array::from_fn(|i| i as u8);
        for pair in plugboard.split_whitespace() {
            let p: Vec<u8> = pair.bytes().map(|b| b.to_ascii_uppercase()).collect();
            let [a, b] = p.as_slice() else { return Err(format!("plugboard pair `{pair}` must be two letters")) };
            if !a.is_ascii_uppercase() || !b.is_ascii_uppercase() || a == b {
                return Err(format!("plugboard pair `{pair}` must be two different letters"));
            }
            let (a, b) = ((a - b'A') as usize, (b - b'A') as usize);
            if plug[a] != a as u8 || plug[b] != b as u8 {
                return Err(format!("plugboard letter in `{pair}` is already cabled"));
            }
            plug[a] = b as u8;
            plug[b] = a as u8;
        }
        Ok(Enigma { rotors, reflector, plugboard: plug })
    }

    /// The pawl mechanism: the right rotor always steps; the middle steps when the right is at
    /// its notch, *and* steps again (taking the left with it) when it is itself at its notch —
    /// the double step that made the real machine's period 26·25·26, not 26³.
    fn step(&mut self) {
        let [left, middle, right] = &mut self.rotors;
        if middle.at_notch() {
            middle.step();
            left.step();
        } else if right.at_notch() {
            middle.step();
        }
        right.step();
    }

    pub fn press(&mut self, letter: u8) -> u8 {
        self.step();
        let mut x = self.plugboard[letter as usize];
        for r in self.rotors.iter().rev() { x = r.pass(x, &r.forward); }
        x = self.reflector[x as usize];
        for r in &self.rotors { x = r.pass(x, &r.backward); }
        self.plugboard[x as usize]
    }

    /// Letters are enciphered (keeping their case); anything else passes through and does not
    /// advance the rotors.
    pub fn process(&mut self, text: &str) -> String {
        text.chars().map(|c| {
            if c.is_ascii_alphabetic() {
                let out = char::from(b'A' + self.press(c.to_ascii_uppercase() as u8 - b'A'));
                if c.is_ascii_lowercase() { out.to_ascii_lowercase() } else { out }
            } else {
                c
            }
        }).collect()
    }
}

pub fn enigma(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let text = str_arg(args, 0, line, "enigma")?;
    let setting = |key: &str, default: &str| kw_str(kwargs, key).unwrap_or_else(|| default.to_string());
    let mut machine = Enigma::new(
        &setting("rotors", "I II III"),
        &setting("reflector", "B"),
        &setting("rings", "AAA"),
        &setting("positions", "AAA"),
        &setting("plugboard", ""),
    ).map_err(|e| RuntimeError::new(400, line, format!("enigma: {e}")))?;
    Ok(emit_text(ctx, "enigma", machine.process(&text)))
}
