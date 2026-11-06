//! Model designer (`mnnn model` / TUI Data ▸ Model designer…).
//! Browse designs and rows, edit fields as a draft, preview and apply migrations,
//! roll versions back, and attach model-owned rules (via the rules API).
use super::data::{open, open_write};
use muninn_core::designer::{self as d};
use muninn_core::store::models::bar_key;
use muninn_core::store::{ApplyOpts, Design, Field, FieldOp, FieldType, Model, ModelVersion, NewAction, OwnerKind, Plan, Record, RecordQuery, Store};
use serde_json::Value;

pub struct ModelInfo {
    pub id: String,
    pub short_id: String,
    pub name: String,
    pub description: String,
    pub builtin: bool,
    pub version: i64,
    pub is_bars: bool,
    pub fields: Vec<String>,
    pub record_count: u32,
    pub has_draft: bool,
    pub status: String,
    pub design_fields: Vec<DesignFieldInfo>,
}

#[derive(Debug)]
pub struct DesignFieldInfo {
    pub id: String,
    pub name: String,
    pub spec: String,
    pub ty: String,
    pub required: bool,
    pub unique: bool,
    pub indexed: bool,
    pub default_value: String,
    pub choices: Vec<String>,
    pub about: String,
    /// `+` added in the draft, `~` changed, `-` dropped, space unchanged.
    pub mark: String,
    pub dropped: bool,
    pub builtin: bool,
}

pub struct ModelPlanInfo {
    pub from: i64,
    pub to: i64,
    pub rows: u32,
    pub lost: u32,
    pub destructive: bool,
    pub blocked: Vec<String>,
    pub lines: Vec<String>,
}

pub struct ModelVersionInfo {
    pub version: i64,
    pub created_at: i64,
    pub note: String,
    pub author: String,
    pub field_count: u32,
    pub current: bool,
    pub steps: Vec<String>,
}

#[derive(Debug)]
pub struct RecordField {
    pub name: String,
    pub value: String,
}

pub struct RecordInfo {
    pub id: String,
    pub short_id: String,
    pub model_id: String,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub fields: Vec<RecordField>,
}

fn val_str(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

fn note(store: &Store, a: d::Act) {
    store.note_action(&NewAction::new(a.kind, "app", a.summary).params(a.params));
}

fn field_info(store: &Store, m: &Model, f: &Field, mark: &str, dropped: bool) -> DesignFieldInfo {
    DesignFieldInfo {
        id: f.id.clone(),
        name: f.name.clone(),
        spec: d::field_text(store, f),
        ty: d::type_text(store, &f.ty),
        required: f.required,
        unique: f.unique,
        indexed: f.index,
        default_value: f.default.as_ref().map(d::value_text).unwrap_or_default(),
        choices: f.choices.clone(),
        about: f.about.clone(),
        mark: mark.to_string(),
        dropped,
        builtin: m.is_bars() && bar_key(f).is_some(),
    }
}

fn design_fields(store: &Store, m: &Model) -> Vec<DesignFieldInfo> {
    let mut rows: Vec<DesignFieldInfo> = m
        .pending()
        .fields
        .iter()
        .map(|f| {
            let mark = match m.design.by_id(&f.id) {
                None => "+",
                Some(w) if w != f => "~",
                Some(_) => " ",
            };
            field_info(store, m, f, mark, false)
        })
        .collect();
    for f in &m.design.fields {
        if m.pending().by_id(&f.id).is_none() {
            rows.push(field_info(store, m, f, "-", true));
        }
    }
    rows
}

fn model_info(store: &Store, m: &Model) -> ModelInfo {
    ModelInfo {
        id: m.id.clone(),
        short_id: m.short_id().to_string(),
        name: m.name.clone(),
        description: m.description.clone(),
        builtin: m.builtin,
        version: m.version,
        is_bars: m.is_bars(),
        fields: m.pending().fields.iter().map(|f| f.name.clone()).collect(),
        record_count: store.count_records(m).unwrap_or(0) as u32,
        has_draft: m.draft.is_some(),
        status: d::status_text(store, m),
        design_fields: design_fields(store, m),
    }
}

fn plan_info(store: &Store, p: &Plan) -> ModelPlanInfo {
    ModelPlanInfo {
        from: p.from,
        to: p.to,
        rows: p.rows as u32,
        lost: p.lost() as u32,
        destructive: p.destructive(),
        blocked: p.blocked.clone(),
        lines: d::plan_lines(store, p),
    }
}

fn version_info(store: &Store, m: &Model, v: &ModelVersion) -> ModelVersionInfo {
    let author = v.author_id.as_deref().and_then(|a| store.author(a).ok().flatten()).map(|a| a.name).unwrap_or_default();
    ModelVersionInfo {
        version: v.version,
        created_at: v.created_at,
        note: v.note.clone(),
        author,
        field_count: v.design.fields.len() as u32,
        current: v.version == m.version,
        steps: v.steps.iter().map(|s| format!("{} {}", d::step_mark(s), d::step_text(store, s))).collect(),
    }
}

fn record_info(m: &Model, r: &Record) -> RecordInfo {
    let mut fields = vec![
        RecordField { name: "id".into(), value: r.id.clone() },
        RecordField { name: "_version".into(), value: r.version.to_string() },
        RecordField { name: "_created_at".into(), value: r.created_at.to_string() },
        RecordField { name: "_updated_at".into(), value: r.updated_at.to_string() },
    ];
    if let Some(a) = &r.author_id {
        fields.push(RecordField { name: "_author".into(), value: a.clone() });
    }
    for f in &m.design.fields {
        let v = r.values.get(&f.name).map(val_str).unwrap_or_default();
        fields.push(RecordField { name: f.name.clone(), value: v });
    }
    RecordInfo {
        id: r.id.clone(),
        short_id: r.short_id().to_string(),
        model_id: m.id.clone(),
        version: r.version,
        created_at: r.created_at,
        updated_at: r.updated_at,
        fields,
    }
}

fn parse_ops(add: &[String], change: &[String], drop: &[String], rename: &[String], moves: &[String]) -> anyhow::Result<Vec<FieldOp>> {
    let mut ops = Vec::new();
    for r in rename {
        ops.push(d::parse_rename(r)?);
    }
    ops.extend(change.iter().cloned().map(|spec| FieldOp::Change { spec }));
    ops.extend(drop.iter().cloned().map(|field| FieldOp::Drop { field }));
    ops.extend(add.iter().cloned().map(|spec| FieldOp::Add { spec }));
    for mv in moves {
        ops.push(d::parse_move(mv)?);
    }
    Ok(ops)
}

pub(crate) fn models_of(store: &Store) -> anyhow::Result<Vec<ModelInfo>> {
    Ok(store.models()?.into_iter().map(|m| model_info(store, &m)).collect())
}

/// Models in the store (`mnnn model list` / TUI Data ▸ Model designer…).
pub fn models() -> anyhow::Result<Vec<ModelInfo>> {
    models_of(&open()?)
}

pub(crate) fn model_of(store: &Store, key: &str) -> anyhow::Result<ModelInfo> {
    let m = store.find_model(key.trim())?;
    Ok(model_info(store, &m))
}

/// One model by id or name.
pub fn model(key: String) -> anyhow::Result<ModelInfo> {
    model_of(&open()?, &key)
}

pub(crate) fn model_records_of(store: &Store, model: &str, text: Option<&str>, limit: Option<u32>) -> anyhow::Result<Vec<RecordInfo>> {
    let m = store.find_model(model.trim())?;
    let q = RecordQuery { text: text.unwrap_or("").to_string(), limit: Some(limit.unwrap_or(500) as usize), ..Default::default() };
    Ok(store.records(&m, &q)?.into_iter().map(|r| record_info(&m, &r)).collect())
}

/// Records of a model, optionally filtered by text (TUI Records screen).
pub fn model_records(model: String, text: Option<String>, limit: Option<u32>) -> anyhow::Result<Vec<RecordInfo>> {
    model_records_of(&open()?, &model, text.as_deref(), limit)
}

pub(crate) fn model_record_of(store: &Store, model: &str, id: &str) -> anyhow::Result<RecordInfo> {
    let m = store.find_model(model.trim())?;
    let r = store.find_record(&m, id.trim())?;
    Ok(record_info(&m, &r))
}

/// One record of a model (TUI record detail / found-records Enter).
pub fn model_record(model: String, id: String) -> anyhow::Result<RecordInfo> {
    model_record_of(&open()?, &model, &id)
}

/// Field type names (`mnnn model types`).
pub fn model_field_types() -> anyhow::Result<Vec<String>> {
    Ok(FieldType::NAMES.iter().map(|s| s.to_string()).collect())
}

pub(crate) fn model_create_of(store: &Store, name: &str, description: &str, fields: &str, draft: bool) -> anyhow::Result<ModelInfo> {
    let (m, _) = store.create_model(name, description, Design::default(), false)?;
    let set = (|| -> muninn_core::error::Result<Model> {
        let design = Design { fields: d::parse_fields(store, &m, fields)?, ..Default::default() };
        store.set_model_draft(&m.id, Some(design))
    })();
    let m = match set {
        Ok(m) => m,
        Err(e) => {
            let _ = store.remove_model(&m.id, true);
            return Err(e.into());
        }
    };
    note(store, d::act_model_create(&m));
    let m = if draft {
        m
    } else {
        let (m, p) = store.apply_model(&m.id, &ApplyOpts { destructive: false, note: String::new() })?;
        note(store, d::act_model_apply(store, &m, &p, false));
        m
    };
    Ok(model_info(store, &m))
}

/// Create a model (`mnnn model create` / TUI `n`). [fields] is `name:type …; …`.
/// When [draft] is true the design is not applied (TUI: `A` applies it as v1).
pub fn model_create(name: String, description: String, fields: String, draft: bool) -> anyhow::Result<ModelInfo> {
    model_create_of(&open_write()?, &name, &description, &fields, draft)
}

pub(crate) fn model_rename_of(store: &Store, key: &str, name: Option<&str>, description: Option<&str>) -> anyhow::Result<ModelInfo> {
    let m = store.find_model(key.trim())?;
    let old = m.name.clone();
    let n = store.rename_model(&m.id, name, description)?;
    note(store, d::act_model_edit(&n, &[], (old != n.name).then_some(old.as_str())));
    Ok(model_info(store, &n))
}

/// Rename a model and/or set its description (TUI `N` / `i`).
pub fn model_rename(key: String, name: Option<String>, description: Option<String>) -> anyhow::Result<ModelInfo> {
    model_rename_of(&open_write()?, &key, name.as_deref(), description.as_deref())
}

pub(crate) fn model_edit_of(
    store: &Store,
    key: &str,
    add: &[String],
    change: &[String],
    drop: &[String],
    rename: &[String],
    moves: &[String],
) -> anyhow::Result<ModelInfo> {
    let m = store.find_model(key.trim())?;
    let ops = parse_ops(add, change, drop, rename, moves)?;
    if ops.is_empty() {
        return Ok(model_info(store, &m));
    }
    let n = store.edit_model(&m.id, &ops)?;
    note(store, d::act_model_edit(&n, &ops, None));
    Ok(model_info(store, &n))
}

/// Edit the draft (`mnnn model edit` / TUI `a` `x` `J` `K`). Specs use the designer field syntax.
pub fn model_edit(
    key: String,
    add: Vec<String>,
    change: Vec<String>,
    drop: Vec<String>,
    rename: Vec<String>,
    moves: Vec<String>,
) -> anyhow::Result<ModelInfo> {
    model_edit_of(&open_write()?, &key, &add, &change, &drop, &rename, &moves)
}

pub(crate) fn model_set_field_of(store: &Store, key: &str, field_id: &str, spec: &str) -> anyhow::Result<ModelInfo> {
    let m = store.find_model(key.trim())?;
    let mut f = d::parse_field(store, &m, spec, None)?;
    let mut design = m.pending().clone();
    let i = design.fields.iter().position(|x| x.id == field_id).ok_or_else(|| anyhow::anyhow!("that field no longer exists"))?;
    f.id = field_id.to_string();
    design.fields[i] = f;
    store.check_design(&m, &design)?;
    let ops = d::diff_ops(store, m.pending(), &design);
    let n = store.set_model_draft(&m.id, Some(design))?;
    note(store, d::act_model_edit(&n, &ops, None));
    Ok(model_info(store, &n))
}

/// Replace one draft field, keeping its id so a rename keeps the values (TUI `e`).
pub fn model_set_field(key: String, field_id: String, spec: String) -> anyhow::Result<ModelInfo> {
    model_set_field_of(&open_write()?, &key, &field_id, &spec)
}

pub(crate) fn model_plan_of(store: &Store, key: &str) -> anyhow::Result<Option<ModelPlanInfo>> {
    let m = store.find_model(key.trim())?;
    if m.draft.is_none() && m.version > 0 {
        return Ok(None);
    }
    let p = store.plan_model(&m, m.pending())?;
    if p.is_empty() && m.version > 0 {
        return Ok(None);
    }
    Ok(Some(plan_info(store, &p)))
}

/// What applying the draft would do (`mnnn model plan` / TUI design screen).
pub fn model_plan(key: String) -> anyhow::Result<Option<ModelPlanInfo>> {
    model_plan_of(&open()?, &key)
}

pub(crate) fn model_apply_of(store: &Store, key: &str, destructive: bool) -> anyhow::Result<ModelPlanInfo> {
    let m = store.find_model(key.trim())?;
    let (n, p) = store.apply_model(&m.id, &ApplyOpts { destructive, note: String::new() })?;
    note(store, d::act_model_apply(store, &n, &p, false));
    Ok(plan_info(store, &p))
}

/// Apply the draft as the next version (`mnnn model apply` / TUI `A`).
/// A migration that throws values away needs [destructive] (the app asks first).
pub fn model_apply(key: String, destructive: bool) -> anyhow::Result<ModelPlanInfo> {
    model_apply_of(&open_write()?, &key, destructive)
}

pub(crate) fn model_discard_of(store: &Store, key: &str) -> anyhow::Result<ModelInfo> {
    let m = store.find_model(key.trim())?;
    if m.draft.is_none() {
        return Ok(model_info(store, &m));
    }
    let n = store.set_model_draft(&m.id, None)?;
    note(store, d::act_model_discard(&n));
    Ok(model_info(store, &n))
}

/// Throw away the draft (TUI `z`).
pub fn model_discard(key: String) -> anyhow::Result<ModelInfo> {
    model_discard_of(&open_write()?, &key)
}

pub(crate) fn model_history_of(store: &Store, key: &str) -> anyhow::Result<Vec<ModelVersionInfo>> {
    let m = store.find_model(key.trim())?;
    let mut vs = store.model_versions(&m.id)?;
    vs.reverse();
    Ok(vs.iter().map(|v| version_info(store, &m, v)).collect())
}

/// Applied versions (`mnnn model history` / TUI History).
pub fn model_history(key: String) -> anyhow::Result<Vec<ModelVersionInfo>> {
    model_history_of(&open()?, &key)
}

pub(crate) fn model_rollback_plan_of(store: &Store, key: &str, version: i64) -> anyhow::Result<ModelPlanInfo> {
    let p = store.rollback_plan(key.trim(), version)?;
    Ok(plan_info(store, &p))
}

/// What rolling back to [version] would do (`mnnn model rollback --dry-run`).
pub fn model_rollback_plan(key: String, version: i64) -> anyhow::Result<ModelPlanInfo> {
    model_rollback_plan_of(&open()?, &key, version)
}

pub(crate) fn model_rollback_of(store: &Store, key: &str, version: i64, destructive: bool) -> anyhow::Result<ModelPlanInfo> {
    let m = store.find_model(key.trim())?;
    let (n, p) = store.rollback_model(&m.id, version, &ApplyOpts { destructive, note: String::new() })?;
    note(store, d::act_model_apply(store, &n, &p, true));
    Ok(plan_info(store, &p))
}

/// Roll back to an earlier design as the next version (TUI History `r`).
pub fn model_rollback(key: String, version: i64, destructive: bool) -> anyhow::Result<ModelPlanInfo> {
    model_rollback_of(&open_write()?, &key, version, destructive)
}

pub(crate) fn model_remove_of(store: &Store, key: &str, destructive: bool) -> anyhow::Result<String> {
    let m = store.find_model(key.trim())?;
    let name = m.name.clone();
    let (m, n) = store.remove_model(&m.id, destructive)?;
    note(store, d::act_model_remove(&m, n));
    Ok(format!("removed model {name} ({n} record(s))"))
}

/// Remove a model and its records (TUI `d`). Needs [destructive] when it has rows.
pub fn model_remove(key: String, destructive: bool) -> anyhow::Result<String> {
    model_remove_of(&open_write()?, &key, destructive)
}

pub(crate) fn model_rule_homes_of(store: &Store, key: &str) -> anyhow::Result<Vec<String>> {
    let m = store.find_model(key.trim())?;
    let rules = store.owned_rules(OwnerKind::Model, &m.id)?;
    Ok(rules.into_iter().map(|r| r.id).collect())
}

/// Ids of rules owned by this model (TUI Rules screen). Create them with `rule_create` and home `model:NAME`.
pub fn model_rule_ids(key: String) -> anyhow::Result<Vec<String>> {
    model_rule_homes_of(&open()?, &key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use muninn_core::store::{Design, Field, FieldType, Store};

    #[test]
    fn lists_a_model_and_opens_a_record() {
        let s = Store::open_in_memory().unwrap();
        let (m, _) =
            s.create_model("signals", "alerts", Design { fields: vec![Field::new("memo", FieldType::Text)], ..Default::default() }, true).unwrap();
        let mut values = serde_json::Map::new();
        values.insert("memo".into(), serde_json::json!("alpha signal"));
        let r = s.create_record(&m, &values).unwrap();

        let listed = models_of(&s).unwrap();
        assert!(listed.iter().any(|x| x.name == "signals" && x.fields.contains(&"memo".to_string()) && x.record_count == 1));
        let got = model_of(&s, &m.id).unwrap();
        assert_eq!(got.name, "signals");
        assert_eq!(got.description, "alerts");
        assert!(got.design_fields.iter().any(|f| f.name == "memo"));

        let rows = model_records_of(&s, &m.name, Some("alpha"), None).unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].fields.iter().any(|f| f.name == "memo" && f.value.contains("alpha")), "{:?}", rows[0].fields);
        assert!(model_records_of(&s, &m.name, Some("no-such"), None).unwrap().is_empty());

        let one = model_record_of(&s, &m.id, &r.id).unwrap();
        assert_eq!(one.id, r.id);
        assert_eq!(one.model_id, m.id);
        assert!(one.fields.iter().any(|f| f.name == "memo" && f.value == "alpha signal"));
    }

    #[test]
    fn creates_edits_plans_and_applies_a_draft() {
        let s = Store::open_in_memory().unwrap();
        let m = model_create_of(&s, "signals", "alerts", "memo:text", true).unwrap();
        assert!(m.has_draft);
        assert!(m.design_fields.iter().any(|f| f.name == "memo" && f.mark.trim() == "+"));
        let plan = model_plan_of(&s, &m.id).unwrap().expect("a new model has a plan");
        assert!(plan.lines.iter().any(|l| l.contains("memo")), "{:?}", plan.lines);
        let applied = model_apply_of(&s, &m.id, false).unwrap();
        assert_eq!(applied.to, 1);
        let m = model_of(&s, &m.id).unwrap();
        assert!(!m.has_draft);
        assert_eq!(m.version, 1);
        assert_eq!(m.status.contains('1') || m.version == 1, true);

        let m = model_edit_of(&s, &m.id, &["note:text".into()], &[], &[], &[], &[]).unwrap();
        assert!(m.has_draft);
        assert!(m.design_fields.iter().any(|f| f.name == "note" && f.mark == "+"));
        let note = m.design_fields.iter().find(|f| f.name == "note").unwrap();
        let m = model_set_field_of(&s, &m.id, &note.id, "remark:text").unwrap();
        assert!(m.design_fields.iter().any(|f| f.name == "remark" && f.id == note.id));
        model_discard_of(&s, &m.id).unwrap();
        let m = model_of(&s, &m.id).unwrap();
        assert!(!m.has_draft);
        assert!(!m.fields.contains(&"remark".to_string()));

        let hist = model_history_of(&s, &m.id).unwrap();
        assert!(hist.iter().any(|v| v.version == 1 && v.current));
        let msg = model_remove_of(&s, &m.id, true).unwrap();
        assert!(msg.contains("signals"));
    }
}
