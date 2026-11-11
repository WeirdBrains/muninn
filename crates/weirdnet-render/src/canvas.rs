//! An RGB pixel buffer with points, lines, boxes and 8x8 glyphs, written out as PNG.
use crate::{Error, Result};

pub(crate) type Rgb = [u8; 3];

pub(crate) struct Canvas {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
}

impl Canvas {
    pub fn new(w: usize, h: usize, bg: Rgb) -> Canvas {
        let mut px = Vec::with_capacity(w * h * 3);
        for _ in 0..w * h {
            px.extend_from_slice(&bg);
        }
        Canvas { w, h, px }
    }
    pub fn set(&mut self, x: i64, y: i64, c: Rgb) {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            let i = (y as usize * self.w + x as usize) * 3;
            self.px[i..i + 3].copy_from_slice(&c);
        }
    }
    /// A two-pixel-high line (Bresenham), as the charts have always been drawn.
    pub fn line(&mut self, (x0, y0): (i64, i64), (x1, y1): (i64, i64), c: Rgb) {
        let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
        let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
        let (mut x, mut y, mut err) = (x0, y0, dx + dy);
        loop {
            self.set(x, y, c);
            self.set(x, y + 1, c);
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }
    pub fn fill(&mut self, x: usize, y: usize, w: usize, h: usize, c: Rgb) {
        for yy in y..(y + h).min(self.h) {
            for xx in x..(x + w).min(self.w) {
                self.set(xx as i64, yy as i64, c);
            }
        }
    }
    pub fn hline(&mut self, x0: usize, x1: usize, y: usize, c: Rgb) {
        for x in x0..=x1 {
            self.set(x as i64, y as i64, c);
        }
    }
    pub fn vline(&mut self, x: usize, y0: usize, y1: usize, c: Rgb) {
        for y in y0..=y1 {
            self.set(x as i64, y as i64, c);
        }
    }
    /// An 8x8 glyph with its top left at (x, y); bit 0 of each row is the leftmost pixel.
    pub fn glyph(&mut self, x: usize, y: usize, g: [u8; 8], c: Rgb) {
        for (r, bits) in g.iter().enumerate() {
            for b in 0..8 {
                if bits & (1 << b) != 0 {
                    self.set((x + b) as i64, (y + r) as i64, c);
                }
            }
        }
    }
    pub fn png(&self) -> Result<Vec<u8>> {
        let mut out = vec![];
        {
            let mut e = ::png::Encoder::new(&mut out, self.w as u32, self.h as u32);
            e.set_color(::png::ColorType::Rgb);
            e.set_depth(::png::BitDepth::Eight);
            let mut wr = e.write_header().map_err(|e| Error::Encode(e.to_string()))?;
            wr.write_image_data(&self.px).map_err(|e| Error::Encode(e.to_string()))?;
        }
        Ok(out)
    }
}
