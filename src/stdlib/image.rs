//! Minimal image decoders, no crates: Windows BMP (1/4/8-bit palette incl. RLE4/RLE8, 16/24/32-bit
//! incl. BI_BITFIELDS; bottom-up or top-down) and Netpbm (P1–P6: PBM/PGM/PPM, ASCII and binary,
//! 8- or 16-bit samples). Everything is decoded to grayscale brightness 0.0 (black) ..= 1.0
//! (white), which is all the ASCII renderers need.
//!
//! Every offset is bounds-checked: a truncated or hostile file is an `Err`, never a panic, and
//! dimensions are capped so a lying header can't make us allocate gigabytes.

pub struct Gray {
    pub w: usize,
    pub h: usize,
    pub px: Vec<f32>,
}

const MAX_PIXELS: usize = 50_000_000;

fn luma(r: f32, g: f32, b: f32) -> f32 {
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

fn check_dims(w: usize, h: usize) -> Result<(), String> {
    if w == 0 || h == 0 { return Err("image has zero width or height".into()); }
    if w.checked_mul(h).is_none_or(|n| n > MAX_PIXELS) {
        return Err(format!("image is too large ({w}x{h}); the limit is {MAX_PIXELS} pixels"));
    }
    Ok(())
}

pub fn decode(bytes: &[u8]) -> Result<Gray, String> {
    match bytes {
        [b'B', b'M', ..] => decode_bmp(bytes),
        [b'P', b'1'..=b'6', ..] => decode_pnm(bytes),
        [0x89, b'P', b'N', b'G', ..] => Err("PNG isn't supported (it needs a zlib inflater); convert with e.g. `magick in.png out.bmp`".into()),
        [0xff, 0xd8, ..] => Err("JPEG isn't supported; convert with e.g. `magick in.jpg out.bmp`".into()),
        _ => Err("unrecognised image format (supported: BMP, PBM/PGM/PPM)".into()),
    }
}

// ---------------- BMP ----------------

fn u16_at(b: &[u8], at: usize) -> Result<u16, String> {
    b.get(at..at + 2).map(|s| u16::from_le_bytes([s[0], s[1]])).ok_or_else(|| "BMP is truncated".to_string())
}

fn u32_at(b: &[u8], at: usize) -> Result<u32, String> {
    b.get(at..at + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]])).ok_or_else(|| "BMP is truncated".to_string())
}

/// Value of the bits selected by `mask`, scaled to 0..=1.
fn masked(px: u32, mask: u32) -> f32 {
    if mask == 0 { return 0.0; }
    let shift = mask.trailing_zeros();
    let max = mask >> shift;
    ((px & mask) >> shift) as f32 / max as f32
}

fn decode_bmp(b: &[u8]) -> Result<Gray, String> {
    let data_off = u32_at(b, 10)? as usize;
    let dib = u32_at(b, 14)? as usize;
    let (w, h_signed, bpp, compression, colors_used) = if dib == 12 {
        (i64::from(u16_at(b, 18)?), i64::from(u16_at(b, 20)?), u16_at(b, 24)?, 0, 0)
    } else if dib >= 40 {
        let w = i64::from(u32_at(b, 18)? as i32);
        let h = i64::from(u32_at(b, 22)? as i32);
        (w, h, u16_at(b, 28)?, u32_at(b, 30)?, u32_at(b, 46)? as usize)
    } else {
        return Err(format!("unsupported BMP header size {dib}"));
    };
    let top_down = h_signed < 0;
    let (w, h) = (usize::try_from(w).map_err(|_| "BMP width is negative")?, h_signed.unsigned_abs() as usize);
    check_dims(w, h)?;

    let masks = match (compression, bpp) {
        (0, _) | (1, 8) | (2, 4) => None,
        (3 | 6, 16 | 32) => {
            // Masks follow a 40-byte header; V4/V5 headers carry them in the same place.
            let at = 14 + 40;
            Some([u32_at(b, at)?, u32_at(b, at + 4)?, u32_at(b, at + 8)?])
        }
        (c, _) => return Err(format!("unsupported BMP compression {c} at {bpp} bits per pixel")),
    };

    let palette: Vec<f32> = if bpp <= 8 {
        let entry = if dib == 12 { 3 } else { 4 };
        let count = if colors_used == 0 { 1usize << bpp } else { colors_used.min(256) };
        let start = 14 + dib;
        (0..count).map(|i| {
            let p = b.get(start + i * entry..start + i * entry + 3).ok_or("BMP palette is truncated")?;
            Ok(luma(f32::from(p[2]) / 255.0, f32::from(p[1]) / 255.0, f32::from(p[0]) / 255.0))
        }).collect::<Result<_, &str>>()?
    } else {
        Vec::new()
    };

    if matches!(compression, 1 | 2) {
        if top_down { return Err("RLE BMPs must be stored bottom-up".into()); }
        let indices = decode_rle(b.get(data_off..).ok_or("BMP is truncated")?, w, h, compression == 2)?;
        let px = indices.iter().map(|&i| palette.get(usize::from(i)).copied().ok_or("BMP pixel indexes past the palette"))
            .collect::<Result<Vec<f32>, _>>()?;
        return Ok(Gray { w, h, px });
    }

    let stride = usize::from(bpp).checked_mul(w).ok_or("BMP dimensions overflow")?.div_ceil(32) * 4;
    let needed = stride.checked_mul(h).and_then(|n| n.checked_add(data_off)).ok_or("BMP dimensions overflow")?;
    if b.len() < needed { return Err("BMP pixel data is truncated".into()); }

    let mut px = vec![0.0f32; w * h];
    for row in 0..h {
        let src_row = if top_down { row } else { h - 1 - row };
        let line = &b[data_off + src_row * stride..data_off + (src_row + 1) * stride];
        for x in 0..w {
            let v = match bpp {
                1 | 4 | 8 => {
                    let bit = x * usize::from(bpp);
                    let byte = line[bit / 8];
                    let idx = (byte >> (8 - usize::from(bpp) - bit % 8)) & ((1u16 << bpp) - 1) as u8;
                    *palette.get(idx as usize).ok_or("BMP pixel indexes past the palette")?
                }
                16 => {
                    let p = u32::from(u16::from_le_bytes([line[2 * x], line[2 * x + 1]]));
                    let [r, g, bl] = masks.unwrap_or([0x7c00, 0x03e0, 0x001f]);
                    luma(masked(p, r), masked(p, g), masked(p, bl))
                }
                24 => {
                    let p = &line[3 * x..3 * x + 3];
                    luma(f32::from(p[2]) / 255.0, f32::from(p[1]) / 255.0, f32::from(p[0]) / 255.0)
                }
                32 => {
                    let p = u32::from_le_bytes([line[4 * x], line[4 * x + 1], line[4 * x + 2], line[4 * x + 3]]);
                    let [r, g, bl] = masks.unwrap_or([0x00ff_0000, 0x0000_ff00, 0x0000_00ff]);
                    luma(masked(p, r), masked(p, g), masked(p, bl))
                }
                other => return Err(format!("unsupported BMP bit depth {other}")),
            };
            px[row * w + x] = v;
        }
    }
    Ok(Gray { w, h, px })
}

/// BI_RLE8 / BI_RLE4 → palette indices, top row first. Runs are (count, value) pairs; a zero
/// count escapes to end-of-line (0), end-of-bitmap (1), a cursor delta (2), or an absolute run
/// of n literal indices padded to a 16-bit boundary (n ≥ 3). Rows are stored bottom-up.
fn decode_rle(data: &[u8], w: usize, h: usize, four_bit: bool) -> Result<Vec<u8>, String> {
    // Unlike raw pixel data, RLE doesn't have to be as large as the image it claims to be, so a
    // few hostile bytes could otherwise make us allocate for 50M pixels. Real RLE bitmaps are
    // nowhere near 2048:1; anything past that is rejected before allocating.
    if w * h / 2048 > data.len() {
        return Err(format!("RLE data ({} bytes) is implausibly small for a {w}x{h} image", data.len()));
    }
    let truncated = || "RLE data is truncated".to_string();
    let mut out = vec![0u8; w * h];
    let (mut x, mut row) = (0usize, 0usize); // row counts up from the bottom
    let mut i = 0;
    let mut put = |x: &mut usize, row: usize, v: u8| {
        if *x < w && row < h { out[(h - 1 - row) * w + *x] = v; }
        *x += 1;
    };
    while i + 1 < data.len() {
        let (count, value) = (usize::from(data[i]), data[i + 1]);
        i += 2;
        if count > 0 {
            for k in 0..count {
                let v = if four_bit { if k % 2 == 0 { value >> 4 } else { value & 0x0f } } else { value };
                put(&mut x, row, v);
            }
            continue;
        }
        match value {
            0 => { x = 0; row += 1; }
            1 => return Ok(out),
            2 => {
                let d = data.get(i..i + 2).ok_or_else(truncated)?;
                x += usize::from(d[0]);
                row += usize::from(d[1]);
                i += 2;
            }
            n => {
                let n = usize::from(n);
                let bytes = if four_bit { n.div_ceil(2) } else { n };
                let run = data.get(i..i + bytes).ok_or_else(truncated)?;
                for k in 0..n {
                    let v = if four_bit { let b = run[k / 2]; if k % 2 == 0 { b >> 4 } else { b & 0x0f } } else { run[k] };
                    put(&mut x, row, v);
                }
                i += bytes + bytes % 2;
            }
        }
        if row >= h { break; }
    }
    Ok(out)
}

// ---------------- Netpbm ----------------

struct Header<'a> {
    b: &'a [u8],
    pos: usize,
}

impl Header<'_> {
    fn skip_space_and_comments(&mut self) {
        while let Some(&c) = self.b.get(self.pos) {
            if c == b'#' {
                while self.b.get(self.pos).is_some_and(|c| *c != b'\n') { self.pos += 1; }
            } else if c.is_ascii_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn number(&mut self) -> Result<usize, String> {
        self.skip_space_and_comments();
        let start = self.pos;
        while self.b.get(self.pos).is_some_and(u8::is_ascii_digit) { self.pos += 1; }
        std::str::from_utf8(&self.b[start..self.pos]).ok()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| "malformed Netpbm header or sample".to_string())
    }
}

fn decode_pnm(b: &[u8]) -> Result<Gray, String> {
    let kind = b[1];
    let mut hdr = Header { b, pos: 2 };
    let (w, h) = (hdr.number()?, hdr.number()?);
    check_dims(w, h)?;
    let maxval = if matches!(kind, b'1' | b'4') { 1 } else { hdr.number()? };
    if !(1..=65_535).contains(&maxval) { return Err(format!("Netpbm maxval {maxval} is out of range")); }
    let channels = if matches!(kind, b'3' | b'6') { 3 } else { 1 };
    let n = w * h * channels;
    // Every ASCII sample takes at least one byte, so a header promising more samples than there
    // are bytes left is a lie — refuse it before allocating for it.
    if matches!(kind, b'1' | b'2' | b'3') && n > b.len().saturating_sub(hdr.pos) {
        return Err("Netpbm pixel data is truncated".into());
    }
    let norm = |s: usize| s.min(maxval) as f32 / maxval as f32;

    let samples: Vec<f32> = match kind {
        b'1' => {
            // ASCII bitmap: digits may be packed without spaces; 1 = black.
            let mut out = Vec::with_capacity(n);
            while out.len() < n {
                hdr.skip_space_and_comments();
                match b.get(hdr.pos) {
                    Some(b'0') => out.push(1.0),
                    Some(b'1') => out.push(0.0),
                    _ => return Err("PBM data is truncated or malformed".into()),
                }
                hdr.pos += 1;
            }
            out
        }
        b'2' | b'3' => (0..n).map(|_| hdr.number().map(norm)).collect::<Result<_, _>>()?,
        b'4' => {
            let start = hdr.pos + 1;
            let stride = w.div_ceil(8);
            let data = b.get(start..start + stride * h).ok_or("PBM data is truncated")?;
            (0..h).flat_map(|y| (0..w).map(move |x| (y, x)))
                .map(|(y, x)| if data[y * stride + x / 8] >> (7 - x % 8) & 1 == 1 { 0.0 } else { 1.0 })
                .collect()
        }
        _ => {
            let start = hdr.pos + 1; // exactly one whitespace byte after maxval
            let width = if maxval < 256 { 1 } else { 2 };
            let data = b.get(start..start + n * width).ok_or("Netpbm pixel data is truncated")?;
            data.chunks(width).map(|s| norm(if width == 1 { usize::from(s[0]) } else { usize::from(u16::from_be_bytes([s[0], s[1]])) })).collect()
        }
    };
    let px = if channels == 3 {
        samples.chunks(3).map(|c| luma(c[0], c[1], c[2])).collect()
    } else {
        samples
    };
    Ok(Gray { w, h, px })
}

/// Box-filter resample to `tw`×`th` (averages every source pixel under each target pixel, so
/// thin lines survive downscaling instead of aliasing away).
pub fn resample(img: &Gray, tw: usize, th: usize) -> Vec<f32> {
    let (sx, sy) = (img.w as f64 / tw as f64, img.h as f64 / th as f64);
    let mut out = vec![0.0f32; tw * th];
    for ty in 0..th {
        let y0 = ((ty as f64 * sy) as usize).min(img.h - 1);
        let y1 = (((ty + 1) as f64 * sy).ceil() as usize).clamp(y0 + 1, img.h);
        for tx in 0..tw {
            let x0 = ((tx as f64 * sx) as usize).min(img.w - 1);
            let x1 = (((tx + 1) as f64 * sx).ceil() as usize).clamp(x0 + 1, img.w);
            let mut sum = 0.0f32;
            for y in y0..y1 {
                sum += img.px[y * img.w + x0..y * img.w + x1].iter().sum::<f32>();
            }
            out[ty * tw + tx] = sum / ((x1 - x0) * (y1 - y0)) as f32;
        }
    }
    out
}
