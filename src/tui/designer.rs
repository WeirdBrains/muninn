//! The Pipeline Designer (E): pipelines as graphs, edited node by node and edge by edge,
//! validated as you go (every leaf a model table: M ends a branch in one, made if it does
//! not exist), run on a worker thread (the UI stays live; hops and view frames arrive
//! through the history like any other run), monitored, exported and imported.
use super::workspace::{Ask, Typed, View, ask_key, draw_view, muted, row_style};
use super::{App, Hit};
use crate::code::pipeline::{self as pl, Graph, NodeKind};
use crate::code::{self, CodeEnv};
use crate::store::Store;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use std::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DAsk {
    New,
    Node,
    Edge,
    Tee,
    Sink,
    Export,
    Import,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    #[default]
    List,
    Edit,
}

pub struct Job {
    pub pipeline: String,
    pub rx: mpsc::Receiver<Result<String, String>>,
}

#[derive(Default)]
pub struct DesignerState {
    pub job: Option<Job>,
    pub screen: Screen,
    pub list: Vec<crate::store::pipelines::Pipeline>,
    pub sel: usize,
    /// The pipeline being edited (id) and its selected node.
    pub open: Option<String>,
    pub node: usize,
    pub ask: Option<Ask<DAsk>>,
    pub msg: Option<(String, bool)>,
    /// An import waiting for y: the file, its plan.
    pub import: Option<(pl::Export, pl::ImportPlan)>,
    /// Asked to remove the selected pipeline (y).
    pub removing: bool,
}

fn ok(app: &mut App, m: impl Into<String>) {
    app.designer.msg = Some((m.into(), false));
}

fn err(app: &mut App, m: impl std::fmt::Display) {
    app.designer.msg = Some((m.to_string(), true));
}

fn reload(app: &mut App) {
    app.designer.list = app.store.pipelines().unwrap_or_default();
    app.designer.sel = app.designer.sel.min(app.designer.list.len().saturating_sub(1));
}

pub fn open(app: &mut App) {
    reload(app);
    app.designer.screen = Screen::List;
    app.modal = Some(super::modal::Modal::Pipelines);
}

/// Open the Designer on one pipeline.
pub fn open_pipeline(app: &mut App, id: &str) {
    open(app);
    app.designer.open = Some(id.to_string());
    app.designer.screen = Screen::Edit;
    app.designer.node = 0;
}

fn current(app: &App) -> Option<crate::store::pipelines::Pipeline> {
    app.designer.open.as_ref().and_then(|id| app.store.pipeline(id).ok().flatten())
}

fn selected_name(app: &App) -> Option<String> {
    match app.designer.screen {
        Screen::Edit => current(app).map(|p| p.name),
        Screen::List => app.designer.list.get(app.designer.sel).map(|p| p.name.clone()),
    }
}

/// The graph's nodes in run order (or as stored when it has a cycle).
fn order(g: &Graph) -> Vec<String> {
    g.order().unwrap_or_else(|_| g.nodes.iter().map(|n| n.id.clone()).collect())
}

fn sel_node(app: &App) -> Option<pl::Node> {
    let p = current(app)?;
    let ids = order(&p.graph);
    ids.get(app.designer.node).and_then(|id| p.graph.node(id).cloned())
}

/// Change the open pipeline's graph (a new version) and say what is still wrong.
fn change(app: &mut App, f: impl FnOnce(&mut Graph) -> crate::Result<String>) {
    let Some(p) = current(app) else { return };
    let mut g = p.graph.clone();
    match f(&mut g) {
        Ok(m) => match app.store.update_pipeline(&p.id, None, None, Some(&g)) {
            Ok(q) => {
                note(app, "pipeline.edit", format!("pipeline {} edited (v{})", q.name, q.version), &q.id);
                let probs = q.graph.problems(Some(&app.store));
                if probs.is_empty() { ok(app, format!("{m} · valid")) } else { ok(app, format!("{m} · {} to fix", probs.len())) }
            }
            Err(e) => err(app, e),
        },
        Err(e) => err(app, e),
    }
    reload(app);
}

fn note(app: &App, kind: &str, summary: String, id: &str) {
    app.store.note_action(&crate::store::NewAction::new(kind, "tui", summary).params(serde_json::json!({ "pipeline": id })));
}

/// The columns last seen at a node (its view or code output), for a new model's fields.
fn seen_columns(app: &App, p: &str, n: &pl::Node, g: &Graph) -> Option<serde_json::Value> {
    let key = match &n.kind {
        NodeKind::View { .. } => format!("view:{p}/{}", n.id),
        NodeKind::Code { space } => format!("code:{space}"),
        _ => {
            let up = g.ups(&n.id).first().map(|e| e.from.clone())?;
            return g.node(&up).and_then(|u| seen_columns(app, p, u, g));
        }
    };
    app.frames.data.get(&key).cloned()
}

/// M: end the selected node's branch in a model table, making the model when it is new
/// (its fields from the columns last seen there).
fn end_in_model(app: &mut App, model: String) {
    let Some(p) = current(app) else { return };
    let Some(n) = sel_node(app) else { return };
    if model.is_empty() {
        return err(app, "a model name");
    }
    let mut made = String::new();
    if app.store.find_model(&model).is_err() {
        let mut design = crate::store::Design::default();
        if let Some(f) = seen_columns(app, &p.name, &n, &p.graph) {
            for c in super::frametile::columns(&f) {
                if c.starts_with('_') || c == "row" {
                    continue;
                }
                let num = f["columns"][&c].as_array().is_some_and(|v| v.iter().all(|x| x.is_number() || x.is_null()));
                design.fields.push(crate::store::Field::new(&c, if num { crate::store::FieldType::Real } else { crate::store::FieldType::Text }));
            }
        }
        let k = design.fields.len();
        match app.store.create_model(&model, &format!("rows of pipeline {}", p.name), design, true) {
            Ok(_) => {
                made = if k > 0 {
                    format!("made model {model} ({k} fields from what passed there) · ")
                } else {
                    format!("made model {model} (no fields yet: add them in the Model Designer, D) · ")
                }
            }
            Err(e) => return err(app, e),
        }
    }
    let mut id = format!("{}_out", n.id);
    while p.graph.node(&id).is_some() {
        id.push('_');
    }
    change(app, |g| {
        g.add_node(&format!("{id}={}", NodeKind::Sink { model: model.clone() }.text()))?;
        g.add_edges(&format!("{}>{id}", n.id))?;
        Ok(format!("{made}{} ends in model {model}", n.id))
    });
}

fn answer(app: &mut App, kind: DAsk, v: String) {
    match kind {
        DAsk::New => {
            if v.is_empty() {
                return;
            }
            match app.store.create_pipeline(&v, "", &Graph::default()) {
                Ok(p) => {
                    note(app, "pipeline.create", format!("pipeline {} created", p.name), &p.id);
                    app.designer.open = Some(p.id);
                    app.designer.screen = Screen::Edit;
                    app.designer.node = 0;
                    ok(app, "a: add a node (src=search:NAME, -, code:SPACE, view:chart, model:NAME) · e: connect A>B");
                }
                Err(e) => err(app, e),
            }
            reload(app);
        }
        DAsk::Node => change(app, |g| {
            g.add_node(&v)?;
            Ok(format!("node {}", v.split('=').next().unwrap_or_default()))
        }),
        DAsk::Edge => change(app, |g| {
            g.add_edges(&v)?;
            Ok(format!("connected {v}"))
        }),
        DAsk::Tee => {
            let Some(n) = sel_node(app) else { return };
            change(app, |g| {
                crate::code::pipe::Sink::parse(&v)?;
                if let Some(x) = g.nodes.iter_mut().find(|x| x.id == n.id) {
                    x.tees.push(v.clone());
                }
                Ok(format!("{} also saves to {v}", n.id))
            })
        }
        DAsk::Sink => end_in_model(app, v),
        DAsk::Export => {
            let Some(name) = selected_name(app) else { return };
            let path = super::forms::expand(&v);
            match pl::export(&app.store, &name, true).and_then(|e| Ok(std::fs::write(&path, serde_json::to_vec_pretty(&e)?)?)) {
                Ok(()) => {
                    note(app, "pipeline.export", format!("pipeline {name} exported"), &name);
                    ok(app, format!("exported {name} to {}", path.display()))
                }
                Err(e) => err(app, e),
            }
        }
        DAsk::Import => {
            // PATH, PATH as NAME, or PATH merge
            let (path, name, merge) = if let Some((p, n)) = v.split_once(" as ") {
                (p.trim().to_string(), Some(n.trim().to_string()), false)
            } else if let Some(p) = v.strip_suffix(" merge") {
                (p.trim().to_string(), None, true)
            } else {
                (v.clone(), None, false)
            };
            let r = std::fs::read(super::forms::expand(&path))
                .map_err(crate::Error::from)
                .and_then(|b| pl::Export::parse(&b))
                .and_then(|e| pl::plan_import(&app.store, &e, name.as_deref(), merge).map(|p| (e, p)));
            match r {
                Ok((e, plan)) => {
                    ok(app, "the plan is above: y imports, any other key cancels");
                    app.designer.import = Some((e, plan));
                }
                Err(e) => err(app, format!("{e} (type `PATH as NAME` to rename, `PATH merge` for a new version)")),
            }
        }
    }
}

/// Approve a draft pipeline (from a peer): it can run once its code is approved too.
pub fn approve(s: &Store, origin: &str, p: &crate::store::pipelines::Pipeline) -> crate::Result<String> {
    s.set_pipeline_draft(&p.id, false)?;
    s.note_action(
        &crate::store::NewAction::new("pipeline.approve", origin, format!("pipeline {} approved", p.name))
            .params(serde_json::json!({ "pipeline": p.id })),
    );
    let waiting: Vec<String> = p.graph.code_spaces().into_iter().filter(|c| s.find_code_space(c).is_ok_and(|c| !code::approved(&c))).collect();
    Ok(if waiting.is_empty() {
        format!("approved {}", p.name)
    } else {
        format!("approved {}; its code still needs approval: {}", p.name, waiting.join(", "))
    })
}

fn ask(app: &mut App, kind: DAsk, title: &str, value: &str) {
    app.designer.ask = Some(Ask::new(kind, title, value));
}

pub fn on_key(app: &mut App, k: KeyEvent) {
    if let Some(a) = app.designer.ask.as_mut() {
        match ask_key(a, k) {
            Typed::Cancel => app.designer.ask = None,
            Typed::Done(v) => {
                let kind = a.kind;
                app.designer.ask = None;
                answer(app, kind, v);
            }
            Typed::Editing => {}
        }
        return;
    }
    if let Some((e, plan)) = app.designer.import.take() {
        if k.code == KeyCode::Char('y') {
            match pl::import(&app.store, &e, &plan) {
                Ok(p) => {
                    note(app, "pipeline.import", format!("pipeline {} imported (v{})", p.name, p.version), &p.id);
                    ok(app, format!("imported {} v{}: approve its code before it runs (Code window, C)", p.name, p.version));
                }
                Err(e) => err(app, e),
            }
            reload(app);
        } else {
            ok(app, "import cancelled");
        }
        return;
    }
    if app.designer.removing {
        app.designer.removing = false;
        if k.code == KeyCode::Char('y')
            && let Some(p) = app.designer.list.get(app.designer.sel).cloned()
        {
            match app.store.remove_pipeline(&p.id) {
                Ok(()) => {
                    note(app, "pipeline.remove", format!("pipeline {} removed", p.name), &p.id);
                    ok(app, format!("removed {}", p.name));
                }
                Err(e) => err(app, e),
            }
            reload(app);
        } else {
            ok(app, "kept");
        }
        return;
    }
    app.designer.msg = None;
    let name = selected_name(app);
    match (app.designer.screen, k.code) {
        (_, KeyCode::Char('r')) => {
            if let Some(n) = name {
                run(app, &n);
                ok(app, app.status.clone());
            }
        }
        (_, KeyCode::Char('m')) => {
            if let Some(n) = name
                && super::frametile::open(app, super::frametile::FrameTile::new(format!("monitor:{n}"))).is_some()
            {
                app.modal = None;
            }
        }
        (_, KeyCode::Char('x')) => {
            if let Some(n) = name {
                ask(app, DAsk::Export, "Export to file", &format!("{n}.pipeline.json"));
            }
        }
        (_, KeyCode::Char('A')) => {
            let p = match app.designer.screen {
                Screen::Edit => current(app),
                Screen::List => app.designer.list.get(app.designer.sel).cloned(),
            };
            if let Some(p) = p.filter(|p| p.draft) {
                match approve(&app.store, "tui", &p) {
                    Ok(m) => ok(app, m),
                    Err(e) => err(app, e),
                }
                reload(app);
            } else {
                ok(app, "not a draft");
            }
        }
        (_, KeyCode::Char('i')) => ask(app, DAsk::Import, "Import file (PATH, PATH as NAME, PATH merge)", ""),
        (Screen::List, KeyCode::Down | KeyCode::Char('j')) => {
            app.designer.sel = (app.designer.sel + 1).min(app.designer.list.len().saturating_sub(1))
        }
        (Screen::List, KeyCode::Up | KeyCode::Char('k')) => app.designer.sel = app.designer.sel.saturating_sub(1),
        (Screen::List, KeyCode::Char('n')) => ask(app, DAsk::New, "New pipeline's name", ""),
        (Screen::List, KeyCode::Char('d')) => {
            if let Some(n) = name {
                app.designer.removing = true;
                ok(app, format!("remove pipeline {n}? y/n"));
            }
        }
        (Screen::List, KeyCode::Enter | KeyCode::Char('e')) => {
            if let Some(p) = app.designer.list.get(app.designer.sel) {
                app.designer.open = Some(p.id.clone());
                app.designer.screen = Screen::Edit;
                app.designer.node = 0;
            }
        }
        (Screen::List, KeyCode::Esc | KeyCode::Char('q')) => app.modal = None,
        (Screen::Edit, KeyCode::Down | KeyCode::Char('j')) => {
            let n = current(app).map(|p| p.graph.nodes.len()).unwrap_or(0);
            app.designer.node = (app.designer.node + 1).min(n.saturating_sub(1));
        }
        (Screen::Edit, KeyCode::Up | KeyCode::Char('k')) => app.designer.node = app.designer.node.saturating_sub(1),
        (Screen::Edit, KeyCode::Char('a')) => {
            ask(app, DAsk::Node, "Node ID=KIND (src=search:NAME, -, code:SPACE, view:chart:col, filter:SEARCH, model:NAME)", "")
        }
        (Screen::Edit, KeyCode::Char('e')) => {
            let from = sel_node(app).map(|n| format!("{}>", n.id)).unwrap_or_default();
            ask(app, DAsk::Edge, "Connect FROM>TO[:NAME] (A>B>C)", &from)
        }
        (Screen::Edit, KeyCode::Char('t')) => {
            if sel_node(app).is_some() {
                ask(app, DAsk::Tee, "Also save here to (journal:NAME or model:NAME)", "journal:")
            }
        }
        (Screen::Edit, KeyCode::Char('M')) => {
            if let Some(n) = sel_node(app) {
                ask(app, DAsk::Sink, &format!("End {} in model table (made when new)", n.id), &format!("{}_rows", n.id))
            }
        }
        (Screen::Edit, KeyCode::Char('D') | KeyCode::Delete) => {
            if let Some(n) = sel_node(app) {
                change(app, |g| {
                    g.remove_node(&n.id);
                    Ok(format!("removed {}", n.id))
                });
                app.designer.node = app.designer.node.saturating_sub(1);
            }
        }
        (Screen::Edit, KeyCode::Enter) => {
            // open what the node is: a view or code tile, a code space
            if let (Some(p), Some(n)) = (current(app), sel_node(app)) {
                let key = match &n.kind {
                    NodeKind::View { .. } => Some(format!("view:{}/{}", p.name, n.id)),
                    NodeKind::Code { space } => Some(format!("code:{space}")),
                    _ => None,
                };
                if let Some(key) = key
                    && super::frametile::open(app, super::frametile::FrameTile::new(key)).is_some()
                {
                    app.modal = None;
                }
            }
        }
        (Screen::Edit, KeyCode::Esc | KeyCode::Char('q')) => {
            app.designer.screen = Screen::List;
            reload(app);
        }
        _ => {}
    }
}

pub fn draw(app: &mut App, f: &mut Frame, r: Rect, hits: &mut Vec<(Rect, Hit)>) {
    let t = app.theme.clone();
    let st = &app.designer;
    let mut v;
    match st.screen {
        Screen::List => {
            v = View::new("Pipelines · graphs of sources, code, views and filters that end in model tables");
            if st.list.is_empty() {
                v.head.push(muted(&t, " no pipelines yet: n makes one (or `mnnn pipeline create`), i imports a file"));
            }
            for (i, p) in st.list.iter().enumerate() {
                let bad = p.graph.problems(Some(&app.store)).len();
                let state = if p.draft {
                    "draft from a peer: A approves".to_string()
                } else if bad == 0 {
                    "valid".to_string()
                } else {
                    format!("{bad} to fix")
                };
                v.rows.push(Line::from(vec![
                    Span::styled(format!(" {:<24}", p.name), row_style(&t, i == st.sel)),
                    Span::styled(
                        format!(" v{:<3} {:>2} nodes → {}  ", p.version, p.graph.nodes.len(), p.graph.sinks().join(", ")),
                        Style::new().fg(t.fg),
                    ),
                    Span::styled(state, Style::new().fg(if bad == 0 && !p.draft { t.success } else { t.warning })),
                ]));
            }
            v.sel = (!st.list.is_empty()).then_some(st.sel);
            v.keys = vec![
                ("n new", KeyCode::Char('n')),
                ("Enter edit", KeyCode::Enter),
                ("r run", KeyCode::Char('r')),
                ("m monitor", KeyCode::Char('m')),
                ("x export", KeyCode::Char('x')),
                ("i import", KeyCode::Char('i')),
                ("d remove", KeyCode::Char('d')),
                ("Esc", KeyCode::Esc),
            ];
        }
        Screen::Edit => {
            let Some(p) = current(app) else {
                app.designer.screen = Screen::List;
                return;
            };
            v = View::new(format!("Pipeline {} · v{}", p.name, p.version));
            let ids = order(&p.graph);
            for (i, id) in ids.iter().enumerate() {
                let Some(n) = p.graph.node(id) else { continue };
                let to: Vec<String> =
                    p.graph.downs(id).iter().map(|e| e.name.as_ref().map(|x| format!("{} as {x}", e.to)).unwrap_or(e.to.clone())).collect();
                let tees = if n.tees.is_empty() { String::new() } else { format!("  + {}", n.tees.join(" + ")) };
                let colour = match n.kind {
                    NodeKind::Source { .. } => t.info,
                    NodeKind::Code { .. } => t.accent,
                    NodeKind::Sink { .. } => t.success,
                    _ => t.fg,
                };
                v.rows.push(Line::from(vec![
                    Span::styled(format!(" {:<12}", id), row_style(&t, i == st.node)),
                    Span::styled(format!(" {:<28}", n.kind.text()), Style::new().fg(colour)),
                    Span::styled(if to.is_empty() { String::new() } else { format!("→ {}", to.join(", ")) }, Style::new().fg(t.fg)),
                    Span::styled(tees, Style::new().fg(t.muted)),
                ]));
            }
            let probs = p.graph.problems(Some(&app.store));
            if p.draft {
                v.head.push(Line::styled(
                    " a draft from a peer: review it, then A approves it (its code needs approval too)",
                    Style::new().fg(t.warning),
                ));
            }
            if probs.is_empty() {
                v.head.push(Line::styled(" valid: every leaf ends in a model table", Style::new().fg(t.success)));
            }
            for x in probs {
                v.head.push(Line::styled(format!(" ! {x}"), Style::new().fg(t.warning)));
            }
            v.sel = (!ids.is_empty()).then_some(st.node);
            v.keys = vec![
                ("a node", KeyCode::Char('a')),
                ("e connect", KeyCode::Char('e')),
                ("M end in model", KeyCode::Char('M')),
                ("t tee", KeyCode::Char('t')),
                ("D remove", KeyCode::Char('D')),
                ("Enter open", KeyCode::Enter),
                ("r run", KeyCode::Char('r')),
                ("m monitor", KeyCode::Char('m')),
                ("x export", KeyCode::Char('x')),
                ("Esc", KeyCode::Esc),
            ];
        }
    }
    if let Some((_, plan)) = &st.import {
        v.head.push(Line::raw(""));
        for l in plan.lines() {
            v.head.push(Line::styled(format!(" {l}"), Style::new().fg(t.info)));
        }
    }
    v.ask = st.ask.as_ref().map(|a| (a.title.clone(), a.value.clone()));
    v.msg = st.msg.clone();
    super::share::add_key(app, &mut v.keys);
    draw_view(f, r, v, &t, hits);
}

/// Run a pipeline on a thread: its code spaces must be approved (the Code window asks).
pub fn run(app: &mut App, name: &str) {
    if app.designer.job.is_some() {
        return app.set_status("a pipeline is running already");
    }
    let p = match app.store.find_pipeline(name) {
        Ok(p) => p,
        Err(e) => return app.set_status(e.to_string()),
    };
    if p.draft {
        return app.set_status(format!("{} is a draft from a peer: review it, then approve it (A in the Pipelines window)", p.name));
    }
    let problems = p.graph.problems(Some(&app.store));
    if !problems.is_empty() {
        return app.set_status(format!("{} is not valid: {}", p.name, problems.join("; ")));
    }
    for s in p.graph.code_spaces() {
        match app.store.find_code_space(&s) {
            Ok(c) if code::approved(&c) => {}
            Ok(c) => return app.set_status(format!("approve code space {} first (Code window: open it, r asks)", c.name)),
            Err(e) => return app.set_status(e.to_string()),
        }
    }
    super::frametile::open_many(app, super::frametile::views_of(&p));
    let (db, dek, home) = (app.paths.db.clone(), app.dek.clone(), app.paths.home.clone());
    let (tx, rx) = mpsc::channel();
    let pname = p.name.clone();
    std::thread::spawn(move || {
        let r = (|| -> crate::Result<String> {
            let s = Store::open(&db, Some(&dek.0))?;
            let env = CodeEnv::new(&home);
            let r = code::pipeline::run(&s, &env, "tui", &p, None, &mut |_| {})?;
            let landed: Vec<String> = r.landed.iter().map(|(m, n)| format!("{n} → {m}")).collect();
            Ok(format!("pipeline {} {} · {}", p.name, if r.ok() { "ran" } else { "ran with errors" }, landed.join(", ")))
        })();
        let _ = tx.send(r.map_err(|e| e.to_string()));
    });
    app.designer.job = Some(Job { pipeline: pname.clone(), rx });
    app.set_status(format!("running pipeline {pname}… (the UI stays live)"));
}

pub fn poll(app: &mut App) {
    let Some(j) = app.designer.job.as_ref() else { return };
    let r = match j.rx.try_recv() {
        Ok(r) => r,
        Err(mpsc::TryRecvError::Empty) => return,
        Err(mpsc::TryRecvError::Disconnected) => Err(format!("pipeline {} stopped", j.pipeline)),
    };
    app.designer.job = None;
    match r {
        Ok(m) => app.set_status(m),
        Err(e) => app.set_status(format!("pipeline failed: {e}")),
    }
    app.refresh_history(true);
    app.refresh_rows();
}
