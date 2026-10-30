//! Packs: a whole project (or one journal) as plain data that can be written to a file
//! and restored into any store. Bytes of attachments and bars travel separately (files in
//! an archive, or inline in JSON); the pack says where to find them.
//!
//! Restoring always creates new things with new ids (so a project can be imported next to
//! the original), keeps authorship and times, and rewrites every reference inside the pack
//! (attachments in entry text, project items, report sections) to the new ids.
use super::journals::{Annotation, Attachment, Entry, Journal};
use super::models::{ApplyOpts, BARS, Design, FieldType, Model, ModelVersion};
use super::projects::{ItemKind, Project, ProjectItem, SeriesRef};
use super::records::Record;
use super::reports::{Report, Section, SectionKind};
use super::rules::{ItemRef, OwnerKind, Rule, Then};
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
    /// The project's rules and those living in its journals and live searches (2.3+).
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub data: Vec<DataPack>,
    /// Designed models the project uses, with their versions and records (2.4+).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<ModelPack>,
    /// Code spaces it uses (2.5+): source only, never built plugins, approvals or results.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub code: Vec<super::code::CodeSpace>,
}

/// A model in a project pack. For the built-in bars model: only the fields added to it,
/// and their values on the bars the pack carries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPack {
    pub model: Model,
    #[serde(default)]
    pub versions: Vec<ModelVersion>,
    #[serde(default)]
    pub records: Vec<Record>,
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
    /// Rules brought in (they arrive switched off).
    pub rules: usize,
    pub models: usize,
    pub records: usize,
    /// Code spaces brought in (each needs approving before it runs).
    pub code: usize,
    pub bars: u64,
    pub authors_added: usize,
}

impl Restored {
    /// "1 journal, 3 entries, …" for messages.
    pub fn counts(&self, project: bool) -> String {
        fn n(c: u64, one: &str, many: &str) -> String {
            format!("{c} {}", if c == 1 { one } else { many })
        }
        let mut v = vec![];
        if project {
            v.push(n(self.journals.len() as u64, "journal", "journals"));
        }
        v.push(n(self.entries as u64, "entry", "entries"));
        v.push(n(self.attachments as u64, "image", "images"));
        if project {
            v.push(n(self.reports as u64, "report", "reports"));
            v.push(n(self.live_searches as u64, "live search", "live searches"));
            if self.rules > 0 {
                v.push(n(self.rules as u64, "rule (off)", "rules (off)"));
            }
            if self.models > 0 {
                v.push(n(self.models as u64, "model", "models"));
                v.push(n(self.records as u64, "record", "records"));
            }
            if self.code > 0 {
                v.push(n(self.code as u64, "code space (to approve)", "code spaces (to approve)"));
            }
            v.push(n(self.bars, "new bar", "new bars"));
        }
        if self.authors_added > 0 {
            v.push(n(self.authors_added as u64, "author added", "authors added"));
        }
        v.join(", ")
    }
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
        let mut rules: Vec<Rule> = vec![];
        let mut rule_ids = std::collections::BTreeSet::new();
        let owned = journals
            .iter()
            .map(|j| (OwnerKind::Journal, j.journal.id.clone()))
            .chain(live_searches.iter().map(|l| (OwnerKind::LiveSearch, l.id.clone())))
            .map(|(k, i)| self.owned_rules(k, &i))
            .collect::<Result<Vec<_>>>()?;
        for r in self.project_rules(&project.id)?.into_iter().chain(owned.into_iter().flatten()) {
            if rule_ids.insert(r.id.clone()) {
                rules.push(r);
            }
        }
        // models it uses (items, report sections, live searches, rules), and what they refer to
        let mut model_ids: Vec<String> = items.iter().filter(|i| i.kind == ItemKind::Model).map(|i| i.ref_id.clone()).collect();
        model_ids.extend(reports.iter().flat_map(|r| r.sections.iter().filter_map(|s| s.params.model.clone())));
        model_ids.extend(live_searches.iter().filter_map(|l| l.spec.model.clone()));
        model_ids.extend(rules.iter().flat_map(|r| r.spec.then.iter().filter_map(|t| then_model(t).cloned())));
        let mut models: Vec<Model> = vec![];
        let mut seen_models = std::collections::BTreeSet::new();
        while let Some(id) = model_ids.pop() {
            if !seen_models.insert(id.clone()) {
                continue;
            }
            let Some(m) = self.model(&id)? else { continue };
            for f in m.design.fields.iter().chain(m.draft.iter().flat_map(|d| d.fields.iter())) {
                if let FieldType::Ref(t) = &f.ty {
                    model_ids.push(t.clone());
                }
            }
            // its own rules and the rules that created or migrated it
            let authors_of = self.model_versions(&m.id)?.into_iter().filter_map(|v| v.rule_id);
            let migrators = authors_of.filter_map(|id| self.rule(&id).transpose()).collect::<Result<Vec<_>>>()?;
            for r in self.owned_rules(OwnerKind::Model, &m.id)?.into_iter().chain(migrators) {
                if rule_ids.insert(r.id.clone()) {
                    model_ids.extend(r.spec.then.iter().filter_map(|t| then_model(t).cloned()));
                    rules.push(r);
                }
            }
            models.push(m);
        }
        // code spaces it uses (items, report sections, rules): their source
        let mut code_ids: Vec<String> = items.iter().filter(|i| i.kind == ItemKind::Code).map(|i| i.ref_id.clone()).collect();
        for s in reports.iter_mut().flat_map(|r| r.sections.iter_mut()).filter(|s| s.kind == SectionKind::Code) {
            // sections from before 2.5 name the space; the pack names it by id
            if let Some(c) = s.params.space.as_deref().and_then(|k| self.find_code_space(k).ok()) {
                s.params.space = Some(c.id.clone());
                code_ids.push(c.id);
            }
        }
        code_ids.extend(rules.iter().flat_map(|r| r.spec.then.iter()).filter_map(|t| match t {
            Then::RunCode { space } => Some(space.clone()),
            _ => None,
        }));
        let mut code = vec![];
        let mut seen_code = std::collections::BTreeSet::new();
        for id in code_ids {
            if seen_code.insert(id.clone())
                && let Some(mut c) = self.code_space(&id)?
            {
                c.approved_hash = None;
                c.last_run = None;
                code.push(c);
            }
        }
        let mut model_packs = vec![];
        for m in models {
            if m.is_bars() {
                if !m.design.fields.is_empty() {
                    model_packs.push(ModelPack { model: m, versions: vec![], records: vec![] });
                }
                continue;
            }
            let versions = self.model_versions(&m.id)?;
            let records = if m.version > 0 { self.records(&m, &Default::default())? } else { vec![] };
            model_packs.push(ModelPack { model: m, versions, records });
        }
        let mut ids: Vec<Option<String>> = vec![project.author_id.clone()];
        for mp in &model_packs {
            ids.push(mp.model.author_id.clone());
            ids.extend(mp.versions.iter().map(|v| v.author_id.clone()));
            ids.extend(mp.records.iter().map(|r| r.author_id.clone()));
        }
        ids.extend(items.iter().map(|i| i.author_id.clone()));
        ids.extend(rules.iter().map(|r| r.author_id.clone()));
        ids.extend(code.iter().map(|c| c.author_id.clone()));
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
            rules,
            data: vec![],
            models: model_packs,
            code,
        };
        if with_data {
            for r in data_refs(self, &pack)? {
                let n = self.count_filtered(&r.filter())? as usize;
                if n > 0 {
                    pack.data.push(DataPack { series: r, bars: n, file: None, rows: None });
                }
            }
            // values of the fields added to bars, on the bars it carries
            let data = pack.data.clone();
            if let Some(mp) = pack.models.iter_mut().find(|m| m.model.is_bars())
                && mp.model.version > 0
            {
                let names: Vec<String> = mp.model.design.fields.iter().map(|f| f.name.clone()).collect();
                for d in &data {
                    let s = &d.series;
                    let c = |f: &str, op: super::Op, v: String| super::Condition { field: f.into(), op, value: v };
                    let mut filter = vec![
                        c("source", super::Op::Eq, s.source.clone()),
                        c("base", super::Op::Eq, s.symbol.base.clone()),
                        c("quote", super::Op::Eq, s.symbol.quote.clone()),
                        c("period", super::Op::Eq, s.duration.to_string()),
                    ];
                    filter.extend(s.from.map(|f| c("timestamp", super::Op::Ge, f.to_string())));
                    filter.extend(s.to.map(|t| c("timestamp", super::Op::Le, t.to_string())));
                    for mut r in self.records(&mp.model, &super::RecordQuery { filter, ..Default::default() })? {
                        r.values.retain(|k, v| names.contains(k) && !v.is_null());
                        if !r.values.is_empty() {
                            mp.records.push(r);
                        }
                    }
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
        // models get their new ids first: searches, sections, rules and other models name them
        for mp in &p.models {
            if !mp.model.is_bars() {
                ids.insert(mp.model.id.clone(), super::new_id());
            }
        }
        for c in &p.code {
            let mut nc = c.clone();
            nc.id = super::new_id();
            let names: Vec<String> = self.code_spaces()?.into_iter().map(|x| x.name.to_lowercase()).collect();
            if names.contains(&nc.name.to_lowercase()) {
                nc.name = format!("{} (imported)", nc.name);
                let mut i = 2;
                while names.contains(&nc.name.to_lowercase()) {
                    nc.name = format!("{} (imported {i})", c.name);
                    i += 1;
                }
            }
            // an imported code space runs only after its new owner approves it
            nc.approved_hash = None;
            nc.last_run = None;
            self.insert_code_space(&nc)?;
            ids.insert(c.id.clone(), nc.id);
            out.code += 1;
        }
        for s in &p.live_searches {
            let nid = super::new_id();
            let mut s = s.clone();
            s.spec.model = s.spec.model.as_ref().map(|m| ids.get(m).cloned().unwrap_or_else(|| m.clone()));
            let s = &s;
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
                ns.params.model = map(&s.params.model);
                ns.params.space = map(&s.params.space);
                ns.params.entries = s.params.entries.iter().map(|e| ids.get(e).cloned().unwrap_or_else(|| e.clone())).collect();
                self.insert_section(&ns)?;
            }
            out.reports += 1;
        }
        for r in &p.rules {
            let mut nr = r.clone();
            nr.id = super::new_id();
            if self.rules()?.iter().any(|x| x.name == nr.name) {
                nr.name = format!("{} (imported)", nr.name);
            }
            let map = |v: &str| ids.get(v).cloned().unwrap_or_else(|| v.to_string());
            nr.owner_id = r.owner_id.as_deref().and_then(|o| ids.get(o).cloned());
            if nr.owner_id.is_none() {
                nr.owner_kind = OwnerKind::Standalone;
            }
            for t in &mut nr.spec.then {
                remap_then(t, &map);
            }
            // a rule acts on its own; it waits for `mnnn rule enable` after an import
            nr.enabled = false;
            nr.runs = 0;
            nr.last_run_at = None;
            nr.last_status = None;
            self.insert_rule(&nr)?;
            ids.insert(r.id.clone(), nr.id);
            out.rules += 1;
        }
        for mp in p.models.iter().filter(|m| !m.model.is_bars()) {
            let n = self.restore_model(mp, &ids)?;
            out.models += 1;
            out.records += n;
        }
        for it in &p.items {
            let ref_id = match it.kind {
                ItemKind::Result => it.ref_id.clone(),
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
        if let Some(mp) = p.models.iter().find(|m| m.model.is_bars()) {
            out.records += self.merge_bars_model(mp, &ids)?;
        }
        out.project = Some(np);
        Ok(out)
    }
}

impl Store {
    /// A model from a pack, under its new id (a name in use gets `_imported`).
    fn restore_model(&self, mp: &ModelPack, ids: &HashMap<String, String>) -> Result<usize> {
        let mut m = mp.model.clone();
        m.id = ids.get(&m.id).cloned().unwrap_or_else(super::new_id);
        m.builtin = false;
        let names: Vec<String> = self.models()?.into_iter().map(|x| x.name).collect();
        if names.contains(&m.name) {
            let base = format!("{}_imported", m.name);
            m.name = base.clone();
            let mut i = 2;
            while names.contains(&m.name) {
                m.name = format!("{base}{i}");
                i += 1;
            }
        }
        m.design = remap_design(&m.design, ids);
        m.draft = m.draft.as_ref().map(|d| remap_design(d, ids));
        self.insert_model(&m)?;
        for v in &mp.versions {
            let mut nv = v.clone();
            nv.model_id = m.id.clone();
            nv.design = remap_design(&v.design, ids);
            nv.rule_id = v.rule_id.as_ref().and_then(|r| ids.get(r).cloned());
            self.insert_model_version(&nv)?;
        }
        if m.version == 0 {
            return Ok(0);
        }
        self.rebuild(&m, &m.design.clone(), m.version, &[])?;
        let records: Vec<Record> = mp
            .records
            .iter()
            .map(|r| {
                // record ids are kept, so references between records still hold
                let mut r = r.clone();
                r.values.retain(|k, _| m.design.field(k).is_some());
                r
            })
            .collect();
        self.insert_raw_records(&m, &records)
    }

    /// Fields added to bars in the pack join this store's bars (as optional fields); their
    /// values land on the bars both stores have.
    fn merge_bars_model(&self, mp: &ModelPack, ids: &HashMap<String, String>) -> Result<usize> {
        let Some(local) = self.model(BARS)? else { return Ok(0) };
        let theirs = remap_design(&mp.model.design, ids);
        let add: Vec<_> = theirs
            .fields
            .iter()
            .filter(|f| local.pending().field(&f.name).is_none() && local.design.field(&f.name).is_none())
            .map(|f| {
                let mut f = f.clone();
                f.id = super::models::new_field_id();
                f.required = false;
                f
            })
            .collect();
        if !add.is_empty() {
            let mut d = local.design.clone();
            d.fields.extend(add.iter().cloned());
            self.apply_design(&local, &d, &ApplyOpts { destructive: false, note: "import".into() })?;
            if let Some(mut dr) = local.draft.clone() {
                dr.fields.extend(add);
                self.set_model_draft(BARS, Some(dr))?;
            }
        }
        let local = self.model(BARS)?.ok_or_else(|| Error::invalid("model", "bars is missing"))?;
        let mut n = 0;
        for r in &mp.records {
            if !self.record_exists(BARS, &r.id)? {
                continue;
            }
            let vals: serde_json::Map<String, serde_json::Value> =
                r.values.iter().filter(|(k, _)| local.design.field(k).is_some()).map(|(k, v)| (k.clone(), v.clone())).collect();
            if !vals.is_empty() {
                self.update_record(&local, &r.id, &vals)?;
                n += 1;
            }
        }
        Ok(n)
    }
}

fn remap_design(d: &Design, ids: &HashMap<String, String>) -> Design {
    let mut d = d.clone();
    for f in &mut d.fields {
        if let FieldType::Ref(t) = &mut f.ty
            && let Some(n) = ids.get(t.as_str())
        {
            *t = n.clone();
        }
    }
    d
}

/// The model a then-action names, if any.
fn then_model(t: &Then) -> Option<&String> {
    match t {
        Then::ModelEdit { model, .. }
        | Then::ModelApply { model, .. }
        | Then::ModelRemove { model, .. }
        | Then::RecordCreate { model, .. }
        | Then::RecordUpdate { model, .. }
        | Then::RecordDelete { model, .. } => model.as_ref(),
        Then::AddToProject { item: ItemRef::Model(m), .. } => Some(m),
        _ => None,
    }
}

/// Point a then-action at the imported copies of what it names.
fn remap_then(t: &mut Then, map: &dyn Fn(&str) -> String) {
    let opt = |v: &mut Option<String>| {
        if let Some(x) = v {
            *x = map(x);
        }
    };
    match t {
        Then::Status { .. } => {}
        Then::RunSearch { search } => opt(search),
        Then::RunCode { space } => *space = map(space),
        Then::ExportReport { report, .. } => *report = map(report),
        Then::ExportJournal { journal, .. } | Then::WriteEntry { journal, .. } | Then::AddNote { journal, .. } => opt(journal),
        Then::AddToProject { project, item } => {
            opt(project);
            match item {
                ItemRef::Subject => {}
                ItemRef::LiveSearch(i) | ItemRef::Journal(i) | ItemRef::Report(i) | ItemRef::Model(i) => *i = map(i),
            }
        }
        Then::ModelCreate { .. } => {}
        Then::ModelEdit { model, .. }
        | Then::ModelApply { model, .. }
        | Then::ModelRemove { model, .. }
        | Then::RecordCreate { model, .. }
        | Then::RecordUpdate { model, .. }
        | Then::RecordDelete { model, .. } => opt(model),
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

#[cfg(test)]
mod tests {
    use super::Restored;

    #[test]
    fn restored_counts_read_naturally() {
        let r = Restored { entries: 1, attachments: 2, reports: 1, bars: 0, ..Default::default() };
        assert_eq!(r.counts(false), "1 entry, 2 images");
        assert_eq!(r.counts(true), "0 journals, 1 entry, 2 images, 1 report, 0 live searches, 0 new bars");
        let r = Restored { rules: 2, ..Default::default() };
        assert!(r.counts(true).contains("2 rules (off)"));
    }

    #[test]
    fn rules_travel_with_projects() {
        use crate::store::*;
        let a = Store::open_in_memory().unwrap();
        let p = a.create_project("P", "").unwrap();
        let j = a.create_journal("J", Some(&p.id), EntryFormat::Markdown).unwrap();
        a.add_project_item(&p.id, ItemKind::Journal, &j.id, serde_json::json!({})).unwrap();
        let spec = |t: Then| RuleSpec { on: vec!["*".into()], then: vec![t], ..Default::default() };
        let owned = a
            .create_rule(&NewRule {
                name: "in journal".into(),
                owner_kind: OwnerKind::Journal,
                owner_id: Some(j.id.clone()),
                spec: spec(Then::AddNote { journal: Some(j.id.clone()), text: "x".into() }),
                enabled: true,
                ..Default::default()
            })
            .unwrap();
        let member = a
            .create_rule(&NewRule {
                name: "in project".into(),
                spec: spec(Then::AddToProject { project: Some(p.id.clone()), item: ItemRef::Subject }),
                enabled: true,
                ..Default::default()
            })
            .unwrap();
        a.add_project_item(&p.id, ItemKind::RuleSet, &member.id, serde_json::json!({})).unwrap();
        a.create_rule(&NewRule { name: "elsewhere".into(), spec: spec(Then::Status { text: "x".into() }), ..Default::default() }).unwrap();
        let pack = a.project_pack(&p.id, false).unwrap();
        assert_eq!(pack.rules.len(), 2);
        let json = serde_json::to_string(&pack).unwrap();
        let mut b = Store::open_in_memory().unwrap();
        let out = b.import_project(&serde_json::from_str(&json).unwrap(), None, &super::Inline).unwrap();
        assert_eq!(out.rules, 2);
        let np = out.project.unwrap();
        let nj = &out.journals[0];
        let rules = b.rules().unwrap();
        assert!(rules.iter().all(|r| !r.enabled && r.id != owned.id && r.id != member.id));
        let o = rules.iter().find(|r| r.name == "in journal").unwrap();
        assert_eq!(o.owner_id.as_deref(), Some(nj.id.as_str()));
        assert_eq!(o.spec.then[0], Then::AddNote { journal: Some(nj.id.clone()), text: "x".into() });
        let m = &b.project_rules(&np.id).unwrap()[0];
        assert_eq!(m.spec.then[0], Then::AddToProject { project: Some(np.id.clone()), item: ItemRef::Subject });
        // a 2.2 pack (no rules) still reads
        let mut v: serde_json::Value = serde_json::from_str(&json).unwrap();
        v.as_object_mut().unwrap().remove("rules");
        let old = b.import_project(&serde_json::from_value(v).unwrap(), None, &super::Inline).unwrap();
        assert_eq!((old.rules, old.journals.len()), (0, 1));
        let p2 = old.project.unwrap();
        assert!(b.project_rules(&p2.id).unwrap().is_empty(), "a rule item with no rule in the pack is dropped");
    }
}
