//! Notifications (2.18): which events notify you, and the links they carry. Built-in
//! events (a review comment from someone else, a connection request, an application to
//! your org, sync conflicts, org code waiting for approval, certificate warnings, other
//! nodes' broadcasts) make a notification as they are recorded; rule sets add their own
//! with the `notify` then-action. Every new notification is itself recorded as a
//! `notification` action, so rule sets can react to it. See `docs/notifications.md`.
use crate::error::Result;
use crate::store::{ActionRecord, NewAction, NewNotification, Notification, Store};
use serde_json::{Value, json};

/// The setting holding muted kinds (a JSON list).
pub const MUTED: &str = "notify.muted";

/// One built-in source of notifications.
pub struct Builtin {
    pub kind: &'static str,
    pub about: &'static str,
}

pub const BUILTINS: &[Builtin] = &[
    Builtin { kind: "net.exchange.incoming", about: "another node asks to connect (or invites you)" },
    Builtin { kind: "org.member.apply", about: "someone applied to join an org of yours" },
    Builtin { kind: "review.comment", about: "a reviewer commented on a review of yours" },
    Builtin { kind: "review.suggest", about: "a reviewer suggested a change" },
    Builtin { kind: "review.vote", about: "a reviewer voted" },
    Builtin { kind: "sync.pull", about: "a sync with a linked node kept conflicts" },
    Builtin { kind: "org.code.sync", about: "an org's code arrived and waits for your approval" },
    Builtin { kind: "notify", about: "a warning from Muninn (certificates expiring or failing to renew)" },
    Builtin { kind: "net.broadcast", about: "a broadcast from another node" },
];

fn s(v: &Value, k: &str) -> String {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
}

/// What a recorded action notifies, if anything.
pub fn builtin_for(a: &ActionRecord) -> Option<NewNotification> {
    let p = &a.params;
    let net = a.origin == "network";
    let n = |level: &str, title: String, body: String, link: Option<String>| NewNotification {
        kind: a.kind.clone(),
        level: level.into(),
        title,
        body,
        link,
        action_id: Some(a.id),
    };
    match a.kind.as_str() {
        "net.exchange.incoming" if net => {
            Some(n("info", a.summary.clone(), "accept or decline it in Network ▸ Requests (mnnn net requests)".into(), Some("requests".into())))
        }
        "org.member.apply" if net => Some(n("info", a.summary.clone(), String::new(), Some(format!("org:{}", s(p, "org"))))),
        "review.comment" | "review.suggest" | "review.vote" if net => {
            Some(n("info", a.summary.clone(), String::new(), Some(format!("review:{}", s(p, "review")))))
        }
        "sync.pull" if p.get("conflicts").and_then(|c| c.as_i64()).unwrap_or(0) > 0 => {
            Some(n("warn", a.summary.clone(), "mnnn sync conflicts lists them".into(), Some("sync".into())))
        }
        "org.code.sync" if p.get("spaces").and_then(|c| c.as_i64()).unwrap_or(0) > 0 => {
            Some(n("info", a.summary.clone(), "approve each code space before it runs (Code ▸ a, mnnn code approve)".into(), Some("code".into())))
        }
        "notify" => {
            let level = match s(p, "level").as_str() {
                l @ ("warn" | "error") => l.to_string(),
                _ => "info".into(),
            };
            let msg = s(p, "message");
            let org = s(p, "org");
            Some(n(&level, if msg.is_empty() { a.summary.clone() } else { msg }, String::new(), (!org.is_empty()).then(|| format!("org:{org}"))))
        }
        "net.broadcast" if net => {
            Some(n("info", format!("{}: {}", s(p, "from"), s(p, "topic")), s(p, "text"), Some(format!("peer:{}", s(p, "peer")))))
        }
        _ => None,
    }
}

pub fn muted(store: &Store) -> Vec<String> {
    store.setting(MUTED).ok().flatten().and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default()
}

/// Mute (or unmute) a built-in kind; `true` when it changed.
pub fn set_muted(store: &Store, kind: &str, mute: bool) -> Result<bool> {
    if !BUILTINS.iter().any(|b| b.kind == kind) {
        return Err(crate::Error::invalid("kind", format!("`{kind}` (one of {})", BUILTINS.iter().map(|b| b.kind).collect::<Vec<_>>().join(", "))));
    }
    let mut v = muted(store);
    let had = v.iter().any(|k| k == kind);
    if mute == had {
        return Ok(false);
    }
    if mute {
        v.push(kind.into());
    } else {
        v.retain(|k| k != kind);
    }
    let j = serde_json::to_string(&v)?;
    store.set_setting(MUTED, if v.is_empty() { None } else { Some(&j) })?;
    Ok(true)
}

/// Make a notification and record it (rule sets can react to `notification`).
pub fn add(store: &Store, origin: &str, n: &NewNotification) -> Result<Notification> {
    let made = store.add_notification(n)?;
    store.note_action(&NewAction::new("notification", origin, format!("notification: {}", made.title)).params(json!({
        "id": made.id,
        "source": made.kind,
        "level": made.level,
        "title": made.title,
        "body": made.body,
        "link": made.link,
    })));
    Ok(made)
}

/// Called for every recorded action (before rules run on it).
pub(crate) fn after_action(store: &Store, a: &ActionRecord) {
    let Some(n) = builtin_for(a) else { return };
    if muted(store).contains(&n.kind) {
        return;
    }
    if let Err(e) = add(store, &a.origin, &n) {
        tracing::warn!("could not add a notification for `{}`: {e}", a.kind);
    }
}

/// Where a notification's link leads, in words.
pub fn describe_link(store: &Store, link: &str) -> String {
    let (k, id) = link.split_once(':').unwrap_or((link, ""));
    match k {
        "review" => store.review(id).ok().flatten().map(|r| format!("review {}", r.title())).unwrap_or_else(|| format!("review {id}")),
        "org" => store.org(id).ok().flatten().map(|o| format!("org {}", o.name)).unwrap_or_else(|| format!("org {id}")),
        "peer" => format!("node {}", &id[id.len().saturating_sub(8)..]),
        "requests" => "network requests".into(),
        "sync" => "sync conflicts".into(),
        "code" => "code spaces".into(),
        "journal" => store.journal(id).ok().flatten().map(|j| format!("journal {}", j.name)).unwrap_or_else(|| format!("journal {id}")),
        "project" => store.project(id).ok().flatten().map(|p| format!("project {}", p.name)).unwrap_or_else(|| format!("project {id}")),
        "rule" => store.rule(id).ok().flatten().map(|r| format!("rule {}", r.name)).unwrap_or_else(|| format!("rule {id}")),
        "search" => store.live_search(id).ok().flatten().map(|s| format!("live search {}", s.name)).unwrap_or_else(|| format!("live search {id}")),
        "action" => format!("history #{id}"),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_events_notify_unless_muted() {
        let s = Store::open_in_memory().unwrap();
        s.note_action(&NewAction::new("org.member.apply", "network", "ben applied to join Lab").params(json!({"org": "o1"})));
        // your own actions do not notify you
        s.note_action(&NewAction::new("review.comment", "cli", "you commented").params(json!({"review": "r1"})));
        s.note_action(&NewAction::new("sync.pull", "network", "synced").params(json!({"conflicts": 0})));
        let v = s.notifications(false, 10).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!((v[0].title.as_str(), v[0].link.as_deref()), ("ben applied to join Lab", Some("org:o1")));
        // the notification is itself an action rules can react to
        let a = s.actions_of_kind("notification").unwrap();
        assert_eq!(a[0].params["source"], "org.member.apply");
        assert!(set_muted(&s, "org.member.apply", true).unwrap() && !set_muted(&s, "org.member.apply", true).unwrap());
        assert!(set_muted(&s, "nonsense", true).is_err());
        s.note_action(&NewAction::new("org.member.apply", "network", "x").params(json!({"org": "o1"})));
        assert_eq!(s.notifications(false, 10).unwrap().len(), 1);
        s.note_action(&NewAction::new("notify", "rule", "cert").params(json!({"level": "warn", "message": "expires in 3 days", "org": "o2"})));
        let v = s.notifications(true, 10).unwrap();
        assert_eq!((v[0].level.as_str(), v[0].title.as_str()), ("warn", "expires in 3 days"));
    }
}
