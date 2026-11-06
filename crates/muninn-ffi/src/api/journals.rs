//! Journals (`mnnn journal` / TUI Journals): list, create, entries, notes,
//! word-span highlights, images, copy and document export.
//! Image bytes stay in Rust (attach/save take file paths); Dart sees names and text.
use super::data::open_write;
use anyhow::Context;
use muninn_core::export::archive;
use muninn_core::export::files::{self as export_files, OutFormat, Target};
use muninn_core::store::journals as jstore;
use muninn_core::store::{Annotation, AnnotationKind, EntryFormat, Journal, NewAction, Span, Store};
use std::path::{Path, PathBuf};

pub struct JournalInfo {
    pub id: String,
    pub short_id: String,
    pub name: String,
    pub project_id: Option<String>,
    pub project_name: String,
    pub format: String,
    pub entries: i64,
    pub forked_from: Option<String>,
}

pub struct EntryInfo {
    pub id: String,
    pub short_id: String,
    pub journal_id: String,
    pub title: String,
    pub heading: String,
    pub body: String,
    pub format: String,
    pub position: i64,
}

pub struct JournalNoteInfo {
    pub id: String,
    pub short_id: String,
    pub body: String,
}

fn fmt(f: EntryFormat) -> String {
    f.as_str().into()
}

fn parse_fmt(s: &str) -> anyhow::Result<EntryFormat> {
    Ok(EntryFormat::parse(s)?)
}

fn journal_info(store: &Store, j: Journal) -> anyhow::Result<JournalInfo> {
    let entries = store.entries(&j.id)?.len() as i64;
    let project_name = match &j.project_id {
        Some(id) => store.project(id)?.map(|p| p.name).unwrap_or_default(),
        None => String::new(),
    };
    Ok(JournalInfo {
        short_id: j.short_id().to_string(),
        id: j.id,
        name: j.name,
        project_id: j.project_id,
        project_name,
        format: fmt(j.format),
        entries,
        forked_from: j.forked_from,
    })
}

fn entry_info(e: muninn_core::store::Entry) -> EntryInfo {
    EntryInfo {
        short_id: e.short_id().to_string(),
        heading: e.heading(),
        id: e.id,
        journal_id: e.journal_id,
        title: e.title,
        body: e.body,
        format: fmt(e.format),
        position: e.position,
    }
}

fn note(store: &Store, kind: &str, summary: String, params: serde_json::Value) {
    store.note_action(&NewAction::new(kind, "app", summary).params(params));
}

pub(crate) fn journals_of(store: &Store) -> anyhow::Result<Vec<JournalInfo>> {
    store.journals()?.into_iter().map(|j| journal_info(store, j)).collect()
}

/// `mnnn journal list`.
pub fn journals() -> anyhow::Result<Vec<JournalInfo>> {
    journals_of(&super::data::open()?)
}

pub(crate) fn create_journal_of(store: &Store, name: &str, project: Option<&str>, format: &str) -> anyhow::Result<JournalInfo> {
    let name = name.trim();
    if name.is_empty() {
        return Err(anyhow::anyhow!("give the journal a name"));
    }
    let project = project.map(str::trim).filter(|s| !s.is_empty());
    let project_id = match project {
        Some(p) => Some(store.find_project(p)?.id),
        None => None,
    };
    let j = store.create_journal(name, project_id.as_deref(), parse_fmt(format)?)?;
    note(store, "journal.create", format!("create journal {}", j.name), serde_json::json!({ "journal": j.id, "name": j.name }));
    journal_info(store, j)
}

/// `mnnn journal new` (standalone or in a project).
pub fn create_journal(name: String, project: Option<String>, format: String) -> anyhow::Result<JournalInfo> {
    create_journal_of(&open_write()?, &name, project.as_deref(), if format.trim().is_empty() { "markdown" } else { &format })
}

pub(crate) fn update_journal_of(store: &Store, id: &str, name: Option<&str>, format: Option<&str>) -> anyhow::Result<JournalInfo> {
    let j = store.find_journal(id)?;
    let format = format.map(parse_fmt).transpose()?;
    let j = store.update_journal(&j.id, name.map(str::trim).filter(|s| !s.is_empty()), format)?;
    note(store, "journal.edit", format!("edit journal {}", j.name), serde_json::json!({ "journal": j.id, "name": j.name }));
    journal_info(store, j)
}

pub fn update_journal(id: String, name: Option<String>, format: Option<String>) -> anyhow::Result<JournalInfo> {
    update_journal_of(&open_write()?, &id, name.as_deref(), format.as_deref())
}

pub(crate) fn remove_journal_of(store: &Store, id: &str) -> anyhow::Result<String> {
    let j = store.find_journal(id)?;
    let name = j.name.clone();
    store.remove_journal(&j.id)?;
    note(store, "journal.remove", format!("remove journal {name}"), serde_json::json!({ "journal": j.id, "name": name }));
    Ok(format!("removed journal {name}"))
}

pub fn remove_journal(id: String) -> anyhow::Result<String> {
    remove_journal_of(&open_write()?, &id)
}

pub(crate) fn fork_journal_of(store: &Store, id: &str, project: Option<&str>, name: Option<&str>) -> anyhow::Result<JournalInfo> {
    let j = store.find_journal(id)?;
    let project_id = match project.map(str::trim).filter(|s| !s.is_empty()) {
        Some(p) => Some(store.find_project(p)?.id),
        None => None,
    };
    let n = store.fork_journal(&j.id, project_id.as_deref(), name.map(str::trim).filter(|s| !s.is_empty()))?;
    note(store, "journal.fork", format!("fork journal {} → {}", j.name, n.name), serde_json::json!({ "journal": n.id, "name": n.name }));
    journal_info(store, n)
}

pub fn fork_journal(id: String, project: Option<String>, name: Option<String>) -> anyhow::Result<JournalInfo> {
    fork_journal_of(&open_write()?, &id, project.as_deref(), name.as_deref())
}

pub(crate) fn entries_of(store: &Store, journal: &str) -> anyhow::Result<Vec<EntryInfo>> {
    let j = store.find_journal(journal)?;
    Ok(store.entries(&j.id)?.into_iter().map(entry_info).collect())
}

pub fn journal_entries(journal: String) -> anyhow::Result<Vec<EntryInfo>> {
    entries_of(&super::data::open()?, &journal)
}

pub(crate) fn add_entry_of(store: &Store, journal: &str, title: &str, body: &str, format: Option<&str>) -> anyhow::Result<EntryInfo> {
    let j = store.find_journal(journal)?;
    let format = format.map(parse_fmt).transpose()?;
    let e = store.add_entry(&j.id, title, body, format)?;
    note(store, "journal.entry", format!("write entry {} in journal {}", e.heading(), j.name), serde_json::json!({ "journal": j.id, "entry": e.id }));
    Ok(entry_info(e))
}

pub fn add_journal_entry(journal: String, title: String, body: String, format: Option<String>) -> anyhow::Result<EntryInfo> {
    add_entry_of(&open_write()?, &journal, &title, &body, format.as_deref())
}

pub(crate) fn update_entry_of(
    store: &Store,
    entry: &str,
    title: Option<&str>,
    body: Option<&str>,
    format: Option<&str>,
) -> anyhow::Result<EntryInfo> {
    let e = store.find_entry(entry)?;
    let format = format.map(parse_fmt).transpose()?;
    let e = store.update_entry(&e.id, title, body, format)?;
    note(store, "journal.entry.edit", format!("edit entry {}", e.heading()), serde_json::json!({ "journal": e.journal_id, "entry": e.id }));
    Ok(entry_info(e))
}

pub fn update_journal_entry(entry: String, title: Option<String>, body: Option<String>, format: Option<String>) -> anyhow::Result<EntryInfo> {
    update_entry_of(&open_write()?, &entry, title.as_deref(), body.as_deref(), format.as_deref())
}

pub(crate) fn remove_entry_of(store: &Store, entry: &str) -> anyhow::Result<String> {
    let e = store.find_entry(entry)?;
    let heading = e.heading();
    store.remove_entry(&e.id)?;
    note(store, "journal.entry.remove", format!("remove entry {heading}"), serde_json::json!({ "journal": e.journal_id, "entry": e.id }));
    Ok(format!("removed entry {heading}"))
}

pub fn remove_journal_entry(entry: String) -> anyhow::Result<String> {
    remove_entry_of(&open_write()?, &entry)
}

pub(crate) fn journal_notes_of(store: &Store, journal: &str) -> anyhow::Result<Vec<JournalNoteInfo>> {
    let j = store.find_journal(journal)?;
    Ok(store
        .annotations(&j.id)?
        .into_iter()
        .filter(|a| a.entry_id.is_none() && a.kind == muninn_core::store::AnnotationKind::Note)
        .map(|a| JournalNoteInfo { short_id: a.short_id().to_string(), id: a.id, body: a.body })
        .collect())
}

pub fn journal_notes(journal: String) -> anyhow::Result<Vec<JournalNoteInfo>> {
    journal_notes_of(&super::data::open()?, &journal)
}

pub(crate) fn add_journal_note_of(store: &Store, journal: &str, body: &str) -> anyhow::Result<JournalNoteInfo> {
    let j = store.find_journal(journal)?;
    let a = store.add_note(&j.id, None, None, body)?;
    note(store, "journal.note", format!("note on journal {}", j.name), serde_json::json!({ "journal": j.id, "note": a.id }));
    Ok(JournalNoteInfo { short_id: a.short_id().to_string(), id: a.id, body: a.body })
}

pub fn add_journal_note(journal: String, body: String) -> anyhow::Result<JournalNoteInfo> {
    add_journal_note_of(&open_write()?, &journal, &body)
}

pub struct JournalAnnotationInfo {
    pub id: String,
    pub short_id: String,
    pub journal_id: String,
    pub entry_id: Option<String>,
    pub kind: String,
    pub start: Option<i64>,
    pub end: Option<i64>,
    pub quote: Option<String>,
    pub body: String,
    pub color: Option<String>,
    pub detached: bool,
}

/// Attachment metadata only. Image bytes stay in the store.
pub struct JournalImageInfo {
    pub id: String,
    pub short_id: String,
    pub entry_id: String,
    pub name: String,
    pub mime: String,
    pub size: i64,
}

pub struct JournalExportInfo {
    pub path: String,
    pub files: i64,
    pub message: String,
}

fn kind_label(k: AnnotationKind) -> &'static str {
    match k {
        AnnotationKind::Note => "note",
        AnnotationKind::Highlight => "highlight",
    }
}

fn annotation_info(a: Annotation) -> JournalAnnotationInfo {
    JournalAnnotationInfo {
        short_id: a.short_id().to_string(),
        detached: a.detached(),
        kind: kind_label(a.kind).to_string(),
        start: a.start.map(|n| n as i64),
        end: a.end.map(|n| n as i64),
        id: a.id,
        journal_id: a.journal_id,
        entry_id: a.entry_id,
        quote: a.quote,
        body: a.body,
        color: a.color,
    }
}

fn image_info(a: muninn_core::store::Attachment) -> JournalImageInfo {
    JournalImageInfo { short_id: a.short_id().to_string(), id: a.id, entry_id: a.entry_id, name: a.name, mime: a.mime, size: a.size }
}

fn resolve_span(body: &str, words: Option<&str>, nth: Option<i64>, start: Option<i64>, end: Option<i64>) -> anyhow::Result<Span> {
    if let (Some(s), Some(e)) = (start, end) {
        if s < 0 || e < 0 || e < s {
            return Err(anyhow::anyhow!("range must be start..end with end >= start"));
        }
        return Ok(Span { start: s as usize, end: e as usize });
    }
    let words = words.map(str::trim).filter(|s| !s.is_empty()).ok_or_else(|| anyhow::anyhow!("select words or give a start and end"))?;
    let n = nth.unwrap_or(1).max(1) as usize;
    Ok(jstore::select(body, words, n)?)
}

pub(crate) fn entry_annotations_of(store: &Store, entry: &str) -> anyhow::Result<Vec<JournalAnnotationInfo>> {
    let e = store.find_entry(entry)?;
    Ok(store.entry_annotations(&e.id)?.into_iter().map(annotation_info).collect())
}

/// Notes and highlights on an entry (`mnnn journal entry` / TUI entry view).
pub fn entry_annotations(entry: String) -> anyhow::Result<Vec<JournalAnnotationInfo>> {
    entry_annotations_of(&super::data::open()?, &entry)
}

pub(crate) fn add_entry_note_of(
    store: &Store,
    entry: &str,
    body: &str,
    words: Option<&str>,
    nth: Option<i64>,
    start: Option<i64>,
    end: Option<i64>,
) -> anyhow::Result<JournalAnnotationInfo> {
    let e = store.find_entry(entry)?;
    let span = if words.map(str::trim).filter(|s| !s.is_empty()).is_some() || (start.is_some() && end.is_some()) {
        Some(resolve_span(&e.body, words, nth, start, end)?)
    } else {
        None
    };
    let a = store.add_note(&e.journal_id, Some(&e.id), span, body)?;
    note(
        store,
        "note.add",
        format!("add note on {}", e.heading()),
        serde_json::json!({ "journal": a.journal_id, "entry": a.entry_id, "note": a.id }),
    );
    Ok(annotation_info(a))
}

/// Note on an entry, or on selected words (`mnnn journal note --entry`).
#[allow(clippy::too_many_arguments)]
pub fn add_entry_note(
    entry: String,
    body: String,
    words: Option<String>,
    nth: Option<i64>,
    start: Option<i64>,
    end: Option<i64>,
) -> anyhow::Result<JournalAnnotationInfo> {
    add_entry_note_of(&open_write()?, &entry, &body, words.as_deref(), nth, start, end)
}

pub(crate) fn add_highlight_of(
    store: &Store,
    entry: &str,
    words: Option<&str>,
    nth: Option<i64>,
    start: Option<i64>,
    end: Option<i64>,
    color: Option<&str>,
    comment: Option<&str>,
) -> anyhow::Result<JournalAnnotationInfo> {
    let e = store.find_entry(entry)?;
    let span = resolve_span(&e.body, words, nth, start, end)?;
    let color = color.map(str::trim).filter(|s| !s.is_empty()).or(Some("yellow"));
    let a = store.add_highlight(&e.id, span, color, comment.unwrap_or(""))?;
    note(
        store,
        "highlight.add",
        format!("add highlight on {}", e.heading()),
        serde_json::json!({ "journal": a.journal_id, "entry": a.entry_id, "highlight": a.id }),
    );
    Ok(annotation_info(a))
}

/// Highlight selected words of an entry (`mnnn journal highlight` / TUI `m`).
#[allow(clippy::too_many_arguments)]
pub fn add_entry_highlight(
    entry: String,
    words: Option<String>,
    nth: Option<i64>,
    start: Option<i64>,
    end: Option<i64>,
    color: Option<String>,
    comment: Option<String>,
) -> anyhow::Result<JournalAnnotationInfo> {
    add_highlight_of(&open_write()?, &entry, words.as_deref(), nth, start, end, color.as_deref(), comment.as_deref())
}

pub(crate) fn update_annotation_of(store: &Store, id: &str, body: &str) -> anyhow::Result<JournalAnnotationInfo> {
    let a = store.find_annotation(id)?;
    let a = store.update_annotation(&a.id, body)?;
    note(store, "note.edit", format!("edit {}", kind_label(a.kind)), serde_json::json!({ "journal": a.journal_id, "note": a.id }));
    Ok(annotation_info(a))
}

pub fn update_journal_annotation(id: String, body: String) -> anyhow::Result<JournalAnnotationInfo> {
    update_annotation_of(&open_write()?, &id, &body)
}

pub(crate) fn remove_annotation_of(store: &Store, id: &str) -> anyhow::Result<String> {
    let a = store.find_annotation(id)?;
    store.remove_annotation(&a.id)?;
    let kind = if a.kind == AnnotationKind::Highlight { "highlight.remove" } else { "note.remove" };
    let what = kind_label(a.kind);
    note(store, kind, format!("remove {what}"), serde_json::json!({ "journal": a.journal_id, "id": a.id }));
    Ok(format!("removed {what}"))
}

pub fn remove_journal_annotation(id: String) -> anyhow::Result<String> {
    remove_annotation_of(&open_write()?, &id)
}

pub(crate) fn entry_images_of(store: &Store, entry: &str) -> anyhow::Result<Vec<JournalImageInfo>> {
    let e = store.find_entry(entry)?;
    Ok(store.attachments(&e.id)?.into_iter().map(image_info).collect())
}

/// Image metadata for an entry. Bytes stay in the store.
pub fn entry_images(entry: String) -> anyhow::Result<Vec<JournalImageInfo>> {
    entry_images_of(&super::data::open()?, &entry)
}

pub(crate) fn attach_image_of(store: &Store, entry: &str, path: &str, name: Option<&str>) -> anyhow::Result<JournalImageInfo> {
    let e = store.find_entry(entry)?;
    let path = Path::new(path);
    let data = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let name = name
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "image".into()));
    let a = store.attach_image(&e.id, &name, &data)?;
    note(store, "image.add", format!("attach image {} to {}", a.name, e.heading()), serde_json::json!({ "entry": e.id, "image": a.id }));
    Ok(image_info(a))
}

/// Attach an image from a file path. Rust reads the bytes; Dart never sends them.
pub fn attach_journal_image(entry: String, path: String, name: Option<String>) -> anyhow::Result<JournalImageInfo> {
    attach_image_of(&open_write()?, &entry, &path, name.as_deref())
}

fn image_data(store: &Store, id: &str) -> anyhow::Result<(muninn_core::store::Attachment, Vec<u8>)> {
    let id = id.trim();
    match store.attachment_data(id)? {
        Some(pair) => Ok(pair),
        None => {
            let mut hits = vec![];
            for j in store.journals()? {
                for e in store.entries(&j.id)? {
                    for att in store.attachments(&e.id)? {
                        if att.id.starts_with(id) {
                            hits.push(att.id);
                        }
                    }
                }
            }
            match hits.as_slice() {
                [one] => store.attachment_data(one)?.ok_or_else(|| anyhow::anyhow!("image `{id}` not found")),
                [] => Err(anyhow::anyhow!("image `{id}` not found")),
                _ => Err(anyhow::anyhow!("image `{id}` is ambiguous")),
            }
        }
    }
}

pub(crate) fn save_image_of(store: &Store, id: &str, out: &str) -> anyhow::Result<String> {
    let (a, data) = image_data(store, id)?;
    let out = Path::new(out);
    if let Some(dir) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(out, &data).with_context(|| format!("writing {}", out.display()))?;
    Ok(format!("wrote {} ({} bytes) to {}", a.name, data.len(), out.display()))
}

/// Write an attached image to a file (`mnnn journal image`). Bytes never return to Dart.
pub fn save_journal_image(id: String, out: String) -> anyhow::Result<String> {
    save_image_of(&open_write()?, &id, &out)
}

fn shrink_preview(data: &[u8], max_px: u32) -> Vec<u8> {
    let Ok(img) = image::load_from_memory(data) else {
        return Vec::new();
    };
    let img = img.thumbnail(max_px, max_px);
    let mut out = Vec::new();
    if img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).is_err() {
        return Vec::new();
    }
    out
}

pub(crate) fn journal_image_thumbnail_of(store: &Store, id: &str, max_px: u32) -> anyhow::Result<Vec<u8>> {
    let (_, data) = image_data(store, id)?;
    Ok(shrink_preview(&data, max_px.max(16)))
}

/// A downscaled PNG preview of an attached image. The original bytes stay in the store.
pub fn journal_image_thumbnail(id: String, max_px: Option<u32>) -> anyhow::Result<Vec<u8>> {
    journal_image_thumbnail_of(&super::data::open()?, &id, max_px.unwrap_or(96))
}

pub(crate) fn copy_entries_of(
    store: &Store,
    entries: &[String],
    to: Option<&str>,
    new_name: Option<&str>,
    project: Option<&str>,
) -> anyhow::Result<Vec<EntryInfo>> {
    if entries.is_empty() {
        return Err(anyhow::anyhow!("give at least one entry to copy"));
    }
    let mut ids = Vec::new();
    for k in entries {
        ids.push(store.find_entry(k)?.id);
    }
    let target = match (to.map(str::trim).filter(|s| !s.is_empty()), new_name.map(str::trim).filter(|s| !s.is_empty())) {
        (Some(t), _) => store.find_journal(t)?,
        (None, Some(name)) => {
            let pid = match project.map(str::trim).filter(|s| !s.is_empty()) {
                Some(p) => Some(store.find_project(p)?.id),
                None => None,
            };
            let first = store.entry(&ids[0])?.and_then(|e| store.journal(&e.journal_id).ok().flatten());
            let j = store.create_journal(name, pid.as_deref(), first.map(|j| j.format).unwrap_or_default())?;
            note(store, "journal.create", format!("create journal {}", j.name), serde_json::json!({ "journal": j.id, "name": j.name }));
            j
        }
        (None, None) => return Err(anyhow::anyhow!("copy to an existing journal or give a new journal name")),
    };
    let copied = store.copy_entries(&ids, &target.id)?;
    note(
        store,
        "entry.copy",
        format!("add {} to journal {}", copied.len(), target.name),
        serde_json::json!({ "journal": target.id, "entries": copied.iter().map(|e| e.id.clone()).collect::<Vec<_>>(), "from": ids }),
    );
    Ok(copied.into_iter().map(entry_info).collect())
}

/// Copy entries (with notes, highlights and images) to another journal (`mnnn journal copy`).
pub fn copy_journal_entries(
    entries: Vec<String>,
    to: Option<String>,
    new_name: Option<String>,
    project: Option<String>,
) -> anyhow::Result<Vec<EntryInfo>> {
    copy_entries_of(&open_write()?, &entries, to.as_deref(), new_name.as_deref(), project.as_deref())
}

pub(crate) fn export_journal_of(store: &Store, journal: &str, format: &str, out: &str, entries: &[String]) -> anyhow::Result<JournalExportInfo> {
    let j = store.find_journal(journal)?;
    let mut ids = vec![];
    for k in entries {
        let e = store.find_entry(k)?;
        if e.journal_id != j.id {
            return Err(anyhow::anyhow!("entry {} is not in journal {}", e.heading(), j.name));
        }
        ids.push(e.id);
    }
    let fmt = OutFormat::parse(format)?;
    let dest = PathBuf::from(out);
    let files = export_files::write_recorded(store, &Target::Journal { id: j.id.clone(), entries: ids }, fmt, &dest, "app")?;
    let n = files.len() as i64;
    Ok(JournalExportInfo {
        path: dest.display().to_string(),
        files: n,
        message: format!("exported journal {} to {} ({})", j.name, dest.display(), fmt.as_str()),
    })
}

/// Export a journal as PDF, Word, HTML, LaTeX, text or JSON (`mnnn journal export`). File bytes stay in Rust.
pub fn export_journal(journal: String, format: String, out: String, entries: Vec<String>) -> anyhow::Result<JournalExportInfo> {
    export_journal_of(&open_write()?, &journal, &format, &out, &entries)
}

pub(crate) fn import_journal_of(store: &Store, path: &str, project: Option<&str>, name: Option<&str>) -> anyhow::Result<JournalInfo> {
    let pid = match project.map(str::trim).filter(|s| !s.is_empty()) {
        Some(p) => Some(store.find_project(p)?.id),
        None => None,
    };
    let r = archive::import_journal(store, Path::new(path), pid.as_deref(), name.map(str::trim).filter(|s| !s.is_empty()))?;
    let j = r.journals.first().cloned().ok_or_else(|| anyhow::anyhow!("import produced no journal"))?;
    note(
        store,
        "journal.import",
        format!("import journal {} from {path}", j.name),
        serde_json::json!({ "journal": j.id, "name": j.name, "file": path, "entries": r.entries }),
    );
    journal_info(store, j)
}

/// Import a journal exported as JSON (`mnnn journal import`). Dart only passes the path.
pub fn import_journal(path: String, project: Option<String>, name: Option<String>) -> anyhow::Result<JournalInfo> {
    import_journal_of(&open_write()?, &path, project.as_deref(), name.as_deref())
}

pub(crate) fn print_journal_of(
    store: &Store,
    journal: &str,
    entries: &[String],
    printer: Option<&str>,
    out: Option<&str>,
    save_only: bool,
) -> anyhow::Result<JournalExportInfo> {
    let j = store.find_journal(journal)?;
    let mut ids = vec![];
    for k in entries {
        let e = store.find_entry(k)?;
        if e.journal_id != j.id {
            return Err(anyhow::anyhow!("entry {} is not in journal {}", e.heading(), j.name));
        }
        ids.push(e.id);
    }
    let dest = match out.map(str::trim).filter(|s| !s.is_empty()) {
        Some(p) => PathBuf::from(p),
        None => {
            let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            std::env::temp_dir().join(format!("{}-{ts}.pdf", export_files::slug(&j.name)))
        }
    };
    let dir = dest.parent().filter(|p| !p.as_os_str().is_empty()).map(Path::to_path_buf).unwrap_or_else(std::env::temp_dir);
    let printed = export_files::print_journal(store, &j.id, &ids, printer, Some(dest.as_path()), &dir, !save_only, "app")?;
    Ok(JournalExportInfo { path: printed.pdf.display().to_string(), files: 1, message: printed.message() })
}

/// Print a journal as PDF (`mnnn journal print` / TUI `P`). Default is save-only; `out` is a path, never ~/.muninn unless the caller chose it.
pub fn print_journal(
    journal: String,
    entries: Vec<String>,
    printer: Option<String>,
    out: Option<String>,
    save_only: bool,
) -> anyhow::Result<JournalExportInfo> {
    print_journal_of(&open_write()?, &journal, &entries, printer.as_deref(), out.as_deref(), save_only)
}

#[cfg(test)]
mod tests {
    use super::*;
    use muninn_core::store::Store;

    #[test]
    fn create_write_note_and_remove() {
        let s = Store::open_in_memory().unwrap();
        let j = create_journal_of(&s, "Notes", None, "markdown").unwrap();
        assert_eq!((j.name.as_str(), j.format.as_str(), j.entries), ("Notes", "markdown", 0));
        let e = add_entry_of(&s, &j.id, "Hello", "body text", None).unwrap();
        assert_eq!((e.title.as_str(), e.body.as_str()), ("Hello", "body text"));
        assert_eq!(entries_of(&s, &j.id).unwrap().len(), 1);
        let n = add_journal_note_of(&s, &j.id, "a thought").unwrap();
        assert_eq!(n.body, "a thought");
        assert_eq!(journal_notes_of(&s, &j.id).unwrap().len(), 1);
        let e = update_entry_of(&s, &e.id, Some("Hi"), None, None).unwrap();
        assert_eq!(e.title, "Hi");
        assert!(remove_entry_of(&s, &e.id).unwrap().contains("Hi"));
        let p = s.create_project("Desk", "").unwrap();
        let f = fork_journal_of(&s, &j.id, Some(&p.id), None).unwrap();
        assert!(f.name.contains("fork"));
        assert_eq!(f.project_id.as_deref(), Some(p.id.as_str()));
        assert!(remove_journal_of(&s, &j.id).unwrap().contains("Notes"));
    }

    #[test]
    fn writes_need_unlock() {
        crate::api::data::lock();
        assert!(create_journal("X".into(), None, "markdown".into()).is_err());
        assert!(journals().is_err());
        assert!(add_entry_highlight("x".into(), Some("a".into()), None, None, None, None, None).is_err());
        assert!(copy_journal_entries(vec!["x".into()], Some("y".into()), None, None).is_err());
        assert!(export_journal("x".into(), "json".into(), "/tmp/x.json".into(), vec![]).is_err());
    }

    #[test]
    fn highlights_images_copy_and_export() {
        let s = Store::open_in_memory().unwrap();
        let j = create_journal_of(&s, "Notes", None, "markdown").unwrap();
        let e = add_entry_of(&s, &j.id, "Hello", "the quick brown fox", None).unwrap();
        let h = add_highlight_of(&s, &e.id, Some("quick brown"), None, None, None, Some("yellow"), Some("mark")).unwrap();
        assert_eq!(h.kind, "highlight");
        assert_eq!(h.quote.as_deref(), Some("quick brown"));
        assert_eq!(h.color.as_deref(), Some("yellow"));
        assert!(!h.detached);
        let n = add_entry_note_of(&s, &e.id, "a note on fox", Some("fox"), None, None, None).unwrap();
        assert_eq!(n.kind, "note");
        assert_eq!(n.quote.as_deref(), Some("fox"));
        assert_eq!(entry_annotations_of(&s, &e.id).unwrap().len(), 2);
        let dir = tempfile::tempdir().unwrap();
        let img = dir.path().join("dot.png");
        std::fs::write(&img, b"not-a-real-png-but-ok").unwrap();
        let a = attach_image_of(&s, &e.id, img.to_str().unwrap(), None).unwrap();
        assert_eq!((a.name.as_str(), a.mime.as_str(), a.size), ("dot.png", "image/png", 21));
        assert_eq!(entry_images_of(&s, &e.id).unwrap().len(), 1);
        let out = dir.path().join("out.png");
        let msg = save_image_of(&s, &a.id, out.to_str().unwrap()).unwrap();
        assert!(msg.contains("out.png"));
        assert!(!msg.contains("not-a-real-png-but-ok"), "image bytes must not appear in the message");
        assert_eq!(std::fs::read(&out).unwrap(), b"not-a-real-png-but-ok");
        assert!(journal_image_thumbnail_of(&s, &a.id, 32).unwrap().is_empty(), "invalid images have no preview");
        let red = {
            let img = image::RgbImage::from_pixel(8, 8, image::Rgb([200, 30, 30]));
            let mut buf = Vec::new();
            image::DynamicImage::ImageRgb8(img).write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png).unwrap();
            buf
        };
        let red_path = dir.path().join("red.png");
        std::fs::write(&red_path, &red).unwrap();
        let preview = attach_image_of(&s, &e.id, red_path.to_str().unwrap(), Some("red.png")).unwrap();
        let thumb = journal_image_thumbnail_of(&s, &preview.id, 4).unwrap();
        assert!(thumb.starts_with(b"\x89PNG"), "preview is a PNG, got {} bytes", thumb.len());
        assert!(!thumb.is_empty());
        let dest = create_journal_of(&s, "Other", None, "markdown").unwrap();
        let copied = copy_entries_of(&s, &[e.id.clone()], Some(&dest.id), None, None).unwrap();
        assert_eq!(copied.len(), 1);
        assert_eq!(copied[0].journal_id, dest.id);
        assert_eq!(entries_of(&s, &dest.id).unwrap().len(), 1);
        let exp = dir.path().join("notes.json");
        let r = export_journal_of(&s, &j.id, "json", exp.to_str().unwrap(), &[]).unwrap();
        assert!(exp.exists());
        assert!(r.message.contains("json"));
        let text = std::fs::read_to_string(&exp).unwrap();
        assert!(text.contains("Hello") || text.contains("quick brown"));
        let imported = import_journal_of(&s, exp.to_str().unwrap(), None, Some("Imported")).unwrap();
        assert_eq!(imported.name, "Imported");
        let edited = update_annotation_of(&s, &n.id, "updated fox").unwrap();
        assert_eq!(edited.body, "updated fox");
        assert!(remove_annotation_of(&s, &h.id).unwrap().contains("highlight"));
        assert_eq!(entry_annotations_of(&s, &e.id).unwrap().len(), 1);
    }
}
