//! Small widgets: text inputs and a candle chart.
use crate::model::Bar;
use crate::theme::Theme;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Widget;

/// A single-line text field.
#[derive(Debug, Clone, Default)]
pub struct Input {
    pub label: &'static str,
    pub value: String,
    pub masked: bool,
}

impl Input {
    pub fn new(label: &'static str) -> Self {
        Self { label, ..Default::default() }
    }
    pub fn masked(label: &'static str) -> Self {
        Self { label, masked: true, ..Default::default() }
    }
    pub fn with(mut self, v: &str) -> Self {
        self.value = v.to_string();
        self
    }
    pub fn shown(&self) -> String {
        if self.masked { "•".repeat(self.value.chars().count()) } else { self.value.clone() }
    }
    pub fn opt(&self) -> Option<String> {
        let v = self.value.trim();
        (!v.is_empty()).then(|| v.to_string())
    }
    /// Apply an edit key; returns true if handled.
    pub fn edit(&mut self, code: ratatui::crossterm::event::KeyCode) -> bool {
        use ratatui::crossterm::event::KeyCode;
        match code {
            KeyCode::Char(c) => {
                self.value.push(c);
                true
            }
            KeyCode::Backspace => {
                self.value.pop();
                true
            }
            _ => false,
        }
    }
}

/// Candlestick chart: one column per bar, wick `│`, body `█`, colored up/down.
pub struct Candles<'a> {
    pub bars: &'a [Bar],
    pub theme: &'a Theme,
}

impl Widget for Candles<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 || self.bars.is_empty() {
            return;
        }
        let n = (area.width as usize).min(self.bars.len());
        let bars = &self.bars[self.bars.len() - n..];
        let hi = bars.iter().map(|b| b.high).fold(f64::MIN, f64::max);
        let lo = bars.iter().map(|b| b.low).fold(f64::MAX, f64::min);
        let span = (hi - lo).max(f64::EPSILON);
        let h = area.height as f64 - 1.0;
        let row = |v: f64| area.y + (((hi - v) / span) * h).round() as u16;
        for (i, b) in bars.iter().enumerate() {
            let x = area.x + i as u16;
            let color = if b.close >= b.open { self.theme.up } else { self.theme.down };
            let st = Style::new().fg(color).bg(self.theme.bg);
            let (top, bot) = (row(b.high), row(b.low));
            let (bt, bb) = (row(b.open.max(b.close)), row(b.open.min(b.close)));
            for y in top..=bot {
                let ch = if y >= bt && y <= bb { "█" } else { "│" };
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_symbol(ch).set_style(st);
                }
            }
        }
    }
}

/// Synthetic bars for theme previews.
pub fn sample_bars(n: usize) -> Vec<Bar> {
    let mut v = Vec::with_capacity(n);
    let mut p = 100.0f64;
    for i in 0..n {
        let d = ((i as f64 * 0.7).sin() + (i as f64 * 0.23).cos()) * 1.5;
        let o = p;
        let c = p + d;
        let h = o.max(c) + 0.8 + (i % 3) as f64 * 0.3;
        let l = o.min(c) - 0.8 - (i % 2) as f64 * 0.4;
        v.push(Bar { ts: i as i64 * 300_000, open: o, high: h, low: l, close: c, volume: 1.0, flags: 0 });
        p = c;
    }
    v
}
