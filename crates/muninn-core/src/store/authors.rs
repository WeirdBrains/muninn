//! Authors: who did the work (name, email, free-form details) and the current author.
//!
//! Authors live in the encrypted database so they travel with the data. One author can be
//! the current one (`settings.current_author`); actions and, later, journals, notes and
//! reports are attributed to it.
use super::{Store, now_ms};
use crate::error::{Error, Result};
use rusqlite::{OptionalExtension, params};
use std::collections::BTreeMap;

pub const MIGRATION: &str = r#"
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE authors (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT,
    details TEXT NOT NULL DEFAULT '{}',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
"#;

const CURRENT: &str = "current_author";

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct Author {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
    /// Free-form details (`key = value`), e.g. affiliation, website, phone.
    pub details: BTreeMap<String, String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Author {
    /// Short id shown in listings (first 8 characters).
    pub fn short_id(&self) -> &str {
        &self.id[..self.id.len().min(8)]
    }
    /// `Name <email>`.
    pub fn display(&self) -> String {
        match &self.email {
            Some(e) => format!("{} <{e}>", self.name),
            None => self.name.clone(),
        }
    }
}

/// Changes for [`Store::update_author`]; `None` keeps a value.
#[derive(Debug, Clone, Default)]
pub struct AuthorChange {
    pub name: Option<String>,
    /// `Some("")` clears the email.
    pub email: Option<String>,
    /// Details to set (an empty value removes the key).
    pub details: Vec<(String, String)>,
    /// Replace all details instead of merging.
    pub replace_details: bool,
}

/// Parse `key=value` detail arguments.
pub fn parse_details(items: &[String]) -> Result<Vec<(String, String)>> {
    items
        .iter()
        .map(|s| {
            let (k, v) = s.split_once('=').ok_or_else(|| Error::invalid("detail", format!("`{s}` (use key=value)")))?;
            let k = k.trim();
            if k.is_empty() {
                return Err(Error::invalid("detail", format!("`{s}` (empty key)")));
            }
            Ok((k.to_string(), v.trim().to_string()))
        })
        .collect()
}

fn check_name(name: &str) -> Result<String> {
    let n = name.trim();
    if n.is_empty() {
        return Err(Error::invalid("author name", "must not be empty"));
    }
    Ok(n.to_string())
}

fn check_email(email: &str) -> Result<Option<String>> {
    let e = email.trim();
    if e.is_empty() {
        return Ok(None);
    }
    if !e.contains('@') || e.contains(char::is_whitespace) {
        return Err(Error::invalid("email", format!("`{e}`")));
    }
    Ok(Some(e.to_string()))
}

impl Store {
    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self.conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0)).optional()?)
    }

    pub fn set_setting(&self, key: &str, value: Option<&str>) -> Result<()> {
        match value {
            Some(v) => self
                .conn
                .execute("INSERT INTO settings(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value", [key, v])?,
            None => self.conn.execute("DELETE FROM settings WHERE key = ?1", [key])?,
        };
        Ok(())
    }

    fn row_author(r: &rusqlite::Row) -> rusqlite::Result<Author> {
        let details: String = r.get(3)?;
        Ok(Author {
            id: r.get(0)?,
            name: r.get(1)?,
            email: r.get(2)?,
            details: serde_json::from_str(&details).unwrap_or_default(),
            created_at: r.get(4)?,
            updated_at: r.get(5)?,
        })
    }

    pub fn authors(&self) -> Result<Vec<Author>> {
        let mut st =
            self.conn.prepare("SELECT id, name, email, details, created_at, updated_at FROM authors ORDER BY name COLLATE NOCASE, created_at")?;
        let v = st.query_map([], Self::row_author)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn author(&self, id: &str) -> Result<Option<Author>> {
        Ok(self
            .conn
            .query_row("SELECT id, name, email, details, created_at, updated_at FROM authors WHERE id = ?1", [id], Self::row_author)
            .optional()?)
    }

    /// Find an author by id, unique id prefix, exact name or email (case-insensitive).
    pub fn find_author(&self, key: &str) -> Result<Author> {
        let k = key.trim();
        let all = self.authors()?;
        if let Some(a) = all.iter().find(|a| a.id == k) {
            return Ok(a.clone());
        }
        let lk = k.to_lowercase();
        let by_name: Vec<&Author> =
            all.iter().filter(|a| a.name.to_lowercase() == lk || a.email.as_deref().is_some_and(|e| e.to_lowercase() == lk)).collect();
        if by_name.len() == 1 {
            return Ok(by_name[0].clone());
        }
        let by_prefix: Vec<&Author> = all.iter().filter(|a| !k.is_empty() && a.id.starts_with(k)).collect();
        match (by_name.len(), by_prefix.len()) {
            (0, 1) => Ok(by_prefix[0].clone()),
            (0, 0) => Err(Error::invalid("author", format!("`{k}` not found (see `mnnn author list`)"))),
            _ => Err(Error::invalid("author", format!("`{k}` matches more than one author; use the id"))),
        }
    }

    /// Create an author. The first author becomes the current one.
    pub fn add_author(&self, name: &str, email: Option<&str>, details: &[(String, String)]) -> Result<Author> {
        let now = now_ms();
        let a = Author {
            id: uuid::Uuid::new_v4().simple().to_string(),
            name: check_name(name)?,
            email: check_email(email.unwrap_or(""))?,
            details: details.iter().filter(|(_, v)| !v.is_empty()).cloned().collect(),
            created_at: now,
            updated_at: now,
        };
        self.conn.execute(
            "INSERT INTO authors(id, name, email, details, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![a.id, a.name, a.email, serde_json::to_string(&a.details)?, a.created_at, a.updated_at],
        )?;
        if self.current_author()?.is_none() {
            self.set_setting(CURRENT, Some(&a.id))?;
        }
        Ok(a)
    }

    pub fn update_author(&self, id: &str, c: &AuthorChange) -> Result<Author> {
        let mut a = self.author(id)?.ok_or_else(|| Error::invalid("author", format!("`{id}` not found")))?;
        if let Some(n) = &c.name {
            a.name = check_name(n)?;
        }
        if let Some(e) = &c.email {
            a.email = check_email(e)?;
        }
        if c.replace_details {
            a.details.clear();
        }
        for (k, v) in &c.details {
            if v.is_empty() {
                a.details.remove(k);
            } else {
                a.details.insert(k.clone(), v.clone());
            }
        }
        a.updated_at = now_ms();
        self.conn.execute(
            "UPDATE authors SET name = ?2, email = ?3, details = ?4, updated_at = ?5 WHERE id = ?1",
            params![a.id, a.name, a.email, serde_json::to_string(&a.details)?, a.updated_at],
        )?;
        Ok(a)
    }

    /// Delete an author. Work attributed to it keeps the id (shown as a removed author).
    /// If it was current, no author is current afterwards.
    pub fn remove_author(&self, id: &str) -> Result<Author> {
        let a = self.author(id)?.ok_or_else(|| Error::invalid("author", format!("`{id}` not found")))?;
        self.conn.execute("DELETE FROM authors WHERE id = ?1", [id])?;
        if self.setting(CURRENT)?.as_deref() == Some(id) {
            self.set_setting(CURRENT, None)?;
        }
        Ok(a)
    }

    /// Id of the current author, for attributing new work.
    pub fn current_author_id(&self) -> Result<Option<String>> {
        self.setting(CURRENT)
    }

    pub fn current_author(&self) -> Result<Option<Author>> {
        match self.setting(CURRENT)? {
            Some(id) => self.author(&id),
            None => Ok(None),
        }
    }

    /// Make an author current (None: nobody).
    pub fn set_current_author(&self, id: Option<&str>) -> Result<()> {
        if let Some(id) = id
            && self.author(id)?.is_none()
        {
            return Err(Error::invalid("author", format!("`{id}` not found")));
        }
        self.set_setting(CURRENT, id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn author_crud_and_current() {
        let s = Store::open_in_memory().unwrap();
        assert!(s.current_author().unwrap().is_none());
        let d = parse_details(&["org=Kommandosaldat".into(), "site = https://x".into()]).unwrap();
        let a = s.add_author("Michael", Some("m@example.com"), &d).unwrap();
        assert_eq!(s.current_author().unwrap().unwrap().id, a.id, "first author becomes current");
        let b = s.add_author("Ada", None, &[]).unwrap();
        assert_eq!(s.current_author().unwrap().unwrap().id, a.id);
        assert_eq!(s.authors().unwrap().iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["Ada", "Michael"]);
        assert_eq!(s.find_author("michael").unwrap().id, a.id);
        assert_eq!(s.find_author("m@example.com").unwrap().id, a.id);
        assert_eq!(s.find_author(&b.id[..6]).unwrap().id, b.id);
        assert!(s.find_author("nobody").is_err());
        let c = AuthorChange { email: Some("".into()), details: vec![("org".into(), "".into()), ("tz".into(), "EST".into())], ..Default::default() };
        let a2 = s.update_author(&a.id, &c).unwrap();
        assert_eq!(a2.email, None);
        assert_eq!(a2.details.keys().collect::<Vec<_>>(), ["site", "tz"]);
        assert_eq!(s.author(&a.id).unwrap().unwrap(), a2);
        s.set_current_author(Some(&b.id)).unwrap();
        assert_eq!(s.current_author().unwrap().unwrap().name, "Ada");
        s.remove_author(&b.id).unwrap();
        assert!(s.current_author().unwrap().is_none());
        assert!(s.set_current_author(Some("nope")).is_err());
    }

    #[test]
    fn validation() {
        let s = Store::open_in_memory().unwrap();
        assert!(s.add_author("  ", None, &[]).is_err());
        assert!(s.add_author("A", Some("not an email"), &[]).is_err());
        assert!(parse_details(&["novalue".into()]).is_err());
        assert!(parse_details(&["=x".into()]).is_err());
    }
}
