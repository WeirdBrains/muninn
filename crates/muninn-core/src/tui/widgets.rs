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

// ---------- clickable key hints (2.15) ----------

use ratatui::crossterm::event::{KeyCode as KC, KeyModifiers as KM};

/// A key named in a hint: `Enter`, `Esc`, `Tab`, `S-Tab`, `Space`, `F1`, `Ctrl-S`,
/// `Ctrl+R`, `Alt-x`, `j/k` (the first), `↑↓`, `1-9`, or one character.
pub fn parse_key(tok: &str) -> Option<(KC, KM)> {
    let tok = tok.trim().trim_end_matches([':', ',', ';', ')']).trim_start_matches('(');
    let first = tok.split('/').next().unwrap_or(tok);
    let first = if first.chars().count() > 1 && first.contains(['-', '–']) && first.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        &first[..1]
    } else {
        first
    };
    let named = |n: &str| -> Option<KC> {
        Some(match n.to_ascii_lowercase().as_str() {
            "enter" | "return" => KC::Enter,
            "esc" | "escape" => KC::Esc,
            "tab" => KC::Tab,
            "s-tab" | "shift-tab" | "backtab" => KC::BackTab,
            "space" => KC::Char(' '),
            "del" | "delete" => KC::Delete,
            "backspace" => KC::Backspace,
            "pgup" => KC::PageUp,
            "pgdn" => KC::PageDown,
            "home" => KC::Home,
            "end" => KC::End,
            f if f.len() >= 2 && f.starts_with('f') && f[1..].parse::<u8>().is_ok_and(|n| (1..=12).contains(&n)) => KC::F(f[1..].parse().ok()?),
            _ => return None,
        })
    };
    if let Some(k) = named(first) {
        return Some((k, KM::NONE));
    }
    for (pre, m) in [("ctrl-", KM::CONTROL), ("ctrl+", KM::CONTROL), ("^", KM::CONTROL), ("alt-", KM::ALT), ("alt+", KM::ALT)] {
        if first.len() > pre.len() && first.to_ascii_lowercase().starts_with(pre) {
            let rest = &first[pre.len()..];
            if let Some(k) = named(rest) {
                return Some((k, m));
            }
            let mut cs = rest.chars();
            let (Some(c), None) = (cs.next(), cs.next()) else { return None };
            return Some((KC::Char(if m == KM::CONTROL { c.to_ascii_lowercase() } else { c }), m));
        }
    }
    let mut cs = first.chars();
    match (cs.next(), cs.next()) {
        (Some('↑'), _) => Some((KC::Up, KM::NONE)),
        (Some('↓'), _) => Some((KC::Down, KM::NONE)),
        (Some('←'), _) => Some((KC::Left, KM::NONE)),
        (Some('→'), _) => Some((KC::Right, KM::NONE)),
        (Some(c), None) if c.is_ascii_graphic() => Some((KC::Char(c), KM::NONE)),
        _ => None,
    }
}

/// The keys a hint text names, with the character span (start, len) of each key's
/// part of the text. A hint is a `·`-separated list (`Enter ok · Esc cancel`), a
/// confirmation (`y confirms`, `y/n`) or a parenthesis (`(Ctrl-T switches)`, `(f: all)`).
pub fn hint_keys(text: &str) -> Vec<(usize, usize, KC, KM)> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = vec![];
    let listy = text.contains('·');
    // segments between `·`
    let mut start = 0;
    let mut segs = vec![];
    for (i, c) in chars.iter().enumerate() {
        if *c == '·' {
            segs.push((start, i));
            start = i + 1;
        }
    }
    segs.push((start, chars.len()));
    let word_at = |from: usize, to: usize| -> Option<(usize, usize)> {
        let mut a = from;
        while a < to && chars[a] == ' ' {
            a += 1;
        }
        let mut b = a;
        while b < to && chars[b] != ' ' {
            b += 1;
        }
        (b > a).then_some((a, b))
    };
    for (i, &(a, b)) in segs.iter().enumerate() {
        let seg: String = chars[a..b].iter().collect();
        if listy {
            // the key comes first in a segment; in the first one it may follow a title (`Prefs · Esc`)
            if let Some((wa, wb)) = word_at(a, b) {
                let w: String = chars[wa..wb].iter().collect();
                let rest_ok = i > 0 || segs.len() == 1 || chars[wb..b].iter().collect::<String>().trim().len() < 40;
                if let Some((k, m)) = parse_key(&w).filter(|_| rest_ok) {
                    // a lone letter must be followed by a word (`n new`), not stand in prose
                    let lone = w.chars().count() == 1 && w.chars().all(|c| c.is_ascii_alphabetic());
                    if !lone || wb < b {
                        out.push((wa, wb - wa, k, m));
                        continue;
                    }
                }
            }
        }
        for pat in ["y confirms", "y/n"] {
            if let Some(p) = seg.find(pat) {
                let at = a + seg[..p].chars().count();
                out.push((at, 1, KC::Char('y'), KM::NONE));
            }
        }
        // `(Ctrl-T switches)`, `(f: all)`, `(Esc: keys)`
        let mut j = a;
        while j < b {
            if chars[j] == '('
                && let Some((wa, wb)) = word_at(j + 1, b)
            {
                let w: String = chars[wa..wb].iter().collect();
                let wt = w.trim_end_matches([':', ')']);
                let keyish = wt.chars().count() == 1 || parse_key(wt).is_some_and(|(k, m)| m != KM::NONE || !matches!(k, KC::Char(_)));
                if keyish
                    && (w.ends_with(':') || wb < b && chars[wb] == ' ')
                    && let Some((k, m)) = parse_key(wt)
                {
                    out.push((wa, wt.chars().count(), k, m));
                }
            }
            j += 1;
        }
    }
    out
}

/// Clickable regions for every key hint drawn in `area` of the buffer, in the given
/// colours (hints are muted, titles and warnings; content text is left alone).
pub fn scan_hints(buf: &ratatui::buffer::Buffer, area: Rect, colours: &[ratatui::style::Color]) -> Vec<(Rect, super::Hit)> {
    let mut out = vec![];
    let area = area.intersection(buf.area);
    for y in area.top()..area.bottom() {
        // runs of cells in a hint colour
        let mut run: Vec<(u16, String)> = vec![];
        let flush = |run: &mut Vec<(u16, String)>, out: &mut Vec<(Rect, super::Hit)>| {
            if run.is_empty() {
                return;
            }
            let text: String = run.iter().map(|(_, s)| s.as_str()).collect();
            // char index → x
            let mut xs = vec![];
            for (x, s) in run.iter() {
                for _ in s.chars() {
                    xs.push(*x);
                }
            }
            for (a, n, k, m) in hint_keys(&text) {
                let (Some(&x0), Some(&x1)) = (xs.get(a), xs.get(a + n - 1)) else { continue };
                out.push((Rect { x: x0, y, width: x1 - x0 + 1, height: 1 }, super::Hit::Chord(k, m)));
            }
            run.clear();
        };
        for x in area.left()..area.right() {
            let c = &buf[(x, y)];
            if c.symbol().is_empty() {
                continue; // the second half of a wide character
            }
            if colours.contains(&c.fg) {
                run.push((x, c.symbol().to_string()));
            } else {
                flush(&mut run, &mut out);
            }
        }
        flush(&mut run, &mut out);
    }
    out
}

/// Where URLs (http, https, muninn) are drawn in `area`, one line at a time.
pub fn scan_urls(buf: &ratatui::buffer::Buffer, area: Rect) -> Vec<(Rect, String)> {
    let mut out = vec![];
    let area = area.intersection(buf.area);
    for y in area.top()..area.bottom() {
        // the row's text, with each char's x
        let mut text = String::new();
        let mut xs = vec![];
        for x in area.left()..area.right() {
            let sym = buf[(x, y)].symbol();
            for ch in sym.chars() {
                text.push(ch);
                xs.push(x);
            }
        }
        out.extend(find_urls(&text).into_iter().filter_map(|(a, n)| {
            let chars: Vec<char> = text.chars().collect();
            let url: String = chars[a..a + n].iter().collect();
            let (x0, x1) = (*xs.get(a)?, *xs.get(a + n - 1)?);
            Some((Rect { x: x0, y, width: x1 - x0 + 1, height: 1 }, url))
        }));
    }
    out
}

/// (char start, char length) of every URL in `text`; trailing punctuation is left out.
pub fn find_urls(text: &str) -> Vec<(usize, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = vec![];
    let mut i = 0;
    while i < chars.len() {
        let rest: String = chars[i..chars.len().min(i + 10)].iter().collect();
        let scheme = ["https://", "http://", "muninn://"].iter().find(|s| rest.starts_with(**s));
        let boundary = i == 0 || !chars[i - 1].is_alphanumeric();
        if let (Some(sc), true) = (scheme, boundary) {
            let mut j = i + sc.len();
            while j < chars.len() && !chars[j].is_whitespace() && !matches!(chars[j], '"' | '<' | '>' | '`' | '│' | '┃' | '║') {
                j += 1;
            }
            while j > i + sc.len() && matches!(chars[j - 1], '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '\'' | '…') {
                // keep a closing bracket that has its opening one inside the URL
                let c = chars[j - 1];
                let open = match c {
                    ')' => Some('('),
                    ']' => Some('['),
                    _ => None,
                };
                if let Some(o) = open
                    && chars[i..j - 1].contains(&o)
                {
                    break;
                }
                j -= 1;
            }
            if j > i + sc.len() {
                out.push((i, j - i));
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    out
}

/// Write each URL again as an OSC 8 hyperlink, in the colours already on screen, so
/// terminals that support it make it a real link. The buffer is left as drawn.
pub fn paint_links(buf: &ratatui::buffer::Buffer, urls: &[(Rect, String)]) -> std::io::Result<()> {
    use ratatui::crossterm::{cursor, queue, style};
    use std::io::Write;
    if urls.is_empty() {
        return Ok(());
    }
    let mut out = std::io::stdout().lock();
    queue!(out, cursor::SavePosition)?;
    for (r, url) in urls {
        let safe: String = url.chars().filter(|c| !c.is_control()).collect();
        queue!(out, cursor::MoveTo(r.x, r.y))?;
        write!(out, "\x1b]8;;{safe}\x1b\\")?;
        for x in r.left()..r.right() {
            if !buf.area.contains((x, r.y).into()) {
                break;
            }
            let c = &buf[(x, r.y)];
            queue!(out, style::SetForegroundColor(ct(c.fg)), style::SetBackgroundColor(ct(c.bg)))?;
            write!(out, "{}", c.symbol())?;
        }
        write!(out, "\x1b]8;;\x1b\\")?;
    }
    queue!(out, style::ResetColor, cursor::RestorePosition)?;
    out.flush()
}

/// ratatui's colour as crossterm's.
fn ct(c: ratatui::style::Color) -> ratatui::crossterm::style::Color {
    use ratatui::crossterm::style::Color as X;
    use ratatui::style::Color as C;
    match c {
        C::Reset => X::Reset,
        C::Rgb(r, g, b) => X::Rgb { r, g, b },
        C::Indexed(i) => X::AnsiValue(i),
        C::Black => X::Black,
        C::Red => X::DarkRed,
        C::Green => X::DarkGreen,
        C::Yellow => X::DarkYellow,
        C::Blue => X::DarkBlue,
        C::Magenta => X::DarkMagenta,
        C::Cyan => X::DarkCyan,
        C::Gray => X::Grey,
        C::DarkGray => X::DarkGrey,
        C::LightRed => X::Red,
        C::LightGreen => X::Green,
        C::LightYellow => X::Yellow,
        C::LightBlue => X::Blue,
        C::LightMagenta => X::Magenta,
        C::LightCyan => X::Cyan,
        C::White => X::White,
    }
}

#[cfg(test)]
mod url_tests {
    use super::find_urls;

    fn urls(t: &str) -> Vec<String> {
        let c: Vec<char> = t.chars().collect();
        find_urls(t).into_iter().map(|(a, n)| c[a..a + n].iter().collect()).collect()
    }

    #[test]
    fn finds_urls_and_drops_trailing_punctuation() {
        assert_eq!(urls("site: https://lab.example.org:4743/x, ok"), vec!["https://lab.example.org:4743/x"]);
        assert_eq!(urls("(see http://a.b/c)."), vec!["http://a.b/c"]);
        assert_eq!(urls("https://en.wikipedia.org/wiki/A_(b)"), vec!["https://en.wikipedia.org/wiki/A_(b)"]);
        assert_eq!(urls("join muninn://join/abc│"), vec!["muninn://join/abc"]);
        assert!(urls("nohttps://x https:// plain").is_empty());
    }
}
