//! Read-only network status and peers (`mnnn net status` / `mnnn net peers`).
//! Uses the unlocked key in Rust; never returns key material.
use super::data::open;
use super::info::paths;
use muninn_core::net::{auto, client, identity, local};
use muninn_core::prefs::Preferences;
use muninn_core::store::{ExchangeStatus, Store};
use std::path::Path;

/// What `mnnn net status` shows: the node and counts, no secrets.
pub struct NetStatus {
    pub label: String,
    pub peer_id: Option<String>,
    pub fingerprint: Option<String>,
    pub running: bool,
    pub pid: Option<i64>,
    pub started_at: Option<i64>,
    pub nat: String,
    pub mdns: bool,
    pub relay_server: bool,
    pub addresses: Vec<String>,
    pub reservations: Vec<String>,
    pub connected: i64,
    pub exchanges_active: i64,
    pub incoming: i64,
    pub shared: i64,
}

/// One known peer (`mnnn net peers`): public facts only.
pub struct NetPeerInfo {
    pub peer_id: String,
    pub short_id: String,
    pub label: String,
    pub user: String,
    pub caps: String,
    pub link: String,
    pub source: String,
    pub last_seen: i64,
}

pub struct NetOverview {
    pub status: NetStatus,
    pub peers: Vec<NetPeerInfo>,
}

fn status_of(store: &Store, home: &Path) -> anyhow::Result<NetStatus> {
    let prefs: Preferences = std::fs::read_to_string(home.join("preferences.toml")).ok().and_then(|s| toml::from_str(&s).ok()).unwrap_or_default();
    let label = local::my_label(store, &prefs.net);
    let running = local::running(home);
    let peer_id = match identity::peer_id(store)? {
        Some(p) => Some(p.to_string()),
        None => running.as_ref().map(|f| f.status.peer_id.clone()).filter(|s| !s.is_empty()),
    };
    let fingerprint = identity::fingerprint_of(store)?;
    let xs = store.exchanges()?;
    let exchanges_active = xs.iter().filter(|x| x.status.active()).count() as i64;
    let incoming = xs.iter().filter(|x| x.status == ExchangeStatus::Incoming).count() as i64;
    let shared = store.shares()?.len() as i64;
    let (nat, mdns, relay_server, addresses, reservations, connected, pid, started_at) = match &running {
        Some(f) => {
            let s = &f.status;
            (
                s.nat.clone(),
                s.mdns,
                s.relay_server,
                local::reachable(s),
                s.reservations.clone(),
                s.connected.len() as i64,
                Some(f.pid as i64),
                (s.started_at != 0).then_some(s.started_at),
            )
        }
        None => (String::new(), false, false, Vec::new(), Vec::new(), 0, None, None),
    };
    Ok(NetStatus {
        label,
        peer_id,
        fingerprint,
        running: running.is_some(),
        pid,
        started_at,
        nat,
        mdns,
        relay_server,
        addresses,
        reservations,
        connected,
        exchanges_active,
        incoming,
        shared,
    })
}

fn peers_of(store: &Store) -> anyhow::Result<Vec<NetPeerInfo>> {
    let mut out = Vec::new();
    for p in store.net_peers()? {
        let link = auto::node_link(store, &p.peer_id)?.map(|x| auto::status_of(&x).as_str().to_string()).unwrap_or_else(|| "-".into());
        let user = muninn_core::net::user::user_of_peer(store, &p.peer_id)?.map(|u| u.label).filter(|l| !l.is_empty()).unwrap_or_default();
        out.push(NetPeerInfo {
            short_id: p.short_id().to_string(),
            peer_id: p.peer_id,
            label: p.label,
            user,
            caps: client::caps_text(&p.caps),
            link,
            source: p.source,
            last_seen: p.last_seen,
        });
    }
    Ok(out)
}

pub(crate) fn net_overview_of(store: &Store, home: &Path) -> anyhow::Result<NetOverview> {
    Ok(NetOverview { status: status_of(store, home)?, peers: peers_of(store)? })
}

/// `mnnn net status` and `mnnn net peers`. The store is opened read-only.
pub fn net_overview(home: Option<String>) -> anyhow::Result<NetOverview> {
    net_overview_of(&open()?, &paths(home)?.home)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::data::lock;
    use muninn_core::net::identity;
    use muninn_core::net::node::Status;
    use muninn_core::store::Store;

    #[test]
    fn empty_store_is_stopped_with_no_identity() {
        let s = Store::open_in_memory().unwrap();
        let home = tempfile::tempdir().unwrap();
        let o = net_overview_of(&s, home.path()).unwrap();
        assert!(!o.status.running);
        assert!(o.status.peer_id.is_none() && o.status.fingerprint.is_none());
        assert_eq!(o.status.label, "muninn");
        assert_eq!((o.status.exchanges_active, o.status.shared, o.peers.len()), (0, 0, 0));
        assert!(!home.path().join("preferences.toml").exists(), "status must not create preferences");
    }

    #[test]
    fn identity_is_read_not_created() {
        let s = Store::open_in_memory().unwrap();
        let home = tempfile::tempdir().unwrap();
        assert!(net_overview_of(&s, home.path()).unwrap().status.peer_id.is_none());
        let kp = identity::load_or_create(&s).unwrap();
        let o = net_overview_of(&s, home.path()).unwrap();
        assert_eq!(o.status.peer_id.as_deref(), Some(kp.public().to_peer_id().to_string().as_str()));
        assert_eq!(o.status.fingerprint.as_deref(), Some(identity::fingerprint(&kp.public()).as_str()));
    }

    #[test]
    fn running_node_file_and_known_peers() {
        let s = Store::open_in_memory().unwrap();
        let home = tempfile::tempdir().unwrap();
        let st = Status {
            peer_id: "12D3KooWabc".into(),
            listening: vec!["/ip4/1.2.3.4/tcp/4741".into()],
            connected: vec!["12D3KooWxyz".into()],
            nat: "public".into(),
            mdns: true,
            relay_server: false,
            started_at: 1_704_067_200_000,
            ..Default::default()
        };
        local::write(home.path(), "desk", &st).unwrap();
        s.upsert_net_peer("12D3KooWpeerXXXX", Some("lab"), &[], Some(&["muninn".into()]), None, "mdns").unwrap();
        let o = net_overview_of(&s, home.path()).unwrap();
        assert!(o.status.running);
        assert!(o.status.pid.is_some_and(|p| p > 0));
        assert_eq!(o.status.nat, "public");
        assert!(o.status.mdns && !o.status.relay_server);
        assert_eq!(o.status.connected, 1);
        assert!(o.status.addresses.iter().any(|a| a.contains("1.2.3.4")), "{:?}", o.status.addresses);
        assert_eq!(o.status.started_at, Some(1_704_067_200_000));
        assert_eq!(o.peers.len(), 1);
        assert_eq!((o.peers[0].label.as_str(), o.peers[0].source.as_str(), o.peers[0].link.as_str()), ("lab", "mdns", "-"));
        assert_eq!(o.peers[0].caps, "muninn");
        assert!(o.peers[0].short_id.len() <= 8);
    }

    #[test]
    fn overview_needs_unlock() {
        lock();
        assert!(net_overview(None).is_err());
    }
}
