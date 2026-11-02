//! Broadcasts (2.18): a short topic and text sent to linked nodes, your connections or an
//! org's member nodes. Each one arrives as a `net.broadcast` action, which notifies (unless
//! muted) and which rule sets can trigger on. A rule set's `broadcast` then-action queues
//! here (the outbox, node-local) and the running node sends it; the CLI sends at once.
use crate::error::{Error, Result};
use crate::net::exchange::scope_kind;
use crate::store::{Exchange, NewAction, Store};
use serde::{Deserialize, Serialize};
use serde_json::json;

pub const MAX_TOPIC: usize = 64;
pub const MAX_TEXT: usize = 2000;
/// Queued broadcasts (node-local: settings are not synced).
pub const OUTBOX: &str = "broadcast.outbox";
const MAX_QUEUED: usize = 100;

/// Who hears a broadcast.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", tag = "kind", content = "org")]
pub enum To {
    /// Your linked nodes.
    Nodes,
    /// Nodes you exchanged keys with (node scope).
    Connections,
    /// The member nodes of an org (org id).
    Org(String),
    /// All of the above.
    All,
}

impl To {
    /// `nodes`, `connections`, `all` or `org:SLUG`.
    pub fn parse(s: &Store, v: &str) -> Result<To> {
        Ok(match v.trim() {
            "nodes" | "linked" | "" => To::Nodes,
            "connections" | "peers" => To::Connections,
            "all" => To::All,
            o => match o.strip_prefix("org:") {
                Some(k) => To::Org(s.find_org(k)?.id),
                None => return Err(Error::invalid("to", format!("`{o}` (nodes, connections, all or org:SLUG)"))),
            },
        })
    }
    pub fn text(&self, s: &Store) -> String {
        match self {
            To::Nodes => "nodes".into(),
            To::Connections => "connections".into(),
            To::All => "all".into(),
            To::Org(id) => format!("org:{}", s.org(id).ok().flatten().map(|o| o.slug).unwrap_or_else(|| id.clone())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Queued {
    pub to: To,
    pub topic: String,
    pub text: String,
    pub at: i64,
}

fn hears(x: &Exchange, to: &To) -> bool {
    let org = |id: &str| scope_kind(&x.scope) == "organization" && x.scope.get("org").and_then(|v| v.as_str()) == Some(id);
    match to {
        To::Nodes => scope_kind(&x.scope) == "link",
        To::Connections => scope_kind(&x.scope) == "node",
        To::Org(id) => org(id),
        To::All => matches!(scope_kind(&x.scope), "link" | "node" | "organization"),
    }
}

/// The active exchanges a broadcast goes over.
pub fn targets(s: &Store, to: &To) -> Result<Vec<Exchange>> {
    Ok(s.exchanges()?.into_iter().filter(|x| x.status.active() && x.peer_id.is_some() && hears(x, to)).collect())
}

fn check(topic: &str, text: &str) -> Result<()> {
    if topic.trim().is_empty() || topic.chars().count() > MAX_TOPIC {
        return Err(Error::invalid("topic", format!("1 to {MAX_TOPIC} characters")));
    }
    if text.chars().count() > MAX_TEXT {
        return Err(Error::invalid("text", format!("at most {MAX_TEXT} characters")));
    }
    Ok(())
}

/// Queue one for the running node.
pub fn queue(s: &Store, to: To, topic: &str, text: &str) -> Result<()> {
    check(topic, text)?;
    let mut q = queued(s);
    if q.len() >= MAX_QUEUED {
        return Err(Error::Other(format!("{MAX_QUEUED} broadcasts are already waiting for the node to send them")));
    }
    q.push(Queued { to, topic: topic.trim().into(), text: text.into(), at: crate::store::now_ms() });
    s.set_setting(OUTBOX, Some(&serde_json::to_string(&q)?))
}

pub fn queued(s: &Store) -> Vec<Queued> {
    s.setting(OUTBOX).ok().flatten().and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default()
}

/// How a broadcast went.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Report {
    pub sent: Vec<String>,
    pub failed: Vec<(String, String)>,
}

/// Send through `send` (one call per exchange: the network, or a test) and record it.
pub fn send_with(
    s: &Store,
    to: &To,
    topic: &str,
    text: &str,
    origin: &str,
    mut send: impl FnMut(&Exchange, &str, &str) -> Result<()>,
) -> Result<Report> {
    check(topic, text)?;
    let mut r = Report::default();
    for x in targets(s, to)? {
        match send(&x, topic, text) {
            Ok(()) => r.sent.push(x.title()),
            Err(e) => r.failed.push((x.title(), e.to_string())),
        }
    }
    s.note_action(
        &NewAction::new("net.broadcast.send", origin, format!("broadcast {topic} to {} node(s)", r.sent.len()))
            .params(json!({ "to": to.text(s), "topic": topic, "text": text, "sent": r.sent.len(), "failed": r.failed.len() })),
    );
    Ok(r)
}

/// Who is sending, for a broadcast's subject: this node's label.
pub struct From<'a> {
    pub node: &'a crate::net::node::Node,
    pub label: &'a str,
    pub relays: &'a [String],
}

/// Send over the network now.
pub fn send(s: &Store, by: &From, to: &To, topic: &str, text: &str, origin: &str) -> Result<Report> {
    let (node, from, relays) = (by.node, by.label, by.relays);
    send_with(s, to, topic, text, origin, |x, topic, text| {
        let req = crate::net::proto::Req::Broadcast { from: from.to_string(), topic: topic.into(), text: text.into() };
        match crate::net::client::send(s, node, x, req, relays)? {
            crate::net::proto::Resp::Ok => Ok(()),
            crate::net::proto::Resp::Error { msg } => Err(Error::Other(msg)),
            _ => Err(Error::Other("unexpected answer".into())),
        }
    })
}

/// Send what is queued (the running node calls this); what cannot be sent now is dropped
/// with a record, so the queue never grows without bound.
pub fn drain(s: &Store, by: &From) -> usize {
    let q = queued(s);
    if q.is_empty() {
        return 0;
    }
    let _ = s.set_setting(OUTBOX, None);
    let mut n = 0;
    for b in q {
        if send(s, by, &b.to, &b.topic, &b.text, "rule").is_ok() {
            n += 1;
        }
    }
    n
}

/// A broadcast arrived over exchange `x` from peer `peer`.
pub fn receive(s: &Store, x: &Exchange, peer: &str, from: &str, topic: &str, text: &str) -> Result<()> {
    let kind = scope_kind(&x.scope);
    if !matches!(kind, "link" | "node" | "organization") {
        return Err(Error::invalid("broadcast", "this exchange may not broadcast"));
    }
    check(topic, text)?;
    let from: String = from.trim().chars().take(MAX_TOPIC).collect();
    let from = if from.is_empty() { x.title() } else { from };
    let org = x.scope.get("org").and_then(|v| v.as_str()).unwrap_or_default();
    s.note_action(&NewAction::new("net.broadcast", "network", format!("{from} broadcast {topic}")).params(json!({
        "peer": peer, "from": from, "topic": topic.trim(), "text": text, "scope": kind, "org": org, "exchange": x.id,
    })));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn x(id: &str, scope: serde_json::Value) -> Exchange {
        serde_json::from_value(json!({
            "id": id, "label": "", "role": "answerer", "status": "complete", "peer_id": format!("peer{id}"), "peer_label": id,
            "scope": scope, "offer": "", "answer": null, "short_code": null, "fingerprint": null, "expires_at": null,
            "revoked_by": null, "author_id": null, "created_at": 0, "updated_at": 0,
            "verified_at": null, "revoked_at": null
        }))
        .unwrap()
    }

    #[test]
    fn scopes_limits_queue_and_receive() {
        let s = Store::open_in_memory().unwrap();
        let link = x("l", json!({"kind": "link"}));
        let node = x("n", json!({"kind": "node"}));
        let org = x("o", json!({"kind": "organization", "org": "o1"}));
        let share = x("s", json!({"kind": "share"}));
        assert!(hears(&link, &To::Nodes) && !hears(&node, &To::Nodes));
        assert!(hears(&node, &To::Connections) && hears(&org, &To::Org("o1".into())) && !hears(&org, &To::Org("o2".into())));
        assert!(hears(&org, &To::All) && !hears(&share, &To::All));
        assert!(check("", "x").is_err() && check(&"t".repeat(65), "").is_err() && check("t", &"x".repeat(2001)).is_err());
        assert!(To::parse(&s, "everyone").is_err());
        assert_eq!(To::parse(&s, "connections").unwrap(), To::Connections);

        queue(&s, To::Nodes, "deploy", "v2 is out").unwrap();
        assert_eq!(queued(&s).len(), 1);

        assert!(receive(&s, &share, "peers", "A", "t", "x").is_err());
        receive(&s, &org, "peero", "Ben", "deploy", "v2 is out").unwrap();
        let a = s.actions_of_kind("net.broadcast").unwrap();
        assert_eq!((a[0].params["from"].as_str(), a[0].params["org"].as_str()), (Some("Ben"), Some("o1")));
        let n = s.notifications(false, 5).unwrap();
        assert_eq!(n.len(), 1, "a broadcast notifies");
        assert_eq!((n[0].title.as_str(), n[0].body.as_str()), ("Ben: deploy", "v2 is out"));
        let r = send_with(&s, &To::All, "hi", "", "test", |_, _, _| Ok(())).unwrap();
        assert!(r.sent.is_empty(), "no active exchanges here");
    }
}
