//! Journals: entries in Markdown or LaTeX, with images, notes and highlights.
//!
//! A journal is standalone or belongs to one project (a project can have many). Entries
//! keep their own format. Notes attach to the journal, to an entry, or to a span of an
//! entry's text; highlights mark a span. Spans are character offsets plus the quoted text,
//! so editing an entry moves them along with their words (or detaches them if the words
//! are gone). Images are stored in the database and referenced from the text as
//! `attachment:ID`. Everything records the author who created it. See `docs/journals.md`.
//!
//! [`JournalDoc`] is the whole journal as one serializable value: `mnnn journal show --json`
//! prints it, and the Phase 3 exporters (HTML, JSON, LaTeX, PDF, Docx) render from it.
use super::{Store, now_ms};
use crate::error::{Error, Result};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const MIGRATION: &str = r#"
CREATE TABLE journals (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
    format TEXT NOT NULL DEFAULT 'markdown',
    forked_from TEXT,
    author_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE journal_entries (
    id TEXT PRIMARY KEY,
    journal_id TEXT NOT NULL REFERENCES journals(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    title TEXT NOT NULL DEFAULT '',
    body TEXT NOT NULL DEFAULT '',
    format TEXT NOT NULL DEFAULT 'markdown',
    forked_from TEXT,
    author_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX journal_entries_journal ON journal_entries(journal_id, position);
CREATE TABLE journal_annotations (
    id TEXT PRIMARY KEY,
    journal_id TEXT NOT NULL REFERENCES journals(id) ON DELETE CASCADE,
    entry_id TEXT REFERENCES journal_entries(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    start INTEGER,
    end INTEGER,
    quote TEXT,
    body TEXT NOT NULL DEFAULT '',
    color TEXT,
    author_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX journal_annotations_journal ON journal_annotations(journal_id);
CREATE TABLE journal_attachments (
    id TEXT PRIMARY KEY,
    entry_id TEXT NOT NULL REFERENCES journal_entries(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    mime TEXT NOT NULL,
    data BLOB NOT NULL,
    author_id TEXT,
    created_at INTEGER NOT NULL
);
"#;

/// Largest image accepted (bytes).
pub const MAX_ATTACHMENT: usize = 20 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum EntryFormat {
    #[default]
    Markdown,
    Latex,
}

impl EntryFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            EntryFormat::Markdown => "markdown",
            EntryFormat::Latex => "latex",
        }
    }
    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_lowercase().as_str() {
            "markdown" | "md" => Ok(EntryFormat::Markdown),
            "latex" | "tex" => Ok(EntryFormat::Latex),
            o => Err(Error::invalid("format", format!("`{o}` (use markdown or latex)"))),
        }
    }
    pub fn toggle(self) -> Self {
        match self {
            EntryFormat::Markdown => EntryFormat::Latex,
            EntryFormat::Latex => EntryFormat::Markdown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Journal {
    pub id: String,
    pub name: String,
    /// The project that owns this journal (None: standalone).
    pub project_id: Option<String>,
    /// Format for new entries.
    pub format: EntryFormat,
    pub forked_from: Option<String>,
    pub author_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Journal {
    pub fn short_id(&self) -> &str {
        super::short(&self.id)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub journal_id: String,
    pub position: i64,
    pub title: String,
    pub body: String,
    pub format: EntryFormat,
    /// The entry this one was copied from (fork or copy to another journal).
    pub forked_from: Option<String>,
    pub author_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Entry {
    pub fn short_id(&self) -> &str {
        super::short(&self.id)
    }
    /// Title, else the first line of the text.
    pub fn heading(&self) -> String {
        let t = self.title.trim();
        if !t.is_empty() {
            return t.to_string();
        }
        let first = self.body.lines().find(|l| !l.trim().is_empty()).unwrap_or("(empty)").trim();
        first.trim_start_matches('#').trim().chars().take(60).collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnnotationKind {
    Note,
    Highlight,
}

impl AnnotationKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            AnnotationKind::Note => "note",
            AnnotationKind::Highlight => "highlight",
        }
    }
}

/// A note or highlight. `entry_id` None: a note on the whole journal. A span (`start..end`,
/// character offsets into the entry text) marks selected words; `quote` is their text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Annotation {
    pub id: String,
    pub journal_id: String,
    pub entry_id: Option<String>,
    pub kind: AnnotationKind,
    pub start: Option<usize>,
    pub end: Option<usize>,
    pub quote: Option<String>,
    pub body: String,
    pub color: Option<String>,
    pub author_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Annotation {
    pub fn short_id(&self) -> &str {
        super::short(&self.id)
    }
    pub fn span(&self) -> Option<(usize, usize)> {
        Some((self.start?, self.end?))
    }
    /// Quoted words whose text no longer exists in the entry.
    pub fn detached(&self) -> bool {
        self.quote.is_some() && self.span().is_none()
    }
}

/// Attachment metadata (the bytes come from [`Store::attachment_data`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attachment {
    pub id: String,
    pub entry_id: String,
    pub name: String,
    pub mime: String,
    pub size: i64,
    pub author_id: Option<String>,
    pub created_at: i64,
}

impl Attachment {
    pub fn short_id(&self) -> &str {
        super::short(&self.id)
    }
    /// How an entry refers to the image.
    pub fn reference(&self, f: EntryFormat) -> String {
        match f {
            EntryFormat::Markdown => format!("![{}](attachment:{})", self.name, self.id),
            EntryFormat::Latex => format!("\\includegraphics{{attachment:{}}}", self.id),
        }
    }
}

/// A selection of entry text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

/// Image type from a file name (only images can be attached).
pub fn image_mime(name: &str) -> Result<&'static str> {
    let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
    Ok(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        _ => return Err(Error::invalid("image", format!("`{name}` (use png, jpg, gif, webp, svg or bmp)"))),
    })
}

/// Characters `start..end` of `s`.
pub fn chars_slice(s: &str, start: usize, end: usize) -> String {
    s.chars().skip(start).take(end.saturating_sub(start)).collect()
}

/// Character spans of every occurrence of `needle` in `hay`.
pub fn find_all(hay: &str, needle: &str) -> Vec<Span> {
    if needle.is_empty() {
        return vec![];
    }
    let mut out = vec![];
    let mut from = 0;
    while let Some(p) = hay[from..].find(needle) {
        let b = from + p;
        let start = hay[..b].chars().count();
        out.push(Span { start, end: start + needle.chars().count() });
        from = b + needle.len().max(1);
        if from > hay.len() {
            break;
        }
    }
    out
}

/// The `nth` (1-based) occurrence of `text` in an entry, as a span.
pub fn select(body: &str, text: &str, nth: usize) -> Result<Span> {
    let all = find_all(body, text);
    let n = nth.max(1);
    all.get(n - 1).copied().ok_or_else(|| {
        if all.is_empty() {
            Error::invalid("selection", format!("`{text}` is not in the entry"))
        } else {
            Error::invalid("selection", format!("`{text}` occurs {} times; --nth {n} is out of range", all.len()))
        }
    })
}

/// Check a span against an entry's text.
pub fn check_span(body: &str, sp: Span) -> Result<Span> {
    let n = body.chars().count();
    if sp.start >= sp.end || sp.end > n {
        return Err(Error::invalid("selection", format!("{}..{} (the entry has {n} characters)", sp.start, sp.end)));
    }
    Ok(sp)
}

/// Where a quote sits after an edit: unchanged, the nearest occurrence, or nowhere.
fn reanchor(body: &str, a: &Annotation) -> Option<Span> {
    let q = a.quote.as_deref()?;
    if let Some((s, e)) = a.span()
        && chars_slice(body, s, e) == q
    {
        return Some(Span { start: s, end: e });
    }
    let old = a.start.unwrap_or(0) as i64;
    find_all(body, q).into_iter().min_by_key(|sp| (sp.start as i64 - old).abs())
}

/// The whole journal as one value: what `--json` prints and Phase 3 exports render.
#[derive(Debug, Clone, Serialize)]
pub struct JournalDoc {
    pub journal: Journal,
    pub project: Option<String>,
    pub author: Option<String>,
    /// Notes on the whole journal.
    pub notes: Vec<AnnotationDoc>,
    pub entries: Vec<EntryDoc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EntryDoc {
    pub entry: Entry,
    pub author: Option<String>,
    pub annotations: Vec<AnnotationDoc>,
    pub attachments: Vec<Attachment>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnnotationDoc {
    #[serde(flatten)]
    pub annotation: Annotation,
    pub author: Option<String>,
}

fn check_name(name: &str) -> Result<String> {
    let n = name.trim();
    if n.is_empty() {
        return Err(Error::invalid("journal name", "must not be empty"));
    }
    Ok(n.to_string())
}

const JCOLS: &str = "id, name, project_id, format, forked_from, author_id, created_at, updated_at";
const ECOLS: &str = "id, journal_id, position, title, body, format, forked_from, author_id, created_at, updated_at";
const ACOLS: &str = "id, journal_id, entry_id, kind, start, end, quote, body, color, author_id, created_at, updated_at";
const FCOLS: &str = "id, entry_id, name, mime, length(data), author_id, created_at";

fn fmt_of(s: String) -> EntryFormat {
    EntryFormat::parse(&s).unwrap_or_default()
}

impl Store {
    fn row_journal(r: &rusqlite::Row) -> rusqlite::Result<Journal> {
        Ok(Journal {
            id: r.get(0)?,
            name: r.get(1)?,
            project_id: r.get(2)?,
            format: fmt_of(r.get(3)?),
            forked_from: r.get(4)?,
            author_id: r.get(5)?,
            created_at: r.get(6)?,
            updated_at: r.get(7)?,
        })
    }
    fn row_entry(r: &rusqlite::Row) -> rusqlite::Result<Entry> {
        Ok(Entry {
            id: r.get(0)?,
            journal_id: r.get(1)?,
            position: r.get(2)?,
            title: r.get(3)?,
            body: r.get(4)?,
            format: fmt_of(r.get(5)?),
            forked_from: r.get(6)?,
            author_id: r.get(7)?,
            created_at: r.get(8)?,
            updated_at: r.get(9)?,
        })
    }
    fn row_annotation(r: &rusqlite::Row) -> rusqlite::Result<Annotation> {
        let kind: String = r.get(3)?;
        let us = |v: Option<i64>| v.map(|x| x.max(0) as usize);
        Ok(Annotation {
            id: r.get(0)?,
            journal_id: r.get(1)?,
            entry_id: r.get(2)?,
            kind: if kind == "highlight" { AnnotationKind::Highlight } else { AnnotationKind::Note },
            start: us(r.get(4)?),
            end: us(r.get(5)?),
            quote: r.get(6)?,
            body: r.get(7)?,
            color: r.get(8)?,
            author_id: r.get(9)?,
            created_at: r.get(10)?,
            updated_at: r.get(11)?,
        })
    }
    fn row_attachment(r: &rusqlite::Row) -> rusqlite::Result<Attachment> {
        Ok(Attachment {
            id: r.get(0)?,
            entry_id: r.get(1)?,
            name: r.get(2)?,
            mime: r.get(3)?,
            size: r.get(4)?,
            author_id: r.get(5)?,
            created_at: r.get(6)?,
        })
    }

    // ---- journals ----

    pub fn journals(&self) -> Result<Vec<Journal>> {
        let mut st = self.conn.prepare(&format!("SELECT {JCOLS} FROM journals ORDER BY name COLLATE NOCASE, created_at"))?;
        let v = st.query_map([], Self::row_journal)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn journal(&self, id: &str) -> Result<Option<Journal>> {
        Ok(self.conn.query_row(&format!("SELECT {JCOLS} FROM journals WHERE id = ?1"), [id], Self::row_journal).optional()?)
    }

    pub fn find_journal(&self, key: &str) -> Result<Journal> {
        super::pick(self.journals()?, key, |j| &j.id, |j| &j.name, "journal", "mnnn journal list")
    }

    /// Journals of a project: the ones it owns and the ones added to it.
    pub fn project_journals(&self, project: &str) -> Result<Vec<Journal>> {
        let linked: Vec<String> =
            self.project_items(project)?.into_iter().filter(|i| i.kind == super::projects::ItemKind::Journal).map(|i| i.ref_id).collect();
        Ok(self.journals()?.into_iter().filter(|j| j.project_id.as_deref() == Some(project) || linked.contains(&j.id)).collect())
    }

    /// Create a journal, standalone or in a project (it is then also listed among the project's items).
    pub fn create_journal(&self, name: &str, project: Option<&str>, format: EntryFormat) -> Result<Journal> {
        self.create_journal_from(name, project, format, None)
    }

    fn create_journal_from(&self, name: &str, project: Option<&str>, format: EntryFormat, forked_from: Option<&str>) -> Result<Journal> {
        if let Some(p) = project
            && self.project(p)?.is_none()
        {
            return Err(Error::invalid("project", format!("`{p}` not found")));
        }
        let now = now_ms();
        let j = Journal {
            id: super::new_id(),
            name: check_name(name)?,
            project_id: project.map(str::to_string),
            format,
            forked_from: forked_from.map(str::to_string),
            author_id: self.current_author_id()?,
            created_at: now,
            updated_at: now,
        };
        self.conn.execute(
            &format!("INSERT INTO journals({JCOLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"),
            params![j.id, j.name, j.project_id, j.format.as_str(), j.forked_from, j.author_id, j.created_at, j.updated_at],
        )?;
        if let Some(p) = project {
            self.add_project_item(p, super::projects::ItemKind::Journal, &j.id, serde_json::json!({}))?;
        }
        Ok(j)
    }

    pub fn update_journal(&self, id: &str, name: Option<&str>, format: Option<EntryFormat>) -> Result<Journal> {
        let mut j = self.journal(id)?.ok_or_else(|| Error::invalid("journal", format!("`{id}` not found")))?;
        if let Some(n) = name {
            j.name = check_name(n)?;
        }
        if let Some(f) = format {
            j.format = f;
        }
        self.touch_journal(&mut j)?;
        Ok(j)
    }

    fn touch_journal(&self, j: &mut Journal) -> Result<()> {
        j.updated_at = now_ms();
        self.conn.execute(
            "UPDATE journals SET name = ?2, format = ?3, updated_at = ?4 WHERE id = ?1",
            params![j.id, j.name, j.format.as_str(), j.updated_at],
        )?;
        Ok(())
    }

    /// Delete a journal with its entries, notes, highlights and images.
    pub fn remove_journal(&self, id: &str) -> Result<Journal> {
        let j = self.journal(id)?.ok_or_else(|| Error::invalid("journal", format!("`{id}` not found")))?;
        self.conn.execute("DELETE FROM project_items WHERE kind = 'journal' AND ref_id = ?1", [id])?;
        self.conn.execute("DELETE FROM journals WHERE id = ?1", [id])?;
        Ok(j)
    }

    /// Fork a journal (entries, notes, highlights, images) into a project-specific copy.
    pub fn fork_journal(&self, id: &str, project: Option<&str>, name: Option<&str>) -> Result<Journal> {
        let o = self.journal(id)?.ok_or_else(|| Error::invalid("journal", format!("`{id}` not found")))?;
        let name = name.map(str::to_string).unwrap_or_else(|| format!("{} (fork)", o.name));
        let j = self.create_journal_from(&name, project, o.format, Some(&o.id))?;
        let ids: Vec<String> = self.entries(&o.id)?.into_iter().map(|e| e.id).collect();
        self.copy_entries(&ids, &j.id)?;
        for n in self.annotations(&o.id)?.into_iter().filter(|a| a.entry_id.is_none()) {
            self.insert_annotation(&Annotation { id: super::new_id(), journal_id: j.id.clone(), ..n })?;
        }
        Ok(j)
    }

    // ---- entries ----

    pub fn entries(&self, journal: &str) -> Result<Vec<Entry>> {
        let mut st = self.conn.prepare(&format!("SELECT {ECOLS} FROM journal_entries WHERE journal_id = ?1 ORDER BY position, created_at"))?;
        let v = st.query_map([journal], Self::row_entry)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn all_entries(&self) -> Result<Vec<Entry>> {
        let mut st = self.conn.prepare(&format!("SELECT {ECOLS} FROM journal_entries ORDER BY created_at"))?;
        let v = st.query_map([], Self::row_entry)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn entry(&self, id: &str) -> Result<Option<Entry>> {
        Ok(self.conn.query_row(&format!("SELECT {ECOLS} FROM journal_entries WHERE id = ?1"), [id], Self::row_entry).optional()?)
    }

    /// By id or unique id prefix.
    pub fn find_entry(&self, key: &str) -> Result<Entry> {
        let k = key.trim();
        let ids: Vec<String> = {
            let mut st = self.conn.prepare("SELECT id FROM journal_entries WHERE id LIKE ?1 || '%'")?;
            st.query_map([k], |r| r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?
        };
        match ids.as_slice() {
            [one] if !k.is_empty() => Ok(self.entry(one)?.expect("listed")),
            [] | [_] => Err(Error::invalid("entry", format!("`{k}` not found (see `mnnn journal show JOURNAL`)"))),
            _ => Err(Error::invalid("entry", format!("`{k}` matches more than one entry; use more of the id"))),
        }
    }

    /// Write an entry (format: the journal's when None).
    pub fn add_entry(&self, journal: &str, title: &str, body: &str, format: Option<EntryFormat>) -> Result<Entry> {
        let mut j = self.journal(journal)?.ok_or_else(|| Error::invalid("journal", format!("`{journal}` not found")))?;
        let e = self.insert_entry(&j, title, body, format.unwrap_or(j.format), None)?;
        self.touch_journal(&mut j)?;
        Ok(e)
    }

    fn insert_entry(&self, j: &Journal, title: &str, body: &str, format: EntryFormat, forked_from: Option<&str>) -> Result<Entry> {
        let pos: i64 =
            self.conn.query_row("SELECT COALESCE(MAX(position), 0) + 1 FROM journal_entries WHERE journal_id = ?1", [&j.id], |r| r.get(0))?;
        let now = now_ms();
        let e = Entry {
            id: super::new_id(),
            journal_id: j.id.clone(),
            position: pos,
            title: title.trim().to_string(),
            body: body.to_string(),
            format,
            forked_from: forked_from.map(str::to_string),
            author_id: self.current_author_id()?,
            created_at: now,
            updated_at: now,
        };
        if e.title.is_empty() && e.body.trim().is_empty() {
            return Err(Error::invalid("entry", "needs a title or some text"));
        }
        self.conn.execute(
            &format!("INSERT INTO journal_entries({ECOLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"),
            params![e.id, e.journal_id, e.position, e.title, e.body, e.format.as_str(), e.forked_from, e.author_id, e.created_at, e.updated_at],
        )?;
        Ok(e)
    }

    /// Change an entry. Notes and highlights on its words follow them (or detach).
    pub fn update_entry(&self, id: &str, title: Option<&str>, body: Option<&str>, format: Option<EntryFormat>) -> Result<Entry> {
        let mut e = self.entry(id)?.ok_or_else(|| Error::invalid("entry", format!("`{id}` not found")))?;
        if let Some(t) = title {
            e.title = t.trim().to_string();
        }
        if let Some(b) = body {
            e.body = b.to_string();
        }
        if let Some(f) = format {
            e.format = f;
        }
        if e.title.is_empty() && e.body.trim().is_empty() {
            return Err(Error::invalid("entry", "needs a title or some text"));
        }
        e.updated_at = now_ms();
        self.conn.execute(
            "UPDATE journal_entries SET title = ?2, body = ?3, format = ?4, updated_at = ?5 WHERE id = ?1",
            params![e.id, e.title, e.body, e.format.as_str(), e.updated_at],
        )?;
        if body.is_some() {
            for a in self.entry_annotations(&e.id)?.into_iter().filter(|a| a.quote.is_some()) {
                let sp = reanchor(&e.body, &a);
                self.conn.execute(
                    "UPDATE journal_annotations SET start = ?2, end = ?3 WHERE id = ?1",
                    params![a.id, sp.map(|s| s.start as i64), sp.map(|s| s.end as i64)],
                )?;
            }
        }
        Ok(e)
    }

    pub fn remove_entry(&self, id: &str) -> Result<Entry> {
        let e = self.entry(id)?.ok_or_else(|| Error::invalid("entry", format!("`{id}` not found")))?;
        self.conn.execute("DELETE FROM journal_entries WHERE id = ?1", [id])?;
        Ok(e)
    }

    /// Copy entries (with their notes, highlights and images) to the end of another journal.
    pub fn copy_entries(&self, ids: &[String], target: &str) -> Result<Vec<Entry>> {
        let mut j = self.journal(target)?.ok_or_else(|| Error::invalid("journal", format!("`{target}` not found")))?;
        let mut out = vec![];
        for id in ids {
            let o = self.entry(id)?.ok_or_else(|| Error::invalid("entry", format!("`{id}` not found")))?;
            let mut body = o.body.clone();
            let atts = self.attachments(&o.id)?;
            let mut new_atts = vec![];
            for a in &atts {
                let nid = super::new_id();
                body = body.replace(&format!("attachment:{}", a.id), &format!("attachment:{nid}"));
                new_atts.push((a, nid));
            }
            let e = self.insert_entry(&j, &o.title, &body, o.format, Some(&o.id))?;
            for (a, nid) in new_atts {
                self.conn.execute(
                    "INSERT INTO journal_attachments(id, entry_id, name, mime, data, author_id, created_at)
                     SELECT ?1, ?2, name, mime, data, author_id, created_at FROM journal_attachments WHERE id = ?3",
                    params![nid, e.id, a.id],
                )?;
            }
            for a in self.entry_annotations(&o.id)? {
                let mut n = Annotation { id: super::new_id(), journal_id: j.id.clone(), entry_id: Some(e.id.clone()), ..a };
                if let Some(q) = n.quote.clone()
                    && body != o.body
                {
                    let sp = reanchor(&body, &Annotation { quote: Some(q), ..n.clone() });
                    (n.start, n.end) = (sp.map(|s| s.start), sp.map(|s| s.end));
                }
                self.insert_annotation(&n)?;
            }
            out.push(e);
        }
        self.touch_journal(&mut j)?;
        Ok(out)
    }

    // ---- notes and highlights ----

    pub fn annotations(&self, journal: &str) -> Result<Vec<Annotation>> {
        let mut st = self.conn.prepare(&format!("SELECT {ACOLS} FROM journal_annotations WHERE journal_id = ?1 ORDER BY created_at, id"))?;
        let v = st.query_map([journal], Self::row_annotation)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn entry_annotations(&self, entry: &str) -> Result<Vec<Annotation>> {
        let mut st = self.conn.prepare(&format!("SELECT {ACOLS} FROM journal_annotations WHERE entry_id = ?1 ORDER BY start, created_at"))?;
        let v = st.query_map([entry], Self::row_annotation)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn annotation(&self, id: &str) -> Result<Option<Annotation>> {
        Ok(self.conn.query_row(&format!("SELECT {ACOLS} FROM journal_annotations WHERE id = ?1"), [id], Self::row_annotation).optional()?)
    }

    pub fn find_annotation(&self, key: &str) -> Result<Annotation> {
        let k = key.trim();
        let ids: Vec<String> = {
            let mut st = self.conn.prepare("SELECT id FROM journal_annotations WHERE id LIKE ?1 || '%'")?;
            st.query_map([k], |r| r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?
        };
        match ids.as_slice() {
            [one] if !k.is_empty() => Ok(self.annotation(one)?.expect("listed")),
            [] | [_] => Err(Error::invalid("note or highlight", format!("`{k}` not found"))),
            _ => Err(Error::invalid("note or highlight", format!("`{k}` matches more than one; use more of the id"))),
        }
    }

    fn insert_annotation(&self, a: &Annotation) -> Result<()> {
        let i = |v: Option<usize>| v.map(|x| x as i64);
        self.conn.execute(
            &format!("INSERT INTO journal_annotations({ACOLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)"),
            params![
                a.id,
                a.journal_id,
                a.entry_id,
                a.kind.as_str(),
                i(a.start),
                i(a.end),
                a.quote,
                a.body,
                a.color,
                a.author_id,
                a.created_at,
                a.updated_at
            ],
        )?;
        Ok(())
    }

    fn new_annotation(
        &self,
        kind: AnnotationKind,
        journal: &str,
        entry: Option<&Entry>,
        span: Option<Span>,
        body: &str,
        color: Option<&str>,
    ) -> Result<Annotation> {
        let span = match (entry, span) {
            (Some(e), Some(sp)) => Some(check_span(&e.body, sp)?),
            (None, Some(_)) => return Err(Error::invalid("selection", "words can only be selected in an entry")),
            _ => None,
        };
        let now = now_ms();
        let a = Annotation {
            id: super::new_id(),
            journal_id: journal.to_string(),
            entry_id: entry.map(|e| e.id.clone()),
            kind,
            start: span.map(|s| s.start),
            end: span.map(|s| s.end),
            quote: match (entry, span) {
                (Some(e), Some(s)) => Some(chars_slice(&e.body, s.start, s.end)),
                _ => None,
            },
            body: body.trim().to_string(),
            color: color.map(str::to_string),
            author_id: self.current_author_id()?,
            created_at: now,
            updated_at: now,
        };
        self.insert_annotation(&a)?;
        Ok(a)
    }

    /// A note on the journal (entry None), on an entry, or on selected words of an entry.
    pub fn add_note(&self, journal: &str, entry: Option<&str>, span: Option<Span>, body: &str) -> Result<Annotation> {
        if body.trim().is_empty() {
            return Err(Error::invalid("note", "must not be empty"));
        }
        let j = self.journal(journal)?.ok_or_else(|| Error::invalid("journal", format!("`{journal}` not found")))?;
        let e = match entry {
            Some(id) => {
                let e = self.entry(id)?.ok_or_else(|| Error::invalid("entry", format!("`{id}` not found")))?;
                if e.journal_id != j.id {
                    return Err(Error::invalid("entry", format!("`{id}` is not in journal `{}`", j.name)));
                }
                Some(e)
            }
            None => None,
        };
        self.new_annotation(AnnotationKind::Note, &j.id, e.as_ref(), span, body, None)
    }

    /// Highlight selected words of an entry (an optional comment goes in the body).
    pub fn add_highlight(&self, entry: &str, span: Span, color: Option<&str>, comment: &str) -> Result<Annotation> {
        let e = self.entry(entry)?.ok_or_else(|| Error::invalid("entry", format!("`{entry}` not found")))?;
        self.new_annotation(AnnotationKind::Highlight, &e.journal_id.clone(), Some(&e), Some(span), comment, color)
    }

    pub fn update_annotation(&self, id: &str, body: &str) -> Result<Annotation> {
        let mut a = self.annotation(id)?.ok_or_else(|| Error::invalid("note or highlight", format!("`{id}` not found")))?;
        if a.kind == AnnotationKind::Note && body.trim().is_empty() {
            return Err(Error::invalid("note", "must not be empty"));
        }
        a.body = body.trim().to_string();
        a.updated_at = now_ms();
        self.conn.execute("UPDATE journal_annotations SET body = ?2, updated_at = ?3 WHERE id = ?1", params![a.id, a.body, a.updated_at])?;
        Ok(a)
    }

    pub fn remove_annotation(&self, id: &str) -> Result<Annotation> {
        let a = self.annotation(id)?.ok_or_else(|| Error::invalid("note or highlight", format!("`{id}` not found")))?;
        self.conn.execute("DELETE FROM journal_annotations WHERE id = ?1", [id])?;
        Ok(a)
    }

    // ---- images ----

    pub fn attachments(&self, entry: &str) -> Result<Vec<Attachment>> {
        let mut st = self.conn.prepare(&format!("SELECT {FCOLS} FROM journal_attachments WHERE entry_id = ?1 ORDER BY created_at, id"))?;
        let v = st.query_map([entry], Self::row_attachment)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn attachment_data(&self, id: &str) -> Result<Option<(Attachment, Vec<u8>)>> {
        let a = self.conn.query_row(&format!("SELECT {FCOLS} FROM journal_attachments WHERE id = ?1"), [id], Self::row_attachment).optional()?;
        let Some(a) = a else { return Ok(None) };
        let data: Vec<u8> = self.conn.query_row("SELECT data FROM journal_attachments WHERE id = ?1", [id], |r| r.get(0))?;
        Ok(Some((a, data)))
    }

    /// Store an image for an entry and append its reference to the entry text.
    pub fn attach_image(&self, entry: &str, name: &str, data: &[u8]) -> Result<Attachment> {
        let e = self.entry(entry)?.ok_or_else(|| Error::invalid("entry", format!("`{entry}` not found")))?;
        let mime = image_mime(name)?;
        if data.is_empty() || data.len() > MAX_ATTACHMENT {
            return Err(Error::invalid("image", format!("`{name}` is {} bytes (1 byte to 20 MiB)", data.len())));
        }
        let a = Attachment {
            id: super::new_id(),
            entry_id: e.id.clone(),
            name: name.to_string(),
            mime: mime.to_string(),
            size: data.len() as i64,
            author_id: self.current_author_id()?,
            created_at: now_ms(),
        };
        self.conn.execute(
            "INSERT INTO journal_attachments(id, entry_id, name, mime, data, author_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![a.id, a.entry_id, a.name, a.mime, data, a.author_id, a.created_at],
        )?;
        let sep = if e.body.is_empty() || e.body.ends_with('\n') { "" } else { "\n" };
        let body = format!("{}{sep}{}\n", e.body, a.reference(e.format));
        self.update_entry(&e.id, None, Some(&body), None)?;
        Ok(a)
    }

    // ---- whole documents ----

    pub fn journal_doc(&self, id: &str) -> Result<JournalDoc> {
        let j = self.journal(id)?.ok_or_else(|| Error::invalid("journal", format!("`{id}` not found")))?;
        let mut names: HashMap<String, Option<String>> = HashMap::new();
        let mut who = |id: &Option<String>| -> Result<Option<String>> {
            let Some(i) = id else { return Ok(None) };
            if !names.contains_key(i) {
                names.insert(i.clone(), Some(self.author(i)?.map(|a| a.name).unwrap_or_else(|| "(removed author)".into())));
            }
            Ok(names[i].clone())
        };
        let anns = self.annotations(&j.id)?;
        let mut notes = vec![];
        for a in anns.iter().filter(|a| a.entry_id.is_none()) {
            notes.push(AnnotationDoc { author: who(&a.author_id)?, annotation: a.clone() });
        }
        let mut entries = vec![];
        for e in self.entries(&j.id)? {
            let mut annotations = vec![];
            for a in anns.iter().filter(|a| a.entry_id.as_deref() == Some(e.id.as_str())) {
                annotations.push(AnnotationDoc { author: who(&a.author_id)?, annotation: a.clone() });
            }
            entries.push(EntryDoc { author: who(&e.author_id)?, attachments: self.attachments(&e.id)?, annotations, entry: e });
        }
        let project = match &j.project_id {
            Some(p) => self.project(p)?.map(|p| p.name),
            None => None,
        };
        Ok(JournalDoc { author: who(&j.author_id)?, project, notes, entries, journal: j })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Store, Journal) {
        let s = Store::open_in_memory().unwrap();
        s.add_author("Ada", None, &[]).unwrap();
        let j = s.create_journal("Trading log", None, EntryFormat::Markdown).unwrap();
        (s, j)
    }

    #[test]
    fn entries_notes_and_highlights() {
        let (s, j) = setup();
        let e = s.add_entry(&j.id, "Monday", "BTC broke out. Volume was thin, volume matters.", None).unwrap();
        assert_eq!(e.format, EntryFormat::Markdown);
        assert!(s.add_entry(&j.id, "", "  ", None).is_err());
        let sp = select(&e.body, "volume", 1).unwrap();
        assert_eq!(chars_slice(&e.body, sp.start, sp.end), "volume");
        assert!(select(&e.body, "volume", 2).is_err());
        let h = s.add_highlight(&e.id, select(&e.body, "broke out", 1).unwrap(), Some("yellow"), "").unwrap();
        assert_eq!(h.quote.as_deref(), Some("broke out"));
        let n1 = s.add_note(&j.id, None, None, "Week one").unwrap();
        let n2 = s.add_note(&j.id, Some(&e.id), None, "Check the 1h chart").unwrap();
        let n3 = s.add_note(&j.id, Some(&e.id), Some(Span { start: 0, end: 3 }), "the ticker").unwrap();
        assert!(s.add_note(&j.id, None, Some(Span { start: 0, end: 3 }), "x").is_err());
        assert!(s.add_note(&j.id, Some(&e.id), Some(Span { start: 0, end: 999 }), "x").is_err());
        assert!(s.add_note(&j.id, None, None, " ").is_err());
        assert_eq!(s.annotations(&j.id).unwrap().len(), 4);
        // editing moves spans with their words, or detaches them
        let e2 = s.update_entry(&e.id, None, Some("Note: BTC broke out today."), None).unwrap();
        let h2 = s.annotation(&h.id).unwrap().unwrap();
        assert_eq!(chars_slice(&e2.body, h2.start.unwrap(), h2.end.unwrap()), "broke out");
        assert_eq!(s.annotation(&n3.id).unwrap().unwrap().span(), Some((6, 9)));
        s.update_entry(&e.id, None, Some("nothing left"), None).unwrap();
        assert!(s.annotation(&h.id).unwrap().unwrap().detached());
        assert_eq!(s.update_annotation(&n2.id, "Check 4h").unwrap().body, "Check 4h");
        s.remove_annotation(&n1.id).unwrap();
        assert_eq!(s.find_annotation(&n2.id[..6]).unwrap().id, n2.id);
        let doc = s.journal_doc(&j.id).unwrap();
        assert_eq!(doc.entries[0].annotations.len(), 3);
        assert_eq!(doc.entries[0].author.as_deref(), Some("Ada"));
        assert!(doc.notes.is_empty());
    }

    #[test]
    fn images_fork_and_copy() {
        let (s, j) = setup();
        let e = s.add_entry(&j.id, "Chart", "Look:", None).unwrap();
        let a = s.attach_image(&e.id, "btc.png", b"\x89PNG fake").unwrap();
        assert!(s.attach_image(&e.id, "notes.txt", b"x").is_err());
        let e = s.entry(&e.id).unwrap().unwrap();
        assert_eq!(e.body, format!("Look:\n![btc.png](attachment:{})\n", a.id));
        assert_eq!(s.attachment_data(&a.id).unwrap().unwrap().1, b"\x89PNG fake");
        s.add_highlight(&e.id, Span { start: 0, end: 4 }, None, "").unwrap();
        s.add_note(&j.id, None, None, "global").unwrap();
        let p = s.create_project("P", "").unwrap();
        let f = s.fork_journal(&j.id, Some(&p.id), None).unwrap();
        assert_eq!(
            (f.name.as_str(), f.project_id.as_deref(), f.forked_from.as_deref()),
            ("Trading log (fork)", Some(p.id.as_str()), Some(j.id.as_str()))
        );
        let fe = &s.entries(&f.id).unwrap()[0];
        assert_eq!(fe.forked_from.as_deref(), Some(e.id.as_str()));
        let fa = &s.attachments(&fe.id).unwrap()[0];
        assert_ne!(fa.id, a.id);
        assert!(fe.body.contains(&format!("attachment:{}", fa.id)), "the copy points at its own image");
        assert_eq!(s.annotations(&f.id).unwrap().len(), 2);
        assert_eq!(s.project_journals(&p.id).unwrap().len(), 1);
        // select entries and add them to a new journal
        let e2 = s.add_entry(&j.id, "", "second", Some(EntryFormat::Latex)).unwrap();
        let other = s.create_journal("Best of", None, EntryFormat::Latex).unwrap();
        let copied = s.copy_entries(&[e2.id.clone(), e.id.clone()], &other.id).unwrap();
        assert_eq!(copied.iter().map(|e| e.heading()).collect::<Vec<_>>(), ["second", "Chart"]);
        assert_eq!(copied[0].format, EntryFormat::Latex);
        assert_eq!(s.entries(&j.id).unwrap().len(), 2, "the originals stay");
        // removing the project keeps its journal as a standalone one
        s.remove_project(&p.id).unwrap();
        assert_eq!(s.journal(&f.id).unwrap().unwrap().project_id, None);
        s.remove_journal(&j.id).unwrap();
        assert!(s.entry(&e.id).unwrap().is_none());
        assert!(s.attachment_data(&a.id).unwrap().is_none(), "images go with their entry");
        assert!(s.find_entry(&e2.id).is_err());
    }
}
