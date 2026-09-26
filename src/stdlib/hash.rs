//! Hashes and MACs, hand-rolled (no crates): CRC-32 (IEEE), Adler-32, FNV-1a (32/64),
//! MD5, SHA-1, SHA-256, SHA-512, HMAC over any of the four, and PBKDF2-HMAC.
//!
//! MD5 and SHA-1 are here because real formats still use them for checksums — both are
//! broken for collision resistance, so never use them to protect anything. These
//! implementations are straightforward, not constant-time: fine for checksums, learning and
//! CTFs, not for verifying secrets against an attacker who can time you.

use crate::ast::Value;
use crate::interpreter::{Ctx, RuntimeError};
use crate::stdlib::args::{bytes_to_hex, emit_text, input_bytes, int_arg, kw_str, str_arg, Kwargs};
use crate::stdlib::encoding::base64_encode_bytes;

// Constants generated with exact integer arithmetic (fractional parts of cube/square roots of
// the first primes; |sin(i+1)|·2³² for MD5), not transcribed by hand.

const MD5_K: [u32; 64] = [
    0xd76a_a478, 0xe8c7_b756, 0x2420_70db, 0xc1bd_ceee, 0xf57c_0faf, 0x4787_c62a, 0xa830_4613, 0xfd46_9501,
    0x6980_98d8, 0x8b44_f7af, 0xffff_5bb1, 0x895c_d7be, 0x6b90_1122, 0xfd98_7193, 0xa679_438e, 0x49b4_0821,
    0xf61e_2562, 0xc040_b340, 0x265e_5a51, 0xe9b6_c7aa, 0xd62f_105d, 0x0244_1453, 0xd8a1_e681, 0xe7d3_fbc8,
    0x21e1_cde6, 0xc337_07d6, 0xf4d5_0d87, 0x455a_14ed, 0xa9e3_e905, 0xfcef_a3f8, 0x676f_02d9, 0x8d2a_4c8a,
    0xfffa_3942, 0x8771_f681, 0x6d9d_6122, 0xfde5_380c, 0xa4be_ea44, 0x4bde_cfa9, 0xf6bb_4b60, 0xbebf_bc70,
    0x289b_7ec6, 0xeaa1_27fa, 0xd4ef_3085, 0x0488_1d05, 0xd9d4_d039, 0xe6db_99e5, 0x1fa2_7cf8, 0xc4ac_5665,
    0xf429_2244, 0x432a_ff97, 0xab94_23a7, 0xfc93_a039, 0x655b_59c3, 0x8f0c_cc92, 0xffef_f47d, 0x8584_5dd1,
    0x6fa8_7e4f, 0xfe2c_e6e0, 0xa301_4314, 0x4e08_11a1, 0xf753_7e82, 0xbd3a_f235, 0x2ad7_d2bb, 0xeb86_d391,
];

const MD5_S: [u32; 64] = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22,
    5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20,
    4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23,
    6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
];

const SHA256_K: [u32; 64] = [
    0x428a_2f98, 0x7137_4491, 0xb5c0_fbcf, 0xe9b5_dba5, 0x3956_c25b, 0x59f1_11f1, 0x923f_82a4, 0xab1c_5ed5,
    0xd807_aa98, 0x1283_5b01, 0x2431_85be, 0x550c_7dc3, 0x72be_5d74, 0x80de_b1fe, 0x9bdc_06a7, 0xc19b_f174,
    0xe49b_69c1, 0xefbe_4786, 0x0fc1_9dc6, 0x240c_a1cc, 0x2de9_2c6f, 0x4a74_84aa, 0x5cb0_a9dc, 0x76f9_88da,
    0x983e_5152, 0xa831_c66d, 0xb003_27c8, 0xbf59_7fc7, 0xc6e0_0bf3, 0xd5a7_9147, 0x06ca_6351, 0x1429_2967,
    0x27b7_0a85, 0x2e1b_2138, 0x4d2c_6dfc, 0x5338_0d13, 0x650a_7354, 0x766a_0abb, 0x81c2_c92e, 0x9272_2c85,
    0xa2bf_e8a1, 0xa81a_664b, 0xc24b_8b70, 0xc76c_51a3, 0xd192_e819, 0xd699_0624, 0xf40e_3585, 0x106a_a070,
    0x19a4_c116, 0x1e37_6c08, 0x2748_774c, 0x34b0_bcb5, 0x391c_0cb3, 0x4ed8_aa4a, 0x5b9c_ca4f, 0x682e_6ff3,
    0x748f_82ee, 0x78a5_636f, 0x84c8_7814, 0x8cc7_0208, 0x90be_fffa, 0xa450_6ceb, 0xbef9_a3f7, 0xc671_78f2,
];

const SHA256_H: [u32; 8] = [
    0x6a09_e667, 0xbb67_ae85, 0x3c6e_f372, 0xa54f_f53a, 0x510e_527f, 0x9b05_688c, 0x1f83_d9ab, 0x5be0_cd19,
];

const SHA512_K: [u64; 80] = [
    0x428a_2f98_d728_ae22, 0x7137_4491_23ef_65cd, 0xb5c0_fbcf_ec4d_3b2f, 0xe9b5_dba5_8189_dbbc,
    0x3956_c25b_f348_b538, 0x59f1_11f1_b605_d019, 0x923f_82a4_af19_4f9b, 0xab1c_5ed5_da6d_8118,
    0xd807_aa98_a303_0242, 0x1283_5b01_4570_6fbe, 0x2431_85be_4ee4_b28c, 0x550c_7dc3_d5ff_b4e2,
    0x72be_5d74_f27b_896f, 0x80de_b1fe_3b16_96b1, 0x9bdc_06a7_25c7_1235, 0xc19b_f174_cf69_2694,
    0xe49b_69c1_9ef1_4ad2, 0xefbe_4786_384f_25e3, 0x0fc1_9dc6_8b8c_d5b5, 0x240c_a1cc_77ac_9c65,
    0x2de9_2c6f_592b_0275, 0x4a74_84aa_6ea6_e483, 0x5cb0_a9dc_bd41_fbd4, 0x76f9_88da_8311_53b5,
    0x983e_5152_ee66_dfab, 0xa831_c66d_2db4_3210, 0xb003_27c8_98fb_213f, 0xbf59_7fc7_beef_0ee4,
    0xc6e0_0bf3_3da8_8fc2, 0xd5a7_9147_930a_a725, 0x06ca_6351_e003_826f, 0x1429_2967_0a0e_6e70,
    0x27b7_0a85_46d2_2ffc, 0x2e1b_2138_5c26_c926, 0x4d2c_6dfc_5ac4_2aed, 0x5338_0d13_9d95_b3df,
    0x650a_7354_8baf_63de, 0x766a_0abb_3c77_b2a8, 0x81c2_c92e_47ed_aee6, 0x9272_2c85_1482_353b,
    0xa2bf_e8a1_4cf1_0364, 0xa81a_664b_bc42_3001, 0xc24b_8b70_d0f8_9791, 0xc76c_51a3_0654_be30,
    0xd192_e819_d6ef_5218, 0xd699_0624_5565_a910, 0xf40e_3585_5771_202a, 0x106a_a070_32bb_d1b8,
    0x19a4_c116_b8d2_d0c8, 0x1e37_6c08_5141_ab53, 0x2748_774c_df8e_eb99, 0x34b0_bcb5_e19b_48a8,
    0x391c_0cb3_c5c9_5a63, 0x4ed8_aa4a_e341_8acb, 0x5b9c_ca4f_7763_e373, 0x682e_6ff3_d6b2_b8a3,
    0x748f_82ee_5def_b2fc, 0x78a5_636f_4317_2f60, 0x84c8_7814_a1f0_ab72, 0x8cc7_0208_1a64_39ec,
    0x90be_fffa_2363_1e28, 0xa450_6ceb_de82_bde9, 0xbef9_a3f7_b2c6_7915, 0xc671_78f2_e372_532b,
    0xca27_3ece_ea26_619c, 0xd186_b8c7_21c0_c207, 0xeada_7dd6_cde0_eb1e, 0xf57d_4f7f_ee6e_d178,
    0x06f0_67aa_7217_6fba, 0x0a63_7dc5_a2c8_98a6, 0x113f_9804_bef9_0dae, 0x1b71_0b35_131c_471b,
    0x28db_77f5_2304_7d84, 0x32ca_ab7b_40c7_2493, 0x3c9e_be0a_15c9_bebc, 0x431d_67c4_9c10_0d4c,
    0x4cc5_d4be_cb3e_42b6, 0x597f_299c_fc65_7e2a, 0x5fcb_6fab_3ad6_faec, 0x6c44_198c_4a47_5817,
];

const SHA512_H: [u64; 8] = [
    0x6a09_e667_f3bc_c908, 0xbb67_ae85_84ca_a73b, 0x3c6e_f372_fe94_f82b, 0xa54f_f53a_5f1d_36f1,
    0x510e_527f_ade6_82d1, 0x9b05_688c_2b3e_6c1f, 0x1f83_d9ab_fb41_bd6b, 0x5be0_cd19_137e_2179,
];

/// Merkle–Damgård padding: 0x80, zeros, then the bit length in `len_bytes` bytes.
fn md_pad(data: &[u8], block: usize, len_bytes: usize, little_endian: bool) -> Vec<u8> {
    let bit_len = (data.len() as u128) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % block != block - len_bytes { msg.push(0); }
    let len_be = bit_len.to_be_bytes();
    let len_field = &len_be[16 - len_bytes..];
    if little_endian {
        msg.extend(len_field.iter().rev());
    } else {
        msg.extend_from_slice(len_field);
    }
    msg
}

pub fn md5_bytes(data: &[u8]) -> Vec<u8> {
    let mut h: [u32; 4] = [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476];
    for chunk in md_pad(data, 64, 8, true).chunks(64) {
        let m: Vec<u32> = chunk.chunks(4).map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]])).collect();
        let [mut a, mut b, mut c, mut d] = h;
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let f = f.wrapping_add(a).wrapping_add(MD5_K[i]).wrapping_add(m[g]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(f.rotate_left(MD5_S[i]));
        }
        for (hi, v) in h.iter_mut().zip([a, b, c, d]) { *hi = hi.wrapping_add(v); }
    }
    h.iter().flat_map(|w| w.to_le_bytes()).collect()
}

pub fn sha1_bytes(data: &[u8]) -> Vec<u8> {
    let mut h: [u32; 5] = [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476, 0xc3d2_e1f0];
    for chunk in md_pad(data, 64, 8, false).chunks(64) {
        let mut w = [0u32; 80];
        for (i, word) in chunk.chunks(4).enumerate() { w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]); }
        for i in 16..80 { w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1); }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, wi) in w.iter().enumerate() {
            let (f, k) = match i / 20 {
                0 => ((b & c) | (!b & d), 0x5a82_7999),
                1 => (b ^ c ^ d, 0x6ed9_eba1),
                2 => ((b & c) | (b & d) | (c & d), 0x8f1b_bcdc),
                _ => (b ^ c ^ d, 0xca62_c1d6),
            };
            let t = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(*wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = t;
        }
        for (hi, v) in h.iter_mut().zip([a, b, c, d, e]) { *hi = hi.wrapping_add(v); }
    }
    h.iter().flat_map(|w| w.to_be_bytes()).collect()
}

pub fn sha256_bytes(data: &[u8]) -> Vec<u8> {
    let mut h = SHA256_H;
    for chunk in md_pad(data, 64, 8, false).chunks(64) {
        let mut w = [0u32; 64];
        for (i, word) in chunk.chunks(4).enumerate() { w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]); }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let big_s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh.wrapping_add(big_s1).wrapping_add(ch).wrapping_add(SHA256_K[i]).wrapping_add(w[i]);
            let big_s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = big_s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (hi, v) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) { *hi = hi.wrapping_add(v); }
    }
    h.iter().flat_map(|w| w.to_be_bytes()).collect()
}

pub fn sha512_bytes(data: &[u8]) -> Vec<u8> {
    let mut h = SHA512_H;
    for chunk in md_pad(data, 128, 16, false).chunks(128) {
        let mut w = [0u64; 80];
        for (i, word) in chunk.chunks(8).enumerate() {
            w[i] = u64::from_be_bytes([word[0], word[1], word[2], word[3], word[4], word[5], word[6], word[7]]);
        }
        for i in 16..80 {
            let s0 = w[i - 15].rotate_right(1) ^ w[i - 15].rotate_right(8) ^ (w[i - 15] >> 7);
            let s1 = w[i - 2].rotate_right(19) ^ w[i - 2].rotate_right(61) ^ (w[i - 2] >> 6);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..80 {
            let big_s1 = e.rotate_right(14) ^ e.rotate_right(18) ^ e.rotate_right(41);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh.wrapping_add(big_s1).wrapping_add(ch).wrapping_add(SHA512_K[i]).wrapping_add(w[i]);
            let big_s0 = a.rotate_right(28) ^ a.rotate_right(34) ^ a.rotate_right(39);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = big_s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (hi, v) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) { *hi = hi.wrapping_add(v); }
    }
    h.iter().flat_map(|w| w.to_be_bytes()).collect()
}

pub fn crc32_bytes(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 == 1 { (crc >> 1) ^ 0xedb8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

pub fn adler32_bytes(data: &[u8]) -> u32 {
    const MOD: u32 = 65_521;
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + u32::from(byte)) % MOD;
        b = (b + a) % MOD;
    }
    (b << 16) | a
}

pub fn fnv1a32_bytes(data: &[u8]) -> u32 {
    data.iter().fold(0x811c_9dc5u32, |h, b| (h ^ u32::from(*b)).wrapping_mul(0x0100_0193))
}

pub fn fnv1a64_bytes(data: &[u8]) -> u64 {
    data.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3))
}

#[derive(Clone, Copy)]
pub enum Algo { Md5, Sha1, Sha256, Sha512 }

impl Algo {
    pub fn parse(name: &str) -> Option<Algo> {
        match name.to_ascii_lowercase().replace('-', "").as_str() {
            "md5" => Some(Algo::Md5),
            "sha1" => Some(Algo::Sha1),
            "sha256" => Some(Algo::Sha256),
            "sha512" => Some(Algo::Sha512),
            _ => None,
        }
    }
    pub fn digest(self, data: &[u8]) -> Vec<u8> {
        match self {
            Algo::Md5 => md5_bytes(data),
            Algo::Sha1 => sha1_bytes(data),
            Algo::Sha256 => sha256_bytes(data),
            Algo::Sha512 => sha512_bytes(data),
        }
    }
    fn block_size(self) -> usize {
        match self { Algo::Sha512 => 128, _ => 64 }
    }
}

/// RFC 2104.
pub fn hmac_bytes(algo: Algo, key: &[u8], msg: &[u8]) -> Vec<u8> {
    let block = algo.block_size();
    let mut k = if key.len() > block { algo.digest(key) } else { key.to_vec() };
    k.resize(block, 0);
    let mut inner: Vec<u8> = k.iter().map(|b| b ^ 0x36).collect();
    inner.extend_from_slice(msg);
    let mut outer: Vec<u8> = k.iter().map(|b| b ^ 0x5c).collect();
    outer.extend(algo.digest(&inner));
    algo.digest(&outer)
}

/// RFC 8018 PBKDF2 with HMAC as the PRF.
pub fn pbkdf2_bytes(algo: Algo, password: &[u8], salt: &[u8], iterations: u32, len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(len);
    let mut block_index = 1u32;
    while out.len() < len {
        let mut salted = salt.to_vec();
        salted.extend_from_slice(&block_index.to_be_bytes());
        let mut u = hmac_bytes(algo, password, &salted);
        let mut t = u.clone();
        for _ in 1..iterations {
            u = hmac_bytes(algo, password, &u);
            for (ti, ui) in t.iter_mut().zip(&u) { *ti ^= ui; }
        }
        out.extend(t);
        block_index += 1;
    }
    out.truncate(len);
    out
}

// ---------------- Rach-facing commands ----------------

fn digest_out(bytes: &[u8], kwargs: &Kwargs, line: usize, what: &str) -> Result<String, RuntimeError> {
    match kw_str(kwargs, "format").as_deref() {
        None | Some("hex") => Ok(bytes_to_hex(bytes)),
        Some("base64") => Ok(base64_encode_bytes(bytes, false)),
        Some(other) => Err(RuntimeError::new(400, line, format!("{what}: format must be \"hex\" or \"base64\", got \"{other}\""))),
    }
}

fn algo_arg(kwargs: &Kwargs, line: usize, what: &str) -> Result<Algo, RuntimeError> {
    let name = kw_str(kwargs, "algo").unwrap_or_else(|| "sha256".into());
    Algo::parse(&name).ok_or_else(|| RuntimeError::new(400, line, format!("{what}: unknown algo \"{name}\" (md5, sha1, sha256, sha512)")))
}

fn digest_cmd(algo: Algo, what: &str, args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let data = input_bytes(args, kwargs, line, what)?;
    Ok(emit_text(ctx, what, digest_out(&algo.digest(&data), kwargs, line, what)?))
}

pub fn md5(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    digest_cmd(Algo::Md5, "md5", args, kwargs, line, ctx)
}
pub fn sha1(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    digest_cmd(Algo::Sha1, "sha1", args, kwargs, line, ctx)
}
pub fn sha256(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    digest_cmd(Algo::Sha256, "sha256", args, kwargs, line, ctx)
}
pub fn sha512(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    digest_cmd(Algo::Sha512, "sha512", args, kwargs, line, ctx)
}

/// Checksums: 8/16-digit hex by default (matches how they're usually printed), or the
/// number itself with `format="int"`.
fn checksum_cmd(what: &str, value: u64, hex_digits: usize, kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    match kw_str(kwargs, "format").as_deref() {
        None | Some("hex") => Ok(emit_text(ctx, what, format!("{value:0hex_digits$x}"))),
        Some("int") => {
            let n = i64::try_from(value)
                .map_err(|_| RuntimeError::new(400, line, format!("{what}: value exceeds Rach's 64-bit signed int; use format=\"hex\"")))?;
            if !ctx.capturing { println!("{what}: {n}"); println!("completed"); }
            Ok(Value::Int(n))
        }
        Some(other) => Err(RuntimeError::new(400, line, format!("{what}: format must be \"hex\" or \"int\", got \"{other}\""))),
    }
}

pub fn crc32(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let data = input_bytes(args, kwargs, line, "crc32")?;
    checksum_cmd("crc32", u64::from(crc32_bytes(&data)), 8, kwargs, line, ctx)
}
pub fn adler32(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let data = input_bytes(args, kwargs, line, "adler32")?;
    checksum_cmd("adler32", u64::from(adler32_bytes(&data)), 8, kwargs, line, ctx)
}
pub fn fnv1a(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let data = input_bytes(args, kwargs, line, "fnv1a")?;
    match int_arg(&[], 0, kwargs, "bits", 32, line, "fnv1a")? {
        32 => checksum_cmd("fnv1a", u64::from(fnv1a32_bytes(&data)), 8, kwargs, line, ctx),
        64 => checksum_cmd("fnv1a", fnv1a64_bytes(&data), 16, kwargs, line, ctx),
        other => Err(RuntimeError::new(400, line, format!("fnv1a: bits must be 32 or 64, got {other}"))),
    }
}

pub fn hmac(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let msg = input_bytes(args, kwargs, line, "hmac")?;
    let key = str_arg(args, 1, line, "hmac")?;
    let algo = algo_arg(kwargs, line, "hmac")?;
    Ok(emit_text(ctx, "hmac", digest_out(&hmac_bytes(algo, key.as_bytes(), &msg), kwargs, line, "hmac")?))
}

pub fn pbkdf2(args: &[Value], kwargs: &Kwargs, line: usize, ctx: &Ctx) -> Result<Value, RuntimeError> {
    let password = str_arg(args, 0, line, "pbkdf2")?;
    let salt = str_arg(args, 1, line, "pbkdf2")?;
    let iterations = int_arg(args, 2, kwargs, "iterations", 100_000, line, "pbkdf2")?;
    let length = int_arg(args, 3, kwargs, "length", 32, line, "pbkdf2")?;
    let iterations = u32::try_from(iterations).ok().filter(|n| *n >= 1)
        .ok_or_else(|| RuntimeError::new(400, line, "pbkdf2: iterations must be >= 1"))?;
    let length = usize::try_from(length).ok().filter(|n| (1..=1024).contains(n))
        .ok_or_else(|| RuntimeError::new(400, line, "pbkdf2: length must be 1..=1024 bytes"))?;
    let algo = algo_arg(kwargs, line, "pbkdf2")?;
    let key = pbkdf2_bytes(algo, password.as_bytes(), salt.as_bytes(), iterations, length);
    Ok(emit_text(ctx, "pbkdf2", digest_out(&key, kwargs, line, "pbkdf2")?))
}
