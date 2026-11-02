//! Messages between nodes (2.19): direct, or about an item (`journal:ID`, `entry:ID`, …).
//! A message is kept on both sides (and synced to each side's linked nodes); only the node
//! that wrote it delivers it, retrying from the running node until the peer takes it. Each
//! one that arrives is a `net.message` action, which notifies and rule sets can trigger on.
use super::wire::{CollabReq, CollabResp, Transport, unexpected};
use crate::error::{Error, Result};
use crate::store::{Exchange, Message, NewAction, Store, now_ms};
use serde_json::json;

pub const MAX_TEXT: usize = 4000;
pub const MAX_ABOUT: usize = 200;

fn check(about: Option<&str>, text: &str) -> Result<()> {
    if text.trim().is_empty() || text.chars().count() > MAX_TEXT {
        return Err(Error::invalid("message", format!("1 to {MAX_TEXT} characters")));
    }
    if let Some(a) = about
        && (a.len() > MAX_ABOUT || !a.contains(':') || a.chars().any(char::is_whitespace))
    {
        return Err(Error::invalid("about", format!("`{a}` (KIND:ID, like journal:ID)")));
    }
    Ok(())
}

/// Write a message to the peer behind `x` and try to deliver it now; it is kept (and retried
/// by the running node) if the peer cannot be reached.
#[allow(clippy::too_many_arguments)]
pub fn send(s: &Store, t: &dyn Transport, x: &Exchange, me: &str, my_label: &str, about: Option<&str>, text: &str, origin: &str) -> Result<Message> {
    check(about, text)?;
    let peer = x.peer_id.clone().ok_or_else(|| Error::invalid("peer", "this exchange has no peer yet"))?;
    let m = s.new_message(&peer, &x.title(), about, text, me)?;
    s.note_action(
        &NewAction::new("net.message.send", origin, format!("message to {}", x.title()))
            .params(json!({ "peer": peer, "to": x.title(), "about": about, "message": m.id })),
    );
    let _ = deliver(s, t, &m, my_label);
    Ok(s.message(&m.id)?.unwrap_or(m))
}

/// Deliver one waiting message.
pub fn deliver(s: &Store, t: &dyn Transport, m: &Message, my_label: &str) -> Result<()> {
    let req = CollabReq::Message { id: m.id.clone(), from: my_label.into(), about: m.about.clone(), text: m.text.clone(), at: m.created_at };
    let r = match t.call(req) {
        Ok(CollabResp::Delivered) => Ok(()),
        Ok(_) => Err(unexpected()),
        Err(e) => Err(e),
    };
    s.mark_message_delivered(&m.id, r.as_ref().err().map(|e| e.to_string()).as_deref())?;
    r
}

/// The running node: deliver what this node wrote and has not delivered yet.
pub fn tick(s: &Store, node: &crate::net::node::Node, relays: &[String], me: &str, my_label: &str) -> usize {
    let mut n = 0;
    for m in s.undelivered_messages(me).unwrap_or_default() {
        let Ok(x) = super::wire::exchange_to(s, &m.peer, None) else { continue };
        let t = super::wire::Libp2p { store: s, node, exchange: &x, relays };
        if deliver(s, &t, &m, my_label).is_ok() {
            n += 1;
        }
    }
    n
}

/// A message arrived from `peer` over `x`.
#[allow(clippy::too_many_arguments)]
pub fn receive(s: &Store, x: &Exchange, peer: &str, me: &str, id: &str, from: &str, about: Option<&str>, text: &str, at: i64) -> Result<()> {
    check(about, text)?;
    if id.is_empty() || id.len() > 64 {
        return Err(Error::invalid("message", "bad id"));
    }
    let from: String = from.trim().chars().take(64).collect();
    let label = if from.is_empty() { x.title() } else { from };
    let m = Message {
        id: id.into(),
        peer: peer.into(),
        peer_label: label.clone(),
        direction: "in".into(),
        about: about.map(String::from),
        text: text.into(),
        origin: me.into(),
        created_at: if at > 0 { at.min(now_ms()) } else { now_ms() },
        delivered_at: Some(now_ms()),
        read_at: None,
        error: None,
    };
    // a resend of one already here is fine
    if s.put_message(&m)? {
        s.note_action(&NewAction::new("net.message", "network", format!("message from {label}")).params(json!({
            "peer": peer, "from": label, "about": about, "text": text, "message": id, "exchange": x.id,
        })));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct Loop<'a> {
        to: &'a Store,
        x: Exchange,
        down: RefCell<bool>,
    }
    impl Transport for Loop<'_> {
        fn call(&self, req: CollabReq) -> Result<CollabResp> {
            if *self.down.borrow() {
                return Err(Error::Other("unreachable".into()));
            }
            match req {
                CollabReq::Message { id, from, about, text, at } => {
                    receive(self.to, &self.x, "A", "B", &id, &from, about.as_deref(), &text, at)?;
                    Ok(CollabResp::Delivered)
                }
                _ => Err(unexpected()),
            }
        }
    }

    pub(crate) fn x(id: &str, peer: &str, label: &str) -> Exchange {
        serde_json::from_value(json!({
            "id": id, "label": "", "role": "answerer", "status": "complete", "peer_id": peer, "peer_label": label,
            "scope": {"kind": "node"}, "offer": "", "answer": null, "short_code": null, "fingerprint": null, "expires_at": null,
            "revoked_by": null, "author_id": null, "created_at": 0, "updated_at": 0, "verified_at": null, "revoked_at": null
        }))
        .unwrap()
    }

    #[test]
    fn messages_are_kept_retried_deduplicated_and_notify() {
        let (a, b) = (Store::open_in_memory().unwrap(), Store::open_in_memory().unwrap());
        let t = Loop { to: &b, x: x("xb", "A", "ana"), down: RefCell::new(true) };
        let to_b = x("xa", "B", "ben");
        assert!(send(&a, &t, &to_b, "A", "ana", Some("no colon"), "hi", "cli").is_err());
        let m = send(&a, &t, &to_b, "A", "ana", Some("journal:J1"), "hi ben", "cli").unwrap();
        assert!(m.delivered_at.is_none() && m.error.is_some(), "kept while the peer is away");
        assert_eq!(a.undelivered_messages("A").unwrap().len(), 1);
        *t.down.borrow_mut() = false;
        deliver(&a, &t, &a.undelivered_messages("A").unwrap()[0], "ana").unwrap();
        deliver(&a, &t, &m, "ana").unwrap(); // a resend
        assert!(a.undelivered_messages("A").unwrap().is_empty());
        let got = b.messages(None, None).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!((got[0].direction.as_str(), got[0].peer_label.as_str(), got[0].about.as_deref()), ("in", "ana", Some("journal:J1")));
        assert_eq!(b.unread_messages().unwrap(), 1);
        assert_eq!(b.actions_of_kind("net.message").unwrap().len(), 1);
        let n = b.notifications(false, 5).unwrap();
        assert_eq!((n.len(), n[0].title.as_str()), (1, "ana"));
        assert_eq!(b.mark_messages_read(None).unwrap(), 1);
    }
}
