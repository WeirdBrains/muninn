//! Reports: documents for people, built from sections.
//!
//! A section is written text (Markdown or LaTeX) or a live view of something in the store:
//! records (bars of a series, with a summary), the series a live search matches, journal
//! entries, a project's contents, the authors. Sections that show data are rendered from
//! the data at export time, so a report always reflects what is stored now. Code-space
//! output (Phase 6) has its own section kind already; it renders a placeholder until then.
//! Reports can be added to projects (item kind `report`). See `docs/reports.md`.
use super::{EntryFormat, Store, now_ms};
use crate::error::{Error, Result};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

pub const MIGRATION: &str = r#"
CREATE TABLE reports (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    author_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE report_sections (
    id TEXT PRIMARY KEY,
    report_id TEXT NOT NULL REFERENCES reports(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    kind TEXT NOT NULL,
    title TEXT NOT NULL DEFAULT '',
    body TEXT NOT NULL DEFAULT '',
    format TEXT NOT NULL DEFAULT 'markdown',
    params TEXT NOT NULL DEFAULT '{}',
    author_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX report_sections_report ON report_sections(report_id, position);
"#;

/// What a section shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SectionKind {
    /// Written text (Markdown or LaTeX).
    Text,
    /// Bars of one series (or a time range of it): a summary and a table.
    Records,
    /// The series a live search matches now (or every series).
    Series,
    /// Entries of a journal (all, or the chosen ones), with notes and highlights.
    Journal,
    /// A project's contents.
    Project,
    /// Who contributed to the report.
    Authors,
    /// Output from a code space (Phase 6); a placeholder until then.
    Code,
    /// Records of a designed model (`mnnn model`): its fields and a table.
    Model,
}

pub const SECTION_KINDS: [SectionKind; 8] = [
    SectionKind::Text,
    SectionKind::Records,
    SectionKind::Series,
    SectionKind::Journal,
    SectionKind::Project,
    SectionKind::Authors,
    SectionKind::Code,
    SectionKind::Model,
];

impl SectionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SectionKind::Text => "text",
            SectionKind::Records => "records",
            SectionKind::Series => "series",
            SectionKind::Journal => "journal",
            SectionKind::Project => "project",
            SectionKind::Authors => "authors",
            SectionKind::Code => "code",
            SectionKind::Model => "model",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        SECTION_KINDS.into_iter().find(|k| k.as_str() == s)
    }
    /// What the kind is, for pickers.
    pub fn describe(self) -> &'static str {
        match self {
            SectionKind::Text => "written text (Markdown or LaTeX)",
            SectionKind::Records => "bars of a series: summary and table",
            SectionKind::Series => "series a live search matches (or all)",
            SectionKind::Journal => "journal entries with notes and highlights",
            SectionKind::Project => "a project's contents",
            SectionKind::Authors => "who contributed",
            SectionKind::Code => "code-space output (coming with code spaces)",
            SectionKind::Model => "records of a designed model",
        }
    }
}

/// Default number of bars a records section lists (0 lists all).
pub const DEFAULT_ROWS: usize = 20;

/// Kind-specific settings of a section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SectionParams {
    /// Records: the series (and range).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series: Option<super::SeriesRef>,
    /// Records: how many bars to list (0: all; the summary always covers the range).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<usize>,
    /// Series: the live search (None: every series).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,
    /// Journal: the journal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub journal: Option<String>,
    /// Journal: chosen entries (empty: all).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<String>,
    /// Project: the project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// Code: the code space.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space: Option<String>,
    /// Model: the model (`rows` limits the records listed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Model: only records meeting these conditions.
    #[serde(default, rename = "where", skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<super::Condition>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub id: String,
    pub name: String,
    pub description: String,
    pub author_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Report {
    pub fn short_id(&self) -> &str {
        super::short(&self.id)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Section {
    pub id: String,
    pub report_id: String,
    /// 1-based order in the report.
    pub position: i64,
    pub kind: SectionKind,
    pub title: String,
    /// Text sections: the text.
    pub body: String,
    pub format: EntryFormat,
    pub params: SectionParams,
    pub author_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Section {
    pub fn short_id(&self) -> &str {
        super::short(&self.id)
    }
    /// Title, else what the section shows.
    pub fn heading(&self) -> String {
        if !self.title.trim().is_empty() {
            return self.title.trim().to_string();
        }
        match self.kind {
            SectionKind::Text => {
                let first = self.body.lines().find(|l| !l.trim().is_empty()).unwrap_or("(empty)").trim();
                first.trim_start_matches('#').trim().chars().take(60).collect()
            }
            SectionKind::Records => self.params.series.as_ref().map(|s| s.label()).unwrap_or_else(|| "Records".into()),
            SectionKind::Series => "Series".into(),
            SectionKind::Journal => "Journal".into(),
            SectionKind::Project => "Project".into(),
            SectionKind::Authors => "Authors".into(),
            SectionKind::Code => format!("Code space {}", self.params.space.clone().unwrap_or_default()).trim().to_string(),
            SectionKind::Model => "Records".into(),
        }
    }
}

/// A section to add.
#[derive(Debug, Clone)]
pub struct NewSection {
    pub kind: SectionKind,
    pub title: String,
    pub body: String,
    pub format: EntryFormat,
    pub params: SectionParams,
}

impl NewSection {
    pub fn new(kind: SectionKind) -> Self {
        NewSection { kind, title: String::new(), body: String::new(), format: EntryFormat::Markdown, params: SectionParams::default() }
    }
    pub fn text(title: &str, body: &str, format: EntryFormat) -> Self {
        NewSection { title: title.into(), body: body.into(), format, ..Self::new(SectionKind::Text) }
    }
}

/// Changes to a section; `None` keeps a value.
#[derive(Debug, Clone, Default)]
pub struct SectionChange {
    pub title: Option<String>,
    pub body: Option<String>,
    pub format: Option<EntryFormat>,
    pub params: Option<SectionParams>,
}

fn check_name(name: &str) -> Result<String> {
    let n = name.trim();
    if n.is_empty() {
        return Err(Error::invalid("report name", "must not be empty"));
    }
    Ok(n.to_string())
}

const RCOLS: &str = "id, name, description, author_id, created_at, updated_at";
const SCOLS: &str = "id, report_id, position, kind, title, body, format, params, author_id, created_at, updated_at";

impl Store {
    fn row_report(r: &rusqlite::Row) -> rusqlite::Result<Report> {
        Ok(Report { id: r.get(0)?, name: r.get(1)?, description: r.get(2)?, author_id: r.get(3)?, created_at: r.get(4)?, updated_at: r.get(5)? })
    }
    fn row_section(r: &rusqlite::Row) -> rusqlite::Result<Section> {
        let kind: String = r.get(3)?;
        let format: String = r.get(6)?;
        let params: String = r.get(7)?;
        Ok(Section {
            id: r.get(0)?,
            report_id: r.get(1)?,
            position: r.get(2)?,
            kind: SectionKind::parse(&kind).unwrap_or(SectionKind::Text),
            title: r.get(4)?,
            body: r.get(5)?,
            format: EntryFormat::parse(&format).unwrap_or_default(),
            params: serde_json::from_str(&params).unwrap_or_default(),
            author_id: r.get(8)?,
            created_at: r.get(9)?,
            updated_at: r.get(10)?,
        })
    }

    pub fn reports(&self) -> Result<Vec<Report>> {
        let mut st = self.conn.prepare(&format!("SELECT {RCOLS} FROM reports ORDER BY name COLLATE NOCASE, created_at"))?;
        let v = st.query_map([], Self::row_report)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn report(&self, id: &str) -> Result<Option<Report>> {
        Ok(self.conn.query_row(&format!("SELECT {RCOLS} FROM reports WHERE id = ?1"), [id], Self::row_report).optional()?)
    }

    /// By id, unique id prefix or exact name (case-insensitive).
    pub fn find_report(&self, key: &str) -> Result<Report> {
        super::pick(self.reports()?, key, |r| &r.id, |r| &r.name, "report", "mnnn report list")
    }

    pub fn create_report(&self, name: &str, description: &str) -> Result<Report> {
        let now = now_ms();
        let r = Report {
            id: super::new_id(),
            name: check_name(name)?,
            description: description.trim().to_string(),
            author_id: self.current_author_id()?,
            created_at: now,
            updated_at: now,
        };
        self.conn.execute(
            &format!("INSERT INTO reports({RCOLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6)"),
            params![r.id, r.name, r.description, r.author_id, r.created_at, r.updated_at],
        )?;
        Ok(r)
    }

    pub fn update_report(&self, id: &str, name: Option<&str>, description: Option<&str>) -> Result<Report> {
        let mut r = self.report(id)?.ok_or_else(|| Error::invalid("report", format!("`{id}` not found")))?;
        if let Some(n) = name {
            r.name = check_name(n)?;
        }
        if let Some(d) = description {
            r.description = d.trim().to_string();
        }
        r.updated_at = now_ms();
        self.conn.execute(
            "UPDATE reports SET name = ?2, description = ?3, updated_at = ?4 WHERE id = ?1",
            params![r.id, r.name, r.description, r.updated_at],
        )?;
        Ok(r)
    }

    fn touch_report(&self, id: &str) -> Result<()> {
        self.conn.execute("UPDATE reports SET updated_at = ?2 WHERE id = ?1", params![id, now_ms()])?;
        Ok(())
    }

    /// Delete a report (and its sections) and take it out of every project.
    pub fn remove_report(&self, id: &str) -> Result<Report> {
        let r = self.report(id)?.ok_or_else(|| Error::invalid("report", format!("`{id}` not found")))?;
        self.conn.execute("DELETE FROM project_items WHERE kind = 'report' AND ref_id = ?1", [id])?;
        self.conn.execute("DELETE FROM reports WHERE id = ?1", [id])?;
        Ok(r)
    }

    pub fn sections(&self, report: &str) -> Result<Vec<Section>> {
        let mut st = self.conn.prepare(&format!("SELECT {SCOLS} FROM report_sections WHERE report_id = ?1 ORDER BY position, created_at"))?;
        let v = st.query_map([report], Self::row_section)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn section(&self, id: &str) -> Result<Option<Section>> {
        Ok(self.conn.query_row(&format!("SELECT {SCOLS} FROM report_sections WHERE id = ?1"), [id], Self::row_section).optional()?)
    }

    /// A section of a report by position (1-based), id or unique id prefix.
    pub fn find_section(&self, report: &str, key: &str) -> Result<Section> {
        let k = key.trim();
        let all = self.sections(report)?;
        if let Ok(n) = k.parse::<usize>()
            && n >= 1
            && n <= all.len()
            && k.len() < 4
        {
            return Ok(all[n - 1].clone());
        }
        let hits: Vec<&Section> = all.iter().filter(|s| s.id == k || (!k.is_empty() && s.id.starts_with(k))).collect();
        match hits.as_slice() {
            [one] => Ok((*one).clone()),
            [] => Err(Error::invalid("section", format!("`{k}` not found (use its number or id from `mnnn report show`)"))),
            _ => Err(Error::invalid("section", format!("`{k}` matches more than one; use more of the id"))),
        }
    }

    fn check_section(&self, s: &NewSection) -> Result<()> {
        let p = &s.params;
        match s.kind {
            SectionKind::Records if p.series.is_none() => Err(Error::invalid("records section", "needs a series")),
            SectionKind::Series => match &p.search {
                Some(id) if self.live_search(id)?.is_none() => Err(Error::invalid("live search", format!("`{id}` not found"))),
                _ => Ok(()),
            },
            SectionKind::Journal => match &p.journal {
                Some(id) if self.journal(id)?.is_some() => Ok(()),
                Some(id) => Err(Error::invalid("journal", format!("`{id}` not found"))),
                None => Err(Error::invalid("journal section", "needs a journal")),
            },
            SectionKind::Project => match &p.project {
                Some(id) if self.project(id)?.is_some() => Ok(()),
                Some(id) => Err(Error::invalid("project", format!("`{id}` not found"))),
                None => Err(Error::invalid("project section", "needs a project")),
            },
            _ => Ok(()),
        }
    }

    /// Add a section at the end of a report.
    pub fn add_section(&self, report: &str, s: &NewSection) -> Result<Section> {
        if self.report(report)?.is_none() {
            return Err(Error::invalid("report", format!("`{report}` not found")));
        }
        self.check_section(s)?;
        let pos: i64 =
            self.conn.query_row("SELECT COALESCE(MAX(position), 0) + 1 FROM report_sections WHERE report_id = ?1", [report], |r| r.get(0))?;
        let now = now_ms();
        let sec = Section {
            id: super::new_id(),
            report_id: report.to_string(),
            position: pos,
            kind: s.kind,
            title: s.title.trim().to_string(),
            body: s.body.clone(),
            format: s.format,
            params: s.params.clone(),
            author_id: self.current_author_id()?,
            created_at: now,
            updated_at: now,
        };
        self.insert_section(&sec)?;
        self.touch_report(report)?;
        Ok(sec)
    }

    pub(crate) fn insert_section(&self, s: &Section) -> Result<()> {
        self.conn.execute(
            &format!("INSERT INTO report_sections({SCOLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"),
            params![
                s.id,
                s.report_id,
                s.position,
                s.kind.as_str(),
                s.title,
                s.body,
                s.format.as_str(),
                serde_json::to_string(&s.params)?,
                s.author_id,
                s.created_at,
                s.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn update_section(&self, id: &str, c: &SectionChange) -> Result<Section> {
        let mut s = self.section(id)?.ok_or_else(|| Error::invalid("section", format!("`{id}` not found")))?;
        if let Some(t) = &c.title {
            s.title = t.trim().to_string();
        }
        if let Some(b) = &c.body {
            s.body = b.clone();
        }
        if let Some(f) = c.format {
            s.format = f;
        }
        if let Some(p) = &c.params {
            s.params = p.clone();
        }
        self.check_section(&NewSection { kind: s.kind, title: s.title.clone(), body: s.body.clone(), format: s.format, params: s.params.clone() })?;
        s.updated_at = now_ms();
        self.conn.execute(
            "UPDATE report_sections SET title = ?2, body = ?3, format = ?4, params = ?5, updated_at = ?6 WHERE id = ?1",
            params![s.id, s.title, s.body, s.format.as_str(), serde_json::to_string(&s.params)?, s.updated_at],
        )?;
        self.touch_report(&s.report_id)?;
        Ok(s)
    }

    fn renumber_sections(&self, report: &str, order: &[String]) -> Result<()> {
        for (i, id) in order.iter().enumerate() {
            self.conn.execute("UPDATE report_sections SET position = ?2 WHERE id = ?1", params![id, i as i64 + 1])?;
        }
        self.touch_report(report)
    }

    pub fn remove_section(&self, id: &str) -> Result<Section> {
        let s = self.section(id)?.ok_or_else(|| Error::invalid("section", format!("`{id}` not found")))?;
        self.conn.execute("DELETE FROM report_sections WHERE id = ?1", [id])?;
        let order: Vec<String> = self.sections(&s.report_id)?.into_iter().map(|s| s.id).collect();
        self.renumber_sections(&s.report_id, &order)?;
        Ok(s)
    }

    /// Move a section to position `to` (1-based; clamped to the report).
    pub fn move_section(&self, id: &str, to: usize) -> Result<Section> {
        let s = self.section(id)?.ok_or_else(|| Error::invalid("section", format!("`{id}` not found")))?;
        let mut order: Vec<String> = self.sections(&s.report_id)?.into_iter().map(|s| s.id).collect();
        order.retain(|x| x != id);
        let at = to.clamp(1, order.len() + 1) - 1;
        order.insert(at, id.to_string());
        self.renumber_sections(&s.report_id, &order)?;
        Ok(self.section(id)?.expect("still there"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Duration, Symbol};
    use crate::store::{ItemKind, SeriesRef};

    #[test]
    fn reports_hold_ordered_sections() {
        let s = Store::open_in_memory().unwrap();
        let a = s.add_author("Ada", None, &[]).unwrap();
        let r = s.create_report("Weekly", "what happened").unwrap();
        assert_eq!(r.author_id.as_deref(), Some(a.id.as_str()));
        assert!(s.create_report(" ", "").is_err());
        let t = s.add_section(&r.id, &NewSection::text("Intro", "# Hi\n\ntext", EntryFormat::Markdown)).unwrap();
        let mut rec = NewSection::new(SectionKind::Records);
        assert!(s.add_section(&r.id, &rec).is_err(), "records need a series");
        rec.params.series = Some(SeriesRef { source: "b".into(), symbol: Symbol::new("BTC", "USDT"), duration: Duration(300), from: None, to: None });
        let r2 = s.add_section(&r.id, &rec).unwrap();
        let au = s.add_section(&r.id, &NewSection::new(SectionKind::Authors)).unwrap();
        let mut j = NewSection::new(SectionKind::Journal);
        j.params.journal = Some("nope".into());
        assert!(s.add_section(&r.id, &j).is_err());
        assert_eq!(s.sections(&r.id).unwrap().iter().map(|x| x.position).collect::<Vec<_>>(), vec![1, 2, 3]);
        assert_eq!(r2.heading(), "BTC/USDT b 5m");
        assert_eq!(s.find_section(&r.id, "2").unwrap().id, r2.id);
        assert_eq!(s.find_section(&r.id, &au.id[..6]).unwrap().id, au.id);
        s.move_section(&au.id, 1).unwrap();
        let ids: Vec<String> = s.sections(&r.id).unwrap().into_iter().map(|x| x.id).collect();
        assert_eq!(ids, vec![au.id.clone(), t.id.clone(), r2.id.clone()]);
        let e = s.update_section(&t.id, &SectionChange { body: Some("new".into()), ..Default::default() }).unwrap();
        assert_eq!((e.title.as_str(), e.body.as_str()), ("Intro", "new"));
        s.remove_section(&au.id).unwrap();
        assert_eq!(s.sections(&r.id).unwrap().iter().map(|x| x.position).collect::<Vec<_>>(), vec![1, 2]);
        let p = s.create_project("P", "").unwrap();
        s.add_project_item(&p.id, ItemKind::Report, &r.id, serde_json::json!({})).unwrap();
        assert_eq!(s.find_report("weekly").unwrap().id, r.id);
        s.update_report(&r.id, Some("Weekly 44"), None).unwrap();
        s.remove_report(&r.id).unwrap();
        assert!(s.project_items(&p.id).unwrap().is_empty(), "removed from projects");
        assert!(s.sections(&r.id).unwrap().is_empty());
    }
}
