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

/// Draw the shared popup close control `[X]` on the top border of `r` (right side)
/// and return its rectangle for click handling.
pub fn close_button(f: &mut ratatui::Frame, r: ratatui::layout::Rect, t: &crate::theme::Theme) -> ratatui::layout::Rect {
    use ratatui::style::{Modifier, Style};
    let w = 5u16;
    if r.width < w + 4 || r.height == 0 {
        return ratatui::layout::Rect::default();
    }
    let x = ratatui::layout::Rect { x: r.right() - w - 2, y: r.y, width: w, height: 1 };
    f.render_widget(
        ratatui::widgets::Paragraph::new(ratatui::text::Span::styled(" [X] ".to_string(), Style::new().fg(t.error).add_modifier(Modifier::BOLD))),
        x,
    );
    x
}

/// A row of clickable key buttons, e.g. `[i Import] [A Folder]`: each records a
/// [`super::Hit::Press`] so a click does exactly what the key does. Returns the
/// rendered width.
pub fn key_bar(
    f: &mut ratatui::Frame,
    r: Rect,
    items: &[(&str, ratatui::crossterm::event::KeyCode)],
    t: &crate::theme::Theme,
    hits: &mut Vec<(Rect, super::Hit)>,
) -> u16 {
    use ratatui::text::{Line, Span};
    let mut x = r.x;
    let mut spans = vec![];
    for (label, code) in items {
        let text = format!("[{label}]");
        let w = text.chars().count() as u16;
        if x + w > r.right() {
            break;
        }
        hits.push((Rect { x, y: r.y, width: w, height: 1 }, super::Hit::Press(*code)));
        spans.push(Span::styled(text, Style::new().fg(t.accent)));
        spans.push(Span::raw(" "));
        x += w + 1;
    }
    f.render_widget(ratatui::widgets::Paragraph::new(Line::from(spans)), Rect { height: 1.min(r.height), ..r });
    x - r.x
}

/// Braille spinner frames; the frame advances every 100 ms of wall time.
pub const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

pub fn spinner_frame() -> char {
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    SPINNER[(ms / 100) as usize % SPINNER.len()]
}

/// Text of a progress bar `width` cells wide: `▕████░░░░▏ 42%  12,345 / 29,000 bars`.
pub fn progress_text(done: u64, total: u64, width: usize) -> String {
    let pct = (done.min(total) * 100).checked_div(total).unwrap_or(100) as usize;
    let w = width.max(4);
    let fill = (done.min(total) as usize * w).checked_div(total as usize).unwrap_or(w);
    format!("▕{}{}▏ {pct:>3}%  {} / {} bars", "█".repeat(fill), "░".repeat(w - fill), thousands(done), thousands(total))
}

fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// What a tile shows while its series loads: a progress bar when the total is known,
/// else a spinner; a failed load shows the error.
pub fn loading(f: &mut ratatui::Frame, area: Rect, p: &super::data::Pending, t: &crate::theme::Theme) {
    use super::data::Pending;
    use ratatui::text::Line;
    if area.height == 0 || area.width < 4 {
        return;
    }
    let (lines, st) = match p {
        Pending::Loading { done, total: Some(total) } => {
            let bar = progress_text(*done, *total, (area.width as usize).saturating_sub(34).clamp(4, 40));
            (vec![Line::from(" loading series…"), Line::from(format!(" {bar}"))], Style::new().fg(t.accent))
        }
        Pending::Loading { done, total: None } => {
            let extra = if *done > 0 { format!(" · {} bars", thousands(*done)) } else { String::new() };
            (vec![Line::from(format!(" {} loading series…{extra}", spinner_frame()))], Style::new().fg(t.accent))
        }
        Pending::Failed(e) => (vec![Line::from(format!(" could not load the series: {e}"))], Style::new().fg(t.error)),
    };
    let h = (lines.len() as u16).min(area.height);
    let y = area.y + area.height.saturating_sub(h) / 2;
    f.render_widget(ratatui::widgets::Paragraph::new(lines).style(st), Rect { y, height: h, ..area });
}
