//! Tile views: virtualized, sortable bar list and the chart; per-tile caches and keys.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]
use super::data::SeriesData;
use super::layout::{SeriesKey, Tile, View};
use super::{App, Hit};
use crate::ingest::time::fmt_ts;
use crate::model::{Bar, FLAG_ZERO_VOLUME};
use crate::store::BarSort;
use crate::theme::Theme;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

type Data = Option<std::sync::Arc<SeriesData>>;

fn ptr(d: &Data) -> usize {
    d.as_ref().map(|d| std::sync::Arc::as_ptr(d) as usize).unwrap_or(0)
}

/// Runtime state of one list tile (not persisted).
#[derive(Debug, Clone, Default)]
pub struct ListCache {
    key: Option<(SeriesKey, BarSort, Option<(i64, i64)>, usize)>,
    pub total: i64,
    pub sel: i64,
    pub top: i64,
    pub height: i64,
    /// Series not loaded yet.
    pub loading: bool,
    data: Data,
    /// First bar index of the scope.
    lo: usize,
    /// Sorted bar indices (None = time order).
    perm: Option<std::sync::Arc<Vec<u32>>>,
    desc: bool,
}

impl ListCache {
    /// Point the list at the series data; sorts in memory when the sort or scope changes.
    pub fn ensure(&mut self, data: Data, tile: &Tile, scope: Option<(i64, i64)>, height: i64) {
        self.height = height.max(1);
        let Some(series) = &tile.series else {
            self.total = 0;
            self.data = None;
            self.loading = false;
            return;
        };
        self.loading = data.is_none();
        let key = (series.clone(), tile.list.sort, scope, ptr(&data));
        if self.key.as_ref() != Some(&key) {
            self.perm = None;
            match &data {
                Some(d) => {
                    let r = d.range(scope);
                    self.lo = r.start;
                    self.total = r.len() as i64;
                    if tile.list.sort != BarSort::Time {
                        self.perm = Some(std::sync::Arc::new(d.sorted(r, tile.list.sort)));
                    }
                }
                None => self.total = 0,
            }
            self.data = data;
            self.key = Some(key);
        }
        self.desc = tile.list.desc;
        self.sel = self.sel.clamp(0, (self.total - 1).max(0));
        if self.sel < self.top {
            self.top = self.sel;
        }
        if self.sel >= self.top + self.height {
            self.top = self.sel - self.height + 1;
        }
        self.top = self.top.clamp(0, (self.total - self.height).max(0));
    }

    pub fn row(&self, i: i64) -> Option<&Bar> {
        if i < 0 || i >= self.total {
            return None;
        }
        let j = if self.desc { self.total - 1 - i } else { i } as usize;
        let idx = match &self.perm {
            Some(p) => *p.get(j)? as usize,
            None => self.lo + j,
        };
        self.data.as_ref()?.bars.get(idx)
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
    key: Option<(SeriesKey, u16, i64, i64, Option<(i64, i64)>, usize, usize)>,
    pub bars: Vec<Bar>,
    pub first: usize,
    /// Plot width in columns and bucket size from the last draw.
    pub width: u16,
    pub bucket_ms: i64,
    pub dur_ms: i64,
    /// Window from the last draw.
    pub start: i64,
    pub end: i64,
    /// Data bounds of the series.
    pub data_first: i64,
    pub data_last: i64,
    /// Crosshair bar time.
    pub cursor: Option<i64>,
    pub loading: bool,
}

impl ChartCache {
    pub fn ensure(&mut self, data: Data, tile: &Tile, scope: Option<(i64, i64)>, width: u16, bounds: Option<(i64, i64)>) {
        self.loading = tile.series.is_some() && data.is_none();
        let Some(series) = &tile.series else {
            self.bars.clear();
            return;
        };
        let Some((mut lo, mut hi)) = bounds else {
            self.bars.clear();
            return;
        };
        if let Some((a, b)) = scope {
            lo = lo.max(a);
            hi = hi.min(b);
        }
        let dur = crate::model::Duration::parse(&series.duration).map(|d| d.millis()).unwrap_or(60_000).max(1);
        let o = &tile.chart;
        let bucket = dur * o.zoom.max(1) as i64;
        let w = width.max(1) as i64;
        let end = o.end.unwrap_or(hi).clamp(lo, hi.max(lo));
        let start = end - (w - 1) * bucket;
        let warm = if o.layers.ma { o.mas.iter().map(|m| m.period).max().unwrap_or(0) } else { 0 }.max(if o.layers.envelope { 20 } else { 0 });
        self.width = width;
        self.bucket_ms = bucket;
        self.dur_ms = dur;
        self.start = start;
        self.end = end;
        self.data_first = lo;
        self.data_last = hi;
        let key = (series.clone(), width, bucket, end, scope, warm, ptr(&data));
        if self.key.as_ref() == Some(&key) {
            return;
        }
        let from = (start - warm as i64 * bucket).max(lo);
        let mut to = end + bucket - 1;
        if let Some((_, b)) = scope {
            to = to.min(b);
        }
        self.bars = data.as_ref().map(|d| d.bucketed(from, to, bucket, dur)).unwrap_or_default();
        let aligned = start - start.rem_euclid(bucket);
        self.first = self.bars.iter().position(|b| b.ts >= aligned).unwrap_or(self.bars.len());
        self.key = Some(key);
    }
    pub fn invalidate(&mut self) {
        self.key = None;
    }
    /// First column time (bucket aligned).
    pub fn aligned_start(&self) -> i64 {
        let b = self.bucket_ms.max(1);
        self.start - self.start.rem_euclid(b)
    }
    pub fn visible(&self) -> &[Bar] {
        let v = &self.bars[self.first.min(self.bars.len())..];
        let lim = self.aligned_start() + self.width as i64 * self.bucket_ms.max(1);
        let n = v.iter().take_while(|b| b.ts < lim).count();
        &v[..n]
    }
    /// Index of the crosshair in the visible bars (defaults to the last bar).
    pub fn cursor_index(&self) -> Option<usize> {
        let v = self.visible();
        if v.is_empty() {
            return None;
        }
        Some(match self.cursor {
            Some(ts) => v.iter().position(|b| b.ts >= ts - ts.rem_euclid(self.bucket_ms.max(1))).unwrap_or(v.len() - 1),
            None => v.len() - 1,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct TileCache {
    pub list: ListCache,
    pub chart: ChartCache,
    /// Set before drawing when the tile's series is still loading (or failed).
    pub pending: Option<super::data::Pending>,
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
        BarSort::Flags => {
            if b.flags & FLAG_ZERO_VOLUME != 0 {
                "∅".into()
            } else {
                String::new()
            }
        }
        _ => fmt_num(k.value(b)),
    }
}

/// Title shown on a tile's border.
pub fn title(t: &Tile, cache: &TileCache) -> String {
    let series = t.series.as_ref().map(|s| s.label()).unwrap_or_else(|| "no series".into());
    let link = t.link.map(|l| format!(" ⛓{l}")).unwrap_or_default();
    match t.view {
        View::List => format!(
            "[{}] list · {series} · {} {} · {}{link}",
            t.id,
            t.list.sort.label(),
            if t.list.desc { "▼" } else { "▲" },
            if cache.list.loading { "loading…".to_string() } else { format!("{} bars", cache.list.total) }
        ),
        View::Chart => format!("[{}] chart · {series}{link}", t.id),
    }
}

pub fn draw_list(
    f: &mut Frame,
    area: Rect,
    t: &Tile,
    cache: &mut ListCache,
    data: Data,
    scope: Option<(i64, i64)>,
    focused: bool,
    th: &Theme,
    hits: &mut Vec<(Rect, Hit)>,
) {
    if area.height < 2 {
        return;
    }
    cache.ensure(data, t, scope, area.height as i64 - 1);
    let mut x = area.x;
    let mut header = vec![];
    let mut cols = vec![];
    for (i, (k, w)) in COLUMNS.iter().enumerate() {
        if x + w > area.right() {
            break;
        }
        let arrow = if *k == t.list.sort { if t.list.desc { "▼" } else { "▲" } } else { "" };
        let label = format!("{}{arrow}", k.label());
        let st = if *k == t.list.sort {
            Style::new().fg(th.accent).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(th.header).add_modifier(Modifier::BOLD)
        };
        let text = if i == 0 { format!("{label:<w$}", w = *w as usize) } else { format!("{label:>w$}", w = *w as usize - 1) + " " };
        header.push(Span::styled(text, st));
        hits.push((Rect { x, y: area.y, width: *w, height: 1 }, Hit::ListHeader(t.id, i)));
        cols.push((*k, *w));
        x += w;
    }
    f.render_widget(Paragraph::new(Line::from(header)), Rect { height: 1, ..area });
    if t.series.is_none() {
        f.render_widget(
            Paragraph::new(" pick a series: `,` / `.` or the sidebar").style(Style::new().fg(th.muted)),
            Rect { y: area.y + 1, height: 1, ..area },
        );
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
        let row = Rect { y: area.y + 1 + r as u16, height: 1, ..area };
        f.render_widget(Paragraph::new(Line::from(spans)), row);
        hits.push((row, Hit::ListRow(t.id, i)));
    }
}

pub fn draw_chart(
    f: &mut Frame,
    area: Rect,
    t: &Tile,
    cache: &mut ChartCache,
    data: Data,
    scope: Option<(i64, i64)>,
    bounds: Option<(i64, i64)>,
    th: &Theme,
    hits: &mut Vec<(Rect, Hit)>,
) {
    if area.height < 3 || area.width < 10 {
        return;
    }
    let l = &t.chart.layers;
    let head_h = if l.crosshair { 2 } else { 1 };
    let body = Rect { y: area.y + head_h, height: area.height - head_h, ..area };
    let plot_w = super::chart::areas(body, l).plot.width;
    cache.ensure(data, t, scope, plot_w, bounds);
    if cache.loading {
        f.render_widget(Paragraph::new(" loading series…").style(Style::new().fg(th.muted)), area);
        return;
    }
    if t.series.is_none() || cache.bars.is_empty() {
        f.render_widget(Paragraph::new(" no bars · pick a series with `,` / `.` or the sidebar").style(Style::new().fg(th.muted)), area);
        return;
    }
    // period selectors: start ◂ date ▸   end ◂ date ▸   zoom
    let ms = Style::new().fg(th.muted);
    let acc = Style::new().fg(th.accent);
    let mut x = area.x;
    let mut spans: Vec<Span> = vec![];
    let mut push = |text: String, st: Style, hit: Option<u8>, spans: &mut Vec<Span>, hits: &mut Vec<(Rect, Hit)>| {
        let w = text.chars().count() as u16;
        if let Some(h) = hit {
            hits.push((Rect { x, y: area.y, width: w, height: 1 }, Hit::ChartNav(t.id, h)));
        }
        spans.push(Span::styled(text, st));
        x += w;
    };
    push("start ".into(), ms, None, &mut spans, hits);
    push("◂".into(), acc, Some(0), &mut spans, hits);
    let w = 18u16;
    // "start ◂" is 7 cells, then the 18-cell date; "▸   end ◂" is 9 more
    hits.push((Rect { x: area.x + 7, y: area.y, width: w, height: 1 }, Hit::ChartDate(t.id, 0)));
    hits.push((Rect { x: area.x + 34, y: area.y, width: w, height: 1 }, Hit::ChartDate(t.id, 1)));
    push(format!(" {} ", &fmt_ts(cache.start.max(cache.data_first))[..16]), Style::new().fg(th.fg), None, &mut spans, hits);
    push("▸".into(), acc, Some(1), &mut spans, hits);
    push("   end ".into(), ms, None, &mut spans, hits);
    push("◂".into(), acc, Some(2), &mut spans, hits);
    push(format!(" {} ", &fmt_ts(cache.end)[..16]), Style::new().fg(th.fg), None, &mut spans, hits);
    push("▸".into(), acc, Some(3), &mut spans, hits);
    let per = crate::model::Duration(cache.bucket_ms / 1000).to_string();
    push(format!("   {per}/col{}", if t.chart.end.is_none() { " · live" } else { "" }), ms, None, &mut spans, hits);
    f.render_widget(Paragraph::new(Line::from(spans)), Rect { height: 1, ..area });
    let ci = cache.cursor_index();
    if l.crosshair
        && let Some(b) = ci.and_then(|i| cache.visible().get(i))
    {
        let mut text = super::chart::readout(b);
        if l.ma {
            text.push_str(&format!("  {}", t.chart.mas.iter().map(|m| m.label()).collect::<Vec<_>>().join(" ")));
        }
        if l.line {
            text.push_str(&format!("  line {}", t.chart.line_metric.label()));
        }
        f.render_widget(Paragraph::new(text).style(Style::new().fg(th.info)), Rect { y: area.y + 1, height: 1, ..area });
    }
    let plot = super::chart::areas(body, l).plot;
    hits.push((plot, Hit::ChartPlot(t.id)));
    let data = super::chart::ChartData {
        bars: &cache.bars,
        first: cache.first,
        bucket_ms: cache.bucket_ms,
        start: cache.aligned_start(),
        opts: &t.chart,
        cursor: ci,
        theme: th,
    };
    super::chart::render(&data, body, f.buffer_mut());
}

/// Draw one tile with its border.
#[allow(clippy::too_many_arguments)]
pub fn draw_tile(
    f: &mut Frame,
    area: Rect,
    t: &Tile,
    cache: &mut TileCache,
    data: Data,
    scope: Option<(i64, i64)>,
    bounds: Option<(i64, i64)>,
    focused: bool,
    th: &Theme,
    hits: &mut Vec<(Rect, Hit)>,
) {
    let scope = if t.view == View::List { t.list.scope(scope) } else { scope };
    if t.view == View::List {
        let inner = Block::bordered().inner(area);
        cache.list.ensure(data.clone(), t, scope, inner.height as i64 - 1);
    }
    let title = title(t, cache);
    // list range labels after the title, clickable
    let mut range_hits = vec![];
    let title = if t.view == View::List {
        let f = |v: Option<i64>| v.map(|x| fmt_ts(x)[..16].to_string()).unwrap_or_else(|| "start".into());
        let g = |v: Option<i64>| v.map(|x| fmt_ts(x)[..16].to_string()).unwrap_or_else(|| "end".into());
        let (a, b) = (format!("from {}", f(t.list.from)), format!("to {}", g(t.list.to)));
        let x0 = area.x + 2 + title.chars().count() as u16 + 3;
        range_hits.push((Rect { x: x0, y: area.y, width: a.chars().count() as u16, height: 1 }, Hit::ListRange(t.id, 0)));
        let x1 = x0 + a.chars().count() as u16 + 1;
        range_hits.push((Rect { x: x1, y: area.y, width: b.chars().count() as u16, height: 1 }, Hit::ListRange(t.id, 1)));
        format!("{title} · {a} {b}")
    } else {
        title
    };
    let block = Block::bordered()
        .title(Span::styled(format!(" {title} "), Style::new().fg(if focused { th.accent } else { th.header }).add_modifier(Modifier::BOLD)))
        .border_style(Style::new().fg(if focused { th.border_focus } else { th.border }))
        .style(Style::new().bg(th.bg).fg(th.fg));
    let inner = block.inner(area);
    f.render_widget(block, area);
    hits.push((area, Hit::Tile(t.id)));
    for (r, h) in range_hits {
        if r.right() < area.right() {
            hits.push((r, h));
        }
    }
    if t.series.is_some() && data.is_none() {
        cache.chart.loading = true;
        let p = cache.pending.clone().unwrap_or(super::data::Pending::Loading { done: 0, total: None });
        super::widgets::loading(f, inner, &p, th);
        return;
    }
    match t.view {
        View::List => draw_list(f, inner, t, &mut cache.list, data, scope, focused, th, hits),
        View::Chart => draw_chart(f, inner, t, &mut cache.chart, data, scope, bounds, th, hits),
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
            KeyCode::Char('[') => {
                app.open_picker(id, super::picker::Target::ListFrom);
                return true;
            }
            KeyCode::Char(']') => {
                app.open_picker(id, super::picker::Target::ListTo);
                return true;
            }
            KeyCode::Char('\\') => {
                app.list_set_range(id, None, None, true);
                app.list_set_range(id, None, None, false);
                return true;
            }
            _ => return false,
        }
        let c = &mut app.caches.entry(id).or_default().list;
        c.sel = c.sel.clamp(0, (c.total - 1).max(0));
        app.propagate_from_list(id);
        return true;
    }
    chart_key(app, k)
}

/// Keys for a focused chart tile.
pub fn chart_key(app: &mut App, k: KeyEvent) -> bool {
    use super::modal::{Modal, PromptKind};
    let id = app.layout.focus;
    match k.code {
        KeyCode::Char(c) if super::chart::LAYERS.iter().any(|(k, _)| *k == c) => {
            let on = app.layout.focused_mut().chart.layers.toggle(c).unwrap_or(false);
            let name = super::chart::LAYERS.iter().find(|(k, _)| *k == c).map(|(_, n)| *n).unwrap_or("");
            app.set_status(format!("{name} {}", if on { "on" } else { "off" }));
            app.layout_changed();
        }
        KeyCode::Char('N') => {
            let t = app.layout.focused_mut();
            t.chart.line_metric = t.chart.line_metric.next();
            t.chart.layers.line = true;
            let m = t.chart.line_metric.label();
            app.set_status(format!("line: {m}"));
            app.layout_changed();
        }
        KeyCode::Char('M') => app.modal = Some(Modal::prompt("Moving averages (e.g. sma20, ema50)", PromptKind::Mas)),
        KeyCode::Char('L') => app.dispatch(super::action::Action::ChartLayers),
        KeyCode::Char('[') => app.open_picker(id, super::picker::Target::ChartStart),
        KeyCode::Char(']') => app.open_picker(id, super::picker::Target::ChartEnd),
        KeyCode::Char('{') => app.modal = Some(Modal::prompt("Period start (YYYY-MM-DD [HH:MM])", PromptKind::ChartStart)),
        KeyCode::Char('}') => app.modal = Some(Modal::prompt("Period end (YYYY-MM-DD [HH:MM])", PromptKind::ChartEnd)),
        KeyCode::Char('@') => app.modal = Some(Modal::prompt("Jump to date (YYYY-MM-DD [HH:MM])", PromptKind::JumpDate)),
        KeyCode::Char('h') => app.chart_pan(id, -0.25),
        KeyCode::Char('l') => app.chart_pan(id, 0.25),
        KeyCode::Char('H') | KeyCode::PageUp => app.chart_pan(id, -1.0),
        KeyCode::PageDown => app.chart_pan(id, 1.0),
        KeyCode::Char('+') | KeyCode::Char('=') => app.chart_zoom(id, true),
        KeyCode::Char('-') => app.chart_zoom(id, false),
        KeyCode::Left => app.chart_cursor(id, -1),
        KeyCode::Right => app.chart_cursor(id, 1),
        KeyCode::Home => {
            let c = app.caches.entry(id).or_default().chart.clone();
            let end = c.data_first + (c.width.max(1) as i64 - 1) * c.bucket_ms;
            app.chart_set_end(id, Some(end));
        }
        KeyCode::End | KeyCode::Char('0') => app.chart_set_end(id, None),
        _ => return false,
    }
    true
}
