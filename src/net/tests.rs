//! Several nodes in one process over the in-memory transport: exchanges, sharing,
//! searches across nodes, relays and dark shards. No internet needed.
use super::client;
use super::identity;
use super::node::{Node, NodeConfig};
use super::service::Service;
use crate::crypto::Key32;
use crate::live::Found;
use crate::store::{EntryFormat, ExchangeStatus, SearchSpec, SearchTarget, ShareKind, Store};
use libp2p::identity::Keypair;
use secrecy::SecretString;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

struct Peer {
    dir: tempfile::TempDir,
    store: Store,
    node: Option<Node>,
    kp: Keypair,
    label: String,
}

impl Peer {
    fn new(label: &str) -> Peer {
        Self::with(label, false, tempfile::tempdir().unwrap())
    }

    fn with(label: &str, relay_server: bool, dir: tempfile::TempDir) -> Peer {
        let store = Store::open(&dir.path().join("db"), None).unwrap();
        let kp = identity::load_or_create(&store).unwrap();
        let mut p = Peer { dir, store, node: None, kp, label: label.into() };
        p.start(relay_server, vec![]);
        p
    }

    fn db(&self) -> PathBuf {
        self.dir.path().join("db")
    }

    fn start(&mut self, relay_server: bool, relays: Vec<libp2p::Multiaddr>) {
        let svc = Service::new(Store::open(&self.db(), None).unwrap(), self.kp.public().to_peer_id(), &self.label);
        let cfg = NodeConfig {
            listen: vec!["/memory/0".parse().unwrap()],
            memory: true,
            relay_server,
            relays,
            label: self.label.clone(),
            ..Default::default()
        };
        self.node = Some(Node::start(self.kp.clone(), cfg, Arc::new(svc)).unwrap());
    }

    fn node(&self) -> &Node {
        self.node.as_ref().unwrap()
    }

    fn addr(&self) -> String {
        self.node().reachable_addrs().into_iter().next().expect("listening")
    }

    fn peer_id(&self) -> String {
        self.kp.public().to_peer_id().to_string()
    }

    /// Offer over the network to `other`, which accepts (answer goes back over the network).
    fn exchange_with(&self, other: &Peer) -> (String, String) {
        let c = client::create(
            &self.store,
            Some(self.node()),
            &self.kp,
            "",
            &self.label,
            client::my_addrs(&self.store, Some(self.node())),
            Some(&other.addr()),
            client::DEFAULT_TTL_MS,
            &[],
            "cli",
        )
        .unwrap();
        assert!(matches!(c.delivered, Some(Ok(_))), "{:?}", c.delivered);
        let inc = other.store.exchange(&c.exchange.id).unwrap().expect("incoming offer stored");
        assert_eq!(inc.status, ExchangeStatus::Incoming);
        let a = client::accept(
            &other.store,
            Some(other.node()),
            &other.kp,
            &inc.id,
            "",
            &other.label,
            client::my_addrs(&other.store, Some(other.node())),
            "cli",
        )
        .unwrap();
        assert!(matches!(a.delivered, Some(Ok(_))), "{:?}", a.delivered);
        let mine = wait(|| self.store.exchange(&c.exchange.id).unwrap().filter(|x| x.status.active()));
        assert_eq!(mine.short_code, a.exchange.short_code);
        (c.exchange.id, mine.short_code.unwrap())
    }
}

fn wait<T>(mut f: impl FnMut() -> Option<T>) -> T {
    let t = Instant::now();
    loop {
        if let Some(v) = f() {
            return v;
        }
        assert!(t.elapsed() < Duration::from_secs(20), "timed out waiting");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn two_nodes_exchange_keys_over_the_network_and_verify() {
    let a = Peer::new("anna");
    let b = Peer::new("ben");
    let (id, code) = a.exchange_with(&b);
    let xa = a.store.exchange(&id).unwrap().unwrap();
    let xb = b.store.exchange(&id).unwrap().unwrap();
    assert_eq!(xa.peer_id.as_deref(), Some(b.peer_id().as_str()));
    assert_eq!(xb.peer_id.as_deref(), Some(a.peer_id().as_str()));
    assert_eq!(xa.peer_label, "ben");
    assert_eq!(xb.peer_label, "anna");
    assert_eq!(xa.fingerprint, xb.fingerprint);
    assert_eq!(a.store.exchange_key(&id).unwrap().unwrap().0, b.store.exchange_key(&id).unwrap().unwrap().0);
    assert!(client::verify(&a.store, &id, "00000-00000", "cli").is_err());
    assert_eq!(client::verify(&a.store, &id, &code.replace('-', " "), "cli").unwrap().status, ExchangeStatus::Verified);
    // both sides recorded it
    assert!(a.store.actions().unwrap().iter().any(|x| x.kind == "net.exchange.complete"));
    assert!(b.store.actions().unwrap().iter().any(|x| x.kind == "net.exchange.incoming"));
    // ping over the sealed channel
    let (label, shared) = client::pong(&a.store, a.node(), &xa, &[]).unwrap();
    assert_eq!((label.as_str(), shared), ("ben", 0));
}

#[test]
fn offline_tokens_complete_an_exchange_without_a_connection() {
    let a = Peer::new("anna");
    let b = Peer::new("ben");
    // the offer travels by copy and paste; the answer too (no node given)
    let c = client::create(&a.store, None, &a.kp, "ben", "anna", vec![], None, client::DEFAULT_TTL_MS, &[], "cli").unwrap();
    let acc = client::accept(&b.store, None, &b.kp, &c.token, "anna", "ben", vec![], "cli").unwrap();
    let done = client::accept(&a.store, None, &a.kp, acc.answer.as_ref().unwrap(), "", "anna", vec![], "cli").unwrap();
    assert_eq!(done.exchange.status, ExchangeStatus::Complete);
    assert_eq!(done.exchange.short_code, acc.exchange.short_code);
    assert_eq!(done.exchange.label, "ben");
    // an answer replayed, or an offer accepted twice, is refused
    assert!(client::accept(&a.store, None, &a.kp, acc.answer.as_ref().unwrap(), "", "anna", vec![], "cli").is_err());
    assert!(client::accept(&b.store, None, &b.kp, &c.token, "", "ben", vec![], "cli").is_err());
}

#[test]
fn peers_see_only_what_is_shared_with_them_and_searches_merge_by_origin() {
    let a = Peer::new("anna");
    let b = Peer::new("ben");
    let c = Peer::new("cleo");
    let (ab, _) = a.exchange_with(&b);
    let (ac, _) = a.exchange_with(&c);
    // B and C each have a shared and a private journal
    for (p, word) in [(&b, "ben"), (&c, "cleo")] {
        let shared = p.store.create_journal("field notes", None, EntryFormat::Markdown).unwrap();
        p.store.add_entry(&shared.id, "Comet", &format!("a comet seen by {word}"), None).unwrap();
        let private = p.store.create_journal("diary", None, EntryFormat::Markdown).unwrap();
        p.store.add_entry(&private.id, "Comet secret", "a private comet", None).unwrap();
        client::share(&p.store, ShareKind::Journal, "field notes", &[], "cli").unwrap();
        let r = p.store.create_report("summary", "").unwrap();
        let _ = r;
    }
    // A has its own
    let mine = a.store.create_journal("mine", None, EntryFormat::Markdown).unwrap();
    a.store.add_entry(&mine.id, "Comet here", "my comet", None).unwrap();

    let spec = SearchSpec { target: SearchTarget::Journals, query: "comet".into(), ..Default::default() };
    let res = client::search(&a.store, Some(a.node()), &spec, &[], true, 100, &[], "cli").unwrap();
    assert_eq!(res.len(), 3);
    assert_eq!(res[0].origin, "local");
    assert_eq!(res[0].found.len(), 1);
    for o in &res[1..] {
        assert!(o.error.is_none(), "{:?}", o.error);
        assert_eq!(o.found.len(), 1, "only the shared journal: {}", o.origin);
        let Found::Entry { journal_name, entry, .. } = &o.found[0] else { panic!() };
        assert_eq!(journal_name, "field notes");
        assert!(entry.body.contains(&o.origin));
    }
    // only one peer
    let only = client::search(&a.store, Some(a.node()), &spec, &["cleo".to_string()], false, 100, &[], "cli").unwrap();
    assert_eq!(only.len(), 1);
    assert_eq!(only[0].origin, "cleo");

    // listing and fetching: shared yes, private no
    let xb = a.store.exchange(&ab).unwrap().unwrap();
    let items = client::remote_list(&a.store, a.node(), &xb, &[]).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].name, "field notes");
    let it = client::fetch(&a.store, a.node(), &xb, "field notes", None, &[], "cli").unwrap();
    assert_eq!(it.format, "json");
    assert!(String::from_utf8_lossy(&it.data).contains("a comet seen by ben"));
    let html = client::fetch(&a.store, a.node(), &xb, "field notes", Some("html"), &[], "cli").unwrap();
    assert_eq!(html.format, "html");
    assert!(client::fetch(&a.store, a.node(), &xb, "diary", None, &[], "cli").is_err());
    assert!(client::fetch(&a.store, a.node(), &xb, "summary", None, &[], "cli").is_err());
    // shared with one exchange only
    client::share(&b.store, ShareKind::Report, "summary", std::slice::from_ref(&ab), "cli").unwrap();
    assert!(client::fetch(&a.store, a.node(), &xb, "summary", None, &[], "cli").is_ok());
    // C never shared its report with anyone
    let xc = a.store.exchange(&ac).unwrap().unwrap();
    assert!(client::fetch(&a.store, a.node(), &xc, "summary", None, &[], "cli").is_err());
    // unshare: gone
    client::unshare(&b.store, ShareKind::Journal, "field notes", &[], "cli").unwrap();
    let res = client::search(&a.store, Some(a.node()), &spec, std::slice::from_ref(&ab), false, 100, &[], "cli").unwrap();
    assert!(res[0].found.is_empty());
    // revoke: B tells A; nothing flows any more, in either direction
    let (x, told) = client::revoke(&b.store, Some(b.node()), &ab, &[], "cli").unwrap();
    assert_eq!(x.status, ExchangeStatus::Revoked);
    assert!(matches!(told, Some(Ok(()))));
    let xa = a.store.exchange(&ab).unwrap().unwrap();
    assert_eq!(xa.status, ExchangeStatus::Revoked);
    assert_eq!(xa.revoked_by.as_deref(), Some("peer"));
    assert!(client::remote_list(&a.store, a.node(), &xb, &[]).is_err());
}

#[test]
fn sealed_requests_from_the_wrong_node_are_refused() {
    let a = Peer::new("anna");
    let b = Peer::new("ben");
    let m = Peer::new("mallory");
    let (ab, _) = a.exchange_with(&b);
    // mallory copies A's exchange row (id and key) but signs with its own node identity
    let x = a.store.exchange(&ab).unwrap().unwrap();
    let key = a.store.exchange_key(&ab).unwrap().unwrap();
    m.store.restore_exchange_raw(&x, None, Some(&key.0)).unwrap();
    let err = client::remote_list(&m.store, m.node(), &x, &[]).unwrap_err().to_string();
    assert!(err.contains("another node"), "{err}");
}

#[test]
fn relays_carry_traffic_and_say_what_they_are() {
    let r = Peer::with("relay", true, tempfile::tempdir().unwrap());
    let mut b = Peer::new("ben");
    let a = Peer::new("anna");
    // B is reachable only through the relay
    b.node = None;
    let relay_addr: libp2p::Multiaddr = r.addr().parse().unwrap();
    b.start(false, vec![relay_addr.clone()]);
    let circuit = wait(|| {
        let s = b.node().status();
        if s.reservations.is_empty() { None } else { b.node().reachable_addrs().into_iter().find(|x| x.contains("p2p-circuit")) }
    });
    // A learns about the relay's capabilities when it says hello
    client::hello(&a.store, a.node(), &r.addr(), None, &[]).unwrap();
    let rp = wait(|| a.store.net_peer(&r.peer_id()).unwrap().filter(|p| p.is_relay()));
    assert!(rp.agent.starts_with("muninn/"));
    // A reaches B through the relay
    let peer = client::hello(&a.store, a.node(), &circuit, None, &[]).unwrap();
    assert_eq!(peer.label, "ben");
}

#[test]
fn dark_shards_go_to_peers_sealed_and_come_back() {
    let a = Peer::new("anna");
    let custodians: Vec<Peer> = ["ben", "cleo", "dev"].iter().map(|l| Peer::new(l)).collect();
    let mut ids = vec![];
    for c in &custodians {
        ids.push(a.exchange_with(c).0);
    }
    let master = Key32([42u8; 32]);
    let pw = SecretString::from("shard password");
    let e = client::entrust(&a.store, a.node(), &master, &pw, &ids, 2, &[], "cli").unwrap();
    assert_eq!((e.threshold, e.placed.len()), (2, 3));
    assert_eq!(a.store.entrusted().unwrap().len(), 3);
    for c in &custodians {
        let held = c.store.held_shards().unwrap();
        assert_eq!(held.len(), 1);
        // custodians hold only a password-sealed blob
        assert!(!held[0].blob.contains("muninn-shard-v1"));
        assert_eq!(held[0].owner_peer, a.peer_id());
    }
    // one custodian away: two are enough
    let (k, g) = client::gather(&a.store, a.node(), &pw, None, &ids[..2], &[], "cli").unwrap();
    assert_eq!(k, master);
    assert_eq!((g.got, g.threshold), (2, 2));
    // the wrong password opens nothing
    assert!(client::gather(&a.store, a.node(), &SecretString::from("nope"), None, &ids, &[], "cli").is_err());
    // a fresh node (new identity, everything else lost) reclaims them with the password
    let fresh = Peer::new("anna again");
    let mut fx = vec![];
    for c in &custodians[1..] {
        fx.push(fresh.exchange_with(c).0);
    }
    let (k, g) = client::gather(&fresh.store, fresh.node(), &pw, None, &fx, &[], "cli").unwrap();
    assert_eq!(k, master);
    assert_eq!(g.got, 2);
    // one shard is not enough
    assert!(client::gather(&fresh.store, fresh.node(), &pw, None, &fx[..1], &[], "cli").is_err());
}

#[test]
fn identity_and_exchanges_survive_a_restart() {
    let dir_a = tempfile::tempdir().unwrap();
    let mut a = Peer::with("anna", false, dir_a);
    let b = Peer::new("ben");
    let (id, _) = a.exchange_with(&b);
    client::share(&b.store, ShareKind::Journal, "x", &[], "cli").unwrap_err();
    let j = b.store.create_journal("x", None, EntryFormat::Markdown).unwrap();
    b.store.add_entry(&j.id, "t", "kept", None).unwrap();
    client::share(&b.store, ShareKind::Journal, "x", &[], "cli").unwrap();
    let before = a.peer_id();
    // stop and reopen everything from disk
    a.node = None;
    let store = Store::open(&a.db(), None).unwrap();
    let kp = identity::load_or_create(&store).unwrap();
    assert_eq!(kp.public().to_peer_id().to_string(), before);
    a.store = store;
    a.kp = kp;
    a.start(false, vec![]);
    let x = a.store.exchange(&id).unwrap().unwrap();
    assert!(x.status.active());
    let items = client::remote_list(&a.store, a.node(), &x, &[]).unwrap();
    assert_eq!(items.len(), 1);
}
