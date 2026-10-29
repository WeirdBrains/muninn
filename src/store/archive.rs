//! Packs: a whole project (or one journal) as plain data that can be written to a file
//! and restored into any store. Bytes of attachments and bars travel separately (files in
//! an archive, or inline in JSON); the pack says where to find them.
//!
//! Restoring always creates new things with new ids (so a project can be imported next to
//! the original), keeps authorship and times, and rewrites every reference inside the pack
//! (attachments in entry text, project items, report sections) to the new ids.
use super::journals::{Annotation, Attachment, Entry, Journal};
use super::projects::{ItemKind, Project, ProjectItem, SeriesRef};
use super::reports::{Report, Section, SectionKind};
use super::searches::{LiveSearch, SearchTarget};
use super::{Author, Store, now_ms};
use crate::error::{Error, Result};
use crate::model::Bar;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

pub const PROJECT_FORMAT: &str = "muninn-project";
pub const JOURNAL_FORMAT: &str = "muninn-journal";
pub const PACK_VERSION: u32 = 1;

/// An attachment and where its bytes are.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentPack {
    #[serde(flatten)]
    pub meta: Attachment,
    /// Path inside an archive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// The bytes, base64 (JSON packs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalPack {
    pub journal: Journal,
    pub entries: Vec<Entry>,
    /// Notes and highlights (on the journal and on entries).
    pub annotations: Vec<Annotation>,
    pub attachments: Vec<AttachmentPack>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportPack {
    pub report: Report,
    pub sections: Vec<Section>,
}

/// A bar as `[timestamp, open, high, low, close, volume, flags]`.
pub type BarRow = (i64, f64, f64, f64, f64, f64, i64);

/// One series (or a range of it).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPack {
    pub series: SeriesRef,
    pub bars: usize,
    /// CSV inside an archive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// `[timestamp, open, high, low, close, volume, flags]` rows (JSON packs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<Vec<BarRow>>,
}

/// A project with everything in it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectPack {
    pub format: String,
    pub version: u32,
    /// Muninn version that wrote it.
    pub muninn: String,
    pub exported_at: i64,
    pub project: Project,
    pub authors: Vec<Author>,
    pub items: Vec<ProjectItem>,
    pub live_searches: Vec<LiveSearch>,
    pub journals: Vec<JournalPack>,
    pub reports: Vec<ReportPack>,
    #[serde(default)]
    pub data: Vec<DataPack>,
}

/// One journal on its own (`mnnn journal export --format json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalFile {
    pub format: String,
    pub version: u32,
    pub muninn: String,
    pub exported_at: i64,
    pub authors: Vec<Author>,
    #[serde(flatten)]
    pub pack: JournalPack,
}

/// What an import created.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Restored {
    pub project: Option<Project>,
    pub journals: Vec<Journal>,
    pub entries: usize,
    pub attachments: usize,
    pub live_searches: usize,
    pub reports: usize,
    pub bars: u64,
    pub authors_added: usize,
}

/// Bytes for a pack's attachments and data.
pub trait Blobs {
    fn attachment(&self, a: &AttachmentPack) -> Result<Vec<u8>>;
    fn bars(&self, d: &DataPack) -> Result<Vec<Bar>>;
}

/// Blobs inline in a JSON pack.
pub struct Inline;

impl Blobs for Inline {
    fn attachment(&self, a: &AttachmentPack) -> Result<Vec<u8>> {
        use base64::Engine;
        let s = a.data.as_deref().ok_or_else(|| Error::invalid("pack", format!("attachment {} has no data", a.meta.name)))?;
        base64::engine::general_purpose::STANDARD.decode(s).map_err(|e| Error::invalid("pack", format!("attachment {}: {e}", a.meta.name)))
    }
    fn bars(&self, d: &DataPack) -> Result<Vec<Bar>> {
        let rows = d.rows.as_ref().ok_or_else(|| Error::invalid("pack", format!("data {} has no rows", d.series.label())))?;
        Ok(rows.iter().map(|&(ts, open, high, low, close, volume, flags)| Bar { ts, open, high, low, close, volume, flags }).collect())
    }
}

/// CSV of bars as written in archives (exact: Rust prints the shortest form that reads
/// back to the same number).
pub fn bars_csv(bars: &[Bar]) -> String {
    let mut s = String::from("timestamp,open,high,low,close,volume,flags\n");
    for b in bars {
        s.push_str(&format!("{},{},{},{},{},{},{}\n", b.ts, b.open, b.high, b.low, b.close, b.volume, b.flags));
    }
    s
}

pub fn parse_bars_csv(s: &str) -> Result<Vec<Bar>> {
    let bad = |n: usize| Error::invalid("pack data", format!("line {n} is not a bar"));
    let mut out = vec![];
    for (n, line) in s.lines().enumerate().skip(1) {
        if line.trim().is_empty() {
            continue;
        }
        let v: Vec<&str> = line.split(',').collect();
        if v.len() != 7 {
            return Err(bad(n + 1));
        }
        let f = |i: usize| v[i].parse::<f64>().map_err(|_| bad(n + 1));
        out.push(Bar {
            ts: v[0].parse().map_err(|_| bad(n + 1))?,
            open: f(1)?,
            high: f(2)?,
            low: f(3)?,
            close: f(4)?,
            volume: f(5)?,
            flags: v[6].parse().map_err(|_| bad(n + 1))?,
        });
    }
    Ok(out)
}

/// Series a project uses: its results, what its live searches match now, and records
/// sections of its reports. Ranges of one series are joined into one covering range.
fn data_refs(store: &Store, pack: &ProjectPack) -> Result<Vec<SeriesRef>> {
    let mut by: BTreeMap<String, SeriesRef> = BTreeMap::new();
    let mut add = |r: SeriesRef| {
        let key = format!("{}|{}|{}", r.source, r.symbol.display(), r.duration);
        match by.get_mut(&key) {
            None => {
                by.insert(key, r);
            }
            Some(have) => {
                have.from = match (have.from, r.from) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    _ => None,
                };
                have.to = match (have.to, r.to) {
                    (Some(a), Some(b)) => Some(a.max(b)),
                    _ => None,
                };
            }
        }
    };
    for it in &pack.items {
        if it.kind == ItemKind::Result
            && let Some(r) = it.series()
        {
            add(r);
        }
    }
    for s in &pack.live_searches {
        if s.spec.target == SearchTarget::Series {
            for m in crate::live::series_matches(store, &s.spec)? {
                add(SeriesRef { source: m.source, symbol: m.symbol, duration: m.duration, from: s.spec.filters.from, to: s.spec.filters.to });
            }
        }
    }
    for r in &pack.reports {
        for s in &r.sections {
            if s.kind == SectionKind::Records
                && let Some(sr) = &s.params.series
            {
                add(sr.clone());
            }
        }
    }
    Ok(by.into_values().collect())
}

impl Store {
    /// A journal with its entries, notes, highlights and attachments (bytes not included).
    pub fn journal_pack(&self, id: &str) -> Result<JournalPack> {
        let journal = self.journal(id)?.ok_or_else(|| Error::invalid("journal", format!("`{id}` not found")))?;
        let entries = self.entries(&journal.id)?;
        let annotations = self.annotations(&journal.id)?;
        let mut attachments = vec![];
        for e in &entries {
            for a in self.attachments(&e.id)? {
                attachments.push(AttachmentPack { meta: a, file: None, data: None });
            }
        }
        Ok(JournalPack { journal, entries, annotations, attachments })
    }

    /// Authors named anywhere in the given ids.
    fn authors_of<'a>(&self, ids: impl Iterator<Item = &'a Option<String>>) -> Result<Vec<Author>> {
        let mut seen = BTreeMap::new();
        for id in ids.flatten() {
            if !seen.contains_key(id)
                && let Some(a) = self.author(id)?
            {
                seen.insert(id.clone(), a);
            }
        }
        Ok(seen.into_values().collect())
    }

    /// A journal as a file of its own (bytes not included).
    pub fn journal_file(&self, id: &str) -> Result<JournalFile> {
        let pack = self.journal_pack(id)?;
        let authors = self.authors_of(journal_author_ids(&pack).into_iter().collect::<Vec<_>>().iter())?;
        Ok(JournalFile {
            format: JOURNAL_FORMAT.into(),
            version: PACK_VERSION,
            muninn: env!("CARGO_PKG_VERSION").into(),
            exported_at: now_ms(),
            authors,
            pack,
        })
    }

    /// A project with its items, the journals, reports and live searches they point at
    /// (plus journals and live searches its reports use), and (unless `with_data` is off)
    /// the series it uses. Bytes not included: see [`Blobs`].
    pub fn project_pack(&self, id: &str, with_data: bool) -> Result<ProjectPack> {
        let project = self.project(id)?.ok_or_else(|| Error::invalid("project", format!("`{id}` not found")))?;
        let items = self.project_items(&project.id)?;
        let mut journal_ids: Vec<String> = vec![];
        let mut search_ids: Vec<String> = vec![];
        let mut reports = vec![];
        for it in &items {
            match it.kind {
                ItemKind::Journal => journal_ids.push(it.ref_id.clone()),
                ItemKind::LiveSearch => search_ids.push(it.ref_id.clone()),
                ItemKind::Report => {
                    if let Some(r) = self.report(&it.ref_id)? {
                        let sections = self.sections(&r.id)?;
                        for s in &sections {
                            if let Some(j) = &s.params.journal {
                                journal_ids.push(j.clone());
                            }
                            if let Some(q) = &s.params.search {
                                search_ids.push(q.clone());
                            }
                        }
                        reports.push(ReportPack { report: r, sections });
                    }
                }
                _ => {}
            }
        }
        // journals that name this project but are not items (should not happen; be safe)
        for j in self.journals()? {
            if j.project_id.as_deref() == Some(project.id.as_str()) {
                journal_ids.push(j.id);
            }
        }
        let mut journals = vec![];
        let mut seen = std::collections::BTreeSet::new();
        for j in journal_ids {
            if seen.insert(j.clone()) && self.journal(&j)?.is_some() {
                journals.push(self.journal_pack(&j)?);
            }
        }
        let mut live_searches = vec![];
        for s in search_ids {
            if seen.insert(s.clone())
                && let Some(l) = self.live_search(&s)?
            {
                live_searches.push(l);
            }
        }
        let mut ids: Vec<Option<String>> = vec![project.author_id.clone()];
        ids.extend(items.iter().map(|i| i.author_id.clone()));
        ids.extend(live_searches.iter().map(|l| l.author_id.clone()));
        for j in &journals {
            ids.extend(journal_author_ids(j));
        }
        for r in &reports {
            ids.push(r.report.author_id.clone());
            ids.extend(r.sections.iter().map(|s| s.author_id.clone()));
        }
        let authors = self.authors_of(ids.iter())?;
        let mut pack = ProjectPack {
            format: PROJECT_FORMAT.into(),
            version: PACK_VERSION,
            muninn: env!("CARGO_PKG_VERSION").into(),
            exported_at: now_ms(),
            project,
            authors,
            items,
            live_searches,
            journals,
            reports,
            data: vec![],
        };
        if with_data {
            for r in data_refs(self, &pack)? {
                let n = self.count_filtered(&r.filter())? as usize;
                if n > 0 {
                    pack.data.push(DataPack { series: r, bars: n, file: None, rows: None });
                }
            }
        }
        Ok(pack)
    }

    /// Bars of a data pack entry, from this store.
    pub fn pack_bars(&self, d: &DataPack) -> Result<Vec<Bar>> {
        Ok(self.query_bars(&d.series.filter())?.into_iter().map(|b| b.bar).collect())
    }

    /// Bring authors in: the same id is the same person (kept as is), else added with its id.
    fn restore_authors(&self, authors: &[Author], out: &mut Restored) -> Result<()> {
        for a in authors {
            if self.author(&a.id)?.is_none() {
                self.conn.execute(
                    "INSERT INTO authors(id, name, email, details, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![a.id, a.name, a.email, serde_json::to_string(&a.details)?, a.created_at, a.updated_at],
                )?;
                out.authors_added += 1;
            }
        }
        Ok(())
    }

    /// Insert a journal pack with new ids; returns the new journal and old→new ids.
    fn restore_journal(
        &self,
        j: &JournalPack,
        project: Option<&str>,
        name: Option<&str>,
        blobs: &dyn Blobs,
        ids: &mut HashMap<String, String>,
        out: &mut Restored,
    ) -> Result<Journal> {
        let mut nj = j.journal.clone();
        nj.id = super::new_id();
        nj.project_id = project.map(str::to_string);
        if let Some(n) = name {
            nj.name = n.trim().to_string();
        }
        self.conn.execute(
            "INSERT INTO journals(id, name, project_id, format, forked_from, author_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![nj.id, nj.name, nj.project_id, nj.format.as_str(), nj.forked_from, nj.author_id, nj.created_at, nj.updated_at],
        )?;
        ids.insert(j.journal.id.clone(), nj.id.clone());
        for e in &j.entries {
            ids.insert(e.id.clone(), super::new_id());
        }
        for a in &j.attachments {
            ids.insert(a.meta.id.clone(), super::new_id());
        }
        for e in &j.entries {
            let mut body = e.body.clone();
            for a in j.attachments.iter().filter(|a| a.meta.entry_id == e.id) {
                body = body.replace(&format!("attachment:{}", a.meta.id), &format!("attachment:{}", ids[&a.meta.id]));
            }
            self.conn.execute(
                "INSERT INTO journal_entries(id, journal_id, position, title, body, format, forked_from, author_id, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![ids[&e.id], nj.id, e.position, e.title, body, e.format.as_str(), e.forked_from, e.author_id, e.created_at, e.updated_at],
            )?;
            out.entries += 1;
        }
        for a in &j.attachments {
            let Some(entry) = ids.get(&a.meta.entry_id).cloned() else { continue };
            let data = blobs.attachment(a)?;
            self.conn.execute(
                "INSERT INTO journal_attachments(id, entry_id, name, mime, data, author_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![ids[&a.meta.id], entry, a.meta.name, a.meta.mime, data, a.meta.author_id, a.meta.created_at],
            )?;
            out.attachments += 1;
        }
        for a in &j.annotations {
            let entry = match &a.entry_id {
                Some(e) => match ids.get(e) {
                    Some(n) => Some(n.clone()),
                    None => continue,
                },
                None => None,
            };
            self.conn.execute(
                "INSERT INTO journal_annotations(id, journal_id, entry_id, kind, start, end, quote, body, color, author_id, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    super::new_id(),
                    nj.id,
                    entry,
                    a.kind.as_str(),
                    a.start.map(|v| v as i64),
                    a.end.map(|v| v as i64),
                    a.quote,
                    a.body,
                    a.color,
                    a.author_id,
                    a.created_at,
                    a.updated_at
                ],
            )?;
        }
        out.journals.push(nj.clone());
        Ok(nj)
    }

    /// Restore a journal file as a new journal (optionally into a project, optionally renamed).
    pub fn import_journal(&self, f: &JournalFile, project: Option<&str>, name: Option<&str>, blobs: &dyn Blobs) -> Result<Restored> {
        check_format(&f.format, f.version, JOURNAL_FORMAT)?;
        if let Some(p) = project
            && self.project(p)?.is_none()
        {
            return Err(Error::invalid("project", format!("`{p}` not found")));
        }
        // Like projects: a journal of the same name gets "(imported)" unless a name is given.
        let clash = match name {
            None if self.journals()?.iter().any(|j| j.name == f.pack.journal.name) => Some(format!("{} (imported)", f.pack.journal.name)),
            _ => None,
        };
        let name = clash.as_deref().or(name);
        self.begin_atomic()?;
        let mut out = Restored::default();
        let r = (|| -> Result<()> {
            self.restore_authors(&f.authors, &mut out)?;
            let j = self.restore_journal(&f.pack, project, name, blobs, &mut HashMap::new(), &mut out)?;
            if let Some(p) = project {
                self.add_project_item(p, ItemKind::Journal, &j.id, serde_json::json!({}))?;
            }
            Ok(())
        })();
        match r {
            Ok(()) => {
                self.commit_atomic()?;
                Ok(out)
            }
            Err(e) => {
                self.rollback_atomic()?;
                Err(e)
            }
        }
    }

    /// Restore a project pack as a new project. A project of the same name gets
    /// "(imported)" appended unless `name` is given.
    pub fn import_project(&mut self, p: &ProjectPack, name: Option<&str>, blobs: &dyn Blobs) -> Result<Restored> {
        check_format(&p.format, p.version, PROJECT_FORMAT)?;
        self.begin_atomic()?;
        let r = self.import_project_inner(p, name, blobs);
        match r {
            Ok(out) => {
                self.commit_atomic()?;
                Ok(out)
            }
            Err(e) => {
                self.rollback_atomic()?;
                Err(e)
            }
        }
    }

    fn import_project_inner(&mut self, p: &ProjectPack, name: Option<&str>, blobs: &dyn Blobs) -> Result<Restored> {
        let mut out = Restored::default();
        self.restore_authors(&p.authors, &mut out)?;
        let mut np = p.project.clone();
        np.id = super::new_id();
        np.name = match name {
            Some(n) if !n.trim().is_empty() => n.trim().to_string(),
            _ if self.projects()?.iter().any(|x| x.name == np.name) => format!("{} (imported)", np.name),
            _ => np.name.clone(),
        };
        self.conn.execute(
            "INSERT INTO projects(id, name, description, author_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![np.id, np.name, np.description, np.author_id, np.created_at, np.updated_at],
        )?;
        let mut ids: HashMap<String, String> = HashMap::new();
        ids.insert(p.project.id.clone(), np.id.clone());
        for s in &p.live_searches {
            let nid = super::new_id();
            self.conn.execute(
                "INSERT INTO live_searches(id, name, spec, forked_from, author_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![nid, s.name, serde_json::to_string(&s.spec)?, s.forked_from, s.author_id, s.created_at, s.updated_at],
            )?;
            ids.insert(s.id.clone(), nid);
            out.live_searches += 1;
        }
        for j in &p.journals {
            let in_project = j.journal.project_id.as_deref() == Some(p.project.id.as_str());
            let clash = self.journals()?.iter().any(|x| x.name == j.journal.name);
            let jname = clash.then(|| format!("{} (imported)", j.journal.name));
            self.restore_journal(j, in_project.then_some(np.id.as_str()), jname.as_deref(), blobs, &mut ids, &mut out)?;
        }
        for r in &p.reports {
            let mut nr = r.report.clone();
            nr.id = super::new_id();
            if self.reports()?.iter().any(|x| x.name == nr.name) {
                nr.name = format!("{} (imported)", nr.name);
            }
            self.conn.execute(
                "INSERT INTO reports(id, name, description, author_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![nr.id, nr.name, nr.description, nr.author_id, nr.created_at, nr.updated_at],
            )?;
            ids.insert(r.report.id.clone(), nr.id.clone());
            for s in &r.sections {
                let mut ns = s.clone();
                ns.id = super::new_id();
                ns.report_id = nr.id.clone();
                let map = |v: &Option<String>| v.as_ref().map(|x| ids.get(x).cloned().unwrap_or_else(|| x.clone()));
                ns.params.journal = map(&s.params.journal);
                ns.params.search = map(&s.params.search);
                ns.params.project = map(&s.params.project);
                ns.params.entries = s.params.entries.iter().map(|e| ids.get(e).cloned().unwrap_or_else(|| e.clone())).collect();
                self.insert_section(&ns)?;
            }
            out.reports += 1;
        }
        for it in &p.items {
            let ref_id = match it.kind {
                ItemKind::Result | ItemKind::RuleSet => it.ref_id.clone(),
                _ => match ids.get(&it.ref_id) {
                    Some(n) => n.clone(),
                    None => continue,
                },
            };
            self.conn.execute(
                "INSERT OR IGNORE INTO project_items(project_id, kind, ref_id, payload, author_id, added_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![np.id, it.kind.as_str(), ref_id, serde_json::to_string(&it.payload)?, it.author_id, it.added_at],
            )?;
        }
        for d in &p.data {
            let bars = blobs.bars(d)?;
            if bars.is_empty() {
                continue;
            }
            let src = self.get_or_create_source(&d.series.source, "import", None)?;
            let c = self.upsert_bars(&src, &d.series.symbol, d.series.duration, &bars)?;
            out.bars += c.inserted;
        }
        out.project = Some(np);
        Ok(out)
    }
}

fn journal_author_ids(j: &JournalPack) -> Vec<Option<String>> {
    let mut v = vec![j.journal.author_id.clone()];
    v.extend(j.entries.iter().map(|e| e.author_id.clone()));
    v.extend(j.annotations.iter().map(|a| a.author_id.clone()));
    v.extend(j.attachments.iter().map(|a| a.meta.author_id.clone()));
    v
}

fn check_format(format: &str, version: u32, want: &str) -> Result<()> {
    if format != want {
        return Err(Error::invalid("pack", format!("this is a `{format}` file, not `{want}`")));
    }
    if version > PACK_VERSION {
        return Err(Error::invalid("pack", format!("version {version} is newer than this Muninn reads ({PACK_VERSION}); update Muninn")));
    }
    Ok(())
}
