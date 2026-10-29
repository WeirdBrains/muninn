//! Projects: named collections of live searches, results and journals.
//!
//! A project holds items. Each item points at something by kind and id: a live search,
//! a result (a series, optionally within a time range) or a journal. Rule sets (2.x Phase 4)
//! attach the same way with kind `rule_set`; nothing else in the schema has to change.
//! Journals created in (or forked into) a project are owned by it (`journals.project_id`);
//! removing a project keeps its journals as standalone ones. See `docs/projects.md`.
use super::{Store, now_ms};
use crate::error::{Error, Result};
use crate::model::{Duration, Symbol};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

pub const MIGRATION: &str = r#"
CREATE TABLE projects (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    author_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE project_items (
    id INTEGER PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    ref_id TEXT NOT NULL,
    payload TEXT NOT NULL DEFAULT '{}',
    author_id TEXT,
    added_at INTEGER NOT NULL,
    UNIQUE(project_id, kind, ref_id)
);
CREATE INDEX project_items_ref ON project_items(kind, ref_id);
"#;

/// What a project item points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    LiveSearch,
    /// A series (optionally a time range of it): always shows the bars stored now.
    Result,
    Journal,
    /// Reserved for rule sets (Phase 4).
    RuleSet,
}

impl ItemKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ItemKind::LiveSearch => "live_search",
            ItemKind::Result => "result",
            ItemKind::Journal => "journal",
            ItemKind::RuleSet => "rule_set",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "live_search" => ItemKind::LiveSearch,
            "result" => ItemKind::Result,
            "journal" => ItemKind::Journal,
            "rule_set" => ItemKind::RuleSet,
            _ => return None,
        })
    }
    pub fn label(self) -> &'static str {
        match self {
            ItemKind::LiveSearch => "live search",
            ItemKind::Result => "result",
            ItemKind::Journal => "journal",
            ItemKind::RuleSet => "rule set",
        }
    }
}

/// A result: one series, optionally limited to a time range (UTC ms).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeriesRef {
    pub source: String,
    pub symbol: Symbol,
    pub duration: Duration,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<i64>,
}

impl SeriesRef {
    /// Stable key, e.g. `binance:BTC/USDT:5m` or `binance:BTC/USDT:5m@1704067200000..`.
    pub fn key(&self) -> String {
        let base = format!("{}:{}:{}", self.source, self.symbol.display(), self.duration);
        match (self.from, self.to) {
            (None, None) => base,
            (a, b) => format!("{base}@{}..{}", a.map(|v| v.to_string()).unwrap_or_default(), b.map(|v| v.to_string()).unwrap_or_default()),
        }
    }
    /// `BTC/USDT binance 5m` plus the range when set.
    pub fn label(&self) -> String {
        let d = |ms: i64| crate::ingest::time::fmt_ts(ms)[..10].to_string();
        let r = match (self.from, self.to) {
            (None, None) => String::new(),
            (a, b) => format!(" {}→{}", a.map(d).unwrap_or_default(), b.map(d).unwrap_or_default()),
        };
        format!("{} {} {}{r}", self.symbol.display(), self.source, self.duration)
    }
    pub fn filter(&self) -> super::BarFilter {
        super::BarFilter {
            source: Some(self.source.clone()),
            symbol: Some(self.symbol.clone()),
            duration: Some(self.duration),
            from: self.from,
            to: self.to,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub description: String,
    pub author_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Project {
    pub fn short_id(&self) -> &str {
        super::short(&self.id)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProjectItem {
    pub id: i64,
    pub project_id: String,
    pub kind: ItemKind,
    pub ref_id: String,
    /// Kind-specific data (a [`SeriesRef`] for results).
    pub payload: serde_json::Value,
    pub author_id: Option<String>,
    pub added_at: i64,
}

impl ProjectItem {
    pub fn series(&self) -> Option<SeriesRef> {
        serde_json::from_value(self.payload.clone()).ok()
    }
}

/// First 8 characters of any id.
pub fn short_of(id: &str) -> &str {
    super::short(id)
}

fn check_name(name: &str) -> Result<String> {
    let n = name.trim();
    if n.is_empty() {
        return Err(Error::invalid("project name", "must not be empty"));
    }
    Ok(n.to_string())
}

impl Store {
    fn row_project(r: &rusqlite::Row) -> rusqlite::Result<Project> {
        Ok(Project { id: r.get(0)?, name: r.get(1)?, description: r.get(2)?, author_id: r.get(3)?, created_at: r.get(4)?, updated_at: r.get(5)? })
    }

    pub fn projects(&self) -> Result<Vec<Project>> {
        let mut st = self
            .conn
            .prepare("SELECT id, name, description, author_id, created_at, updated_at FROM projects ORDER BY name COLLATE NOCASE, created_at")?;
        let v = st.query_map([], Self::row_project)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn project(&self, id: &str) -> Result<Option<Project>> {
        Ok(self
            .conn
            .query_row("SELECT id, name, description, author_id, created_at, updated_at FROM projects WHERE id = ?1", [id], Self::row_project)
            .optional()?)
    }

    pub fn find_project(&self, key: &str) -> Result<Project> {
        super::pick(self.projects()?, key, |p| &p.id, |p| &p.name, "project", "mnnn project list")
    }

    pub fn create_project(&self, name: &str, description: &str) -> Result<Project> {
        let now = now_ms();
        let p = Project {
            id: super::new_id(),
            name: check_name(name)?,
            description: description.trim().to_string(),
            author_id: self.current_author_id()?,
            created_at: now,
            updated_at: now,
        };
        self.conn.execute(
            "INSERT INTO projects(id, name, description, author_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![p.id, p.name, p.description, p.author_id, p.created_at, p.updated_at],
        )?;
        Ok(p)
    }

    pub fn update_project(&self, id: &str, name: Option<&str>, description: Option<&str>) -> Result<Project> {
        let mut p = self.project(id)?.ok_or_else(|| Error::invalid("project", format!("`{id}` not found")))?;
        if let Some(n) = name {
            p.name = check_name(n)?;
        }
        if let Some(d) = description {
            p.description = d.trim().to_string();
        }
        p.updated_at = now_ms();
        self.conn.execute(
            "UPDATE projects SET name = ?2, description = ?3, updated_at = ?4 WHERE id = ?1",
            params![p.id, p.name, p.description, p.updated_at],
        )?;
        Ok(p)
    }

    /// Delete a project and its item links. Its journals stay, as standalone journals.
    pub fn remove_project(&self, id: &str) -> Result<Project> {
        let p = self.project(id)?.ok_or_else(|| Error::invalid("project", format!("`{id}` not found")))?;
        self.conn.execute("UPDATE journals SET project_id = NULL WHERE project_id = ?1", [id])?;
        self.conn.execute("DELETE FROM projects WHERE id = ?1", [id])?;
        Ok(p)
    }

    fn row_item(r: &rusqlite::Row) -> rusqlite::Result<ProjectItem> {
        let kind: String = r.get(2)?;
        let payload: String = r.get(4)?;
        Ok(ProjectItem {
            id: r.get(0)?,
            project_id: r.get(1)?,
            kind: ItemKind::parse(&kind).unwrap_or(ItemKind::RuleSet),
            ref_id: r.get(3)?,
            payload: serde_json::from_str(&payload).unwrap_or_default(),
            author_id: r.get(5)?,
            added_at: r.get(6)?,
        })
    }

    /// A project's items in the order they were added.
    pub fn project_items(&self, project: &str) -> Result<Vec<ProjectItem>> {
        let mut st = self
            .conn
            .prepare("SELECT id, project_id, kind, ref_id, payload, author_id, added_at FROM project_items WHERE project_id = ?1 ORDER BY id")?;
        let v = st.query_map([project], Self::row_item)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    /// Projects an item is in.
    pub fn projects_with(&self, kind: ItemKind, ref_id: &str) -> Result<Vec<Project>> {
        let ids: Vec<String> = {
            let mut st = self.conn.prepare("SELECT project_id FROM project_items WHERE kind = ?1 AND ref_id = ?2")?;
            st.query_map([kind.as_str(), ref_id], |r| r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?
        };
        Ok(self.projects()?.into_iter().filter(|p| ids.contains(&p.id)).collect())
    }

    /// Add an item. Adding the same thing twice returns the existing item (`false`).
    pub fn add_project_item(&self, project: &str, kind: ItemKind, ref_id: &str, payload: serde_json::Value) -> Result<(ProjectItem, bool)> {
        if self.project(project)?.is_none() {
            return Err(Error::invalid("project", format!("`{project}` not found")));
        }
        let exists = match kind {
            ItemKind::LiveSearch => self.live_search(ref_id)?.is_some(),
            ItemKind::Journal => self.journal(ref_id)?.is_some(),
            ItemKind::Result => true,
            ItemKind::RuleSet => return Err(Error::NotImplemented("rule sets")),
        };
        if !exists {
            return Err(Error::invalid(kind.label(), format!("`{ref_id}` not found")));
        }
        let n = self.conn.execute(
            "INSERT OR IGNORE INTO project_items(project_id, kind, ref_id, payload, author_id, added_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![project, kind.as_str(), ref_id, payload.to_string(), self.current_author_id()?, now_ms()],
        )?;
        let item = self.conn.query_row(
            "SELECT id, project_id, kind, ref_id, payload, author_id, added_at FROM project_items WHERE project_id = ?1 AND kind = ?2 AND ref_id = ?3",
            params![project, kind.as_str(), ref_id],
            Self::row_item,
        )?;
        Ok((item, n == 1))
    }

    /// Add a result (a series or a time range of it).
    pub fn add_project_result(&self, project: &str, r: &SeriesRef) -> Result<(ProjectItem, bool)> {
        self.add_project_item(project, ItemKind::Result, &r.key(), serde_json::to_value(r)?)
    }

    pub fn remove_project_item(&self, item: i64) -> Result<ProjectItem> {
        let it = self
            .conn
            .query_row("SELECT id, project_id, kind, ref_id, payload, author_id, added_at FROM project_items WHERE id = ?1", [item], Self::row_item)
            .optional()?
            .ok_or_else(|| Error::invalid("project item", format!("`{item}` not found")))?;
        self.conn.execute("DELETE FROM project_items WHERE id = ?1", [item])?;
        Ok(it)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::SearchSpec;

    #[test]
    fn projects_hold_items_and_survive_removal_of_things() {
        let s = Store::open_in_memory().unwrap();
        let p = s.create_project("Halving 2024", "what moved").unwrap();
        assert!(s.create_project("", "").is_err());
        let l = s.save_live_search("btc", &SearchSpec::default(), None).unwrap();
        let (it, new) = s.add_project_item(&p.id, ItemKind::LiveSearch, &l.id, serde_json::json!({})).unwrap();
        assert!(new);
        assert!(!s.add_project_item(&p.id, ItemKind::LiveSearch, &l.id, serde_json::json!({})).unwrap().1, "no duplicates");
        let r = SeriesRef { source: "binance".into(), symbol: Symbol::new("BTC", "USDT"), duration: Duration(300), from: Some(0), to: None };
        assert_eq!(r.key(), "binance:BTC/USDT:5m@0..");
        let (ri, _) = s.add_project_result(&p.id, &r).unwrap();
        assert_eq!(ri.series(), Some(r));
        assert!(s.add_project_item(&p.id, ItemKind::LiveSearch, "nope", serde_json::json!({})).is_err());
        assert!(s.add_project_item(&p.id, ItemKind::RuleSet, "x", serde_json::json!({})).is_err());
        assert_eq!(s.project_items(&p.id).unwrap().len(), 2);
        assert_eq!(s.projects_with(ItemKind::LiveSearch, &l.id).unwrap()[0].id, p.id);
        s.remove_live_search(&l.id).unwrap();
        assert_eq!(s.project_items(&p.id).unwrap().len(), 1, "a removed search leaves its projects");
        s.remove_project_item(ri.id).unwrap();
        assert!(s.project_items(&p.id).unwrap().is_empty());
        let _ = it;
        let p2 = s.update_project(&p.id, Some("Halving"), None).unwrap();
        assert_eq!((p2.name.as_str(), p2.description.as_str()), ("Halving", "what moved"));
        assert_eq!(s.find_project("halving").unwrap().id, p.id);
        s.remove_project(&p.id).unwrap();
        assert!(s.projects().unwrap().is_empty());
    }
}
