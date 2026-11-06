//! Model and record viewer (`mnnn model` / TUI Data ▸ Model designer…).
//! Browse designs and rows so the app can open a found-records hit into a pane.
//! Schema edits, migrations and rules stay in the TUI and CLI.
use super::data::open;
use muninn_core::store::{Model, Record, RecordQuery, Store};
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

fn model_info(store: &Store, m: &Model) -> ModelInfo {
    ModelInfo {
        id: m.id.clone(),
        short_id: m.short_id().to_string(),
        name: m.name.clone(),
        description: m.description.clone(),
        builtin: m.builtin,
        version: m.version,
        is_bars: m.is_bars(),
        fields: m.design.fields.iter().map(|f| f.name.clone()).collect(),
        record_count: store.count_records(m).unwrap_or(0) as u32,
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

        let rows = model_records_of(&s, &m.name, Some("alpha"), None).unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].fields.iter().any(|f| f.name == "memo" && f.value.contains("alpha")), "{:?}", rows[0].fields);
        assert!(model_records_of(&s, &m.name, Some("no-such"), None).unwrap().is_empty());

        let one = model_record_of(&s, &m.id, &r.id).unwrap();
        assert_eq!(one.id, r.id);
        assert_eq!(one.model_id, m.id);
        assert!(one.fields.iter().any(|f| f.name == "memo" && f.value == "alpha signal"));
    }
}
