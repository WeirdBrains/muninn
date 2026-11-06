//! In-app help: the same embedded `docs/` topics as `mnnn help` and the TUI.
//! Documentation only; no secrets.
use muninn_core::help::{self, Topic};

/// A topic in the index or search results (no body).
pub struct HelpTopicInfo {
    pub id: String,
    pub title: String,
}

/// One help page, Markdown body included.
pub struct HelpTopic {
    pub id: String,
    pub title: String,
    pub body: String,
}

fn info(t: &Topic) -> HelpTopicInfo {
    HelpTopicInfo { id: t.id.to_string(), title: t.title.clone() }
}

fn full(t: &Topic) -> HelpTopic {
    HelpTopic { id: t.id.to_string(), title: t.title.clone(), body: t.body.to_string() }
}

/// Every embedded topic (`mnnn help` with no argument).
pub fn help_topics() -> Vec<HelpTopicInfo> {
    help::topics().iter().map(info).collect()
}

/// One topic by id or title (`mnnn help keys`). `index` and an empty id are README.
pub fn help_topic(id: String) -> anyhow::Result<HelpTopic> {
    help::topic(&id).map(|t| full(&t)).ok_or_else(|| anyhow::anyhow!("unknown help topic `{id}`"))
}

/// Search titles and bodies (substring, then fuzzy titles), same ranking as the TUI.
pub fn help_search(query: String) -> Vec<HelpTopicInfo> {
    help::search(&query).iter().map(info).collect()
}

/// Resolve a Markdown href from topic `from` to another topic id, or `None` for a web/mail link.
pub fn help_resolve_href(from: String, href: String) -> Option<String> {
    help::resolve_href(&from, &href)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_topics_exist() {
        let ids: Vec<String> = help_topics().into_iter().map(|t| t.id).collect();
        for id in ["README", "help", "keybindings", "keys", "flutter-app", "tui", "charts"] {
            assert!(ids.iter().any(|x| x == id), "missing {id}");
        }
    }

    #[test]
    fn topic_body_is_the_doc() {
        let t = help_topic("keybindings".into()).unwrap();
        assert_eq!(t.id, "keybindings");
        assert!(t.title.to_lowercase().contains("key"), "{}", t.title);
        assert!(t.body.contains("F5") && t.body.contains("F1"), "expected TUI key reference");
        assert!(help_topic("no-such-topic".into()).is_err());
    }

    #[test]
    fn search_finds_shards_and_empty_lists_all() {
        let r = help_search("shard".into());
        assert!(!r.is_empty());
        assert!(r.iter().any(|t| t.id == "dark-shards" || t.id == "keys"), "{:?}", r.iter().map(|t| &t.id).collect::<Vec<_>>());
        assert_eq!(help_search(String::new()).len(), help_topics().len());
    }

    #[test]
    fn href_resolution_matches_the_core() {
        assert_eq!(help_resolve_href("keys".into(), "#rotate".into()).as_deref(), Some("keys"));
        assert_eq!(help_resolve_href("services/http".into(), "../keys.md".into()).as_deref(), Some("keys"));
        assert_eq!(help_resolve_href("keys".into(), "https://age-encryption.org".into()), None);
    }
}
