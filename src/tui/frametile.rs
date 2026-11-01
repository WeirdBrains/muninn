//! Data tiles: a code space's result, a pipeline view node's output (a chart or a table,
//! with the columns or series you pick) and a pipeline's monitor. They update on every run
//! here and on `pipe.frame` / `pipe.hop` records from runs anywhere on the same home.
use super::layout::{Dir, Tile};
use super::{App, Hit};
use crate::theme::Theme;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Axis, Block, Chart, Dataset, GraphType, Paragraph};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// What a data tile shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FrameTile {
    /// `code:SPACE`, `view:PIPELINE/NODE` or `monitor:PIPELINE`.
    pub key: String,
    /// A table instead of a chart.
    #[serde(default)]
    pub table: bool,
    /// The columns (table) or series (chart) shown; empty: all (charts: the numeric ones).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<String>,
}

impl FrameTile {
    pub fn new(key: impl Into<String>) -> Self {
        FrameTile { key: key.into(), ..Default::default() }
    }
    pub fn monitor(&self) -> Option<&str> {
        self.key.strip_prefix("monitor:")
    }
}

#[derive(Debug, Default)]
pub struct FramesState {
    pub seen: Option<i64>,
    /// The latest frame per key.
    pub data: HashMap<String, Value>,
    /// Selected row (tables, monitors) per tile.
    pub sel: HashMap<u32, usize>,
}

/// Columns of a frame in field order.
pub fn columns(f: &Value) -> Vec<String> {
    let cols = f.get("columns").and_then(Value::as_object);
    let mut v: Vec<String> = f
        .get("fields")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|x| x["name"].as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    v.retain(|n| cols.is_some_and(|c| c.contains_key(n)));
    if let Some(c) = cols {
        for k in c.keys() {
            if !v.contains(k) {
                v.push(k.clone());
            }
        }
    }
    v
}

fn col<'a>(f: &'a Value, name: &str) -> &'a [Value] {
    f.get("columns").and_then(|c| c.get(name)).and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
}

pub fn rows(f: &Value) -> usize {
    columns(f).first().map(|c| col(f, c).len()).unwrap_or(0)
}

fn cell(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Number(n) => n.as_f64().map(super::tiles::fmt_num).unwrap_or_else(|| n.to_string()),
        o => o.to_string(),
    }
}

/// The x column of a chart: a time column if there is one.
pub fn x_column(f: &Value) -> Option<String> {
    let cs = columns(f);
    ["timestamp", "ts", "time", "date", "_created_at"]
        .iter()
        .map(|s| s.to_string())
        .find(|c| cs.contains(c) && col(f, c).iter().all(|v| v.is_number() || v.is_null()))
}

/// The series a chart draws: the picked ones, else every numeric column but x (at most 6).
pub fn series(f: &Value, t: &FrameTile) -> Vec<String> {
    let x = x_column(f);
    let numeric = |c: &String| {
        let v = col(f, c);
        v.iter().any(Value::is_number) && v.iter().all(|x| x.is_number() || x.is_null())
    };
    let all: Vec<String> = columns(f).into_iter().filter(|c| Some(c) != x.as_ref() && numeric(c) && !c.starts_with('_')).collect();
    if t.columns.is_empty() { all.into_iter().take(6).collect() } else { t.columns.iter().filter(|c| all.contains(c)).cloned().collect() }
}

/// Read `pipe.frame` records into the frames tiles show; a pipeline that starts opens
/// tiles for its view nodes when none shows them yet.
pub fn poll(app: &mut App) {
    let Some(seen) = app.frames.seen else {
        app.frames.seen = Some(app.store.history_stamp().map(|s| s.1).unwrap_or(0));
        return;
    };
    let mut acts = app.store.actions_of_kind_after("pipe.frame", seen).unwrap_or_default();
    acts.extend(app.store.actions_of_kind_after("pipe.start", seen).unwrap_or_default());
    if acts.is_empty() {
        return;
    }
    acts.sort_by_key(|a| a.id);
    app.frames.seen = acts.last().map(|a| a.id);
    for a in acts {
        if a.kind == "pipe.start" {
            let views = a.params["pipeline"].as_str().and_then(|p| app.store.find_pipeline(p).ok()).map(|p| views_of(&p)).unwrap_or_default();
            if !views.is_empty() && !views.iter().any(|v| shown(app, &v.key)) {
                open_many(app, views);
            }
            continue;
        }
        if let Some(k) = a.params["key"].as_str() {
            app.frames.data.insert(k.to_string(), a.params["frame"].clone());
        }
    }
}

/// A pipeline's view nodes as tiles (their mode and columns).
pub fn views_of(p: &crate::store::pipelines::Pipeline) -> Vec<FrameTile> {
    p.graph
        .nodes
        .iter()
        .filter_map(|n| match &n.kind {
            crate::code::pipeline::NodeKind::View { view, columns } => {
                Some(FrameTile { key: format!("view:{}/{}", p.name, n.id), table: view == "table", columns: columns.clone() })
            }
            _ => None,
        })
        .collect()
}

fn shown(app: &App, key: &str) -> bool {
    app.layout.tiles().iter().any(|t| t.frame.as_ref().is_some_and(|f| f.key == key))
}

/// Show `ft` in a tile: focus the one showing it already, take the focused tile when it
/// shows nothing, else split it (right, or below when too narrow).
pub fn open(app: &mut App, ft: FrameTile) -> Option<u32> {
    if let Some(id) = app.layout.tiles().iter().find(|t| t.frame.as_ref().is_some_and(|f| f.key == ft.key)).map(|t| t.id) {
        app.layout.set_focus(id);
        return Some(id);
    }
    let area = if app.content.width > 0 { app.content } else { Rect::new(0, 0, 200, 50) };
    let blank = { app.layout.focused().series.is_none() && app.layout.focused().frame.is_none() };
    let id = if blank {
        app.layout.focus
    } else {
        match app.layout.split(Dir::Horizontal, None, area).or_else(|_| app.layout.split(Dir::Vertical, None, area)) {
            Ok(id) => id,
            Err(e) => {
                app.set_status(format!("no room for a tile: {e}"));
                return None;
            }
        }
    };
    if let Some(t) = app.layout.tile_mut(id) {
        t.series = None;
        t.link = None;
        t.frame = Some(Box::new(ft));
    }
    app.layout_changed();
    Some(id)
}

/// Several tiles side by side (a pipeline's views: three make three columns).
pub fn open_many(app: &mut App, v: Vec<FrameTile>) {
    let n = v.len();
    for ft in v {
        open(app, ft);
    }
    if n > 1 {
        app.layout.equalize();
        app.layout_changed();
    }
}

pub fn title(t: &Tile, ft: &FrameTile, app: &App) -> String {
    if let Some(p) = ft.monitor() {
        return format!("[{}] monitor · {p}", t.id);
    }
    let n = app.frames.data.get(&ft.key).map(rows).unwrap_or(0);
    let what = if ft.table { "table" } else { "chart" };
    let cols = if ft.columns.is_empty() { String::new() } else { format!(" · {}", ft.columns.join(", ")) };
    format!("[{}] {what} · {} · {n} rows{cols}", t.id, ft.key)
}

#[allow(clippy::too_many_arguments)]
pub fn draw(app: &mut App, f: &mut Frame, area: Rect, t: &Tile, ft: &FrameTile, focused: bool, th: &Theme, hits: &mut Vec<(Rect, Hit)>) {
    let block = Block::bordered()
        .title(Span::styled(
            format!(" {} ", title(t, ft, app)),
            Style::new().fg(if focused { th.accent } else { th.header }).add_modifier(Modifier::BOLD),
        ))
        .border_style(Style::new().fg(if focused { th.border_focus } else { th.border }))
        .style(Style::new().bg(th.bg).fg(th.fg));
    let inner = block.inner(area);
    f.render_widget(block, area);
    hits.push((area, Hit::Tile(t.id)));
    let sel = app.frames.sel.get(&t.id).copied().unwrap_or(0);
    if let Some(p) = ft.monitor() {
        let lines = monitor_lines(app, p, sel, th);
        f.render_widget(Paragraph::new(lines), inner);
        return;
    }
    let Some(fr) = app.frames.data.get(&ft.key) else {
        let hint = if ft.key.starts_with("code:") {
            "no result yet: run the code space (r), or pipe data into it"
        } else {
            "waiting for data: run the pipeline"
        };
        f.render_widget(Paragraph::new(Span::styled(hint, Style::new().fg(th.muted))), inner);
        return;
    };
    if ft.table {
        f.render_widget(Paragraph::new(table_lines(fr, ft, sel, inner.height as usize, inner.width as usize, th)), inner);
    } else {
        draw_chart(f, inner, fr, ft, th);
    }
}

pub fn table_lines(fr: &Value, ft: &FrameTile, sel: usize, height: usize, width: usize, th: &Theme) -> Vec<Line<'static>> {
    let cs: Vec<String> = if ft.columns.is_empty() { columns(fr) } else { ft.columns.iter().filter(|c| columns(fr).contains(c)).cloned().collect() };
    let n = rows(fr);
    let w = (width / cs.len().max(1)).clamp(6, 18);
    let pad = |s: &str| {
        let s: String = s.chars().take(w - 1).collect();
        format!("{s:<w$}")
    };
    let mut out =
        vec![Line::from(Span::styled(cs.iter().map(|c| pad(c)).collect::<String>(), Style::new().fg(th.header).add_modifier(Modifier::BOLD)))];
    let body = height.saturating_sub(1).max(1);
    // newest rows at the bottom; the selection scrolls back from the end
    let sel = sel.min(n.saturating_sub(1));
    let end = n.saturating_sub(sel.saturating_sub(body.saturating_sub(1)));
    let start = end.saturating_sub(body);
    for i in start..end {
        let text: String = cs.iter().map(|c| pad(&cell(col(fr, c).get(i).unwrap_or(&Value::Null)))).collect();
        let st = if n - 1 - i == sel { Style::new().fg(th.selection_fg).bg(th.selection_bg) } else { Style::new() };
        out.push(Line::from(Span::styled(text, st)));
    }
    out
}

fn draw_chart(f: &mut Frame, r: Rect, fr: &Value, ft: &FrameTile, th: &Theme) {
    let xs = x_column(fr);
    let n = rows(fr);
    let x: Vec<f64> = match &xs {
        Some(c) => col(fr, c).iter().enumerate().map(|(i, v)| v.as_f64().unwrap_or(i as f64)).collect(),
        None => (0..n).map(|i| i as f64).collect(),
    };
    let ss = series(fr, ft);
    if ss.is_empty() || n == 0 {
        f.render_widget(Paragraph::new(Span::styled("no numeric series to chart: t shows the table", Style::new().fg(th.muted))), r);
        return;
    }
    let pts: Vec<Vec<(f64, f64)>> = ss.iter().map(|s| col(fr, s).iter().zip(&x).filter_map(|(v, x)| v.as_f64().map(|y| (*x, y))).collect()).collect();
    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
    for p in pts.iter().flatten() {
        lo = lo.min(p.1);
        hi = hi.max(p.1);
    }
    if lo == hi {
        lo -= 1.0;
        hi += 1.0;
    }
    let (x0, x1) = (x.first().copied().unwrap_or(0.0), x.last().copied().unwrap_or(1.0).max(x.first().copied().unwrap_or(0.0) + 1.0));
    let lab = |v: f64| if xs.is_some() && v > 1e11 { crate::ingest::time::fmt_ts(v as i64)[5..16].to_string() } else { super::tiles::fmt_num(v) };
    let sets: Vec<Dataset> = ss
        .iter()
        .zip(&pts)
        .enumerate()
        .map(|(i, (name, p))| {
            Dataset::default()
                .name(name.clone())
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::new().fg(th.charts[i % th.charts.len()]))
                .data(p)
        })
        .collect();
    let chart = Chart::new(sets)
        .style(Style::new().bg(th.bg).fg(th.fg))
        .x_axis(Axis::default().bounds([x0, x1]).labels(vec![Span::raw(lab(x0)), Span::raw(lab(x1))]).style(Style::new().fg(th.muted)))
        .y_axis(
            Axis::default()
                .bounds([lo, hi])
                .labels(vec![Span::raw(super::tiles::fmt_num(lo)), Span::raw(super::tiles::fmt_num(hi))])
                .style(Style::new().fg(th.muted)),
        );
    f.render_widget(chart, r);
}

/// One monitor row per node.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NodeState {
    pub id: String,
    pub kind: String,
    /// idle, running, ok, error, skipped or waiting approval.
    pub status: String,
    pub rows_in: u64,
    pub rows_out: u64,
    pub ms: u64,
    pub updated: i64,
    pub error: Option<String>,
}

/// A pipeline's nodes in order with their state in its latest run (from Pipes).
pub fn monitor(app: &App, pipeline: &str) -> (Vec<NodeState>, Vec<String>, String) {
    let Ok(p) = app.store.find_pipeline(pipeline) else { return (vec![], vec![], format!("no pipeline {pipeline}")) };
    let run = app.pipes.list.iter().find(|r| r.pipeline.as_deref() == Some(p.name.as_str()));
    let order = p.graph.order().unwrap_or_else(|_| p.graph.nodes.iter().map(|n| n.id.clone()).collect());
    let mut first_open = true;
    let v = order
        .iter()
        .filter_map(|id| p.graph.node(id))
        .map(|n| {
            let mut s = NodeState { id: n.id.clone(), kind: n.kind.text(), status: "idle".into(), ..Default::default() };
            if let Some(r) = run {
                match r.hops.iter().find(|h| h.name.starts_with(&format!("{} (", n.id)) && h.status != "waiting") {
                    Some(h) => {
                        s.status = if h.state.is_empty() { h.status.clone() } else { h.state.clone() };
                        (s.rows_in, s.rows_out, s.ms, s.updated, s.error) = (h.rows_in, h.rows, h.ms, h.updated, h.error.clone());
                    }
                    None if r.status == "running" && first_open => {
                        s.status = "running".into();
                        first_open = false;
                    }
                    None => {}
                }
            }
            s
        })
        .collect();
    let state = run.map(|r| r.status.clone()).unwrap_or_else(|| "not run yet".into());
    (v, run.map(|r| r.saved.clone()).unwrap_or_default(), state)
}

fn monitor_lines(app: &App, p: &str, sel: usize, th: &Theme) -> Vec<Line<'static>> {
    let (nodes, landed, state) = monitor(app, p);
    let mut out = vec![Line::from(Span::styled(
        format!("{:<10} {:<20} {:<17} {:>7} {:>7} {:>9} {:>8}  {}", "node", "kind", "status", "in", "out", "rows/s", "updated", state),
        Style::new().fg(th.header).add_modifier(Modifier::BOLD),
    ))];
    for (i, n) in nodes.iter().enumerate() {
        let rate = if n.ms > 0 {
            format!("{:.0}", n.rows_out as f64 * 1000.0 / n.ms as f64)
        } else if n.rows_out > 0 {
            "∞".into()
        } else {
            String::new()
        };
        let when = if n.updated > 0 { crate::ingest::time::fmt_ts(n.updated)[11..19].to_string() } else { String::new() };
        let colr = match n.status.as_str() {
            "ok" => th.success,
            "error" => th.error,
            "waiting approval" | "skipped" => th.warning,
            "running" => th.info,
            _ => th.muted,
        };
        let mut st = Style::new().fg(colr);
        if i == sel {
            st = st.bg(th.selection_bg);
        }
        out.push(Line::from(Span::styled(
            format!("{:<10} {:<20} {:<17} {:>7} {:>7} {:>9} {:>8}", n.id, n.kind, n.status, n.rows_in, n.rows_out, rate, when),
            st,
        )));
        if let Some(e) = &n.error {
            out.push(Line::from(Span::styled(format!("           {e}"), Style::new().fg(th.error))));
        }
    }
    for l in landed {
        out.push(Line::from(Span::styled(format!("landed {l}"), Style::new().fg(th.success))));
    }
    out.push(Line::from(Span::styled("j/k select · Enter open the node · r run", Style::new().fg(th.muted))));
    out
}

/// Keys for a focused data tile.
pub fn on_key(app: &mut App, k: KeyEvent) -> bool {
    let id = app.layout.focus;
    let Some(ft) = app.layout.focused().frame.clone() else { return false };
    let sel = app.frames.sel.get(&id).copied().unwrap_or(0);
    match k.code {
        KeyCode::Down | KeyCode::Char('j') => {
            app.frames.sel.insert(id, if ft.monitor().is_some() { sel + 1 } else { sel.saturating_sub(1) });
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.frames.sel.insert(id, if ft.monitor().is_some() { sel.saturating_sub(1) } else { sel + 1 });
        }
        KeyCode::Char('t') if ft.monitor().is_none() => {
            if let Some(f) = app.layout.focused_mut().frame.as_mut() {
                f.table = !f.table;
            }
            app.layout_changed();
        }
        KeyCode::Char('c') if ft.monitor().is_none() => {
            let have = app.frames.data.get(&ft.key).map(columns).unwrap_or_default();
            app.modal = Some(super::modal::Modal::prompt(
                &format!("Columns or series, comma separated (empty: all) · {}", have.join(", ")),
                super::modal::PromptKind::FrameColumns,
            ));
            if let Some(super::modal::Modal::Prompt(p)) = app.modal.as_mut() {
                p.input = ft.columns.join(", ");
            }
        }
        KeyCode::Enter if ft.monitor().is_some() => monitor_enter(app, ft.monitor().unwrap_or_default().to_string(), sel),
        KeyCode::Char('r') if ft.monitor().is_some() => super::designer::run(app, ft.monitor().unwrap_or_default()),
        _ => return false,
    }
    true
}

/// The column prompt's answer.
pub fn set_columns(app: &mut App, input: &str) {
    let cols: Vec<String> = input.split(',').map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).collect();
    if let Some(f) = app.layout.focused_mut().frame.as_mut() {
        f.columns = cols;
    }
    app.layout_changed();
}

fn monitor_enter(app: &mut App, p: String, sel: usize) {
    let (nodes, _, _) = monitor(app, &p);
    let Some(n) = nodes.get(sel) else { return };
    let Ok(pl) = app.store.find_pipeline(&p) else { return };
    match pl.graph.node(&n.id).map(|x| x.kind.clone()) {
        Some(crate::code::pipeline::NodeKind::View { view, columns }) => {
            let id = open(app, FrameTile { key: format!("view:{}/{}", pl.name, n.id), table: view == "table", columns });
            if id.is_none() {
                app.set_status("no room for the view");
            }
        }
        Some(crate::code::pipeline::NodeKind::Code { space }) => {
            open(app, FrameTile::new(format!("code:{space}")));
        }
        Some(k) => app.set_status(format!(
            "{}: {} · {} rows in, {} out{}",
            n.id,
            k.text(),
            n.rows_in,
            n.rows_out,
            n.error.clone().map(|e| format!(" · {e}")).unwrap_or_default()
        )),
        None => {}
    }
}
