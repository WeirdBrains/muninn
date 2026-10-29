//! Date-and-time picker popup for period/range start and end (keyboard and mouse).
use super::{App, Hit, block, centered};
use crate::theme::Theme;
use chrono::{DateTime, Datelike, Months, NaiveDate, NaiveDateTime, TimeDelta, Timelike, Utc};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

/// What the picked time is applied to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    ChartStart,
    ChartEnd,
    ListFrom,
    ListTo,
}

impl Target {
    pub fn title(self) -> &'static str {
        match self {
            Target::ChartStart => "Period start",
            Target::ChartEnd => "Period end",
            Target::ListFrom => "List from",
            Target::ListTo => "List to",
        }
    }
    fn clearable(self) -> bool {
        matches!(self, Target::ListFrom | Target::ListTo | Target::ChartEnd)
    }
}

/// Fields: 0 year, 1 month, 2 day, 3 hour, 4 minute.
pub const FIELDS: [&str; 5] = ["year", "mo", "dy", "hh", "mm"];

#[derive(Debug, Clone)]
pub struct DatePicker {
    pub target: Target,
    pub tile: u32,
    pub at: NaiveDateTime,
    pub field: usize,
    /// Digits typed into the current field, not yet applied.
    pub typed: String,
}

/// Mouse targets inside the picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickHit {
    Field(u8),
    Up(u8),
    Down(u8),
    Ok,
    Cancel,
    Clear,
}

impl DatePicker {
    pub fn new(target: Target, tile: u32, ms: i64) -> Self {
        let at = DateTime::<Utc>::from_timestamp_millis(ms).unwrap_or_default().naive_utc();
        let at = at.with_second(0).and_then(|t| t.with_nanosecond(0)).unwrap_or(at);
        DatePicker { target, tile, at, field: 0, typed: String::new() }
    }

    pub fn millis(&self) -> i64 {
        self.at.and_utc().timestamp_millis()
    }

    /// Move the selected field by `n` steps (months/years keep the day valid).
    pub fn step(&mut self, field: usize, n: i64) {
        self.commit();
        let t = self.at;
        self.at = match field {
            0 => shift_months(t, n * 12),
            1 => shift_months(t, n),
            2 => t + TimeDelta::days(n),
            3 => t + TimeDelta::hours(n),
            _ => t + TimeDelta::minutes(n),
        };
    }

    fn width(field: usize) -> usize {
        if field == 0 { 4 } else { 2 }
    }

    /// Apply typed digits to the current field (invalid values are ignored).
    pub fn commit(&mut self) {
        if self.typed.is_empty() {
            return;
        }
        let v: u32 = self.typed.parse().unwrap_or(0);
        self.typed.clear();
        let t = self.at;
        let set = match self.field {
            0 => NaiveDate::from_ymd_opt(v as i32, t.month(), t.day().min(days_in(v as i32, t.month()))).map(|d| d.and_time(t.time())),
            1 if (1..=12).contains(&v) => NaiveDate::from_ymd_opt(t.year(), v, t.day().min(days_in(t.year(), v))).map(|d| d.and_time(t.time())),
            2 => t.with_day(v),
            3 => t.with_hour(v),
            4 => t.with_minute(v),
            _ => None,
        };
        if let Some(n) = set {
            self.at = n;
        }
    }

    pub fn select(&mut self, field: usize) {
        self.commit();
        self.field = field.min(4);
    }

    pub fn text(&self) -> String {
        self.at.format("%Y-%m-%d %H:%M").to_string()
    }
}

fn days_in(y: i32, m: u32) -> u32 {
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    NaiveDate::from_ymd_opt(ny, nm, 1).and_then(|d| d.pred_opt()).map(|d| d.day()).unwrap_or(28)
}

fn shift_months(t: NaiveDateTime, n: i64) -> NaiveDateTime {
    let m = Months::new(n.unsigned_abs() as u32);
    if n >= 0 { t.checked_add_months(m) } else { t.checked_sub_months(m) }.unwrap_or(t)
}

/// Keys while the picker is open.
pub fn on_key(app: &mut App, k: KeyEvent) {
    let Some(super::modal::Modal::DateTime(p)) = app.modal.as_mut() else { return };
    match k.code {
        KeyCode::Esc => app.modal = None,
        KeyCode::Enter => apply(app),
        KeyCode::Left | KeyCode::BackTab => {
            let f = (p.field + 4) % 5;
            p.select(f);
        }
        KeyCode::Right | KeyCode::Tab => {
            let f = (p.field + 1) % 5;
            p.select(f);
        }
        // separators finish a partly typed field (`2017-8-5 9:30`)
        KeyCode::Char(':' | '-' | ' ' | '/') => {
            if !p.typed.is_empty() {
                let f = (p.field + 1).min(4);
                p.select(f);
            }
        }
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('+') => p.step(p.field, 1),
        KeyCode::Down | KeyCode::Char('j') => p.step(p.field, -1),
        KeyCode::PageUp => p.step(p.field, 10),
        KeyCode::PageDown => p.step(p.field, -10),
        KeyCode::Backspace => {
            p.typed.pop();
        }
        KeyCode::Char(c) if c.is_ascii_digit() => {
            p.typed.push(c);
            if p.typed.len() >= DatePicker::width(p.field) {
                let f = (p.field + 1).min(4);
                p.commit();
                p.field = f;
            }
        }
        KeyCode::Char('n') => {
            p.typed.clear();
            *p = DatePicker::new(p.target, p.tile, Utc::now().timestamp_millis());
        }
        KeyCode::Delete | KeyCode::Char('x') if p.target.clearable() => clear(app),
        _ => {}
    }
}

pub fn on_click(app: &mut App, h: PickHit) {
    let Some(super::modal::Modal::DateTime(p)) = app.modal.as_mut() else { return };
    match h {
        PickHit::Field(f) => p.select(f as usize),
        PickHit::Up(f) => {
            p.select(f as usize);
            p.step(f as usize, 1)
        }
        PickHit::Down(f) => {
            p.select(f as usize);
            p.step(f as usize, -1)
        }
        PickHit::Ok => apply(app),
        PickHit::Cancel => app.modal = None,
        PickHit::Clear => clear(app),
    }
}

fn apply(app: &mut App) {
    let Some(super::modal::Modal::DateTime(mut p)) = app.modal.take() else { return };
    p.commit();
    let (ms, id) = (p.millis(), p.tile);
    app.set_status(format!("{}: {}", p.target.title().to_lowercase(), p.text()));
    match p.target {
        Target::ChartStart => app.chart_set_start(id, ms),
        Target::ChartEnd => app.chart_set_end(id, Some(ms)),
        Target::ListFrom => app.list_set_range(id, Some(ms), None, true),
        Target::ListTo => app.list_set_range(id, None, Some(ms + 59_999), false),
    }
}

fn clear(app: &mut App) {
    let Some(super::modal::Modal::DateTime(p)) = app.modal.take() else { return };
    match p.target {
        Target::ChartEnd => {
            app.chart_set_end(p.tile, None);
            app.set_status("period end: live");
        }
        Target::ListFrom => app.list_set_range(p.tile, None, None, true),
        Target::ListTo => app.list_set_range(p.tile, None, None, false),
        Target::ChartStart => {}
    }
}

pub fn draw(p: &DatePicker, f: &mut Frame, area: Rect, t: &Theme, hits: &mut Vec<(Rect, Hit)>) -> Rect {
    let r = centered(area, 46.min(area.width), 11.min(area.height));
    f.render_widget(Clear, r);
    let title = format!("{} (UTC)", p.target.title());
    let b = block(&title, true, t);
    let inner = b.inner(r);
    f.render_widget(b, r);
    // layout: "  YYYY - MM - DD   HH : MM"
    let cols: [(u16, u16); 5] = [(2, 4), (9, 2), (14, 2), (19, 2), (24, 2)];
    let seps = [(6, " - "), (11, " - "), (16, "   "), (21, " : ")];
    let vals = [
        format!("{:04}", p.at.year()),
        format!("{:02}", p.at.month()),
        format!("{:02}", p.at.day()),
        format!("{:02}", p.at.hour()),
        format!("{:02}", p.at.minute()),
    ];
    let y0 = inner.y + 1;
    let mut up = vec![];
    let mut mid = vec![];
    let mut down = vec![];
    let mut labels = vec![];
    let mut x = 0u16;
    let pad = |v: &mut Vec<Span<'static>>, from: u16, to: u16| {
        if to > from {
            v.push(Span::raw(" ".repeat((to - from) as usize)));
        }
    };
    for (i, (cx, w)) in cols.iter().enumerate() {
        pad(&mut up, x, *cx);
        pad(&mut down, x, *cx);
        pad(&mut labels, x, *cx);
        if i > 0 {
            let (sx, s) = seps[i - 1];
            pad(&mut mid, x, sx);
            mid.push(Span::styled(s, Style::new().fg(t.muted)));
        } else {
            pad(&mut mid, x, *cx);
        }
        let arrow_pad = (*w - 1) / 2;
        up.push(Span::styled(format!("{}▲{}", " ".repeat(arrow_pad as usize), " ".repeat((*w - 1 - arrow_pad) as usize)), Style::new().fg(t.accent)));
        down.push(Span::styled(
            format!("{}▼{}", " ".repeat(arrow_pad as usize), " ".repeat((*w - 1 - arrow_pad) as usize)),
            Style::new().fg(t.accent),
        ));
        let text = if i == p.field && !p.typed.is_empty() { format!("{:_<w$}", p.typed, w = *w as usize) } else { vals[i].clone() };
        let st = if i == p.field {
            Style::new().bg(t.selection_bg).fg(t.selection_fg).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(t.fg).add_modifier(Modifier::BOLD)
        };
        mid.push(Span::styled(text, st));
        let lab = &FIELDS[i][..(*w as usize).min(FIELDS[i].len())];
        labels.push(Span::styled(format!("{lab:<w$}", w = *w as usize), Style::new().fg(t.muted)));
        x = cx + w;
        let fx = inner.x + cx;
        hits.push((Rect { x: fx, y: y0, width: *w, height: 1 }, Hit::Picker(PickHit::Up(i as u8))));
        hits.push((Rect { x: fx, y: y0 + 1, width: *w, height: 1 }, Hit::Picker(PickHit::Field(i as u8))));
        hits.push((Rect { x: fx, y: y0 + 2, width: *w, height: 1 }, Hit::Picker(PickHit::Down(i as u8))));
    }
    let lines = vec![
        Line::raw(""),
        Line::from(up),
        Line::from(mid),
        Line::from(down),
        Line::from(labels),
        Line::raw(""),
        Line::styled(" ←→ field  ↑↓ change  0-9 type  n now", Style::new().fg(t.muted)),
    ];
    f.render_widget(Paragraph::new(lines), Rect { y: inner.y, ..inner });
    let bar = Rect { x: inner.x + 1, y: inner.bottom().saturating_sub(1), width: inner.width.saturating_sub(2), height: 1 };
    let mut keys = vec![("Enter OK", KeyCode::Enter), ("Esc cancel", KeyCode::Esc)];
    if p.target.clearable() {
        keys.push((if p.target == Target::ChartEnd { "x live" } else { "x clear" }, KeyCode::Char('x')));
    }
    let start = hits.len();
    super::widgets::key_bar(f, bar, &keys, t, hits);
    for (_, h) in hits[start..].iter_mut() {
        *h = Hit::Picker(match h {
            Hit::Press(KeyCode::Enter) => PickHit::Ok,
            Hit::Press(KeyCode::Esc) => PickHit::Cancel,
            _ => PickHit::Clear,
        });
    }
    r
}
