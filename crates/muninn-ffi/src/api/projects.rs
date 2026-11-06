//! Create, edit and remove projects, and attach or detach series results.
//! Thin wrappers over `muninn-core`; writes use the unlocked key in Rust.
use super::data::{ProjectSummary, open_write};
use muninn_core::model::{Duration, Symbol};
use muninn_core::store::{NewAction, SeriesRef, Store};

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
    fn writes_need_unlock() {
        assert!(create_project("X".into(), "".into()).is_err());
        assert!(remove_project("x".into()).is_err());
        assert!(remove_project_series(1).is_err());
    }
}
