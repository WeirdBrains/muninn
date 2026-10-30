//! Code spaces: algorithms (math formulas, Polars expressions, Rust or Python) stored with
//! their input and the last run's result. Compiling and running them is `crate::code`; this
//! is the table. See `docs/code-spaces.md`.
use super::{Store, new_id, now_ms};
use crate::error::{Error, Result};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MIGRATION: &str = r#"
CREATE TABLE code_spaces (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    description TEXT NOT NULL DEFAULT '',
    lang TEXT NOT NULL,
    source TEXT NOT NULL DEFAULT '',
    input TEXT NOT NULL DEFAULT '{}',
    approved_hash TEXT,
    last_run TEXT,
    author_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
"#;

/// What a code space is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lang {
    /// Formulas over columns (`sma20 = sma(close, 20)`), compiled through generated Rust.
    Math,
    /// Polars expressions (`ret = col("close").pct_change(lit(1))`), compiled through generated Rust.
    Polars,
    /// Rust against the `muninn-plugin` SDK.
    Rust,
    /// Typed Python functions, translated to Rust in process (ruff parser, python-ast codegen).
    Python,
    /// A library built elsewhere (the source is its path).
    Plugin,
}

impl Lang {
    pub const ALL: [Lang; 5] = [Lang::Math, Lang::Polars, Lang::Rust, Lang::Python, Lang::Plugin];
    pub fn as_str(self) -> &'static str {
        match self {
            Lang::Math => "math",
            Lang::Polars => "polars",
            Lang::Rust => "rust",
            Lang::Python => "python",
            Lang::Plugin => "plugin",
        }
    }
    pub fn parse(s: &str) -> Option<Lang> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "math" | "formula" | "formulas" => Lang::Math,
            "polars" | "pl" => Lang::Polars,
            "rust" | "rs" => Lang::Rust,
            "python" | "py" | "rython" => Lang::Python,
            "plugin" | "prebuilt" | "lib" => Lang::Plugin,
            _ => return None,
        })
    }
    /// The source is code to compile (not a path to a built library).
    pub fn compiled(self) -> bool {
        self != Lang::Plugin
    }
    /// File extension for editing the source.
    pub fn ext(self) -> &'static str {
        match self {
            Lang::Math => "math",
            Lang::Polars => "polars",
            Lang::Rust => "rs",
            Lang::Python => "py",
            Lang::Plugin => "txt",
        }
    }
}

/// The records a code space gets as its input (`host.input()`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CodeInput {
    /// A model name (`bars` or a designed model); empty: no input.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub model: String,
    /// Conditions as written (`symbol = BTC/USDT`, `close > 3`).
    #[serde(default, rename = "where", skip_serializing_if = "Vec::is_empty")]
    pub filter: Vec<String>,
    /// Sort fields; `-f` descending.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sort: Vec<String>,
    /// Group fields (math code spaces compute per group).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub group: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

impl CodeInput {
    pub fn is_empty(&self) -> bool {
        self.model.is_empty()
    }
    /// `bars where symbol = BTC/USDT sort timestamp group symbol limit 500`.
    pub fn text(&self) -> String {
        if self.model.is_empty() {
            return "none".into();
        }
        let mut s = self.model.clone();
        if !self.filter.is_empty() {
            s += &format!(" where {}", self.filter.join("; "));
        }
        if !self.text.is_empty() {
            s += &format!(" matching \"{}\"", self.text);
        }
        if !self.group.is_empty() {
            s += &format!(" group {}", self.group.join(","));
        }
        if !self.sort.is_empty() {
            s += &format!(" sort {}", self.sort.join(","));
        }
        if let Some(n) = self.limit {
            s += &format!(" limit {n}");
        }
        s
    }
}

/// What the last run produced.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RunInfo {
    pub at: i64,
    /// `ok` or `error`.
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub logs: Vec<String>,
    /// Records created or changed.
    #[serde(default)]
    pub writes: usize,
    #[serde(default)]
    pub ms: u64,
    /// The history action of the run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<i64>,
}

/// A code space.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodeSpace {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub lang: Lang,
    pub source: String,
    #[serde(default)]
    pub input: CodeInput,
    /// The trust hash the owner approved to run (see `crate::code::trust_hash`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_run: Option<RunInfo>,
    pub author_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl CodeSpace {
    pub fn short_id(&self) -> &str {
        super::short(&self.id)
    }
}

/// Fields to change (`None`: keep).
#[derive(Debug, Clone, Default)]
pub struct CodeChange {
    pub name: Option<String>,
    pub description: Option<String>,
    pub lang: Option<Lang>,
    pub source: Option<String>,
    pub input: Option<CodeInput>,
}

const COLS: &str = "id, name, description, lang, source, input, approved_hash, last_run, author_id, created_at, updated_at";

fn row(r: &rusqlite::Row) -> rusqlite::Result<CodeSpace> {
    let lang: String = r.get(3)?;
    let input: String = r.get(5)?;
    let last: Option<String> = r.get(7)?;
    Ok(CodeSpace {
        id: r.get(0)?,
        name: r.get(1)?,
        description: r.get(2)?,
        lang: Lang::parse(&lang).unwrap_or(Lang::Rust),
        source: r.get(4)?,
        input: serde_json::from_str(&input).unwrap_or_default(),
        approved_hash: r.get(6)?,
        last_run: last.and_then(|s| serde_json::from_str(&s).ok()),
        author_id: r.get(8)?,
        created_at: r.get(9)?,
        updated_at: r.get(10)?,
    })
}

fn check_space_name(n: &str) -> Result<String> {
    let n = n.trim();
    if n.is_empty() || n.len() > 80 || n.chars().any(|c| c.is_control()) {
        return Err(Error::invalid("code space name", format!("`{n}`: give a name (up to 80 characters)")));
    }
    Ok(n.to_string())
}

impl Store {
    /// All code spaces, by name.
    pub fn code_spaces(&self) -> Result<Vec<CodeSpace>> {
        let mut st = self.conn.prepare(&format!("SELECT {COLS} FROM code_spaces ORDER BY name COLLATE NOCASE"))?;
        Ok(st.query_map([], row)?.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn code_space(&self, id: &str) -> Result<Option<CodeSpace>> {
        Ok(self.conn.query_row(&format!("SELECT {COLS} FROM code_spaces WHERE id = ?1"), [id], row).optional()?)
    }

    /// By id, id prefix or name.
    pub fn find_code_space(&self, key: &str) -> Result<CodeSpace> {
        super::pick(self.code_spaces()?, key, |c| &c.id, |c| &c.name, "code space", "mnnn code list")
    }

    /// Create a code space (by the current author; not approved to run yet).
    pub fn create_code_space(&self, name: &str, description: &str, lang: Lang, source: &str, input: CodeInput) -> Result<CodeSpace> {
        let name = check_space_name(name)?;
        if self.code_spaces()?.iter().any(|c| c.name.eq_ignore_ascii_case(&name)) {
            return Err(Error::invalid("code space name", format!("`{name}` already exists")));
        }
        let now = now_ms();
        let c = CodeSpace {
            id: new_id(),
            name,
            description: description.trim().to_string(),
            lang,
            source: source.to_string(),
            input,
            approved_hash: None,
            last_run: None,
            author_id: self.current_author_id()?,
            created_at: now,
            updated_at: now,
        };
        self.insert_code_space(&c)?;
        Ok(c)
    }

    pub(crate) fn insert_code_space(&self, c: &CodeSpace) -> Result<()> {
        self.conn.execute(
            &format!("INSERT INTO code_spaces({COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"),
            params![
                c.id,
                c.name,
                c.description,
                c.lang.as_str(),
                c.source,
                serde_json::to_string(&c.input)?,
                c.approved_hash,
                c.last_run.as_ref().map(serde_json::to_string).transpose()?,
                c.author_id,
                c.created_at,
                c.updated_at
            ],
        )?;
        Ok(())
    }

    /// Change a code space (approval stays; a changed source needs approving again because
    /// its trust hash changes).
    pub fn update_code_space(&self, id: &str, ch: &CodeChange) -> Result<CodeSpace> {
        let mut c = self.code_space(id)?.ok_or_else(|| Error::invalid("code space", format!("`{id}` not found")))?;
        if let Some(n) = &ch.name {
            let n = check_space_name(n)?;
            if self.code_spaces()?.iter().any(|o| o.id != c.id && o.name.eq_ignore_ascii_case(&n)) {
                return Err(Error::invalid("code space name", format!("`{n}` already exists")));
            }
            c.name = n;
        }
        if let Some(d) = &ch.description {
            c.description = d.trim().to_string();
        }
        if let Some(l) = ch.lang {
            c.lang = l;
        }
        if let Some(s) = &ch.source {
            c.source = s.clone();
        }
        if let Some(i) = &ch.input {
            c.input = i.clone();
        }
        c.updated_at = now_ms();
        self.conn.execute(
            "UPDATE code_spaces SET name = ?2, description = ?3, lang = ?4, source = ?5, input = ?6, updated_at = ?7 WHERE id = ?1",
            params![c.id, c.name, c.description, c.lang.as_str(), c.source, serde_json::to_string(&c.input)?, c.updated_at],
        )?;
        Ok(c)
    }

    /// Record the owner's approval of a trust hash (`None` withdraws it).
    pub fn approve_code_space(&self, id: &str, hash: Option<&str>) -> Result<()> {
        self.conn.execute("UPDATE code_spaces SET approved_hash = ?2 WHERE id = ?1", params![id, hash])?;
        Ok(())
    }

    pub fn set_code_run(&self, id: &str, run: &RunInfo) -> Result<()> {
        self.conn.execute("UPDATE code_spaces SET last_run = ?2 WHERE id = ?1", params![id, serde_json::to_string(run)?])?;
        Ok(())
    }

    /// Run `f` all-or-nothing (bulk writes of a plugin).
    pub(crate) fn code_atomic<T>(&self, f: impl FnOnce() -> Result<T>) -> Result<T> {
        self.conn.execute_batch("SAVEPOINT muninn_code")?;
        match f() {
            Ok(v) => {
                self.conn.execute_batch("RELEASE muninn_code")?;
                Ok(v)
            }
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK TO muninn_code; RELEASE muninn_code");
                Err(e)
            }
        }
    }

    /// Remove a code space; it leaves projects and reports point at nothing.
    pub fn remove_code_space(&self, id: &str) -> Result<CodeSpace> {
        let c = self.code_space(id)?.ok_or_else(|| Error::invalid("code space", format!("`{id}` not found")))?;
        self.conn.execute("DELETE FROM project_items WHERE kind = 'code' AND ref_id = ?1", [&c.id])?;
        self.conn.execute("DELETE FROM code_spaces WHERE id = ?1", [&c.id])?;
        Ok(c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_find_change_remove() {
        let s = Store::open_in_memory().unwrap();
        let input = CodeInput { model: "bars".into(), sort: vec!["timestamp".into()], ..Default::default() };
        let c = s.create_code_space("SMA cross", "two averages", Lang::Math, "a = sma(close, 5)", input.clone()).unwrap();
        assert!(s.create_code_space("sma CROSS", "", Lang::Rust, "", CodeInput::default()).is_err(), "names are unique, any case");
        assert_eq!(s.find_code_space("sma cross").unwrap().id, c.id);
        assert_eq!(s.find_code_space(&c.id[..6]).unwrap().input, input);
        assert_eq!(input.text(), "bars sort timestamp");
        let c2 = s.update_code_space(&c.id, &CodeChange { source: Some("b = 1".into()), lang: Some(Lang::Python), ..Default::default() }).unwrap();
        assert_eq!((c2.source.as_str(), c2.lang), ("b = 1", Lang::Python));
        s.approve_code_space(&c.id, Some("abc")).unwrap();
        s.set_code_run(&c.id, &RunInfo { status: "ok".into(), writes: 2, ..Default::default() }).unwrap();
        let got = s.code_space(&c.id).unwrap().unwrap();
        assert_eq!((got.approved_hash.as_deref(), got.last_run.unwrap().writes), (Some("abc"), 2));
        s.remove_code_space(&c.id).unwrap();
        assert!(s.code_spaces().unwrap().is_empty());
        for l in Lang::ALL {
            assert_eq!(Lang::parse(l.as_str()), Some(l));
        }
    }
}
