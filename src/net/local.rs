//! The running node as seen from other processes: `MUNINN_HOME/net/node.json` (refreshed
//! while it runs) and a stop request file. No secrets are written here.
use super::node::{Node, NodeConfig, Status};
use super::service::Service;
use crate::error::Result;
use crate::prefs::NetPrefs;
use crate::store::{NewAction, Store};
use libp2p::identity::Keypair;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Written by a running node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeFile {
    pub pid: u32,
    pub label: String,
    pub status: Status,
    pub updated_at: i64,
}

pub fn dir(home: &Path) -> PathBuf {
    home.join("net")
}

pub fn node_file(home: &Path) -> PathBuf {
    dir(home).join("node.json")
}

pub fn stop_file(home: &Path) -> PathBuf {
    dir(home).join("stop")
}

/// Record the node's state for other processes.
pub fn write(home: &Path, label: &str, status: &Status) -> Result<()> {
    std::fs::create_dir_all(dir(home))?;
    let f = NodeFile { pid: std::process::id(), label: label.to_string(), status: status.clone(), updated_at: crate::store::now_ms() };
    let tmp = dir(home).join("node.json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&f)?)?;
    std::fs::rename(tmp, node_file(home))?;
    Ok(())
}

/// The node another process runs, if its file is fresh (updated in the last 15 s).
pub fn running(home: &Path) -> Option<NodeFile> {
    let f: NodeFile = serde_json::from_slice(&std::fs::read(node_file(home)).ok()?).ok()?;
    (crate::store::now_ms() - f.updated_at < 15_000).then_some(f)
}

/// Clean up when the node stops.
pub fn clear(home: &Path) {
    let _ = std::fs::remove_file(node_file(home));
    let _ = std::fs::remove_file(stop_file(home));
}

/// Ask a running node to stop (it checks twice a second).
pub fn request_stop(home: &Path) -> Result<()> {
    std::fs::create_dir_all(dir(home))?;
    std::fs::write(stop_file(home), b"stop")?;
    Ok(())
}

/// Was a stop asked for? (Consumes the request.)
pub fn stop_requested(home: &Path) -> bool {
    let p = stop_file(home);
    if p.exists() {
        let _ = std::fs::remove_file(p);
        true
    } else {
        false
    }
}

/// Addresses in a status others can dial (with `/p2p/`).
pub fn reachable(s: &Status) -> Vec<String> {
    let mut v = vec![];
    for a in s.external.iter().chain(s.listening.iter()) {
        if let Ok(m) = a.parse::<libp2p::Multiaddr>()
            && super::node::shareable(&m)
        {
            let full = if super::node::addr_peer(&m).is_some() { m.to_string() } else { format!("{m}/p2p/{}", s.peer_id) };
            if !v.contains(&full) {
                v.push(full);
            }
        }
    }
    v
}

/// Our addresses for offers: the running node's (this or another process), else the saved ones.
pub fn offer_addrs(home: &Path, store: &Store, node: Option<&Node>) -> Vec<String> {
    if let Some(n) = node {
        let v = n.reachable_addrs();
        if !v.is_empty() {
            return v;
        }
    }
    match running(home) {
        Some(f) => reachable(&f.status),
        None => super::client::my_addrs(store, None),
    }
}

/// The node this process runs for the user (`mnnn net start` or the TUI): it keeps
/// `node.json` and the saved addresses fresh and records start and stop.
pub struct Hosted {
    pub node: Arc<Node>,
    pub label: String,
    home: PathBuf,
    last: Instant,
    last_addrs: Vec<String>,
    settle: Settle,
}

/// Background upkeep of pending exchanges ([`super::auto::settle`]) on a spare store.
#[derive(Default)]
struct Settle {
    spare: Option<Store>,
    relays: Vec<String>,
    running: Option<std::thread::JoinHandle<Store>>,
    last: Option<Instant>,
}

/// How often a running node brings pending exchanges along.
pub const SETTLE_EVERY: Duration = Duration::from_secs(20);

impl Hosted {
    /// Start listening as configured. Refused while another process runs a node here.
    pub fn start(home: &Path, store: &Store, service: Store, kp: &Keypair, label: &str, prefs: &NetPrefs, origin: &str) -> Result<Hosted> {
        if let Some(f) = running(home) {
            return Err(crate::error::Error::Other(format!("a node is already running here (pid {}); `mnnn net stop` stops it", f.pid)));
        }
        let cfg = prefs.node_config(label)?;
        let node =
            Node::start(kp.clone(), cfg, Arc::new(Service::new(service, kp.public().to_peer_id(), label).accept_anyone(prefs.accept_anyone())))?;
        // a stale stop request must not stop the new node
        stop_requested(home);
        // let addresses settle
        std::thread::sleep(Duration::from_millis(300));
        let st = node.status();
        write(home, label, &st)?;
        let addrs = node.reachable_addrs();
        store.set_setting(super::client::ADDRS_SETTING, Some(&serde_json::to_string(&addrs)?))?;
        store.note_action(
            &NewAction::new("net.node.start", origin, format!("node {label} started"))
                .params(serde_json::json!({ "peer": st.peer_id, "listen": st.listening, "relay_server": prefs.relay_server, "mdns": prefs.mdns })),
        );
        Ok(Hosted {
            node: Arc::new(node),
            label: label.to_string(),
            home: home.to_path_buf(),
            last: Instant::now(),
            last_addrs: addrs,
            settle: Settle::default(),
        })
    }

    /// Keep pending exchanges moving in the background, on this store connection: ask
    /// peers about our open requests and deliver answers that did not get through.
    pub fn settle_with(mut self, store: Store, relays: Vec<String>) -> Hosted {
        self.settle = Settle { spare: Some(store), relays, running: None, last: None };
        self
    }

    /// Call often: every 2 s it refreshes `node.json` and the saved addresses.
    /// False when the node stopped or `mnnn net stop` asked it to.
    pub fn tick(&mut self, store: &Store) -> bool {
        if !self.node.running() || stop_requested(&self.home) {
            return false;
        }
        if self.last.elapsed() >= Duration::from_secs(2) {
            self.last = Instant::now();
            let _ = write(&self.home, &self.label, &self.node.status());
            let addrs = self.node.reachable_addrs();
            if addrs != self.last_addrs {
                let _ = store.set_setting(super::client::ADDRS_SETTING, Some(&serde_json::to_string(&addrs).unwrap_or_default()));
                self.last_addrs = addrs;
            }
        }
        let st = &mut self.settle;
        if st.running.as_ref().is_some_and(|h| h.is_finished())
            && let Some(Ok(s)) = st.running.take().map(|h| h.join())
        {
            st.spare = Some(s);
        }
        if st.spare.is_some() && st.last.is_none_or(|l| l.elapsed() >= SETTLE_EVERY) {
            st.last = Some(Instant::now());
            let s = st.spare.take().expect("spare");
            let node = self.node.clone();
            let relays = st.relays.clone();
            st.running = Some(std::thread::spawn(move || {
                let _ = super::auto::settle(&s, &node, &relays, "network");
                s
            }));
        }
        true
    }

    /// Stop the node, clean up and record it.
    pub fn stop(self, store: &Store, origin: &str) {
        let peer = self.node.peer_id().to_string();
        self.node.halt();
        drop(self.node);
        clear(&self.home);
        store.note_action(
            &NewAction::new("net.node.stop", origin, format!("node {} stopped", self.label)).params(serde_json::json!({ "peer": peer })),
        );
    }
}

/// How this node introduces itself: the setting, else the current author, else "muninn".
pub fn my_label(store: &Store, prefs: &NetPrefs) -> String {
    if !prefs.label.trim().is_empty() {
        return prefs.label.trim().to_string();
    }
    store.current_author().ok().flatten().map(|a| a.name).filter(|n| !n.is_empty()).unwrap_or_else(|| "muninn".into())
}

/// A short-lived node for one command: same identity, dials out, does not listen or
/// announce itself. It still answers peers on the connections it opens.
pub fn ephemeral(kp: &Keypair, service: Store, label: &str, prefs: &NetPrefs) -> Result<Node> {
    let cfg = NodeConfig {
        listen: vec![],
        mdns: false,
        relay_server: false,
        bootstrap: prefs.bootstrap.iter().filter_map(|a| a.parse().ok()).collect(),
        relays: vec![],
        memory: false,
        label: label.to_string(),
    };
    Node::start(kp.clone(), cfg, Arc::new(Service::new(service, kp.public().to_peer_id(), label).accept_anyone(prefs.accept_anyone())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_file_and_stop_request() {
        let d = tempfile::tempdir().unwrap();
        assert!(running(d.path()).is_none());
        let st = Status { peer_id: "12D3KooW".into(), ..Default::default() };
        write(d.path(), "anna", &st).unwrap();
        let f = running(d.path()).unwrap();
        assert_eq!((f.label.as_str(), f.status.peer_id.as_str()), ("anna", "12D3KooW"));
        assert!(!stop_requested(d.path()));
        request_stop(d.path()).unwrap();
        assert!(stop_requested(d.path()));
        assert!(!stop_requested(d.path()));
        clear(d.path());
        assert!(running(d.path()).is_none());
    }
}
