//! Pipes… (L): pipes running now or lately, from this TUI or anywhere else on the same home
//! (`cat data.csv | mnnn pipe - …`): each hop with its status, rows, time and error, live.
//! Fed by the history (`pipe.start`, `pipe.hop`, `pipe.run`), polled on every tick.
use super::workspace::{View, draw_view, muted, row_style};
use super::{App, Hit};
use crate::ingest::time::fmt_ts;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

/// Pipes kept in the window.
pub const KEEP: usize = 20;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Hop {
    pub name: String,
    /// `waiting`, `ok` or `error`.
    pub status: String,
    pub rows: u64,
    pub ms: u64,
    pub columns: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LivePipe {
    pub token: String,
    pub origin: String,
    pub hops: Vec<Hop>,
    /// `running`, `done` or `error`.
    pub status: String,
    pub saved: Vec<String>,
    pub updated: i64,
}

#[derive(Debug, Default)]
pub struct PipesState {
    /// The last history id read (`None`: start from now).
    pub seen: Option<i64>,
    pub list: Vec<LivePipe>,
    pub sel: usize,
}

/// Read new pipe actions (any origin) into the list; a status line when one finishes.
pub fn poll(app: &mut App) {
    let Some(seen) = app.pipes.seen else {
        app.pipes.seen = Some(app.store.history_stamp().map(|s| s.1).unwrap_or(0));
        return;
    };
    let mut acts = vec![];
    for k in ["pipe.start", "pipe.hop", "pipe.run"] {
        acts.extend(app.store.actions_of_kind_after(k, seen).unwrap_or_default());
    }
    if acts.is_empty() {
        return;
    }
    acts.sort_by_key(|a| a.id);
    app.pipes.seen = acts.last().map(|a| a.id);
    let mut done = vec![];
    for a in acts {
        let token = a.params["token"].as_str().unwrap_or_default().to_string();
        let pos = match app.pipes.list.iter().position(|p| p.token == token) {
            Some(i) => i,
            None => {
                app.pipes.list.insert(0, LivePipe { token: token.clone(), origin: a.origin.clone(), status: "running".into(), ..Default::default() });
                app.pipes.list.truncate(KEEP);
                0
            }
        };
        let p = &mut app.pipes.list[pos];
        p.updated = a.created_at;
        let strs =
            |k: &str| a.params[k].as_array().map(|v| v.iter().filter_map(|x| x.as_str().map(str::to_string)).collect::<Vec<_>>()).unwrap_or_default();
        match a.kind.as_str() {
            "pipe.start" => {
                p.hops = strs("plan").into_iter().map(|n| Hop { name: n, status: "waiting".into(), ..Default::default() }).collect();
            }
            "pipe.hop" => {
                let i = a.params["i"].as_u64().unwrap_or(0) as usize;
                while p.hops.len() <= i {
                    p.hops.push(Hop { status: "waiting".into(), ..Default::default() });
                }
                let h = &mut p.hops[i];
                if h.name.is_empty() || !a.params["hop"].as_str().unwrap_or_default().is_empty() {
                    h.name = a.params["hop"].as_str().unwrap_or(&h.name).to_string();
                }
                h.status = a.params["status"].as_str().unwrap_or("ok").to_string();
                h.rows = a.params["rows"].as_u64().unwrap_or(0);
                h.ms = a.params["ms"].as_u64().unwrap_or(0);
                h.columns = strs("columns");
                h.error = a.params["error"].as_str().map(str::to_string);
                if h.status == "error" {
                    p.status = "error".into();
                    done.push(format!("pipe failed at {}: {}", h.name, h.error.clone().unwrap_or_default()));
                }
            }
            _ => {
                p.status = "done".into();
                p.saved = strs("saved");
                for h in p.hops.iter_mut().filter(|h| h.status == "waiting") {
                    h.status = "ok".into();
                }
                done.push(format!("pipe done: {}", a.summary.trim_start_matches("pipe ")));
            }
        }
    }
    if let Some(m) = done.pop() {
        app.set_status(m);
    }
}

pub fn open(app: &mut App) {
    poll(app);
    app.modal = Some(super::modal::Modal::Pipes);
}

pub fn on_key(app: &mut App, k: KeyEvent) {
    let n = app.pipes.list.len();
    match k.code {
        KeyCode::Down | KeyCode::Char('j') => app.pipes.sel = (app.pipes.sel + 1).min(n.saturating_sub(1)),
        KeyCode::Up | KeyCode::Char('k') => app.pipes.sel = app.pipes.sel.saturating_sub(1),
        KeyCode::Esc | KeyCode::Char('q') => app.modal = None,
        _ => {}
    }
}

pub fn draw(app: &mut App, f: &mut Frame, r: Rect, hits: &mut Vec<(Rect, Hit)>) {
    let t = app.theme.clone();
    let st = &app.pipes;
    let mut v = View::new("Pipes · live, from here and the CLI (mnnn pipe)");
    if st.list.is_empty() {
        v.head.push(muted(&t, " no pipes yet: `mnnn pipe SOURCE STEP…`, share data to a code space, or > in a code space"));
    }
    for (i, p) in st.list.iter().enumerate() {
        let colour = match p.status.as_str() {
            "error" => t.error,
            "done" => t.accent,
            _ => t.warning,
        };
        let chain = p.hops.iter().map(|h| h.name.clone()).collect::<Vec<_>>().join(" → ");
        v.rows.push(Line::from(vec![
            Span::styled(format!(" {:<8}", p.status), Style::new().fg(colour)),
            Span::styled(format!("{chain} "), row_style(&t, i == st.sel)),
            Span::styled(format!(" {} · {}", p.origin, fmt_ts(p.updated)), Style::new().fg(t.muted)),
        ]));
    }
    // the selected pipe's hops
    if let Some(p) = st.list.get(st.sel) {
        v.head.push(Line::styled(" node                         status    rows      ms   columns / error", Style::new().fg(t.header)));
        for h in &p.hops {
            let colour = match h.status.as_str() {
                "error" => t.error,
                "ok" => t.accent,
                _ => t.muted,
            };
            let tail = h.error.clone().unwrap_or_else(|| h.columns.join(", "));
            v.head.push(Line::from(vec![
                Span::styled(format!(" {:<28} ", h.name), Style::new().fg(t.fg)),
                Span::styled(format!("{:<8}", h.status), Style::new().fg(colour)),
                Span::styled(format!("{:>6} {:>7}   {tail}", h.rows, h.ms), Style::new().fg(t.muted)),
            ]));
        }
        for s in &p.saved {
            v.head.push(muted(&t, format!(" saved {s}")));
        }
        v.head.push(Line::raw(""));
    }
    v.sel = (!st.list.is_empty()).then_some(st.sel);
    v.keys = vec![("↑↓ pipe", KeyCode::Down), ("Esc", KeyCode::Esc)];
    draw_view(f, r, v, &t, hits);
}
