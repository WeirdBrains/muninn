//! Documents for people: journals, entries, reports, projects and bars as a [`Doc`], ready
//! for any writer in [`crate::render`] (HTML, LaTeX, PDF, Docx, text). Data sections are
//! built from what is stored now.
//!
//! Entry text keeps its notes and highlights: highlighted words are marked, a note on
//! selected words becomes a footnote at the end of those words, notes on the whole entry
//! follow it and notes on the journal close the document. Every piece names its author.
use crate::error::{Error, Result};
use crate::ingest::time::fmt_ts;
use crate::render::{Block, Doc, Image, Inline, markdown, meta, para, plain, shift_headings, t};
use crate::store::journals::EntryDoc;
use crate::store::{AnnotationKind, BarFilter, BarSort, EntryFormat, ItemKind, Report, SearchTarget, Section, SectionKind, SeriesRef, Store};
use std::collections::{BTreeMap, BTreeSet};

const HL_ON: char = '\u{E000}';
const HL_OFF: char = '\u{E001}';
const NOTE_ON: char = '\u{E002}';
const NOTE_OFF: char = '\u{E003}';

fn is_sentinel(c: char) -> bool {
    ('\u{E000}'..='\u{E003}').contains(&c)
}

fn strip(s: &str) -> String {
    let mut out = String::new();
    let mut in_note = false;
    for c in s.chars() {
        match c {
            NOTE_ON => in_note = true,
            NOTE_OFF => in_note = false,
            c if is_sentinel(c) => {}
            _ if in_note => {}
            c => out.push(c),
        }
    }
    out
}

/// A date for people (`2026-10-29`).
pub fn day(ms: i64) -> String {
    fmt_ts(ms).chars().take(10).collect()
}

/// A price or amount without noise (`42000.5`, `0.00012`).
pub fn num(v: f64) -> String {
    if v.is_finite() && v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{v:.0}")
    } else if v.abs() >= 1000.0 {
        format!("{v:.2}")
    } else {
        let s = format!("{v:.8}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// A report's code space: by id (or, for sections made before 2.5, by name).
pub fn code_space_of(store: &Store, key: &str) -> Option<crate::store::CodeSpace> {
    store.find_code_space(key).ok()
}

/// Builds documents; collects images and who contributed.
pub struct Builder<'a> {
    pub store: &'a Store,
    pub images: BTreeMap<String, Image>,
    pub authors: Vec<String>,
}

impl<'a> Builder<'a> {
    pub fn new(store: &'a Store) -> Self {
        Builder { store, images: BTreeMap::new(), authors: vec![] }
    }

    /// The author's name (and remember them as a contributor).
    fn who(&mut self, id: Option<&str>) -> String {
        let name = crate::history::author_label(self.store, id);
        if id.is_some() && !self.authors.contains(&name) {
            self.authors.push(name.clone());
        }
        name
    }

    fn doc(self, title: String, blocks: Vec<Block>) -> Doc {
        Doc { title, authors: self.authors, date: day(crate::store::now_ms()), blocks, images: self.images }
    }

    /// One entry: heading (at `level`), who and when, the text with notes and highlights,
    /// images not shown in the text, and notes on the entry.
    pub fn entry(&mut self, ed: &EntryDoc, level: u8) -> Result<Vec<Block>> {
        let e = &ed.entry;
        let mut out = vec![Block::Heading(level, vec![t(e.heading())])];
        let author = self.who(e.author_id.as_deref());
        let mut m = format!("{author} · {}", day(e.created_at));
        if e.updated_at > e.created_at + 60_000 {
            m.push_str(&format!(" · edited {}", day(e.updated_at)));
        }
        out.push(meta(m));
        // images
        for a in &ed.attachments {
            if let Some((att, data)) = self.store.attachment_data(&a.id)? {
                self.images.insert(att.id.clone(), Image { name: att.name, mime: att.mime, data });
            }
        }
        // notes and highlights on spans become sentinels in the text
        let mut marks: Vec<(usize, u8, usize)> = vec![]; // (char offset, order, note index or 0)
        let mut notes: Vec<Vec<Inline>> = vec![];
        let mut loose: Vec<Vec<Inline>> = vec![];
        for ad in &ed.annotations {
            let a = &ad.annotation;
            let by = self.who(a.author_id.as_deref());
            let mut text = vec![];
            if let Some(q) = &a.quote {
                text.push(Inline::Italic(vec![t(format!("“{q}”"))]));
                text.push(t(if a.body.is_empty() { "" } else { ": " }));
            }
            if !a.body.is_empty() {
                text.push(t(a.body.clone()));
            }
            text.push(t(format!(" — {by}")));
            match a.span() {
                Some((s, en)) => {
                    if a.kind == AnnotationKind::Highlight {
                        marks.push((s, 1, 0));
                        marks.push((en, 0, 0));
                    }
                    if a.kind == AnnotationKind::Note || !a.body.is_empty() {
                        notes.push(text);
                        marks.push((en, 2, notes.len()));
                    }
                }
                None if a.kind == AnnotationKind::Note || !a.body.is_empty() => {
                    if a.detached() {
                        text.insert(0, t("(the quoted words are no longer in the entry) "));
                    }
                    loose.push(text);
                }
                None => {}
            }
        }
        marks.sort();
        let mut body = String::new();
        let mut it = marks.iter().peekable();
        for (i, c) in e.body.chars().chain(std::iter::once('\0')).enumerate() {
            while let Some((pos, kind, n)) = it.peek() {
                if *pos != i {
                    break;
                }
                match kind {
                    0 => body.push(HL_OFF),
                    1 => body.push(HL_ON),
                    _ => {
                        body.push(NOTE_ON);
                        body.push_str(&n.to_string());
                        body.push(NOTE_OFF);
                    }
                }
                it.next();
            }
            if c != '\0' {
                body.push(c);
            }
        }
        let blocks = match e.format {
            EntryFormat::Markdown => markdown::parse(&body),
            EntryFormat::Latex => crate::render::latex::read_blocks(&body, &self.images),
        };
        let mut used = BTreeSet::new();
        let mut depth = 0usize;
        let mut blocks = post_blocks(blocks, &mut depth, &notes, &mut used);
        blocks = shift_headings(blocks, level);
        // images not shown in the text
        let shown: BTreeSet<String> = image_keys(&blocks);
        for a in &ed.attachments {
            if !shown.contains(&a.id) && self.images.contains_key(&a.id) {
                blocks.push(Block::Image { key: a.id.clone(), caption: a.name.clone() });
            }
        }
        out.extend(blocks);
        for (i, n) in notes.into_iter().enumerate() {
            if !used.contains(&(i + 1)) {
                loose.push(n);
            }
        }
        if !loose.is_empty() {
            out.push(Block::Para(vec![Inline::Bold(vec![t("Notes")])]));
            out.push(Block::List { ordered: false, items: loose.into_iter().map(|n| vec![Block::Para(n)]).collect() });
        }
        Ok(out)
    }

    /// Blocks of a journal (all entries, or the chosen ones) with entry headings at `level`.
    pub fn journal_blocks(&mut self, id: &str, entries: &[String], level: u8, with_notes: bool) -> Result<Vec<Block>> {
        let jd = self.store.journal_doc(id)?;
        let mut out = vec![];
        let chosen: Vec<&EntryDoc> = if entries.is_empty() {
            jd.entries.iter().collect()
        } else {
            let v: Vec<&EntryDoc> = jd.entries.iter().filter(|e| entries.contains(&e.entry.id)).collect();
            if v.len() != entries.len() {
                return Err(Error::invalid(
                    "entry",
                    format!("{} of the chosen entries are not in journal {}", entries.len() - v.len(), jd.journal.name),
                ));
            }
            v
        };
        for e in &chosen {
            out.extend(self.entry(e, level)?);
        }
        if chosen.is_empty() {
            out.push(meta("No entries yet."));
        }
        if with_notes && !jd.notes.is_empty() {
            out.push(Block::Heading(level, vec![t("Notes on this journal")]));
            let mut items = vec![];
            for n in &jd.notes {
                let by = self.who(n.annotation.author_id.as_deref());
                items.push(vec![Block::Para(vec![t(n.annotation.body.clone()), t(format!(" — {by}, {}", day(n.annotation.created_at)))])]);
            }
            out.push(Block::List { ordered: false, items });
        }
        Ok(out)
    }

    /// Summary and table of a series (or a range of it); `rows` 0 lists every bar, else the
    /// latest `rows` bars.
    pub fn records(&mut self, r: &SeriesRef, rows: usize) -> Result<Vec<Block>> {
        let f = r.filter();
        let Some(s) = self.store.bar_summary(&f)? else {
            return Ok(vec![meta(format!("No bars stored now for {}.", r.label()))]);
        };
        let change = s.change_pct().map(|c| format!("{c:+.2}%")).unwrap_or_else(|| "-".into());
        let mut out = vec![Block::Table {
            head: vec![
                "bars".into(),
                "from".into(),
                "to".into(),
                "open".into(),
                "high".into(),
                "low".into(),
                "close".into(),
                "change".into(),
                "volume".into(),
            ],
            rows: vec![vec![
                s.count.to_string(),
                fmt_ts(s.first),
                fmt_ts(s.last),
                num(s.open),
                num(s.high),
                num(s.low),
                num(s.close),
                change,
                num(s.volume),
            ]],
        }];
        let limit = if rows == 0 { s.count } else { rows as i64 };
        let mut bars = self.store.page_bars(&f, BarSort::Time, true, limit, 0)?;
        bars.reverse();
        let shown = bars.len() as i64;
        out.push(meta(if shown < s.count { format!("The latest {shown} of {} bars:", s.count) } else { format!("All {} bars:", s.count) }));
        out.push(Block::Table {
            head: vec!["time (UTC)".into(), "open".into(), "high".into(), "low".into(), "close".into(), "volume".into()],
            rows: bars.iter().map(|b| vec![fmt_ts(b.ts), num(b.open), num(b.high), num(b.low), num(b.close), num(b.volume)]).collect(),
        });
        Ok(out)
    }

    /// A model's design and records (those matching `filter`; `rows` 0 lists all).
    pub fn model_records(&mut self, id: &str, filter: &[crate::store::Condition], rows: usize) -> Result<Vec<Block>> {
        let Some(m) = self.store.model(id)? else { return Ok(vec![meta("(removed model)")]) };
        let mut out = vec![];
        if !m.description.is_empty() {
            out.push(Block::Para(vec![Inline::Italic(vec![t(m.description.clone())])]));
        }
        let ty = |f: &crate::store::Field| crate::designer::type_text(self.store, &f.ty);
        out.push(Block::Table {
            head: vec!["field".into(), "type".into(), "constraints".into(), "about".into()],
            rows: m
                .design
                .fields
                .iter()
                .map(|f| {
                    let mut c = vec![];
                    if f.required {
                        c.push("required".to_string());
                    }
                    if f.unique {
                        c.push("unique".into());
                    }
                    if let Some(d) = &f.default {
                        c.push(format!("default {}", crate::designer::value_text(d)));
                    }
                    if !f.choices.is_empty() {
                        c.push(f.choices.join(" | "));
                    }
                    vec![f.name.clone(), ty(f), c.join(", "), f.about.clone()]
                })
                .collect(),
        });
        if m.version == 0 {
            out.push(meta(format!("{} has not been applied yet.", m.name)));
            return Ok(out);
        }
        let q = crate::store::RecordQuery { filter: filter.to_vec(), ..Default::default() };
        let all = if m.is_bars() {
            self.store.records(
                &m,
                &crate::store::RecordQuery { limit: Some(if rows == 0 { crate::store::reports::DEFAULT_ROWS } else { rows }), ..q.clone() },
            )?
        } else {
            self.store.records(&m, &q)?
        };
        let total = if m.is_bars() { self.store.count_records(&m)? } else { all.len() };
        let shown: Vec<_> = if rows == 0 { all } else { all.into_iter().take(rows).collect() };
        let mut cond = String::new();
        if !filter.is_empty() {
            cond = format!(" where {}", filter.iter().map(crate::rules::condition_text).collect::<Vec<_>>().join(", "));
        }
        out.push(meta(format!("{} v{}: {} of {} record{}{cond}.", m.name, m.version, shown.len(), total, if total == 1 { "" } else { "s" })));
        let names = crate::store::records::field_names(&m);
        let mut head: Vec<String> = vec!["id".into()];
        head.extend(names.iter().cloned());
        out.push(Block::Table {
            head,
            rows: shown
                .iter()
                .map(|r| {
                    let mut row = vec![r.short_id().to_string()];
                    row.extend(names.iter().map(|n| match (crate::store::records::field_type(&m, n), r.values.get(n)) {
                        (Some(crate::store::FieldType::Timestamp), Some(v)) if v.is_i64() => fmt_ts(v.as_i64().unwrap_or(0)),
                        (_, Some(v)) => crate::designer::value_text(v),
                        _ => String::new(),
                    }));
                    row
                })
                .collect(),
        });
        Ok(out)
    }

    /// What a live search matches now (series or entries), or every series.
    pub fn series(&mut self, search: Option<&str>) -> Result<Vec<Block>> {
        let mut out = vec![];
        let list = match search {
            Some(id) => {
                let Some(s) = self.store.live_search(id)? else { return Ok(vec![meta("(removed live search)")]) };
                out.push(meta(format!("Live search {}: {}", s.name, s.spec.describe())));
                if s.spec.target == SearchTarget::Records {
                    let found = crate::live::record_matches(self.store, &s.spec)?;
                    out.push(Block::Table {
                        head: vec!["model".into(), "record".into(), "values".into(), "created".into()],
                        rows: found
                            .iter()
                            .map(|(m, r)| vec![m.name.clone(), r.short_id().to_string(), crate::live::record_brief(r), day(r.created_at)])
                            .collect(),
                    });
                    return Ok(out);
                }
                if s.spec.target == SearchTarget::Journals {
                    let found = crate::live::entry_matches(self.store, &s.spec)?;
                    out.push(Block::Table {
                        head: vec!["journal".into(), "entry".into(), "written".into()],
                        rows: found.iter().map(|(j, e)| vec![j.name.clone(), e.heading(), day(e.created_at)]).collect(),
                    });
                    return Ok(out);
                }
                crate::live::series_matches(self.store, &s.spec)?
            }
            None => self.store.series()?,
        };
        if list.is_empty() {
            out.push(meta("No series match now."));
            return Ok(out);
        }
        out.push(Block::Table {
            head: vec!["symbol".into(), "source".into(), "period".into(), "bars".into(), "first".into(), "last".into()],
            rows: list
                .iter()
                .map(|s| vec![s.symbol.display(), s.source.clone(), s.duration.to_string(), s.count.to_string(), fmt_ts(s.first), fmt_ts(s.last)])
                .collect(),
        });
        Ok(out)
    }

    /// A project's items as a table.
    pub fn project_items(&mut self, id: &str) -> Result<Vec<Block>> {
        let Some(p) = self.store.project(id)? else { return Ok(vec![meta("(removed project)")]) };
        let mut out = vec![];
        if !p.description.is_empty() {
            out.push(para(p.description.clone()));
        }
        let by = self.who(p.author_id.as_deref());
        out.push(meta(format!("Project {} by {by}, created {}", p.name, day(p.created_at))));
        let items = self.store.project_items(&p.id)?;
        if items.is_empty() {
            out.push(meta("The project is empty."));
            return Ok(out);
        }
        let mut rows = vec![];
        for it in &items {
            let (name, now) = crate::live::describe_item(self.store, it);
            let who = self.who(it.author_id.as_deref());
            rows.push(vec![it.kind.label().to_string(), name, now, format!("{who}, {}", day(it.added_at))]);
        }
        out.push(Block::Table { head: vec!["kind".into(), "name".into(), "now".into(), "added".into()], rows });
        Ok(out)
    }

    /// Everyone who contributed to a report, the sections they added and what they wrote.
    fn authors_table(&mut self, r: &Report, sections: &[Section]) -> Result<Vec<Block>> {
        let mut by: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut add = |id: &Option<String>, what: String| {
            if let Some(i) = id {
                by.entry(i.clone()).or_default().push(what);
            }
        };
        add(&r.author_id, "created the report".into());
        for s in sections {
            add(&s.author_id, format!("section {}", s.position));
            if s.kind == SectionKind::Journal
                && let Some(j) = &s.params.journal
                && let Ok(jd) = self.store.journal_doc(j)
            {
                for e in jd.entries.iter().filter(|e| s.params.entries.is_empty() || s.params.entries.contains(&e.entry.id)) {
                    add(&e.entry.author_id, format!("entry {}", e.entry.heading()));
                }
            }
        }
        // Later edits are in the history: whoever changed the report or one of its sections.
        for a in self.store.actions()?.iter().filter(|a| {
            matches!(a.kind.as_str(), "report.edit" | "report.section.edit" | "report.section.move" | "report.section.remove")
                && a.params.get("report").and_then(|v| v.as_str()) == Some(r.id.as_str())
        }) {
            add(&a.author_id, "edits".into());
        }
        let mut rows = vec![];
        for (id, what) in by {
            let a = self.store.author(&id)?;
            let name = self.who(Some(&id));
            let email = a.as_ref().and_then(|a| a.email.clone()).unwrap_or_default();
            let details = a.map(|a| a.details.iter().map(|(k, v)| format!("{k}: {v}")).collect::<Vec<_>>().join("; ")).unwrap_or_default();
            let mut what: Vec<String> = what;
            let mut seen = std::collections::HashSet::new();
            what.retain(|w| seen.insert(w.clone()));
            rows.push(vec![name, email, details, what.join(", ")]);
        }
        if rows.is_empty() {
            return Ok(vec![meta("No authors recorded (set one with `mnnn author use`).")]);
        }
        Ok(vec![Block::Table { head: vec!["author".into(), "email".into(), "details".into(), "contributed".into()], rows }])
    }

    /// The content of one report section (without its heading).
    pub fn section(&mut self, r: &Report, s: &Section, all: &[Section]) -> Result<Vec<Block>> {
        let p = &s.params;
        Ok(match s.kind {
            SectionKind::Text => {
                let blocks = match s.format {
                    EntryFormat::Markdown => markdown::parse(&s.body),
                    EntryFormat::Latex => crate::render::latex::read_blocks(&s.body, &self.images),
                };
                shift_headings(blocks, 1)
            }
            SectionKind::Records => match &p.series {
                Some(sr) => self.records(sr, p.rows.unwrap_or(crate::store::reports::DEFAULT_ROWS))?,
                None => vec![meta("(no series chosen)")],
            },
            SectionKind::Series => self.series(p.search.as_deref())?,
            SectionKind::Journal => match &p.journal {
                Some(j) if self.store.journal(j)?.is_some() => {
                    let entries: Vec<String> = p.entries.iter().filter(|e| self.store.entry(e).ok().flatten().is_some()).cloned().collect();
                    let all = !p.entries.is_empty() && entries.is_empty();
                    if all { vec![meta("(the chosen entries were removed)")] } else { self.journal_blocks(j, &entries, 2, p.entries.is_empty())? }
                }
                _ => vec![meta("(removed journal)")],
            },
            SectionKind::Project => match &p.project {
                Some(id) => self.project_items(id)?,
                None => vec![meta("(no project chosen)")],
            },
            SectionKind::Authors => self.authors_table(r, all)?,
            SectionKind::Model => match &p.model {
                Some(id) => self.model_records(id, &p.conditions, p.rows.unwrap_or(crate::store::reports::DEFAULT_ROWS))?,
                None => vec![meta("(no model chosen)")],
            },
            SectionKind::Code => match p.space.as_deref().and_then(|k| code_space_of(self.store, k)) {
                Some(c) => self.code_blocks(&c)?,
                None => vec![meta(format!("(code space {} not found)", p.space.clone().unwrap_or_else(|| "not chosen".into())))],
            },
        })
    }

    /// A code space: what it is, its last run's result (tables as tables, charts as images
    /// with their numbers), and its source.
    pub fn code_blocks(&mut self, c: &crate::store::CodeSpace) -> Result<Vec<Block>> {
        let mut out = vec![];
        if !c.description.is_empty() {
            out.push(para(c.description.clone()));
        }
        out.push(meta(format!("{} code by {} · input: {}", c.lang.as_str(), self.who(c.author_id.as_deref()), c.input.text())));
        match &c.last_run {
            None => out.push(meta(format!("Not run yet (`mnnn code run {}`).", c.name))),
            Some(r) => {
                let writes = if r.writes > 0 { format!(" · {} record(s) written", r.writes) } else { String::new() };
                out.push(meta(format!("Last run {} ({} ms){writes}", crate::ingest::time::fmt_ts(r.at), r.ms)));
                if let Some(e) = &r.error {
                    out.push(Block::Para(vec![Inline::Bold(vec![t("The last run failed: ")]), t(e.clone())]));
                }
                if let Some(v) = crate::code::run_view(r) {
                    for (i, leaf) in v.leaves().into_iter().enumerate() {
                        if !leaf.title().is_empty() {
                            out.push(Block::Para(vec![Inline::Bold(vec![t(leaf.title().to_string())])]));
                        }
                        out.extend(self.view_blocks(&c.id, i, leaf));
                    }
                }
            }
        }
        out.push(meta(format!("Source ({})", c.lang.as_str())));
        out.push(Block::Code(c.source.clone()));
        Ok(out)
    }

    fn view_blocks(&mut self, id: &str, i: usize, v: &crate::code::view::ResultView) -> Vec<Block> {
        use crate::code::view::ResultView;
        const ROWS: usize = 200;
        match v {
            ResultView::Text { text, .. } => markdown::parse(text),
            ResultView::Table { .. } => {
                let (head, mut rows) = v.as_table();
                let n = rows.len();
                rows.truncate(ROWS);
                let mut out = vec![Block::Table { head, rows }];
                if n > ROWS {
                    out.push(meta(format!("The first {ROWS} of {n} rows.")));
                }
                out
            }
            ResultView::Chart { x, series, .. } => {
                let mut out = vec![];
                if let Some(data) = crate::code::png::chart(x, series, 900, 320) {
                    let key = format!("code-{}-{i}", crate::store::short(id));
                    self.images.insert(key.clone(), Image { name: format!("{key}.png"), mime: "image/png".into(), data });
                    out.push(Block::Image { key, caption: crate::code::png::legend(series) });
                }
                // the numbers behind it: first, last, low and high of each line
                let rows = series
                    .iter()
                    .map(|s| {
                        let vals: Vec<f64> = s.values.iter().flatten().copied().filter(|v| v.is_finite()).collect();
                        let f = |o: Option<f64>| o.map(num).unwrap_or_default();
                        let lo = vals.iter().copied().fold(None, |a: Option<f64>, v| Some(a.map_or(v, |a| a.min(v))));
                        let hi = vals.iter().copied().fold(None, |a: Option<f64>, v| Some(a.map_or(v, |a| a.max(v))));
                        vec![s.name.clone(), f(vals.first().copied()), f(vals.last().copied()), f(lo), f(hi), vals.len().to_string()]
                    })
                    .collect();
                out.push(Block::Table { head: ["line", "first", "last", "low", "high", "points"].map(String::from).to_vec(), rows });
                out
            }
            ResultView::Tiles { .. } => vec![],
        }
    }

    /// All sections of a report, each under a level-1 heading with who added it.
    pub fn report_blocks(&mut self, r: &Report, level: u8) -> Result<Vec<Block>> {
        let sections = self.store.sections(&r.id)?;
        let mut out = vec![];
        for s in &sections {
            let heading = match s.params.space.as_deref().and_then(|k| code_space_of(self.store, k)) {
                Some(c) if s.kind == SectionKind::Code => format!("Code space {}", c.name),
                _ => s.heading(),
            };
            out.push(Block::Heading(level, vec![t(heading)]));
            let by = self.who(s.author_id.as_deref());
            let mut m = format!("Added by {by} on {}", day(s.created_at));
            if s.updated_at > s.created_at + 60_000 {
                m.push_str(&format!(" · edited {}", day(s.updated_at)));
            }
            out.push(meta(m));
            out.extend(shift_headings(self.section(r, s, &sections)?, level - 1));
        }
        if sections.is_empty() {
            out.push(meta("This report has no sections yet."));
        }
        Ok(out)
    }
}

fn image_keys(blocks: &[Block]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for b in blocks {
        match b {
            Block::Image { key, .. } => {
                out.insert(key.clone());
            }
            Block::Quote(v) => out.extend(image_keys(v)),
            Block::List { items, .. } => items.iter().for_each(|it| out.extend(image_keys(it))),
            _ => {}
        }
    }
    out
}

/// Replace sentinels: marked words become [`Inline::Mark`], note markers become footnotes.
fn post_blocks(blocks: Vec<Block>, depth: &mut usize, notes: &[Vec<Inline>], used: &mut BTreeSet<usize>) -> Vec<Block> {
    blocks
        .into_iter()
        .map(|b| match b {
            Block::Heading(l, v) => Block::Heading(l, post_inlines(v, depth, notes, used)),
            Block::Para(v) => Block::Para(post_inlines(v, depth, notes, used)),
            Block::List { ordered, items } => {
                Block::List { ordered, items: items.into_iter().map(|it| post_blocks(it, depth, notes, used)).collect() }
            }
            Block::Quote(v) => Block::Quote(post_blocks(v, depth, notes, used)),
            Block::Code(c) => Block::Code(track(&c, depth)),
            Block::Math(m) => Block::Math(track(&m, depth)),
            Block::Table { head, rows } => Block::Table {
                head: head.iter().map(|c| track(c, depth)).collect(),
                rows: rows.iter().map(|r| r.iter().map(|c| track(c, depth)).collect()).collect(),
            },
            Block::Image { key, caption } => Block::Image { key: strip(&key), caption: track(&caption, depth) },
            o => o,
        })
        .collect()
}

/// Strip sentinels from text that cannot hold marks, keeping track of highlight depth.
fn track(s: &str, depth: &mut usize) -> String {
    for c in s.chars() {
        match c {
            HL_ON => *depth += 1,
            HL_OFF => *depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    strip(s)
}

fn post_inlines(v: Vec<Inline>, depth: &mut usize, notes: &[Vec<Inline>], used: &mut BTreeSet<usize>) -> Vec<Inline> {
    let mut out: Vec<Inline> = vec![];
    let push = |out: &mut Vec<Inline>, i: Inline, marked: bool| {
        if !marked {
            out.push(i);
            return;
        }
        match out.last_mut() {
            Some(Inline::Mark(c)) => c.push(i),
            _ => out.push(Inline::Mark(vec![i])),
        }
    };
    for i in v {
        match i {
            Inline::Text(s) => {
                let mut cur = String::new();
                let mut chars = s.chars().peekable();
                while let Some(c) = chars.next() {
                    match c {
                        HL_ON | HL_OFF | NOTE_ON => {
                            if !cur.is_empty() {
                                push(&mut out, Inline::Text(std::mem::take(&mut cur)), *depth > 0);
                            }
                            match c {
                                HL_ON => *depth += 1,
                                HL_OFF => *depth = depth.saturating_sub(1),
                                _ => {
                                    let mut n = String::new();
                                    for d in chars.by_ref() {
                                        if d == NOTE_OFF {
                                            break;
                                        }
                                        n.push(d);
                                    }
                                    if let Ok(k) = n.parse::<usize>()
                                        && let Some(note) = notes.get(k.wrapping_sub(1))
                                    {
                                        used.insert(k);
                                        out.push(Inline::Note(note.clone()));
                                    }
                                }
                            }
                        }
                        NOTE_OFF => {}
                        c => cur.push(c),
                    }
                }
                if !cur.is_empty() {
                    push(&mut out, Inline::Text(cur), *depth > 0);
                }
            }
            Inline::Bold(c) => {
                let m = *depth > 0;
                let inner = post_inlines(c, depth, notes, used);
                push(&mut out, Inline::Bold(unmark(inner)), m);
            }
            Inline::Italic(c) => {
                let m = *depth > 0;
                let inner = post_inlines(c, depth, notes, used);
                push(&mut out, Inline::Italic(unmark(inner)), m);
            }
            Inline::Mark(c) => {
                let inner = post_inlines(c, depth, notes, used);
                out.push(Inline::Mark(unmark(inner)));
            }
            Inline::Link(c, u) => {
                let m = *depth > 0;
                let inner = post_inlines(c, depth, notes, used);
                push(&mut out, Inline::Link(unmark(inner), strip(&u)), m);
            }
            Inline::Mono(s) => {
                let m = *depth > 0;
                let s = track(&s, depth);
                push(&mut out, Inline::Mono(s), m || *depth > 0);
            }
            Inline::Math(s) => {
                let m = *depth > 0;
                let s = track(&s, depth);
                push(&mut out, Inline::Math(s), m || *depth > 0);
            }
            o => out.push(o),
        }
    }
    out
}

/// Inside a container that is already marked as a whole, inner marks are redundant; inner
/// marks of a partly-marked container stay.
fn unmark(v: Vec<Inline>) -> Vec<Inline> {
    if v.len() == 1
        && let Inline::Mark(_) = &v[0]
    {
        return v;
    }
    v
}

/// What a section shows, in one line (`records BTC/USDT binance 5m · latest 20 bars`).
pub fn section_summary(store: &Store, s: &Section) -> String {
    let p = &s.params;
    let name = |r: Result<Option<String>>, gone: &str| r.ok().flatten().unwrap_or_else(|| gone.to_string());
    match s.kind {
        SectionKind::Text => {
            let words = s.body.split_whitespace().count();
            format!("text · {} · {words} words", s.format.as_str())
        }
        SectionKind::Records => match &p.series {
            Some(r) => {
                let rows = match p.rows.unwrap_or(crate::store::reports::DEFAULT_ROWS) {
                    0 => "every bar".to_string(),
                    n => format!("latest {n} bars"),
                };
                format!("records {} · {rows}", r.label())
            }
            None => "records (no series)".into(),
        },
        SectionKind::Series => match &p.search {
            Some(id) => format!("series matching live search {}", name(store.live_search(id).map(|o| o.map(|l| l.name)), "(removed)")),
            None => "every series".into(),
        },
        SectionKind::Journal => {
            let j = name(p.journal.as_deref().map(|j| store.journal(j).map(|o| o.map(|j| j.name))).unwrap_or(Ok(None)), "(removed)");
            match p.entries.len() {
                0 => format!("journal {j} · every entry"),
                1 => format!("journal {j} · 1 entry"),
                n => format!("journal {j} · {n} entries"),
            }
        }
        SectionKind::Project => {
            format!("project {}", name(p.project.as_deref().map(|x| store.project(x).map(|o| o.map(|p| p.name))).unwrap_or(Ok(None)), "(removed)"))
        }
        SectionKind::Authors => "authors of the report".into(),
        SectionKind::Code => match p.space.as_deref().and_then(|k| code_space_of(store, k)) {
            Some(c) => format!("code space {} ({})", c.name, c.lang.as_str()),
            None => format!("code space {} (not found)", p.space.clone().unwrap_or_default()),
        },
        SectionKind::Model => {
            let m = name(p.model.as_deref().map(|x| store.model(x).map(|o| o.map(|m| m.name))).unwrap_or(Ok(None)), "(removed)");
            let w = if p.conditions.is_empty() {
                String::new()
            } else {
                format!(" where {}", p.conditions.iter().map(crate::rules::condition_text).collect::<Vec<_>>().join(", "))
            };
            format!("model {m} records{w}")
        }
    }
}

/// A journal, or some of its entries, as a document.
pub fn journal(store: &Store, id: &str, entries: &[String]) -> Result<Doc> {
    let j = store.journal(id)?.ok_or_else(|| Error::invalid("journal", format!("`{id}` not found")))?;
    let mut b = Builder::new(store);
    b.who(j.author_id.as_deref());
    let mut blocks = vec![];
    if let Some(p) = &j.project_id
        && let Some(p) = store.project(p)?
    {
        blocks.push(meta(format!("Journal of project {}", p.name)));
    }
    blocks.extend(b.journal_blocks(&j.id, entries, 1, entries.is_empty())?);
    let title = if entries.len() == 1 { store.entry(&entries[0])?.map(|e| e.heading()).unwrap_or_else(|| j.name.clone()) } else { j.name.clone() };
    let mut d = b.doc(title, blocks);
    if entries.len() == 1 {
        // a single entry: its title is the document title
        if let Some(Block::Heading(1, _)) = d.blocks.iter().find(|b| matches!(b, Block::Heading(..))) {
            let at = d.blocks.iter().position(|b| matches!(b, Block::Heading(1, _))).unwrap_or(0);
            d.blocks.remove(at);
        }
        d.title = format!("{} — {}", d.title, j.name);
    }
    Ok(d)
}

/// A report as a document, rendered from the data stored now.
pub fn report(store: &Store, id: &str) -> Result<Doc> {
    let r = store.report(id)?.ok_or_else(|| Error::invalid("report", format!("`{id}` not found")))?;
    let mut b = Builder::new(store);
    b.who(r.author_id.as_deref());
    let mut blocks = vec![];
    if !r.description.is_empty() {
        blocks.push(Block::Para(vec![Inline::Italic(vec![t(r.description.clone())])]));
    }
    blocks.extend(b.report_blocks(&r, 1)?);
    Ok(b.doc(r.name.clone(), blocks))
}

/// A whole project for reading: its items, then every report, journal, result and live
/// search in it. `rows`: bars listed per result (0: all).
pub fn project(store: &Store, id: &str, rows: usize) -> Result<Doc> {
    let p = store.project(id)?.ok_or_else(|| Error::invalid("project", format!("`{id}` not found")))?;
    let mut b = Builder::new(store);
    b.who(p.author_id.as_deref());
    let mut blocks = vec![Block::Heading(1, vec![t("Contents")])];
    blocks.extend(b.project_items(&p.id)?);
    for it in store.project_items(&p.id)? {
        let (name, _) = crate::live::describe_item(store, &it);
        match it.kind {
            ItemKind::Report => {
                if let Some(r) = store.report(&it.ref_id)? {
                    blocks.push(Block::Heading(1, vec![t(format!("Report: {}", r.name))]));
                    if !r.description.is_empty() {
                        blocks.push(Block::Para(vec![Inline::Italic(vec![t(r.description.clone())])]));
                    }
                    blocks.extend(b.report_blocks(&r, 2)?);
                }
            }
            ItemKind::Journal => {
                if store.journal(&it.ref_id)?.is_some() {
                    blocks.push(Block::Heading(1, vec![t(format!("Journal: {name}"))]));
                    blocks.extend(b.journal_blocks(&it.ref_id, &[], 2, true)?);
                }
            }
            ItemKind::Result => {
                if let Some(r) = it.series() {
                    blocks.push(Block::Heading(1, vec![t(format!("Result: {}", r.label()))]));
                    blocks.extend(b.records(&r, rows)?);
                }
            }
            ItemKind::LiveSearch => {
                if store.live_search(&it.ref_id)?.is_some() {
                    blocks.push(Block::Heading(1, vec![t(format!("Live search: {name}"))]));
                    blocks.extend(b.series(Some(&it.ref_id))?);
                }
            }
            ItemKind::Model => {
                if store.model(&it.ref_id)?.is_some() {
                    blocks.push(Block::Heading(1, vec![t(format!("Model: {name}"))]));
                    blocks.extend(b.model_records(&it.ref_id, &[], rows)?);
                }
            }
            ItemKind::Code => {
                if let Some(c) = store.code_space(&it.ref_id)? {
                    blocks.push(Block::Heading(1, vec![t(format!("Code space: {}", c.name))]));
                    blocks.extend(b.code_blocks(&c)?);
                }
            }
            ItemKind::RuleSet => {
                if let Some(r) = store.rule(&it.ref_id)? {
                    blocks.push(Block::Heading(1, vec![t(format!("Rule: {}", r.name))]));
                    let mut lines = vec![crate::rules::brief(&r)];
                    lines.extend(r.spec.when.iter().map(|c| format!("if {}", crate::rules::condition_text(c))));
                    lines.extend(r.spec.then.iter().map(|x| format!("then {}", crate::rules::then_text(store, x))));
                    blocks.extend(lines.into_iter().map(|l| Block::Para(vec![t(l)])));
                }
            }
        }
    }
    let mut d = b.doc(p.name.clone(), blocks);
    if !p.description.is_empty() {
        d.blocks.insert(0, Block::Para(vec![Inline::Italic(vec![t(p.description.clone())])]));
    }
    Ok(d)
}

/// Bars as a document: one section per series with a summary and every bar.
pub fn bars(store: &Store, f: &BarFilter) -> Result<Doc> {
    let mut b = Builder::new(store);
    let mut blocks = vec![];
    for s in store.series()? {
        if f.source.as_ref().is_some_and(|x| *x != s.source)
            || f.symbol.as_ref().is_some_and(|x| *x != s.symbol)
            || f.duration.is_some_and(|d| d != s.duration)
        {
            continue;
        }
        let r = SeriesRef { source: s.source.clone(), symbol: s.symbol.clone(), duration: s.duration, from: f.from, to: f.to };
        if store.count_filtered(&r.filter())? == 0 {
            continue;
        }
        blocks.push(Block::Heading(1, vec![t(r.label())]));
        blocks.extend(b.records(&r, 0)?);
    }
    if blocks.is_empty() {
        blocks.push(meta("No bars match."));
    }
    Ok(b.doc("Bars".into(), blocks))
}

/// Every table in a document as CSV, with a leading `section` column (the level-1 heading
/// it is under). Tables with different columns are separated by a blank line and a new
/// header row.
pub fn tables_csv(d: &Doc) -> String {
    use crate::render::csv::line;
    let mut out = String::new();
    let mut section = String::new();
    let mut last_head: Option<Vec<String>> = None;
    fn walk(blocks: &[Block], section: &mut String, last: &mut Option<Vec<String>>, out: &mut String) {
        for b in blocks {
            match b {
                Block::Heading(1, v) => *section = plain(v),
                Block::Table { head, rows } => {
                    if last.as_ref() != Some(head) {
                        if !out.is_empty() {
                            out.push('\n');
                        }
                        let mut h = vec!["section".to_string()];
                        h.extend(head.iter().cloned());
                        out.push_str(&line(&h));
                        *last = Some(head.clone());
                    }
                    for r in rows {
                        let mut v = vec![section.clone()];
                        v.extend(r.iter().cloned());
                        out.push_str(&line(&v));
                    }
                }
                Block::Quote(v) => walk(v, section, last, out),
                Block::List { items, .. } => items.iter().for_each(|it| walk(it, section, last, out)),
                _ => {}
            }
        }
    }
    walk(&d.blocks, &mut section, &mut last_head, &mut out);
    if out.is_empty() {
        out.push_str("section\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Bar, Duration, Symbol};
    use crate::render::{DocFormat, render};
    use crate::store::{NewSection, Span};

    pub(crate) fn store_with_data() -> Store {
        let mut s = Store::open_in_memory().unwrap();
        s.add_author("Ada", Some("ada@example.com"), &[("org".into(), "Analytical".into())]).unwrap();
        let src = s.add_source("binance", "file", None).unwrap();
        let bars: Vec<Bar> = (0..30)
            .map(|i| Bar {
                ts: i * 300_000,
                open: 100.0 + i as f64,
                high: 102.0 + i as f64,
                low: 99.0,
                close: 101.0 + i as f64,
                volume: 2.5,
                flags: 0,
            })
            .collect();
        s.upsert_bars(&src, &Symbol::new("BTC", "USDT"), Duration(300), &bars).unwrap();
        s
    }

    #[test]
    fn entries_keep_notes_and_highlights() {
        let s = store_with_data();
        let j = s.create_journal("Log", None, EntryFormat::Markdown).unwrap();
        let e = s.add_entry(&j.id, "Monday", "BTC **broke out** today. Volume was thin.\n\n```\ncode here\n```", None).unwrap();
        let png = crate::render::sample::png(4, 4);
        let a = s.attach_image(&e.id, "chart.png", &png).unwrap();
        s.update_entry(&e.id, None, Some(&format!("{}\n\n![chart](attachment:{})", e.body, a.id)), None).unwrap();
        let body = s.entry(&e.id).unwrap().unwrap().body;
        s.add_highlight(&e.id, crate::store::journals::select(&body, "**broke out** today", 1).unwrap(), None, "").unwrap();
        s.add_note(&j.id, Some(&e.id), Some(crate::store::journals::select(&body, "thin", 1).unwrap()), "check volume").unwrap();
        s.add_note(&j.id, Some(&e.id), Some(crate::store::journals::select(&body, "code here", 1).unwrap()), "inside code").unwrap();
        s.add_note(&j.id, Some(&e.id), None, "whole entry").unwrap();
        s.add_note(&j.id, None, None, "on the journal").unwrap();
        let d = journal(&s, &j.id, &[]).unwrap();
        assert_eq!(d.title, "Log");
        assert_eq!(d.authors, vec!["Ada".to_string()]);
        assert!(d.images.contains_key(&a.id));
        let html = String::from_utf8(render(&d, DocFormat::Html).unwrap()).unwrap();
        assert!(html.contains("<mark>"), "{html}");
        assert!(html.contains("check volume — Ada"));
        assert!(html.contains("whole entry — Ada"));
        assert!(html.contains("inside code — Ada"), "a note that cannot be placed is listed");
        assert!(html.contains("on the journal"));
        assert!(!html.contains('\u{E000}') && !html.contains('\u{E002}'), "no sentinels left");
        assert!(html.contains("code here"));
        let txt = crate::render::text::write(&d, 100);
        assert!(txt.contains("broke out"));
        for f in [DocFormat::Pdf, DocFormat::Docx, DocFormat::Latex] {
            render(&d, f).unwrap();
        }
        let one = journal(&s, &j.id, std::slice::from_ref(&e.id)).unwrap();
        assert_eq!(one.title, "Monday — Log");
    }

    #[test]
    fn reports_render_live_sections_and_csv() {
        let s = store_with_data();
        let r = s.create_report("Weekly", "what moved").unwrap();
        s.add_section(&r.id, &NewSection::text("Intro", "Hello **world**", EntryFormat::Markdown)).unwrap();
        let mut rec = NewSection::new(SectionKind::Records);
        rec.params.series =
            Some(SeriesRef { source: "binance".into(), symbol: Symbol::new("BTC", "USDT"), duration: Duration(300), from: None, to: None });
        rec.params.rows = Some(5);
        s.add_section(&r.id, &rec).unwrap();
        s.add_section(&r.id, &NewSection::new(SectionKind::Series)).unwrap();
        s.add_section(&r.id, &NewSection::new(SectionKind::Authors)).unwrap();
        let mut code = NewSection::new(SectionKind::Code);
        code.params.space = Some("lab".into());
        s.add_section(&r.id, &code).unwrap();
        let d = report(&s, &r.id).unwrap();
        let txt = crate::render::text::write(&d, 120);
        assert!(txt.contains("The latest 5 of 30 bars"), "{txt}");
        assert!(txt.contains("+30.00%"), "change from first open to last close: {txt}");
        assert!(txt.contains("ada@example.com"));
        assert!(txt.contains("code space lab"));
        assert!(txt.contains("Added by Ada"));
        let csv = tables_csv(&d);
        let rows = crate::render::csv::parse(&csv);
        assert!(rows.iter().any(|r| r[0] == "BTC/USDT binance 5m" && r.get(1).map(String::as_str) == Some("30")), "{csv}");
        assert!(rows.iter().any(|r| r.first().map(String::as_str) == Some("section") && r.get(1).map(String::as_str) == Some("time (UTC)")));
        assert_eq!(rows.iter().filter(|r| r.len() == 7 && r[1].starts_with("1970")).count(), 5);
    }

    #[test]
    fn code_sections_show_the_last_result_and_the_source() {
        let s = store_with_data();
        let c =
            crate::code::create(&s, "cli", "fast", "doubles", crate::store::Lang::Math, "@title Doubled\ndouble = close * 2\n", Default::default())
                .unwrap();
        let r = s.create_report("Weekly", "").unwrap();
        let mut sec = NewSection::new(SectionKind::Code);
        sec.params.space = Some(c.id.clone());
        s.add_section(&r.id, &sec).unwrap();
        let txt = crate::render::text::write(&report(&s, &r.id).unwrap(), 120);
        assert!(txt.contains("Code space fast") && txt.contains("Not run yet") && txt.contains("double = close * 2"), "{txt}");
        let view = serde_json::json!({ "kind": "tiles", "tiles": [
            { "kind": "text", "title": "Notes", "text": "All **good**" },
            { "kind": "table", "title": "Rows", "columns": ["a", "b"], "rows": [[1, "x"], [2, "y"]] },
            { "kind": "chart", "title": "Close", "x": { "name": "row", "values": [0.0, 1.0, 2.0] }, "series": [{ "name": "close", "values": [1.0, 3.0, 2.0] }] },
        ] });
        let run = crate::store::RunInfo { at: 1_704_067_200_000, status: "ok".into(), view: Some(view), writes: 3, ms: 12, ..Default::default() };
        s.set_code_run(&c.id, &run).unwrap();
        let d = report(&s, &r.id).unwrap();
        let txt = crate::render::text::write(&d, 120);
        assert!(txt.contains("Last run") && txt.contains("12 ms") && txt.contains("3 record(s) written"), "{txt}");
        assert!(txt.contains("Notes") && txt.contains("good") && txt.contains("Rows") && txt.contains("Close"), "{txt}");
        assert_eq!(d.images.len(), 1, "the chart is an image");
        assert!(d.images.values().next().unwrap().data.starts_with(b"\x89PNG"));
        // a failed run says so
        let run = crate::store::RunInfo { status: "error".into(), error: Some("boom".into()), ..run };
        s.set_code_run(&c.id, &run).unwrap();
        let txt = crate::render::text::write(&report(&s, &r.id).unwrap(), 120);
        assert!(txt.contains("The last run failed") && txt.contains("boom"), "{txt}");
        // a removed space leaves a note
        s.remove_code_space(&c.id).unwrap();
        let txt = crate::render::text::write(&report(&s, &r.id).unwrap(), 120);
        assert!(txt.contains("not found"), "{txt}");
    }

    #[test]
    fn projects_and_bars_render() {
        let s = store_with_data();
        let p = s.create_project("Halving", "notes").unwrap();
        let j = s.create_journal("Log", Some(&p.id), EntryFormat::Latex).unwrap();
        s.add_entry(&j.id, "T", "\\section{Hi} \\textbf{bold}", None).unwrap();
        let r = SeriesRef { source: "binance".into(), symbol: Symbol::new("BTC", "USDT"), duration: Duration(300), from: Some(0), to: Some(600_000) };
        s.add_project_result(&p.id, &r).unwrap();
        let rep = s.create_report("R", "").unwrap();
        s.add_project_item(&p.id, ItemKind::Report, &rep.id, serde_json::json!({})).unwrap();
        let d = project(&s, &p.id, 0).unwrap();
        let txt = crate::render::text::write(&d, 120);
        assert!(txt.contains("Journal: Log") && txt.contains("Result: BTC/USDT") && txt.contains("Report: R"), "{txt}");
        assert!(txt.contains("All 3 bars"));
        let b = bars(&s, &BarFilter::default()).unwrap();
        assert!(crate::render::text::write(&b, 120).contains("All 30 bars"));
        let _ = Span { start: 0, end: 1 };
    }

    #[test]
    fn numbers_read_well() {
        assert_eq!((num(42000.0), num(42000.456), num(0.000123), num(1.5)), ("42000".into(), "42000.46".into(), "0.000123".into(), "1.5".into()));
    }

    #[test]
    fn reports_list_model_records() {
        use crate::store::{Design, FieldOp};
        let s = store_with_data();
        let (m, _) = s.create_model("signals", "trade ideas", Design::default(), false).unwrap();
        s.edit_model(&m.id, &[FieldOp::Add { spec: "symbol:text!".into() }, FieldOp::Add { spec: "price:real".into() }]).unwrap();
        let (m, _) = s.apply_model(&m.id, &Default::default()).unwrap();
        for (sym, p) in [("BTC/USDT", 42000), ("ETH/USDT", 2500)] {
            s.create_record(&m, &serde_json::json!({ "symbol": sym, "price": p }).as_object().unwrap().clone()).unwrap();
        }
        let r = s.create_report("Ideas", "").unwrap();
        let mut sec = NewSection::new(SectionKind::Model);
        sec.params.model = Some(m.id.clone());
        sec.params.conditions = vec![crate::rules::parse_condition("price > 10000").unwrap()];
        s.add_section(&r.id, &sec).unwrap();
        let d = report(&s, &r.id).unwrap();
        let txt = crate::render::text::write(&d, 120);
        assert!(txt.contains("BTC/USDT") && !txt.contains("ETH/USDT"), "{txt}");
        assert!(txt.contains("symbol") && txt.contains("price"), "{txt}");
        let csv = tables_csv(&d);
        assert!(csv.contains("BTC/USDT"), "{csv}");
    }
}
