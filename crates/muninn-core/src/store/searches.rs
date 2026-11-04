//! Live searches: saved questions, not saved answers.
//!
//! A live search stores the search itself (text, filters, time range, sort) and runs it
//! against the data every time it is opened, so its results follow imports and removals.
//! Forking copies a search (keeping a link to where it came from) so it can be refined
//! without losing the original. See `docs/live-searches.md`.
use super::{Store, now_ms};
use crate::error::{Error, Result};
use crate::tui::sidebar::{Filters, SortBy};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

pub const MIGRATION: &str = r#"
CREATE TABLE live_searches (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    spec TEXT NOT NULL,
    forked_from TEXT,
    author_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
"#;

/// What a search looks through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SearchTarget {
    /// Imported series (the sidebar search).
    #[default]
    Series,
    /// Journal entries (title and text; the time range applies to when they were written).
    Journals,
    /// Records of designed models (`mnnn model`): text over their text fields, `where`
    /// conditions on their fields, the time range on when they were created.
    Records,
}

impl SearchTarget {
    pub fn label(self) -> &'static str {
        match self {
            SearchTarget::Series => "series",
            SearchTarget::Journals => "journals",
            SearchTarget::Records => "records",
        }
    }
}

/// The search itself: everything needed to run it again on current data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SearchSpec {
    #[serde(default)]
    pub target: SearchTarget,
    /// Free text, matched like the sidebar search (fuzzy, words in any order).
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub filters: Filters,
    #[serde(default)]
    pub sort: SortBy,
    #[serde(default)]
    pub desc: bool,
    /// Records: the model (its id; `None`: every model but bars).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Records: conditions on fields (`price > 3`), all must hold.
    #[serde(default, rename = "where", skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<super::Condition>,
}

impl SearchSpec {
    /// One line: `series "btc" · src:binance, 2024`.
    pub fn describe(&self) -> String {
        let mut parts = vec![self.target.label().to_string()];
        if !self.conditions.is_empty() {
            parts.push(self.conditions.iter().map(crate::rules::condition_text).collect::<Vec<_>>().join(", "));
        }
        if !self.query.trim().is_empty() {
            parts.push(format!("\"{}\"", self.query.trim()));
        }
        let chips: Vec<String> = self.filters.chips().into_iter().map(|(_, l)| l).collect();
        if !chips.is_empty() {
            parts.push(chips.join(", "));
        }
        parts.join(" · ")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveSearch {
    pub id: String,
    pub name: String,
    pub spec: SearchSpec,
    /// The search this one was forked from.
    pub forked_from: Option<String>,
    pub author_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl LiveSearch {
    pub fn short_id(&self) -> &str {
        super::short(&self.id)
    }
}

fn check_name(name: &str) -> Result<String> {
    let n = name.trim();
    if n.is_empty() {
        return Err(Error::invalid("live search name", "must not be empty"));
    }
    Ok(n.to_string())
}

const COLS: &str = "id, name, spec, forked_from, author_id, created_at, updated_at";

impl Store {
    fn row_search(r: &rusqlite::Row) -> rusqlite::Result<LiveSearch> {
        let spec: String = r.get(2)?;
        Ok(LiveSearch {
            id: r.get(0)?,
            name: r.get(1)?,
            spec: serde_json::from_str(&spec).unwrap_or_default(),
            forked_from: r.get(3)?,
            author_id: r.get(4)?,
            created_at: r.get(5)?,
            updated_at: r.get(6)?,
        })
    }

    pub fn live_searches(&self) -> Result<Vec<LiveSearch>> {
        let mut st = self.conn.prepare(&format!("SELECT {COLS} FROM live_searches ORDER BY name COLLATE NOCASE, created_at"))?;
        let v = st.query_map([], Self::row_search)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn live_search(&self, id: &str) -> Result<Option<LiveSearch>> {
        Ok(self.conn.query_row(&format!("SELECT {COLS} FROM live_searches WHERE id = ?1"), [id], Self::row_search).optional()?)
    }

    /// By id, unique id prefix or exact name (case-insensitive).
    pub fn find_live_search(&self, key: &str) -> Result<LiveSearch> {
        super::pick(self.live_searches()?, key, |s| &s.id, |s| &s.name, "live search", "mnnn search list")
    }

    /// Save a search. `forked_from` links a fork to its original.
    pub fn save_live_search(&self, name: &str, spec: &SearchSpec, forked_from: Option<&str>) -> Result<LiveSearch> {
        let now = now_ms();
        let s = LiveSearch {
            id: super::new_id(),
            name: check_name(name)?,
            spec: spec.clone(),
            forked_from: forked_from.map(str::to_string),
            author_id: self.current_author_id()?,
            created_at: now,
            updated_at: now,
        };
        self.conn.execute(
            &format!("INSERT INTO live_searches({COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"),
            params![s.id, s.name, serde_json::to_string(&s.spec)?, s.forked_from, s.author_id, s.created_at, s.updated_at],
        )?;
        Ok(s)
    }

    /// Fork: a copy linked to the original, optionally with a changed spec.
    pub fn fork_live_search(&self, id: &str, name: Option<&str>, spec: Option<&SearchSpec>) -> Result<LiveSearch> {
        let o = self.live_search(id)?.ok_or_else(|| Error::invalid("live search", format!("`{id}` not found")))?;
        let name = name.map(str::to_string).unwrap_or_else(|| format!("{} (fork)", o.name));
        self.save_live_search(&name, spec.unwrap_or(&o.spec), Some(&o.id))
    }

    /// Rename and/or change the search (refine it).
    pub fn update_live_search(&self, id: &str, name: Option<&str>, spec: Option<&SearchSpec>) -> Result<LiveSearch> {
        let mut s = self.live_search(id)?.ok_or_else(|| Error::invalid("live search", format!("`{id}` not found")))?;
        if let Some(n) = name {
            s.name = check_name(n)?;
        }
        if let Some(sp) = spec {
            s.spec = sp.clone();
        }
        s.updated_at = now_ms();
        self.conn.execute(
            "UPDATE live_searches SET name = ?2, spec = ?3, updated_at = ?4 WHERE id = ?1",
            params![s.id, s.name, serde_json::to_string(&s.spec)?, s.updated_at],
        )?;
        Ok(s)
    }

    /// Delete a search and take it out of every project. Forks keep their link (shown as removed).
    pub fn remove_live_search(&self, id: &str) -> Result<LiveSearch> {
        let s = self.live_search(id)?.ok_or_else(|| Error::invalid("live search", format!("`{id}` not found")))?;
        self.conn.execute("DELETE FROM project_items WHERE kind = 'live_search' AND ref_id = ?1", [id])?;
        self.orphan_rules(super::OwnerKind::LiveSearch, id)?;
        self.conn.execute("DELETE FROM live_searches WHERE id = ?1", [id])?;
        Ok(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_fork_update_remove() {
        let s = Store::open_in_memory().unwrap();
        let a = s.add_author("Ada", None, &[]).unwrap();
        let mut spec = SearchSpec { query: "btc".into(), ..Default::default() };
        spec.filters.sources.insert("binance".into());
        let l = s.save_live_search("BTC on Binance", &spec, None).unwrap();
        assert_eq!(l.author_id.as_deref(), Some(a.id.as_str()));
        assert_eq!(l.spec.describe(), "series · \"btc\" · src:binance");
        let mut sp2 = spec.clone();
        sp2.filters.years.insert(2024);
        let f = s.fork_live_search(&l.id, None, Some(&sp2)).unwrap();
        assert_eq!((f.name.as_str(), f.forked_from.as_deref()), ("BTC on Binance (fork)", Some(l.id.as_str())));
        assert_eq!(s.live_search(&l.id).unwrap().unwrap().spec, spec, "the original is untouched");
        assert_eq!(s.find_live_search("btc on binance").unwrap().id, l.id);
        assert_eq!(s.find_live_search(&f.id[..5]).unwrap().id, f.id);
        let u = s.update_live_search(&f.id, Some("2024 only"), None).unwrap();
        assert_eq!(u.spec, sp2);
        assert!(s.update_live_search(&f.id, Some(" "), None).is_err());
        s.remove_live_search(&l.id).unwrap();
        assert_eq!(s.live_searches().unwrap().len(), 1);
        assert!(s.find_live_search("nope").is_err());
    }
}
