//! Modern symmetric cryptography, hand-rolled (no crates): AES-128/192/256 (FIPS-197) in
//! ECB / CBC / CTR, ChaCha20 and Poly1305 (RFC 8439), the ChaCha20-Poly1305 AEAD, plus XOR and
//! RC4 for their historical/teaching value, and password-based `encrypt` / `decrypt`.
//!
//! Honest limits: these follow the specs and pass the official test vectors, but they are not
//! audited and AES here is table-based (not constant-time), so it can leak key bits through
//! cache timing to a co-located attacker. Good for learning, CTFs, file obfuscation and
//! interop with other tools; for protecting real secrets against real adversaries, prefer
//! `encrypt`/`decrypt` (ChaCha20-Poly1305 is constant-time by construction) and keep in mind
//! nothing here has been reviewed by a third party. XOR and RC4 are broken — never rely on them.
//!
//! Keys: 32/48/64 hex digits are used as raw key bytes; anything else is treated as a
//! passphrase and hashed with SHA-256 (fast — fine for random passphrases, weak for human
//! passwords; `encrypt` uses PBKDF2 instead). IVs/nonces default to fresh OS randomness and are
//! prepended to the hex output; pass `iv=`/`nonce=` explicitly to manage them yourself.

use std::sync::OnceLock;

use crate::ast::Value;
use crate::interpreter::{Ctx, RuntimeError};
use crate::stdlib::args::{bytes_to_hex, emit_text, hex_to_bytes, input_bytes, int_arg, kw_str, output_bytes, str_arg, Kwargs};
use crate::stdlib::encoding::{base64_decode_bytes, base64_encode_bytes};
use crate::stdlib::hash::{pbkdf2_bytes, sha256_bytes, Algo};

// ---------------- OS randomness ----------------

#[cfg(unix)]
pub fn os_random(buf: &mut [u8]) -> Result<(), String> {
    use std::io::Read;
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(buf))
        .map_err(|e| format!("cannot read /dev/urandom: {e}"))
}

#[cfg(windows)]
pub fn os_random(buf: &mut [u8]) -> Result<(), String> {
    #[link(name = "bcrypt")]
    extern "system" {
        fn BCryptGenRandom(alg: *mut core::ffi::c_void, buf: *mut u8, len: u32, flags: u32) -> i32;
    }
    const BCRYPT_USE_SYSTEM_PREFERRED_RNG: u32 = 2;
    let len = u32::try_from(buf.len()).map_err(|_| "random buffer too large".to_string())?;
    // SAFETY: buf is a valid, writable slice of exactly `len` bytes; a null algorithm handle is
    // allowed together with BCRYPT_USE_SYSTEM_PREFERRED_RNG.
    let status = unsafe { BCryptGenRandom(core::ptr::null_mut(), buf.as_mut_ptr(), len, BCRYPT_USE_SYSTEM_PREFERRED_RNG) };
    if status == 0 { Ok(()) } else { Err(format!("BCryptGenRandom failed with status {status:#x}")) }
}

#[cfg(not(any(unix, windows)))]
pub fn os_random(_buf: &mut [u8]) -> Result<(), String> {
    Err("no OS randomness source on this platform — pass iv=/nonce= explicitly".into())
}

fn random_vec(n: usize) -> Result<Vec<u8>, String> {
    let mut v = vec![0u8; n];
    os_random(&mut v)?;
    Ok(v)
}

/// Compare MACs without an early exit, so the time taken doesn't reveal how many bytes matched.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

// ---------------- AES (FIPS-197) ----------------

fn gmul(mut a: u8, mut b: u8) -> u8 {
    let mut p = 0u8;
    while b != 0 {
        if b & 1 != 0 { p ^= a; }
        let hi = a & 0x80;
        a <<= 1;
        if hi != 0 { a ^= 0x1b; }
        b >>= 1;
    }
    p
}

/// S-box derived from its definition (multiplicative inverse in GF(2⁸) followed by the affine
/// map), not typed in as 256 magic numbers.
fn sboxes() -> &'static ([u8; 256], [u8; 256]) {
    static TABLES: OnceLock<([u8; 256], [u8; 256])> = OnceLock::new();
    TABLES.get_or_init(|| {
        let mut sbox = [0u8; 256];
        let mut inv = [0u8; 256];
        for x in 0..=255u8 {
            let b = if x == 0 { 0 } else { (1..=255u8).find(|y| gmul(x, *y) == 1).expect("GF(2^8) inverse") };
            let s = b ^ b.rotate_left(1) ^ b.rotate_left(2) ^ b.rotate_left(3) ^ b.rotate_left(4) ^ 0x63;
            sbox[x as usize] = s;
            inv[s as usize] = x;
        }
        (sbox, inv)
    })
}

pub struct Aes {
    round_keys: Vec<[u8; 16]>,
}

impl Aes {
    pub fn new(key: &[u8]) -> Result<Self, String> {
        let nk = match key.len() { 16 => 4, 24 => 6, 32 => 8, n => return Err(format!("AES key must be 16, 24 or 32 bytes, got {n}")) };
        let rounds = nk + 6;
        let (sbox, _) = sboxes();
        let mut w: Vec<[u8; 4]> = key.chunks(4).map(|c| [c[0], c[1], c[2], c[3]]).collect();
        let mut rcon = 1u8;
        for i in nk..4 * (rounds + 1) {
            let mut t = w[i - 1];
            if i % nk == 0 {
                t = [sbox[t[1] as usize] ^ rcon, sbox[t[2] as usize], sbox[t[3] as usize], sbox[t[0] as usize]];
                rcon = gmul(rcon, 2);
            } else if nk > 6 && i % nk == 4 {
                t = t.map(|b| sbox[b as usize]);
            }
            let prev = w[i - nk];
            w.push([prev[0] ^ t[0], prev[1] ^ t[1], prev[2] ^ t[2], prev[3] ^ t[3]]);
        }
        let round_keys = w.chunks(4).map(|ws| {
            let mut k = [0u8; 16];
            for (j, word) in ws.iter().enumerate() { k[4 * j..4 * j + 4].copy_from_slice(word); }
            k
        }).collect();
        Ok(Aes { round_keys })
    }

    fn add(state: &mut [u8; 16], k: &[u8; 16]) {
        for (s, k) in state.iter_mut().zip(k) { *s ^= k; }
    }

    /// State is column-major: byte `r + 4c` is row r, column c.
    fn shift_rows(s: &mut [u8; 16], inverse: bool) {
        let old = *s;
        for r in 1..4 {
            for c in 0..4 {
                let from = if inverse { (c + 4 - r) % 4 } else { (c + r) % 4 };
                s[r + 4 * c] = old[r + 4 * from];
            }
        }
    }

    fn mix_columns(s: &mut [u8; 16], inverse: bool) {
        let m: [u8; 4] = if inverse { [14, 11, 13, 9] } else { [2, 3, 1, 1] };
        for c in 0..4 {
            let col = [s[4 * c], s[4 * c + 1], s[4 * c + 2], s[4 * c + 3]];
            for r in 0..4 {
                s[4 * c + r] = (0..4).fold(0, |acc, i| acc ^ gmul(col[i], m[(i + 4 - r) % 4]));
            }
        }
    }

    pub fn encrypt_block(&self, block: &mut [u8; 16]) {
        let (sbox, _) = sboxes();
        let last = self.round_keys.len() - 1;
        Self::add(block, &self.round_keys[0]);
        for round in 1..=last {
            for b in block.iter_mut() { *b = sbox[*b as usize]; }
            Self::shift_rows(block, false);
            if round != last { Self::mix_columns(block, false); }
            Self::add(block, &self.round_keys[round]);
        }
    }

    pub fn decrypt_block(&self, block: &mut [u8; 16]) {
        let (_, inv) = sboxes();
        let last = self.round_keys.len() - 1;
        Self::add(block, &self.round_keys[last]);
        for round in (0..last).rev() {
            Self::shift_rows(block, true);
            for b in block.iter_mut() { *b = inv[*b as usize]; }
            Self::add(block, &self.round_keys[round]);
            if round != 0 { Self::mix_columns(block, true); }
        }
    }
}

fn pkcs7_pad(data: &[u8]) -> Vec<u8> {
    let pad = 16 - data.len() % 16;
    let mut v = data.to_vec();
    v.extend(std::iter::repeat_n(pad as u8, pad));
    v
}

fn pkcs7_unpad(mut data: Vec<u8>) -> Result<Vec<u8>, String> {
    let bad = || "bad padding — wrong key, wrong mode, or corrupted ciphertext".to_string();
    let pad = *data.last().ok_or_else(bad)? as usize;
    if pad == 0 || pad > 16 || pad > data.len() || !data[data.len() - pad..].iter().all(|b| *b as usize == pad) {
        return Err(bad());
    }
    data.truncate(data.len() - pad);
    Ok(data)
}

fn block(b: &[u8]) -> [u8; 16] {
    let mut out = [0u8; 16];
    out.copy_from_slice(b);
    out
}

pub fn aes_ecb(aes: &Aes, data: &[u8], decrypt: bool) -> Result<Vec<u8>, String> {
    if decrypt {
        if !data.len().is_multiple_of(16) { return Err("ECB ciphertext must be a whole number of 16-byte blocks".into()); }
        let mut out = Vec::with_capacity(data.len());
        for b in data.chunks(16) { let mut blk = block(b); aes.decrypt_block(&mut blk); out.extend(blk); }
        pkcs7_unpad(out)
    } else {
        let mut out = Vec::new();
        for b in pkcs7_pad(data).chunks(16) { let mut blk = block(b); aes.encrypt_block(&mut blk); out.extend(blk); }
        Ok(out)
    }
}

pub fn aes_cbc(aes: &Aes, iv: &[u8; 16], data: &[u8], decrypt: bool) -> Result<Vec<u8>, String> {
    let mut prev = *iv;
    let mut out = Vec::with_capacity(data.len() + 16);
    if decrypt {
        if !data.len().is_multiple_of(16) { return Err("CBC ciphertext must be a whole number of 16-byte blocks".into()); }
        for b in data.chunks(16) {
            let ct = block(b);
            let mut blk = ct;
            aes.decrypt_block(&mut blk);
            for (x, p) in blk.iter_mut().zip(prev) { *x ^= p; }
            out.extend(blk);
            prev = ct;
        }
        pkcs7_unpad(out)
    } else {
        for b in pkcs7_pad(data).chunks(16) {
            let mut blk = block(b);
            for (x, p) in blk.iter_mut().zip(prev) { *x ^= p; }
            aes.encrypt_block(&mut blk);
            out.extend(blk);
            prev = blk;
        }
        Ok(out)
    }
}

/// CTR with the whole 16-byte block as a big-endian counter (NIST SP 800-38A). Encryption and
/// decryption are the same keystream XOR; no padding.
pub fn aes_ctr(aes: &Aes, iv: &[u8; 16], data: &[u8]) -> Vec<u8> {
    let mut counter = u128::from_be_bytes(*iv);
    let mut out = Vec::with_capacity(data.len());
    for chunk in data.chunks(16) {
        let mut ks = counter.to_be_bytes();
        aes.encrypt_block(&mut ks);
        out.extend(chunk.iter().zip(ks).map(|(d, k)| d ^ k));
        counter = counter.wrapping_add(1);
    }
    out
}

// ---------------- ChaCha20 & Poly1305 (RFC 8439) ----------------

fn quarter(s: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    s[a] = s[a].wrapping_add(s[b]); s[d] ^= s[a]; s[d] = s[d].rotate_left(16);
    s[c] = s[c].wrapping_add(s[d]); s[b] ^= s[c]; s[b] = s[b].rotate_left(12);
    s[a] = s[a].wrapping_add(s[b]); s[d] ^= s[a]; s[d] = s[d].rotate_left(8);
    s[c] = s[c].wrapping_add(s[d]); s[b] ^= s[c]; s[b] = s[b].rotate_left(7);
}

fn le32(b: &[u8]) -> u32 { u32::from_le_bytes([b[0], b[1], b[2], b[3]]) }

pub fn chacha20_block(key: &[u8; 32], counter: u32, nonce: &[u8; 12]) -> [u8; 64] {
    let mut init = [0u32; 16];
    init[..4].copy_from_slice(&[0x6170_7865, 0x3320_646e, 0x7962_2d32, 0x6b20_6574]); // "expand 32-byte k"
    for i in 0..8 { init[4 + i] = le32(&key[4 * i..]); }
    init[12] = counter;
    for i in 0..3 { init[13 + i] = le32(&nonce[4 * i..]); }
    let mut s = init;
    for _ in 0..10 {
        quarter(&mut s, 0, 4, 8, 12); quarter(&mut s, 1, 5, 9, 13); quarter(&mut s, 2, 6, 10, 14); quarter(&mut s, 3, 7, 11, 15);
        quarter(&mut s, 0, 5, 10, 15); quarter(&mut s, 1, 6, 11, 12); quarter(&mut s, 2, 7, 8, 13); quarter(&mut s, 3, 4, 9, 14);
    }
    let mut out = [0u8; 64];
    for i in 0..16 { out[4 * i..4 * i + 4].copy_from_slice(&s[i].wrapping_add(init[i]).to_le_bytes()); }
    out
}

pub fn chacha20_xor(key: &[u8; 32], counter: u32, nonce: &[u8; 12], data: &[u8]) -> Vec<u8> {
    data.chunks(64).enumerate().flat_map(|(i, chunk)| {
        let ks = chacha20_block(key, counter.wrapping_add(i as u32), nonce);
        chunk.iter().zip(ks).map(|(d, k)| d ^ k).collect::<Vec<u8>>()
    }).collect()
}

/// Poly1305 in radix 2²⁶ (the "donna" layout), so every product fits in a u64.
pub fn poly1305_tag(msg: &[u8], key: &[u8; 32]) -> [u8; 16] {
    const M: u32 = 0x03ff_ffff;
    let r0 = le32(&key[0..]) & 0x03ff_ffff;
    let r1 = (le32(&key[3..]) >> 2) & 0x03ff_ff03;
    let r2 = (le32(&key[6..]) >> 4) & 0x03ff_c0ff;
    let r3 = (le32(&key[9..]) >> 6) & 0x03f0_3fff;
    let r4 = (le32(&key[12..]) >> 8) & 0x000f_ffff;
    let (s1, s2, s3, s4) = (r1 * 5, r2 * 5, r3 * 5, r4 * 5);
    let (mut h0, mut h1, mut h2, mut h3, mut h4) = (0u32, 0u32, 0u32, 0u32, 0u32);
    for chunk in msg.chunks(16) {
        let mut b = [0u8; 17];
        b[..chunk.len()].copy_from_slice(chunk);
        b[chunk.len()] = 1; // the 2^128 (or 2^(8·len)) bit
        h0 += le32(&b[0..]) & M;
        h1 += (le32(&b[3..]) >> 2) & M;
        h2 += (le32(&b[6..]) >> 4) & M;
        h3 += (le32(&b[9..]) >> 6) & M;
        h4 += (le32(&b[12..]) >> 8) | (u32::from(b[16]) << 24);
        let m = |a: u32, b: u32| u64::from(a) * u64::from(b);
        let d0 = m(h0, r0) + m(h1, s4) + m(h2, s3) + m(h3, s2) + m(h4, s1);
        let mut d1 = m(h0, r1) + m(h1, r0) + m(h2, s4) + m(h3, s3) + m(h4, s2);
        let mut d2 = m(h0, r2) + m(h1, r1) + m(h2, r0) + m(h3, s4) + m(h4, s3);
        let mut d3 = m(h0, r3) + m(h1, r2) + m(h2, r1) + m(h3, r0) + m(h4, s4);
        let mut d4 = m(h0, r4) + m(h1, r3) + m(h2, r2) + m(h3, r1) + m(h4, r0);
        h0 = (d0 as u32) & M; d1 += d0 >> 26;
        h1 = (d1 as u32) & M; d2 += d1 >> 26;
        h2 = (d2 as u32) & M; d3 += d2 >> 26;
        h3 = (d3 as u32) & M; d4 += d3 >> 26;
        h4 = (d4 as u32) & M;
        h0 += ((d4 >> 26) as u32) * 5;
        h1 += h0 >> 26; h0 &= M;
    }
    // Full carry, then compute h - p and keep it if it didn't go negative.
    let mut c;
    c = h1 >> 26; h1 &= M; h2 += c;
    c = h2 >> 26; h2 &= M; h3 += c;
    c = h3 >> 26; h3 &= M; h4 += c;
    c = h4 >> 26; h4 &= M; h0 += c * 5;
    c = h0 >> 26; h0 &= M; h1 += c;
    let mut g0 = h0 + 5; c = g0 >> 26; g0 &= M;
    let mut g1 = h1 + c; c = g1 >> 26; g1 &= M;
    let mut g2 = h2 + c; c = g2 >> 26; g2 &= M;
    let mut g3 = h3 + c; c = g3 >> 26; g3 &= M;
    let g4 = (h4 + c).wrapping_sub(1 << 26);
    let keep_g = (g4 >> 31).wrapping_sub(1); // all ones when g4 >= 0
    let sel = |g: u32, h: u32| (g & keep_g) | (h & !keep_g);
    let (h0, h1, h2, h3, h4) = (sel(g0, h0), sel(g1, h1), sel(g2, h2), sel(g3, h3), sel(g4, h4));
    let words = [h0 | (h1 << 26), (h1 >> 6) | (h2 << 20), (h2 >> 12) | (h3 << 14), (h3 >> 18) | (h4 << 8)];
    let mut tag = [0u8; 16];
    let mut carry = 0u64;
    for i in 0..4 {
        let f = u64::from(words[i]) + u64::from(le32(&key[16 + 4 * i..])) + carry;
        tag[4 * i..4 * i + 4].copy_from_slice(&(f as u32).to_le_bytes());
        carry = f >> 32;
    }
    tag
}

fn aead_mac_data(aad: &[u8], ct: &[u8]) -> Vec<u8> {
    let pad16 = |n: usize| (16 - n % 16) % 16;
    let mut m = Vec::with_capacity(aad.len() + ct.len() + 48);
    m.extend_from_slice(aad);
    m.extend(std::iter::repeat_n(0, pad16(aad.len())));
    m.extend_from_slice(ct);
    m.extend(std::iter::repeat_n(0, pad16(ct.len())));
    m.extend((aad.len() as u64).to_le_bytes());
    m.extend((ct.len() as u64).to_le_bytes());
    m
}

fn aead_poly_key(key: &[u8; 32], nonce: &[u8; 12]) -> [u8; 32] {
    let mut k = [0u8; 32];
    k.copy_from_slice(&chacha20_block(key, 0, nonce)[..32]);
    k
}

/// RFC 8439 §2.8: returns ciphertext ‖ 16-byte tag.
pub fn chacha20_poly1305_seal(key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], plaintext: &[u8]) -> Vec<u8> {
    let mut ct = chacha20_xor(key, 1, nonce, plaintext);
    let tag = poly1305_tag(&aead_mac_data(aad, &ct), &aead_poly_key(key, nonce));
    ct.extend(tag);
    ct
}

pub fn chacha20_poly1305_open(key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], sealed: &[u8]) -> Result<Vec<u8>, String> {
    if sealed.len() < 16 { return Err("ciphertext is shorter than its 16-byte tag".into()); }
    let (ct, tag) = sealed.split_at(sealed.len() - 16);
    let expected = poly1305_tag(&aead_mac_data(aad, ct), &aead_poly_key(key, nonce));
    if !ct_eq(&expected, tag) {
        return Err("authentication failed — wrong key/password, or the data was tampered with".into());
    }
    Ok(chacha20_xor(key, 1, nonce, ct))
}

// ---------------- XOR & RC4 (historical; broken) ----------------

pub fn xor_bytes(data: &[u8], key: &[u8]) -> Vec<u8> {
    data.iter().zip(key.iter().cycle()).map(|(d, k)| d ^ k).collect()
}

pub fn rc4_bytes(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut s: [u8; 256] = std::array::from_fn(|i| i as u8);
    let mut j = 0u8;
    for i in 0..256 {
        j = j.wrapping_add(s[i]).wrapping_add(key[i % key.len()]);
        s.swap(i, j as usize);
    }
    let (mut i, mut j) = (0u8, 0u8);
    data.iter().map(|b| {
        i = i.wrapping_add(1);
        j = j.wrapping_add(s[i as usize]);
        s.swap(i as usize, j as usize);
        b ^ s[s[i as usize].wrapping_add(s[j as usize]) as usize]
    }).collect()
}

// ---------------- Rach-facing commands ----------------

fn err(what: &str, line: usize) -> impl Fn(String) -> RuntimeError + '_ {
    move |e| RuntimeError::new(400, line, format!("{what}: {e}"))
}

/// Hex of an allowed raw length → raw bytes; otherwise a passphrase → SHA-256 (truncated to
/// `default_len`).
fn key_bytes(key: &str, raw_lens: &[usize], default_len: usize) -> Vec<u8> {
    let trimmed = key.trim();
    if raw_lens.contains(&(trimmed.len() / 2)) && trimmed.len().is_multiple_of(2) {
        if let Ok(bytes) = hex_to_bytes(trimmed) { return bytes; }
    }
    sha256_bytes(key.as_bytes())[..default_len].to_vec()
}

fn fixed<const N: usize>(bytes: &[u8], what: &str) -> Result<[u8; N], String> {
    <[u8; N]>::try_from(bytes).map_err(|_| format!("{what} must be exactly {N} bytes ({} hex digits), got {}", N * 2, bytes.len()))
}

/// Explicit `name=` hex value, or fresh randomness that gets prepended to the output.
fn iv_or_random<const N: usize>(kwargs: &Kwargs, name: &str) -> Result<([u8; N], bool), String> {
    match kw_str(kwargs, name) {
        Some(h) => Ok((fixed(&hex_to_bytes(&h)?, name)?, false)),
        None => Ok((fixed(&random_vec(N)?, name)?, true)),
    }
}

/// Explicit `name=` hex value, or the first N bytes of the ciphertext.
fn iv_or_prefix<'a, const N: usize>(kwargs: &Kwargs, name: &str, data: &'a [u8]) -> Result<([u8; N], &'a [u8]), String> {
    match kw_str(kwargs, name) {
        Some(h) => Ok((fixed(&hex_to_bytes(&h)?, name)?, data)),
        None => {
            if data.len() < N { return Err(format!("ciphertext is too short to contain its {N}-byte {name}")); }
            let (head, rest) = data.split_at(N);
            Ok((fixed(head, name)?, rest))
        }
    }
}

fn ciphertext_arg(args: &[Value], line: usize, what: &str) -> Result<Vec<u8>, RuntimeError> {
    hex_to_bytes(&str_arg(args, 0, line, what)?).map_err(err(what, line))
}

pub fn xor_encrypt(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let data = input_bytes(args, kwargs, line, "xor_encrypt")?;
    let key = str_arg(args, 1, line, "xor_encrypt")?;
    if key.is_empty() { return Err(RuntimeError::new(400, line, "xor_encrypt: key must not be empty")); }
    Ok(emit_text(ctx, "xor_encrypt", bytes_to_hex(&xor_bytes(&data, key.as_bytes()))))
}

pub fn xor_decrypt(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let data = ciphertext_arg(args, line, "xor_decrypt")?;
    let key = str_arg(args, 1, line, "xor_decrypt")?;
    if key.is_empty() { return Err(RuntimeError::new(400, line, "xor_decrypt: key must not be empty")); }
    Ok(emit_text(ctx, "xor_decrypt", output_bytes(xor_bytes(&data, key.as_bytes()), kwargs, line, "xor_decrypt")?))
}

pub fn rc4_encrypt(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let data = input_bytes(args, kwargs, line, "rc4_encrypt")?;
    let key = str_arg(args, 1, line, "rc4_encrypt")?;
    if key.is_empty() { return Err(RuntimeError::new(400, line, "rc4_encrypt: key must not be empty")); }
    Ok(emit_text(ctx, "rc4_encrypt", bytes_to_hex(&rc4_bytes(key.as_bytes(), &data))))
}

pub fn rc4_decrypt(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let data = ciphertext_arg(args, line, "rc4_decrypt")?;
    let key = str_arg(args, 1, line, "rc4_decrypt")?;
    if key.is_empty() { return Err(RuntimeError::new(400, line, "rc4_decrypt: key must not be empty")); }
    Ok(emit_text(ctx, "rc4_decrypt", output_bytes(rc4_bytes(key.as_bytes(), &data), kwargs, line, "rc4_decrypt")?))
}

fn aes_mode(kwargs: &Kwargs, line: usize, what: &str) -> Result<String, RuntimeError> {
    let mode = kw_str(kwargs, "mode").unwrap_or_else(|| "cbc".into()).to_ascii_lowercase();
    if matches!(mode.as_str(), "ecb" | "cbc" | "ctr") { Ok(mode) } else {
        Err(RuntimeError::new(400, line, format!("{what}: mode must be \"cbc\", \"ctr\" or \"ecb\", got \"{mode}\"")))
    }
}

pub fn aes_encrypt(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let what = "aes_encrypt";
    let data = input_bytes(args, kwargs, line, what)?;
    let aes = Aes::new(&key_bytes(&str_arg(args, 1, line, what)?, &[16, 24, 32], 32)).map_err(err(what, line))?;
    let mode = aes_mode(kwargs, line, what)?;
    let out = if mode == "ecb" {
        aes_ecb(&aes, &data, false).map_err(err(what, line))?
    } else {
        let (iv, prepend) = iv_or_random::<16>(kwargs, "iv").map_err(err(what, line))?;
        let body = if mode == "cbc" { aes_cbc(&aes, &iv, &data, false).map_err(err(what, line))? } else { aes_ctr(&aes, &iv, &data) };
        if prepend { [iv.as_slice(), &body].concat() } else { body }
    };
    Ok(emit_text(ctx, what, bytes_to_hex(&out)))
}

pub fn aes_decrypt(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let what = "aes_decrypt";
    let data = ciphertext_arg(args, line, what)?;
    let aes = Aes::new(&key_bytes(&str_arg(args, 1, line, what)?, &[16, 24, 32], 32)).map_err(err(what, line))?;
    let mode = aes_mode(kwargs, line, what)?;
    let plain = if mode == "ecb" {
        aes_ecb(&aes, &data, true).map_err(err(what, line))?
    } else {
        let (iv, body) = iv_or_prefix::<16>(kwargs, "iv", &data).map_err(err(what, line))?;
        if mode == "cbc" { aes_cbc(&aes, &iv, body, true).map_err(err(what, line))? } else { aes_ctr(&aes, &iv, body) }
    };
    Ok(emit_text(ctx, what, output_bytes(plain, kwargs, line, what)?))
}

fn key32(key: &str) -> [u8; 32] {
    fixed(&key_bytes(key, &[32], 32), "key").expect("key_bytes returns 32 bytes here")
}

fn counter_arg(args: &[Value], kwargs: &Kwargs, line: usize, what: &str) -> Result<u32, RuntimeError> {
    let c = int_arg(args, 2, kwargs, "counter", 1, line, what)?;
    u32::try_from(c).map_err(|_| RuntimeError::new(400, line, format!("{what}: counter must fit in 32 bits")))
}

pub fn chacha20_encrypt(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let what = "chacha20_encrypt";
    let data = input_bytes(args, kwargs, line, what)?;
    let key = key32(&str_arg(args, 1, line, what)?);
    let counter = counter_arg(args, kwargs, line, what)?;
    let (nonce, prepend) = iv_or_random::<12>(kwargs, "nonce").map_err(err(what, line))?;
    let body = chacha20_xor(&key, counter, &nonce, &data);
    let out = if prepend { [nonce.as_slice(), &body].concat() } else { body };
    Ok(emit_text(ctx, what, bytes_to_hex(&out)))
}

pub fn chacha20_decrypt(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let what = "chacha20_decrypt";
    let data = ciphertext_arg(args, line, what)?;
    let key = key32(&str_arg(args, 1, line, what)?);
    let counter = counter_arg(args, kwargs, line, what)?;
    let (nonce, body) = iv_or_prefix::<12>(kwargs, "nonce", &data).map_err(err(what, line))?;
    Ok(emit_text(ctx, what, output_bytes(chacha20_xor(&key, counter, &nonce, body), kwargs, line, what)?))
}

pub fn poly1305(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let what = "poly1305";
    let msg = input_bytes(args, kwargs, line, what)?;
    let key: [u8; 32] = fixed(&hex_to_bytes(&str_arg(args, 1, line, what)?).map_err(err(what, line))?, "poly1305 one-time key")
        .map_err(err(what, line))?;
    Ok(emit_text(ctx, what, bytes_to_hex(&poly1305_tag(&msg, &key))))
}

fn aad_arg(kwargs: &Kwargs) -> Vec<u8> {
    kw_str(kwargs, "aad").unwrap_or_default().into_bytes()
}

pub fn chacha20_poly1305_encrypt(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let what = "chacha20_poly1305_encrypt";
    let data = input_bytes(args, kwargs, line, what)?;
    let key = key32(&str_arg(args, 1, line, what)?);
    let (nonce, prepend) = iv_or_random::<12>(kwargs, "nonce").map_err(err(what, line))?;
    let sealed = chacha20_poly1305_seal(&key, &nonce, &aad_arg(kwargs), &data);
    let out = if prepend { [nonce.as_slice(), &sealed].concat() } else { sealed };
    Ok(emit_text(ctx, what, bytes_to_hex(&out)))
}

pub fn chacha20_poly1305_decrypt(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let what = "chacha20_poly1305_decrypt";
    let data = ciphertext_arg(args, line, what)?;
    let key = key32(&str_arg(args, 1, line, what)?);
    let (nonce, body) = iv_or_prefix::<12>(kwargs, "nonce", &data).map_err(err(what, line))?;
    let plain = chacha20_poly1305_open(&key, &nonce, &aad_arg(kwargs), body).map_err(err(what, line))?;
    Ok(emit_text(ctx, what, output_bytes(plain, kwargs, line, what)?))
}

pub fn random_bytes(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let n = int_arg(args, 0, kwargs, "n", 32, line, "random_bytes")?;
    let n = usize::try_from(n).ok().filter(|n| (1..=4096).contains(n))
        .ok_or_else(|| RuntimeError::new(400, line, "random_bytes: n must be 1..=4096"))?;
    Ok(emit_text(ctx, "random_bytes", bytes_to_hex(&random_vec(n).map_err(err("random_bytes", line))?)))
}

// ---------------- password-based encryption ----------------

const TOKEN_PREFIX: &str = "rach1";
const DEFAULT_ITERATIONS: u32 = 200_000;

/// `rach1$<iterations>$<base64url(salt ‖ nonce ‖ ciphertext ‖ tag)>`: PBKDF2-HMAC-SHA256 turns
/// the password into a key (salted, deliberately slow), ChaCha20-Poly1305 encrypts *and*
/// authenticates — a wrong password or a flipped bit is detected, not silently decrypted into
/// garbage. Self-describing, so `decrypt` needs nothing but the password.
pub fn encrypt_with_password(plaintext: &[u8], password: &str, iterations: u32) -> Result<String, String> {
    let salt = random_vec(16)?;
    let nonce: [u8; 12] = fixed(&random_vec(12)?, "nonce")?;
    let key: [u8; 32] = fixed(&pbkdf2_bytes(Algo::Sha256, password.as_bytes(), &salt, iterations, 32), "key")?;
    let sealed = chacha20_poly1305_seal(&key, &nonce, TOKEN_PREFIX.as_bytes(), plaintext);
    let payload = [salt.as_slice(), &nonce, &sealed].concat();
    Ok(format!("{TOKEN_PREFIX}${iterations}${}", base64_encode_bytes(&payload, true)))
}

pub fn decrypt_with_password(token: &str, password: &str) -> Result<Vec<u8>, String> {
    let bad = || "not a Rach encrypted token (expected rach1$<iterations>$<data>)".to_string();
    let mut parts = token.trim().splitn(3, '$');
    if parts.next() != Some(TOKEN_PREFIX) { return Err(bad()); }
    let iterations: u32 = parts.next().and_then(|n| n.parse().ok()).filter(|n| *n >= 1).ok_or_else(bad)?;
    let payload = base64_decode_bytes(parts.next().ok_or_else(bad)?)?;
    if payload.len() < 16 + 12 + 16 { return Err("token is truncated".into()); }
    let (salt, rest) = payload.split_at(16);
    let (nonce, sealed) = rest.split_at(12);
    let key: [u8; 32] = fixed(&pbkdf2_bytes(Algo::Sha256, password.as_bytes(), salt, iterations, 32), "key")?;
    chacha20_poly1305_open(&key, &fixed(nonce, "nonce")?, TOKEN_PREFIX.as_bytes(), sealed)
}

pub fn encrypt(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let what = "encrypt";
    let data = input_bytes(args, kwargs, line, what)?;
    let password = str_arg(args, 1, line, what)?;
    if password.is_empty() { return Err(RuntimeError::new(400, line, "encrypt: password must not be empty")); }
    let iterations = int_arg(args, 2, kwargs, "iterations", i64::from(DEFAULT_ITERATIONS), line, what)?;
    let iterations = u32::try_from(iterations).ok().filter(|n| *n >= 1)
        .ok_or_else(|| RuntimeError::new(400, line, "encrypt: iterations must be >= 1"))?;
    Ok(emit_text(ctx, what, encrypt_with_password(&data, &password, iterations).map_err(err(what, line))?))
}

pub fn decrypt(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let what = "decrypt";
    let token = str_arg(args, 0, line, what)?;
    let password = str_arg(args, 1, line, what)?;
    let plain = decrypt_with_password(&token, &password).map_err(err(what, line))?;
    Ok(emit_text(ctx, what, output_bytes(plain, kwargs, line, what)?))
}
