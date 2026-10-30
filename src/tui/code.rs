//! Projects ▸ Code spaces… (C): a list of code spaces and an editor window with build
//! output, run and a result tile. Thin over [`crate::code`]: builds run on a thread (they
//! need no database); runs happen on the UI thread (plugins use the store).
use super::workspace::{Ask, Back, Purpose, Typed, View, ask_key, draw_view, muted, pick_project, row_style};
use super::{App, Hit, block};
use crate::code::{self, CodeEnv};
use crate::ingest::time::fmt_ts;
use crate::store::{CodeChange, CodeInput, CodeSpace, ItemKind, Lang, RunInfo};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use std::sync::mpsc;

/// The one-line prompts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CAsk {
    /// A new code space: its name, then its language.
    Name,
    Lang,
    /// The input of the open space (`MODEL where … sort … limit N`).
    Input,
    Rename,
}

impl CAsk {
    fn title(self) -> &'static str {
        match self {
            CAsk::Name => "Code space name",
            CAsk::Lang => "Language (math, polars, rust, python, plugin)",
            CAsk::Input => "Input (MODEL [where C; C] [matching \"T\"] [group F] [sort -F] [limit N], or none)",
            CAsk::Rename => "New name",
        }
    }
}

/// A y/n question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirm {
    Delete,
    /// Approve the shown source, then run it.
    ApproveRun,
    /// Leave the editor without saving.
    Discard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    #[default]
    List,
    Edit,
}

/// A plain multi-line text editor (cursor in chars).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Editor {
    pub lines: Vec<String>,
    pub row: usize,
    pub col: usize,
    pub top: usize,
}

fn byte_at(s: &str, col: usize) -> usize {
    s.char_indices().nth(col).map(|(i, _)| i).unwrap_or(s.len())
}

impl Editor {
    pub fn new(text: &str) -> Self {
        let mut lines: Vec<String> = text.split('\n').map(|l| l.trim_end_matches('\r').to_string()).collect();
        if lines.len() > 1 && lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        Editor { lines, ..Default::default() }
    }
    pub fn text(&self) -> String {
        let mut s = self.lines.join("\n").trim_end_matches('\n').to_string();
        s.push('\n');
        s
    }
    fn len(&self, row: usize) -> usize {
        self.lines.get(row).map(|l| l.chars().count()).unwrap_or(0)
    }
    fn clamp(&mut self) {
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        self.row = self.row.min(self.lines.len() - 1);
        self.col = self.col.min(self.len(self.row));
    }
    fn insert(&mut self, c: char) {
        self.clamp();
        let l = &mut self.lines[self.row];
        let at = byte_at(l, self.col);
        l.insert(at, c);
        self.col += 1;
    }
    fn newline(&mut self) {
        self.clamp();
        let l = &mut self.lines[self.row];
        let at = byte_at(l, self.col);
        let rest = l.split_off(at);
        let indent: String = l.chars().take_while(|c| *c == ' ').collect();
        self.row += 1;
        self.col = indent.chars().count();
        self.lines.insert(self.row, indent + &rest);
    }
    fn backspace(&mut self) {
        self.clamp();
        if self.col > 0 {
            let l = &mut self.lines[self.row];
            let at = byte_at(l, self.col - 1);
            l.remove(at);
            self.col -= 1;
        } else if self.row > 0 {
            let l = self.lines.remove(self.row);
            self.row -= 1;
            self.col = self.len(self.row);
            self.lines[self.row].push_str(&l);
        }
    }
    fn delete(&mut self) {
        self.clamp();
        if self.col < self.len(self.row) {
            let l = &mut self.lines[self.row];
            let at = byte_at(l, self.col);
            l.remove(at);
        } else if self.row + 1 < self.lines.len() {
            let l = self.lines.remove(self.row + 1);
            self.lines[self.row].push_str(&l);
        }
    }
    /// Edit with a key; `false` when the key is not an editing key.
    pub fn key(&mut self, k: KeyEvent, page: usize) -> bool {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        match k.code {
            KeyCode::Char(c) if !ctrl => self.insert(c),
            KeyCode::Enter => self.newline(),
            KeyCode::Tab => (0..4).for_each(|_| self.insert(' ')),
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete => self.delete(),
            KeyCode::Left if self.col > 0 => self.col -= 1,
            KeyCode::Left if self.row > 0 => {
                self.row -= 1;
                self.col = self.len(self.row);
            }
            KeyCode::Right if self.col < self.len(self.row) => self.col += 1,
            KeyCode::Right if self.row + 1 < self.lines.len() => {
                self.row += 1;
                self.col = 0;
            }
            KeyCode::Left | KeyCode::Right => {}
            KeyCode::Up => self.row = self.row.saturating_sub(1),
            KeyCode::Down => self.row = (self.row + 1).min(self.lines.len().saturating_sub(1)),
            KeyCode::PageUp => self.row = self.row.saturating_sub(page.max(1)),
            KeyCode::PageDown => self.row = (self.row + page.max(1)).min(self.lines.len().saturating_sub(1)),
            KeyCode::Home => self.col = 0,
            KeyCode::End => self.col = self.len(self.row),
            _ => return false,
        }
        self.clamp();
        true
    }
    fn scroll_to(&mut self, h: usize) {
        if self.row < self.top {
            self.top = self.row;
        } else if h > 0 && self.row >= self.top + h {
            self.top = self.row + 1 - h;
        }
    }
}

/// What the build thread sends.
#[derive(Debug)]
pub enum Msg {
    Log(String),
    Done(std::result::Result<code::Built, String>, f64),
}

/// A build running on its thread; `then_run` runs the space when it is done.
#[derive(Debug)]
pub struct Building {
    pub rx: mpsc::Receiver<Msg>,
    pub space: CodeSpace,
    pub then_run: bool,
}

#[derive(Debug, Default)]
pub struct CodeState {
    pub screen: Screen,
    pub list: Vec<CodeSpace>,
    pub sel: usize,
    /// The space open in the editor (as saved).
    pub space: Option<CodeSpace>,
    pub editor: Editor,
    /// Keys go to the editor (Esc leaves it for the window's keys).
    pub editing: bool,
    pub log: Vec<String>,
    pub result: Option<RunInfo>,
    /// The result tile fills the window.
    pub wide: bool,
    pub ask: Option<Ask<CAsk>>,
    pub new_name: String,
    pub confirm: Option<Confirm>,
    pub msg: Option<(String, bool)>,
    pub building: Option<Building>,
}

impl CodeState {
    pub fn typing(&self) -> bool {
        self.ask.is_some() || (self.screen == Screen::Edit && self.editing)
    }
    fn selected(&self) -> Option<&CodeSpace> {
        self.list.get(self.sel)
    }
    pub fn dirty(&self) -> bool {
        self.space.as_ref().is_some_and(|s| s.source.trim_end() != self.editor.text().trim_end())
    }
}

fn err(app: &mut App, e: impl std::fmt::Display) {
    app.code.msg = Some((e.to_string(), true));
}

fn ok(app: &mut App, m: impl Into<String>) {
    app.code.msg = Some((m.into(), false));
}

pub fn open(app: &mut App) {
    let st = &mut app.code;
    st.screen = Screen::List;
    st.ask = None;
    st.confirm = None;
    st.msg = None;
    reopen(app);
}

/// Show the window again (after the project picker), as it was.
pub fn reopen(app: &mut App) {
    reload(app);
    app.modal = Some(super::modal::Modal::Code);
}

fn reload(app: &mut App) {
    app.code.list = app.store.code_spaces().unwrap_or_default();
    app.code.sel = app.code.sel.min(app.code.list.len().saturating_sub(1));
}

/// Open the editor on one code space (a code item in a project).
pub fn open_space(app: &mut App, id: &str) {
    open(app);
    match app.code.list.iter().position(|c| c.id == id) {
        Some(i) => {
            app.code.sel = i;
            edit_selected(app);
        }
        None => err(app, "that code space no longer exists"),
    }
}

fn edit_selected(app: &mut App) {
    let Some(c) = app.code.selected().cloned() else { return };
    let st = &mut app.code;
    st.editor = Editor::new(&c.source);
    st.result = c.last_run.clone();
    st.log = c.last_run.as_ref().map(|r| r.logs.clone()).unwrap_or_default();
    st.space = Some(c);
    st.screen = Screen::Edit;
    st.editing = false;
    st.wide = false;
}

fn status_of(c: &CodeSpace) -> &'static str {
    match (code::approved(c), c.approved_hash.is_some()) {
        (true, _) => "approved",
        (false, true) => "changed since approved",
        (false, false) => "not approved",
    }
}

/// Save the editor's text (a change needs approving again).
fn save(app: &mut App) -> bool {
    let Some(c) = app.code.space.clone() else { return false };
    if !app.code.dirty() {
        return true;
    }
    let ch = CodeChange { source: Some(app.code.editor.text()), ..Default::default() };
    match code::edit(&app.store, "tui", &c.id, &ch) {
        Ok(n) => {
            app.code.space = Some(n);
            app.refresh_history(true);
            reload(app);
            ok(app, "saved (a changed source is approved again before it runs)");
            true
        }
        Err(e) => {
            err(app, e);
            false
        }
    }
}

fn env(app: &App) -> crate::Result<CodeEnv> {
    Ok(CodeEnv::new(&app.paths.home))
}

/// b: build on a thread (r: then run).
fn start_build(app: &mut App, then_run: bool) {
    if app.code.building.is_some() {
        return ok(app, "a build is running already");
    }
    if !save(app) {
        return;
    }
    let Some(c) = app.code.space.clone() else { return };
    let env = match env(app) {
        Ok(e) => e,
        Err(e) => return err(app, e),
    };
    let (tx, rx) = mpsc::channel();
    let (lang, source) = (c.lang, c.source.clone());
    std::thread::spawn(move || {
        let t = std::time::Instant::now();
        let log = tx.clone();
        let r = code::build(&env, lang, &source, &mut |l| {
            let _ = log.send(Msg::Log(l.to_string()));
        });
        let _ = tx.send(Msg::Done(r.map_err(|e| e.to_string()), t.elapsed().as_secs_f64()));
    });
    app.code.log = vec![format!("building {} ({})…", c.name, c.lang.as_str())];
    app.code.building = Some(Building { rx, space: c, then_run });
    ok(app, "building… (the window stays usable)");
}

/// Take the build thread's output; a finished build is recorded (and runs when asked).
pub fn poll(app: &mut App) {
    let Some(b) = app.code.building.as_ref() else { return };
    let mut done = None;
    loop {
        match b.rx.try_recv() {
            Ok(Msg::Log(l)) => app.code.log.push(l),
            Ok(Msg::Done(r, secs)) => {
                done = Some((r, secs));
                break;
            }
            Err(mpsc::TryRecvError::Empty) => break,
            Err(mpsc::TryRecvError::Disconnected) => {
                done = Some((Err("the build stopped".into()), 0.0));
                break;
            }
        }
    }
    let Some((r, secs)) = done else { return };
    let Some(b) = app.code.building.take() else { return };
    match r {
        Ok(built) => {
            code::note_build(&app.store, "tui", &b.space, &built, secs);
            app.refresh_history(true);
            app.code.log.push(if built.cached { "already built".into() } else { format!("built in {secs:.1}s") });
            if b.then_run && app.code.space.as_ref().is_some_and(|s| s.id == b.space.id) {
                run_now(app);
            } else {
                ok(app, format!("built {}", b.space.name));
            }
        }
        Err(e) => {
            app.code.log.extend(e.lines().map(String::from));
            err(app, format!("build failed: {}", e.lines().next().unwrap_or("")));
        }
    }
}

/// r: run (asks to approve a new or changed source first).
fn run(app: &mut App) {
    if !save(app) {
        return;
    }
    let Some(c) = app.code.space.clone() else { return };
    if !code::approved(&c) {
        app.code.confirm = Some(Confirm::ApproveRun);
        let hash = code::trust_hash(&c).map(|h| h[..16].to_string()).unwrap_or_default();
        return ok(
            app,
            format!("{} is {}: it runs as native code with your rights. Approve this source ({hash}) and run? y/n", c.name, status_of(&c)),
        );
    }
    let env = match env(app) {
        Ok(e) => e,
        Err(e) => return err(app, e),
    };
    // compiled and not cached yet: build on the thread first
    if c.lang != Lang::Plugin && !env.plugin_path(&code::build_key(c.lang, &c.source)).exists() {
        return start_build(app, true);
    }
    run_now(app);
}

fn run_now(app: &mut App) {
    let Some(c) = app.code.space.clone() else { return };
    let env = match env(app) {
        Ok(e) => e,
        Err(e) => return err(app, e),
    };
    let mut logs = vec![];
    let res = code::run(&app.store, &env, "tui", &c, &mut |l| logs.push(l.to_string()));
    app.code.log.extend(logs);
    match res {
        Ok(info) => {
            app.code.log.extend(info.logs.iter().cloned());
            match &info.error {
                Some(e) => err(app, format!("the run failed: {e}")),
                None => ok(
                    app,
                    format!("ran {} in {} ms{}", c.name, info.ms, if info.writes > 0 { format!(", {} writes", info.writes) } else { String::new() }),
                ),
            }
            app.code.result = Some(info);
            if let Ok(Some(n)) = app.store.code_space(&c.id) {
                app.code.space = Some(n);
            }
            reload(app);
            app.refresh_history(true);
            app.refresh_rows();
        }
        Err(e) => err(app, e),
    }
}

fn ask(app: &mut App, kind: CAsk, value: impl Into<String>) {
    app.code.ask = Some(Ask::new(kind, kind.title(), value));
}

fn answer(app: &mut App, kind: CAsk, v: String) {
    match kind {
        CAsk::Name => {
            if v.is_empty() {
                return err(app, "a code space needs a name");
            }
            app.code.new_name = v;
            ask(app, CAsk::Lang, "math");
        }
        CAsk::Lang => {
            let Some(lang) = Lang::parse(&v) else {
                err(app, format!("unknown language `{v}`"));
                return ask(app, CAsk::Lang, v);
            };
            let name = std::mem::take(&mut app.code.new_name);
            match code::create(&app.store, "tui", &name, "", lang, code::template(lang), CodeInput::default()) {
                Ok(c) => {
                    app.refresh_history(true);
                    reload(app);
                    app.code.sel = app.code.list.iter().position(|x| x.id == c.id).unwrap_or(0);
                    edit_selected(app);
                    app.code.editing = true;
                    ok(app, format!("created {} from the {} template · Esc for the window's keys", c.name, lang.as_str()));
                }
                Err(e) => err(app, e),
            }
        }
        CAsk::Input | CAsk::Rename => {
            let Some(c) = app.code.space.clone().or_else(|| app.code.selected().cloned()) else { return };
            let ch = if kind == CAsk::Input {
                match code::parse_input(&v) {
                    Ok(i) => CodeChange { input: Some(i), ..Default::default() },
                    Err(e) => {
                        err(app, e);
                        return ask(app, kind, v);
                    }
                }
            } else {
                CodeChange { name: Some(v.clone()), ..Default::default() }
            };
            match code::edit(&app.store, "tui", &c.id, &ch) {
                Ok(n) => {
                    if app.code.space.is_some() {
                        app.code.space = Some(n.clone());
                    }
                    app.refresh_history(true);
                    reload(app);
                    ok(app, if kind == CAsk::Input { format!("input: {}", n.input.text()) } else { format!("renamed to {}", n.name) });
                }
                Err(e) => {
                    err(app, e);
                    ask(app, kind, v);
                }
            }
        }
    }
}

fn confirmed(app: &mut App, c: Confirm) {
    match c {
        Confirm::Delete => {
            let Some(s) = app.code.selected().cloned() else { return };
            match code::remove(&app.store, "tui", &s.id) {
                Ok(_) => {
                    app.refresh_history(true);
                    reload(app);
                    ok(app, format!("removed code space {}", s.name));
                }
                Err(e) => err(app, e),
            }
        }
        Confirm::ApproveRun => {
            let Some(s) = app.code.space.clone() else { return };
            match code::approve(&app.store, "tui", &s) {
                Ok(_) => {
                    app.code.space = app.store.code_space(&s.id).ok().flatten();
                    run(app);
                }
                Err(e) => err(app, e),
            }
        }
        Confirm::Discard => {
            app.code.screen = Screen::List;
            app.code.space = None;
            reload(app);
            ok(app, "changes discarded");
        }
    }
}

fn back_to_list(app: &mut App) {
    if app.code.dirty() {
        app.code.confirm = Some(Confirm::Discard);
        return ok(app, "leave without saving? y discards · s saves first");
    }
    app.code.screen = Screen::List;
    app.code.space = None;
    reload(app);
}

pub fn on_key(app: &mut App, k: KeyEvent) {
    if let Some(a) = app.code.ask.as_mut() {
        match ask_key(a, k) {
            Typed::Cancel => {
                app.code.ask = None;
                ok(app, "cancelled");
            }
            Typed::Done(v) => {
                let kind = a.kind;
                app.code.ask = None;
                app.code.msg = None;
                answer(app, kind, v);
            }
            Typed::Editing => {}
        }
        return;
    }
    if let Some(c) = app.code.confirm.take() {
        match k.code {
            KeyCode::Char('y' | 'Y') => confirmed(app, c),
            KeyCode::Char('s') if c == Confirm::Discard => {
                if save(app) {
                    back_to_list(app);
                }
            }
            _ => ok(app, "cancelled"),
        }
        return;
    }
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    match app.code.screen {
        Screen::List => list_key(app, k),
        Screen::Edit if app.code.editing => match k.code {
            KeyCode::Esc => {
                app.code.editing = false;
                app.code.msg = None;
            }
            KeyCode::Char('s') if ctrl => {
                save(app);
            }
            KeyCode::Char('r') if ctrl => run(app),
            _ => {
                app.code.editor.key(k, 10);
            }
        },
        Screen::Edit => {
            app.code.msg = None;
            match k.code {
                KeyCode::Enter | KeyCode::Char('e') => app.code.editing = true,
                KeyCode::Char('s') => {
                    save(app);
                }
                KeyCode::Char('b') => start_build(app, false),
                KeyCode::Char('r') => run(app),
                KeyCode::Char('t') => app.code.wide = !app.code.wide,
                KeyCode::Char('i') => {
                    let v = app.code.space.as_ref().map(|c| c.input.text()).unwrap_or_default();
                    ask(app, CAsk::Input, if v == "none" { String::new() } else { v });
                }
                KeyCode::Char('c') => {
                    let lines: Vec<String> = match (app.code.space.as_ref(), env(app)) {
                        (Some(c), Ok(e)) => code::check(&e, c.lang, &app.code.editor.text()).unwrap_or_else(|e| vec![format!("error: {e}")]),
                        (_, Err(e)) => vec![e.to_string()],
                        _ => vec![],
                    };
                    app.code.log = lines;
                    app.code.wide = false;
                }
                KeyCode::Char('p') => {
                    if let Some(c) = app.code.space.clone() {
                        pick_project(app, Purpose::Add(ItemKind::Code, c.id.clone(), serde_json::json!({}), c.name.clone()), Back::Code);
                    }
                }
                KeyCode::Up | KeyCode::Down | KeyCode::PageUp | KeyCode::PageDown | KeyCode::Left | KeyCode::Right | KeyCode::Home | KeyCode::End => {
                    app.code.editor.key(k, 10);
                }
                KeyCode::Esc | KeyCode::Char('q') => back_to_list(app),
                _ => {}
            }
        }
    }
}

fn list_key(app: &mut App, k: KeyEvent) {
    let st = &mut app.code;
    let n = st.list.len();
    st.msg = None;
    match k.code {
        KeyCode::Down | KeyCode::Char('j') => st.sel = (st.sel + 1).min(n.saturating_sub(1)),
        KeyCode::Up | KeyCode::Char('k') => st.sel = st.sel.saturating_sub(1),
        KeyCode::Enter | KeyCode::Char('e') => edit_selected(app),
        KeyCode::Char('n') | KeyCode::Char('a') => ask(app, CAsk::Name, ""),
        KeyCode::Char('m') => {
            if let Some(c) = st.selected().cloned() {
                app.code.space = None;
                ask(app, CAsk::Rename, c.name);
            }
        }
        KeyCode::Char('r') => {
            edit_selected(app);
            run(app);
        }
        KeyCode::Char('d') | KeyCode::Delete => {
            if let Some(c) = st.selected() {
                st.msg = Some((format!("delete code space {}? (its source stays in the history) y confirms", c.name), false));
                st.confirm = Some(Confirm::Delete);
            }
        }
        KeyCode::Char('p') => {
            if let Some(c) = st.selected().cloned() {
                pick_project(app, Purpose::Add(ItemKind::Code, c.id.clone(), serde_json::json!({}), c.name.clone()), Back::Code);
            }
        }
        KeyCode::Esc | KeyCode::Char('q') => app.modal = None,
        _ => {}
    }
}

pub fn on_click(app: &mut App, i: usize) {
    if app.code.ask.is_some() || app.code.screen != Screen::List {
        return;
    }
    if app.code.sel == i {
        edit_selected(app);
    } else if i < app.code.list.len() {
        app.code.sel = i;
    }
}

fn last_run(c: &CodeSpace) -> String {
    match &c.last_run {
        None => "never run".into(),
        Some(r) => format!("{} {}", r.status, fmt_ts(r.at)),
    }
}

pub fn draw(app: &mut App, f: &mut Frame, r: Rect, hits: &mut Vec<(Rect, Hit)>) {
    match app.code.screen {
        Screen::List => draw_list(app, f, r, hits),
        Screen::Edit => draw_edit(app, f, r, hits),
    }
}

fn draw_list(app: &mut App, f: &mut Frame, r: Rect, hits: &mut Vec<(Rect, Hit)>) {
    let t = app.theme.clone();
    let st = &app.code;
    let mut v = View::new("Code spaces · math, Polars, Rust, Python → native plugins");
    let w = st.list.iter().map(|x| x.name.chars().count()).max().unwrap_or(4).min(28);
    for (i, c) in st.list.iter().enumerate() {
        let ok = code::approved(c);
        v.rows.push(Line::from(vec![
            Span::styled(format!(" {} ", if ok { "●" } else { "○" }), Style::new().fg(if ok { t.accent } else { t.muted })),
            Span::styled(format!("{:<w$} ", c.name), row_style(&t, i == st.sel)),
            Span::styled(format!(" {:<7}", c.lang.as_str()), Style::new().fg(t.accent)),
            Span::styled(format!(" {} · {} · {}", c.input.text(), status_of(c), last_run(c)), Style::new().fg(t.muted)),
        ]));
    }
    if st.list.is_empty() {
        v.head.push(muted(&t, " no code spaces yet: n creates one (try math: sma20 = sma(close, 20))"));
        v.head.push(muted(&t, " code runs as native code with your rights; each new or changed source asks before its first run"));
    }
    v.sel = (!st.list.is_empty()).then_some(st.sel);
    v.ask = st.ask.as_ref().map(|a| (a.title.clone(), a.value.clone()));
    v.msg = st.msg.clone();
    v.keys = vec![
        ("Enter edit", KeyCode::Enter),
        ("n new", KeyCode::Char('n')),
        ("r run", KeyCode::Char('r')),
        ("m rename", KeyCode::Char('m')),
        ("d delete", KeyCode::Char('d')),
        ("p project", KeyCode::Char('p')),
        ("Esc", KeyCode::Esc),
    ];
    draw_view(f, r, v, &t, hits);
}

/// The result tile's lines: the last run's error or its view (as text).
fn result_lines(st: &CodeState, rows: usize) -> Vec<String> {
    match &st.result {
        None => vec!["not run yet: r runs it".into()],
        Some(i) => {
            let mut v = vec![format!(
                "{} · {} · {} ms{}",
                i.status,
                fmt_ts(i.at),
                i.ms,
                if i.writes > 0 { format!(" · {} writes", i.writes) } else { String::new() }
            )];
            if let Some(e) = &i.error {
                v.extend(e.lines().map(|l| format!("error: {l}")));
            }
            if let Some(view) = code::run_view(i) {
                v.extend(view.text_lines(rows));
            }
            v
        }
    }
}

fn draw_edit(app: &mut App, f: &mut Frame, r: Rect, hits: &mut Vec<(Rect, Hit)>) {
    let t = app.theme.clone();
    let Some(c) = app.code.space.clone() else { return };
    let dirty = app.code.dirty();
    let title = format!("Code space {} · {}{}", c.name, c.lang.as_str(), if dirty { " · modified" } else { "" });
    f.render_widget(Paragraph::new("").block(block(&title, true, &t)), r);
    let inner = Rect { x: r.x + 1, y: r.y + 1, width: r.width.saturating_sub(2), height: r.height.saturating_sub(2) };
    if inner.height < 6 || inner.width < 20 {
        return;
    }
    let head = Line::from(vec![
        Span::styled(format!(" input: {}", c.input.text()), Style::new().fg(t.fg)),
        Span::styled(format!(" · {}", status_of(&c)), Style::new().fg(if code::approved(&c) { t.accent } else { t.warning })),
        Span::styled(
            if app.code.building.is_some() { format!(" · {} building", super::widgets::spinner_frame()) } else { String::new() },
            Style::new().fg(t.accent),
        ),
    ]);
    f.render_widget(Paragraph::new(head), Rect { height: 1, ..inner });
    let body = Rect { y: inner.y + 1, height: inner.height.saturating_sub(3), ..inner };
    let (ed, side) = if app.code.wide {
        (Rect { width: 0, ..body }, body)
    } else {
        let w = body.width * 3 / 5;
        (Rect { width: w, ..body }, Rect { x: body.x + w, width: body.width - w, ..body })
    };
    // the editor
    if ed.width > 0 {
        let editing = app.code.editing;
        let eb = block(if editing { "Source · editing (Esc: keys)" } else { "Source · e edits" }, editing, &t);
        let ein = eb.inner(ed);
        f.render_widget(eb, ed);
        let e = &mut app.code.editor;
        e.scroll_to(ein.height as usize);
        let gutter = format!("{}", e.lines.len()).len().max(2);
        let lines: Vec<Line> = e
            .lines
            .iter()
            .enumerate()
            .skip(e.top)
            .take(ein.height as usize)
            .map(|(i, l)| {
                let mut spans = vec![Span::styled(format!("{:>gutter$} ", i + 1), Style::new().fg(t.muted))];
                if editing && i == e.row {
                    let at = byte_at(l, e.col);
                    let (a, b) = l.split_at(at);
                    let mut rest = b.chars();
                    let cur = rest.next().map(String::from).unwrap_or_else(|| " ".into());
                    spans.push(Span::styled(a.to_string(), Style::new().fg(t.fg)));
                    spans.push(Span::styled(cur, Style::new().bg(t.selection_bg).fg(t.selection_fg)));
                    spans.push(Span::styled(rest.collect::<String>(), Style::new().fg(t.fg)));
                } else {
                    spans.push(Span::styled(l.clone(), Style::new().fg(t.fg)));
                }
                Line::from(spans)
            })
            .collect();
        f.render_widget(Paragraph::new(lines), ein);
    }
    // the result tile over the build output
    let rh = if app.code.wide { side.height.saturating_sub(6) } else { side.height * 3 / 5 };
    let rt = Rect { height: rh, ..side };
    let lt = Rect { y: side.y + rh, height: side.height - rh, ..side };
    let rb = block("Result · t widens", app.code.wide, &t);
    let rin = rb.inner(rt);
    f.render_widget(rb, rt);
    let res: Vec<Line> = result_lines(&app.code, rin.height.saturating_sub(4).max(3) as usize)
        .into_iter()
        .map(|l| {
            let st = if l.starts_with("error") {
                Style::new().fg(t.error)
            } else if l.starts_with("==") {
                Style::new().fg(t.header).add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(t.fg)
            };
            Line::styled(l, st)
        })
        .collect();
    f.render_widget(Paragraph::new(res), rin);
    let lb = block("Build output", false, &t);
    let lin = lb.inner(lt);
    f.render_widget(lb, lt);
    let skip = app.code.log.len().saturating_sub(lin.height as usize);
    let log: Vec<Line> = app.code.log.iter().skip(skip).map(|l| Line::styled(l.clone(), Style::new().fg(t.muted))).collect();
    f.render_widget(Paragraph::new(log).wrap(Wrap { trim: false }), lin);
    // prompt or message, then keys
    let line_y = inner.bottom().saturating_sub(2);
    if let Some(a) = &app.code.ask {
        let l = Line::from(vec![
            Span::styled(format!(" {}: ", a.title), Style::new().fg(t.accent).add_modifier(Modifier::BOLD)),
            Span::styled(a.value.clone(), Style::new().bg(t.selection_bg).fg(t.selection_fg)),
            Span::styled("▏", Style::new().fg(t.accent)),
        ]);
        f.render_widget(Paragraph::new(l), Rect { y: line_y, height: 1, ..inner });
    } else if let Some((m, e)) = &app.code.msg {
        f.render_widget(
            Paragraph::new(Line::styled(format!(" {m}"), Style::new().fg(if *e { t.error } else { t.warning }))),
            Rect { y: line_y, height: 1, ..inner },
        );
    }
    let keys: Vec<(&str, KeyCode)> = if app.code.editing {
        vec![("Esc keys", KeyCode::Esc)]
    } else {
        vec![
            ("e edit", KeyCode::Char('e')),
            ("s save", KeyCode::Char('s')),
            ("b build", KeyCode::Char('b')),
            ("r run", KeyCode::Char('r')),
            ("c check", KeyCode::Char('c')),
            ("i input", KeyCode::Char('i')),
            ("t result", KeyCode::Char('t')),
            ("p project", KeyCode::Char('p')),
            ("Esc back", KeyCode::Esc),
        ]
    };
    let bar = Rect { x: inner.x + 1, y: inner.bottom().saturating_sub(1), width: inner.width.saturating_sub(2), height: 1 };
    if app.code.editing {
        let hint = Line::styled(" Ctrl+S save · Ctrl+R run · Esc for the window's keys", Style::new().fg(t.muted));
        let w = super::widgets::key_bar(f, bar, &keys, &t, hits);
        f.render_widget(Paragraph::new(hint), Rect { x: bar.x + w, width: bar.width.saturating_sub(w), ..bar });
    } else {
        super::widgets::key_bar(f, bar, &keys, &t, hits);
    }
}
