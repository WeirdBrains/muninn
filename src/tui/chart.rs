//! Chart tiles: period window, zoom/pan, and independently toggleable layers.
//!
//! Layers draw in a fixed z-order: grid < area < volume < candles/bars < lines
//! (line, envelope, moving averages) < markers (wicks, gaps, zero volume) < crosshair.
use super::tiles::fmt_num;
use crate::ingest::time::fmt_ts;
use crate::model::{Bar, PriceMetric, FLAG_ZERO_VOLUME};
use crate::theme::Theme;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layers {
    pub candles: bool,
    pub ohlc: bool,
    pub line: bool,
    pub area: bool,
    pub envelope: bool,
    pub volume: bool,
    pub ma: bool,
    pub wicks: bool,
    pub gaps: bool,
    pub zero_volume: bool,
    pub grid: bool,
    pub crosshair: bool,
    pub price_axis: bool,
    pub time_axis: bool,
}

impl Default for Layers {
    fn default() -> Self {
        Layers {
            candles: true,
            ohlc: false,
            line: false,
            area: false,
            envelope: false,
            volume: true,
            ma: false,
            wicks: false,
            gaps: true,
            zero_volume: false,
            grid: true,
            crosshair: true,
            price_axis: true,
            time_axis: true,
        }
    }
}

/// One layer: key, label and accessor (the order is the menu order).
pub const LAYERS: [(char, &str); 14] = [
    ('c', "Candles"),
    ('b', "OHLC bars"),
    ('n', "Line"),
    ('a', "Area under line"),
    ('e', "High/low envelope"),
    ('v', "Volume bars"),
    ('m', "Moving averages"),
    ('w', "Wick markers"),
    ('g', "Gap markers"),
    ('z', "Zero-volume markers"),
    ('#', "Grid"),
    ('x', "Crosshair + readout"),
    ('y', "Price axis"),
    ('t', "Time axis"),
];

impl Layers {
    pub fn get_mut(&mut self, key: char) -> Option<&mut bool> {
        Some(match key {
            'c' => &mut self.candles,
            'b' => &mut self.ohlc,
            'n' => &mut self.line,
            'a' => &mut self.area,
            'e' => &mut self.envelope,
            'v' => &mut self.volume,
            'm' => &mut self.ma,
            'w' => &mut self.wicks,
            'g' => &mut self.gaps,
            'z' => &mut self.zero_volume,
            '#' => &mut self.grid,
            'x' => &mut self.crosshair,
            'y' => &mut self.price_axis,
            't' => &mut self.time_axis,
            _ => return None,
        })
    }
    pub fn get(&self, key: char) -> bool {
        let mut c = *self;
        c.get_mut(key).map(|b| *b).unwrap_or(false)
    }
    pub fn toggle(&mut self, key: char) -> Option<bool> {
        let b = self.get_mut(key)?;
        *b = !*b;
        Some(*b)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MaKind {
    Sma,
    Ema,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ma {
    pub kind: MaKind,
    pub period: usize,
}

impl Ma {
    /// Parse `sma20,ema50`.
    pub fn parse_list(s: &str) -> Result<Vec<Ma>, String> {
        s.split(|c: char| c == ',' || c.is_whitespace())
            .filter(|p| !p.is_empty())
            .map(|p| {
                let p = p.to_ascii_lowercase();
                let (kind, n) = if let Some(n) = p.strip_prefix("sma") {
                    (MaKind::Sma, n)
                } else if let Some(n) = p.strip_prefix("ema") {
                    (MaKind::Ema, n)
                } else {
                    (MaKind::Sma, p.as_str())
                };
                let period: usize = n.parse().map_err(|_| format!("bad moving average `{p}` (use sma20, ema50)"))?;
                if !(2..=1000).contains(&period) {
                    return Err(format!("period {period} out of range 2..1000"));
                }
                Ok(Ma { kind, period })
            })
            .collect()
    }
    pub fn label(&self) -> String {
        format!("{}{}", if self.kind == MaKind::Sma { "sma" } else { "ema" }, self.period)
    }
}

fn default_mas() -> Vec<Ma> {
    vec![Ma { kind: MaKind::Sma, period: 20 }, Ma { kind: MaKind::Sma, period: 50 }]
}
fn one() -> u32 {
    1
}

/// Per-tile chart options (saved with the layout).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartOpts {
    #[serde(default)]
    pub layers: Layers,
    #[serde(default)]
    pub line_metric: PriceMetric,
    #[serde(default = "default_mas")]
    pub mas: Vec<Ma>,
    /// Right edge of the window (ms); none = the latest bar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<i64>,
    /// Bars per column (zoom); 1 = one bar per cell.
    #[serde(default = "one")]
    pub zoom: u32,
}

impl Default for ChartOpts {
    fn default() -> Self {
        ChartOpts { layers: Layers::default(), line_metric: PriceMetric::Close, mas: default_mas(), end: None, zoom: 1 }
    }
}

pub fn sma(v: &[f64], n: usize) -> Vec<Option<f64>> {
    let mut out = vec![None; v.len()];
    let mut sum = 0.0;
    for i in 0..v.len() {
        sum += v[i];
        if i >= n {
            sum -= v[i - n];
        }
        if i + 1 >= n {
            out[i] = Some(sum / n as f64);
        }
    }
    out
}

pub fn ema(v: &[f64], n: usize) -> Vec<Option<f64>> {
    let mut out = vec![None; v.len()];
    let k = 2.0 / (n as f64 + 1.0);
    let mut e: Option<f64> = None;
    for i in 0..v.len() {
        e = Some(match e {
            None => v[i],
            Some(p) => v[i] * k + p * (1.0 - k),
        });
        if i + 1 >= n {
            out[i] = e;
        }
    }
    out
}

/// Shooting star: long upper wick, small body near the low.
pub fn shooting_star(b: &Bar) -> bool {
    let body = (b.close - b.open).abs();
    let upper = b.high - b.open.max(b.close);
    let lower = b.open.min(b.close) - b.low;
    let range = b.high - b.low;
    range > 0.0 && upper >= 2.0 * body && upper >= 0.6 * range && lower <= 0.25 * range
}

/// Everything the renderer needs for one frame.
pub struct ChartData<'a> {
    /// Bars including warm-up history before `first` (for moving averages).
    pub bars: &'a [Bar],
    /// Index of the first visible bar.
    pub first: usize,
    pub bucket_ms: i64,
    /// Time of the first column (bucket aligned).
    pub start: i64,
    pub opts: &'a ChartOpts,
    /// Crosshair column (index into the visible bars).
    pub cursor: Option<usize>,
    pub theme: &'a Theme,
}

/// Areas inside a chart tile.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChartAreas {
    pub plot: Rect,
    pub volume: Option<Rect>,
    pub price_axis: Option<Rect>,
    pub time_axis: Option<Rect>,
}

pub fn areas(area: Rect, l: &Layers) -> ChartAreas {
    let mut r = area;
    let price_axis = (l.price_axis && r.width > 30).then(|| {
        let a = Rect { x: r.right() - 11, width: 11, ..r };
        r.width -= 11;
        a
    });
    let time_axis = (l.time_axis && r.height > 6).then(|| {
        let a = Rect { y: r.bottom() - 1, height: 1, ..r };
        r.height -= 1;
        a
    });
    let volume = (l.volume && r.height >= 10).then(|| {
        let h = (r.height / 4).max(3);
        let a = Rect { y: r.bottom() - h, height: h, ..r };
        r.height -= h;
        a
    });
    ChartAreas { plot: r, volume, price_axis, time_axis }
}

fn put(buf: &mut Buffer, x: u16, y: u16, s: &str, fg: Color, bg: Color) {
    if let Some(c) = buf.cell_mut((x, y)) {
        c.set_symbol(s).set_style(Style::new().fg(fg).bg(bg));
    }
}

fn empty(buf: &Buffer, x: u16, y: u16) -> bool {
    buf.cell((x, y)).map(|c| c.symbol() == " " || c.symbol() == "·").unwrap_or(false)
}

pub fn render(d: &ChartData, area: Rect, buf: &mut Buffer) {
    let t = d.theme;
    let l = &d.opts.layers;
    let vis = &d.bars[d.first.min(d.bars.len())..];
    if area.width < 4 || area.height < 3 || vis.is_empty() {
        return;
    }
    let a = areas(area, l);
    let p = a.plot;
    let bk = d.bucket_ms.max(1);
    let n = vis.iter().take_while(|b| b.ts < d.start + p.width as i64 * bk).count();
    let vis = &vis[..n];
    if vis.is_empty() {
        return;
    }
    let off = d.first.min(d.bars.len()); // index of vis[0] in d.bars
    // Bars are placed by time, so missing buckets stay empty.
    let xs: Vec<u16> = vis.iter().map(|b| p.x + ((b.ts - d.start).max(0) / bk).min(p.width as i64 - 1) as u16).collect();
    let metric: Vec<f64> = d.bars.iter().map(|b| d.opts.line_metric.value(b)).collect();
    let closes: Vec<f64> = d.bars.iter().map(|b| b.close).collect();
    let mas: Vec<(Ma, Vec<Option<f64>>)> = if l.ma {
        d.opts.mas.iter().map(|m| (*m, if m.kind == MaKind::Sma { sma(&closes, m.period) } else { ema(&closes, m.period) })).collect()
    } else {
        vec![]
    };
    let env_n = 20usize;
    let env: Vec<(f64, f64)> = (0..d.bars.len())
        .map(|i| {
            let s = i.saturating_sub(env_n - 1);
            let w = &d.bars[s..=i];
            (w.iter().map(|b| b.high).fold(f64::MIN, f64::max), w.iter().map(|b| b.low).fold(f64::MAX, f64::min))
        })
        .collect();
    // scale
    let mut hi = vis.iter().map(|b| b.high).fold(f64::MIN, f64::max);
    let mut lo = vis.iter().map(|b| b.low).fold(f64::MAX, f64::min);
    for (_, v) in &mas {
        for x in v[off..].iter().flatten() {
            hi = hi.max(*x);
            lo = lo.min(*x);
        }
    }
    if l.envelope {
        for (h, lw) in &env[off..] {
            hi = hi.max(*h);
            lo = lo.min(*lw);
        }
    }
    let span = (hi - lo).max(f64::EPSILON);
    let ph = p.height.max(1) as f64 - 1.0;
    let row = |v: f64| p.y + (((hi - v) / span) * ph).round().clamp(0.0, ph) as u16;
    let bg = t.bg;

    // grid
    if l.grid {
        for y in (p.y..p.bottom()).step_by(4) {
            for x in (p.x..p.right()).step_by(2) {
                put(buf, x, y, "·", t.border, bg);
            }
        }
        for x in (p.x..p.right()).step_by(16) {
            for y in p.y..p.bottom() {
                put(buf, x, y, "·", t.border, bg);
            }
        }
    }
    // area under line
    if l.area {
        for (i, _) in vis.iter().enumerate() {
            let x = xs[i];
            for y in row(metric[off + i])..p.bottom() {
                put(buf, x, y, "░", t.charts[1], bg);
            }
        }
    }
    // volume
    if let Some(va) = a.volume {
        let vmax = vis.iter().map(|b| b.volume).fold(0.0, f64::max).max(f64::EPSILON);
        const EIGHTHS: [&str; 9] = [" ", "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
        for (i, b) in vis.iter().enumerate() {
            let x = xs[i];
            let col = if b.close >= b.open { t.up } else { t.down };
            let total = ((b.volume / vmax) * va.height as f64 * 8.0).round() as u32;
            for r in 0..va.height {
                let y = va.bottom() - 1 - r;
                let fill = total.saturating_sub(r as u32 * 8).min(8) as usize;
                if fill > 0 {
                    put(buf, x, y, EIGHTHS[fill], col, bg);
                }
            }
            if l.zero_volume && b.flags & FLAG_ZERO_VOLUME != 0 {
                put(buf, x, va.bottom() - 1, "○", t.warning, bg);
            }
        }
    }
    // candles / OHLC bars
    for (i, b) in vis.iter().enumerate() {
        let x = xs[i];
        let col = if b.close >= b.open { t.up } else { t.down };
        let (top, bot) = (row(b.high), row(b.low));
        if l.candles {
            let (bt, bb) = (row(b.open.max(b.close)), row(b.open.min(b.close)));
            for y in top..=bot {
                let wick_up = y < bt;
                let ch = if y >= bt && y <= bb { "█" } else { "│" };
                let c = if l.wicks && wick_up && (b.high - b.open.max(b.close)) > (b.close - b.open).abs() { t.warning } else { col };
                put(buf, x, y, ch, c, bg);
            }
        } else if l.ohlc {
            let (ro, rc) = (row(b.open), row(b.close));
            for y in top..=bot {
                let ch = if y == ro && y == rc { "┼" } else if y == ro { "┤" } else if y == rc { "├" } else { "│" };
                put(buf, x, y, ch, col, bg);
            }
        }
    }
    // lines
    if l.line {
        let mut prev: Option<u16> = None;
        for i in 0..vis.len() {
            let x = xs[i];
            let y = row(metric[off + i]);
            if let Some(py) = prev {
                let (a0, a1) = if py < y { (py + 1, y) } else { (y + 1, py) };
                for yy in a0..a1 {
                    put(buf, x, yy, "│", t.charts[0], bg);
                }
            }
            put(buf, x, y, "•", t.charts[0], bg);
            prev = Some(y);
        }
    }
    if l.envelope {
        for i in 0..vis.len() {
            let x = xs[i];
            let (h, lw) = env[off + i];
            put(buf, x, row(h), "┄", t.charts[2], bg);
            put(buf, x, row(lw), "┄", t.charts[2], bg);
        }
    }
    for (k, (_, v)) in mas.iter().enumerate() {
        let c = t.charts[3 + k % 3];
        for i in 0..vis.len() {
            if let Some(m) = v[off + i] {
                put(buf, xs[i], row(m), "·", c, bg);
                if let Some(cell) = buf.cell_mut((xs[i], row(m))) {
                    cell.set_symbol("•");
                }
            }
        }
    }
    // markers
    if l.wicks {
        for (i, b) in vis.iter().enumerate() {
            if shooting_star(b) {
                let y = row(b.high).saturating_sub(1).max(p.y);
                put(buf, xs[i], y, "▼", t.warning, bg);
            }
        }
    }
    if l.gaps {
        for i in 1..vis.len() {
            if xs[i] > xs[i - 1] + 1 {
                let x = xs[i - 1] + 1;
                for y in p.y..p.bottom() {
                    if empty(buf, x, y) {
                        put(buf, x, y, "┊", t.warning, bg);
                    }
                }
            }
        }
    }
    if l.zero_volume && a.volume.is_none() {
        for (i, b) in vis.iter().enumerate() {
            if b.flags & FLAG_ZERO_VOLUME != 0 {
                put(buf, xs[i], p.bottom() - 1, "○", t.warning, bg);
            }
        }
    }
    // crosshair
    if l.crosshair
        && let Some(c) = d.cursor.filter(|c| *c < vis.len()) {
            let x = xs[c];
            let y = row(vis[c].close);
            for yy in p.y..p.bottom() {
                if empty(buf, x, yy) {
                    put(buf, x, yy, "│", t.muted, bg);
                }
            }
            for xx in p.x..p.right() {
                if empty(buf, xx, y) {
                    put(buf, xx, y, "─", t.muted, bg);
                }
            }
        }
    // axes
    if let Some(ax) = a.price_axis {
        for y in (p.y..p.bottom()).step_by(4).chain(std::iter::once(p.bottom() - 1)) {
            let v = hi - (y - p.y) as f64 / ph.max(1.0) * span;
            let s = format!(" {:>10}", fmt_num(v));
            for (j, ch) in s.chars().take(ax.width as usize).enumerate() {
                put(buf, ax.x + j as u16, y, &ch.to_string(), t.muted, bg);
            }
        }
    }
    if let Some(ax) = a.time_axis {
        let fmt = |ms: i64| {
            let s = fmt_ts(ms);
            if d.bucket_ms >= 86_400_000 { s[..10].to_string() } else { s[5..16].to_string() }
        };
        let mut x = 0usize;
        while x < p.width as usize {
            let label = format!("└{}", fmt(d.start + x as i64 * bk));
            if ax.x as usize + x + label.chars().count() > ax.right() as usize {
                break;
            }
            for (j, ch) in label.chars().enumerate() {
                put(buf, ax.x + (x + j) as u16, ax.y, &ch.to_string(), t.muted, bg);
            }
            x += 16;
        }
    }
}

/// One-line readout for the crosshair bar.
pub fn readout(b: &Bar) -> String {
    let pct = if b.open != 0.0 { (b.close - b.open) / b.open * 100.0 } else { 0.0 };
    format!(
        "{}  o {}  h {}  l {}  c {}  v {}  {:+.2}%{}",
        fmt_ts(b.ts),
        fmt_num(b.open),
        fmt_num(b.high),
        fmt_num(b.low),
        fmt_num(b.close),
        fmt_num(b.volume),
        pct,
        if b.flags & FLAG_ZERO_VOLUME != 0 { "  ∅" } else { "" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layers_defaults_and_toggles() {
        let mut l = Layers::default();
        let on: Vec<char> = LAYERS.iter().map(|(k, _)| *k).filter(|k| l.get(*k)).collect();
        assert_eq!(on, vec!['c', 'v', 'g', '#', 'x', 'y', 't']);
        assert_eq!(l.toggle('m'), Some(true));
        assert_eq!(l.toggle('c'), Some(false));
        assert_eq!(l.toggle('?'), None);
        let o = ChartOpts { layers: l, ..Default::default() };
        let s = toml::to_string(&o).unwrap();
        assert_eq!(toml::from_str::<ChartOpts>(&s).unwrap(), o);
        // missing fields fall back to defaults
        assert_eq!(toml::from_str::<ChartOpts>("").unwrap(), ChartOpts::default());
    }

    #[test]
    fn moving_averages() {
        let v = [1.0, 2.0, 3.0, 4.0, 5.0];
        assert_eq!(sma(&v, 3), vec![None, None, Some(2.0), Some(3.0), Some(4.0)]);
        let e = ema(&v, 3);
        assert!(e[1].is_none() && e[4].unwrap() > 3.5);
        assert_eq!(Ma::parse_list("sma20, ema50 10").unwrap().len(), 3);
        assert!(Ma::parse_list("foo").is_err());
        assert!(Ma::parse_list("sma1").is_err());
    }

    #[test]
    fn shooting_star_detection() {
        assert!(shooting_star(&Bar { ts: 0, open: 10.0, close: 10.2, high: 12.0, low: 9.9, volume: 1.0, flags: 0 }));
        assert!(!shooting_star(&Bar { ts: 0, open: 10.0, close: 12.0, high: 12.1, low: 9.9, volume: 1.0, flags: 0 }));
    }
}
