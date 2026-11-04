//! Pipelines: saved, named graphs (DAGs) binding data to code spaces and ending in model
//! tables. The graph is [`crate::code::pipeline::Graph`]; see `docs/pipelines.md`.
use super::{Store, new_id, now_ms};
use crate::code::pipeline::Graph;
use crate::error::{Error, Result};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

pub const MIGRATION: &str = r#"
CREATE TABLE pipelines (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    description TEXT NOT NULL DEFAULT '',
    version INTEGER NOT NULL DEFAULT 1,
    graph TEXT NOT NULL,
    author_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE pipeline_versions (
    pipeline_id TEXT NOT NULL REFERENCES pipelines(id) ON DELETE CASCADE,
    version INTEGER NOT NULL,
    graph TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (pipeline_id, version)
);
"#;

/// Pipelines from peers arrive as drafts: they run once approved.
pub const MIGRATION_DRAFT: &str = "ALTER TABLE pipelines ADD COLUMN draft INTEGER NOT NULL DEFAULT 0;";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pipeline {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: i64,
    pub graph: Graph,
    pub author_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    /// From a peer (`net fetch`) and not approved yet: it does not run.
    #[serde(default)]
    pub draft: bool,
}

const COLS: &str = "id, name, description, version, graph, author_id, created_at, updated_at, draft";

fn row(r: &rusqlite::Row) -> rusqlite::Result<Pipeline> {
    let g: String = r.get(4)?;
    Ok(Pipeline {
        id: r.get(0)?,
        name: r.get(1)?,
        description: r.get(2)?,
        version: r.get(3)?,
        graph: serde_json::from_str(&g).unwrap_or_default(),
        author_id: r.get(5)?,
        created_at: r.get(6)?,
        updated_at: r.get(7)?,
        draft: r.get::<_, i64>(8)? != 0,
    })
}

impl Store {
    pub fn pipelines(&self) -> Result<Vec<Pipeline>> {
        let mut st = self.conn.prepare(&format!("SELECT {COLS} FROM pipelines ORDER BY name COLLATE NOCASE"))?;
        Ok(st.query_map([], row)?.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn pipeline(&self, id: &str) -> Result<Option<Pipeline>> {
        Ok(self.conn.query_row(&format!("SELECT {COLS} FROM pipelines WHERE id = ?1"), [id], row).optional()?)
    }

    /// By name (any case), id or id prefix.
    pub fn find_pipeline(&self, key: &str) -> Result<Pipeline> {
        let all = self.pipelines()?;
        if let Some(p) = all.iter().find(|p| p.name.eq_ignore_ascii_case(key) || p.id == key) {
            return Ok(p.clone());
        }
        let v: Vec<&Pipeline> = all.iter().filter(|p| key.len() >= 4 && p.id.starts_with(key)).collect();
        match v.as_slice() {
            [one] => Ok((*one).clone()),
            [] => Err(Error::invalid("pipeline", format!("no pipeline `{key}`"))),
            _ => Err(Error::invalid("pipeline", format!("`{key}` matches several pipelines"))),
        }
    }

    pub fn create_pipeline(&self, name: &str, description: &str, graph: &Graph) -> Result<Pipeline> {
        if name.trim().is_empty() {
            return Err(Error::invalid("pipeline", "a pipeline needs a name"));
        }
        if self.find_pipeline(name).is_ok() {
            return Err(Error::invalid("pipeline", format!("a pipeline `{name}` exists already")));
        }
        let now = now_ms();
        let p = Pipeline {
            id: new_id(),
            name: name.trim().to_string(),
            description: description.to_string(),
            version: 1,
            graph: graph.clone(),
            author_id: self.current_author_id()?,
            created_at: now,
            updated_at: now,
            draft: false,
        };
        let g = serde_json::to_string(&p.graph)?;
        self.conn.execute(
            &format!("INSERT INTO pipelines ({COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0)"),
            params![p.id, p.name, p.description, p.version, g, p.author_id, p.created_at, p.updated_at],
        )?;
        self.conn.execute("INSERT INTO pipeline_versions VALUES (?1, 1, ?2, ?3)", params![p.id, g, now])?;
        Ok(p)
    }

    /// A new version of the graph (and maybe a new name or description).
    pub fn update_pipeline(&self, id: &str, name: Option<&str>, description: Option<&str>, graph: Option<&Graph>) -> Result<Pipeline> {
        let mut p = self.pipeline(id)?.ok_or_else(|| Error::invalid("pipeline", "it no longer exists"))?;
        if let Some(n) = name {
            if let Ok(o) = self.find_pipeline(n)
                && o.id != p.id
            {
                return Err(Error::invalid("pipeline", format!("a pipeline `{n}` exists already")));
            }
            p.name = n.trim().to_string();
        }
        if let Some(d) = description {
            p.description = d.to_string();
        }
        let now = now_ms();
        if let Some(g) = graph
            && *g != p.graph
        {
            p.graph = g.clone();
            p.version += 1;
            self.conn
                .execute("INSERT INTO pipeline_versions VALUES (?1, ?2, ?3, ?4)", params![p.id, p.version, serde_json::to_string(&p.graph)?, now])?;
        }
        p.updated_at = now;
        self.conn.execute(
            "UPDATE pipelines SET name = ?2, description = ?3, version = ?4, graph = ?5, updated_at = ?6 WHERE id = ?1",
            params![p.id, p.name, p.description, p.version, serde_json::to_string(&p.graph)?, p.updated_at],
        )?;
        Ok(p)
    }

    pub fn remove_pipeline(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM pipelines WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Mark a pipeline a draft (from a peer) or approve it (`draft` false).
    pub fn set_pipeline_draft(&self, id: &str, draft: bool) -> Result<()> {
        self.conn.execute("UPDATE pipelines SET draft = ?2, updated_at = ?3 WHERE id = ?1", params![id, draft as i64, now_ms()])?;
        Ok(())
    }

    /// Every version of a pipeline's graph, oldest first.
    pub fn pipeline_versions(&self, id: &str) -> Result<Vec<(i64, Graph, i64)>> {
        let mut st = self.conn.prepare("SELECT version, graph, created_at FROM pipeline_versions WHERE pipeline_id = ?1 ORDER BY version")?;
        let v = st
            .query_map([id], |r| Ok((r.get::<_, i64>(0)?, serde_json::from_str(&r.get::<_, String>(1)?).unwrap_or_default(), r.get(2)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }
}
