//! Navigator: series list -> paged bar table, candles and coverage.
use super::{block, widgets::Candles, App};
use crate::ingest::time::fmt_ts;
use crate::model::FLAG_ZERO_VOLUME;
use crate::store::{BarFilter, SeriesInfo, Store};
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{List, ListItem, ListState, Paragraph, Row, Table};
use ratatui::Frame;

pub const PAGE: i64 = 200;

#[derive(Default)]
pub struct NavState {
    pub series: Vec<SeriesInfo>,
    pub list: ListState,
    pub page: i64,
    pub bars: Vec<crate::model::StoredBar>,
    pub row: usize,
    pub focus_table: bool,
}

impl NavState {
    pub fn reload(&mut self, store: &Store, status: &mut String) {
        match store.series() {
            Ok(s) => {
                self.series = s;
                if self.list.selected().is_none() && !self.series.is_empty() {
                    self.list.select(Some(0));
                }
                self.load_page(store, status);
            }
            Err(e) => *status = format!("error: {e}"),
        }
    }

    pub fn current(&self) -> Option<&SeriesInfo> {
        self.list.selected().and_then(|i| self.series.get(i))
    }

    pub fn pages(&self) -> i64 {
        self.current().map(|s| (s.count + PAGE - 1) / PAGE).unwrap_or(0)
    }

    pub fn load_page(&mut self, store: &Store, status: &mut String) {
        let Some(s) = self.current().cloned() else {
            self.bars.clear();
            return;
        };
        let f = BarFilter {
            source: Some(s.source.clone()),
            symbol: Some(s.symbol.clone()),
            duration: Some(s.duration),
            limit: Some(PAGE),
            offset: Some(self.page * PAGE),
            ..Default::default()
        };
        match store.query_bars(&f) {
            Ok(b) => {
                self.bars = b;
                self.row = self.row.min(self.bars.len().saturating_sub(1));
            }
            Err(e) => *status = format!("error: {e}"),
        }
    }
}

pub fn on_key(app: &mut App, k: KeyEvent) {
    let n = &mut app.nav;
    match k.code {
        KeyCode::Char('r') => {
            app.nav.reload(&app.store, &mut app.status);
            app.status = format!("{} series", app.nav.series.len());
        }
        KeyCode::Enter | KeyCode::Right if !n.focus_table => n.focus_table = true,
        KeyCode::Esc | KeyCode::Left if n.focus_table => n.focus_table = false,
        KeyCode::Down | KeyCode::Char('j') => {
            if n.focus_table {
                n.row = (n.row + 1).min(n.bars.len().saturating_sub(1));
            } else if !n.series.is_empty() {
                let i = (n.list.selected().unwrap_or(0) + 1).min(n.series.len() - 1);
                n.list.select(Some(i));
                n.page = 0;
                n.row = 0;
                app.nav.load_page(&app.store, &mut app.status);
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if n.focus_table {
                n.row = n.row.saturating_sub(1);
            } else {
                let i = n.list.selected().unwrap_or(0).saturating_sub(1);
                n.list.select(Some(i));
                n.page = 0;
                n.row = 0;
                app.nav.load_page(&app.store, &mut app.status);
            }
        }
        KeyCode::PageDown => {
            if n.page + 1 < n.pages() {
                n.page += 1;
                n.row = 0;
                app.nav.load_page(&app.store, &mut app.status);
            }
        }
        KeyCode::PageUp => {
            if n.page > 0 {
                n.page -= 1;
                n.row = 0;
                app.nav.load_page(&app.store, &mut app.status);
            }
        }
        KeyCode::Home => {
            n.page = 0;
            n.row = 0;
            app.nav.load_page(&app.store, &mut app.status);
        }
        KeyCode::End => {
            n.page = (n.pages() - 1).max(0);
            n.row = 0;
            app.nav.load_page(&app.store, &mut app.status);
        }
        _ => {}
    }
}

pub fn draw(app: &mut App, f: &mut Frame, area: Rect) {
    let t = app.theme.clone();
    let [left, right] = Layout::horizontal([Constraint::Length(38), Constraint::Min(0)]).areas(area);
    let items: Vec<ListItem> = app
        .nav
        .series
        .iter()
        .map(|s| ListItem::new(format!("{:<10} {:<10} {:>4} {:>8}", s.source, s.symbol.display(), s.duration.to_string(), s.count)))
        .collect();
    let empty = app.nav.series.is_empty();
    let list = List::new(items)
        .block(block("Series (source symbol dur bars)", !app.nav.focus_table, &t))
        .highlight_style(Style::new().bg(t.selection_bg).fg(t.selection_fg).add_modifier(Modifier::BOLD));
    f.render_stateful_widget(list, left, &mut app.nav.list);
    if empty {
        f.render_widget(
            Paragraph::new("No bars yet. Press 2 (Ingest) to load a file.").style(Style::new().fg(t.muted)),
            super::centered(left, 34, 3),
        );
    }
    let [info, chart, table] = Layout::vertical([Constraint::Length(4), Constraint::Length(10), Constraint::Min(0)]).areas(right);
    let info_text = match app.nav.current() {
        Some(s) => vec![
            Line::from(format!("{} {} {} · {} bars", s.source, s.symbol, s.duration, s.count)),
            Line::from(format!(
                "coverage {} .. {} UTC · {} missing bars · page {}/{}",
                fmt_ts(s.first),
                fmt_ts(s.last),
                s.missing(),
                app.nav.page + 1,
                app.nav.pages().max(1)
            )),
        ],
        None => vec![Line::from("select a series")],
    };
    f.render_widget(Paragraph::new(info_text).block(block("Coverage", false, &t)), info);
    let bars: Vec<crate::model::Bar> = app.nav.bars.iter().map(|b| b.bar).collect();
    let cb = block("Candles (this page)", false, &t);
    let inner = cb.inner(chart);
    f.render_widget(cb, chart);
    f.render_widget(Candles { bars: &bars, theme: &t }, inner);

    let h = table.height.saturating_sub(3) as usize;
    let start = app.nav.row.saturating_sub(h.saturating_sub(1));
    let rows: Vec<Row> = app
        .nav
        .bars
        .iter()
        .enumerate()
        .skip(start)
        .take(h.max(1))
        .map(|(i, b)| {
            let up = b.bar.close >= b.bar.open;
            let mut st = Style::new().fg(if up { t.up } else { t.down });
            if app.nav.focus_table && i == app.nav.row {
                st = Style::new().bg(t.selection_bg).fg(t.selection_fg);
            }
            Row::new(vec![
                fmt_ts(b.bar.ts),
                format!("{:.2}", b.bar.open),
                format!("{:.2}", b.bar.high),
                format!("{:.2}", b.bar.low),
                format!("{:.2}", b.bar.close),
                format!("{:.4}{}", b.bar.volume, if b.bar.flags & FLAG_ZERO_VOLUME != 0 { " ∅" } else { "" }),
            ])
            .style(st)
        })
        .collect();
    let tbl = Table::new(
        rows,
        [Constraint::Length(20), Constraint::Length(12), Constraint::Length(12), Constraint::Length(12), Constraint::Length(12), Constraint::Min(10)],
    )
    .header(Row::new(vec!["time (UTC)", "open", "high", "low", "close", "volume"]).style(Style::new().fg(t.header).add_modifier(Modifier::BOLD)))
    .block(block("Bars (Enter: focus, PgUp/PgDn: page)", app.nav.focus_table, &t));
    f.render_widget(tbl, table);
}
