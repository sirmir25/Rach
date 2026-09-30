//! A small raster canvas (intensity 0.0..=1.0 per pixel) and the renderers that turn it into
//! text: Unicode braille (2×4 dots per character — the highest resolution a terminal cell can
//! show), half blocks (▀▄, square pixels), solid blocks, and grayscale character ramps.
//! Everything in `ascii_art` draws onto this and picks a renderer.

pub struct Canvas {
    pub w: usize,
    pub h: usize,
    px: Vec<f32>,
}

/// Terminal-friendly grayscale ramps, darkest first.
pub const RAMP_ASCII: &[char] = &[' ', '.', ':', '-', '=', '+', '*', '#', '%', '@'];
pub const RAMP_SHADE: &[char] = &[' ', '░', '▒', '▓', '█'];

#[derive(Clone, Copy)]
pub enum Render {
    /// 2×4 pixels per character via U+2800 braille; lit where intensity ≥ 0.5.
    Braille,
    /// 1×2 pixels per character via ▀ ▄ █.
    HalfBlocks,
    /// One pixel per `width` characters; lit pixels use `fill`, half-lit (0 < v < 1) use `half`.
    Blocks { fill: char, half: char, width: usize },
    /// One pixel per character, intensity mapped onto a ramp.
    Ramp(&'static [char]),
}

impl Render {
    /// How many canvas pixels one character cell covers (x, y).
    pub fn cell(self) -> (usize, usize) {
        match self {
            Render::Braille => (2, 4),
            Render::HalfBlocks => (1, 2),
            Render::Blocks { .. } | Render::Ramp(_) => (1, 1),
        }
    }

    /// Pixel aspect ratio (height / width) of this renderer's pixels on a typical terminal,
    /// where a character cell is about twice as tall as it is wide.
    pub fn pixel_aspect(self) -> f64 {
        match self {
            Render::Braille | Render::HalfBlocks => 1.0,
            Render::Blocks { width, .. } => 2.0 / width as f64,
            Render::Ramp(_) => 2.0,
        }
    }
}

impl Canvas {
    pub fn new(w: usize, h: usize) -> Self {
        Canvas { w, h, px: vec![0.0; w * h] }
    }

    pub fn get(&self, x: usize, y: usize) -> f32 {
        if x < self.w && y < self.h { self.px[y * self.w + x] } else { 0.0 }
    }

    /// Out-of-bounds writes are ignored, so shapes can be clipped by the canvas edge.
    pub fn set(&mut self, x: i64, y: i64, v: f32) {
        if let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) {
            if x < self.w && y < self.h { self.px[y * self.w + x] = v.clamp(0.0, 1.0); }
        }
    }

    /// Bresenham line.
    pub fn line(&mut self, x0: i64, y0: i64, x1: i64, y1: i64) {
        let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
        let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
        let (mut x, mut y, mut err) = (x0, y0, dx + dy);
        loop {
            self.set(x, y, 1.0);
            if x == x1 && y == y1 { break; }
            let e2 = 2 * err;
            if e2 >= dy { err += dy; x += sx; }
            if e2 <= dx { err += dx; y += sy; }
        }
    }

    /// Floyd–Steinberg error diffusion to pure black/white, so the 1-bit renderers can show
    /// gradients (photos) instead of a hard threshold's blotches.
    pub fn dither(&mut self) {
        for y in 0..self.h {
            for x in 0..self.w {
                let i = y * self.w + x;
                let old = self.px[i];
                let new = if old >= 0.5 { 1.0 } else { 0.0 };
                self.px[i] = new;
                let err = old - new;
                let mut spread = |dx: i64, dy: usize, k: f32| {
                    let (nx, ny) = (x as i64 + dx, y + dy);
                    if nx >= 0 && (nx as usize) < self.w && ny < self.h {
                        let j = ny * self.w + nx as usize;
                        self.px[j] = (self.px[j] + err * k).clamp(0.0, 1.0);
                    }
                };
                spread(1, 0, 7.0 / 16.0);
                spread(-1, 1, 3.0 / 16.0);
                spread(0, 1, 5.0 / 16.0);
                spread(1, 1, 1.0 / 16.0);
            }
        }
    }

    pub fn render(&self, mode: Render) -> String {
        let (cw, ch) = mode.cell();
        let cols = self.w.div_ceil(cw);
        let rows = self.h.div_ceil(ch);
        let lit = |x: usize, y: usize| self.get(x, y) >= 0.5;
        let mut lines: Vec<String> = Vec::with_capacity(rows);
        for row in 0..rows {
            let mut line = String::new();
            for col in 0..cols {
                let (x, y) = (col * cw, row * ch);
                match mode {
                    Render::Braille => {
                        const BITS: [[u32; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];
                        let mut bits = 0u32;
                        for (dx, column) in BITS.iter().enumerate() {
                            for (dy, bit) in column.iter().enumerate() {
                                if lit(x + dx, y + dy) { bits |= bit; }
                            }
                        }
                        line.push(if bits == 0 { ' ' } else { char::from_u32(0x2800 + bits).unwrap_or(' ') });
                    }
                    Render::HalfBlocks => line.push(match (lit(x, y), lit(x, y + 1)) {
                        (true, true) => '█',
                        (true, false) => '▀',
                        (false, true) => '▄',
                        (false, false) => ' ',
                    }),
                    Render::Blocks { fill, half, width } => {
                        let v = self.get(x, y);
                        let c = if v >= 0.75 { fill } else if v > 0.0 { half } else { ' ' };
                        line.extend(std::iter::repeat_n(c, width));
                    }
                    Render::Ramp(ramp) => {
                        let v = self.get(x, y);
                        line.push(ramp[((v * (ramp.len() - 1) as f32).round() as usize).min(ramp.len() - 1)]);
                    }
                }
            }
            lines.push(line.trim_end().to_string());
        }
        while lines.last().is_some_and(String::is_empty) { lines.pop(); }
        lines.join("\n")
    }
}
