//! Line charts: a grey frame and one colored line per series, scaled to the finite values.
use crate::canvas::Canvas;
use crate::{Chart, Error, Limits, Result, check_size};

const COLORS: [[u8; 3]; 6] = [[31, 119, 180], [214, 39, 40], [44, 160, 44], [255, 127, 14], [148, 103, 189], [140, 86, 75]];
const NAMES: [&str; 6] = ["blue", "red", "green", "orange", "purple", "brown"];

fn check(c: &Chart, l: &Limits) -> Result<()> {
    if c.series.len() > l.max_series {
        return Err(Error::TooLarge(format!("{} series (at most {})", c.series.len(), l.max_series)));
    }
    let most = c.series.iter().map(|s| s.values.len()).chain([c.x.len()]).max().unwrap_or(0);
    if most > l.max_points {
        return Err(Error::TooLarge(format!("{most} points (at most {})", l.max_points)));
    }
    Ok(())
}

/// Draw the series over x as a `width`×`height` PNG. [`Error::Empty`] when no value (or no
/// x) is there to draw.
pub fn chart_png(c: &Chart, width: usize, height: usize, limits: &Limits) -> Result<Vec<u8>> {
    check_size(width, height, limits)?;
    check(c, limits)?;
    let (x, lines, w, h) = (&c.x, &c.series, width, height);
    let xs: Vec<f64> = x.iter().enumerate().map(|(i, v)| v.filter(|v| v.is_finite()).unwrap_or(i as f64)).collect();
    let all: Vec<f64> = lines.iter().flat_map(|l| l.values.iter().flatten().copied()).filter(|v| v.is_finite()).collect();
    if all.is_empty() || xs.is_empty() {
        return Err(Error::Empty);
    }
    let (lo, hi) = all.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(*v), b.max(*v)));
    let (xl, xh) = xs.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(*v), b.max(*v)));
    let (hi, lo) = if hi > lo { (hi, lo) } else { (hi + 1.0, lo - 1.0) };
    let xh = if xh > xl { xh } else { xl + 1.0 };
    let mut c = Canvas::new(w, h, [255; 3]);
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
    c.png()
}

/// The legend: series names with their colors' names, in drawing order.
pub fn legend(c: &Chart) -> String {
    c.series.iter().enumerate().map(|(i, l)| format!("{} ({})", l.name, NAMES[i % NAMES.len()])).collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Series;

    fn decode(b: &[u8]) -> (u32, u32, Vec<u8>) {
        let mut r = ::png::Decoder::new(std::io::Cursor::new(b)).read_info().unwrap();
        let mut buf = vec![0; r.output_buffer_size().unwrap()];
        let i = r.next_frame(&mut buf).unwrap();
        buf.truncate(i.buffer_size());
        (i.width, i.height, buf)
    }

    fn sample() -> Chart {
        Chart {
            x: vec![Some(0.0), Some(1.0), Some(2.0)],
            series: vec![
                Series { name: "a".into(), values: vec![Some(1.0), None, Some(3.0)] },
                Series { name: "b".into(), values: vec![Some(2.0); 3] },
            ],
        }
    }

    #[test]
    fn charts_draw_as_png() {
        let l = Limits::default();
        let png = chart_png(&sample(), 200, 100, &l).unwrap();
        let (w, h, px) = decode(&png);
        assert_eq!((w, h), (200, 100));
        let at = |x: usize, y: usize| px[(y * 200 + x) * 3..][..3].to_vec();
        assert_eq!(at(0, 0), vec![255, 255, 255], "background");
        assert_eq!(at(12, 12), vec![200, 200, 200], "frame corner");
        // series b is flat at 2, the middle of 1..3: a red line across the middle
        assert_eq!(at(100, 50), vec![214, 39, 40]);
        // series a starts at its low (1) bottom left, in blue
        assert_eq!(at(12, 88), vec![31, 119, 180]);
        assert_eq!(legend(&sample()), "a (blue), b (red)");
    }

    #[test]
    fn nothing_finite_is_empty_and_limits_hold() {
        let l = Limits::default();
        let none =
            Chart { x: vec![Some(0.0); 3], series: vec![Series { name: "n".into(), values: vec![None, Some(f64::NAN), Some(f64::INFINITY)] }] };
        assert_eq!(chart_png(&none, 10, 10, &l), Err(Error::Empty));
        assert_eq!(chart_png(&Chart::default(), 10, 10, &l), Err(Error::Empty));
        assert!(matches!(chart_png(&sample(), 0, 10, &l), Err(Error::Invalid(_))));
        assert!(matches!(chart_png(&sample(), 9_000, 10, &l), Err(Error::TooLarge(_))));
        let small = Limits { max_pixels: 100, ..l };
        assert!(matches!(chart_png(&sample(), 20, 20, &small), Err(Error::TooLarge(_))));
        let few = Limits { max_points: 2, ..l };
        assert!(matches!(chart_png(&sample(), 20, 20, &few), Err(Error::TooLarge(_))));
        let one = Limits { max_series: 1, ..l };
        assert!(matches!(chart_png(&sample(), 20, 20, &one), Err(Error::TooLarge(_))));
    }

    #[test]
    fn missing_x_values_fall_back_to_the_index() {
        let l = Limits::default();
        let mut c = sample();
        let a = chart_png(&c, 64, 48, &l).unwrap();
        c.x = vec![None, Some(f64::NAN), Some(2.0)];
        assert_eq!(chart_png(&c, 64, 48, &l).unwrap(), a);
    }
}
