//! Help topics built from `docs/` (embedded at build time), with search, link resolution and rendering.
mod view;
pub use view::HelpView;

include!(concat!(env!("OUT_DIR"), "/help_topics.rs"));

#[derive(Debug, Clone, PartialEq)]
pub struct Topic {
    pub id: &'static str,
    pub title: String,
    pub body: &'static str,
}

pub fn topics() -> Vec<Topic> {
    TOPICS
        .iter()
        .map(|(id, body)| Topic {
            id,
            title: body.lines().find_map(|l| l.strip_prefix("# ")).map(|s| s.trim().to_string()).unwrap_or_else(|| id.to_string()),
            body,
        })
        .collect()
}

pub fn topic(id: &str) -> Option<Topic> {
    let id = id.trim().trim_end_matches(".md");
    let id = if id.is_empty() || id == "index" { "README" } else { id };
    topics().into_iter().find(|t| t.id.eq_ignore_ascii_case(id) || t.title.eq_ignore_ascii_case(id))
}

/// A link inside a topic.
#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub text: String,
    pub href: String,
    /// Internal topic id, if the link points at another doc.
    pub target: Option<String>,
}

/// Resolve a relative markdown href (`../keys.md#rotate`) from topic `from` to a topic id.
/// Returns None for external links (`http:`, `mailto:` ...).
pub fn resolve_href(from: &str, href: &str) -> Option<String> {
    if href.contains("://") || href.starts_with("mailto:") {
        return None;
    }
    let path = href.split('#').next().unwrap_or("");
    if path.is_empty() {
        return Some(from.to_string());
    }
    // Files other than docs (images such as ../site/web/screenshots/x.png) are not topics.
    let file = path.rsplit('/').next().unwrap_or("");
    if file.rsplit_once('.').is_some_and(|(_, ext)| ext != "md") {
        return None;
    }
    let path = path.trim_end_matches(".md");
    let mut parts: Vec<&str> = from.split('/').collect();
    parts.pop(); // directory of `from`
    for seg in path.split('/') {
        match seg {
            "." | "" => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    Some(parts.join("/"))
}

/// Parse `[text](href)` links from markdown (outside code).
pub fn links(id: &str, body: &str) -> Vec<Link> {
    let mut out = vec![];
    let mut in_code = false;
    for line in body.lines() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }
        for (text, href) in inline_links(line) {
            out.push(Link { target: resolve_href(id, &href), text, href });
        }
    }
    out
}

/// `[text](href)` pairs in one line, skipping inline code spans.
pub fn inline_links(line: &str) -> Vec<(String, String)> {
    let mut out = vec![];
    let b: Vec<char> = line.chars().collect();
    let mut i = 0;
    let mut code = false;
    while i < b.len() {
        if b[i] == '`' {
            code = !code;
        } else if !code
            && b[i] == '['
            && let Some(close) = (i + 1..b.len()).find(|&j| b[j] == ']')
            && close + 1 < b.len()
            && b[close + 1] == '('
            && let Some(end) = (close + 2..b.len()).find(|&j| b[j] == ')')
        {
            let text: String = b[i + 1..close].iter().collect();
            let href: String = b[close + 2..end].iter().collect();
            out.push((text, href));
            i = end;
        }
        i += 1;
    }
    out
}

/// Search titles and bodies. Substring matches rank above fuzzy (subsequence) title matches.
pub fn search(query: &str) -> Vec<Topic> {
    let q = query.trim().to_lowercase();
    let all = topics();
    if q.is_empty() {
        return all;
    }
    let mut scored: Vec<(i64, Topic)> = all
        .into_iter()
        .filter_map(|t| {
            let title = t.title.to_lowercase();
            let id = t.id.to_lowercase();
            let body = t.body.to_lowercase();
            let mut s = 0i64;
            if title == q || id == q {
                s += 1000;
            }
            if title.contains(&q) || id.contains(&q) {
                s += 500;
            }
            let hits = body.matches(&q).count() as i64;
            s += hits.min(50) * 5;
            if s == 0 && (subsequence(&q, &title) || subsequence(&q, &id)) {
                s += 50;
            }
            (s > 0).then_some((s, t))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.id.cmp(b.1.id)));
    scored.into_iter().map(|(_, t)| t).collect()
}

fn subsequence(q: &str, s: &str) -> bool {
    let mut it = s.chars();
    q.chars().filter(|c| !c.is_whitespace()).all(|c| it.any(|x| x == c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_links_are_not_topics() {
        assert_eq!(resolve_href("announcement-2.0", "../site/web/screenshots/home.png"), None);
        assert_eq!(resolve_href("announcement-2.0", "install.md#debian"), Some("install".into()));
        assert_eq!(resolve_href("announcement-2.0", "network"), Some("network".into()));
    }

    #[test]
    fn every_internal_link_resolves() {
        let ids: Vec<&str> = TOPICS.iter().map(|(id, _)| *id).collect();
        let mut broken = vec![];
        for (id, body) in TOPICS {
            for l in links(id, body) {
                if let Some(t) = &l.target
                    && !ids.contains(&t.as_str())
                {
                    broken.push(format!("{id}: [{}]({}) -> {t}", l.text, l.href));
                }
            }
        }
        assert!(broken.is_empty(), "broken help links:\n{}", broken.join("\n"));
    }

    #[test]
    fn required_topics_exist() {
        for id in [
            "README",
            "cli",
            "organizations",
            "reviews",
            "share",
            "org-hosting",
            "ingest-csv",
            "ingest-json",
            "services/http",
            "services/redis",
            "services/sqlite",
            "services/postgres",
            "data-model",
            "storage-layout",
            "export",
            "keys",
            "encryption",
            "backup-recovery",
            "themes",
            "preferences",
            "tui",
            "help",
            "menus",
            "keybindings",
            "tiling",
            "charts",
            "sidebar",
            "import",
            "rules",
            "models",
            "model-templates",
            "code-spaces",
            "pipes",
            "pipelines",
            "network",
            "key-exchanges",
            "dark-shards",
            "users",
            "sync",
        ] {
            assert!(topic(id).is_some(), "missing docs/{id}.md");
        }
    }

    #[test]
    fn href_resolution() {
        assert_eq!(resolve_href("services/http", "../keys.md#rotate").as_deref(), Some("keys"));
        assert_eq!(resolve_href("services/http", "redis.md").as_deref(), Some("services/redis"));
        assert_eq!(resolve_href("README", "services/http.md").as_deref(), Some("services/http"));
        assert_eq!(resolve_href("keys", "#rotate").as_deref(), Some("keys"));
        assert_eq!(resolve_href("keys", "https://age-encryption.org"), None);
    }

    #[test]
    fn search_ranks_and_fuzzy() {
        let r = search("shard");
        assert_eq!(r[0].id, "dark-shards", "{:?}", r.iter().map(|t| t.id).collect::<Vec<_>>());
        assert!(r.iter().take(2).any(|t| t.id == "keys"));
        assert!(search("postgres").iter().take(2).any(|t| t.id == "services/postgres"));
        assert!(search("zzzzqqq").is_empty());
        assert!(search("bckprcv").iter().any(|t| t.id == "backup-recovery"));
        assert_eq!(search("").len(), TOPICS.len());
    }

    #[test]
    fn inline_link_parsing_skips_code() {
        let l = inline_links("see [keys](keys.md) and `[not](a.md)` or [x](y.md#z)");
        assert_eq!(l, vec![("keys".into(), "keys.md".into()), ("x".into(), "y.md#z".into())]);
    }
}
