//! Model Designer storage: record tables you design field by field and change in
//! versions. See `docs/models.md`.
//!
//! A model has an applied design (version N: the table as it is now) and maybe a draft
//! (the next design, not applied yet). Applying works out a [`Plan`] (the steps from the
//! applied design to the new one, and what each costs in data), rebuilds the table in one
//! savepoint, stamps every row with the new version and keeps each design in
//! `model_versions`, so any version can be looked at again or rolled back to.
//!
//! Tables: model `name` keeps its records in `m_<name>` (`id`, `_version`, `_created_at`,
//! `_updated_at`, `_author`, then one column per field). The built-in `bars` model is the
//! bars table itself: its fields are read-only, and fields added to it live in `mx_bars`,
//! one row per bar keyed by the bar id (following source renames and removals).
//!
//! Field ids stay the same across versions, so a renamed field keeps its values.
use super::{Store, new_id, now_ms};
use crate::error::{Error, Result};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MIGRATION: &str = r#"
CREATE TABLE models (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    builtin INTEGER NOT NULL DEFAULT 0,
    version INTEGER NOT NULL DEFAULT 0,
    design TEXT NOT NULL,
    draft TEXT,
    author_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE model_versions (
    model_id TEXT NOT NULL REFERENCES models(id) ON DELETE CASCADE,
    version INTEGER NOT NULL,
    design TEXT NOT NULL,
    steps TEXT NOT NULL,
    note TEXT NOT NULL DEFAULT '',
    rule_id TEXT,
    author_id TEXT,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (model_id, version)
);
INSERT INTO models(id, name, description, builtin, version, design, created_at, updated_at)
    VALUES ('bars', 'bars', 'imported price bars (built in)', 1, 0, '{"fields":[]}', 0, 0);
"#;

/// Id and name of the built-in bars model.
pub const BARS: &str = "bars";

/// The bars table's own fields (read-only in the bars model).
pub const BAR_FIELDS: &[(&str, &str)] = &[
    ("source", "text"),
    ("symbol", "text"),
    ("base", "text"),
    ("quote", "text"),
    ("period", "text"),
    ("timestamp", "timestamp"),
    ("open", "real"),
    ("high", "real"),
    ("low", "real"),
    ("close", "real"),
    ("volume", "real"),
    ("flags", "integer"),
];

/// Names a field cannot have (besides anything starting with `_`).
pub const RESERVED: &[&str] = &["id", "where", "author"];

/// What a field holds.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub enum FieldType {
    Text,
    Integer,
    Real,
    Bool,
    /// Milliseconds since 1970 (dates and times are accepted on input).
    Timestamp,
    /// Any JSON value.
    Json,
    /// A record of another model (by model id), or of `bars`.
    Ref(String),
}

impl FieldType {
    /// Type names for help texts.
    pub const NAMES: [&'static str; 7] = ["text", "integer", "real", "bool", "timestamp", "json", "ref:MODEL"];
    /// The types without a target (the TUI cycles through these).
    pub const BASIC: [FieldType; 6] = [FieldType::Text, FieldType::Integer, FieldType::Real, FieldType::Bool, FieldType::Timestamp, FieldType::Json];

    /// `text`, `int`, `real`, `bool`, `time`, `json` and their long names.
    pub fn basic(s: &str) -> Option<FieldType> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "text" | "string" | "str" => FieldType::Text,
            "integer" | "int" => FieldType::Integer,
            "real" | "float" | "number" | "num" | "decimal" => FieldType::Real,
            "bool" | "boolean" => FieldType::Bool,
            "timestamp" | "time" | "date" | "datetime" => FieldType::Timestamp,
            "json" => FieldType::Json,
            _ => return None,
        })
    }
    /// Stored form: `text` … `ref:<model id>`.
    pub fn code(&self) -> String {
        match self {
            FieldType::Text => "text".into(),
            FieldType::Integer => "integer".into(),
            FieldType::Real => "real".into(),
            FieldType::Bool => "bool".into(),
            FieldType::Timestamp => "timestamp".into(),
            FieldType::Json => "json".into(),
            FieldType::Ref(m) => format!("ref:{m}"),
        }
    }
    fn sql(&self) -> &'static str {
        match self {
            FieldType::Integer | FieldType::Bool | FieldType::Timestamp => "INTEGER",
            FieldType::Real => "REAL",
            _ => "TEXT",
        }
    }
    pub fn is_number(&self) -> bool {
        matches!(self, FieldType::Integer | FieldType::Real | FieldType::Timestamp)
    }
}

impl From<FieldType> for String {
    fn from(t: FieldType) -> String {
        t.code()
    }
}

impl TryFrom<String> for FieldType {
    type Error = String;
    fn try_from(s: String) -> std::result::Result<Self, String> {
        match s.strip_prefix("ref:") {
            Some(m) if !m.is_empty() => Ok(FieldType::Ref(m.to_string())),
            _ => FieldType::basic(&s).ok_or_else(|| format!("unknown field type `{s}`")),
        }
    }
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// One field of a design.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Field {
    /// Stays the same across versions (a rename keeps the values).
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub ty: FieldType,
    /// Every record has a value (`NOT NULL`).
    #[serde(default, skip_serializing_if = "is_false")]
    pub required: bool,
    /// No two records share a value.
    #[serde(default, skip_serializing_if = "is_false")]
    pub unique: bool,
    /// Indexed for fast lookups (refs always are).
    #[serde(default, skip_serializing_if = "is_false")]
    pub index: bool,
    /// Used when a record is created without the field (and to fill rows on migration).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    /// For text: the only values allowed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub about: String,
}

impl Field {
    pub fn new(name: &str, ty: FieldType) -> Self {
        Field {
            id: new_field_id(),
            name: name.to_string(),
            ty,
            required: false,
            unique: false,
            index: false,
            default: None,
            choices: vec![],
            about: String::new(),
        }
    }
}

pub(crate) fn new_field_id() -> String {
    new_id()[..12].to_string()
}

/// The fields of a model at one version.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Design {
    #[serde(default)]
    pub fields: Vec<Field>,
}

impl Design {
    pub fn field(&self, name: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.name == name)
    }
    pub fn by_id(&self, id: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.id == id)
    }
}

/// A change to a design (the CLI's `model edit`, the rule action `model-edit`). Specs
/// are in the [`crate::designer`] field syntax, e.g. `price:real required default=0`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum FieldOp {
    Add {
        spec: String,
    },
    /// Replace a field's type and constraints (same name, values kept where they fit).
    Change {
        spec: String,
    },
    Drop {
        field: String,
    },
    Rename {
        from: String,
        to: String,
    },
    /// Move a field to a position (1 is first).
    Move {
        field: String,
        to: usize,
    },
}

/// A model: its applied design, and the draft being worked on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub builtin: bool,
    /// The applied version (0: nothing applied yet, no table).
    pub version: i64,
    pub design: Design,
    #[serde(default)]
    pub draft: Option<Design>,
    pub author_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Model {
    pub fn short_id(&self) -> &str {
        super::short(&self.id)
    }
    /// The SQL table the records are in.
    pub fn table(&self) -> String {
        if self.builtin { format!("mx_{}", self.name) } else { format!("m_{}", self.name) }
    }
    /// The draft if there is one, else the applied design.
    pub fn pending(&self) -> &Design {
        self.draft.as_ref().unwrap_or(&self.design)
    }
    pub fn is_bars(&self) -> bool {
        self.id == BARS
    }
}

/// One applied version.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelVersion {
    pub model_id: String,
    pub version: i64,
    pub design: Design,
    pub steps: Vec<Step>,
    #[serde(default)]
    pub note: String,
    /// The rule whose then-action made this version, if one did.
    #[serde(default)]
    pub rule_id: Option<String>,
    pub author_id: Option<String>,
    pub created_at: i64,
}

/// One step of a migration plan (types are stored codes, `ref:<model id>`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum Step {
    CreateTable { table: String },
    AddField { field: String, ty: String, filled: usize, with: Option<Value> },
    DropField { field: String, ty: String, lost: usize },
    RenameField { from: String, to: String },
    ChangeType { field: String, from: String, to: String, lost: usize },
    Require { field: String, on: bool, filled: usize, with: Option<Value> },
    Unique { field: String, on: bool },
    Index { field: String, on: bool },
    Default { field: String, value: Option<Value> },
    Choices { field: String, choices: Vec<String> },
    About { field: String },
    Order { fields: Vec<String> },
}

impl Step {
    /// Values this step throws away.
    pub fn lost(&self) -> usize {
        match self {
            Step::DropField { lost, .. } | Step::ChangeType { lost, .. } => *lost,
            _ => 0,
        }
    }
}

/// What applying a design would do.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Plan {
    pub model: String,
    pub from: i64,
    pub to: i64,
    /// Records now.
    pub rows: usize,
    pub steps: Vec<Step>,
    /// Why it cannot be applied as it is (each with what to do about it).
    pub blocked: Vec<String>,
}

impl Plan {
    /// Values it throws away.
    pub fn lost(&self) -> usize {
        self.steps.iter().map(Step::lost).sum()
    }
    pub fn destructive(&self) -> bool {
        self.lost() > 0
    }
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

/// How to apply.
#[derive(Debug, Clone, Default)]
pub struct ApplyOpts {
    /// Go ahead even if values are lost (asked for on the command line, `-y`).
    pub destructive: bool,
    /// Kept with the version (`rollback to v2`, …).
    pub note: String,
}

const MCOLS: &str = "id, name, description, builtin, version, design, draft, author_id, created_at, updated_at";

/// `Trade Signals` → `trade_signals`.
pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('_') && !out.is_empty() {
            out.push('_');
        }
    }
    out.trim_end_matches('_').to_string()
}

/// A model or field name: `a-z`, `0-9` and `_`, starting with a letter, at most 48.
pub fn check_name(what: &'static str, n: &str) -> Result<()> {
    let ok = n.len() <= 48
        && n.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && n.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if !ok {
        return Err(Error::invalid(what, format!("`{n}`: use a-z, 0-9 and _, starting with a letter (at most 48)")));
    }
    Ok(())
}

pub(crate) fn q(ident: &str) -> String {
    format!("\"{}\"", ident.replace('"', "\"\""))
}

// ---------------------------------------------------------------------------- values

fn num(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        Value::Bool(b) => Some(*b as u8 as f64),
        _ => None,
    }
}

/// A value as a field type, or `None` if it does not fit (migrations count these as lost).
/// Null stays null. Choices are not checked here.
pub fn convert(v: &Value, to: &FieldType) -> Option<Value> {
    if v.is_null() {
        return Some(Value::Null);
    }
    Some(match to {
        FieldType::Text | FieldType::Ref(_) => match v {
            Value::String(s) => Value::String(s.clone()),
            Value::Number(_) | Value::Bool(_) => Value::String(v.to_string()),
            o if matches!(to, FieldType::Text) => Value::String(o.to_string()),
            _ => return None,
        },
        FieldType::Integer => match v {
            Value::Number(n) if n.is_i64() => v.clone(),
            Value::Bool(b) => Value::from(*b as i64),
            o => {
                let f = num(o)?;
                if f.fract() != 0.0 || !f.is_finite() {
                    return None;
                }
                Value::from(f as i64)
            }
        },
        FieldType::Real => Value::from(num(v).filter(|f| f.is_finite())?),
        FieldType::Bool => match v {
            Value::Bool(_) => v.clone(),
            Value::Number(n) => match n.as_f64()? {
                0.0 => Value::Bool(false),
                1.0 => Value::Bool(true),
                _ => return None,
            },
            Value::String(s) => match s.trim().to_ascii_lowercase().as_str() {
                "true" | "yes" | "y" | "on" | "1" => Value::Bool(true),
                "false" | "no" | "n" | "off" | "0" => Value::Bool(false),
                _ => return None,
            },
            _ => return None,
        },
        FieldType::Timestamp => match v {
            Value::Number(n) => Value::from(crate::ingest::time::epoch_to_ms(n.as_f64()? as i64)),
            Value::String(s) => match s.trim().parse::<i64>() {
                Ok(n) => Value::from(crate::ingest::time::epoch_to_ms(n)),
                Err(_) => Value::from(crate::ingest::time::parse_ts(s)?),
            },
            _ => return None,
        },
        FieldType::Json => match v {
            Value::String(s) => serde_json::from_str(s).unwrap_or_else(|_| v.clone()),
            o => o.clone(),
        },
    })
}

/// A value for a field from what a person typed (or a JSON value): converted, choices
/// checked (case-insensitive, stored as written in the design). Empty text is null.
pub fn coerce(f: &Field, v: Value) -> Result<Value> {
    let v = match v {
        Value::String(s) if s.trim().is_empty() && !matches!(f.ty, FieldType::Text) => Value::Null,
        o => o,
    };
    let Some(out) = convert(&v, &f.ty) else {
        let shown = match &v {
            Value::String(s) => s.clone(),
            o => o.to_string(),
        };
        let ty = match &f.ty {
            FieldType::Ref(_) => "a record id".to_string(),
            t => t.code(),
        };
        return Err(Error::invalid("value", format!("{}: `{shown}` is not {ty}", f.name)));
    };
    if let (Value::String(s), false) = (&out, f.choices.is_empty()) {
        return match f.choices.iter().find(|c| c.eq_ignore_ascii_case(s.trim())) {
            Some(c) => Ok(Value::String(c.clone())),
            None => Err(Error::invalid("value", format!("{}: `{s}` is not one of {}", f.name, f.choices.join(", ")))),
        };
    }
    Ok(out)
}

pub(crate) fn to_sql(v: &Value, ty: &FieldType) -> rusqlite::types::Value {
    use rusqlite::types::Value as S;
    match (v, ty) {
        (Value::Null, _) => S::Null,
        (Value::Bool(b), _) => S::Integer(*b as i64),
        (Value::Number(n), FieldType::Real) => S::Real(n.as_f64().unwrap_or(0.0)),
        (Value::Number(n), _) if n.is_i64() => S::Integer(n.as_i64().unwrap_or(0)),
        (Value::Number(n), _) => S::Real(n.as_f64().unwrap_or(0.0)),
        (Value::String(s), FieldType::Json) => S::Text(serde_json::to_string(s).unwrap_or_default()),
        (Value::String(s), _) => S::Text(s.clone()),
        (o, _) => S::Text(o.to_string()),
    }
}

pub(crate) fn from_sql(v: rusqlite::types::Value, ty: &FieldType) -> Value {
    use rusqlite::types::Value as S;
    match (v, ty) {
        (S::Null, _) => Value::Null,
        (S::Integer(i), FieldType::Bool) => Value::Bool(i != 0),
        (S::Integer(i), FieldType::Real) => Value::from(i as f64),
        (S::Integer(i), _) => Value::from(i),
        (S::Real(f), _) => Value::from(f),
        (S::Text(s), FieldType::Json) => serde_json::from_str(&s).unwrap_or(Value::String(s)),
        (S::Text(s), _) => Value::String(s),
        (S::Blob(b), _) => Value::String(hex::encode(b)),
    }
}

// ---------------------------------------------------------------------------- store

impl Store {
    fn row_model(r: &rusqlite::Row) -> rusqlite::Result<Model> {
        let design: String = r.get(5)?;
        let draft: Option<String> = r.get(6)?;
        Ok(Model {
            id: r.get(0)?,
            name: r.get(1)?,
            description: r.get(2)?,
            builtin: r.get::<_, i64>(3)? != 0,
            version: r.get(4)?,
            design: serde_json::from_str(&design).unwrap_or_default(),
            draft: draft.and_then(|d| serde_json::from_str(&d).ok()),
            author_id: r.get(7)?,
            created_at: r.get(8)?,
            updated_at: r.get(9)?,
        })
    }

    /// All models, the built-in ones first.
    pub fn models(&self) -> Result<Vec<Model>> {
        let mut st = self.conn.prepare(&format!("SELECT {MCOLS} FROM models ORDER BY builtin DESC, name"))?;
        Ok(st.query_map([], Self::row_model)?.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn model(&self, id: &str) -> Result<Option<Model>> {
        Ok(self.conn.query_row(&format!("SELECT {MCOLS} FROM models WHERE id = ?1"), [id], Self::row_model).optional()?)
    }

    /// By id, id prefix or name.
    pub fn find_model(&self, key: &str) -> Result<Model> {
        let all = self.models()?;
        let k = key.trim();
        if let Some(m) = all.iter().find(|m| m.name == slug(k) && m.id != k) {
            return Ok(m.clone());
        }
        super::pick(all, k, |m| &m.id, |m| &m.name, "model", "mnnn model list")
    }

    fn model_or_err(&self, id: &str) -> Result<Model> {
        self.model(id)?.ok_or_else(|| Error::invalid("model", format!("`{id}` not found")))
    }

    fn save_model_row(&self, m: &Model) -> Result<()> {
        self.conn.execute(
            "UPDATE models SET name = ?2, description = ?3, version = ?4, design = ?5, draft = ?6, updated_at = ?7 WHERE id = ?1",
            params![
                m.id,
                m.name,
                m.description,
                m.version,
                serde_json::to_string(&m.design)?,
                m.draft.as_ref().map(serde_json::to_string).transpose()?,
                m.updated_at
            ],
        )?;
        Ok(())
    }

    /// Create a model with a design. It is a draft (no table yet) until applied; with
    /// `apply` it is applied at once as version 1.
    pub fn create_model(&self, name: &str, description: &str, design: Design, apply: bool) -> Result<(Model, Option<Plan>)> {
        let name = slug(name);
        check_name("model name", &name)?;
        if self.models()?.iter().any(|m| m.name == name) {
            return Err(Error::invalid("model name", format!("`{name}` already exists")));
        }
        let now = now_ms();
        let m = Model {
            id: new_id(),
            name,
            description: description.trim().to_string(),
            builtin: false,
            version: 0,
            design: Design::default(),
            draft: Some(design),
            author_id: self.current_author_id()?,
            created_at: now,
            updated_at: now,
        };
        self.check_design(&m, m.pending())?;
        self.insert_model(&m)?;
        if apply {
            let (m, p) = self.apply_model(&m.id, &ApplyOpts::default())?;
            return Ok((m, Some(p)));
        }
        Ok((m, None))
    }

    pub(crate) fn insert_model(&self, m: &Model) -> Result<()> {
        self.conn.execute(
            &format!("INSERT INTO models({MCOLS}) VALUES (?1, ?2, ?3, 0, ?4, ?5, ?6, ?7, ?8, ?9)"),
            params![
                m.id,
                m.name,
                m.description,
                m.version,
                serde_json::to_string(&m.design)?,
                m.draft.as_ref().map(serde_json::to_string).transpose()?,
                m.author_id,
                m.created_at,
                m.updated_at
            ],
        )?;
        Ok(())
    }

    /// Rename a model (its table too) and/or change its description. Not versioned.
    pub fn rename_model(&self, id: &str, name: Option<&str>, description: Option<&str>) -> Result<Model> {
        let mut m = self.model_or_err(id)?;
        if let Some(n) = name {
            let n = slug(n);
            if n != m.name {
                if m.builtin {
                    return Err(Error::invalid("model", format!("{} is built in and keeps its name", m.name)));
                }
                check_name("model name", &n)?;
                if self.models()?.iter().any(|o| o.name == n) {
                    return Err(Error::invalid("model name", format!("`{n}` already exists")));
                }
                let old = m.table();
                m.name = n;
                if self.table_exists(&old)? {
                    self.conn.execute_batch(&format!("ALTER TABLE {} RENAME TO {}", q(&old), q(&m.table())))?;
                }
            }
        }
        if let Some(d) = description {
            m.description = d.trim().to_string();
        }
        m.updated_at = now_ms();
        self.save_model_row(&m)?;
        Ok(m)
    }

    /// Replace the draft (`None` discards it). The design is checked first.
    pub fn set_model_draft(&self, id: &str, draft: Option<Design>) -> Result<Model> {
        let mut m = self.model_or_err(id)?;
        if let Some(d) = &draft {
            self.check_design(&m, d)?;
        }
        m.draft = draft.filter(|d| *d != m.design);
        m.updated_at = now_ms();
        self.save_model_row(&m)?;
        Ok(m)
    }

    /// Apply field changes to the draft (starting from the applied design if there is none).
    pub fn edit_model(&self, id: &str, ops: &[FieldOp]) -> Result<Model> {
        let m = self.model_or_err(id)?;
        let d = crate::designer::apply_ops(self, &m, m.pending(), ops)?;
        self.set_model_draft(id, Some(d))
    }

    /// Is a design sound? Names, duplicates, refs, choices and defaults.
    pub fn check_design(&self, m: &Model, d: &Design) -> Result<()> {
        let mut names = std::collections::HashSet::new();
        let mut ids = std::collections::HashSet::new();
        for f in &d.fields {
            check_name("field name", &f.name)?;
            if RESERVED.contains(&f.name.as_str()) {
                return Err(Error::invalid("field name", format!("`{}` is reserved ({})", f.name, RESERVED.join(", "))));
            }
            if m.is_bars() && BAR_FIELDS.iter().any(|(n, _)| *n == f.name) {
                return Err(Error::invalid("field name", format!("`{}` is a built-in bars field", f.name)));
            }
            if !names.insert(f.name.clone()) {
                return Err(Error::invalid("design", format!("two fields are named `{}`", f.name)));
            }
            if f.id.is_empty() || !ids.insert(f.id.clone()) {
                return Err(Error::invalid("design", format!("field `{}` has no id or a duplicate one", f.name)));
            }
            if m.is_bars() && f.required {
                return Err(Error::invalid("design", format!("{}: fields added to bars cannot be required (give a default)", f.name)));
            }
            if let FieldType::Ref(t) = &f.ty
                && t != &m.id
                && self.model(t)?.is_none()
            {
                return Err(Error::invalid("design", format!("{}: refers to a model that does not exist ({t})", f.name)));
            }
            if !f.choices.is_empty() && f.ty != FieldType::Text {
                return Err(Error::invalid("design", format!("{}: only text fields take choices", f.name)));
            }
            if f.unique && f.ty == FieldType::Json {
                return Err(Error::invalid("design", format!("{}: json fields cannot be unique", f.name)));
            }
            if let Some(v) = &f.default {
                let ok = coerce(f, v.clone())?;
                if ok.is_null() {
                    return Err(Error::invalid("design", format!("{}: the default is empty", f.name)));
                }
            }
        }
        Ok(())
    }

    pub(crate) fn table_exists(&self, t: &str) -> Result<bool> {
        Ok(self.conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1", [t], |r| r.get::<_, i64>(0))? > 0)
    }

    /// Stored rows: (id, meta, values by field id) — what migrations work on.
    fn raw_rows(&self, m: &Model) -> Result<Vec<RawRow>> {
        let t = m.table();
        if m.version == 0 || !self.table_exists(&t)? {
            return Ok(vec![]);
        }
        let cols: Vec<String> = m.design.fields.iter().map(|f| q(&f.name)).collect();
        let sel = if cols.is_empty() { String::new() } else { format!(", {}", cols.join(", ")) };
        let mut st = self.conn.prepare(&format!("SELECT id, _created_at, _updated_at, _author{sel} FROM {}", q(&t)))?;
        let n = m.design.fields.len();
        let rows = st
            .query_map([], |r| {
                let mut values = Vec::with_capacity(n);
                for (i, f) in m.design.fields.iter().enumerate() {
                    values.push((f.id.clone(), from_sql(r.get(4 + i)?, &f.ty)));
                }
                let id = match r.get::<_, rusqlite::types::Value>(0)? {
                    rusqlite::types::Value::Blob(b) => hex::encode(b),
                    rusqlite::types::Value::Text(t) => t,
                    o => format!("{o:?}"),
                };
                Ok(RawRow { id, created_at: r.get(1)?, updated_at: r.get(2)?, author: r.get(3)?, values })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// What applying `to` to a model would do (nothing is changed).
    pub fn plan_model(&self, m: &Model, to: &Design) -> Result<Plan> {
        let rows = self.raw_rows(m)?;
        Ok(plan_rows(m, to, &rows))
    }

    /// Apply the draft as the next version.
    pub fn apply_model(&self, id: &str, opts: &ApplyOpts) -> Result<(Model, Plan)> {
        let m = self.model_or_err(id)?;
        let Some(d) = m.draft.clone() else {
            if m.version == 0 && !m.builtin {
                return self.apply_design(&m, &Design::default(), opts);
            }
            return Err(Error::invalid("model", format!("{} has no changes to apply (edit it first)", m.name)));
        };
        self.apply_design(&m, &d, opts)
    }

    /// Apply the design of an earlier version as the next version.
    pub fn rollback_model(&self, id: &str, to: i64, opts: &ApplyOpts) -> Result<(Model, Plan)> {
        let m = self.model_or_err(id)?;
        if to < 1 || to >= m.version {
            return Err(Error::invalid("version", format!("{} is at v{}; roll back to v1..v{}", m.name, m.version, m.version - 1)));
        }
        let v =
            self.model_versions(id)?.into_iter().find(|v| v.version == to).ok_or_else(|| Error::invalid("version", format!("v{to} not found")))?;
        let mut opts = opts.clone();
        if opts.note.is_empty() {
            opts.note = format!("rollback to v{to}");
        }
        self.apply_design(&m, &v.design, &opts)
    }

    /// The plan for rolling back to a version (what `rollback --dry-run` shows).
    pub fn rollback_plan(&self, id: &str, to: i64) -> Result<Plan> {
        let m = self.model_or_err(id)?;
        let v =
            self.model_versions(id)?.into_iter().find(|v| v.version == to).ok_or_else(|| Error::invalid("version", format!("v{to} not found")))?;
        self.plan_model(&m, &v.design)
    }

    pub(crate) fn apply_design(&self, m: &Model, to: &Design, opts: &ApplyOpts) -> Result<(Model, Plan)> {
        self.check_design(m, to)?;
        let rows = self.raw_rows(m)?;
        let plan = plan_rows(m, to, &rows);
        if !plan.blocked.is_empty() {
            return Err(Error::invalid("migration", plan.blocked.join("; ")));
        }
        if plan.is_empty() && m.version > 0 {
            return Err(Error::invalid("model", format!("{} already has this design (v{})", m.name, m.version)));
        }
        if plan.destructive() && !opts.destructive {
            return Err(Error::NeedsConfirmation(format!("applying {} v{} loses {} stored value(s)", m.name, plan.to, plan.lost())));
        }
        let version = m.version + 1;
        let author = self.current_author_id()?;
        let rule = self.rules_rt.chain.borrow().last().cloned();
        self.conn.execute_batch("SAVEPOINT model_apply")?;
        let r = (|| -> Result<()> {
            self.rebuild(m, to, version, &rows)?;
            let now = now_ms();
            self.conn.execute(
                "UPDATE models SET version = ?2, design = ?3, draft = NULL, updated_at = ?4 WHERE id = ?1",
                params![m.id, version, serde_json::to_string(to)?, now],
            )?;
            self.conn.execute(
                "INSERT INTO model_versions(model_id, version, design, steps, note, rule_id, author_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![m.id, version, serde_json::to_string(to)?, serde_json::to_string(&plan.steps)?, opts.note, rule, author, now],
            )?;
            Ok(())
        })();
        match r {
            Ok(()) => self.conn.execute_batch("RELEASE model_apply")?,
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK TO model_apply; RELEASE model_apply");
                return Err(e);
            }
        }
        Ok((self.model_or_err(&m.id)?, plan))
    }

    /// Build the table for `to` and copy the rows over (converted, filled, stamped).
    pub(crate) fn rebuild(&self, m: &Model, to: &Design, version: i64, rows: &[RawRow]) -> Result<()> {
        let t = m.table();
        let new = format!("{t}__new");
        let id_col = if m.is_bars() { "id BLOB PRIMARY KEY REFERENCES bars(id) ON DELETE CASCADE ON UPDATE CASCADE" } else { "id TEXT PRIMARY KEY" };
        let mut cols = vec![
            id_col.to_string(),
            "_version INTEGER NOT NULL".into(),
            "_created_at INTEGER NOT NULL".into(),
            "_updated_at INTEGER NOT NULL".into(),
            "_author TEXT".into(),
        ];
        for f in &to.fields {
            let mut c = format!("{} {}", q(&f.name), f.ty.sql());
            if f.required {
                c.push_str(" NOT NULL");
            }
            if f.unique {
                c.push_str(" UNIQUE");
            }
            cols.push(c);
        }
        self.conn.execute_batch(&format!("DROP TABLE IF EXISTS {}; CREATE TABLE {} ({})", q(&new), q(&new), cols.join(", ")))?;
        {
            let names: Vec<String> = to.fields.iter().map(|f| q(&f.name)).collect();
            let marks: Vec<String> = (0..5 + to.fields.len()).map(|i| format!("?{}", i + 1)).collect();
            let extra = if names.is_empty() { String::new() } else { format!(", {}", names.join(", ")) };
            let mut ins = self.conn.prepare(&format!(
                "INSERT INTO {}(id, _version, _created_at, _updated_at, _author{extra}) VALUES ({})",
                q(&new),
                marks.join(", ")
            ))?;
            for r in rows {
                let mut vals: Vec<rusqlite::types::Value> = vec![
                    if m.is_bars() { rusqlite::types::Value::Blob(hex::decode(&r.id).unwrap_or_default()) } else { r.id.clone().into() },
                    version.into(),
                    r.created_at.into(),
                    r.updated_at.into(),
                    r.author.clone().map(Into::into).unwrap_or(rusqlite::types::Value::Null),
                ];
                for f in &to.fields {
                    let old = r.values.iter().find(|(id, _)| *id == f.id).map(|(_, v)| v.clone()).unwrap_or(Value::Null);
                    let mut v = convert(&old, &f.ty).unwrap_or(Value::Null);
                    if v.is_null()
                        && let Some(d) = &f.default
                        && (f.required || m.design.by_id(&f.id).is_none())
                    {
                        v = coerce(f, d.clone())?;
                    }
                    vals.push(to_sql(&v, &f.ty));
                }
                ins.execute(rusqlite::params_from_iter(vals))?;
            }
        }
        self.conn.execute_batch(&format!("DROP TABLE IF EXISTS {}; ALTER TABLE {} RENAME TO {}", q(&t), q(&new), q(&t)))?;
        for f in &to.fields {
            if f.index || matches!(f.ty, FieldType::Ref(_)) {
                self.conn.execute_batch(&format!("CREATE INDEX {} ON {}({})", q(&format!("ix_{t}_{}", f.id)), q(&t), q(&f.name)))?;
            }
        }
        Ok(())
    }

    /// Versions, oldest first.
    pub fn model_versions(&self, id: &str) -> Result<Vec<ModelVersion>> {
        let mut st = self.conn.prepare(
            "SELECT model_id, version, design, steps, note, rule_id, author_id, created_at FROM model_versions WHERE model_id = ?1 ORDER BY version",
        )?;
        let v = st
            .query_map([id], |r| {
                let d: String = r.get(2)?;
                let s: String = r.get(3)?;
                Ok(ModelVersion {
                    model_id: r.get(0)?,
                    version: r.get(1)?,
                    design: serde_json::from_str(&d).unwrap_or_default(),
                    steps: serde_json::from_str(&s).unwrap_or_default(),
                    note: r.get(4)?,
                    rule_id: r.get(5)?,
                    author_id: r.get(6)?,
                    created_at: r.get(7)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub(crate) fn insert_model_version(&self, v: &ModelVersion) -> Result<()> {
        self.conn.execute(
            "INSERT INTO model_versions(model_id, version, design, steps, note, rule_id, author_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![v.model_id, v.version, serde_json::to_string(&v.design)?, serde_json::to_string(&v.steps)?, v.note, v.rule_id, v.author_id, v.created_at],
        )?;
        Ok(())
    }

    /// Models with a field referring to this one.
    pub fn models_referring_to(&self, id: &str) -> Result<Vec<Model>> {
        Ok(self.models()?.into_iter().filter(|m| m.id != id && m.design.fields.iter().any(|f| f.ty == FieldType::Ref(id.to_string()))).collect())
    }

    /// Remove a model, its table and its versions. Losing records needs `destructive`;
    /// models other models refer to stay. Its rules become standalone (switched off).
    pub fn remove_model(&self, id: &str, destructive: bool) -> Result<(Model, usize)> {
        let m = self.model_or_err(id)?;
        if m.builtin {
            return Err(Error::invalid(
                "model",
                format!("{} is built in; drop the fields you added with `mnnn model edit {} --drop FIELD`", m.name, m.name),
            ));
        }
        let refs = self.models_referring_to(&m.id)?;
        if !refs.is_empty() {
            let n: Vec<String> = refs.into_iter().map(|r| r.name).collect();
            return Err(Error::invalid("model", format!("{} is referred to by {}; change those fields first", m.name, n.join(", "))));
        }
        let rows = self.count_records(&m)?;
        if rows > 0 && !destructive {
            return Err(Error::NeedsConfirmation(format!("removing {} deletes its {rows} record(s)", m.name)));
        }
        self.conn.execute_batch("SAVEPOINT model_remove")?;
        let r = (|| -> Result<()> {
            self.conn.execute_batch(&format!("DROP TABLE IF EXISTS {}", q(&m.table())))?;
            self.conn.execute("DELETE FROM project_items WHERE kind = 'model' AND ref_id = ?1", [&m.id])?;
            self.orphan_rules(super::OwnerKind::Model, &m.id)?;
            self.conn.execute("DELETE FROM models WHERE id = ?1", [&m.id])?;
            Ok(())
        })();
        match r {
            Ok(()) => self.conn.execute_batch("RELEASE model_remove")?,
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK TO model_remove; RELEASE model_remove");
                return Err(e);
            }
        }
        Ok((m, rows))
    }
}

/// A stored row as migrations see it.
#[derive(Debug, Clone)]
pub(crate) struct RawRow {
    pub id: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub author: Option<String>,
    /// (field id, value)
    pub values: Vec<(String, Value)>,
}

fn plan_rows(m: &Model, to: &Design, rows: &[RawRow]) -> Plan {
    let from = &m.design;
    let mut steps = vec![];
    let mut blocked = vec![];
    let n = rows.len();
    let fresh = m.version == 0;
    if fresh {
        steps.push(Step::CreateTable { table: m.table() });
    }
    let value = |r: &RawRow, id: &str| r.values.iter().find(|(i, _)| i == id).map(|(_, v)| v.clone()).unwrap_or(Value::Null);
    for f in &from.fields {
        if to.by_id(&f.id).is_none() {
            let lost = rows.iter().filter(|r| !value(r, &f.id).is_null()).count();
            steps.push(Step::DropField { field: f.name.clone(), ty: f.ty.code(), lost });
        }
    }
    for f in &to.fields {
        let Some(old) = from.by_id(&f.id).filter(|_| !fresh) else {
            let filled = if f.default.is_some() { n } else { 0 };
            steps.push(Step::AddField { field: f.name.clone(), ty: f.ty.code(), filled, with: f.default.clone() });
            if f.required && f.default.is_none() && n > 0 {
                blocked.push(format!("{} is required but the {n} record(s) have no value: give it a default", f.name));
            }
            if f.unique && f.default.is_some() && n > 1 {
                blocked.push(format!("{} is unique but its default would give {n} records the same value", f.name));
            }
            continue;
        };
        if old.name != f.name {
            steps.push(Step::RenameField { from: old.name.clone(), to: f.name.clone() });
        }
        let converted: Vec<Value> = rows.iter().map(|r| convert(&value(r, &f.id), &f.ty).unwrap_or(Value::Null)).collect();
        if old.ty != f.ty {
            let lost = rows.iter().zip(&converted).filter(|(r, c)| !value(r, &f.id).is_null() && c.is_null()).count();
            steps.push(Step::ChangeType { field: f.name.clone(), from: old.ty.code(), to: f.ty.code(), lost });
        }
        let empty = converted.iter().filter(|v| v.is_null()).count();
        if old.required != f.required {
            let filled = if f.required && f.default.is_some() { empty } else { 0 };
            steps.push(Step::Require { field: f.name.clone(), on: f.required, filled, with: f.default.clone().filter(|_| filled > 0) });
        }
        if f.required && empty > 0 && f.default.is_none() {
            blocked.push(format!("{} is required but {empty} record(s) would have no value: give it a default first", f.name));
        }
        if old.unique != f.unique {
            steps.push(Step::Unique { field: f.name.clone(), on: f.unique });
        }
        if f.unique {
            let mut seen = std::collections::HashSet::new();
            let dups = converted.iter().filter(|v| !v.is_null()).filter(|v| !seen.insert(v.to_string())).count();
            if dups > 0 {
                blocked.push(format!("{} would be unique but {dups} record(s) repeat a value", f.name));
            }
        }
        if old.index != f.index {
            steps.push(Step::Index { field: f.name.clone(), on: f.index });
        }
        if old.default != f.default {
            steps.push(Step::Default { field: f.name.clone(), value: f.default.clone() });
        }
        if old.choices != f.choices {
            steps.push(Step::Choices { field: f.name.clone(), choices: f.choices.clone() });
            if !f.choices.is_empty() {
                let bad = converted.iter().filter(|v| v.as_str().is_some_and(|s| !f.choices.iter().any(|c| c.eq_ignore_ascii_case(s)))).count();
                if bad > 0 {
                    blocked.push(format!("{}: {bad} record(s) hold a value that is not one of {}", f.name, f.choices.join(", ")));
                }
            }
        }
        if old.about != f.about {
            steps.push(Step::About { field: f.name.clone() });
        }
    }
    if !fresh {
        let kept_old: Vec<&str> = from.fields.iter().filter(|f| to.by_id(&f.id).is_some()).map(|f| f.id.as_str()).collect();
        let kept_new: Vec<&str> = to.fields.iter().filter(|f| from.by_id(&f.id).is_some()).map(|f| f.id.as_str()).collect();
        if kept_old != kept_new {
            steps.push(Step::Order { fields: to.fields.iter().map(|f| f.name.clone()).collect() });
        }
    }
    Plan { model: m.name.clone(), from: m.version, to: m.version + 1, rows: n, steps, blocked }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn f(name: &str, ty: FieldType) -> Field {
        Field::new(name, ty)
    }

    #[test]
    fn values_convert_and_coerce() {
        assert_eq!(convert(&json!("3"), &FieldType::Integer), Some(json!(3)));
        assert_eq!(convert(&json!(2.5), &FieldType::Integer), None);
        assert_eq!(convert(&json!("yes"), &FieldType::Bool), Some(json!(true)));
        assert_eq!(convert(&json!("2024-01-02"), &FieldType::Timestamp), Some(json!(1704153600000i64)));
        assert_eq!(convert(&json!(1704153600), &FieldType::Timestamp), Some(json!(1704153600000i64)), "seconds become ms");
        assert_eq!(convert(&json!("{\"a\":1}"), &FieldType::Json), Some(json!({"a": 1})));
        assert_eq!(convert(&json!(7), &FieldType::Text), Some(json!("7")));
        let mut side = f("side", FieldType::Text);
        side.choices = vec!["buy".into(), "sell".into()];
        assert_eq!(coerce(&side, json!("BUY")).unwrap(), json!("buy"));
        assert!(coerce(&side, json!("hold")).unwrap_err().to_string().contains("not one of buy, sell"));
        assert_eq!(coerce(&f("n", FieldType::Real), json!("")).unwrap(), Value::Null);
        assert_eq!(slug(" Trade Signals! "), "trade_signals");
        assert!(check_name("x", "9a").is_err() && check_name("x", "ok_1").is_ok());
    }

    #[test]
    fn create_apply_migrate_and_roll_back() {
        let s = Store::open_in_memory().unwrap();
        let mut price = f("price", FieldType::Real);
        price.required = true;
        let note = f("note", FieldType::Text);
        let (m, plan) = s.create_model("Signals", "", Design { fields: vec![price.clone(), note.clone()] }, true).unwrap();
        assert_eq!((m.name.as_str(), m.version, m.draft.is_none()), ("signals", 1, true));
        assert!(matches!(plan.unwrap().steps[0], Step::CreateTable { .. }));
        assert!(s.table_exists("m_signals").unwrap());
        s.conn
            .execute(
                "INSERT INTO m_signals(id, _version, _created_at, _updated_at, price, note) VALUES ('a', 1, 0, 0, 1.5, 'x'), ('b', 1, 0, 0, 2, NULL)",
                [],
            )
            .unwrap();

        // rename note, drop nothing, add a required field with a default
        let mut d = m.design.clone();
        d.fields[1].name = "comment".into();
        let mut size = f("size", FieldType::Integer);
        size.required = true;
        d.fields.push(size.clone());
        let p = s.plan_model(&m, &d).unwrap();
        assert!(p.blocked[0].contains("size is required"), "{:?}", p.blocked);
        d.fields[2].default = Some(json!(1));
        let p = s.plan_model(&m, &d).unwrap();
        assert!(p.blocked.is_empty() && !p.destructive(), "{p:?}");
        assert!(p.steps.contains(&Step::RenameField { from: "note".into(), to: "comment".into() }));
        s.set_model_draft(&m.id, Some(d.clone())).unwrap();
        let (m2, _) = s.apply_model(&m.id, &ApplyOpts::default()).unwrap();
        assert_eq!(m2.version, 2);
        let rows: Vec<(String, Option<String>, i64, i64)> = s
            .conn
            .prepare("SELECT id, comment, size, _version FROM m_signals ORDER BY id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(rows, vec![("a".into(), Some("x".into()), 1, 2), ("b".into(), None, 1, 2)], "renamed values kept, rows stamped v2");

        // dropping a field with values is destructive: it needs confirming
        let mut d3 = m2.design.clone();
        d3.fields.retain(|f| f.name != "comment");
        s.set_model_draft(&m.id, Some(d3)).unwrap();
        assert!(matches!(s.apply_model(&m.id, &ApplyOpts::default()), Err(Error::NeedsConfirmation(_))));
        let (m3, p3) = s.apply_model(&m.id, &ApplyOpts { destructive: true, ..Default::default() }).unwrap();
        assert_eq!((m3.version, p3.lost()), (3, 1));

        // roll back to v2: the field comes back empty (its values are gone)
        let p = s.rollback_plan(&m.id, 2).unwrap();
        assert!(matches!(&p.steps[0], Step::AddField { field, .. } if field == "comment"));
        let (m4, _) = s.rollback_model(&m.id, 2, &ApplyOpts::default()).unwrap();
        assert_eq!(m4.version, 4);
        let vs = s.model_versions(&m.id).unwrap();
        assert_eq!(vs.len(), 4);
        assert_eq!(vs[3].note, "rollback to v2");
        assert!(s.rollback_model(&m.id, 4, &ApplyOpts::default()).is_err());

        // types: real → integer loses 1.5
        let mut d5 = m4.design.clone();
        d5.fields[0].ty = FieldType::Integer;
        let p = s.plan_model(&m4, &d5).unwrap();
        assert_eq!(p.lost(), 1);
        assert!(p.blocked[0].contains("price is required"), "1.5 becomes empty: {:?}", p.blocked);
    }

    #[test]
    fn designs_are_checked_and_bars_is_protected() {
        let s = Store::open_in_memory().unwrap();
        let bars = s.find_model("bars").unwrap();
        assert!(bars.builtin && bars.version == 0);
        assert!(s.remove_model("bars", true).is_err());
        assert!(s.rename_model("bars", Some("x"), None).is_err());
        let bad = Design { fields: vec![f("close", FieldType::Real)] };
        assert!(s.set_model_draft("bars", Some(bad)).unwrap_err().to_string().contains("built-in"));
        let mut req = f("score", FieldType::Real);
        req.required = true;
        assert!(s.set_model_draft("bars", Some(Design { fields: vec![req] })).is_err());
        s.set_model_draft("bars", Some(Design { fields: vec![f("score", FieldType::Real)] })).unwrap();
        let (b, _) = s.apply_model("bars", &ApplyOpts::default()).unwrap();
        assert_eq!(b.version, 1);
        assert!(s.table_exists("mx_bars").unwrap());

        assert!(s.create_model("x", "", Design { fields: vec![f("id", FieldType::Text)] }, false).is_err(), "reserved");
        assert!(s.create_model("x", "", Design { fields: vec![f("a", FieldType::Ref("nope".into()))] }, false).is_err());
        let (a, _) = s.create_model("a", "", Design { fields: vec![f("n", FieldType::Text)] }, true).unwrap();
        let (_b, _) = s.create_model("b", "", Design { fields: vec![f("a", FieldType::Ref(a.id.clone()))] }, true).unwrap();
        assert!(s.remove_model(&a.id, true).unwrap_err().to_string().contains("referred to by b"));
        assert!(s.create_model("a", "", Design::default(), false).is_err(), "name taken");
        let r = s.rename_model(&a.id, Some("Alpha"), Some("first")).unwrap();
        assert_eq!(r.name, "alpha");
        assert!(s.table_exists("m_alpha").unwrap() && !s.table_exists("m_a").unwrap());
    }
}
