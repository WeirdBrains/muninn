//! Projects ▸ Rules… (R): rules that act when something happens. Thin over [`crate::rules`].
use super::workspace::{Ask, Back, Purpose, Typed, View, ask_key, draw_view, muted, pick_project, record, row_style};
use super::{App, Hit};
use crate::rules;
use crate::store::{ItemKind, NewRule, OwnerKind, Rule, RuleChange, RuleSpec};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

/// The steps of the new/edit prompt, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RAsk {
    Name,
    On,
    If,
    Then,
    Home,
}

impl RAsk {
    fn title(self) -> &'static str {
        match self {
            RAsk::Name => "Rule name",
            RAsk::On => "On (action kinds: import, journal.*, *; or every 6h / cron \"0 3 * * *\")",
            RAsk::If => "If (FIELD OP VALUE; …, empty: always)",
            RAsk::Then => "Then (status …; search …; export-report … to=…; entry …)",
            RAsk::Home => "Lives in (project:NAME, journal:NAME, search:NAME, model:NAME or empty)",
        }
    }
    fn next(self) -> Option<RAsk> {
        match self {
            RAsk::Name => Some(RAsk::On),
            RAsk::On => Some(RAsk::If),
            RAsk::If => Some(RAsk::Then),
            RAsk::Then => Some(RAsk::Home),
            RAsk::Home => None,
        }
    }
}

/// What the prompts collect; `editing` is the rule being changed.
#[derive(Debug, Clone, Default)]
pub struct Draft {
    pub editing: Option<String>,
    pub name: String,
    pub on: String,
    pub when: String,
    pub then: String,
    pub home: String,
}

impl Draft {
    fn get(&self, s: RAsk) -> String {
        match s {
            RAsk::Name => &self.name,
            RAsk::On => &self.on,
            RAsk::If => &self.when,
            RAsk::Then => &self.then,
            RAsk::Home => &self.home,
        }
        .clone()
    }
    fn set(&mut self, s: RAsk, v: String) {
        *match s {
            RAsk::Name => &mut self.name,
            RAsk::On => &mut self.on,
            RAsk::If => &mut self.when,
            RAsk::Then => &mut self.then,
            RAsk::Home => &mut self.home,
        } = v;
    }
}

#[derive(Debug, Default)]
pub struct RulesState {
    pub list: Vec<Rule>,
    pub sel: usize,
    pub ask: Option<Ask<RAsk>>,
    pub draft: Option<Draft>,
    pub confirm_delete: bool,
    pub msg: Option<(String, bool)>,
    /// Lines shown above the list: the selected rule, a test or a dry run.
    pub detail: Vec<String>,
    /// Newest `rule.run` already shown on the status line.
    pub seen: Option<i64>,
    /// When due scheduled rules were last run from here.
    pub last_tick: Option<std::time::Instant>,
}

impl RulesState {
    pub fn typing(&self) -> bool {
        self.ask.is_some()
    }
    fn selected(&self) -> Option<&Rule> {
        self.list.get(self.sel)
    }
}

fn params(r: &Rule) -> serde_json::Value {
    serde_json::json!({ "rule": r.id, "name": r.name })
}

pub fn open(app: &mut App) {
    let st = &mut app.rules;
    st.ask = None;
    st.draft = None;
    st.confirm_delete = false;
    st.msg = None;
    st.detail.clear();
    reopen(app);
}

/// Show the popup again (after the project picker), keeping the selection.
pub fn reopen(app: &mut App) {
    reload(app);
    app.modal = Some(super::modal::Modal::Rules);
}

/// Open the window on one rule (a rule item in a project).
pub fn open_rule(app: &mut App, id: &str) {
    open(app);
    if let Some(i) = app.rules.list.iter().position(|r| r.id == id) {
        app.rules.sel = i;
        show(app);
    } else {
        app.rules.msg = Some(("that rule no longer exists".into(), true));
    }
}

fn reload(app: &mut App) {
    app.rules.list = app.store.rules().unwrap_or_default();
    app.rules.sel = app.rules.sel.min(app.rules.list.len().saturating_sub(1));
}

fn select_id(app: &mut App, id: &str) {
    app.rules.sel = app.rules.list.iter().position(|r| r.id == id).unwrap_or(0);
}

/// Put messages of rule runs (from this and other connections, e.g. imports) on the status line.
/// Run due scheduled rules about once a minute, unless an org host serves from this
/// node (it runs them; this keeps one runner).
fn tick_schedules(app: &mut App) {
    if app.rules.last_tick.is_some_and(|t| t.elapsed() < std::time::Duration::from_secs(60)) {
        return;
    }
    app.rules.last_tick = Some(std::time::Instant::now());
    if app.orgs.host.is_some() || crate::org::host::running(&app.paths.home).is_some() {
        return;
    }
    app.store.set_host_running(false);
    if let Err(e) = rules::tick(&app.store, crate::store::now_ms()) {
        tracing::warn!("scheduled rules: {e}");
    }
}

pub fn poll(app: &mut App) {
    tick_schedules(app);
    let Some(seen) = app.rules.seen else {
        app.rules.seen = Some(app.store.history_stamp().map(|s| s.1).unwrap_or(0));
        return;
    };
    let Ok(runs) = app.store.actions_of_kind_after("rule.run", seen) else { return };
    let Some(last) = runs.last() else { return };
    app.rules.seen = Some(last.id);
    let msgs: Vec<String> = runs
        .iter()
        .filter_map(|a| {
            let name = a.params["name"].as_str().unwrap_or("rule");
            let m: Vec<&str> = a.params["messages"].as_array().map(|v| v.iter().filter_map(|m| m.as_str()).collect()).unwrap_or_default();
            (!m.is_empty()).then(|| format!("rule {name}: {}", m.join("; ")))
        })
        .collect();
    if !msgs.is_empty() {
        app.set_status(msgs.join(" · "));
        app.refresh_history(true);
        if matches!(app.modal, Some(super::modal::Modal::Rules)) {
            reload(app);
        }
    }
}

fn show(app: &mut App) {
    let Some(r) = app.rules.selected().cloned() else { return };
    let s = &app.store;
    let mut d = vec![format!("{} · {} · lives in {}", r.name, if r.enabled { "on" } else { "off" }, rules::home(s, &r))];
    if !r.description.is_empty() {
        d.push(r.description.clone());
    }
    d.push(format!("on {}{}", rules::on_text(&r.spec), if r.spec.related { " (related only)" } else { "" }));
    if r.spec.schedule.is_some()
        && let Some(n) = rules::next_run(&r)
    {
        d.push(format!("next run {}", crate::ingest::time::fmt_ts(n.max(crate::store::now_ms()))));
    }
    d.extend(r.spec.when.iter().map(|c| format!("if {}", rules::condition_text(c))));
    d.extend(r.spec.then.iter().map(|t| format!("then {}", rules::then_text(s, t))));
    let runs = s.rule_runs(&r.id, 3).unwrap_or_default();
    d.push(format!("{} run{}{}", r.runs, if r.runs == 1 { "" } else { "s" }, if runs.is_empty() { "" } else { ", latest:" }));
    d.extend(runs.iter().map(|a| format!("  #{} {}", a.id, a.summary)));
    app.rules.detail = d;
}

/// t: test, x: dry run, against the newest action.
fn try_rule(app: &mut App, dry: bool) {
    let Some(r) = app.rules.selected().cloned() else { return };
    let s = &app.store;
    let a = if r.spec.schedule.is_some() { Some(rules::schedule_action(&r)) } else { rules::last_trigger(s).ok().flatten() };
    let Some(a) = a else {
        app.rules.msg = Some(("nothing in the history yet".into(), true));
        return;
    };
    let c = rules::check(s, &r, &a);
    let mut d = vec![format!("{} vs #{} {} ({}):", r.name, a.id, a.kind, a.summary)];
    d.push(match &c.on {
        Some(p) => format!("  ✓ on {p}"),
        None => format!("  ✗ on {}", r.spec.on.join(", ")),
    });
    if let Some(rel) = c.related {
        d.push(format!("  {} related to {}", if rel { "✓" } else { "✗" }, rules::home(s, &r)));
    }
    for k in &c.conditions {
        let actual = k.actual.as_ref().map(|v| v.to_string()).unwrap_or_else(|| "missing".into());
        d.push(format!("  {} {}   ({actual})", if k.ok { "✓" } else { "✗" }, k.condition));
    }
    d.push(format!("  → {}", if c.matched { "matches" } else { "no match" }));
    if dry && c.matched {
        for t in &r.spec.then {
            d.push(match rules::plan(s, &r, &a, t) {
                Ok(m) => format!("  would {m}"),
                Err(e) => format!("  ! {} would fail: {e}", t.verb()),
            });
        }
    }
    app.rules.detail = d;
}

fn ask(app: &mut App, step: RAsk) {
    let v = app.rules.draft.as_ref().map(|d| d.get(step)).unwrap_or_default();
    app.rules.ask = Some(Ask::new(step, step.title(), v));
}

fn start(app: &mut App, edit: bool) {
    let d = match (edit, app.rules.selected()) {
        (true, Some(r)) => {
            let s = &app.store;
            let home = match (r.owner_kind, &r.owner_id) {
                (OwnerKind::Journal, Some(i)) => format!("journal:{}", s.journal(i).ok().flatten().map(|j| j.name).unwrap_or_default()),
                (OwnerKind::LiveSearch, Some(i)) => format!("search:{}", s.live_search(i).ok().flatten().map(|l| l.name).unwrap_or_default()),
                (OwnerKind::Model, Some(i)) => format!("model:{}", s.model(i).ok().flatten().map(|m| m.name).unwrap_or_default()),
                _ => String::new(),
            };
            Draft {
                editing: Some(r.id.clone()),
                name: r.name.clone(),
                on: rules::on_text(&r.spec),
                when: r.spec.when.iter().map(rules::condition_text).collect::<Vec<_>>().join("; "),
                then: r.spec.then.iter().map(|t| rules::then_text(s, t)).collect::<Vec<_>>().join("; "),
                home,
            }
        }
        (true, None) => return,
        (false, _) => Draft::default(),
    };
    app.rules.draft = Some(d);
    app.rules.detail.clear();
    ask(app, RAsk::Name);
}

/// Where `project:P`, `journal:J`, `search:S` or empty points.
/// Where a rule lives: an owner change (if any) and a project to join (if any).
type Home = (Option<(OwnerKind, Option<String>)>, Option<crate::store::Project>);

fn home_of(app: &App, h: &str) -> crate::Result<Home> {
    let h = h.trim();
    if h.is_empty() {
        return Ok((Some((OwnerKind::Standalone, None)), None));
    }
    let s = &app.store;
    match h.split_once(':').map(|(k, v)| (k.trim(), v.trim())) {
        Some(("project", n)) => Ok((None, Some(s.find_project(n)?))),
        Some(("journal", n)) => Ok((Some((OwnerKind::Journal, Some(s.find_journal(n)?.id))), None)),
        Some(("search", n)) => Ok((Some((OwnerKind::LiveSearch, Some(s.find_live_search(n)?.id))), None)),
        Some(("model", n)) => Ok((Some((OwnerKind::Model, Some(s.find_model(n)?.id))), None)),
        _ => Err(crate::Error::invalid("lives in", format!("`{h}`: project:NAME, journal:NAME, search:NAME, model:NAME or empty"))),
    }
}

/// Check one step; an error keeps the prompt open on it.
fn check_step(app: &App, step: RAsk, v: &str) -> crate::Result<()> {
    match step {
        RAsk::Name if v.trim().is_empty() => Err(crate::Error::invalid("rule name", "must not be empty")),
        RAsk::On => rules::parse_on_text(v).and_then(|(o, sch)| {
            if o.is_empty() && sch.is_none() { Err(crate::Error::invalid("on", "give at least one action kind or a schedule")) } else { Ok(()) }
        }),
        RAsk::If => rules::split_list(v).iter().try_for_each(|c| rules::parse_condition(c).map(|_| ())),
        RAsk::Then => {
            let t = rules::split_list(v);
            if t.is_empty() {
                return Err(crate::Error::invalid("then", "give at least one action"));
            }
            t.iter().try_for_each(|x| rules::parse_then(&app.store, x).map(|_| ()))
        }
        RAsk::Home => home_of(app, v).map(|_| ()),
        _ => Ok(()),
    }
}

fn answer(app: &mut App, step: RAsk, v: String) {
    if let Err(e) = check_step(app, step, &v) {
        app.rules.msg = Some((e.to_string(), true));
        app.rules.ask = Some(Ask::new(step, step.title(), v));
        return;
    }
    let Some(d) = app.rules.draft.as_mut() else { return };
    d.set(step, v);
    match step.next() {
        Some(n) => ask(app, n),
        None => save(app),
    }
}

fn save(app: &mut App) {
    let Some(d) = app.rules.draft.take() else { return };
    let res = (|| -> crate::Result<(Rule, &'static str, Option<crate::store::Project>)> {
        let s = &app.store;
        let (on, schedule) = rules::parse_on_text(&d.on)?;
        let when = rules::split_list(&d.when).iter().map(|c| rules::parse_condition(c)).collect::<crate::Result<Vec<_>>>()?;
        let then = rules::split_list(&d.then).iter().map(|t| rules::parse_then(s, t)).collect::<crate::Result<Vec<_>>>()?;
        let (owner, project) = home_of(app, &d.home)?;
        Ok(match &d.editing {
            Some(id) => {
                let old = s.rule(id)?.ok_or_else(|| crate::Error::invalid("rule", "no longer exists"))?;
                let spec = RuleSpec { on, when, then, schedule, ..old.spec };
                let r = s.update_rule(id, &RuleChange { name: Some(d.name.clone()), description: None, owner, spec: Some(spec) })?;
                (r, "rule.edit", project)
            }
            None => {
                let (kind, oid) = owner.unwrap_or((OwnerKind::Standalone, None));
                let spec = RuleSpec { on, when, then, schedule, ..Default::default() };
                let r = s.create_rule(&NewRule {
                    name: d.name.clone(),
                    description: String::new(),
                    owner_kind: kind,
                    owner_id: oid,
                    spec,
                    enabled: true,
                })?;
                (r, "rule.create", project)
            }
        })
    })();
    match res {
        Ok((r, kind, project)) => {
            let verb = if kind == "rule.create" { "create" } else { "edit" };
            record(app, kind, format!("{verb} rule {}", r.name), params(&r));
            let mut m = format!("{verb}d rule {}: {}", r.name, rules::brief(&r));
            if let Some(p) = project
                && let Ok((it, new)) = app.store.add_project_item(&p.id, ItemKind::RuleSet, &r.id, serde_json::json!({}))
            {
                if new {
                    record(
                        app,
                        "project.add",
                        format!("add rule {} to project {}", r.name, p.name),
                        serde_json::json!({ "project": p.id, "item": it.id, "kind": it.kind, "ref": it.ref_id }),
                    );
                }
                m.push_str(&format!(" (in project {})", p.name));
            }
            reload(app);
            select_id(app, &r.id);
            show(app);
            app.rules.msg = Some((m, false));
        }
        Err(e) => {
            app.rules.msg = Some((e.to_string(), true));
            app.rules.draft = Some(d);
            ask(app, RAsk::Name);
        }
    }
}

fn toggle(app: &mut App) {
    let Some(r) = app.rules.selected().cloned() else { return };
    match app.store.set_rule_enabled(&r.id, !r.enabled) {
        Ok(n) => {
            let (kind, verb) = if n.enabled { ("rule.enable", "on") } else { ("rule.disable", "off") };
            record(app, kind, format!("switch rule {} {verb}", n.name), params(&n));
            reload(app);
            app.rules.msg = Some((format!("rule {} is {verb}", n.name), false));
        }
        Err(e) => app.rules.msg = Some((e.to_string(), true)),
    }
}

pub fn on_key(app: &mut App, k: KeyEvent) {
    if let Some(a) = app.rules.ask.as_mut() {
        match ask_key(a, k) {
            Typed::Cancel => {
                app.rules.ask = None;
                app.rules.draft = None;
                app.rules.msg = Some(("cancelled".into(), false));
            }
            Typed::Done(v) => {
                let step = a.kind;
                app.rules.ask = None;
                app.rules.msg = None;
                answer(app, step, v);
            }
            Typed::Editing => {}
        }
        return;
    }
    let st = &mut app.rules;
    if st.confirm_delete {
        st.confirm_delete = false;
        if let (KeyCode::Char('y' | 'Y'), Some(r)) = (k.code, st.selected().cloned()) {
            match app.store.remove_rule(&r.id) {
                Ok(_) => {
                    record(app, "rule.remove", format!("remove rule {}", r.name), params(&r));
                    reload(app);
                    app.rules.detail.clear();
                    app.rules.msg = Some((format!("removed rule {}", r.name), false));
                }
                Err(e) => app.rules.msg = Some((e.to_string(), true)),
            }
        } else {
            st.msg = Some(("delete cancelled".into(), false));
        }
        return;
    }
    let n = st.list.len();
    st.msg = None;
    match k.code {
        KeyCode::Down | KeyCode::Char('j') => {
            st.sel = (st.sel + 1).min(n.saturating_sub(1));
            st.detail.clear();
        }
        KeyCode::Up | KeyCode::Char('k') => {
            st.sel = st.sel.saturating_sub(1);
            st.detail.clear();
        }
        KeyCode::Enter => show(app),
        KeyCode::Char('n') | KeyCode::Char('a') => start(app, false),
        KeyCode::Char('e') => start(app, true),
        KeyCode::Char(' ') => toggle(app),
        KeyCode::Char('t') => try_rule(app, false),
        KeyCode::Char('x') => try_rule(app, true),
        KeyCode::Char('d') | KeyCode::Delete => {
            if let Some(r) = st.selected() {
                st.msg = Some((format!("delete rule {}? y confirms, any other key cancels", r.name), false));
                st.confirm_delete = true;
            }
        }
        KeyCode::Char('p') => {
            if let Some(r) = st.selected().cloned() {
                pick_project(app, Purpose::Add(ItemKind::RuleSet, r.id.clone(), serde_json::json!({}), r.name.clone()), Back::Rules);
            }
        }
        KeyCode::Esc if !st.detail.is_empty() => st.detail.clear(),
        KeyCode::Esc | KeyCode::Char('q') => app.modal = None,
        _ => {}
    }
}

pub fn on_click(app: &mut App, i: usize) {
    if app.rules.ask.is_some() {
        return;
    }
    if app.rules.sel == i {
        on_key(app, KeyEvent::from(KeyCode::Enter));
    } else if i < app.rules.list.len() {
        app.rules.sel = i;
        app.rules.detail.clear();
    }
}

pub fn draw(app: &mut App, f: &mut Frame, r: Rect, hits: &mut Vec<(Rect, Hit)>) {
    let t = app.theme.clone();
    let st = &app.rules;
    let mut v = View::new("Rules · when this happens, do that");
    for d in &st.detail {
        v.head.push(Line::styled(format!(" {d}"), Style::new().fg(t.fg)));
    }
    if !st.detail.is_empty() {
        v.head.push(Line::raw(""));
    }
    let w = st.list.iter().map(|x| x.name.chars().count()).max().unwrap_or(4).min(28);
    for (i, x) in st.list.iter().enumerate() {
        let on = i == st.sel;
        v.rows.push(Line::from(vec![
            Span::styled(format!(" {} ", if x.enabled { "●" } else { "○" }), Style::new().fg(if x.enabled { t.accent } else { t.muted })),
            Span::styled(format!("{:<w$} ", x.name), row_style(&t, on)),
            Span::styled(format!(" on {} → {} ", rules::on_text(&x.spec), x.spec.then.len()), Style::new().fg(t.accent)),
            Span::styled(format!(" {} · {} run{}", rules::home(&app.store, x), x.runs, if x.runs == 1 { "" } else { "s" }), Style::new().fg(t.muted)),
        ]));
    }
    if st.list.is_empty() && st.detail.is_empty() {
        v.head.push(muted(&t, " no rules yet: n creates one (on: import · then: status Imported {inserted} bars)"));
    }
    v.sel = (!st.list.is_empty()).then_some(st.sel);
    v.ask = st.ask.as_ref().map(|a| (a.title.clone(), a.value.clone()));
    v.msg = st.msg.clone();
    v.keys = vec![
        ("Enter show", KeyCode::Enter),
        ("n new", KeyCode::Char('n')),
        ("e edit", KeyCode::Char('e')),
        ("Space on/off", KeyCode::Char(' ')),
        ("t test", KeyCode::Char('t')),
        ("x dry run", KeyCode::Char('x')),
        ("d delete", KeyCode::Char('d')),
        ("p project", KeyCode::Char('p')),
        ("Esc", KeyCode::Esc),
    ];
    draw_view(f, r, v, &t, hits);
}
