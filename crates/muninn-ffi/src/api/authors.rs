//! File ▸ Authors… (TUI `A`): list, add, edit, delete and choose the current author.
use super::data::{open, open_write};
use muninn_core::store::{Author, AuthorChange, NewAction, Store};

#[derive(Debug)]
pub struct AuthorInfo {
    pub id: String,
    pub short_id: String,
    pub name: String,
    pub email: String,
    pub details: Vec<String>,
    pub current: bool,
}

fn info(a: &Author, current: Option<&str>) -> AuthorInfo {
    AuthorInfo {
        short_id: a.short_id().to_string(),
        email: a.email.clone().unwrap_or_default(),
        details: a.details.iter().map(|(k, v)| format!("{k}={v}")).collect(),
        current: current == Some(a.id.as_str()),
        id: a.id.clone(),
        name: a.name.clone(),
    }
}

pub(crate) fn authors_of(store: &Store) -> anyhow::Result<Vec<AuthorInfo>> {
    let cur = store.current_author_id()?;
    Ok(store.authors()?.iter().map(|a| info(a, cur.as_deref())).collect())
}

/// `mnnn author list` / TUI Authors….
pub fn authors() -> anyhow::Result<Vec<AuthorInfo>> {
    authors_of(&open()?)
}

pub(crate) fn add_author_of(store: &Store, name: &str, email: Option<&str>, details: &[String]) -> anyhow::Result<AuthorInfo> {
    let parsed = muninn_core::store::authors::parse_details(details)?;
    let a = store.add_author(name.trim(), email.map(str::trim).filter(|s| !s.is_empty()), &parsed)?;
    if store.current_author_id()?.is_none() {
        store.set_current_author(Some(&a.id))?;
    }
    store.note_action(&NewAction::new("author.add", "app", format!("add author {}", a.name)).params(serde_json::json!({ "author": a.id })));
    let cur = store.current_author_id()?;
    Ok(info(&a, cur.as_deref()))
}

/// `mnnn author add` / TUI `a`. The first author becomes current.
pub fn add_author(name: String, email: Option<String>, details: Vec<String>) -> anyhow::Result<AuthorInfo> {
    add_author_of(&open_write()?, &name, email.as_deref(), &details)
}

pub(crate) fn update_author_of(
    store: &Store,
    id: &str,
    name: Option<&str>,
    email: Option<&str>,
    details: &[String],
    replace_details: bool,
) -> anyhow::Result<AuthorInfo> {
    let a = store.find_author(id.trim())?;
    let parsed = muninn_core::store::authors::parse_details(details)?;
    let c = AuthorChange {
        name: name.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string),
        email: email.map(|s| s.trim().to_string()),
        details: parsed,
        replace_details,
    };
    let a = store.update_author(&a.id, &c)?;
    store.note_action(&NewAction::new("author.edit", "app", format!("edit author {}", a.name)).params(serde_json::json!({ "author": a.id })));
    let cur = store.current_author_id()?;
    Ok(info(&a, cur.as_deref()))
}

/// `mnnn author edit`.
pub fn update_author(
    id: String,
    name: Option<String>,
    email: Option<String>,
    details: Vec<String>,
    replace_details: bool,
) -> anyhow::Result<AuthorInfo> {
    update_author_of(&open_write()?, &id, name.as_deref(), email.as_deref(), &details, replace_details)
}

pub(crate) fn remove_author_of(store: &Store, id: &str) -> anyhow::Result<String> {
    let a = store.remove_author(id.trim())?;
    store.note_action(&NewAction::new("author.remove", "app", format!("remove author {}", a.name)).params(serde_json::json!({ "author": a.id })));
    Ok(format!("removed {}", a.name))
}

/// `mnnn author remove` / TUI `d`.
pub fn remove_author(id: String) -> anyhow::Result<String> {
    remove_author_of(&open_write()?, &id)
}

pub(crate) fn set_current_author_of(store: &Store, id: Option<&str>) -> anyhow::Result<String> {
    store.set_current_author(id.map(str::trim).filter(|s| !s.is_empty()))?;
    match store.current_author()? {
        Some(a) => {
            store.note_action(
                &NewAction::new("author.current", "app", format!("current author {}", a.name)).params(serde_json::json!({ "author": a.id })),
            );
            Ok(format!("{} is current", a.name))
        }
        None => Ok("no current author".into()),
    }
}

/// `mnnn author current` / TUI Enter. Empty id clears it.
pub fn set_current_author(id: Option<String>) -> anyhow::Result<String> {
    set_current_author_of(&open_write()?, id.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use muninn_core::store::Store;

    #[test]
    fn add_list_current_edit_and_remove() {
        let s = Store::open_in_memory().unwrap();
        assert!(authors_of(&s).unwrap().is_empty());
        let a = add_author_of(&s, "Ada", Some("ada@example"), &["affiliation=Lab".into()]).unwrap();
        assert_eq!(a.name, "Ada");
        assert_eq!(a.email, "ada@example");
        assert!(a.current, "the first author becomes current");
        assert!(a.details.iter().any(|d| d.contains("affiliation=Lab")));
        let list = authors_of(&s).unwrap();
        assert_eq!(list.len(), 1);
        let b = add_author_of(&s, "Ben", None, &[]).unwrap();
        assert!(!b.current);
        let msg = set_current_author_of(&s, Some(&b.id)).unwrap();
        assert!(msg.contains("Ben"));
        let edited = update_author_of(&s, &a.id, Some("Ada Lovelace"), Some(""), &["role=writer".into()], true).unwrap();
        assert_eq!(edited.name, "Ada Lovelace");
        assert!(edited.email.is_empty());
        assert_eq!(edited.details, vec!["role=writer".to_string()]);
        assert!(remove_author_of(&s, &a.id).unwrap().contains("Ada Lovelace"));
        assert_eq!(authors_of(&s).unwrap().len(), 1);
    }
}
