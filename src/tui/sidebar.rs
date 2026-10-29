//! Right sidebar: series list with fuzzy search, sort by year/name, and filters.
use super::Hit;
use super::layout::SeriesKey;
use crate::model::PriceMetric;
use crate::store::SeriesInfo;
use crate::theme::Theme;
use chrono::{Datelike, TimeZone, Utc};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};

pub const WIDTH: u16 = 34;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SortBy {
    #[default]
    Year,
    Name,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PriceFilter {
    pub metric: PriceMetric,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
}

impl PriceFilter {
    pub fn active(&self) -> bool {
        self.min.is_some() || self.max.is_some()
    }
    /// A series matches when its metric range overlaps [min, max].
    pub fn matches(&self, range: Option<(f64, f64)>) -> bool {
        if !self.active() {
            return true;
        }
        let Some((lo, hi)) = range else { return false };
        self.min.is_none_or(|m| hi >= m) && self.max.is_none_or(|m| lo <= m)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Filters {
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub sources: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub symbols: BTreeSet<String>,
    /// Time range (UTC ms) for every view: tiles and the sidebar show only bars inside it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<i64>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub durations: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub years: BTreeSet<i32>,
    #[serde(default)]
    pub price: PriceFilter,
}

impl Filters {
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
            && self.symbols.is_empty()
            && self.from.is_none()
            && self.to.is_none()
            && self.durations.is_empty()
            && self.years.is_empty()
            && !self.price.active()
    }
    /// Time scope: the year filter (Jan 1 of the first year .. end of the last year)
    /// intersected with the time range.
    pub fn scope(&self) -> Option<(i64, i64)> {
        let years = self.year_scope();
        if self.from.is_none() && self.to.is_none() {
            return years;
        }
        let (a, b) = years.unwrap_or((i64::MIN, i64::MAX));
        Some((a.max(self.from.unwrap_or(i64::MIN)), b.min(self.to.unwrap_or(i64::MAX))))
    }
    fn year_scope(&self) -> Option<(i64, i64)> {
        let (a, b) = (*self.years.first()?, *self.years.last()?);
        let from = Utc.with_ymd_and_hms(a, 1, 1, 0, 0, 0).single()?.timestamp_millis();
        let to = Utc.with_ymd_and_hms(b + 1, 1, 1, 0, 0, 0).single()?.timestamp_millis() - 1;
        Some((from, to))
    }
    /// Active filters as removable chips.
    pub fn chips(&self) -> Vec<(Chip, String)> {
        let mut v = vec![];
        for s in &self.sources {
            v.push((Chip::Source(s.clone()), format!("src:{s}")));
        }
        for s in &self.symbols {
            v.push((Chip::Symbol(s.clone()), format!("sym:{s}")));
        }
        let ts = |ms: i64| crate::ingest::time::fmt_ts(ms)[..16].to_string();
        if let Some(a) = self.from {
            v.push((Chip::From, format!("from {}", ts(a))));
        }
        if let Some(b) = self.to {
            v.push((Chip::To, format!("to {}", ts(b))));
        }
        for d in &self.durations {
            v.push((Chip::Duration(d.clone()), format!("per:{d}")));
        }
        for y in &self.years {
            v.push((Chip::Year(*y), format!("{y}")));
        }
        if self.price.active() {
            let f = |o: Option<f64>| o.map(super::tiles::fmt_num).unwrap_or_else(|| "…".into());
            v.push((Chip::Price, format!("{} {}–{}", self.price.metric.label(), f(self.price.min), f(self.price.max))));
        }
        v
    }
    pub fn remove(&mut self, c: &Chip) {
        match c {
            Chip::Source(s) => {
                self.sources.remove(s);
            }
            Chip::Duration(d) => {
                self.durations.remove(d);
            }
            Chip::Symbol(s) => {
                self.symbols.remove(s);
            }
            Chip::From => self.from = None,
            Chip::To => self.to = None,
            Chip::Year(y) => {
                self.years.remove(y);
            }
            Chip::Price => {
                self.price.min = None;
                self.price.max = None;
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chip {
    Source(String),
    Symbol(String),
    From,
    To,
    Duration(String),
    Year(i32),
    Price,
}

/// Sidebar settings remembered in preferences.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SidebarPrefs {
    #[serde(default)]
    pub sort: SortBy,
    #[serde(default)]
    pub desc: bool,
    #[serde(default)]
    pub filters: Filters,
}

/// Sidebar summary of the time range: `any`, `2024-01-01 →`, `→ 2024-06-30` or both.
fn time_summary(f: &Filters) -> String {
    let d = |ms: i64| crate::ingest::time::fmt_ts(ms)[..10].to_string();
    match (f.from, f.to) {
        (None, None) => "any".into(),
        (Some(a), None) => format!("{} →", d(a)),
        (None, Some(b)) => format!("→ {}", d(b)),
        (Some(a), Some(b)) => format!("{}→{}", d(a), d(b)),
    }
}

pub fn year_of(ms: i64) -> i32 {
    Utc.timestamp_millis_opt(ms).single().map(|d| d.year()).unwrap_or(1970)
}

/// Fuzzy match score: substring beats subsequence; None = no match.
pub fn fuzzy(query: &str, text: &str) -> Option<i64> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Some(0);
    }
    let t = text.to_lowercase();
    // Several words (`binance btc`): every word must match, in any order.
    let words: Vec<&str> = q.split_whitespace().collect();
    if words.len() > 1 && t.find(&q).is_none() {
        let compact: String = t.chars().filter(|c| c.is_alphanumeric()).collect();
        let sep = |w: &str| w.chars().filter(|c| c.is_alphanumeric()).collect::<String>();
        if words.iter().all(|w| t.contains(w) || compact.contains(&sep(w))) {
            return Some(700);
        }
    }
    // Separators don't matter for symbols: btc-usdt, btc_usdt, btcusdt all find BTC/USDT.
    let qc: String = q.chars().filter(|c| c.is_alphanumeric()).collect();
    if let Some(p) = t.find(&q) {
        return Some(1000 - p as i64);
    }
    let compact: String = t.chars().filter(|c| c.is_alphanumeric()).collect();
    if let Some(p) = compact.find(&qc) {
        return Some(800 - p as i64);
    }
    let mut it = t.chars();
    let mut gaps = 0i64;
    for c in qc.chars() {
        let mut skipped = 0;
        loop {
            match it.next() {
                Some(x) if x == c => break,
                Some(_) => skipped += 1,
                None => return None,
            }
        }
        gaps += skipped;
    }
    Some(500 - gaps)
}

pub fn search_text(s: &SeriesInfo) -> String {
    format!("{} {} {}", s.symbol.display(), s.source, s.duration)
}

/// Indices of `series` to show, after search, filters and sort.
pub fn rows(series: &[SeriesInfo], query: &str, p: &SidebarPrefs, price_range: &dyn Fn(&SeriesInfo) -> Option<(f64, f64)>) -> Vec<usize> {
    let f = &p.filters;
    let mut v: Vec<(usize, i64)> = series
        .iter()
        .enumerate()
        .filter(|(_, s)| f.sources.is_empty() || f.sources.contains(&s.source))
        .filter(|(_, s)| f.symbols.is_empty() || f.symbols.contains(&s.symbol.display()))
        .filter(|(_, s)| s.last >= f.from.unwrap_or(i64::MIN) && s.first <= f.to.unwrap_or(i64::MAX))
        .filter(|(_, s)| f.durations.is_empty() || f.durations.contains(&s.duration.to_string()))
        .filter(|(_, s)| {
            let (a, b) = (year_of(s.first), year_of(s.last));
            f.years.is_empty() || f.years.iter().any(|y| (a..=b).contains(y))
        })
        .filter(|(_, s)| f.price.matches(if f.price.active() { price_range(s) } else { None }))
        .filter_map(|(i, s)| fuzzy(query, &search_text(s)).map(|sc| (i, sc)))
        .collect();
    let name = |i: usize| (series[i].symbol.display(), series[i].source.clone(), series[i].duration.0);
    v.sort_by(|a, b| {
        let ord = match p.sort {
            SortBy::Year => (year_of(series[a.0].first), year_of(series[a.0].last))
                .cmp(&(year_of(series[b.0].first), year_of(series[b.0].last)))
                .then_with(|| name(a.0).cmp(&name(b.0))),
            SortBy::Name => name(a.0).cmp(&name(b.0)),
        };
        if p.desc { ord.reverse() } else { ord }
    });
    // With a query, better matches first (stable within equal scores).
    if !query.trim().is_empty() {
        v.sort_by_key(|x| std::cmp::Reverse(x.1));
    }
    v.into_iter().map(|(i, _)| i).collect()
}

/// Cache key for a series' metric range within a scope.
pub type PriceKey = (SeriesKey, PriceMetric, Option<(i64, i64)>);

/// Runtime sidebar state.
#[derive(Debug, Clone, Default)]
pub struct Sidebar {
    pub query: String,
    pub searching: bool,
    pub focused: bool,
    pub sel: usize,
    pub rows: Vec<usize>,
    pub price_cache: HashMap<PriceKey, Option<(f64, f64)>>,
    pub top: usize,
}

#[allow(clippy::too_many_arguments)]
pub fn draw(
    sb: &mut Sidebar,
    p: &SidebarPrefs,
    series: &[SeriesInfo],
    current: Option<&SeriesKey>,
    f: &mut Frame,
    area: Rect,
    t: &Theme,
    hits: &mut Vec<(Rect, Hit)>,
) {
    let block = Block::new()
        .borders(Borders::LEFT)
        .border_style(Style::new().fg(if sb.focused { t.border_focus } else { t.border }))
        .style(Style::new().bg(t.bg).fg(t.fg));
    let inner = block.inner(area);
    f.render_widget(block, area);
    hits.push((area, Hit::Sidebar));
    let w = inner.width as usize;
    let mut y = inner.y;
    let line = |f: &mut Frame, y: u16, l: Line| f.render_widget(Paragraph::new(l), Rect { x: inner.x, y, width: inner.width, height: 1 });
    // search
    let sst = if sb.searching { Style::new().fg(t.fg).bg(t.selection_bg) } else { Style::new().fg(t.muted) };
    let q = if sb.query.is_empty() && !sb.searching { "search".to_string() } else { sb.query.clone() };
    line(f, y, Line::from(vec![Span::styled(" / ", Style::new().fg(t.accent)), Span::styled(format!("{q:<w$}", w = w.saturating_sub(3)), sst)]));
    hits.push((Rect { x: inner.x, y, width: inner.width, height: 1 }, Hit::SidebarSearch));
    y += 1;
    // sort buttons
    let key = match p.sort {
        SortBy::Year => "year",
        SortBy::Name => "name",
    };
    let arrow = if p.desc { "▼ desc" } else { "▲ asc" };
    let bst = Style::new().fg(t.bg).bg(t.accent);
    let x0 = inner.x + 7;
    line(
        f,
        y,
        Line::from(vec![
            Span::styled(" sort: ", Style::new().fg(t.muted)),
            Span::styled(format!("[{key}]"), bst),
            Span::raw(" "),
            Span::styled(format!("[{arrow}]"), bst),
        ]),
    );
    hits.push((Rect { x: x0, y, width: key.len() as u16 + 2, height: 1 }, Hit::SidebarSortKey));
    hits.push((Rect { x: x0 + key.len() as u16 + 3, y, width: arrow.chars().count() as u16 + 2, height: 1 }, Hit::SidebarSortOrder));
    y += 1;
    // filter rows
    let fl = &p.filters;
    let summary = |n: usize, what: &str| if n == 0 { "any".to_string() } else { format!("{n} {what}") };
    let rows_f = [
        ("source", summary(fl.sources.len(), "selected"), super::action::FilterKind::Source),
        ("symbol", summary(fl.symbols.len(), "selected"), super::action::FilterKind::Symbol),
        ("time", time_summary(fl), super::action::FilterKind::Time),
        ("price", if fl.price.active() { fl.price.metric.label().to_string() } else { "any".into() }, super::action::FilterKind::Price),
        ("period", summary(fl.durations.len(), "selected"), super::action::FilterKind::Period),
        ("year", summary(fl.years.len(), "selected"), super::action::FilterKind::Year),
    ];
    for (name, val, kind) in rows_f {
        if y >= inner.bottom() {
            break;
        }
        let st = if val == "any" { Style::new().fg(t.muted) } else { Style::new().fg(t.accent) };
        line(f, y, Line::from(vec![Span::styled(format!(" {name:<7}"), Style::new().fg(t.header)), Span::styled(format!("[{val}]"), st)]));
        hits.push((Rect { x: inner.x, y, width: inner.width, height: 1 }, Hit::SidebarFilter(kind)));
        y += 1;
    }
    // chips
    let chips = fl.chips();
    if !chips.is_empty() && y < inner.bottom() {
        let mut x = inner.x + 1;
        let mut spans = vec![Span::raw(" ")];
        for (i, (_, label)) in chips.iter().enumerate() {
            let text = format!("{label} ✕");
            let cw = text.chars().count() as u16;
            if x + cw + 1 > inner.right() {
                line(f, y, Line::from(std::mem::take(&mut spans)));
                y += 1;
                x = inner.x + 1;
                spans.push(Span::raw(" "));
                if y >= inner.bottom() {
                    break;
                }
            }
            spans.push(Span::styled(text, Style::new().fg(t.bg).bg(t.info)));
            spans.push(Span::raw(" "));
            hits.push((Rect { x, y, width: cw, height: 1 }, Hit::Chip(i)));
            x += cw + 1;
        }
        if y < inner.bottom() {
            line(f, y, Line::from(spans));
            y += 1;
        }
        if y < inner.bottom() {
            line(f, y, Line::styled(" X clears all", Style::new().fg(t.muted)));
            hits.push((Rect { x: inner.x, y, width: inner.width, height: 1 }, Hit::ClearFilters));
            y += 1;
        }
    }
    if y < inner.bottom() {
        line(f, y, Line::styled("─".repeat(w), Style::new().fg(t.border)));
        y += 1;
    }
    // series
    let h = inner.bottom().saturating_sub(y) as usize;
    if sb.rows.is_empty() && y < inner.bottom() {
        let msg = if series.is_empty() { " no data: File ▸ Import…" } else { " no series match" };
        line(f, y, Line::styled(msg, Style::new().fg(t.muted)));
        return;
    }
    sb.sel = sb.sel.min(sb.rows.len().saturating_sub(1));
    if sb.sel < sb.top {
        sb.top = sb.sel;
    }
    if h > 0 && sb.sel >= sb.top + h {
        sb.top = sb.sel + 1 - h;
    }
    for (r, &i) in sb.rows.iter().enumerate().skip(sb.top).take(h) {
        let s = &series[i];
        let k = SeriesKey::of(s);
        let mark = if Some(&k) == current { "●" } else { " " };
        let years = format!("{}–{}", year_of(s.first) % 100, year_of(s.last) % 100);
        let text = format!("{mark}{:<10} {:<9} {:>4} {years}", s.symbol.display(), trunc(&s.source, 9), s.duration.to_string());
        let mut st = Style::new().fg(t.fg);
        if r == sb.sel {
            st = if sb.focused {
                Style::new().fg(t.selection_fg).bg(t.selection_bg).add_modifier(Modifier::BOLD)
            } else {
                st.add_modifier(Modifier::REVERSED)
            };
        }
        line(f, y, Line::styled(format!("{text:<w$}"), st));
        hits.push((Rect { x: inner.x, y, width: inner.width, height: 1 }, Hit::SidebarRow(r)));
        y += 1;
    }
}

fn trunc(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_string() } else { s.chars().take(n - 1).collect::<String>() + "…" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Duration, Symbol};

    fn si(base: &str, src: &str, dur: i64, y0: i32, y1: i32) -> SeriesInfo {
        let ms = |y: i32| Utc.with_ymd_and_hms(y, 6, 1, 0, 0, 0).unwrap().timestamp_millis();
        SeriesInfo { source: src.into(), symbol: Symbol::new(base, "USDT"), duration: Duration(dur), count: 10, first: ms(y0), last: ms(y1) }
    }

    fn data() -> Vec<SeriesInfo> {
        vec![
            si("BTC", "binance", 300, 2017, 2026),
            si("ETH", "binance", 300, 2017, 2025),
            si("SOL", "kraken", 3600, 2020, 2024),
            si("ADA", "kraken", 300, 2018, 2026),
        ]
    }

    fn names(v: &[SeriesInfo], r: &[usize]) -> Vec<String> {
        r.iter().map(|i| v[*i].symbol.base.clone()).collect()
    }

    #[test]
    fn sort_by_year_then_last_year_with_toggle() {
        let d = data();
        let mut p = SidebarPrefs::default();
        let none = |_: &SeriesInfo| None;
        assert_eq!(names(&d, &rows(&d, "", &p, &none)), ["ETH", "BTC", "ADA", "SOL"]);
        p.desc = true;
        assert_eq!(names(&d, &rows(&d, "", &p, &none)), ["SOL", "ADA", "BTC", "ETH"]);
        p.sort = SortBy::Name;
        p.desc = false;
        assert_eq!(names(&d, &rows(&d, "", &p, &none)), ["ADA", "BTC", "ETH", "SOL"]);
    }

    #[test]
    fn fuzzy_search_as_you_type() {
        let d = data();
        let p = SidebarPrefs::default();
        let none = |_: &SeriesInfo| None;
        assert_eq!(names(&d, &rows(&d, "eth", &p, &none)), ["ETH"]);
        assert_eq!(names(&d, &rows(&d, "krak", &p, &none)).len(), 2);
        assert_eq!(names(&d, &rows(&d, "btcusdt", &p, &none)), ["BTC"]);
        assert_eq!(names(&d, &rows(&d, "sl1h", &p, &none)), ["SOL"]);
        assert!(rows(&d, "zzz", &p, &none).is_empty());
        assert!(fuzzy("bu", "BTC/USDT binance").unwrap() > fuzzy("bu", "xbxxxxu").unwrap());
    }

    #[test]
    fn filters_combine_and_chips_clear() {
        let d = data();
        let mut p = SidebarPrefs::default();
        let none = |_: &SeriesInfo| None;
        p.filters.sources.insert("kraken".into());
        assert_eq!(names(&d, &rows(&d, "", &p, &none)), ["ADA", "SOL"]);
        p.filters.durations.insert("5m".into());
        assert_eq!(names(&d, &rows(&d, "", &p, &none)), ["ADA"]);
        p.filters.durations.clear();
        p.filters.years.insert(2025);
        assert_eq!(names(&d, &rows(&d, "", &p, &none)), ["ADA"]);
        let (a, b) = p.filters.scope().unwrap();
        assert_eq!(crate::ingest::time::fmt_ts(a), "2025-01-01 00:00:00");
        assert_eq!(crate::ingest::time::fmt_ts(b), "2025-12-31 23:59:59");
        p.filters.price = PriceFilter { metric: PriceMetric::Close, min: Some(100.0), max: Some(200.0) };
        let range = |s: &SeriesInfo| if s.symbol.base == "ADA" { Some((0.2, 3.0)) } else { Some((150.0, 300.0)) };
        assert!(rows(&d, "", &p, &range).is_empty());
        p.filters.sources.clear();
        p.filters.years.clear();
        assert_eq!(names(&d, &rows(&d, "", &p, &range)), ["ETH", "BTC", "SOL"]);
        let chips = p.filters.chips();
        assert_eq!(chips.len(), 1);
        p.filters.remove(&chips[0].0);
        assert!(p.filters.is_empty());
    }
}
