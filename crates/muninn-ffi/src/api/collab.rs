//! Peers, messages and live sessions (`mnnn peers` / TUI `W`). Exchange tokens stay
//! in Rust. Sending, hosting and joining use the hosted node when the app started
//! one, otherwise a short-lived node, the same way the TUI does.
use super::data::{open, open_write};
use crate::hold::hold_node;
use muninn_core::collab::{self, msg as cmsg, peers as cpeers, session, wire};
use muninn_core::net::identity;
use muninn_core::store::Store;

const ORIGIN: &str = "app";

pub struct PeerMessageInfo {
    pub id: String,
    pub peer: String,
    pub peer_label: String,
    pub direction: String,
    pub about: String,
    pub text: String,
    pub created_at: i64,
    pub delivered: bool,
    pub read: bool,
    pub error: String,
}

pub struct LiveSessionInfo {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub role: String,
    pub host_peer: String,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
}

fn message_info(m: &muninn_core::store::Message) -> PeerMessageInfo {
    PeerMessageInfo {
        id: m.id.clone(),
        peer: m.peer.clone(),
        peer_label: m.peer_label.clone(),
        direction: m.direction.clone(),
        about: m.about.clone().unwrap_or_default(),
        text: m.text.clone(),
        created_at: m.created_at,
        delivered: m.delivered_at.is_some(),
        read: m.read_at.is_some(),
        error: m.error.clone().unwrap_or_default(),
    }
}

pub(crate) fn peer_messages_of(store: &Store, peer: Option<&str>) -> anyhow::Result<Vec<PeerMessageInfo>> {
    Ok(store.messages(peer.filter(|s| !s.is_empty()), None)?.iter().map(message_info).collect())
}

pub(crate) fn live_sessions_of(store: &Store) -> anyhow::Result<Vec<LiveSessionInfo>> {
    Ok(store
        .live_sessions()?
        .into_iter()
        .map(|x| LiveSessionInfo {
            id: x.id,
            kind: x.kind,
            title: x.title,
            role: x.role,
            host_peer: x.host_peer,
            status: x.status,
            created_at: x.created_at,
            updated_at: x.updated_at,
        })
        .collect())
}

/// TUI `W` Messages: the conversation stored for a peer (or every peer).
pub fn peer_messages(peer: Option<String>) -> anyhow::Result<Vec<PeerMessageInfo>> {
    peer_messages_of(&open()?, peer.as_deref().map(str::trim).filter(|s| !s.is_empty()))
}

/// Mark messages read (all, or one peer's), like `mnnn peers messages --read`.
pub fn peer_messages_mark_read(peer: Option<String>) -> anyhow::Result<String> {
    let store = super::data::open_write()?;
    let n = store.mark_messages_read(peer.as_deref().map(str::trim).filter(|s| !s.is_empty()))?;
    Ok(if n == 0 { "no unread messages".into() } else { format!("marked {n} message(s) read") })
}

/// TUI `W` Live: sessions this node hosts or joined.
pub fn live_sessions() -> anyhow::Result<Vec<LiveSessionInfo>> {
    live_sessions_of(&open()?)
}

/// Send a message to a peer (`mnnn peers` message).
pub fn collab_send(home: Option<String>, peer: String, text: String) -> anyhow::Result<String> {
    let text = text.trim();
    if text.is_empty() {
        anyhow::bail!("write the message");
    }
    let peer = peer.trim();
    if peer.is_empty() {
        anyhow::bail!("name the peer");
    }
    let store = open_write()?;
    let held = hold_node(home)?;
    let kp = identity::load_or_create(&store)?;
    let x = wire::exchange_to(&store, peer, None)?;
    let mode = collab::stream::mode(&held.prefs.net);
    let t = wire::open(&store, held.node(), &x, &held.prefs.net.relays, mode);
    let me = kp.public().to_peer_id().to_string();
    let m = cmsg::send(&store, t.as_ref(), &x, &me, &held.label, None, text, ORIGIN)?;
    Ok(if m.delivered_at.is_some() {
        format!("delivered to {}", x.title())
    } else {
        format!("kept: {} is not reachable now; the running node keeps trying", x.title())
    })
}

/// Host a live session and invite one peer. The running node answers guests.
pub fn collab_host(peer: String, kind: String, key: String) -> anyhow::Result<String> {
    let peer = peer.trim().to_string();
    if peer.is_empty() {
        anyhow::bail!("name the peer");
    }
    if kind.trim().is_empty() || key.trim().is_empty() {
        anyhow::bail!("give the kind and the name (journal Notes, project Lab, code alpha)");
    }
    let store = open_write()?;
    let kp = identity::load_or_create(&store)?;
    let me = kp.public().to_peer_id().to_string();
    let ls = session::host(&store, ORIGIN, &me, kind.trim(), key.trim(), std::slice::from_ref(&peer))?;
    Ok(format!("hosting {} {} live (the running node answers guests)", ls.kind, ls.title))
}

/// Join a live session a peer invited you to.
pub fn collab_join(home: Option<String>, peer: String, session_id: String) -> anyhow::Result<String> {
    let peer = peer.trim();
    let id = session_id.trim();
    if peer.is_empty() || id.is_empty() {
        anyhow::bail!("name the peer and the session");
    }
    let store = open_write()?;
    let held = hold_node(home)?;
    let x = wire::exchange_to(&store, peer, None)?;
    let mode = collab::stream::mode(&held.prefs.net);
    let relays = &held.prefs.net.relays;
    let t = wire::open(&store, held.node(), &x, relays, mode);
    let offers = cpeers::offers(t.as_ref())?;
    if !offers["live"].as_array().into_iter().flatten().any(|o| o["session"] == id) {
        anyhow::bail!("{} has not invited you to {id}", x.title());
    }
    let ls = session::joined(&store, id, peer, &x.id, &serde_json::json!({}))?;
    let _ = session::sync_with(&store, &ls, vec![], |b| t.live(b))?;
    let title = if ls.title.is_empty() { id.to_string() } else { ls.title.clone() };
    Ok(format!("joined {} {title}", ls.kind))
}

#[cfg(test)]
mod tests {
    use super::*;
    use muninn_core::store::Store;

    #[test]
    fn messages_and_sessions_list_from_the_store() {
        let s = Store::open_in_memory().unwrap();
        assert!(peer_messages_of(&s, None).unwrap().is_empty());
        assert!(live_sessions_of(&s).unwrap().is_empty());
        let out = s.new_message("12D3KooWPeer", "ben", None, "hello there", "me").unwrap();
        // outgoing messages are stored already-read; incoming ones are the unread side
        s.put_message(&muninn_core::store::Message { id: "in1".into(), direction: "in".into(), read_at: None, ..out.clone() }).unwrap();
        let list = peer_messages_of(&s, None).unwrap();
        assert_eq!(list.len(), 2);
        let incoming = list.iter().find(|m| m.direction == "in").expect("incoming");
        assert_eq!(incoming.text, "hello there");
        assert_eq!(incoming.peer_label, "ben");
        assert!(!incoming.read);
        let n = s.mark_messages_read(None).unwrap();
        assert_eq!(n, 1);
        let after = peer_messages_of(&s, None).unwrap();
        assert!(after.iter().all(|m| m.read));
    }
}
