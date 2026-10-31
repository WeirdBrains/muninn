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
//! bars table itself. Its built-in fields (ids `bar.KEY`) read the bars table's columns and
//! can be renamed, moved, constrained, retyped and dropped like any field: a retyped one
//! is stored in the bars table under its name (converted), a dropped one leaves the model while the bars keep
//! their imported values. Fields added to it are columns of the same table (`m_bars`).
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

/// 2.16: the bars live in the bars model's own table (`m_bars`), see [`upgrade_bars_2_16`].
pub const MIGRATION_2_16: &str = "-- 2.16: bars move into the bars model table (done in Rust)";

/// The bars model's table: every bar, its own columns (`_base` … `_flags`, named with a
/// leading `_` so they never meet a field's column) and the columns of the fields stored
/// in the model.
pub const BARS_TABLE: &str = "m_bars";

/// The bars' own columns in [`BARS_TABLE`], with their SQL types.
const BAR_CORE: &str = "_base TEXT NOT NULL, _quote TEXT NOT NULL, _timestamp INTEGER NOT NULL, _open REAL NOT NULL, _high REAL NOT NULL,
    _low REAL NOT NULL, _close REAL NOT NULL, _volume REAL NOT NULL, _source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    _duration_s INTEGER NOT NULL, _flags INTEGER NOT NULL DEFAULT 0";
const BAR_CORE_COLS: &str = "_base, _quote, _timestamp, _open, _high, _low, _close, _volume, _source_id, _duration_s, _flags";

/// `bar_rows`: the bars' own columns under their plain names, for queries.
const BAR_VIEW: &str = "DROP VIEW IF EXISTS bar_rows; CREATE VIEW bar_rows AS SELECT id, _base AS base, _quote AS quote, _timestamp AS timestamp,
    _open AS open, _high AS high, _low AS low, _close AS close, _volume AS volume, _source_id AS source_id, _duration_s AS duration_s,
    _flags AS flags FROM m_bars;";

/// The 2.16 upgrade, inside the migration transaction: the bars move into `m_bars` (ids,
/// values, sources, periods, timestamps and flags as they are); the values of fields added
/// to bars (the old `mx_bars`) wait in `_mx_bars_upgrade` for [`Store::finish_bars_upgrade`],
/// which knows the design.
pub(crate) fn upgrade_bars_2_16(c: &rusqlite::Connection) -> Result<()> {
    c.execute_batch(&format!(
        "CREATE TABLE m_bars (id BLOB PRIMARY KEY, _version INTEGER, _created_at INTEGER, _updated_at INTEGER, _author TEXT, {BAR_CORE}) WITHOUT ROWID;
         INSERT INTO m_bars(id, {BAR_CORE_COLS}) SELECT id, base, quote, timestamp, open, high, low, close, volume, source_id, duration_s, flags FROM bars;"
    ))?;
    let mx: i64 = c.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'mx_bars'", [], |r| r.get(0))?;
    if mx > 0 {
        c.execute_batch("CREATE TABLE _mx_bars_upgrade AS SELECT * FROM mx_bars; DROP TABLE mx_bars;")?;
    }
    c.execute_batch(&format!("DROP TABLE bars; CREATE INDEX bars_series ON m_bars(_base, _quote, _source_id, _duration_s, _timestamp); {BAR_VIEW}"))?;
    Ok(())
}

/// Id and name of the built-in bars model.
pub const BARS: &str = "bars";

/// The bars table's own fields (the built-in fields of the bars model, as shipped).
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

/// Field ids of the built-in bars fields: `bar.` and the column key (`bar.close`).
pub const BAR_ID_PREFIX: &str = "bar.";

/// The built-in bars fields as shipped, as fields of the bars model.
pub fn bar_builtins() -> Vec<Field> {
    BAR_FIELDS
        .iter()
        .map(|(k, t)| Field {
            required: true,
            about: "from imports".into(),
            id: format!("{BAR_ID_PREFIX}{k}"),
            ..Field::new(k, FieldType::basic(t).expect("bar field types are basic"))
        })
        .collect()
}

/// The bars column a field reads: a built-in bars field with its shipped type. A built-in
/// field given another type is stored in `m_bars` like an added field.
pub fn bar_key(f: &Field) -> Option<&'static str> {
    let k = f.id.strip_prefix(BAR_ID_PREFIX)?;
    BAR_FIELDS.iter().find(|(n, t)| *n == k && FieldType::basic(t).as_ref() == Some(&f.ty)).map(|(n, _)| *n)
}

/// The SQL expression of a bars column (`b` is bars, `s` its source).
pub(crate) fn bar_sql(key: &str) -> &'static str {
    match key {
        "source" => "s.name",
        "symbol" => "(b.base || '/' || b.quote)",
        "base" => "b.base",
        "quote" => "b.quote",
        "period" => "b.duration_s",
        "timestamp" => "b.timestamp",
        "open" => "b.open",
        "high" => "b.high",
        "low" => "b.low",
        "close" => "b.close",
        "volume" => "b.volume",
        _ => "b.flags",
    }
}

/// A bars column value read with [`bar_sql`] as the field shows it (`period` as `5m`).
pub(crate) fn bar_value(key: &str, v: rusqlite::types::Value) -> Value {
    use rusqlite::types::Value as S;
    match (key, v) {
        ("period", S::Integer(d)) => Value::String(crate::model::Duration(d).to_string()),
        (_, S::Null) => Value::Null,
        (_, S::Integer(i)) if matches!(key, "open" | "high" | "low" | "close" | "volume") => Value::from(i as f64),
        (_, S::Integer(i)) => Value::from(i),
        (_, S::Real(f)) => Value::from(f),
        (_, S::Text(t)) => Value::String(t),
        (_, S::Blob(b)) => Value::String(hex::encode(b)),
    }
}

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
    /// bars only: the design lists the built-in fields itself (designs from before 2.5.1
    /// did not; [`Design::with_builtins`] puts them in front).
    #[serde(default, skip_serializing_if = "is_false")]
    pub builtins: bool,
}

impl Design {
    /// A bars design with its built-in fields listed (a no-op once they are).
    pub fn with_builtins(mut self) -> Design {
        if !self.builtins {
            let mut f = bar_builtins();
            f.retain(|b| !self.fields.iter().any(|o| o.name == b.name || o.id == b.id));
            f.append(&mut self.fields);
            self.fields = f;
            self.builtins = true;
        }
        self
    }
    /// Fields kept in the model's own table (for bars: all but the built-in ones that read
    /// the bars table).
    pub fn stored(&self) -> impl Iterator<Item = &Field> {
        self.fields.iter().filter(|f| bar_key(f).is_none())
    }
    /// The field reading a bars column, if the design has one.
    pub fn bar_field(&self, key: &str) -> Option<&Field> {
        self.fields.iter().find(|f| bar_key(f) == Some(key))
    }
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
        if self.is_bars() { BARS_TABLE.to_string() } else { format!("m_{}", self.name) }
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
    CreateTable {
        table: String,
    },
    AddField {
        field: String,
        ty: String,
        filled: usize,
        with: Option<Value>,
    },
    DropField {
        field: String,
        ty: String,
        lost: usize,
    },
    RenameField {
        from: String,
        to: String,
    },
    ChangeType {
        field: String,
        from: String,
        to: String,
        lost: usize,
    },
    Require {
        field: String,
        on: bool,
        filled: usize,
        with: Option<Value>,
    },
    Unique {
        field: String,
        on: bool,
    },
    Index {
        field: String,
        on: bool,
    },
    Default {
        field: String,
        value: Option<Value>,
    },
    Choices {
        field: String,
        choices: Vec<String>,
    },
    About {
        field: String,
    },
    Order {
        fields: Vec<String>,
    },
    /// bars: a built-in field leaves the model; the bars keep their imported values.
    HideField {
        field: String,
    },
    /// bars: a built-in field comes (back) into the model with the bars' values.
    ShowField {
        field: String,
    },
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
        let id: String = r.get(0)?;
        let fix = |d: Design| if id == BARS { d.with_builtins() } else { d };
        Ok(Model {
            design: fix(serde_json::from_str(&design).unwrap_or_default()),
            draft: draft.and_then(|d| serde_json::from_str(&d).ok()).map(fix),
            id,
            name: r.get(1)?,
            description: r.get(2)?,
            builtin: r.get::<_, i64>(3)? != 0,
            version: r.get(4)?,
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
                check_name("model name", &n)?;
                if self.models()?.iter().any(|o| o.name == n) {
                    return Err(Error::invalid("model name", format!("`{n}` already exists")));
                }
                let old = m.table();
                m.name = n;
                if old != m.table() && self.table_exists(&old)? {
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
        // a bars design without its built-in fields (as before 2.5.1) keeps them as they were shipped
        let draft = draft.map(|d| if m.is_bars() { d.with_builtins() } else { d });
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
            if !m.is_bars() && f.id.starts_with(BAR_ID_PREFIX) {
                return Err(Error::invalid("design", format!("field `{}`: ids starting with `bar.` belong to the bars model", f.name)));
            }
            if !names.insert(f.name.clone()) {
                return Err(Error::invalid("design", format!("two fields are named `{}`", f.name)));
            }
            if f.id.is_empty() || !ids.insert(f.id.clone()) {
                return Err(Error::invalid("design", format!("field `{}` has no id or a duplicate one", f.name)));
            }
            if m.is_bars() && f.required && f.default.is_none() && bar_key(f).is_none() {
                return Err(Error::invalid(
                    "design",
                    format!("{}: a required bars field kept beside the imported values needs a default (new bars from imports get it)", f.name),
                ));
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
    pub(crate) fn raw_rows(&self, m: &Model) -> Result<Vec<RawRow>> {
        let t = m.table();
        if m.is_bars() {
            // bars that have had stored values written (the rest read the defaults)
            return self.raw_rows_from(&t, &m.design, " WHERE _version IS NOT NULL");
        }
        if m.version == 0 || !self.table_exists(&t)? {
            return Ok(vec![]);
        }
        self.raw_rows_from(&t, &m.design, "")
    }

    fn raw_rows_from(&self, t: &str, design: &Design, wh: &str) -> Result<Vec<RawRow>> {
        let stored: Vec<&Field> = design.stored().collect();
        let cols: Vec<String> = stored.iter().map(|f| q(&f.name)).collect();
        let sel = if cols.is_empty() { String::new() } else { format!(", {}", cols.join(", ")) };
        let mut st = self.conn.prepare(&format!("SELECT id, _created_at, _updated_at, _author, _version{sel} FROM {}{wh}", q(t)))?;
        let n = stored.len();
        let rows = st
            .query_map([], |r| {
                let mut values = Vec::with_capacity(n);
                for (i, f) in stored.iter().enumerate() {
                    values.push((f.id.clone(), from_sql(r.get(5 + i)?, &f.ty)));
                }
                let id = match r.get::<_, rusqlite::types::Value>(0)? {
                    rusqlite::types::Value::Blob(b) => hex::encode(b),
                    rusqlite::types::Value::Text(t) => t,
                    o => format!("{o:?}"),
                };
                Ok(RawRow { id, created_at: r.get(1)?, updated_at: r.get(2)?, author: r.get(3)?, version: r.get(4)?, values })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// What applying `to` to a model would do (nothing is changed).
    pub fn plan_model(&self, m: &Model, to: &Design) -> Result<Plan> {
        let rows = self.raw_rows(m)?;
        if m.is_bars() {
            return self.plan_bars(m, to, &rows);
        }
        Ok(plan_rows(m, to, &rows))
    }

    /// Distinct values of a bars column with how many bars hold each.
    fn bar_column_values(&self, key: &str) -> Result<Vec<(Value, usize)>> {
        let e = bar_sql(key);
        let mut st = self.conn.prepare(&format!("SELECT {e}, COUNT(*) FROM bar_rows b JOIN sources s ON s.id = b.source_id GROUP BY {e}"))?;
        let v = st.query_map([], |r| Ok((bar_value(key, r.get(0)?), r.get::<_, i64>(1)? as usize)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    /// The plan for the bars model. Built-in fields that read the bars table cost nothing
    /// to rename, move, describe or drop (the bars keep their imported values); retyping one
    /// moves its converted values into a stored column, and its type back shows the bars' values again.
    fn plan_bars(&self, m: &Model, to: &Design, rows: &[RawRow]) -> Result<Plan> {
        let bars = self.count_records(m)?;
        let only_stored = |d: &Design| Design { fields: d.stored().cloned().collect(), builtins: d.builtins };
        let inner = Model { design: only_stored(&m.design), ..m.clone() };
        let mut plan = plan_rows(&inner, &only_stored(to), rows);
        plan.rows = bars;
        // the bars table is always there
        plan.steps.retain(|s| !matches!(s, Step::CreateTable { .. }));
        plan.steps.retain(|s| !matches!(s, Step::Order { .. }));
        for step in plan.steps.iter_mut() {
            // a stored field added with a default fills every bar (they read it), not only those with a row
            if let Step::AddField { filled, with: Some(_), .. } = step {
                *filled = bars;
            }
        }
        let from = &m.design;
        let mut extra = vec![];
        let mut blocked = vec![];
        for f in &from.fields {
            if to.by_id(&f.id).is_none()
                && let Some(_k) = bar_key(f)
            {
                extra.push(Step::HideField { field: f.name.clone() });
            }
        }
        for f in &to.fields {
            let old = from.by_id(&f.id);
            match (old, bar_key(f)) {
                (None, Some(_)) => extra.push(Step::ShowField { field: f.name.clone() }),
                (Some(o), Some(k)) => {
                    if o.name != f.name {
                        extra.push(Step::RenameField { from: o.name.clone(), to: f.name.clone() });
                    }
                    if bar_key(o).is_none() {
                        // stored (retyped) → reading the bars again: the stored values go
                        let lost = rows.iter().filter(|r| r.values.iter().any(|(id, v)| *id == f.id && !v.is_null())).count();
                        plan.steps.retain(|s| !matches!(s, Step::DropField { field, .. } if *field == o.name));
                        extra.push(Step::ChangeType { field: f.name.clone(), from: o.ty.code(), to: f.ty.code(), lost });
                    } else {
                        if o.required != f.required {
                            extra.push(Step::Require { field: f.name.clone(), on: f.required, filled: 0, with: None });
                        }
                        if o.unique != f.unique {
                            extra.push(Step::Unique { field: f.name.clone(), on: f.unique });
                        }
                        if o.index != f.index {
                            extra.push(Step::Index { field: f.name.clone(), on: f.index });
                        }
                        if o.default != f.default {
                            extra.push(Step::Default { field: f.name.clone(), value: f.default.clone() });
                        }
                        if o.choices != f.choices {
                            extra.push(Step::Choices { field: f.name.clone(), choices: f.choices.clone() });
                        }
                        if o.about != f.about {
                            extra.push(Step::About { field: f.name.clone() });
                        }
                    }
                    debug_assert!(!k.is_empty());
                }
                (Some(o), None) if bar_key(o).is_some() => {
                    // a built-in field retyped: its values are converted into a stored column
                    let key = bar_key(o).unwrap_or_default();
                    plan.steps.retain(|s| !matches!(s, Step::AddField { field, .. } if *field == f.name));
                    if o.name != f.name {
                        extra.push(Step::RenameField { from: o.name.clone(), to: f.name.clone() });
                    }
                    let vals = self.bar_column_values(key)?;
                    let lost: usize =
                        vals.iter().filter(|(v, _)| !v.is_null() && convert(v, &f.ty).is_none_or(|c| c.is_null())).map(|(_, n)| n).sum();
                    extra.push(Step::ChangeType { field: f.name.clone(), from: o.ty.code(), to: f.ty.code(), lost });
                    if f.required && f.default.is_none() && lost > 0 {
                        blocked.push(format!("{} is required but {lost} bar(s) would have no value: give it a default first", f.name));
                    }
                    if !plan.steps.iter().chain(&extra).any(|s| matches!(s, Step::CreateTable { .. })) && !self.table_exists(&m.table())? {
                        extra.insert(0, Step::CreateTable { table: m.table() });
                    }
                }
                _ => {}
            }
            // constraints on a field reading the bars are checked against the bars
            if let Some(k) = bar_key(f) {
                let old_on = |g: fn(&Field) -> bool| old.filter(|o| bar_key(o).is_some()).is_some_and(g);
                if f.unique && !old_on(|o| o.unique) {
                    let dups: usize = self.bar_column_values(k)?.iter().filter(|(_, n)| *n > 1).map(|(_, n)| n - 1).sum();
                    if dups > 0 {
                        blocked.push(format!("{} would be unique but {dups} bar(s) repeat a value", f.name));
                    }
                }
                if !f.choices.is_empty() {
                    let bad: usize = self
                        .bar_column_values(k)?
                        .iter()
                        .filter(|(v, _)| v.as_str().is_some_and(|s| !f.choices.iter().any(|c| c.eq_ignore_ascii_case(s))))
                        .map(|(_, n)| n)
                        .sum();
                    if bad > 0 {
                        blocked.push(format!("{}: {bad} bar(s) hold a value that is not one of {}", f.name, f.choices.join(", ")));
                    }
                }
            }
        }
        plan.steps.extend(extra);
        let kept_old: Vec<&str> = from.fields.iter().filter(|f| to.by_id(&f.id).is_some()).map(|f| f.id.as_str()).collect();
        let kept_new: Vec<&str> = to.fields.iter().filter(|f| from.by_id(&f.id).is_some()).map(|f| f.id.as_str()).collect();
        if kept_old != kept_new {
            plan.steps.push(Step::Order { fields: to.fields.iter().map(|f| f.name.clone()).collect() });
        }
        plan.blocked.extend(blocked);
        Ok(plan)
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
        let d = self.version_design(&m, to)?;
        let mut opts = opts.clone();
        if opts.note.is_empty() {
            opts.note = format!("rollback to v{to}");
        }
        self.apply_design(&m, &d, &opts)
    }

    /// The plan for rolling back to a version (what `rollback --dry-run` shows).
    pub fn rollback_plan(&self, id: &str, to: i64) -> Result<Plan> {
        let m = self.model_or_err(id)?;
        let d = self.version_design(&m, to)?;
        self.plan_model(&m, &d)
    }

    /// The design of an earlier version to roll back to (bars: v0 is the design as shipped).
    fn version_design(&self, m: &Model, to: i64) -> Result<Design> {
        let first = if m.is_bars() { 0 } else { 1 };
        if to < first || to >= m.version {
            return Err(Error::invalid("version", format!("{} is at v{}; roll back to v{first}..v{}", m.name, m.version, m.version - 1)));
        }
        if to == 0 {
            return Ok(Design::default().with_builtins());
        }
        let v =
            self.model_versions(&m.id)?.into_iter().find(|v| v.version == to).ok_or_else(|| Error::invalid("version", format!("v{to} not found")))?;
        Ok(v.design)
    }

    pub(crate) fn apply_design(&self, m: &Model, to: &Design, opts: &ApplyOpts) -> Result<(Model, Plan)> {
        let fixed;
        let to = if m.is_bars() && !to.builtins {
            fixed = to.clone().with_builtins();
            &fixed
        } else {
            to
        };
        self.check_design(m, to)?;
        let rows = self.raw_rows(m)?;
        let plan = if m.is_bars() { self.plan_bars(m, to, &rows)? } else { plan_rows(m, to, &rows) };
        if !plan.blocked.is_empty() {
            return Err(Error::invalid("migration", plan.blocked.join("; ")));
        }
        if plan.is_empty() && (m.version > 0 || m.is_bars()) {
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
        // the new table logs its changes for linked nodes
        self.sync_ensure()?;
        Ok((self.model_or_err(&m.id)?, plan))
    }

    /// Build the table for `to` and copy the rows over (converted, filled, stamped).
    pub(crate) fn rebuild(&self, m: &Model, to: &Design, version: i64, rows: &[RawRow]) -> Result<()> {
        if m.is_bars() {
            self.rebuild_bars_table(m, to, Some(version), rows)?;
            return self.rebuild_bars(m, to, version);
        }
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
        let stored: Vec<Field> = to.stored().cloned().collect();
        for f in &stored {
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
            let names: Vec<String> = stored.iter().map(|f| q(&f.name)).collect();
            let marks: Vec<String> = (0..5 + stored.len()).map(|i| format!("?{}", i + 1)).collect();
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
                for f in &stored {
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
        for f in &stored {
            if f.index || matches!(f.ty, FieldType::Ref(_)) {
                self.conn.execute_batch(&format!("CREATE INDEX {} ON {}({})", q(&format!("ix_{t}_{}", f.id)), q(&t), q(&f.name)))?;
            }
        }
        Ok(())
    }

    /// Build `m_bars` for design `to`: every bar's own columns as they are, and the stored
    /// fields' values of `rows` (converted; stamped `version`, or each row's own when None).
    pub(crate) fn rebuild_bars_table(&self, m: &Model, to: &Design, version: Option<i64>, rows: &[RawRow]) -> Result<()> {
        let stored: Vec<Field> = to.stored().cloned().collect();
        let mut cols = vec![
            "id BLOB PRIMARY KEY".to_string(),
            "_version INTEGER".into(),
            "_created_at INTEGER".into(),
            "_updated_at INTEGER".into(),
            "_author TEXT".into(),
            BAR_CORE.into(),
        ];
        for f in &stored {
            // no NOT NULL: bars without values read the field's default
            cols.push(format!("{} {}{}", q(&f.name), f.ty.sql(), if f.unique { " UNIQUE" } else { "" }));
        }
        self.conn.execute_batch(&format!(
            "DROP VIEW IF EXISTS bar_rows; DROP TABLE IF EXISTS m_bars__new; CREATE TABLE m_bars__new ({}) WITHOUT ROWID;
             INSERT INTO m_bars__new(id, {BAR_CORE_COLS}) SELECT id, {BAR_CORE_COLS} FROM m_bars;",
            cols.join(", ")
        ))?;
        {
            let set: Vec<String> = stored.iter().enumerate().map(|(i, f)| format!(", {} = ?{}", q(&f.name), i + 6)).collect();
            let mut up = self.conn.prepare(&format!(
                "UPDATE m_bars__new SET _version = ?2, _created_at = ?3, _updated_at = ?4, _author = ?5{} WHERE id = ?1",
                set.join("")
            ))?;
            for r in rows {
                let mut vals: Vec<rusqlite::types::Value> = vec![
                    rusqlite::types::Value::Blob(hex::decode(&r.id).unwrap_or_default()),
                    version.or(r.version).unwrap_or(m.version).into(),
                    r.created_at.into(),
                    r.updated_at.into(),
                    r.author.clone().map(Into::into).unwrap_or(rusqlite::types::Value::Null),
                ];
                for f in &stored {
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
                up.execute(rusqlite::params_from_iter(vals))?;
            }
        }
        self.conn.execute_batch(&format!(
            "DROP TABLE m_bars; ALTER TABLE m_bars__new RENAME TO m_bars;
             CREATE INDEX bars_series ON m_bars(_base, _quote, _source_id, _duration_s, _timestamp); {BAR_VIEW}"
        ))?;
        for f in &stored {
            if f.index || matches!(f.ty, FieldType::Ref(_)) {
                self.conn.execute_batch(&format!("CREATE INDEX {} ON m_bars({})", q(&format!("ix_m_bars_{}", f.id)), q(&f.name)))?;
            }
        }
        self.bar_indexes(to)
    }

    /// Index the bars' own columns of indexed built-in fields.
    fn bar_indexes(&self, to: &Design) -> Result<()> {
        for (k, _) in BAR_FIELDS {
            let col = match *k {
                "source" => "_source_id",
                "symbol" => "_base, _quote",
                "period" => "_duration_s",
                "base" => "_base",
                "quote" => "_quote",
                "timestamp" => "_timestamp",
                "open" => "_open",
                "high" => "_high",
                "low" => "_low",
                "close" => "_close",
                "volume" => "_volume",
                _ => "_flags",
            };
            let ix = q(&format!("ix_bars_{k}"));
            if to.bar_field(k).is_some_and(|f| f.index) {
                self.conn.execute_batch(&format!("CREATE INDEX IF NOT EXISTS {ix} ON m_bars({col})"))?;
            } else {
                self.conn.execute_batch(&format!("DROP INDEX IF EXISTS {ix}"))?;
            }
        }
        Ok(())
    }

    /// The second half of the 2.16 upgrade: the values of the fields added to bars (kept in
    /// `_mx_bars_upgrade`) go into `m_bars` with their stamps, and the indexes are made.
    pub(crate) fn finish_bars_upgrade(&self) -> Result<()> {
        if !self.table_exists("_mx_bars_upgrade")? {
            return Ok(());
        }
        let m = self.model_or_err(BARS)?;
        let design = m.design.clone().with_builtins();
        self.conn.execute_batch("SAVEPOINT bars_upgrade")?;
        let r = (|| -> Result<()> {
            let rows = self.raw_rows_from("_mx_bars_upgrade", &design, "")?;
            self.rebuild_bars_table(&Model { design: design.clone(), ..m.clone() }, &design, None, &rows)?;
            self.conn.execute_batch("DROP TABLE _mx_bars_upgrade")?;
            Ok(())
        })();
        match r {
            Ok(()) => Ok(self.conn.execute_batch("RELEASE bars_upgrade")?),
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK TO bars_upgrade; RELEASE bars_upgrade");
                Err(e)
            }
        }
    }

    /// bars: copy the values of built-in fields given another type into a stored column
    /// (converted), and index the bars columns of indexed built-in fields.
    fn rebuild_bars(&self, m: &Model, to: &Design, version: i64) -> Result<()> {
        let now = now_ms();
        for f in to.stored() {
            let Some(old) = m.design.by_id(&f.id) else { continue };
            let Some(key) = bar_key(old) else { continue };
            let mut read = self.conn.prepare(&format!("SELECT b.id, {} FROM bar_rows b JOIN sources s ON s.id = b.source_id", bar_sql(key)))?;
            let mut write = self.conn.prepare(&format!(
                "UPDATE m_bars SET {c} = ?4, _version = COALESCE(_version, ?2), _created_at = COALESCE(_created_at, ?3),
                 _updated_at = COALESCE(_updated_at, ?3) WHERE id = ?1",
                c = q(&f.name)
            ))?;
            let rows = read.query_map([], |r| Ok((r.get::<_, Vec<u8>>(0)?, bar_value(key, r.get(1)?))))?.collect::<rusqlite::Result<Vec<_>>>()?;
            for (id, v) in rows {
                let v = convert(&v, &f.ty).unwrap_or(Value::Null);
                let v = match (&v, &f.default) {
                    (Value::Null, Some(d)) if f.required => coerce(f, d.clone())?,
                    _ => v,
                };
                write.execute(params![id, version, now, to_sql(&v, &f.ty)])?;
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
                let design: Design = serde_json::from_str(&d).unwrap_or_default();
                Ok(ModelVersion {
                    model_id: r.get(0)?,
                    version: r.get(1)?,
                    design: if id == BARS { design.with_builtins() } else { design },
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
        let refs = self.models_referring_to(&m.id)?;
        if !refs.is_empty() {
            let n: Vec<String> = refs.into_iter().map(|r| r.name).collect();
            return Err(Error::invalid("model", format!("{} is referred to by {}; change those fields first", m.name, n.join(", "))));
        }
        let rows = self.count_records(&m)?;
        if rows > 0 && !destructive {
            return Err(Error::NeedsConfirmation(if m.is_bars() {
                format!("removing {} deletes all {rows} bars (every imported bar) and the values of the fields added to them", m.name)
            } else {
                format!("removing {} deletes its {rows} record(s)", m.name)
            }));
        }
        self.conn.execute_batch("SAVEPOINT model_remove")?;
        let r = (|| -> Result<()> {
            if !m.is_bars() {
                self.conn.execute_batch(&format!("DROP TABLE IF EXISTS {}", q(&m.table())))?;
            }
            self.conn.execute("DELETE FROM project_items WHERE kind = 'model' AND ref_id = ?1", [&m.id])?;
            self.orphan_rules(super::OwnerKind::Model, &m.id)?;
            if m.is_bars() {
                // the bars go with their model; imports start it again as shipped
                self.conn.execute_batch("DELETE FROM m_bars; DELETE FROM series_stats; DELETE FROM model_versions WHERE model_id = 'bars'")?;
                let shipped = Design::default().with_builtins();
                self.rebuild_bars_table(&m, &shipped, Some(0), &[])?;
                self.conn.execute(
                    "UPDATE models SET name = ?1, description = ?2, version = 0, design = ?3, draft = NULL, updated_at = ?4 WHERE id = ?1",
                    params![BARS, "imported price bars (built in)", serde_json::to_string(&Design::default().with_builtins())?, now_ms()],
                )?;
            } else {
                self.conn.execute("DELETE FROM models WHERE id = ?1", [&m.id])?;
            }
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
    /// The version the row was written under.
    pub version: Option<i64>,
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
        let (m, plan) = s.create_model("Signals", "", Design { fields: vec![price.clone(), note.clone()], ..Default::default() }, true).unwrap();
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
    fn designs_are_checked_and_bars_is_editable() {
        let s = Store::open_in_memory().unwrap();
        let bars = s.find_model("bars").unwrap();
        assert!(bars.builtin && bars.version == 0);
        assert_eq!(bars.design.fields.len(), BAR_FIELDS.len(), "the built-in fields are in the design");
        assert!(bars.design.fields.iter().all(|f| bar_key(f).is_some()));
        // a field of another model may not pose as a bars column
        let mut fake = f("close", FieldType::Real);
        fake.id = "bar.close".into();
        assert!(s.create_model("x", "", Design { fields: vec![fake], ..Default::default() }, false).is_err());
        let mut req = f("score", FieldType::Real);
        req.required = true;
        assert!(s.set_model_draft("bars", Some(Design { fields: vec![req.clone()], ..Default::default() })).is_err(), "required needs a default");
        req.default = Some(json!(0));
        s.set_model_draft("bars", Some(Design { fields: vec![req], ..Default::default() })).unwrap();
        let (b, _) = s.apply_model("bars", &ApplyOpts::default()).unwrap();
        assert_eq!(b.version, 1);
        assert!(s.table_exists("m_bars").unwrap());

        assert!(s.create_model("x", "", Design { fields: vec![f("id", FieldType::Text)], ..Default::default() }, false).is_err(), "reserved");
        assert!(s.create_model("x", "", Design { fields: vec![f("a", FieldType::Ref("nope".into()))], ..Default::default() }, false).is_err());
        let (a, _) = s.create_model("a", "", Design { fields: vec![f("n", FieldType::Text)], ..Default::default() }, true).unwrap();
        let (_b, _) = s.create_model("b", "", Design { fields: vec![f("a", FieldType::Ref(a.id.clone()))], ..Default::default() }, true).unwrap();
        assert!(s.remove_model(&a.id, true).unwrap_err().to_string().contains("referred to by b"));
        assert!(s.create_model("a", "", Design::default(), false).is_err(), "name taken");
        let r = s.rename_model(&a.id, Some("Alpha"), Some("first")).unwrap();
        assert_eq!(r.name, "alpha");
        assert!(s.table_exists("m_alpha").unwrap() && !s.table_exists("m_a").unwrap());
    }

    fn bars_store() -> Store {
        use crate::model::{Bar, Duration, Symbol};
        let mut s = Store::open_in_memory().unwrap();
        let src = s.add_source("bn", "csv", None).unwrap();
        let bars: Vec<Bar> = (0..4)
            .map(|i| Bar {
                ts: 1_704_067_200_000 + i * 300_000,
                open: 1.0 + i as f64,
                high: 9.0,
                low: 0.5,
                close: 2.0 + i as f64,
                volume: 10.0,
                flags: 0,
            })
            .collect();
        s.upsert_bars(&src, &Symbol::new("BTC", "USDT"), Duration(300), &bars).unwrap();
        s
    }

    fn edit_bars(s: &Store, f: impl FnOnce(&mut Design)) -> Plan {
        let m = s.find_model("bars").unwrap();
        let mut d = m.pending().clone();
        f(&mut d);
        s.set_model_draft("bars", Some(d.clone())).unwrap();
        s.plan_model(&m, &d).unwrap()
    }

    #[test]
    fn bar_fields_rename_drop_retype_and_come_back() {
        use crate::store::{BarFilter, RecordQuery};
        let s = bars_store();
        let cond = |c: &str| RecordQuery { filter: vec![crate::rules::parse_condition(c).unwrap()], ..Default::default() };
        // rename close to mid: lossless, the bars keep working
        let p = edit_bars(&s, |d| d.fields.iter_mut().find(|f| f.name == "close").unwrap().name = "mid".into());
        assert_eq!(p.lost(), 0);
        assert!(p.steps.contains(&Step::RenameField { from: "close".into(), to: "mid".into() }), "{:?}", p.steps);
        let (m, _) = s.apply_model("bars", &ApplyOpts::default()).unwrap();
        let r = s.records(&m, &cond("mid >= 4")).unwrap();
        assert_eq!(r.len(), 2);
        assert!(r[0].values.contains_key("mid") && !r[0].values.contains_key("close"));
        let e = s.records(&m, &cond("close >= 4")).unwrap_err().to_string();
        assert!(e.contains("called `mid` now"), "{e}");
        assert_eq!(s.query_bars(&BarFilter::default()).unwrap().len(), 4, "charts read the bars as before");

        // drop volume: hidden, the bars keep it (lossless), a rollback shows it again
        let p = edit_bars(&s, |d| d.fields.retain(|f| f.name != "volume"));
        assert_eq!((p.lost(), p.steps.clone()), (0, vec![Step::HideField { field: "volume".into() }]));
        let (m, _) = s.apply_model("bars", &ApplyOpts::default()).unwrap();
        assert!(!s.records(&m, &RecordQuery::default()).unwrap()[0].values.contains_key("volume"));
        let e = s.records(&m, &cond("volume > 1")).unwrap_err().to_string();
        assert!(e.contains("was dropped"), "{e}");
        assert_eq!(s.query_bars(&BarFilter::default()).unwrap()[0].bar.volume, 10.0);

        // retype open to text: values move into a stored column, converted
        // (required with a default, or not required: new bars from imports have no text open)
        let p = edit_bars(&s, |d| {
            let o = d.fields.iter_mut().find(|f| f.name == "open").unwrap();
            (o.ty, o.required) = (FieldType::Text, false);
        });
        assert!(p.steps.iter().any(|x| matches!(x, Step::ChangeType { field, .. } if field == "open")), "{:?}", p.steps);
        let (m, _) = s.apply_model("bars", &ApplyOpts { destructive: true, note: String::new() }).unwrap();
        let r = s.records(&m, &RecordQuery { sort: vec![("timestamp".into(), false)], ..Default::default() }).unwrap();
        assert_eq!(r[0].values["open"], json!("1.0"), "{:?}", r[0].values);
        // a text open can be any text now; the imported price stays on the bar
        s.update_record(&m, &r[0].id, &serde_json::Map::from_iter([("open".to_string(), json!("n/a"))])).unwrap();
        assert_eq!(s.query_bars(&BarFilter::default()).unwrap()[0].bar.open, 1.0);

        // roll back to v1 (close renamed only): volume and the real open come back
        let (m, p) = s.rollback_model("bars", 1, &ApplyOpts { destructive: true, note: String::new() }).unwrap();
        assert!(p.steps.contains(&Step::ShowField { field: "volume".into() }), "{:?}", p.steps);
        let r = s.records(&m, &RecordQuery { sort: vec![("timestamp".into(), false)], ..Default::default() }).unwrap();
        assert_eq!((r[0].values["volume"].clone(), r[0].values["open"].clone()), (json!(10.0), json!(1.0)));
        assert_eq!(m.version, 4);

        // bars can be created and deleted as records too
        let v = json!({"source": "bn", "symbol": "ETH/USDT", "period": "5m", "timestamp": 1_704_067_200_000i64, "open": 1, "high": 2, "low": 0.5, "mid": 1.5});
        let e = s.create_record(&m, v.as_object().unwrap()).unwrap();
        assert_eq!(s.count_records(&m).unwrap(), 5);
        s.delete_record(&m, &e.id).unwrap();
        assert_eq!(s.count_records(&m).unwrap(), 4);

        // renaming bars and removing it (all bars go; it starts again as shipped)
        let m = s.rename_model("bars", Some("prices"), None).unwrap();
        assert!(m.is_bars() && s.find_model("prices").unwrap().is_bars());
        assert!(matches!(s.remove_model("bars", false), Err(crate::error::Error::NeedsConfirmation(_))));
        let (_, n) = s.remove_model("bars", true).unwrap();
        assert_eq!(n, 4);
        let m = s.find_model("bars").unwrap();
        assert_eq!((m.version, s.count_records(&m).unwrap(), m.design.fields.len()), (0, 0, BAR_FIELDS.len()));
        assert!(s.query_bars(&BarFilter::default()).unwrap().is_empty());
    }
}
