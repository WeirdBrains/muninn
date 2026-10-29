//! The history bar: a collapsible left pane with the action tree (forks indented),
//! and the TUI side of recording view changes. See `docs/history.md`.
use super::{App, Hit};
use crate::history::{ViewState, view_action};
use crate::store::{Move, TreeRow};
use crate::theme::Theme;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

pub const WIDTH: u16 = 34;

#[derive(Debug, Default)]
pub struct HistoryPane {
    /// Row 0 is the start (before any action); row i+1 is `rows[i]`.
    pub rows: Vec<TreeRow>,
    pub sel: usize,
    pub focused: bool,
    pub offset: usize,
    stamp: Option<(i64, i64, Option<i64>)>,
}

impl HistoryPane {
    /// Action id of a pane row (None: the start row).
    pub fn id_at(&self, row: usize) -> Option<i64> {
        row.checked_sub(1).and_then(|i| self.rows.get(i)).map(|r| r.action.id)
    }
    pub fn len(&self) -> usize {
        self.rows.len() + 1
    }
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
    fn head_row(&self) -> usize {
        self.rows.iter().position(|r| r.head).map(|i| i + 1).unwrap_or(0)
    }
}

impl App {
    /// Reload the tree if the history changed (another thread, the CLI, or us).
    pub fn refresh_history(&mut self, force: bool) {
        let Ok(stamp) = self.store.history_stamp() else { return };
        if !force && self.history.stamp == Some(stamp) {
            return;
        }
        let moved = self.history.stamp.map(|s| s.2) != Some(stamp.2);
        self.history.stamp = Some(stamp);
        self.history.rows = self.store.history_tree().unwrap_or_default();
        if moved || self.history.sel >= self.history.len() {
            self.history.sel = self.history.head_row();
        }
    }

    /// Record a change to filters, time range, theme or mode (no-op when nothing changed,
    /// e.g. while replaying a history step).
    pub fn note_view(&mut self) {
        let after = ViewState::capture(&self.prefs);
        if after == self.view_base {
            return;
        }
        let before = std::mem::replace(&mut self.view_base, after.clone());
        let (kind, summary) = describe(&before, &after);
        self.store.note_action(&view_action(kind, "tui", summary, &before, &after));
        self.refresh_history(true);
    }

    /// Apply the state a history move returned: filters, theme and mode, without recording.
    pub fn apply_move(&mut self, r: crate::Result<Move>) {
        let m = match r {
            Ok(m) => m,
            Err(e) => return self.set_status(e.to_string()),
        };
        if let Some(v) = m.state.as_ref().and_then(ViewState::from_value) {
            self.apply_view(&v);
        }
        self.refresh_history(true);
        let at = match m.to.and_then(|id| self.store.action(id).ok().flatten()) {
            Some(a) => format!("history: at #{} {}", a.id, a.summary),
            None => "history: at the start".into(),
        };
        self.set_status(at);
    }

    fn apply_view(&mut self, v: &ViewState) {
        let theme_changed = self.prefs.theme.name != v.theme;
        let mode_changed = self.prefs.theme.mode != v.mode;
        let filters_changed = self.prefs.tui.sidebar.filters != v.filters;
        v.apply(&mut self.prefs);
        self.view_base = v.clone();
        if theme_changed || mode_changed {
            let variant = crate::theme::resolve_variant(v.mode);
            let file = if theme_changed {
                crate::theme::load(&self.paths, &v.theme).unwrap_or_else(|_| self.theme_file.clone())
            } else {
                self.theme_file.clone()
            };
            self.apply_theme(file, variant);
            self.themes = super::themes::ThemeState::new(&self.paths, &v.theme);
        }
        if filters_changed {
            self.caches.clear();
            self.refresh_rows();
        }
        self.save_prefs();
    }

    /// Go to the action on pane row `row` (0: the start).
    pub fn history_goto_row(&mut self, row: usize) {
        let id = self.history.id_at(row);
        let r = self.store.history_goto(id);
        self.apply_move(r);
    }
}

/// Kind and summary for a view change.
fn describe(b: &ViewState, a: &ViewState) -> (&'static str, String) {
    if b.theme != a.theme {
        return ("theme", format!("theme {}", a.theme));
    }
    if b.mode != a.mode {
        return ("theme.mode", format!("theme mode {}", a.mode));
    }
    let (fb, fa) = (&b.filters, &a.filters);
    if fa.is_empty() && !fb.is_empty() {
        return ("filter.clear", "clear filters".into());
    }
    if (fb.from, fb.to) != (fa.from, fa.to) {
        let f = |v: Option<i64>| v.map(|x| crate::ingest::time::fmt_ts(x)[..16].to_string()).unwrap_or_else(|| "…".into());
        if fa.from.is_none() && fa.to.is_none() {
            return ("filter.time", "clear time range".into());
        }
        return ("filter.time", format!("time range {} → {}", f(fa.from), f(fa.to)));
    }
    ("filter", format!("filters: {}", a.filters_label()))
}

/// Keys while the history bar has focus. Returns true when handled.
pub fn on_key(app: &mut App, k: KeyEvent) -> bool {
    let n = app.history.len();
    let h = &mut app.history;
    match k.code {
        KeyCode::Down | KeyCode::Char('j') => h.sel = (h.sel + 1).min(n - 1),
        KeyCode::Up | KeyCode::Char('k') => h.sel = h.sel.saturating_sub(1),
        KeyCode::PageDown => h.sel = (h.sel + 10).min(n - 1),
        KeyCode::PageUp => h.sel = h.sel.saturating_sub(10),
        KeyCode::Home | KeyCode::Char('g') => h.sel = 0,
        KeyCode::End | KeyCode::Char('G') => h.sel = n - 1,
        KeyCode::Enter => {
            let row = h.sel;
            app.history_goto_row(row);
        }
        KeyCode::Esc | KeyCode::Tab | KeyCode::Right => h.focused = false,
        _ => return false,
    }
    true
}

fn age(ms: i64) -> String {
    let d = (crate::store::now_ms() - ms).max(0) / 1000;
    match d {
        0..60 => format!("{d}s"),
        60..3600 => format!("{}m", d / 60),
        3600..86_400 => format!("{}h", d / 3600),
        _ => format!("{}d", d / 86_400),
    }
}

fn trunc(s: &str, w: usize) -> String {
    if s.chars().count() <= w { s.to_string() } else { s.chars().take(w.saturating_sub(1)).chain(std::iter::once('…')).collect() }
}

pub fn draw(app: &mut App, f: &mut Frame, area: Rect, t: &Theme, hits: &mut Vec<(Rect, Hit)>) {
    app.refresh_history(false);
    let h = &mut app.history;
    let block = Block::new()
        .borders(Borders::RIGHT)
        .border_style(Style::new().fg(if h.focused { t.border_focus } else { t.border }))
        .style(Style::new().bg(t.bg).fg(t.fg));
    let inner = block.inner(area);
    f.render_widget(block, area);
    hits.push((area, Hit::HistoryBar));
    if inner.height < 2 {
        return;
    }
    let w = inner.width as usize;
    let title = format!(" History · {} ", h.rows.len());
    let close = " « ";
    let head = Line::from(vec![
        Span::styled(format!("{title:<tw$}", tw = w.saturating_sub(3)), Style::new().fg(t.header).add_modifier(Modifier::BOLD)),
        Span::styled(close, Style::new().fg(t.accent)),
    ]);
    f.render_widget(Paragraph::new(head), Rect { height: 1, ..inner });
    hits.push((Rect { x: inner.right().saturating_sub(3), y: inner.y, width: 3, height: 1 }, Hit::HistoryCollapse));
    let list = Rect { y: inner.y + 1, height: inner.height - 2, ..inner };
    let vis = list.height as usize;
    if h.sel < h.offset {
        h.offset = h.sel;
    } else if h.sel >= h.offset + vis {
        h.offset = h.sel + 1 - vis;
    }
    let at_start = !h.rows.iter().any(|r| r.head);
    for (i, row) in (h.offset..h.len()).take(vis).enumerate() {
        let y = list.y + i as u16;
        let sel = row == h.sel;
        let (text, base) = if row == 0 {
            (format!("{} start", if at_start { "●" } else { "◇" }), Style::new().fg(t.muted))
        } else {
            let r = &h.rows[row - 1];
            let mark = if r.head { "●" } else { "○" };
            let fork = if r.fork { "⑂" } else { "" };
            let indent = "  ".repeat(r.depth.min(6));
            let a = age(r.action.created_at);
            let left = format!("{indent}{mark}{fork} #{} {}", r.action.id, r.action.summary);
            let room = w.saturating_sub(a.len() + 2);
            let st = if r.current { Style::new().fg(t.fg) } else { Style::new().fg(t.muted) };
            (format!("{:<room$} {a}", trunc(&left, room)), if r.head { st.add_modifier(Modifier::BOLD) } else { st })
        };
        let st = if sel && h.focused {
            Style::new().bg(t.selection_bg).fg(t.selection_fg).add_modifier(Modifier::BOLD)
        } else if sel {
            base.bg(t.selection_bg)
        } else {
            base
        };
        let rect = Rect { x: list.x, y, width: list.width, height: 1 };
        f.render_widget(Paragraph::new(Line::styled(format!(" {}", trunc(&text, w.saturating_sub(1))), st)), rect);
        hits.push((rect, Hit::HistoryRow(row)));
    }
    let foot = Rect { y: inner.bottom() - 1, height: 1, ..inner };
    let mut ex = vec![];
    super::widgets::key_bar(f, foot, &[("u back", KeyCode::Char('u')), ("U fwd", KeyCode::Char('U'))], t, &mut ex);
    hits.extend(ex);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prefs::Preferences;

    #[test]
    fn describes_changes() {
        let p = Preferences::default();
        let a = ViewState::capture(&p);
        let mut b = a.clone();
        b.theme = "nord".into();
        assert_eq!(describe(&a, &b), ("theme", "theme nord".to_string()));
        let mut c = a.clone();
        c.filters.from = Some(0);
        assert_eq!(describe(&a, &c).0, "filter.time");
        assert_eq!(describe(&c, &a), ("filter.clear", "clear filters".to_string()));
        let mut d = a.clone();
        d.filters.sources.insert("binance".into());
        assert_eq!(describe(&a, &d), ("filter", "filters: src:binance".to_string()));
    }
}
