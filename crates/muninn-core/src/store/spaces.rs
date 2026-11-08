//! Code spaces: named collections of code with a kind and one main (the entry point).
//! Today's single units live in the `code` table (renamed from `code_spaces` in 2.23,
//! ids kept). See `docs/code-spaces.md`.
use super::{CodeSpace, Store, new_id, now_ms};
use crate::error::{Error, Result};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

/// 2.23: `code_spaces` becomes `code` (ids and every reference kept: project items, org
/// resources and shares point at ids; pipelines at names), and spaces arrive.
pub const MIGRATION: &str = r#"
ALTER TABLE code_spaces RENAME TO code;
CREATE TABLE code_spaces (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    description TEXT NOT NULL DEFAULT '',
    main_code_id TEXT NOT NULL REFERENCES code(id) ON DELETE RESTRICT,
    kind TEXT NOT NULL CHECK (kind IN ('parser','transform','analysis','ingest-connector')),
    enabled INTEGER NOT NULL DEFAULT 1,
    approved_hash TEXT,
    author_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE code_space_members (
    space_id TEXT NOT NULL REFERENCES code_spaces(id) ON DELETE CASCADE,
    code_id TEXT NOT NULL REFERENCES code(id) ON DELETE RESTRICT,
    alias TEXT NOT NULL COLLATE NOCASE,
    position INTEGER NOT NULL,
    pin_hash TEXT,
    added_at INTEGER NOT NULL,
    PRIMARY KEY (space_id, code_id),
    UNIQUE (space_id, alias)
);
CREATE INDEX code_space_members_code ON code_space_members(code_id);
"#;

/// What a space is for (one kind per space).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SpaceKind {
    /// Raw bytes as text → rows (`parse-csv`).
    Parser,
    /// A frame → a frame, a pipe step (`to-bars`).
    Transform,
    /// Runs for a view, on its own or over a share.
    Analysis,
    /// Gathers data on a schedule (runs only from a schedule).
    IngestConnector,
}

impl SpaceKind {
    pub const ALL: [SpaceKind; 4] = [SpaceKind::Parser, SpaceKind::Transform, SpaceKind::Analysis, SpaceKind::IngestConnector];
    pub fn as_str(self) -> &'static str {
        match self {
            SpaceKind::Parser => "parser",
            SpaceKind::Transform => "transform",
            SpaceKind::Analysis => "analysis",
            SpaceKind::IngestConnector => "ingest-connector",
        }
    }
    pub fn plural(self) -> &'static str {
        match self {
            SpaceKind::Parser => "Parsers",
            SpaceKind::Transform => "Transforms",
            SpaceKind::Analysis => "Analysis",
            SpaceKind::IngestConnector => "Ingest connectors",
        }
    }
    /// Exactly a known kind (as stored).
    pub fn parse(s: &str) -> Option<SpaceKind> {
        SpaceKind::ALL.into_iter().find(|k| k.as_str() == s)
    }
    /// As typed: case, plurals and `_` forgiven; unknown ones list the kinds.
    pub fn from_arg(s: &str) -> Result<SpaceKind> {
        let l = s.trim().to_ascii_lowercase().replace('_', "-");
        let l = match l.as_str() {
            "parsers" => "parser",
            "transforms" => "transform",
            "ingest-connectors" | "ingest" | "connector" | "connectors" => "ingest-connector",
            x => x,
        };
        SpaceKind::parse(l).ok_or_else(|| {
            Error::invalid("space kind", format!("`{s}` is not a kind; the kinds are {}", SpaceKind::ALL.map(|k| k.as_str()).join(", ")))
        })
    }
}

impl std::fmt::Display for SpaceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A code space.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Space {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub main_code_id: String,
    pub kind: SpaceKind,
    pub enabled: bool,
    /// The line-up hash the owner approved (see `crate::code::spaces::lineup_hash`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved_hash: Option<String>,
    pub author_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Space {
    pub fn short_id(&self) -> &str {
        super::short(&self.id)
    }
}

/// One code in a space. `pin_hash` `None`: it follows the code's current source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpaceMember {
    pub space_id: String,
    pub code_id: String,
    pub alias: String,
    pub position: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin_hash: Option<String>,
    pub added_at: i64,
}

const COLS: &str = "id, name, description, main_code_id, kind, enabled, approved_hash, author_id, created_at, updated_at";

fn row(r: &rusqlite::Row) -> rusqlite::Result<Space> {
    let kind: String = r.get(4)?;
    Ok(Space {
        id: r.get(0)?,
        name: r.get(1)?,
        description: r.get(2)?,
        main_code_id: r.get(3)?,
        kind: SpaceKind::parse(&kind).unwrap_or(SpaceKind::Analysis),
        enabled: r.get(5)?,
        approved_hash: r.get(6)?,
        author_id: r.get(7)?,
        created_at: r.get(8)?,
        updated_at: r.get(9)?,
    })
}

fn member(r: &rusqlite::Row) -> rusqlite::Result<SpaceMember> {
    Ok(SpaceMember { space_id: r.get(0)?, code_id: r.get(1)?, alias: r.get(2)?, position: r.get(3)?, pin_hash: r.get(4)?, added_at: r.get(5)? })
}

/// The longest space name (as for code).
pub const NAME_MAX: usize = 80;

pub fn check_name(n: &str) -> Result<String> {
    let n = n.trim();
    if n.is_empty() || n.chars().count() > NAME_MAX || n.chars().any(|c| c.is_control()) || n.contains(':') {
        return Err(Error::invalid("space name", format!("`{n}`: give a name (up to {NAME_MAX} characters, no `:`)")));
    }
    Ok(n.to_string())
}

fn check_alias(a: &str) -> Result<String> {
    let a = a.trim();
    if a.is_empty() || a.len() > 80 || a.contains(',') || a.contains(':') || a.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(Error::invalid("alias", format!("`{a}`: one word, no `,` or `:`")));
    }
    Ok(a.to_string())
}

/// The default alias of a code: its name with spaces as `-`.
pub fn default_alias(c: &CodeSpace) -> String {
    c.name.split_whitespace().collect::<Vec<_>>().join("-").replace([',', ':'], "-")
}

impl Store {
    pub fn spaces(&self) -> Result<Vec<Space>> {
        let mut st = self.conn.prepare(&format!("SELECT {COLS} FROM code_spaces ORDER BY name COLLATE NOCASE"))?;
        Ok(st.query_map([], row)?.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn space(&self, id: &str) -> Result<Option<Space>> {
        Ok(self.conn.query_row(&format!("SELECT {COLS} FROM code_spaces WHERE id = ?1"), [id], row).optional()?)
    }

    /// By id, id prefix or name.
    pub fn find_space(&self, key: &str) -> Result<Space> {
        super::pick(self.spaces()?, key, |s| &s.id, |s| &s.name, "code space", "mnnn spaces list")
    }

    pub fn space_name_taken(&self, name: &str) -> Result<bool> {
        Ok(self.conn.query_row("SELECT COUNT(*) FROM code_spaces WHERE name = ?1 COLLATE NOCASE", [name.trim()], |r| r.get::<_, i64>(0))? > 0)
    }

    /// Create a space with `main` as its main (member at position 0). Not approved yet.
    pub fn create_space(&self, name: &str, description: &str, kind: SpaceKind, main: &CodeSpace) -> Result<Space> {
        let name = check_name(name)?;
        if self.space_name_taken(&name)? {
            return Err(Error::invalid("space name", format!("`{name}` already exists")));
        }
        let now = now_ms();
        let s = Space {
            id: new_id(),
            name,
            description: description.trim().to_string(),
            main_code_id: main.id.clone(),
            kind,
            enabled: true,
            approved_hash: None,
            author_id: self.current_author_id()?,
            created_at: now,
            updated_at: now,
        };
        self.conn.execute(
            &format!("INSERT INTO code_spaces({COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"),
            params![
                s.id,
                s.name,
                s.description,
                s.main_code_id,
                s.kind.as_str(),
                s.enabled,
                s.approved_hash,
                s.author_id,
                s.created_at,
                s.updated_at
            ],
        )?;
        self.conn.execute(
            "INSERT INTO code_space_members(space_id, code_id, alias, position, pin_hash, added_at) VALUES (?1, ?2, ?3, 0, NULL, ?4)",
            params![s.id, main.id, default_alias(main), now],
        )?;
        Ok(s)
    }

    /// The members, main first, then by position.
    pub fn space_members(&self, space_id: &str) -> Result<Vec<SpaceMember>> {
        let mut st = self.conn.prepare(
            "SELECT space_id, code_id, alias, position, pin_hash, added_at FROM code_space_members WHERE space_id = ?1 ORDER BY position, alias",
        )?;
        Ok(st.query_map([space_id], member)?.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// The spaces a code belongs to.
    pub fn spaces_of_code(&self, code_id: &str) -> Result<Vec<Space>> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {} FROM code_spaces s JOIN code_space_members m ON m.space_id = s.id WHERE m.code_id = ?1 ORDER BY s.name COLLATE NOCASE",
            COLS.split(", ").map(|c| format!("s.{c}")).collect::<Vec<_>>().join(", ")
        ))?;
        Ok(st.query_map([code_id], row)?.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Whether a code is a member of any space.
    pub fn code_in_a_space(&self, code_id: &str) -> Result<bool> {
        Ok(self.conn.query_row("SELECT COUNT(*) FROM code_space_members WHERE code_id = ?1", [code_id], |r| r.get::<_, i64>(0))? > 0)
    }

    /// Add a code (alias: its name); `pin`: its current trust hash.
    pub fn add_space_member(&self, space_id: &str, code: &CodeSpace, alias: Option<&str>, pin: Option<&str>) -> Result<SpaceMember> {
        let alias = check_alias(&alias.map(str::to_string).unwrap_or_else(|| default_alias(code)))?;
        let ms = self.space_members(space_id)?;
        if ms.iter().any(|m| m.code_id == code.id) {
            return Err(Error::invalid("space", format!("{} is a member already", code.name)));
        }
        if ms.iter().any(|m| m.alias.eq_ignore_ascii_case(&alias)) {
            return Err(Error::invalid("alias", format!("`{alias}` is taken in this space; give --as ALIAS")));
        }
        let pos = ms.iter().map(|m| m.position).max().unwrap_or(0) + 1;
        let now = now_ms();
        self.conn.execute(
            "INSERT INTO code_space_members(space_id, code_id, alias, position, pin_hash, added_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![space_id, code.id, alias, pos, pin, now],
        )?;
        self.touch_space(space_id)?;
        Ok(SpaceMember {
            space_id: space_id.into(),
            code_id: code.id.clone(),
            alias,
            position: pos,
            pin_hash: pin.map(str::to_string),
            added_at: now,
        })
    }

    /// Remove a member (main can't be removed while it is main).
    pub fn remove_space_member(&self, space_id: &str, code_id: &str) -> Result<()> {
        let s = self.space(space_id)?.ok_or_else(|| Error::invalid("space", "not found"))?;
        if s.main_code_id == code_id {
            return Err(Error::invalid("space", format!("that is the main of {}; set another main first (`mnnn spaces set-main`)", s.name)));
        }
        if self.conn.execute("DELETE FROM code_space_members WHERE space_id = ?1 AND code_id = ?2", params![space_id, code_id])? == 0 {
            return Err(Error::invalid("space", "not a member"));
        }
        self.touch_space(space_id)
    }

    /// Make a member (or a new code, added) the main: it moves to position 0.
    pub fn set_space_main(&self, space_id: &str, code: &CodeSpace) -> Result<()> {
        if !self.space_members(space_id)?.iter().any(|m| m.code_id == code.id) {
            self.add_space_member(space_id, code, None, None)?;
        }
        self.conn.execute("UPDATE code_space_members SET position = position + 1 WHERE space_id = ?1", [space_id])?;
        self.conn.execute("UPDATE code_space_members SET position = 0 WHERE space_id = ?1 AND code_id = ?2", params![space_id, code.id])?;
        self.conn.execute("UPDATE code_spaces SET main_code_id = ?2, updated_at = ?3 WHERE id = ?1", params![space_id, code.id, now_ms()])?;
        Ok(())
    }

    pub fn set_member_pin(&self, space_id: &str, code_id: &str, pin: Option<&str>) -> Result<()> {
        if self.conn.execute("UPDATE code_space_members SET pin_hash = ?3 WHERE space_id = ?1 AND code_id = ?2", params![space_id, code_id, pin])?
            == 0
        {
            return Err(Error::invalid("space", "not a member"));
        }
        self.touch_space(space_id)
    }

    pub fn approve_space(&self, id: &str, hash: Option<&str>) -> Result<()> {
        self.conn.execute("UPDATE code_spaces SET approved_hash = ?2 WHERE id = ?1", params![id, hash])?;
        Ok(())
    }

    pub fn set_space_enabled(&self, id: &str, on: bool) -> Result<()> {
        self.conn.execute("UPDATE code_spaces SET enabled = ?2, updated_at = ?3 WHERE id = ?1", params![id, on, now_ms()])?;
        Ok(())
    }

    pub fn set_space_kind(&self, id: &str, kind: SpaceKind) -> Result<()> {
        self.conn.execute("UPDATE code_spaces SET kind = ?2, updated_at = ?3 WHERE id = ?1", params![id, kind.as_str(), now_ms()])?;
        Ok(())
    }

    pub fn rename_space(&self, id: &str, name: &str) -> Result<()> {
        let n = check_name(name)?;
        if self.spaces()?.iter().any(|s| s.id != id && s.name.eq_ignore_ascii_case(&n)) {
            return Err(Error::invalid("space name", format!("`{n}` already exists")));
        }
        self.conn.execute("UPDATE code_spaces SET name = ?2, updated_at = ?3 WHERE id = ?1", params![id, n, now_ms()])?;
        Ok(())
    }

    fn touch_space(&self, id: &str) -> Result<()> {
        self.conn.execute("UPDATE code_spaces SET updated_at = ?2 WHERE id = ?1", params![id, now_ms()])?;
        Ok(())
    }

    /// Remove a space (its code stays). Project items pointing at it go.
    pub fn remove_space(&self, id: &str) -> Result<Space> {
        let s = self.space(id)?.ok_or_else(|| Error::invalid("space", format!("`{id}` not found")))?;
        self.conn.execute("DELETE FROM project_items WHERE kind = 'space' AND ref_id = ?1", [&s.id])?;
        self.conn.execute("DELETE FROM code_spaces WHERE id = ?1", [&s.id])?;
        Ok(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{CodeInput, Lang};

    #[test]
    fn spaces_members_main_and_rules() {
        let s = Store::open_in_memory().unwrap();
        let a = s.create_code_space("parse a", "", Lang::Math, "x = close", CodeInput::default()).unwrap();
        let b = s.create_code_space("b", "", Lang::Math, "y = close", CodeInput::default()).unwrap();
        let sp = s.create_space("pipe one", "", SpaceKind::Transform, &a).unwrap();
        assert!(s.create_space("PIPE ONE", "", SpaceKind::Analysis, &b).is_err(), "names are unique, any case");
        let ms = s.space_members(&sp.id).unwrap();
        assert_eq!((ms.len(), ms[0].code_id.as_str(), ms[0].alias.as_str(), ms[0].position), (1, a.id.as_str(), "parse-a", 0));
        s.add_space_member(&sp.id, &b, None, Some("h")).unwrap();
        assert!(s.add_space_member(&sp.id, &b, Some("other"), None).is_err(), "once per space");
        assert!(s.remove_space_member(&sp.id, &a.id).is_err(), "main stays");
        assert!(s.remove_code_space(&b.id).is_err(), "member code can't be deleted");
        s.set_space_main(&sp.id, &b).unwrap();
        let sp2 = s.space(&sp.id).unwrap().unwrap();
        assert_eq!(sp2.main_code_id, b.id);
        assert_eq!(s.space_members(&sp.id).unwrap()[0].code_id, b.id);
        s.remove_space_member(&sp.id, &a.id).unwrap();
        assert!(s.code_in_a_space(&b.id).unwrap() && !s.code_in_a_space(&a.id).unwrap());
        assert_eq!(s.spaces_of_code(&b.id).unwrap()[0].id, sp.id);
        assert_eq!(s.find_space("pipe one").unwrap().kind, SpaceKind::Transform);
        s.remove_space(&sp.id).unwrap();
        s.remove_code_space(&b.id).unwrap();
        for k in SpaceKind::ALL {
            assert_eq!(SpaceKind::from_arg(k.as_str()).unwrap(), k);
        }
        assert!(SpaceKind::from_arg("nope").unwrap_err().to_string().contains("ingest-connector"));
    }
}
