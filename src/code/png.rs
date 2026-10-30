//! A plain line chart as PNG, for code-space charts in reports.
use super::view::{Axis, Line};

const COLORS: [[u8; 3]; 6] = [[31, 119, 180], [214, 39, 40], [44, 160, 44], [255, 127, 14], [148, 103, 189], [140, 86, 75]];

struct Canvas {
    w: usize,
    h: usize,
    px: Vec<u8>,
}

impl Canvas {
    fn set(&mut self, x: i64, y: i64, c: [u8; 3]) {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            let i = (y as usize * self.w + x as usize) * 3;
            self.px[i..i + 3].copy_from_slice(&c);
        }
    }
    fn line(&mut self, (x0, y0): (i64, i64), (x1, y1): (i64, i64), c: [u8; 3]) {
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
}

/// Draw the lines over x (`w`×`h` pixels). `None` when nothing is finite.
pub fn chart(x: &Axis, lines: &[Line], w: usize, h: usize) -> Option<Vec<u8>> {
    let xs: Vec<f64> = x.values.iter().enumerate().map(|(i, v)| v.filter(|v| v.is_finite()).unwrap_or(i as f64)).collect();
    let all: Vec<f64> = lines.iter().flat_map(|l| l.values.iter().flatten().copied()).filter(|v| v.is_finite()).collect();
    if all.is_empty() || xs.is_empty() {
        return None;
    }
    let (lo, hi) = all.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(*v), b.max(*v)));
    let (xl, xh) = xs.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(*v), b.max(*v)));
    let (hi, lo) = if hi > lo { (hi, lo) } else { (hi + 1.0, lo - 1.0) };
    let xh = if xh > xl { xh } else { xl + 1.0 };
    let mut c = Canvas { w, h, px: vec![255; w * h * 3] };
    let m = 12i64;
    let (pw, ph) = (w as i64 - 2 * m, h as i64 - 2 * m);
    let grey = [200, 200, 200];
    for (a, b) in [((m, m), (m + pw, m)), ((m, m + ph), (m + pw, m + ph)), ((m, m), (m, m + ph)), ((m + pw, m), (m + pw, m + ph))] {
        c.line(a, b, grey);
    }
    for (k, l) in lines.iter().enumerate() {
        let col = COLORS[k % COLORS.len()];
        let mut prev: Option<(i64, i64)> = None;
        for (i, v) in l.values.iter().enumerate() {
            let Some(v) = v.filter(|v| v.is_finite()) else {
                prev = None;
                continue;
            };
            let px = m + ((xs.get(i).copied().unwrap_or(i as f64) - xl) / (xh - xl) * pw as f64).round() as i64;
            let py = m + ph - ((v - lo) / (hi - lo) * ph as f64).round() as i64;
            match prev {
                Some(p) => c.line(p, (px, py), col),
                None => c.set(px, py, col),
            }
            prev = Some((px, py));
        }
    }
    let mut out = vec![];
    {
        let mut e = ::png::Encoder::new(&mut out, w as u32, h as u32);
        e.set_color(::png::ColorType::Rgb);
        e.set_depth(::png::BitDepth::Eight);
        let mut wr = e.write_header().ok()?;
        wr.write_image_data(&c.px).ok()?;
    }
    Some(out)
}

/// The legend: line names with their colors' names.
pub fn legend(lines: &[Line]) -> String {
    const NAMES: [&str; 6] = ["blue", "red", "green", "orange", "purple", "brown"];
    lines.iter().enumerate().map(|(i, l)| format!("{} ({})", l.name, NAMES[i % NAMES.len()])).collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charts_draw_as_png() {
        let x = Axis { name: "row".into(), time: false, values: vec![Some(0.0), Some(1.0), Some(2.0)] };
        let l = vec![Line { name: "a".into(), values: vec![Some(1.0), None, Some(3.0)] }, Line { name: "b".into(), values: vec![Some(2.0); 3] }];
        let png = chart(&x, &l, 200, 100).unwrap();
        assert_eq!(&png[1..4], b"PNG");
        assert!(chart(&x, &[Line { name: "n".into(), values: vec![None; 3] }], 10, 10).is_none());
        assert_eq!(legend(&l), "a (blue), b (red)");
    }
}
