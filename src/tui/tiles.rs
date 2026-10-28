//! Tile views: virtualized, sortable bar list and the chart; per-tile caches and keys.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]
use super::layout::{SeriesKey, Tile, View};
use super::{App, Hit};
use crate::ingest::time::fmt_ts;
use crate::model::{Bar, FLAG_ZERO_VOLUME};
use crate::store::{BarSort, Store};
use crate::theme::Theme;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

/// Runtime state of one list tile (not persisted).
#[derive(Debug, Clone, Default)]
pub struct ListCache {
    key: Option<(SeriesKey, BarSort, bool, Option<(i64, i64)>)>,
    pub total: i64,
    pub sel: i64,
    pub top: i64,
    rows: Vec<Bar>,
    rows_at: i64,
    pub height: i64,
}

impl ListCache {
    /// Load what the visible window needs (at most three screens of rows).
    pub fn ensure(&mut self, store: &Store, tile: &Tile, scope: Option<(i64, i64)>, height: i64) {
        self.height = height.max(1);
        let Some(series) = &tile.series else {
            self.total = 0;
            self.rows.clear();
            return;
        };
        let key = (series.clone(), tile.list.sort, tile.list.desc, scope);
        let mut f = series.filter();
        if let Some((a, b)) = scope {
            f.from = Some(a);
            f.to = Some(b);
        }
        if self.key.as_ref() != Some(&key) {
            self.total = store.count_filtered(&f).unwrap_or(0);
            self.rows.clear();
            self.rows_at = 0;
            self.key = Some(key);
        }
        self.sel = self.sel.clamp(0, (self.total - 1).max(0));
        if self.sel < self.top {
            self.top = self.sel;
        }
        if self.sel >= self.top + self.height {
            self.top = self.sel - self.height + 1;
        }
        self.top = self.top.clamp(0, (self.total - self.height).max(0));
        let need_end = (self.top + self.height).min(self.total);
        if self.top < self.rows_at || need_end > self.rows_at + self.rows.len() as i64 {
            let start = (self.top - self.height).max(0);
            self.rows = store.page_bars(&f, tile.list.sort, tile.list.desc, self.height * 3, start).unwrap_or_default();
            self.rows_at = start;
        }
    }

    pub fn row(&self, i: i64) -> Option<&Bar> {
        if i < self.rows_at {
            return None;
        }
        self.rows.get((i - self.rows_at) as usize)
    }

    pub fn selected(&self) -> Option<&Bar> {
        self.row(self.sel)
    }

    pub fn invalidate(&mut self) {
        self.key = None;
    }
}

/// Runtime state of one chart tile.
#[derive(Debug, Clone, Default)]
pub struct ChartCache {
    key: Option<(SeriesKey, u16, Option<(i64, i64)>)>,
    pub bars: Vec<Bar>,
}

impl ChartCache {
    pub fn ensure(&mut self, store: &Store, tile: &Tile, scope: Option<(i64, i64)>, width: u16) {
        let Some(series) = &tile.series else {
            self.bars.clear();
            return;
        };
        let key = (series.clone(), width, scope);
        if self.key.as_ref() == Some(&key) {
            return;
        }
        let mut f = series.filter();
        if let Some((a, b)) = scope {
            f.from = Some(a);
            f.to = Some(b);
        }
        let mut v = store.page_bars(&f, BarSort::Time, true, width as i64, 0).unwrap_or_default();
        v.reverse();
        self.bars = v;
        self.key = Some(key);
    }
    pub fn invalidate(&mut self) {
        self.key = None;
    }
}

#[derive(Debug, Clone, Default)]
pub struct TileCache {
    pub list: ListCache,
    pub chart: ChartCache,
}

pub const COLUMNS: [(BarSort, u16); 14] = [
    (BarSort::Time, 19),
    (BarSort::Open, 11),
    (BarSort::High, 11),
    (BarSort::Low, 11),
    (BarSort::Close, 11),
    (BarSort::Volume, 12),
    (BarSort::Change, 10),
    (BarSort::ChangePct, 8),
    (BarSort::Range, 10),
    (BarSort::Body, 10),
    (BarSort::UpperWick, 10),
    (BarSort::LowerWick, 10),
    (BarSort::BodyRange, 6),
    (BarSort::Flags, 5),
];

pub fn fmt_num(v: f64) -> String {
    if v.is_nan() {
        return "–".into();
    }
    let a = v.abs();
    if a >= 10_000.0 {
        format!("{v:.1}")
    } else if a >= 100.0 {
        format!("{v:.2}")
    } else if a >= 1.0 {
        format!("{v:.4}")
    } else {
        format!("{v:.6}")
    }
}

fn cell(k: BarSort, b: &Bar) -> String {
    match k {
        BarSort::Time => fmt_ts(b.ts),
        BarSort::ChangePct => {
            let v = k.value(b);
            if v.is_nan() { "–".into() } else { format!("{v:+.2}") }
        }
        BarSort::BodyRange => {
            let v = k.value(b);
            if v.is_nan() { "–".into() } else { format!("{v:.2}") }
        }
        BarSort::Flags => if b.flags & FLAG_ZERO_VOLUME != 0 { "∅".into() } else { String::new() },
        _ => fmt_num(k.value(b)),
    }
}

/// Title shown on a tile's border.
pub fn title(t: &Tile, cache: &TileCache) -> String {
    let series = t.series.as_ref().map(|s| s.label()).unwrap_or_else(|| "no series".into());
    let link = t.link.map(|l| format!(" ⛓{l}")).unwrap_or_default();
    match t.view {
        View::List => format!(
            "[{}] list · {series} · {} {} · {} bars{link}",
            t.id,
            t.list.sort.label(),
            if t.list.desc { "▼" } else { "▲" },
            cache.list.total
        ),
        View::Chart => format!("[{}] chart · {series}{link}", t.id),
    }
}

pub fn draw_list(f: &mut Frame, area: Rect, t: &Tile, cache: &mut ListCache, store: &Store, scope: Option<(i64, i64)>, focused: bool, th: &Theme, hits: &mut Vec<(Rect, Hit)>) {
    if area.height < 2 {
        return;
    }
    cache.ensure(store, t, scope, area.height as i64 - 1);
    let mut x = area.x;
    let mut header = vec![];
    let mut cols = vec![];
    for (i, (k, w)) in COLUMNS.iter().enumerate() {
        if x + w > area.right() {
            break;
        }
        let arrow = if *k == t.list.sort { if t.list.desc { "▼" } else { "▲" } } else { "" };
        let label = format!("{}{arrow}", k.label());
        let st = if *k == t.list.sort { Style::new().fg(th.accent).add_modifier(Modifier::BOLD) } else { Style::new().fg(th.header).add_modifier(Modifier::BOLD) };
        let text = if i == 0 { format!("{label:<w$}", w = *w as usize) } else { format!("{label:>w$}", w = *w as usize - 1) + " " };
        header.push(Span::styled(text, st));
        hits.push((Rect { x, y: area.y, width: *w, height: 1 }, Hit::ListHeader(t.id, i)));
        cols.push((*k, *w));
        x += w;
    }
    f.render_widget(Paragraph::new(Line::from(header)), Rect { height: 1, ..area });
    if t.series.is_none() {
        f.render_widget(Paragraph::new(" pick a series: `,` / `.` or the sidebar").style(Style::new().fg(th.muted)), Rect { y: area.y + 1, height: 1, ..area });
        return;
    }
    for r in 0..(area.height - 1) as i64 {
        let i = cache.top + r;
        let Some(b) = cache.row(i).copied() else { break };
        let selected = i == cache.sel;
        let mut spans = vec![];
        for (j, (k, w)) in cols.iter().enumerate() {
            let s = cell(*k, &b);
            let mut st = Style::new().fg(th.fg);
            if matches!(k, BarSort::Change | BarSort::ChangePct) {
                st = st.fg(if b.close >= b.open { th.up } else { th.down });
            }
            if *k == BarSort::Flags {
                st = st.fg(th.warning);
            }
            if selected {
                st = if focused { st.bg(th.selection_bg).fg(th.selection_fg) } else { st.add_modifier(Modifier::REVERSED) };
            }
            let text = if j == 0 { format!("{s:<w$}", w = *w as usize) } else { format!("{s:>w$} ", w = *w as usize - 1) };
            spans.push(Span::styled(text, st));
        }
        f.render_widget(Paragraph::new(Line::from(spans)), Rect { y: area.y + 1 + r as u16, height: 1, ..area });
    }
}

pub fn draw_chart(f: &mut Frame, area: Rect, t: &Tile, cache: &mut ChartCache, store: &Store, scope: Option<(i64, i64)>, th: &Theme) {
    cache.ensure(store, t, scope, area.width);
    if cache.bars.is_empty() {
        f.render_widget(Paragraph::new(" no bars").style(Style::new().fg(th.muted)), area);
        return;
    }
    f.render_widget(super::widgets::Candles { bars: &cache.bars, theme: th }, area);
}

/// Draw one tile with its border.
#[allow(clippy::too_many_arguments)]
pub fn draw_tile(f: &mut Frame, area: Rect, t: &Tile, cache: &mut TileCache, store: &Store, scope: Option<(i64, i64)>, focused: bool, th: &Theme, hits: &mut Vec<(Rect, Hit)>) {
    let title = title(t, cache);
    let block = Block::bordered()
        .title(Span::styled(format!(" {title} "), Style::new().fg(if focused { th.accent } else { th.header }).add_modifier(Modifier::BOLD)))
        .border_style(Style::new().fg(if focused { th.border_focus } else { th.border }))
        .style(Style::new().bg(th.bg).fg(th.fg));
    let inner = block.inner(area);
    f.render_widget(block, area);
    hits.push((area, Hit::Tile(t.id)));
    match t.view {
        View::List => draw_list(f, inner, t, &mut cache.list, store, scope, focused, th, hits),
        View::Chart => draw_chart(f, inner, t, &mut cache.chart, store, scope, th),
    }
}

/// Keys for the focused tile. Returns true when handled.
pub fn on_key(app: &mut App, k: KeyEvent) -> bool {
    let id = app.layout.focus;
    let view = app.layout.focused().view;
    match k.code {
        KeyCode::Char(',') => {
            app.cycle_series(-1);
            return true;
        }
        KeyCode::Char('.') => {
            app.cycle_series(1);
            return true;
        }
        _ => {}
    }
    if view == View::List {
        let c = &mut app.caches.entry(id).or_default().list;
        let page = c.height.max(1);
        match k.code {
            KeyCode::Down | KeyCode::Char('j') => c.sel += 1,
            KeyCode::Up | KeyCode::Char('k') => c.sel -= 1,
            KeyCode::PageDown => c.sel += page,
            KeyCode::PageUp => c.sel -= page,
            KeyCode::Home => c.sel = 0,
            KeyCode::End => c.sel = c.total - 1,
            KeyCode::Char('s') => {
                let t = app.layout.focused_mut();
                t.list.sort = t.list.sort.next();
                app.layout_changed();
            }
            KeyCode::Char('S') => {
                let t = app.layout.focused_mut();
                t.list.sort = t.list.sort.prev();
                app.layout_changed();
            }
            KeyCode::Char('r') => {
                let t = app.layout.focused_mut();
                t.list.desc = !t.list.desc;
                app.layout_changed();
            }
            _ => return false,
        }
        let c = &mut app.caches.entry(id).or_default().list;
        c.sel = c.sel.clamp(0, (c.total - 1).max(0));
        return true;
    }
    false
}
