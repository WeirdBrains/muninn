//! Journals (`mnnn journal` / TUI Journals): list, create, entries and notes.
//! Image bytes and highlight spans stay in Rust; Dart sees names and text.
use super::data::open_write;
use muninn_core::store::{EntryFormat, Journal, NewAction, Store};

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

pub(crate) fn update_entry_of(store: &Store, entry: &str, title: Option<&str>, body: Option<&str>, format: Option<&str>) -> anyhow::Result<EntryInfo> {
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
    }
}
