//! Data ▸ Model designer… (D): design record tables, preview and apply their migrations,
//! roll them back, browse and change their records, and feed them with rules.
//! Thin over [`crate::store::models`], [`crate::store::records`] and [`crate::designer`].
use super::workspace::{Ask, Back, Purpose, Typed, View, ask_key, draw_view, muted, pick_project, record, row_style};
use super::{App, Hit, block};
use crate::designer::{self as dz, Act};
use crate::rules;
use crate::store::{
    ApplyOpts, Design, Field, Group, ItemKind, LiveSearch, Model, ModelVersion, NewRule, OwnerKind, Plan, Record, RecordQuery, Rule, RuleSpec,
};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use serde_json::{Map, Value};

/// Which screen of the window is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    #[default]
    List,
    Design,
    History,
    Records,
    Rules,
}

/// What a prompt is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MAsk {
    NewModel,
    NewFields,
    AddField,
    /// Edit the field with this id.
    EditField(String),
    ModelName,
    ModelAbout,
    Where,
    Text,
    Sort,
    Group,
    NewRecord,
    EditRecord(String),
    UpdateMany,
    RuleName,
    RuleOn,
    RuleIf,
    RuleThen,
}

impl MAsk {
    fn title(&self) -> &'static str {
        match self {
            MAsk::NewModel => "New model name",
            MAsk::NewFields => "Fields (name:type [required] [unique] [default=V]; …, e.g. symbol:text!; price:real)",
            MAsk::AddField => "New field (name:type [required] [unique] [index] [default=V] [choices=a|b] [about=\"…\"])",
            MAsk::EditField(_) => "Field (name:type and constraints; renaming keeps the values)",
            MAsk::ModelName => "Model name",
            MAsk::ModelAbout => "Description",
            MAsk::Where => "Filter (FIELD OP VALUE; …  ops: = != > >= < <= ~ !~, empty: none)",
            MAsk::Text => "Find text in any field (empty: none)",
            MAsk::Sort => "Sort by (field, -field for descending, comma-separated)",
            MAsk::Group => "Group by (fields, then sum:F avg:F min:F max:F; empty: no grouping)",
            MAsk::NewRecord => "New record (field=value …; quote values with spaces)",
            MAsk::EditRecord(_) => "Record (field=value …; an empty value clears it)",
            MAsk::UpdateMany => "Set on every matching record (field=value …)",
            MAsk::RuleName => "Rule name",
            MAsk::RuleOn => "On (action kinds: import, journal.*, record.create, *)",
            MAsk::RuleIf => "If (FIELD OP VALUE; …, empty: always)",
            MAsk::RuleThen => "Then (record-create f={field} …; record-update where=… f=v; status …)",
        }
    }
}

/// What a `y` confirms.
#[derive(Debug, Clone, PartialEq)]
pub enum Confirm {
    RemoveModel,
    /// Apply a migration that loses values.
    Apply,
    Rollback(i64),
    DeleteRecord(String),
    UpdateMany(Map<String, Value>),
    DeleteMany,
    DeleteRule(String),
}

#[derive(Debug, Default)]
pub struct ModelsState {
    pub screen: Screen,
    pub list: Vec<Model>,
    pub counts: Vec<usize>,
    pub sel: usize,
    /// The model open in the designer, history, records or rules screen.
    pub model: Option<Model>,
    /// Selected row of the field list.
    pub fsel: usize,
    /// What applying the draft would do (or why it cannot be planned).
    pub plan: Option<std::result::Result<Plan, String>>,
    pub versions: Vec<ModelVersion>,
    pub vsel: usize,
    pub query: RecordQuery,
    pub group: Vec<String>,
    pub aggs: Vec<String>,
    pub records: Vec<Record>,
    pub groups: Vec<Group>,
    pub rsel: usize,
    /// Records matching the filter (the list may be limited).
    pub total: usize,
    pub rules: Vec<Rule>,
    pub usel: usize,
    /// A rule being made: name, on, if.
    pub rule_draft: Option<(String, String, String)>,
    pub new_name: String,
    pub ask: Option<Ask<MAsk>>,
    pub confirm: Option<Confirm>,
    pub msg: Option<(String, bool)>,
    /// Lines shown above a list: a record, a version, a dry run.
    pub detail: Vec<String>,
    pub from_search: Option<String>,
}

impl ModelsState {
    pub fn typing(&self) -> bool {
        self.ask.is_some()
    }
}

/// Records listed at a time (the filter narrows big tables such as bars).
const PAGE: usize = 500;

fn note(app: &mut App, a: Act) {
    record(app, a.kind, a.summary, a.params);
}

fn err(app: &mut App, e: impl std::fmt::Display) {
    app.models.msg = Some((e.to_string(), true));
}

fn ok(app: &mut App, m: impl Into<String>) {
    app.models.msg = Some((m.into(), false));
}

pub fn open(app: &mut App) {
    let st = &mut app.models;
    st.screen = Screen::List;
    st.ask = None;
    st.confirm = None;
    st.msg = None;
    st.detail.clear();
    st.from_search = None;
    reopen(app);
}

/// Show the window again (after the project picker), on the same screen.
pub fn reopen(app: &mut App) {
    reload(app);
    app.modal = Some(super::modal::Modal::Models);
}

/// Open the designer on one model (a model item in a project).
pub fn open_model(app: &mut App, id: &str) {
    open(app);
    match app.models.list.iter().position(|m| m.id == id) {
        Some(i) => {
            app.models.sel = i;
            enter(app, Screen::Design);
        }
        None => err(app, "that model no longer exists"),
    }
}

/// Show what a `records` live search finds.
pub fn open_found(app: &mut App, s: &LiveSearch) {
    open(app);
    let model = s.spec.model.as_ref().and_then(|k| app.store.find_model(k).ok());
    let Some(m) = model.or_else(|| app.models.list.iter().find(|m| !m.is_bars()).cloned()) else {
        return err(app, format!("live search {}: there are no models to search", s.name));
    };
    if let Some(i) = app.models.list.iter().position(|x| x.id == m.id) {
        app.models.sel = i;
    }
    app.models.model = Some(m);
    app.models.query = RecordQuery { filter: s.spec.conditions.clone(), text: s.spec.query.clone(), ..Default::default() };
    app.models.group.clear();
    app.models.aggs.clear();
    app.models.from_search = Some(s.name.clone());
    app.models.screen = Screen::Records;
    reload(app);
    let n = app.models.total;
    app.set_status(format!("live search {}: {n} record{}", s.name, if n == 1 { "" } else { "s" }));
}

fn reload(app: &mut App) {
    let s = &app.store;
    let st = &mut app.models;
    st.list = s.models().unwrap_or_default();
    st.counts = st.list.iter().map(|m| s.count_records(m).unwrap_or(0)).collect();
    st.sel = st.sel.min(st.list.len().saturating_sub(1));
    if let Some(id) = st.model.as_ref().map(|m| m.id.clone()) {
        st.model = st.list.iter().find(|m| m.id == id).cloned();
        if st.model.is_none() {
            st.screen = Screen::List;
        }
    }
    let Some(m) = st.model.clone() else { return };
    match st.screen {
        Screen::List => {}
        Screen::Design => {
            st.plan = Some(s.plan_model(&m, m.pending()).map_err(|e| e.to_string()));
            st.fsel = st.fsel.min(design_rows(&m).len().saturating_sub(1));
        }
        Screen::History => {
            st.versions = s.model_versions(&m.id).unwrap_or_default();
            st.versions.reverse();
            st.vsel = st.vsel.min(st.versions.len().saturating_sub(1));
        }
        Screen::Records => {
            let all = RecordQuery { limit: None, offset: 0, ..st.query.clone() };
            let r = if st.group.is_empty() {
                st.groups.clear();
                s.records(&m, &RecordQuery { limit: Some(PAGE), ..st.query.clone() }).map(|r| {
                    st.records = r;
                })
            } else {
                st.records.clear();
                s.group_records(&m, &all, &st.group, &st.aggs).map(|g| st.groups = g)
            };
            if let Err(e) = r {
                st.msg = Some((e.to_string(), true));
            }
            st.total = if st.query.filter.is_empty() && st.query.text.is_empty() {
                s.count_records(&m).unwrap_or(0)
            } else if st.group.is_empty() && st.records.len() < PAGE {
                st.records.len()
            } else {
                s.records(&m, &all).map(|r| r.len()).unwrap_or(0)
            };
            let n = if st.group.is_empty() { st.records.len() } else { st.groups.len() };
            st.rsel = st.rsel.min(n.saturating_sub(1));
        }
        Screen::Rules => {
            st.rules = s.owned_rules(OwnerKind::Model, &m.id).unwrap_or_default();
            st.usel = st.usel.min(st.rules.len().saturating_sub(1));
        }
    }
}

/// Open a screen on the selected model.
fn enter(app: &mut App, to: Screen) {
    let Some(m) = app.models.list.get(app.models.sel).cloned() else { return };
    let st = &mut app.models;
    if st.model.as_ref().map(|x| &x.id) != Some(&m.id) {
        st.fsel = 0;
        st.vsel = 0;
        st.rsel = 0;
        st.usel = 0;
        st.query = RecordQuery::default();
        st.group.clear();
        st.aggs.clear();
    }
    st.model = Some(m);
    st.screen = to;
    st.detail.clear();
    st.confirm = None;
    reload(app);
}

/// A row of the design list: a field of the draft (with its change mark) or one it drops.
#[derive(Debug, Clone)]
struct Row {
    mark: char,
    field: Field,
    /// The applied field (absent: new).
    was: Option<Field>,
    dropped: bool,
}

fn design_rows(m: &Model) -> Vec<Row> {
    let mut rows: Vec<Row> = m
        .pending()
        .fields
        .iter()
        .map(|f| {
            let was = m.design.by_id(&f.id).cloned();
            let mark = match &was {
                None => '+',
                Some(w) if w != f => '~',
                Some(_) => ' ',
            };
            Row { mark, field: f.clone(), was, dropped: false }
        })
        .collect();
    for f in &m.design.fields {
        if m.pending().by_id(&f.id).is_none() {
            rows.push(Row { mark: '-', field: f.clone(), was: Some(f.clone()), dropped: true });
        }
    }
    rows
}

fn builtin_field(m: &Model, name: &str) -> bool {
    m.is_bars() && crate::store::records::field_names(&Model { design: Design::default(), draft: None, ..m.clone() }).iter().any(|f| f == name)
}

/// Save a new draft (recording what changed) and replan.
fn save_draft(app: &mut App, d: Design) {
    let Some(m) = app.models.model.clone() else { return };
    let ops = dz::diff_ops(&app.store, m.pending(), &d);
    if ops.is_empty() {
        return ok(app, "no change");
    }
    match app.store.set_model_draft(&m.id, Some(d)) {
        Ok(n) => {
            note(app, dz::act_model_edit(&n, &ops, None));
            reload(app);
            let what: Vec<String> = ops.iter().map(dz::op_text).collect();
            ok(app, format!("draft: {} · A applies", what.join(", ")));
        }
        Err(e) => err(app, e),
    }
}

fn ask(app: &mut App, k: MAsk, v: impl Into<String>) {
    let title = k.title();
    app.models.ask = Some(Ask::new(k, title, v));
}

fn record_line(m: &Model, r: &Record) -> String {
    let names = crate::store::records::field_names(m);
    names
        .iter()
        .filter_map(|n| r.values.get(n).map(|v| (n, v)))
        .filter(|(_, v)| !v.is_null())
        .map(|(n, v)| format!("{n}={}", rules::quote(&dz::value_text(v))))
        .collect::<Vec<_>>()
        .join(" ")
}

fn answer(app: &mut App, k: MAsk, v: String) {
    if let Err(e) = answer_step(app, k, v) {
        err(app, e);
    }
}

fn answer_step(app: &mut App, k: MAsk, v: String) -> crate::Result<()> {
    let m = app.models.model.clone();
    {
        match &k {
            MAsk::NewModel => {
                if v.trim().is_empty() {
                    return Err(crate::Error::invalid("model name", "must not be empty"));
                }
                if app.store.models()?.iter().any(|x| x.name == crate::store::models::slug(&v)) {
                    return Err(crate::Error::invalid("model name", format!("`{v}` already exists")));
                }
                app.models.new_name = v.clone();
                ask(app, MAsk::NewFields, "");
            }
            MAsk::NewFields => {
                let (m, _) = app.store.create_model(&app.models.new_name, "", Design::default(), false)?;
                let fields = match dz::parse_fields(&app.store, &m, &v).and_then(|f| app.store.set_model_draft(&m.id, Some(Design { fields: f }))) {
                    Ok(m) => m,
                    Err(e) => {
                        let _ = app.store.remove_model(&m.id, true);
                        app.models.ask = Some(Ask::new(MAsk::NewFields, MAsk::NewFields.title(), v.clone()));
                        return Err(e);
                    }
                };
                note(app, dz::act_model_create(&fields));
                app.models.model = None;
                reload(app);
                app.models.sel = app.models.list.iter().position(|x| x.id == fields.id).unwrap_or(0);
                enter(app, Screen::Design);
                ok(app, format!("created model {} · a adds fields · A applies it as v1", fields.name));
            }
            MAsk::AddField => {
                let m = m.ok_or_else(|| crate::Error::invalid("model", "none open"))?;
                let d = dz::apply_ops(&app.store, &m, m.pending(), &[crate::store::FieldOp::Add { spec: v.clone() }]);
                match d {
                    Ok(d) => {
                        save_draft(app, d);
                        app.models.fsel = app.models.model.as_ref().map(|m| m.pending().fields.len().saturating_sub(1)).unwrap_or(0);
                    }
                    Err(e) => {
                        app.models.ask = Some(Ask::new(MAsk::AddField, MAsk::AddField.title(), v.clone()));
                        return Err(e);
                    }
                }
            }
            MAsk::EditField(id) => {
                let m = m.ok_or_else(|| crate::Error::invalid("model", "none open"))?;
                let r = (|| {
                    let mut f = dz::parse_field(&app.store, &m, &v, None)?;
                    let mut d = m.pending().clone();
                    let i = d.fields.iter().position(|x| &x.id == id).ok_or_else(|| crate::Error::invalid("field", "no longer exists"))?;
                    f.id = id.clone();
                    d.fields[i] = f;
                    app.store.check_design(&m, &d)?;
                    Ok::<_, crate::Error>(d)
                })();
                match r {
                    Ok(d) => save_draft(app, d),
                    Err(e) => {
                        app.models.ask = Some(Ask::new(k.clone(), k.title(), v.clone()));
                        return Err(e);
                    }
                }
            }
            MAsk::ModelName => {
                let m = m.ok_or_else(|| crate::Error::invalid("model", "none open"))?;
                let n = app.store.rename_model(&m.id, Some(&v), None)?;
                if n.name != m.name {
                    note(app, dz::act_model_edit(&n, &[], Some(&m.name)));
                }
                reload(app);
                ok(app, format!("model {} renamed to {}", m.name, n.name));
            }
            MAsk::ModelAbout => {
                let m = m.ok_or_else(|| crate::Error::invalid("model", "none open"))?;
                let n = app.store.rename_model(&m.id, None, Some(&v))?;
                note(app, dz::act_model_edit(&n, &[], None));
                reload(app);
                ok(app, "description saved");
            }
            MAsk::Where => {
                app.models.query.filter = dz::parse_where(std::slice::from_ref(&v))?;
                app.models.rsel = 0;
                reload(app);
            }
            MAsk::Text => {
                app.models.query.text = v.clone();
                app.models.rsel = 0;
                reload(app);
            }
            MAsk::Sort => {
                app.models.query.sort = dz::parse_sort(std::slice::from_ref(&v));
                reload(app);
            }
            MAsk::Group => {
                let (aggs, by): (Vec<String>, Vec<String>) =
                    v.split([',', ' ']).map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).partition(|x| x.contains(':'));
                app.models.group = by;
                app.models.aggs = aggs;
                app.models.rsel = 0;
                reload(app);
            }
            MAsk::NewRecord | MAsk::EditRecord(_) | MAsk::UpdateMany => {
                let m = m.ok_or_else(|| crate::Error::invalid("model", "none open"))?;
                let toks = rules::tokenize(&v)?;
                let items: Vec<String> = toks.into_iter().map(|(k, x)| format!("{k}={}", x.unwrap_or_default())).collect();
                let vals = dz::parse_assignments(&items);
                let r = vals.and_then(|vals| -> crate::Result<()> {
                    match &k {
                        MAsk::NewRecord => {
                            let r = app.store.create_record(&m, &vals)?;
                            note(app, dz::act_record_create(&m, &r));
                            reload(app);
                            ok(app, format!("created {} record {}", m.name, r.short_id()));
                        }
                        MAsk::EditRecord(id) => {
                            let old = app.store.find_record(&m, id)?;
                            let changes: Map<String, Value> = vals.into_iter().filter(|(f, x)| old.values.get(f) != Some(x)).collect();
                            let r = app.store.update_record(&m, id, &changes)?;
                            note(app, dz::act_record_update(&m, Some(&r), 1, &[], &changes));
                            reload(app);
                            ok(app, format!("updated {} record {}", m.name, r.short_id()));
                        }
                        _ => {
                            let n = app.models.total;
                            app.models.msg =
                                Some((format!("set {} on {n} record{}? y confirms", record_line_map(&vals), if n == 1 { "" } else { "s" }), false));
                            app.models.confirm = Some(Confirm::UpdateMany(vals));
                        }
                    }
                    Ok(())
                });
                if let Err(e) = r {
                    app.models.ask = Some(Ask::new(k.clone(), k.title(), v.clone()));
                    return Err(e);
                }
            }
            MAsk::RuleName => {
                if v.is_empty() {
                    return Err(crate::Error::invalid("rule name", "must not be empty"));
                }
                app.models.rule_draft = Some((v.clone(), String::new(), String::new()));
                ask(app, MAsk::RuleOn, "");
            }
            MAsk::RuleOn => {
                if let Err(e) = rules::parse_on(std::slice::from_ref(&v))
                    .and_then(|o| if o.is_empty() { Err(crate::Error::invalid("on", "give at least one action kind")) } else { Ok(()) })
                {
                    ask(app, MAsk::RuleOn, v.clone());
                    return Err(e);
                }
                if let Some(d) = app.models.rule_draft.as_mut() {
                    d.1 = v.clone();
                }
                ask(app, MAsk::RuleIf, "");
            }
            MAsk::RuleIf => {
                if let Err(e) = rules::split_list(&v).iter().try_for_each(|c| rules::parse_condition(c).map(|_| ())) {
                    ask(app, MAsk::RuleIf, v.clone());
                    return Err(e);
                }
                if let Some(d) = app.models.rule_draft.as_mut() {
                    d.2 = v.clone();
                }
                let m = m.ok_or_else(|| crate::Error::invalid("model", "none open"))?;
                ask(app, MAsk::RuleThen, feed_template(&m));
            }
            MAsk::RuleThen => {
                let m = m.ok_or_else(|| crate::Error::invalid("model", "none open"))?;
                let Some((name, on, when)) = app.models.rule_draft.clone() else { return Ok(()) };
                let r = (|| {
                    let spec = RuleSpec {
                        on: rules::parse_on(&[on])?,
                        when: rules::split_list(&when).iter().map(|c| rules::parse_condition(c)).collect::<crate::Result<Vec<_>>>()?,
                        then: rules::split_list(&v).iter().map(|t| rules::parse_then(&app.store, t)).collect::<crate::Result<Vec<_>>>()?,
                        ..Default::default()
                    };
                    if spec.then.is_empty() {
                        return Err(crate::Error::invalid("then", "give at least one action"));
                    }
                    app.store.create_rule(&NewRule {
                        name,
                        description: String::new(),
                        owner_kind: OwnerKind::Model,
                        owner_id: Some(m.id.clone()),
                        spec,
                        enabled: true,
                    })
                })();
                match r {
                    Ok(r) => {
                        app.models.rule_draft = None;
                        record(app, "rule.create", format!("create rule {}", r.name), serde_json::json!({ "rule": r.id, "name": r.name }));
                        reload(app);
                        app.models.usel = app.models.rules.iter().position(|x| x.id == r.id).unwrap_or(0);
                        ok(app, format!("created rule {} in model {}: {} · x dry run · Enter run now", r.name, m.name, rules::brief(&r)));
                    }
                    Err(e) => {
                        ask(app, MAsk::RuleThen, v.clone());
                        return Err(e);
                    }
                }
            }
        }
    }
    Ok(())
}

fn record_line_map(v: &Map<String, Value>) -> String {
    v.iter().map(|(k, x)| format!("{k}={}", rules::quote(&dz::value_text(x)))).collect::<Vec<_>>().join(" ")
}

/// A then-action that copies a triggering action's fields into a new record.
pub fn feed_template(m: &Model) -> String {
    let fields: Vec<String> = m.design.fields.iter().map(|f| format!("{}={{{}}}", f.name, f.name)).collect();
    format!("record-create {} {}", rules::quote(&m.name), fields.join(" ")).trim().to_string()
}

/// Apply the draft (`destructive`: values may be lost, already confirmed).
fn apply(app: &mut App, destructive: bool) {
    let Some(m) = app.models.model.clone() else { return };
    let plan = match app.store.plan_model(&m, m.pending()) {
        Ok(p) => p,
        Err(e) => return err(app, e),
    };
    if !plan.blocked.is_empty() {
        return err(app, format!("cannot apply: {}", plan.blocked.join("; ")));
    }
    if m.draft.is_none() && m.version > 0 {
        return ok(app, format!("{} has no changes to apply (a adds a field, e edits one)", m.name));
    }
    if plan.destructive() && !destructive {
        app.models.confirm = Some(Confirm::Apply);
        return ok(
            app,
            format!("this loses {} value{}; y applies anyway, any other key cancels", plan.lost(), if plan.lost() == 1 { "" } else { "s" }),
        );
    }
    match app.store.apply_model(&m.id, &ApplyOpts { destructive, note: String::new() }) {
        Ok((n, p)) => {
            note(app, dz::act_model_apply(&app.store, &n, &p, false));
            reload(app);
            ok(app, format!("applied {} v{} → v{} ({} records migrated)", n.name, p.from, p.to, p.rows));
        }
        Err(e) => err(app, e),
    }
}

fn rollback(app: &mut App, to: i64, destructive: bool) {
    let Some(m) = app.models.model.clone() else { return };
    let plan = match app.store.rollback_plan(&m.id, to) {
        Ok(p) => p,
        Err(e) => return err(app, e),
    };
    if !destructive {
        app.models.confirm = Some(Confirm::Rollback(to));
        let lost = if plan.lost() > 0 { format!(", losing {} values", plan.lost()) } else { String::new() };
        return ok(app, format!("roll {} back to the design of v{to} as v{} ({} steps{lost})? y confirms", m.name, m.version + 1, plan.steps.len()));
    }
    match app.store.rollback_model(&m.id, to, &ApplyOpts { destructive: true, note: String::new() }) {
        Ok((n, p)) => {
            note(app, dz::act_model_apply(&app.store, &n, &p, true));
            reload(app);
            app.models.vsel = 0;
            ok(app, format!("{} is now v{} (the design of v{to})", n.name, n.version));
        }
        Err(e) => err(app, e),
    }
}

fn discard(app: &mut App) {
    let Some(m) = app.models.model.clone() else { return };
    if m.draft.is_none() {
        return ok(app, "there is no draft");
    }
    match app.store.set_model_draft(&m.id, None) {
        Ok(n) => {
            note(app, dz::act_model_discard(&n));
            reload(app);
            ok(app, format!("discarded the draft; {} is back to v{}", n.name, n.version));
        }
        Err(e) => err(app, e),
    }
}

fn selected_record(app: &App) -> Option<Record> {
    if !app.models.group.is_empty() {
        return None;
    }
    app.models.records.get(app.models.rsel).cloned()
}

fn show_record(app: &mut App) {
    let Some(m) = app.models.model.clone() else { return };
    if !app.models.group.is_empty() {
        let Some(g) = app.models.groups.get(app.models.rsel).cloned() else { return };
        let mut d = vec![format!("{} · {} record{}", group_key(&g), g.count, if g.count == 1 { "" } else { "s" })];
        d.extend(g.records.iter().take(8).map(|r| format!("  {} {}", r.short_id(), record_line(&m, r))));
        app.models.detail = d;
        return;
    }
    let Some(r) = selected_record(app) else { return };
    let mut d = vec![format!("record {} · v{} · by {}", r.id, r.version, crate::history::author_label(&app.store, r.author_id.as_deref()))];
    for n in crate::store::records::field_names(&m) {
        let v = r.values.get(&n).map(dz::value_text).unwrap_or_default();
        d.push(format!("  {n:<14} {v}"));
    }
    d.push(format!("  created {} · updated {}", crate::ingest::time::fmt_ts(r.created_at), crate::ingest::time::fmt_ts(r.updated_at)));
    app.models.detail = d;
}

fn group_key(g: &Group) -> String {
    g.key.iter().map(|(k, v)| format!("{k}={}", dz::value_text(v))).collect::<Vec<_>>().join(" ")
}

fn show_version(app: &mut App) {
    let Some(v) = app.models.versions.get(app.models.vsel).cloned() else { return };
    let s = &app.store;
    let mut d = vec![format!(
        "v{} · {} · by {}{}",
        v.version,
        crate::ingest::time::fmt_ts(v.created_at),
        crate::history::author_label(s, v.author_id.as_deref()),
        if v.note.is_empty() { String::new() } else { format!(" · {}", v.note) }
    )];
    d.extend(v.design.fields.iter().map(|f| format!("  {}", dz::field_text(s, f))));
    if !v.steps.is_empty() {
        d.push("  steps:".into());
        d.extend(v.steps.iter().map(|st| format!("    {} {}", dz::step_mark(st), dz::step_text(s, st))));
    }
    if let Some(m) = &app.models.model
        && v.version < m.version
        && let Ok(p) = s.rollback_plan(&m.id, v.version)
    {
        d.push(format!("  rolling back to it ({}):", if p.steps.is_empty() { "nothing changes" } else { "r" }));
        d.extend(dz::plan_lines(s, &p).into_iter().skip(1).map(|l| format!("  {l}")));
    }
    app.models.detail = d;
}

fn try_rule(app: &mut App, dry: bool) {
    let Some(r) = app.models.rules.get(app.models.usel).cloned() else { return };
    let s = &app.store;
    let Some(a) = rules::last_trigger(s).ok().flatten() else {
        return err(app, "nothing in the history yet to run it on");
    };
    if dry {
        let c = rules::check(s, &r, &a);
        let mut d = vec![format!("{} vs #{} {} ({}): {}", r.name, a.id, a.kind, a.summary, if c.matched { "matches" } else { "no match" })];
        for t in &r.spec.then {
            d.push(match rules::plan(s, &r, &a, t) {
                Ok(m) => format!("  would {m}"),
                Err(e) => format!("  ! {} would fail: {e}", t.verb()),
            });
        }
        app.models.detail = d;
        return;
    }
    let res = rules::run(s, &r, &a);
    app.refresh_history(true);
    let mut d = vec![format!("ran {} on #{} {}: {}", r.name, a.id, a.kind, res.status)];
    d.extend(res.messages.iter().map(|m| format!("  {m}")));
    app.models.detail = d;
    reload(app);
}

fn confirm(app: &mut App, c: Confirm, yes: bool) {
    if !yes {
        return ok(app, "cancelled");
    }
    let s = &app.store;
    let Some(m) = app.models.model.clone().or_else(|| app.models.list.get(app.models.sel).cloned()) else { return };
    match c {
        Confirm::Apply => apply(app, true),
        Confirm::Rollback(v) => rollback(app, v, true),
        Confirm::RemoveModel => match s.remove_model(&m.id, true) {
            Ok((m, n)) => {
                note(app, dz::act_model_remove(&m, n));
                app.models.model = None;
                app.models.screen = Screen::List;
                app.models.detail.clear();
                reload(app);
                ok(app, format!("removed model {} ({n} records); its rules are standalone and off", m.name));
            }
            Err(e) => err(app, e),
        },
        Confirm::DeleteRecord(id) => match s.delete_record(&m, &id) {
            Ok(r) => {
                note(app, dz::act_record_delete(&m, Some(&r), 1, &[]));
                app.models.detail.clear();
                reload(app);
                ok(app, format!("deleted {} record {}", m.name, r.short_id()));
            }
            Err(e) => err(app, e),
        },
        Confirm::UpdateMany(vals) => {
            let f = app.models.query.filter.clone();
            let r = if !app.models.query.text.is_empty() {
                let ids: Vec<String> =
                    s.records(&m, &RecordQuery { limit: None, ..app.models.query.clone() }).unwrap_or_default().into_iter().map(|r| r.id).collect();
                ids.iter().try_fold(0, |n, id| s.update_record(&m, id, &vals).map(|_| n + 1))
            } else {
                s.update_records(&m, &f, &vals)
            };
            match r {
                Ok(n) => {
                    note(app, dz::act_record_update(&m, None, n, &f, &vals));
                    reload(app);
                    ok(app, format!("updated {n} {} records", m.name));
                }
                Err(e) => err(app, e),
            }
        }
        Confirm::DeleteMany => {
            let f = app.models.query.filter.clone();
            let r = if !app.models.query.text.is_empty() {
                let ids: Vec<String> =
                    s.records(&m, &RecordQuery { limit: None, ..app.models.query.clone() }).unwrap_or_default().into_iter().map(|r| r.id).collect();
                s.delete_records_by_id(&m, &ids)
            } else {
                s.delete_records(&m, &f)
            };
            match r {
                Ok(n) => {
                    note(app, dz::act_record_delete(&m, None, n, &f));
                    reload(app);
                    ok(app, format!("deleted {n} {} records", m.name));
                }
                Err(e) => err(app, e),
            }
        }
        Confirm::DeleteRule(id) => match s.remove_rule(&id) {
            Ok(r) => {
                record(app, "rule.remove", format!("remove rule {}", r.name), serde_json::json!({ "rule": r.id, "name": r.name }));
                reload(app);
                ok(app, format!("removed rule {}", r.name));
            }
            Err(e) => err(app, e),
        },
    }
}

pub fn on_key(app: &mut App, k: KeyEvent) {
    if let Some(a) = app.models.ask.as_mut() {
        match ask_key(a, k) {
            Typed::Cancel => {
                app.models.ask = None;
                app.models.rule_draft = None;
                ok(app, "cancelled");
            }
            Typed::Done(v) => {
                let kind = a.kind.clone();
                app.models.ask = None;
                app.models.msg = None;
                answer(app, kind, v);
            }
            Typed::Editing => {}
        }
        return;
    }
    if let Some(c) = app.models.confirm.take() {
        return confirm(app, c, matches!(k.code, KeyCode::Char('y' | 'Y')));
    }
    app.models.msg = None;
    match app.models.screen {
        Screen::List => list_key(app, k),
        Screen::Design => design_key(app, k),
        Screen::History => history_key(app, k),
        Screen::Records => records_key(app, k),
        Screen::Rules => rules_key(app, k),
    }
}

/// Keys every model screen shares; true when handled.
fn common_key(app: &mut App, k: KeyEvent) -> bool {
    let to = match k.code {
        KeyCode::Char('m') => Screen::Design,
        KeyCode::Char('h') => Screen::History,
        KeyCode::Char('r') => Screen::Records,
        KeyCode::Char('f') => Screen::Rules,
        KeyCode::Char('p') => {
            if let Some(m) = app.models.model.clone().or_else(|| app.models.list.get(app.models.sel).cloned()) {
                pick_project(app, Purpose::Add(ItemKind::Model, m.id.clone(), serde_json::json!({}), m.name.clone()), Back::Models);
            }
            return true;
        }
        KeyCode::Esc if !app.models.detail.is_empty() => {
            app.models.detail.clear();
            return true;
        }
        KeyCode::Esc | KeyCode::Char('q') => {
            if app.models.screen == Screen::List {
                app.modal = None;
            } else {
                app.models.screen = Screen::List;
                app.models.detail.clear();
                reload(app);
            }
            return true;
        }
        _ => return false,
    };
    enter(app, to);
    true
}

fn list_key(app: &mut App, k: KeyEvent) {
    let n = app.models.list.len();
    let st = &mut app.models;
    match k.code {
        KeyCode::Down | KeyCode::Char('j') => st.sel = (st.sel + 1).min(n.saturating_sub(1)),
        KeyCode::Up | KeyCode::Char('k') => st.sel = st.sel.saturating_sub(1),
        KeyCode::Enter => enter(app, Screen::Design),
        KeyCode::Char('n') | KeyCode::Char('a') => ask(app, MAsk::NewModel, ""),
        KeyCode::Char('d') | KeyCode::Delete => {
            let Some(m) = st.list.get(st.sel).cloned() else { return };
            if m.builtin {
                return err(app, format!("{} is built in; open it (Enter) to drop the fields you added", m.name));
            }
            let c = st.counts.get(st.sel).copied().unwrap_or(0);
            st.model = None;
            st.confirm = Some(Confirm::RemoveModel);
            ok(app, format!("remove model {} and its {c} records? y confirms, any other key cancels", m.name));
        }
        _ => {
            common_key(app, k);
        }
    }
}

fn design_key(app: &mut App, k: KeyEvent) {
    let Some(m) = app.models.model.clone() else { return };
    let rows = design_rows(&m);
    let st = &mut app.models;
    let row = rows.get(st.fsel).cloned();
    match k.code {
        KeyCode::Down | KeyCode::Char('j') => st.fsel = (st.fsel + 1).min(rows.len().saturating_sub(1)),
        KeyCode::Up | KeyCode::Char('k') => st.fsel = st.fsel.saturating_sub(1),
        KeyCode::Char('a') | KeyCode::Char('n') => ask(app, MAsk::AddField, ""),
        KeyCode::Enter | KeyCode::Char('e') => match row {
            Some(r) if r.dropped => ok(app, format!("{} is dropped in the draft; u restores it", r.field.name)),
            Some(r) if builtin_field(&m, &r.field.name) => err(app, format!("{} is a built-in bars field", r.field.name)),
            Some(r) => {
                let t = dz::field_text(&app.store, &r.field);
                ask(app, MAsk::EditField(r.field.id.clone()), t)
            }
            None => ask(app, MAsk::AddField, ""),
        },
        KeyCode::Char('x') | KeyCode::Delete if row.is_none() && m.is_bars() => {
            err(app, "the built-in bars fields stay as they are (only fields you added can be dropped)");
        }
        KeyCode::Char('x') | KeyCode::Delete => {
            if let Some(r) = row.filter(|r| !r.dropped) {
                let mut d = m.pending().clone();
                d.fields.retain(|f| f.id != r.field.id);
                save_draft(app, d);
            }
        }
        KeyCode::Char('u') => {
            // undo the draft's change to this field
            let Some(r) = row else { return };
            let mut d = m.pending().clone();
            match (&r.was, r.dropped) {
                (Some(w), true) => {
                    let at = m.design.fields.iter().position(|f| f.id == w.id).unwrap_or(d.fields.len()).min(d.fields.len());
                    d.fields.insert(at, w.clone());
                }
                (Some(w), false) => {
                    if let Some(f) = d.fields.iter_mut().find(|f| f.id == w.id) {
                        *f = w.clone();
                    }
                }
                (None, _) => d.fields.retain(|f| f.id != r.field.id),
            }
            save_draft(app, d);
        }
        KeyCode::Char('K') | KeyCode::Char('J') => {
            let Some(r) = row.filter(|r| !r.dropped) else { return };
            let mut d = m.pending().clone();
            let i = d.fields.iter().position(|f| f.id == r.field.id).unwrap_or(0);
            let j = if k.code == KeyCode::Char('K') { i.checked_sub(1) } else { (i + 1 < d.fields.len()).then_some(i + 1) };
            if let Some(j) = j {
                d.fields.swap(i, j);
                save_draft(app, d);
                app.models.fsel = j;
            }
        }
        KeyCode::Char('A') => apply(app, false),
        KeyCode::Char('z') => discard(app),
        KeyCode::Char('N') => {
            let n = m.name.clone();
            ask(app, MAsk::ModelName, n)
        }
        KeyCode::Char('i') => {
            let d = m.description.clone();
            ask(app, MAsk::ModelAbout, d)
        }
        KeyCode::Char('d') => {
            if m.builtin {
                return err(app, format!("{} is built in and stays", m.name));
            }
            let c = app.store.count_records(&m).unwrap_or(0);
            app.models.confirm = Some(Confirm::RemoveModel);
            ok(app, format!("remove model {} and its {c} records? y confirms, any other key cancels", m.name));
        }
        _ => {
            common_key(app, k);
        }
    }
}

fn history_key(app: &mut App, k: KeyEvent) {
    let st = &mut app.models;
    let n = st.versions.len();
    match k.code {
        KeyCode::Down | KeyCode::Char('j') => {
            st.vsel = (st.vsel + 1).min(n.saturating_sub(1));
            show_version(app);
        }
        KeyCode::Up | KeyCode::Char('k') => {
            st.vsel = st.vsel.saturating_sub(1);
            show_version(app);
        }
        KeyCode::Enter => show_version(app),
        KeyCode::Char('b') | KeyCode::Char('R') => {
            let Some(v) = st.versions.get(st.vsel).map(|v| v.version) else { return };
            rollback(app, v, false)
        }
        _ => {
            common_key(app, k);
        }
    }
}

fn records_key(app: &mut App, k: KeyEvent) {
    let Some(m) = app.models.model.clone() else { return };
    let st = &mut app.models;
    let n = if st.group.is_empty() { st.records.len() } else { st.groups.len() };
    match k.code {
        KeyCode::Down | KeyCode::Char('j') => {
            st.rsel = (st.rsel + 1).min(n.saturating_sub(1));
            if !st.detail.is_empty() {
                show_record(app);
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            st.rsel = st.rsel.saturating_sub(1);
            if !st.detail.is_empty() {
                show_record(app);
            }
        }
        KeyCode::PageDown => st.rsel = (st.rsel + 10).min(n.saturating_sub(1)),
        KeyCode::PageUp => st.rsel = st.rsel.saturating_sub(10),
        KeyCode::Enter => show_record(app),
        KeyCode::Char('w') => {
            let w = st.query.filter.iter().map(rules::condition_text).collect::<Vec<_>>().join("; ");
            ask(app, MAsk::Where, w)
        }
        KeyCode::Char('/') => {
            let t = st.query.text.clone();
            ask(app, MAsk::Text, t)
        }
        KeyCode::Char('s') => {
            let t = st.query.sort.iter().map(|(f, asc)| if *asc { f.clone() } else { format!("-{f}") }).collect::<Vec<_>>().join(",");
            ask(app, MAsk::Sort, t)
        }
        KeyCode::Char('g') => {
            let t = st.group.iter().chain(st.aggs.iter()).cloned().collect::<Vec<_>>().join(", ");
            ask(app, MAsk::Group, t)
        }
        KeyCode::Char('c') => {
            st.query = RecordQuery::default();
            st.group.clear();
            st.aggs.clear();
            st.from_search = None;
            reload(app);
            ok(app, "filter, sort and grouping cleared");
        }
        KeyCode::Char('n') | KeyCode::Char('a') => {
            if m.is_bars() {
                return err(app, "bars come from imports; e edits the fields you added to a bar");
            }
            let t: Vec<String> = m.design.fields.iter().filter(|f| f.default.is_none()).map(|f| format!("{}=", f.name)).collect();
            ask(app, MAsk::NewRecord, t.join(" "))
        }
        KeyCode::Char('e') => {
            let Some(r) = selected_record(app) else { return };
            // bars: only the fields added to them can change
            let t = m
                .design
                .fields
                .iter()
                .map(|f| format!("{}={}", f.name, rules::quote(&r.values.get(&f.name).map(dz::value_text).unwrap_or_default())))
                .collect::<Vec<_>>()
                .join(" ");
            ask(app, MAsk::EditRecord(r.id.clone()), t)
        }
        KeyCode::Char('U') => {
            if app.models.query.filter.is_empty() && app.models.query.text.is_empty() {
                return err(app, "set a filter first (w or /): U changes every record it matches");
            }
            ask(app, MAsk::UpdateMany, "")
        }
        KeyCode::Char('d') | KeyCode::Delete => {
            if m.is_bars() {
                return err(app, "bars are removed with their source (Data ▸ Sources…)");
            }
            let Some(r) = selected_record(app) else { return };
            app.models.confirm = Some(Confirm::DeleteRecord(r.id.clone()));
            ok(app, format!("delete record {}? y confirms", r.short_id()));
        }
        KeyCode::Char('D') => {
            if m.is_bars() {
                return err(app, "bars are removed with their source (Data ▸ Sources…)");
            }
            if app.models.query.filter.is_empty() && app.models.query.text.is_empty() {
                return err(app, "set a filter first (w or /): D deletes every record it matches");
            }
            let n = app.models.total;
            app.models.confirm = Some(Confirm::DeleteMany);
            ok(app, format!("delete {n} matching record{}? y confirms", if n == 1 { "" } else { "s" }));
        }
        _ => {
            common_key(app, k);
        }
    }
}

fn rules_key(app: &mut App, k: KeyEvent) {
    let st = &mut app.models;
    let n = st.rules.len();
    match k.code {
        KeyCode::Down | KeyCode::Char('j') => st.usel = (st.usel + 1).min(n.saturating_sub(1)),
        KeyCode::Up | KeyCode::Char('k') => st.usel = st.usel.saturating_sub(1),
        KeyCode::Char('n') | KeyCode::Char('a') => ask(app, MAsk::RuleName, ""),
        KeyCode::Char('x') => try_rule(app, true),
        KeyCode::Enter => try_rule(app, false),
        KeyCode::Char(' ') => {
            let Some(r) = st.rules.get(st.usel).cloned() else { return };
            match app.store.set_rule_enabled(&r.id, !r.enabled) {
                Ok(n) => {
                    let (kind, verb) = if n.enabled { ("rule.enable", "on") } else { ("rule.disable", "off") };
                    record(app, kind, format!("switch rule {} {verb}", n.name), serde_json::json!({ "rule": n.id, "name": n.name }));
                    reload(app);
                    ok(app, format!("rule {} is {verb}", n.name));
                }
                Err(e) => err(app, e),
            }
        }
        KeyCode::Char('d') | KeyCode::Delete => {
            let Some(r) = st.rules.get(st.usel).cloned() else { return };
            st.confirm = Some(Confirm::DeleteRule(r.id.clone()));
            ok(app, format!("delete rule {}? y confirms", r.name));
        }
        _ => {
            common_key(app, k);
        }
    }
}

pub fn on_click(app: &mut App, i: usize) {
    if app.models.ask.is_some() {
        return;
    }
    let st = &mut app.models;
    let cur = match st.screen {
        Screen::List => &mut st.sel,
        Screen::Design => &mut st.fsel,
        Screen::History => &mut st.vsel,
        Screen::Records => &mut st.rsel,
        Screen::Rules => &mut st.usel,
    };
    if *cur == i {
        on_key(app, KeyEvent::from(KeyCode::Enter));
    } else {
        *cur = i;
        if st.screen == Screen::History {
            show_version(app);
        }
    }
}

fn mark_style(t: &crate::theme::Theme, c: char) -> Style {
    match c {
        '+' => Style::new().fg(t.success),
        '-' | '✗' => Style::new().fg(t.error),
        '~' => Style::new().fg(t.warning),
        _ => Style::new().fg(t.muted),
    }
}

fn plural(n: usize, w: &str) -> String {
    format!("{n} {w}{}", if n == 1 { "" } else { "s" })
}

pub fn draw(app: &mut App, f: &mut Frame, r: Rect, hits: &mut Vec<(Rect, Hit)>) {
    match app.models.screen {
        Screen::List => draw_list(app, f, r, hits),
        Screen::Design => draw_design(app, f, r, hits),
        Screen::History => draw_history(app, f, r, hits),
        Screen::Records => draw_records(app, f, r, hits),
        Screen::Rules => draw_rules(app, f, r, hits),
    }
}

fn finish<'a>(app: &App, mut v: View<'a>, keys: Vec<(&'static str, KeyCode)>) -> View<'a> {
    let t = &app.theme;
    let st = &app.models;
    if !st.detail.is_empty() {
        let mut head: Vec<Line> = st.detail.iter().map(|d| Line::styled(format!(" {d}"), Style::new().fg(t.fg))).collect();
        head.push(Line::raw(""));
        head.append(&mut v.head);
        v.head = head;
    }
    v.ask = st.ask.as_ref().map(|a| (a.title.clone(), a.value.clone()));
    v.msg = st.msg.clone();
    v.keys = keys;
    v
}

fn draw_list(app: &mut App, f: &mut Frame, r: Rect, hits: &mut Vec<(Rect, Hit)>) {
    let t = app.theme.clone();
    let st = &app.models;
    let s = &app.store;
    let mut v = View::new("Model designer · record tables you design");
    let w = st.list.iter().map(|m| m.name.chars().count()).max().unwrap_or(4).clamp(6, 24);
    for (i, m) in st.list.iter().enumerate() {
        let on = i == st.sel;
        let n = st.counts.get(i).copied().unwrap_or(0);
        v.rows.push(Line::from(vec![
            Span::styled(format!(" {} ", if m.builtin { "◆" } else { "◇" }), Style::new().fg(t.accent)),
            Span::styled(format!("{:<w$} ", m.name), row_style(&t, on)),
            Span::styled(format!(" {:<26}", dz::status_text(s, m)), Style::new().fg(if m.draft.is_some() { t.warning } else { t.accent })),
            Span::styled(
                format!(
                    " {} · {}{}",
                    plural(m.pending().fields.len(), "field"),
                    plural(n, "record"),
                    if m.description.is_empty() { String::new() } else { format!(" · {}", m.description) }
                ),
                Style::new().fg(t.muted),
            ),
        ]));
    }
    if st.list.len() <= 1 {
        v.head.push(muted(&t, " n designs a new model (a table of records you define: fields, types, constraints)."));
        v.head.push(muted(&t, " ◆ bars is built in: you can add fields to it, never change or drop its own."));
        v.head.push(Line::raw(""));
    }
    v.sel = (!st.list.is_empty()).then_some(st.sel);
    let v = finish(
        app,
        v,
        vec![
            ("Enter design", KeyCode::Enter),
            ("n new", KeyCode::Char('n')),
            ("r records", KeyCode::Char('r')),
            ("h history", KeyCode::Char('h')),
            ("f rules", KeyCode::Char('f')),
            ("p project", KeyCode::Char('p')),
            ("d remove", KeyCode::Char('d')),
            ("Esc", KeyCode::Esc),
        ],
    );
    draw_view(f, r, v, &t, hits);
}

fn draw_design(app: &mut App, f: &mut Frame, r: Rect, hits: &mut Vec<(Rect, Hit)>) {
    let t = app.theme.clone();
    let Some(m) = app.models.model.clone() else { return };
    let s = &app.store;
    let rows = design_rows(&m);
    let left = Rect { width: (r.width * 3 / 5).max(30).min(r.width), ..r };
    let right = Rect { x: left.right(), width: r.width.saturating_sub(left.width), ..r };
    let mut v = View::new(format!("Design · {} · {}", m.name, dz::status_text(s, &m)));
    if !m.description.is_empty() {
        v.head.push(muted(&t, format!(" {}", m.description)));
    }
    if m.is_bars() {
        let own = crate::store::records::field_names(&Model { design: Design::default(), draft: None, ..m.clone() });
        v.head.push(muted(&t, format!(" built in (fixed): {}", own.join(", "))));
    }
    let w = rows.iter().map(|x| x.field.name.chars().count()).max().unwrap_or(4).clamp(6, 20);
    for (i, row) in rows.iter().enumerate() {
        let on = i == app.models.fsel;
        let f0 = &row.field;
        let mut flags = vec![];
        if f0.required {
            flags.push("required".to_string());
        }
        if f0.unique {
            flags.push("unique".into());
        }
        if f0.index {
            flags.push("index".into());
        }
        if let Some(d) = &f0.default {
            flags.push(format!("default {}", dz::value_text(d)));
        }
        if !f0.choices.is_empty() {
            flags.push(f0.choices.join("|"));
        }
        if let Some(w0) = &row.was
            && w0.name != f0.name
        {
            flags.push(format!("was {}", w0.name));
        }
        let mut name = Style::new().fg(if row.dropped { t.muted } else { t.fg });
        if row.dropped {
            name = name.add_modifier(Modifier::CROSSED_OUT);
        }
        if on {
            name = row_style(&t, true);
        }
        let built = builtin_field(&m, &f0.name);
        v.rows.push(Line::from(vec![
            Span::styled(format!(" {} ", row.mark), mark_style(&t, row.mark).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:<w$} ", f0.name), name),
            Span::styled(format!(" {:<10}", dz::type_text(s, &f0.ty)), Style::new().fg(t.accent)),
            Span::styled(format!(" {}{}", if built { "built in " } else { "" }, flags.join(" · ")), Style::new().fg(t.muted)),
        ]));
    }
    if rows.is_empty() {
        v.head.push(muted(&t, " no fields yet: a adds one, e.g. symbol:text required · price:real · side:text choices=buy|sell"));
    }
    v.sel = (!rows.is_empty()).then_some(app.models.fsel);
    let v = finish(
        app,
        v,
        vec![
            ("a add", KeyCode::Char('a')),
            ("e edit", KeyCode::Char('e')),
            ("x drop", KeyCode::Char('x')),
            ("K/J move", KeyCode::Char('K')),
            ("u undo", KeyCode::Char('u')),
            ("A apply", KeyCode::Char('A')),
            ("z discard", KeyCode::Char('z')),
            ("N rename", KeyCode::Char('N')),
            ("r records", KeyCode::Char('r')),
            ("h history", KeyCode::Char('h')),
            ("f rules", KeyCode::Char('f')),
            ("Esc", KeyCode::Esc),
        ],
    );
    draw_view(f, left, v, &t, hits);
    // the live migration plan
    let mut lines: Vec<Line> = vec![];
    match &app.models.plan {
        Some(Ok(p)) if p.steps.is_empty() && p.blocked.is_empty() && m.version > 0 => {
            lines.push(muted(&t, format!(" v{} is applied; nothing to migrate.", m.version)));
            lines.push(muted(&t, " Changes you make land in a draft; this pane shows what applying it does."));
        }
        Some(Ok(p)) => {
            for (i, l) in dz::plan_lines(s, p).into_iter().enumerate() {
                let st = if i == 0 {
                    Style::new().fg(t.accent).add_modifier(Modifier::BOLD)
                } else {
                    let c = l.trim_start().chars().next().unwrap_or(' ');
                    if l.contains("lose") && c == '!' { Style::new().fg(t.error) } else { mark_style(&t, c) }
                };
                lines.push(Line::styled(format!(" {l}"), st));
            }
            lines.push(Line::raw(""));
            lines.push(muted(&t, if p.blocked.is_empty() { " A applies this as the next version" } else { " fix the ✗ steps before applying" }));
        }
        Some(Err(e)) => lines.push(Line::styled(format!(" {e}"), Style::new().fg(t.error))),
        None => {}
    }
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }).block(block("Migration plan", false, &t)), right);
}

fn draw_history(app: &mut App, f: &mut Frame, r: Rect, hits: &mut Vec<(Rect, Hit)>) {
    let t = app.theme.clone();
    let Some(m) = app.models.model.clone() else { return };
    let s = &app.store;
    let mut v = View::new(format!("History · {} · v{} applied", m.name, m.version));
    for (i, x) in app.models.versions.iter().enumerate() {
        let on = i == app.models.vsel;
        let current = x.version == m.version;
        let by = crate::history::author_label(s, x.author_id.as_deref());
        let rule = x
            .rule_id
            .as_ref()
            .map(|id| format!(" · by rule {}", s.rule(id).ok().flatten().map(|r| r.name).unwrap_or_else(|| "(removed)".into())))
            .unwrap_or_default();
        v.rows.push(Line::from(vec![
            Span::styled(format!(" {} ", if current { "●" } else { "○" }), Style::new().fg(t.accent)),
            Span::styled(format!("v{:<3} ", x.version), row_style(&t, on)),
            Span::styled(format!(" {} ", crate::ingest::time::fmt_ts(x.created_at)), Style::new().fg(t.muted)),
            Span::styled(format!(" {} · {}", plural(x.design.fields.len(), "field"), plural(x.steps.len(), "step")), Style::new().fg(t.accent)),
            Span::styled(
                format!(" · {by}{rule}{}", if x.note.is_empty() { String::new() } else { format!(" · {}", x.note) }),
                Style::new().fg(t.muted),
            ),
        ]));
    }
    if app.models.versions.is_empty() {
        v.head.push(muted(&t, " nothing applied yet: m opens the designer, A applies the draft as v1"));
    }
    v.sel = (!app.models.versions.is_empty()).then_some(app.models.vsel);
    let v = finish(
        app,
        v,
        vec![
            ("Enter show", KeyCode::Enter),
            ("b roll back to", KeyCode::Char('b')),
            ("m design", KeyCode::Char('m')),
            ("r records", KeyCode::Char('r')),
            ("Esc", KeyCode::Esc),
        ],
    );
    draw_view(f, r, v, &t, hits);
}

fn cell(v: Option<&Value>, w: usize) -> String {
    let s = v.map(dz::value_text).unwrap_or_default().replace('\n', " ");
    let s: String = if s.chars().count() > w { s.chars().take(w.saturating_sub(1)).chain(['…']).collect() } else { s };
    format!("{s:<w$}")
}

fn draw_records(app: &mut App, f: &mut Frame, r: Rect, hits: &mut Vec<(Rect, Hit)>) {
    let t = app.theme.clone();
    let Some(m) = app.models.model.clone() else { return };
    let st = &app.models;
    let names = crate::store::records::field_names(&m);
    let mut v = View::new(format!(
        "Records · {} v{} · {}{}",
        m.name,
        m.version,
        plural(st.total, "record"),
        st.from_search.as_ref().map(|s| format!(" · live search {s}")).unwrap_or_default()
    ));
    let mut q = vec![];
    if !st.query.filter.is_empty() {
        q.push(format!("where {}", st.query.filter.iter().map(rules::condition_text).collect::<Vec<_>>().join("; ")));
    }
    if !st.query.text.is_empty() {
        let x = &st.query.text;
        q.push(format!("text \"{x}\""));
    }
    if !st.query.sort.is_empty() {
        q.push(format!("sort {}", st.query.sort.iter().map(|(f, a)| if *a { f.clone() } else { format!("-{f}") }).collect::<Vec<_>>().join(",")));
    }
    if !st.group.is_empty() {
        q.push(format!("group by {}{}", st.group.join(", "), if st.aggs.is_empty() { String::new() } else { format!(" · {}", st.aggs.join(" ")) }));
    }
    if !q.is_empty() {
        v.head.push(Line::styled(format!(" {}", q.join(" · ")), Style::new().fg(t.accent)));
    }
    let avail = r.width.saturating_sub(14) as usize;
    if st.group.is_empty() {
        let shown: Vec<&String> = names.iter().take(12).collect();
        let w = (avail / shown.len().max(1)).clamp(6, 24);
        let head = format!(" {:<9}{}", "id", shown.iter().map(|n| cell(Some(&Value::String((*n).clone())), w)).collect::<Vec<_>>().join(" "));
        v.head.push(Line::styled(head, Style::new().fg(t.muted).add_modifier(Modifier::BOLD)));
        for (i, rec) in st.records.iter().enumerate() {
            let on = i == st.rsel;
            let line = format!(
                " {:<9}{}",
                &rec.short_id()[..rec.short_id().len().min(8)],
                shown.iter().map(|n| cell(rec.values.get(*n), w)).collect::<Vec<_>>().join(" ")
            );
            v.rows.push(Line::styled(line, row_style(&t, on)));
        }
        if st.records.is_empty() {
            v.head.push(muted(&t, if m.is_bars() { " no bars match" } else { " no records yet: n adds one, or f feeds the model with a rule" }));
        } else if st.records.len() < st.total {
            v.head.push(muted(&t, format!(" showing the first {} of {} (w filters, s sorts)", st.records.len(), st.total)));
        }
        v.sel = (!st.records.is_empty()).then_some(st.rsel);
    } else {
        for (i, g) in st.groups.iter().enumerate() {
            let on = i == st.rsel;
            let aggs = g.aggs.iter().map(|(k, x)| format!("{k}={}", dz::value_text(x))).collect::<Vec<_>>().join(" ");
            v.rows.push(Line::from(vec![
                Span::styled(format!(" {:<40}", group_key(g)), row_style(&t, on)),
                Span::styled(format!(" {:>6} ", g.count), Style::new().fg(t.accent)),
                Span::styled(format!(" {aggs}"), Style::new().fg(t.muted)),
            ]));
        }
        v.sel = (!st.groups.is_empty()).then_some(st.rsel);
    }
    let v = finish(
        app,
        v,
        vec![
            ("Enter show", KeyCode::Enter),
            ("n new", KeyCode::Char('n')),
            ("e edit", KeyCode::Char('e')),
            ("d delete", KeyCode::Char('d')),
            ("w where", KeyCode::Char('w')),
            ("/ text", KeyCode::Char('/')),
            ("s sort", KeyCode::Char('s')),
            ("g group", KeyCode::Char('g')),
            ("U update all", KeyCode::Char('U')),
            ("D delete all", KeyCode::Char('D')),
            ("c clear", KeyCode::Char('c')),
            ("m design", KeyCode::Char('m')),
            ("Esc", KeyCode::Esc),
        ],
    );
    draw_view(f, r, v, &t, hits);
}

fn draw_rules(app: &mut App, f: &mut Frame, r: Rect, hits: &mut Vec<(Rect, Hit)>) {
    let t = app.theme.clone();
    let Some(m) = app.models.model.clone() else { return };
    let s = &app.store;
    let mut v = View::new(format!("Rules · {} · what feeds and changes it", m.name));
    for (i, x) in app.models.rules.iter().enumerate() {
        let on = i == app.models.usel;
        v.rows.push(Line::from(vec![
            Span::styled(format!(" {} ", if x.enabled { "●" } else { "○" }), Style::new().fg(if x.enabled { t.accent } else { t.muted })),
            Span::styled(format!("{:<20} ", x.name), row_style(&t, on)),
            Span::styled(format!(" on {} ", x.spec.on.join(",")), Style::new().fg(t.accent)),
            Span::styled(
                format!(
                    " → {} · {}",
                    x.spec.then.iter().map(|th| rules::then_text(s, th)).collect::<Vec<_>>().join("; "),
                    plural(x.runs as usize, "run")
                ),
                Style::new().fg(t.muted),
            ),
        ]));
    }
    if app.models.rules.is_empty() {
        v.head.push(muted(&t, " no rules live in this model yet: n makes one. It is offered a then-action that copies"));
        v.head.push(muted(&t, format!("   the trigger's fields into a record: {}", feed_template(&m))));
    }
    v.sel = (!app.models.rules.is_empty()).then_some(app.models.usel);
    let v = finish(
        app,
        v,
        vec![
            ("n new", KeyCode::Char('n')),
            ("x dry run", KeyCode::Char('x')),
            ("Enter run now", KeyCode::Enter),
            ("Space on/off", KeyCode::Char(' ')),
            ("d delete", KeyCode::Char('d')),
            ("r records", KeyCode::Char('r')),
            ("Esc", KeyCode::Esc),
        ],
    );
    draw_view(f, r, v, &t, hits);
}
