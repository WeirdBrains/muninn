//! Peers, messages and live sessions (`mnnn peers` / TUI `W`). Exchange tokens stay
//! in Rust. Sending a message or joining a live session needs the node worker and
//! stays in the TUI; this lists what the store already holds.
use super::data::open;
use muninn_core::store::Store;

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
