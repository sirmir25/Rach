//! Image → ASCII: BMP (24/32-bit, 1/4/8-bit palettes, RLE4/RLE8, bottom-up and top-down) and
//! Netpbm decoding checked on hand-built files, plus mutation fuzzing of the parsers.
//! (During development every decoder was also cross-checked against ImageMagick's own decoder
//! on the same images; that isn't repeated here so CI doesn't need ImageMagick.)

use rach::ast::Value;
use rach::interpreter::{self, make_ctx};
use rach::stdlib::image::decode;
use rach::{lexer, parser};
use std::path::PathBuf;

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

fn temp_file(name: &str, bytes: &[u8]) -> PathBuf {
    let path = std::env::temp_dir().join(format!("rach-image-test-{}-{name}", std::process::id()));
    std::fs::write(&path, bytes).expect("write temp image");
    path
}

fn render(name: &str, bytes: &[u8], args: &str) -> String {
    let path = temp_file(name, bytes);
    let out = s(&format!("ascii_image(\"{}\"{args})", path.display().to_string().replace('\\', "/")));
    let _ = std::fs::remove_file(path);
    out
}

/// A BMP with a 40-byte header around already-encoded pixel `data`.
fn bmp(w: usize, h: usize, bpp: u16, compression: u32, palette: &[[u8; 3]], top_down: bool, data: Vec<u8>) -> Vec<u8> {
    let pal_bytes: Vec<u8> = palette.iter().flat_map(|[r, g, b]| [*b, *g, *r, 0]).collect();
    let off = 14 + 40 + pal_bytes.len();
    let mut f = Vec::new();
    f.extend(b"BM");
    f.extend(((off + data.len()) as u32).to_le_bytes());
    f.extend([0u8; 4]);
    f.extend((off as u32).to_le_bytes());
    f.extend(40u32.to_le_bytes());
    f.extend((w as i32).to_le_bytes());
    f.extend((if top_down { -(h as i32) } else { h as i32 }).to_le_bytes());
    f.extend(1u16.to_le_bytes());
    f.extend(bpp.to_le_bytes());
    f.extend(compression.to_le_bytes());
    f.extend((data.len() as u32).to_le_bytes());
    f.extend([0u8; 8]);
    f.extend((palette.len() as u32).to_le_bytes());
    f.extend([0u8; 4]);
    f.extend(pal_bytes);
    f.extend(data);
    f
}

/// 24-bit rows (top-first gray levels) → BMP pixel data in storage order, padded to 4 bytes.
fn rows24(rows: &[&[u8]], top_down: bool) -> Vec<u8> {
    let mut ordered: Vec<&[u8]> = rows.to_vec();
    if !top_down { ordered.reverse(); }
    ordered.iter().flat_map(|row| {
        let mut line: Vec<u8> = row.iter().flat_map(|g| [*g, *g, *g]).collect();
        while !line.len().is_multiple_of(4) { line.push(0); }
        line
    }).collect()
}

#[test]
fn pgm_left_black_right_white() {
    let mut pgm = b"P5\n4 2\n255\n".to_vec();
    pgm.extend([0, 0, 255, 255, 0, 0, 255, 255]);
    // 4 columns; 2 source rows at a 2:1 character aspect → 1 output row.
    assert_eq!(render("lr.pgm", &pgm, ", 4"), "  @@");
    assert_eq!(render("lr-inv.pgm", &pgm, ", 4, invert=true"), "@@", "invert flips (trailing spaces trimmed)");
}

#[test]
fn bmp_orientation_and_row_padding() {
    // 2×2 checkerboard, white top-left. 24-bit rows of 6 bytes pad to 8.
    let rows: [&[u8]; 2] = [&[255, 0], &[0, 255]];
    let bottom_up = bmp(2, 2, 24, 0, &[], false, rows24(&rows, false));
    let top_down = bmp(2, 2, 24, 0, &[], true, rows24(&rows, true));
    assert_eq!(render("bu.bmp", &bottom_up, ", 2, style=\"half\""), "▀▄");
    assert_eq!(render("td.bmp", &top_down, ", 2, style=\"half\""), "▀▄");
}

#[test]
fn palettes_and_rle_decode_to_the_same_image() {
    let pal = [[0, 0, 0], [255, 255, 255]];
    // 4×2 image: top row 0 1 1 0, bottom row 1 0 0 1 (indices).
    // Indices, top row first: 0 1 1 0 / 1 0 0 1. BMP stores rows bottom-up.
    let raw8 = [vec![1u8, 0, 0, 1], vec![0u8, 1, 1, 0]].concat();
    // Bottom row as an absolute run of 4, end-of-line; top row as runs (1×0)(2×1)(1×0); end.
    let rle8 = vec![0, 4, 1, 0, 0, 1, 0, 0, 1, 0, 2, 1, 1, 0, 0, 1];
    let raw4 = vec![0x10, 0x01, 0, 0, 0x01, 0x10, 0, 0];
    // Same shape in RLE4: a 4-bit run alternates the two nibbles of its value byte.
    let rle4 = vec![0, 4, 0x10, 0x01, 0, 0, 1, 0x00, 2, 0x11, 1, 0x00, 0, 1];
    let want = render("raw8.bmp", &bmp(4, 2, 8, 0, &pal, false, raw8), ", 4, style=\"half\"");
    assert_eq!(want, "▄▀▀▄");
    assert_eq!(render("rle8.bmp", &bmp(4, 2, 8, 1, &pal, false, rle8), ", 4, style=\"half\""), want);
    assert_eq!(render("raw4.bmp", &bmp(4, 2, 4, 0, &pal, false, raw4), ", 4, style=\"half\""), want);
    assert_eq!(render("rle4.bmp", &bmp(4, 2, 4, 2, &pal, false, rle4), ", 4, style=\"half\""), want);
    let mono = bmp(4, 2, 1, 0, &pal, false, vec![0b1001_0000, 0, 0, 0, 0b0110_0000, 0, 0, 0]);
    assert_eq!(render("mono.bmp", &mono, ", 4, style=\"half\""), want);
}

#[test]
fn netpbm_ascii_variants_with_comments() {
    let pbm = b"P1\n# a comment\n4 2\n1001\n0 1 1 0\n";
    assert_eq!(render("a.pbm", pbm, ", 4, style=\"half\""), "▄▀▀▄", "PBM 1 = black");
    let ppm = b"P3 2 1 255\n255 255 255  0 0 0\n";
    assert_eq!(render("a.ppm", ppm, ", 2"), "@");
}

#[test]
fn unsupported_and_broken_files_are_clean_errors() {
    for (name, bytes, needle) in [
        ("x.png", &b"\x89PNG\r\n\x1a\nrest"[..], "PNG isn't supported"),
        ("x.jpg", &b"\xff\xd8\xff\xe0"[..], "JPEG isn't supported"),
        ("x.bin", &b"hello"[..], "unrecognised image format"),
        ("x.pgm", &b"P5 4 4 255\n\x00\x01"[..], "truncated"),
    ] {
        let path = temp_file(name, bytes);
        let err = run(&format!("ascii_image(\"{}\")", path.display().to_string().replace('\\', "/"))).unwrap_err();
        let _ = std::fs::remove_file(path);
        assert!(err.contains(needle), "{name}: {err}");
    }
    assert!(run("ascii_image(\"/definitely/not/here.bmp\")").unwrap_err().contains("cannot read"));
}

#[test]
fn decoders_never_panic_on_truncated_or_corrupted_input() {
    let pal = [[0, 0, 0], [255, 255, 255]];
    let samples = vec![
        bmp(3, 3, 24, 0, &[], false, rows24(&[&[1, 2, 3], &[4, 5, 6], &[7, 8, 9]], false)),
        bmp(4, 2, 8, 1, &pal, false, vec![0, 4, 1, 0, 0, 1, 0, 0, 1, 0, 2, 1, 1, 0, 0, 1]),
        bmp(4, 2, 4, 2, &pal, false, vec![0, 4, 0x10, 0x01, 0, 0, 1, 0x00, 2, 0x11, 1, 0x00, 0, 1]),
        b"P6 2 2 65535\n\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f\x10\x11\x12\x13\x14\x15\x16\x17".to_vec(),
        b"P4 9 2\n\xff\x80\x00\x00".to_vec(),
    ];
    let mut rng = 0x1234_5678_9abc_def0u64;
    let mut next = || { rng ^= rng << 13; rng ^= rng >> 7; rng ^= rng << 17; rng };
    for sample in &samples {
        for cut in 0..sample.len() { let _ = decode(&sample[..cut]); }
        for _ in 0..2_000 {
            let mut m = sample.clone();
            for _ in 0..1 + next() % 4 {
                let i = (next() % m.len() as u64) as usize;
                m[i] = next() as u8;
            }
            let _ = decode(&m);
        }
    }
}
