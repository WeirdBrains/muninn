//! Live searches (`mnnn search` / TUI Live searches): save, rename, fork and remove.
use super::data::open_write;
use muninn_core::store::{LiveSearch, NewAction, SearchSpec, SearchTarget, Store};

pub struct SearchInfo {
    pub id: String,
    pub short_id: String,
    pub name: String,
    pub target: String,
    pub query: String,
    pub describe: String,
    pub matches: i64,
    pub forked_from: Option<String>,
}

fn info(store: &Store, s: LiveSearch) -> SearchInfo {
    SearchInfo {
        short_id: s.short_id().to_string(),
        describe: s.spec.describe(),
        target: s.spec.target.label().into(),
        query: s.spec.query.clone(),
        matches: muninn_core::live::count(store, &s.spec) as i64,
        id: s.id,
        name: s.name,
        forked_from: s.forked_from,
    }
}

fn note(store: &Store, kind: &str, summary: String, params: serde_json::Value) {
    store.note_action(&NewAction::new(kind, "app", summary).params(params));
}

fn parse_target(t: &str) -> anyhow::Result<SearchTarget> {
    match t.trim().to_ascii_lowercase().as_str() {
        "" | "series" => Ok(SearchTarget::Series),
        "journals" | "journal" => Ok(SearchTarget::Journals),
        "records" | "record" => Ok(SearchTarget::Records),
        other => Err(anyhow::anyhow!("`{other}`: search series, journals or records")),
    }
}

pub(crate) fn searches_of(store: &Store) -> anyhow::Result<Vec<SearchInfo>> {
    Ok(store.live_searches()?.into_iter().map(|s| info(store, s)).collect())
}

/// `mnnn search list`.
pub fn searches() -> anyhow::Result<Vec<SearchInfo>> {
    searches_of(&super::data::open()?)
}

pub(crate) fn save_search_of(store: &Store, name: &str, query: &str, target: &str) -> anyhow::Result<SearchInfo> {
    let spec = SearchSpec { target: parse_target(target)?, query: query.to_string(), ..Default::default() };
    let s = store.save_live_search(name, &spec, None)?;
    note(store, "search.save", format!("save live search {}", s.name), serde_json::json!({ "search": s.id, "name": s.name }));
    Ok(info(store, s))
}

/// `mnnn search save NAME --query …` (`--journals` / `--records` via `target`).
pub fn save_search(name: String, query: String, target: String) -> anyhow::Result<SearchInfo> {
    save_search_of(&open_write()?, &name, &query, &target)
}

pub(crate) fn rename_search_of(store: &Store, id: &str, name: &str) -> anyhow::Result<SearchInfo> {
    let s = store.find_live_search(id)?;
    let n = store.update_live_search(&s.id, Some(name), None)?;
    note(store, "search.edit", format!("rename live search {} → {}", s.name, n.name), serde_json::json!({ "search": n.id, "name": n.name }));
    Ok(info(store, n))
}

pub fn rename_search(id: String, name: String) -> anyhow::Result<SearchInfo> {
    rename_search_of(&open_write()?, &id, &name)
}

pub(crate) fn fork_search_of(store: &Store, id: &str, name: Option<&str>) -> anyhow::Result<SearchInfo> {
    let s = store.find_live_search(id)?;
    let n = store.fork_live_search(&s.id, name.map(str::trim).filter(|x| !x.is_empty()), None)?;
    note(store, "search.fork", format!("fork live search {} → {}", s.name, n.name), serde_json::json!({ "search": n.id, "name": n.name }));
    Ok(info(store, n))
}

pub fn fork_search(id: String, name: Option<String>) -> anyhow::Result<SearchInfo> {
    fork_search_of(&open_write()?, &id, name.as_deref())
}

pub(crate) fn remove_search_of(store: &Store, id: &str) -> anyhow::Result<String> {
    let s = store.find_live_search(id)?;
    let name = s.name.clone();
    store.remove_live_search(&s.id)?;
    note(store, "search.remove", format!("remove live search {name}"), serde_json::json!({ "search": s.id, "name": name }));
    Ok(format!("removed live search {name}"))
}

pub fn remove_search(id: String) -> anyhow::Result<String> {
    remove_search_of(&open_write()?, &id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use muninn_core::store::Store;

    #[test]
    fn save_rename_fork_remove() {
        let s = Store::open_in_memory().unwrap();
        let a = save_search_of(&s, "BTC", "btc", "series").unwrap();
        assert_eq!((a.name.as_str(), a.target.as_str(), a.query.as_str()), ("BTC", "series", "btc"));
        assert!(a.describe.contains("btc"));
        let a = rename_search_of(&s, &a.id, "Bitcoin").unwrap();
        assert_eq!(a.name, "Bitcoin");
        let f = fork_search_of(&s, &a.id, None).unwrap();
        assert!(f.name.contains("fork"));
        assert_eq!(searches_of(&s).unwrap().len(), 2);
        assert!(remove_search_of(&s, &a.id).unwrap().contains("Bitcoin"));
        assert_eq!(searches_of(&s).unwrap().len(), 1);
    }

    #[test]
    fn writes_need_unlock() {
        crate::api::data::lock();
        assert!(save_search("X".into(), "q".into(), "series".into()).is_err());
        assert!(searches().is_err());
    }
}
