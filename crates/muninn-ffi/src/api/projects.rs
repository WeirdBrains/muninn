//! Create, edit and remove projects, attach or detach items, and import/export archives.
//! Thin wrappers over `muninn-core`; writes use the unlocked key in Rust.
use super::data::{ProjectSummary, open_write};
use muninn_core::export::archive;
use muninn_core::model::{Duration, Symbol};
use muninn_core::store::{ItemKind, NewAction, SeriesRef, Store};
use std::path::{Path, PathBuf};

fn summarize(p: muninn_core::store::Project) -> ProjectSummary {
    ProjectSummary { short_id: p.short_id().to_string(), id: p.id, name: p.name, description: p.description, updated_at: p.updated_at }
}

fn note(store: &Store, kind: &str, summary: String, params: serde_json::Value) {
    store.note_action(&NewAction::new(kind, "app", summary).params(params));
}

fn params(p: &muninn_core::store::Project) -> serde_json::Value {
    serde_json::json!({ "project": p.id, "name": p.name })
}

pub(crate) fn create_project_of(store: &Store, name: &str, description: &str) -> anyhow::Result<ProjectSummary> {
    let p = store.create_project(name, description)?;
    note(store, "project.create", format!("create project {}", p.name), params(&p));
    Ok(summarize(p))
}

/// `mnnn project create NAME`.
pub fn create_project(name: String, description: String) -> anyhow::Result<ProjectSummary> {
    create_project_of(&open_write()?, &name, &description)
}

pub(crate) fn update_project_of(store: &Store, id: &str, name: Option<&str>, description: Option<&str>) -> anyhow::Result<ProjectSummary> {
    let p = store.update_project(id, name, description)?;
    note(store, "project.edit", format!("edit project {}", p.name), params(&p));
    Ok(summarize(p))
}

/// `mnnn project edit` (rename and/or description).
pub fn update_project(id: String, name: Option<String>, description: Option<String>) -> anyhow::Result<ProjectSummary> {
    update_project_of(&open_write()?, &id, name.as_deref(), description.as_deref())
}

pub(crate) fn remove_project_of(store: &Store, id: &str) -> anyhow::Result<()> {
    let p = store.remove_project(id)?;
    note(store, "project.remove", format!("remove project {}", p.name), params(&p));
    Ok(())
}

/// `mnnn project remove`. Journals stay as standalone journals.
pub fn remove_project(id: String) -> anyhow::Result<()> {
    remove_project_of(&open_write()?, &id)
}

fn series_ref(source: String, base: String, quote: String, duration_s: i64, from: Option<i64>, to: Option<i64>) -> SeriesRef {
    SeriesRef { source, symbol: Symbol::new(&base, &quote), duration: Duration(duration_s), from, to }
}

pub(crate) fn add_project_series_of(store: &Store, project_id: &str, r: SeriesRef) -> anyhow::Result<bool> {
    if !store.series()?.iter().any(|x| x.source == r.source && x.symbol == r.symbol && x.duration == r.duration) {
        anyhow::bail!("no series {} (pick a dataset in Data first)", r.label());
    }
    let (it, new) = store.add_project_result(project_id, &r)?;
    if new {
        note(
            store,
            "project.add",
            format!("add result {} to project", r.label()),
            serde_json::json!({ "project": project_id, "item": it.id, "kind": "result", "ref": it.ref_id }),
        );
    }
    Ok(new)
}

/// Add the selected dataset (with the current time range) as a project result (`a` in the TUI).
/// Returns whether it was newly added (`false` if it was already there).
#[allow(clippy::too_many_arguments)]
pub fn add_project_series(
    project_id: String,
    source: String,
    base: String,
    quote: String,
    duration_s: i64,
    from: Option<i64>,
    to: Option<i64>,
) -> anyhow::Result<bool> {
    add_project_series_of(&open_write()?, &project_id, series_ref(source, base, quote, duration_s, from, to))
}

pub(crate) fn remove_project_series_of(store: &Store, item_id: i64) -> anyhow::Result<()> {
    let it = store.remove_project_item(item_id)?;
    let pname = store.project(&it.project_id)?.map(|p| p.name).unwrap_or_default();
    note(
        store,
        "project.drop",
        format!("drop {} from project {pname}", it.kind.label()),
        serde_json::json!({ "project": it.project_id, "kind": it.kind, "ref": it.ref_id }),
    );
    Ok(())
}

/// Take a series result out of a project (`mnnn project drop`). The series itself stays.
pub fn remove_project_series(item_id: i64) -> anyhow::Result<()> {
    remove_project_series_of(&open_write()?, item_id)
}


/// One item in a project (`mnnn project show`), any kind the store already has.
pub struct ProjectItemInfo {
    pub item_id: i64,
    pub kind: String,
    pub name: String,
    pub detail: String,
    pub ref_id: String,
    pub source: Option<String>,
    pub base: Option<String>,
    pub quote: Option<String>,
    pub duration_s: Option<i64>,
    pub from: Option<i64>,
    pub to: Option<i64>,
}

pub(crate) fn project_items_of(store: &Store, project_id: &str) -> anyhow::Result<Vec<ProjectItemInfo>> {
    let mut out = Vec::new();
    for it in store.project_items(project_id)? {
        let (name, detail) = muninn_core::live::describe_item(store, &it);
        let series = it.series();
        out.push(ProjectItemInfo {
            item_id: it.id,
            kind: it.kind.as_str().into(),
            name,
            detail,
            ref_id: it.ref_id.clone(),
            source: series.as_ref().map(|r| r.source.clone()),
            base: series.as_ref().map(|r| r.symbol.base.clone()),
            quote: series.as_ref().map(|r| r.symbol.quote.clone()),
            duration_s: series.as_ref().map(|r| r.duration.0),
            from: series.as_ref().and_then(|r| r.from),
            to: series.as_ref().and_then(|r| r.to),
        });
    }
    Ok(out)
}

/// `mnnn project show` items (live searches, results, journals, reports, rules, models, code).
pub fn project_items(project_id: String) -> anyhow::Result<Vec<ProjectItemInfo>> {
    project_items_of(&super::data::open()?, &project_id)
}

fn parse_kind(kind: &str) -> anyhow::Result<ItemKind> {
    match kind.trim().to_ascii_lowercase().replace('-', "_").as_str() {
        "search" | "live_search" => Ok(ItemKind::LiveSearch),
        "journal" => Ok(ItemKind::Journal),
        "report" => Ok(ItemKind::Report),
        "rule" | "rule_set" => Ok(ItemKind::RuleSet),
        "model" => Ok(ItemKind::Model),
        "code" => Ok(ItemKind::Code),
        other => Err(anyhow::anyhow!("`{other}`: add a journal, search, report, rule, model or code (or a series with add_project_series)")),
    }
}

pub(crate) fn add_project_item_of(store: &Store, project: &str, kind: ItemKind, key: &str) -> anyhow::Result<String> {
    let p = store.find_project(project)?;
    let (it, new) = match kind {
        ItemKind::LiveSearch => {
            let s = store.find_live_search(key)?;
            store.add_project_item(&p.id, kind, &s.id, serde_json::json!({}))?
        }
        ItemKind::Journal => {
            let j = store.find_journal(key)?;
            store.add_project_item(&p.id, kind, &j.id, serde_json::json!({}))?
        }
        ItemKind::Report => {
            let r = store.find_report(key)?;
            store.add_project_item(&p.id, kind, &r.id, serde_json::json!({}))?
        }
        ItemKind::RuleSet => {
            let r = store.find_rule(key)?;
            store.add_project_item(&p.id, kind, &r.id, serde_json::json!({}))?
        }
        ItemKind::Model => {
            let m = store.find_model(key)?;
            store.add_project_item(&p.id, kind, &m.id, serde_json::json!({}))?
        }
        ItemKind::Code => {
            let c = store.find_code_space(key)?;
            store.add_project_item(&p.id, kind, &c.id, serde_json::json!({}))?
        }
        ItemKind::Result | ItemKind::Pipeline => {
            return Err(anyhow::anyhow!("add a series with add_project_series"));
        }
    };
    let (name, _) = muninn_core::live::describe_item(store, &it);
    if new {
        note(
            store,
            "project.add",
            format!("add {} {name} to project {}", it.kind.label(), p.name),
            serde_json::json!({ "project": p.id, "item": it.id, "kind": it.kind, "ref": it.ref_id }),
        );
        Ok(format!("added {} {name} to {}", it.kind.label(), p.name))
    } else {
        Ok(format!("{name} is already in {}", p.name))
    }
}

/// `mnnn project add --journal|--search|--report|--rule|--model|--code`.
pub fn add_project_item(project: String, kind: String, key: String) -> anyhow::Result<String> {
    add_project_item_of(&open_write()?, &project, parse_kind(&kind)?, key.trim())
}

pub(crate) fn import_project_of(store: &mut Store, path: &Path, name: Option<&str>) -> anyhow::Result<String> {
    let r = archive::import_project(store, path, name)?;
    let Some(p) = r.project.clone() else {
        return Err(anyhow::anyhow!("the file did not contain a project"));
    };
    note(
        store,
        "project.import",
        format!("import project {} from {}", p.name, path.display()),
        serde_json::json!({ "project": p.id, "name": p.name, "file": path.display().to_string() }),
    );
    Ok(format!("imported project {}: {}", p.name, r.counts(true)))
}

/// `mnnn project import` a `.mnnp` archive or JSON pack.
pub fn project_import(path: String, name: Option<String>) -> anyhow::Result<String> {
    let mut store = open_write()?;
    import_project_of(&mut store, Path::new(&path), name.as_deref())
}

fn export_path(out: &str, json: bool) -> PathBuf {
    let p = PathBuf::from(out);
    if p.extension().is_some() {
        return p;
    }
    p.with_extension(if json { "json" } else { "mnnp" })
}

pub(crate) fn export_project_of(store: &Store, id: &str, out: &Path, json: bool, data: bool) -> anyhow::Result<String> {
    let p = store.find_project(id)?;
    let dest = export_path(&out.to_string_lossy(), json);
    if let Some(dir) = dest.parent()
        && !dir.as_os_str().is_empty()
    {
        std::fs::create_dir_all(dir)?;
    }
    let (bytes, w) = if json {
        let (s, w) = archive::project_json(store, &p.id, data)?;
        (s.into_bytes(), w)
    } else {
        archive::project_archive(store, &p.id, data, muninn_core::store::reports::DEFAULT_ROWS)?
    };
    std::fs::write(&dest, bytes)?;
    note(
        store,
        "project.export",
        format!("export project {} to {}", p.name, dest.display()),
        serde_json::json!({ "project": p.id, "name": p.name, "file": dest.display().to_string(), "format": if json { "json" } else { "archive" } }),
    );
    Ok(format!(
        "exported project {} to {} ({} journals, {} entries, {} reports, {} searches, {} series, {} bytes)",
        p.name,
        dest.display(),
        w.journals,
        w.entries,
        w.reports,
        w.live_searches,
        w.series,
        w.bytes
    ))
}

/// `mnnn project export` as an archive (`.mnnp`) or JSON pack. `no_data` leaves bars out.
pub fn project_export(id: String, path: String, format: String, no_data: bool) -> anyhow::Result<String> {
    let json = match format.trim().to_ascii_lowercase().as_str() {
        "" | "archive" | "mnnp" => false,
        "json" => true,
        other => return Err(anyhow::anyhow!("`{other}`: export as archive or json")),
    };
    export_project_of(&open_write()?, &id, Path::new(&path), json, !no_data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::data::{overview_of, project_series_of};
    use muninn_core::model::{Bar, Duration, Symbol};
    use muninn_core::store::Store;

    fn store_with_bars() -> Store {
        let mut s = Store::open_in_memory().unwrap();
        let src = s.add_source("coinbase", "csv", None).unwrap();
        let t0 = 1_704_067_200_000i64;
        let bars = vec![Bar { ts: t0, open: 1.0, high: 2.0, low: 0.5, close: 1.5, volume: 3.0, flags: 0 }];
        s.upsert_bars(&src, &Symbol::new("BTC", "USD"), Duration(300), &bars).unwrap();
        s
    }

    #[test]
    fn create_rename_describe_and_remove() {
        let s = Store::open_in_memory().unwrap();
        assert!(create_project_of(&s, "  ", "").is_err());
        let p = create_project_of(&s, "Alpha", "notes").unwrap();
        assert_eq!((p.name.as_str(), p.description.as_str()), ("Alpha", "notes"));
        assert_eq!(overview_of(&s).unwrap().projects.len(), 1);
        let p = update_project_of(&s, &p.id, Some("Beta"), None).unwrap();
        assert_eq!((p.name.as_str(), p.description.as_str()), ("Beta", "notes"));
        let p = update_project_of(&s, &p.id, None, Some("updated")).unwrap();
        assert_eq!(p.description, "updated");
        remove_project_of(&s, &p.id).unwrap();
        assert!(overview_of(&s).unwrap().projects.is_empty());
    }

    #[test]
    fn attach_and_detach_a_series() {
        let s = store_with_bars();
        let p = create_project_of(&s, "Desk", "").unwrap();
        let r = series_ref("coinbase".into(), "BTC".into(), "USD".into(), 300, Some(1_704_067_200_000), None);
        assert!(add_project_series_of(&s, &p.id, r.clone()).unwrap());
        let again = add_project_series_of(&s, &p.id, r).unwrap();
        assert!(!again, "same result is not duplicated");
        let items = project_series_of(&s, &p.id).unwrap();
        assert_eq!(items.len(), 1);
        assert!(items[0].item_id > 0);
        assert_eq!(items[0].base, "BTC");
        remove_project_series_of(&s, items[0].item_id).unwrap();
        assert!(project_series_of(&s, &p.id).unwrap().is_empty());
    }

    #[test]
    fn add_rejects_a_series_that_is_not_in_the_store() {
        let s = Store::open_in_memory().unwrap();
        let p = create_project_of(&s, "Desk", "").unwrap();
        let err = add_project_series_of(&s, &p.id, series_ref("x".into(), "BTC".into(), "USD".into(), 300, None, None)).unwrap_err();
        assert!(err.to_string().contains("no series"), "{err}");
    }

    #[test]
    fn add_a_journal_and_list_items() {
        let s = Store::open_in_memory().unwrap();
        let p = create_project_of(&s, "Desk", "").unwrap();
        let j = s.create_journal("Notes", Some(&p.id), muninn_core::store::EntryFormat::Markdown).unwrap();
        let items = project_items_of(&s, &p.id).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!((items[0].kind.as_str(), items[0].name.as_str()), ("journal", "Notes"));
        let msg = add_project_item_of(&s, &p.id, muninn_core::store::ItemKind::Journal, &j.id).unwrap();
        assert!(msg.contains("already"), "{msg}");
    }

    #[test]
    fn export_and_import_a_json_pack() {
        let s = Store::open_in_memory().unwrap();
        let p = create_project_of(&s, "Desk", "notes").unwrap();
        s.create_journal("Field", Some(&p.id), muninn_core::store::EntryFormat::Markdown).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("desk.json");
        let msg = export_project_of(&s, &p.id, &out, true, false).unwrap();
        assert!(msg.contains("exported project Desk"), "{msg}");
        assert!(out.is_file());
        let mut s2 = Store::open_in_memory().unwrap();
        let imported = import_project_of(&mut s2, &out, None).unwrap();
        assert!(imported.contains("imported project Desk"), "{imported}");
        assert_eq!(s2.projects().unwrap().len(), 1);
        assert_eq!(s2.journals().unwrap()[0].name, "Field");
    }

    #[test]
    fn writes_need_unlock() {
        assert!(create_project("X".into(), "".into()).is_err());
        assert!(remove_project("x".into()).is_err());
        assert!(remove_project_series(1).is_err());
        assert!(project_import("/no/such.mnnp".into(), None).is_err());
        assert!(add_project_item("x".into(), "journal".into(), "n".into()).is_err());
    }
}
