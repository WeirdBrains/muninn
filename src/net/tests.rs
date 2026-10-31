//! Several nodes in one process over the in-memory transport: exchanges, sharing,
//! searches across nodes, relays and dark shards. No internet needed.
use super::auto::{self, LinkStatus};
use super::client;
use super::identity;
use super::node::{Node, NodeConfig};
use super::service::Service;
use super::user;
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
    anyone: bool,
}

impl Peer {
    fn new(label: &str) -> Peer {
        Self::with(label, false, tempfile::tempdir().unwrap())
    }

    fn with(label: &str, relay_server: bool, dir: tempfile::TempDir) -> Peer {
        let store = Store::open(&dir.path().join("db"), None).unwrap();
        let kp = identity::load_or_create(&store).unwrap();
        let mut p = Peer { dir, store, node: None, kp, label: label.into(), anyone: false };
        p.start(relay_server, vec![]);
        p
    }

    fn db(&self) -> PathBuf {
        self.dir.path().join("db")
    }

    fn start(&mut self, relay_server: bool, relays: Vec<libp2p::Multiaddr>) {
        let svc = Service::new(Store::open(&self.db(), None).unwrap(), self.kp.public().to_peer_id(), &self.label).accept_anyone(self.anyone);
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

impl Peer {
    /// The black-box connect: what `mnnn net connect` and the TUI do.
    fn connect(&self, other: &Peer) -> auto::Link {
        auto::connect(
            &self.store,
            self.node(),
            &self.kp,
            &other.addr(),
            None,
            &self.label,
            client::my_addrs(&self.store, Some(self.node())),
            &[],
            "cli",
        )
        .unwrap()
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

// ---------- organizations (2.7) ----------

#[test]
fn nodes_apply_get_approved_invited_and_kicked_from_an_org() {
    use crate::org::{self, Actor, Leftovers, net as onet};
    use crate::store::{MemberStatus, OrgChange, Visibility};
    let a = Peer::new("anna");
    let b = Peer::new("ben");
    let c = Peer::new("cleo");
    let d = Peer::new("dora");
    let o = org::create(&a.store, "cli", "Research Lab", None, "we look up").unwrap();
    // a private org is not listed, but its slug finds it
    assert!(onet::browse(&b.store, b.node(), &a.addr(), None, &[]).unwrap().is_empty());
    assert_eq!(onet::browse(&b.store, b.node(), &a.addr(), Some("research-lab"), &[]).unwrap()[0].id, o.id);
    let o = org::edit(&a.store, "cli", &Actor::Owner, &o, &OrgChange { visibility: Some(Visibility::Public), ..Default::default() }).unwrap();
    assert_eq!(onet::browse(&b.store, b.node(), &a.addr(), None, &[]).unwrap().len(), 1);

    // ben applies; anna sees the application with its note
    let j = onet::apply(&b.store, b.node(), &b.kp, &a.addr(), "research-lab", "I study comets", "ben", &[], "cli").unwrap();
    assert_eq!(j.status, MemberStatus::Applied);
    let ben = wait(|| a.store.member_by_peer(&o.id, &b.peer_id()).unwrap());
    assert_eq!((ben.status, ben.note.as_str()), (MemberStatus::Applied, "I study comets"));
    assert!(onet::apply(&b.store, b.node(), &b.kp, &a.addr(), "research-lab", "", "ben", &[], "cli").is_err(), "one application at a time");
    assert_eq!(onet::sync(&b.store, b.node(), &j, &[], "cli").unwrap().status, MemberStatus::Applied);
    // anna approves; the answer reaches ben's node, which joins
    let (ben, delivered) = onet::approve(&a.store, Some(a.node()), &a.kp, &Actor::Owner, &o, &ben, None, "cli").unwrap();
    assert!(matches!(delivered, Some(Ok(_))), "{delivered:?}");
    assert_eq!((ben.status, org::role_name(&a.store, &ben).as_str()), (MemberStatus::Member, "user"));
    wait(|| b.store.joined_org(&o.id).unwrap().filter(|j| j.status == MemberStatus::Member));
    let j = onet::sync(&b.store, b.node(), &b.store.joined_org(&o.id).unwrap().unwrap(), &[], "cli").unwrap();
    assert_eq!((j.role.as_str(), j.perms.contains(&"call".to_string())), ("user", true));

    // resources: ben sees anna's, gives one of his own, cannot manage
    let notes = a.store.create_journal("lab notes", None, EntryFormat::Markdown).unwrap();
    a.store.add_entry(&notes.id, "Comet", "a bright comet", None).unwrap();
    org::transfer(&a.store, "cli", &Actor::Owner, &o, ShareKind::Journal, "lab notes", None).unwrap();
    let v = onet::request(&b.store, b.node(), &j, onet::OrgReq::Resources, &[]).unwrap();
    assert_eq!(v["resources"].as_array().unwrap().len(), 1);
    let it = onet::fetch(&b.store, b.node(), &j, "lab notes", Some("html"), &[]).unwrap();
    assert!(String::from_utf8_lossy(&it.data).contains("bright comet"));
    let mine = b.store.create_journal("ben's comets", None, EntryFormat::Markdown).unwrap();
    b.store.add_entry(&mine.id, "Tail", "a long tail", None).unwrap();
    onet::transfer(&b.store, b.node(), &j, ShareKind::Journal, "ben's comets", &[], "cli").unwrap();
    let given = a.store.resources(&o.id).unwrap().into_iter().find(|r| r.transferred_by.as_deref() == Some(ben.id.as_str())).expect("transferred");
    assert!(a.store.find_journal(&given.ref_id).is_ok(), "the org's node holds a copy");
    let no = onet::request(
        &b.store,
        b.node(),
        &j,
        onet::OrgReq::Manage { op: onet::ManageOp::Publish { resource: given.id.clone(), to: crate::store::Published::Public } },
        &[],
    );
    assert!(no.unwrap_err().to_string().contains("may not"));
    let members = onet::request(&b.store, b.node(), &j, onet::OrgReq::Members, &[]).unwrap();
    assert_eq!(members["members"].as_array().unwrap().len(), 2);

    // cleo is invited as a viewer and takes the invitation
    let viewer = a.store.find_role(&o.id, "viewer").unwrap();
    let (inv, made) = onet::invite(&a.store, Some(a.node()), &a.kp, &Actor::Owner, &o, Some(&viewer), Some(&c.addr()), "cleo", &[], "cli").unwrap();
    assert!(matches!(made.delivered, Some(Ok(_))), "{:?}", made.delivered);
    assert_eq!(inv.status, MemberStatus::Invited);
    let acc =
        client::accept(&c.store, Some(c.node()), &c.kp, &made.exchange.id, "", "cleo", client::my_addrs(&c.store, Some(c.node())), "cli").unwrap();
    assert!(matches!(acc.delivered, Some(Ok(_))), "{:?}", acc.delivered);
    let cleo = wait(|| a.store.member_by_peer(&o.id, &c.peer_id()).unwrap().filter(|m| m.status == MemberStatus::Member));
    assert_eq!(org::role_name(&a.store, &cleo), "viewer");
    let jc = onet::sync(&c.store, c.node(), &c.store.joined_org(&o.id).unwrap().expect("cleo joined"), &[], "cli").unwrap();
    assert_eq!(jc.role, "viewer");
    assert!(onet::request(&c.store, c.node(), &jc, onet::OrgReq::Call { endpoint: "x".into(), input: serde_json::json!({}) }, &[]).is_err());

    // dora applies and is turned down
    let jd = onet::apply(&d.store, d.node(), &d.kp, &a.addr(), &o.id, "", "dora", &[], "cli").unwrap();
    let dora = wait(|| a.store.member_by_peer(&o.id, &d.peer_id()).unwrap());
    onet::reject(&a.store, "cli", &Actor::Owner, &o, &dora).unwrap();
    assert_eq!(onet::sync(&d.store, d.node(), &jd, &[], "cli").unwrap().status, MemberStatus::Rejected);

    // ben is kicked; what he gave stays with the org, archived, and his node is told
    onet::kick(&a.store, Some(a.node()), &Actor::Owner, &o, &ben, Leftovers::Archive, &[], "cli").unwrap();
    wait(|| b.store.joined_org(&o.id).unwrap().filter(|j| j.status == MemberStatus::Kicked));
    let given = a.store.resource(&given.id).unwrap().unwrap();
    assert_eq!(given.state, crate::store::ResourceState::Archived);
    assert!(onet::request(&b.store, b.node(), &j, onet::OrgReq::Status, &[]).is_err());
    // cleo leaves
    onet::leave(&c.store, Some(c.node()), &jc, &[], "cli").unwrap();
    assert_eq!(a.store.member(&cleo.id).unwrap().unwrap().status, MemberStatus::Left);
    assert!(a.store.actions().unwrap().iter().any(|x| x.kind == "org.member.kick"));
}

#[test]
fn an_admin_on_another_node_manages_members_remotely() {
    use crate::org::{self, Actor, net as onet};
    use crate::store::MemberStatus;
    let a = Peer::new("anna");
    let b = Peer::new("ben");
    let c = Peer::new("cleo");
    let o = org::create(&a.store, "cli", "Lab", None, "").unwrap();
    let admin = a.store.find_role(&o.id, "admin").unwrap();
    let (_, made) = onet::invite(&a.store, Some(a.node()), &a.kp, &Actor::Owner, &o, Some(&admin), Some(&b.addr()), "ben", &[], "cli").unwrap();
    client::accept(&b.store, Some(b.node()), &b.kp, &made.exchange.id, "", "ben", client::my_addrs(&b.store, Some(b.node())), "cli").unwrap();
    wait(|| a.store.member_by_peer(&o.id, &b.peer_id()).unwrap().filter(|m| m.status == MemberStatus::Member));
    let jb = onet::sync(&b.store, b.node(), &b.store.joined_org(&o.id).unwrap().unwrap(), &[], "cli").unwrap();
    assert_eq!(jb.role, "admin");
    // cleo applies; ben (admin, on his own node) approves; cleo picks the answer up by polling
    let jc = onet::apply(&c.store, c.node(), &c.kp, &a.addr(), "lab", "", "cleo", &[], "cli").unwrap();
    let cleo = wait(|| a.store.member_by_peer(&o.id, &c.peer_id()).unwrap());
    let v = onet::request(
        &b.store,
        b.node(),
        &jb,
        onet::OrgReq::Manage { op: onet::ManageOp::Approve { member: cleo.id.clone(), role: Some("viewer".into()) } },
        &[],
    )
    .unwrap();
    assert_eq!(v["status"], "member");
    let jc = onet::sync(&c.store, c.node(), &jc, &[], "cli").unwrap();
    assert_eq!((jc.status, jc.role.as_str()), (MemberStatus::Member, "viewer"));
    // an admin cannot make someone admin, nor touch the owner
    let up = onet::ManageOp::Role { member: cleo.id.clone(), role: "admin".into() };
    assert!(onet::request(&b.store, b.node(), &jb, onet::OrgReq::Manage { op: up }, &[]).is_err());
    let owner = a.store.member_by_peer(&o.id, &a.peer_id()).unwrap().unwrap();
    let kick = onet::ManageOp::Kick { member: owner.id, resources: Default::default() };
    assert!(onet::request(&b.store, b.node(), &jb, onet::OrgReq::Manage { op: kick }, &[]).is_err());
    let snap = onet::request(&b.store, b.node(), &jb, onet::OrgReq::Snapshot, &[]).unwrap();
    assert_eq!(snap["org"]["slug"], "lab");
    // an admin may mirror the public site; the copy is served here, read-only
    onet::mirror(&b.store, b.node(), &jb, &[], "cli").unwrap();
    assert!(matches!(crate::org::serve::site(&b.store, "lab").unwrap(), Some(crate::org::serve::Site::Mirror(_))));
    assert_eq!(crate::org::serve::route(&b.store, "GET", "/o/lab/", b"", None).status, 200);
    assert_eq!(crate::org::serve::route(&b.store, "POST", "/o/lab/api/call/x", b"{}", None).status, 405);
}

#[test]
fn reviewers_join_by_link_comment_suggest_vote_and_see_the_merge() {
    use crate::review::{self, Ask, net as rnet};
    use crate::store::{ReviewItemKind, ReviewItemState, ReviewKind, ReviewerStatus, ReviewingStatus};
    use serde_json::json;
    let a = Peer::new("anna");
    let b = Peer::new("ben");
    let c = Peer::new("cleo");
    let j = a.store.create_journal("lab notes", None, EntryFormat::Markdown).unwrap();
    let e = a.store.add_entry(&j.id, "Comet", "a brite comet", None).unwrap();
    let r = review::request(&a.store, "cli", ReviewKind::Entry, &e.id, "Comet check", "typos?", &[]).unwrap();

    // a link (a scoped offer); ben takes it and his answer reaches anna's node
    let (inv, made) = rnet::invite(&a.store, Some(a.node()), &a.kp, &r, "ben", None, &[], "cli").unwrap();
    assert_eq!(inv.status, ReviewerStatus::Invited);
    assert_eq!(rnet::peek(&made.token).unwrap()["title"], json!("Comet check"));
    let (g, acc) = rnet::join(&b.store, Some(b.node()), &b.kp, &made.token, "", "cli").unwrap();
    assert!(matches!(acc.delivered, Some(Ok(_))), "{:?}", acc.delivered);
    assert_eq!((g.status, g.title.as_str()), (ReviewingStatus::Joined, "Comet check"));
    wait(|| a.store.reviewer(&inv.id).unwrap().filter(|v| v.status == ReviewerStatus::Joined));
    assert!(rnet::join(&b.store, Some(b.node()), &b.kp, &made.token, "", "cli").is_err(), "already reviewing");
    assert!(rnet::join(&c.store, Some(c.node()), &c.kp, &made.token, "", "cli").is_err(), "a link is for one reviewer");

    // ben views, comments, suggests and votes
    let g = rnet::sync(&b.store, b.node(), &g, &[]).unwrap();
    assert_eq!(g.allow.len(), 5);
    let v = rnet::view(&b.store, b.node(), &g, &[]).unwrap();
    assert_eq!(v["content"]["body"], json!("a brite comet"));
    rnet::add(&b.store, b.node(), &g, &Ask { kind: ReviewItemKind::Comment, body: "lovely".into(), ..Default::default() }, &[], "cli").unwrap();
    let sug = rnet::add(
        &b.store,
        b.node(),
        &g,
        &Ask { kind: ReviewItemKind::Suggestion, at: Some("brite".into()), value: Some(json!("bright")), ..Default::default() },
        &[],
        "cli",
    )
    .unwrap();
    assert_eq!(sug.who, "ben");
    rnet::add(&b.store, b.node(), &g, &Ask { kind: ReviewItemKind::Vote, value: Some(json!("changes")), ..Default::default() }, &[], "cli").unwrap();
    let bad = rnet::add(
        &b.store,
        b.node(),
        &g,
        &Ask { kind: ReviewItemKind::Suggestion, field: "nope".into(), value: Some(json!(1)), ..Default::default() },
        &[],
        "cli",
    );
    assert!(bad.is_err());

    // anna accepts and merges; ben sees it merged
    review::decide(&a.store, "cli", &r, &[], true).unwrap();
    let m = review::merge(&a.store, "cli", &r, false).unwrap();
    assert_eq!(m.revisions[0].rev, 1);
    assert_eq!(a.store.entry(&e.id).unwrap().unwrap().body, "a bright comet");
    let t = rnet::thread(&b.store, b.node(), &g, &[]).unwrap();
    assert_eq!(t.len(), 3);
    assert_eq!(t.iter().find(|i| i.id == sug.id).unwrap().state, ReviewItemState::Merged);
    assert!(a.store.actions().unwrap().iter().any(|x| x.kind == "review.serve"));

    // cleo may only comment on a second review, then leaves it
    let r2 = review::request(&a.store, "cli", ReviewKind::Journal, "lab notes", "", "", &["comment".into()]).unwrap();
    let (_, made2) = rnet::invite(&a.store, Some(a.node()), &a.kp, &r2, "cleo", None, &[], "cli").unwrap();
    let (gc, _) = rnet::join(&c.store, Some(c.node()), &c.kp, &made2.token, "", "cli").unwrap();
    wait(|| a.store.reviewers(&r2.id).unwrap().into_iter().find(|v| v.status == ReviewerStatus::Joined));
    let no = rnet::add(&c.store, c.node(), &gc, &Ask { kind: ReviewItemKind::Vote, value: Some(json!("approve")), ..Default::default() }, &[], "cli");
    assert!(no.unwrap_err().to_string().contains("may not vote"));
    rnet::add(
        &c.store,
        c.node(),
        &gc,
        &Ask { kind: ReviewItemKind::Comment, target: e.id.clone(), body: "ok".into(), ..Default::default() },
        &[],
        "cli",
    )
    .unwrap();
    rnet::leave(&c.store, Some(c.node()), &gc, &[], "cli").unwrap();
    assert_eq!(a.store.reviewers(&r2.id).unwrap()[0].status, ReviewerStatus::Left);

    // anna closes the first review: ben's exchange ends and his node hears of it
    rnet::close(&a.store, Some(a.node()), &r, &[], "cli").unwrap();
    wait(|| b.store.reviewing_by_exchange(g.exchange_id.as_deref().unwrap()).unwrap().filter(|g| g.status == ReviewingStatus::Closed));
    assert!(rnet::view(&b.store, b.node(), &g, &[]).is_err());
}

#[test]
fn connecting_is_a_request_the_other_side_accepts_and_then_it_just_works() {
    let a = Peer::new("anna");
    let b = Peer::new("ben");
    let l = a.connect(&b);
    assert_eq!(l.status, LinkStatus::Pending, "{}", l.detail);
    assert!(l.warning.is_none());
    // ben sees a request, not a token
    let reqs = wait(|| Some(auto::requests(&b.store).unwrap()).filter(|r| !r.is_empty()));
    assert_eq!(reqs[0].title(), "anna");
    let ok = auto::accept(&b.store, Some(b.node()), &b.kp, "anna", &b.label, client::my_addrs(&b.store, Some(b.node())), "cli").unwrap();
    assert_eq!(ok.status, LinkStatus::Connected, "{}", ok.detail);
    let mine = wait(|| auto::node_link(&a.store, &b.peer_id()).unwrap().filter(|x| x.status.active()));
    assert_eq!(auto::status_of(&mine), LinkStatus::Connected);
    // asking again is a no-op
    assert!(a.connect(&b).detail.contains("already connected"));
    // nothing to settle
    assert!(auto::settle(&a.store, a.node(), &[], "cli").is_empty());
}

#[test]
fn asking_each_other_connects_and_open_nodes_answer_at_once() {
    let a = Peer::new("anna");
    let b = Peer::new("ben");
    assert_eq!(a.connect(&b).status, LinkStatus::Pending);
    wait(|| Some(auto::requests(&b.store).unwrap()).filter(|r| !r.is_empty()));
    // ben connecting back is his yes
    let l = b.connect(&a);
    assert_eq!(l.status, LinkStatus::Connected, "{}", l.detail);
    wait(|| auto::node_link(&a.store, &b.peer_id()).unwrap().filter(|x| x.status.active()));
    // a node that accepts anyone answers on the spot
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("db"), None).unwrap();
    let kp = identity::load_or_create(&store).unwrap();
    let mut c = Peer { dir, store, node: None, kp, label: "cara".into(), anyone: true };
    c.start(false, vec![]);
    let l = a.connect(&c);
    assert_eq!(l.status, LinkStatus::Connected, "{}", l.detail);
    assert!(c.store.actions().unwrap().iter().any(|x| x.kind == "net.exchange.auto"));
    // both ends agree on the key
    let x = l.exchange.unwrap();
    assert_eq!(a.store.exchange_key(&x.id).unwrap().unwrap().0, c.store.exchange_key(&x.id).unwrap().unwrap().0);
}

#[test]
fn declined_requests_end_and_unreachable_nodes_fail_plainly() {
    let a = Peer::new("anna");
    let mut b = Peer::new("ben");
    assert_eq!(a.connect(&b).status, LinkStatus::Pending);
    wait(|| Some(auto::requests(&b.store).unwrap()).filter(|r| !r.is_empty()));
    auto::decline(&b.store, "anna", "cli").unwrap();
    assert!(auto::requests(&b.store).unwrap().is_empty());
    let lines = auto::settle(&a.store, a.node(), &[], "cli");
    assert!(lines.iter().any(|l| l.starts_with("declined")), "{lines:?}");
    assert!(auto::node_link(&a.store, &b.peer_id()).unwrap().is_none(), "no open link");
    let x = a.store.exchanges().unwrap().into_iter().find(|x| x.peer_id.as_deref() == Some(b.peer_id().as_str())).unwrap();
    assert_eq!(auto::status_of(&x), LinkStatus::Ended);
    // a node that is gone
    let addr = b.addr();
    b.node = None;
    let l = auto::connect(&a.store, a.node(), &a.kp, &addr, None, "anna", vec![], &[], "cli").unwrap();
    assert_eq!(l.status, LinkStatus::Failed);
    assert!(!l.detail.is_empty());
}

#[test]
fn answers_made_offline_are_delivered_by_settle() {
    let a = Peer::new("anna");
    let b = Peer::new("ben");
    assert_eq!(a.connect(&b).status, LinkStatus::Pending);
    let inc = wait(|| auto::requests(&b.store).unwrap().into_iter().next());
    // accepted without a node: the answer stays here
    let l = auto::accept(&b.store, None, &b.kp, &inc.id, "ben", vec![], "cli").unwrap();
    assert_eq!(l.status, LinkStatus::Connected);
    assert_eq!(b.store.undelivered_answers().unwrap().len(), 1);
    let lines = auto::settle(&b.store, b.node(), &[], "cli");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(b.store.undelivered_answers().unwrap().is_empty());
    wait(|| auto::node_link(&a.store, &b.peer_id()).unwrap().filter(|x| x.status.active()));
}

#[test]
fn a_new_node_using_a_known_name_is_flagged() {
    let a = Peer::new("anna");
    let b = Peer::new("ben");
    a.exchange_with(&b);
    let other = Peer::new("ben");
    let l = a.connect(&other);
    let w = l.warning.expect("warned");
    assert!(w.contains("ben"), "{w}");
    // names still mean the pinned node
    let (p, _) = auto::resolve(&a.store, "ben", &[]).unwrap();
    assert_eq!(p.to_string(), b.peer_id());
}

// ---------- users and connections (2.10) ----------

#[test]
fn connected_nodes_learn_each_others_users_as_untrusted_connections() {
    let a = Peer::new("anna");
    let b = Peer::new("ben");
    assert_eq!(a.connect(&b).status, LinkStatus::Pending);
    wait(|| Some(auto::requests(&b.store).unwrap()).filter(|r| !r.is_empty()));
    auto::accept(&b.store, Some(b.node()), &b.kp, "anna", &b.label, vec![], "cli").unwrap();
    wait(|| auto::node_link(&a.store, &b.peer_id()).unwrap().filter(|x| x.status.active()));
    // the next settle introduces (quietly)
    assert!(auto::settle(&a.store, a.node(), &[], "cli").is_empty());
    let ua = user::my_card(&a.store).unwrap();
    let ub = user::my_card(&b.store).unwrap();
    let ca = wait(|| a.store.connection(&ub.id).unwrap());
    assert!(!ca.trusted);
    let cb = wait(|| b.store.connection(&ua.id).unwrap());
    assert!(!cb.trusted);
    assert_eq!(user::user_of_peer(&a.store, &b.peer_id()).unwrap().unwrap().id, ub.id);
    assert!(a.store.actions().unwrap().iter().any(|x| x.kind == "connection.add"));
    // not again until the card changes
    assert!(a.store.setting(&auto::intro_key(&ca_exchange(&a, &b))).unwrap().is_some_and(|v| !v.starts_with("retry")));
    // trust is recorded
    assert!(!user::trusted_peer(&a.store, &b.peer_id()).unwrap());
    let (_, changed) = user::set_trust(&a.store, &ub.id, true, "cli").unwrap();
    assert!(changed);
    assert!(user::trusted_peer(&a.store, &b.peer_id()).unwrap());
    assert!(!user::set_trust(&a.store, &ub.id, true, "cli").unwrap().1);
    assert!(a.store.actions().unwrap().iter().any(|x| x.kind == "connection.trust"));
    user::remove_connection(&a.store, &ub.id, "cli").unwrap();
    assert!(a.store.connection(&ub.id).unwrap().is_none());
}

fn ca_exchange(a: &Peer, b: &Peer) -> String {
    auto::node_link(&a.store, &b.peer_id()).unwrap().unwrap().id
}

#[test]
fn a_node_links_to_my_user_after_approval_and_can_be_revoked() {
    let a = Peer::new("laptop");
    let b = Peer::new("ben");
    let c = Peer::new("phone");
    // a and ben know each other
    assert_eq!(a.connect(&b).status, LinkStatus::Pending);
    wait(|| Some(auto::requests(&b.store).unwrap()).filter(|r| !r.is_empty()));
    auto::accept(&b.store, Some(b.node()), &b.kp, "laptop", &b.label, vec![], "cli").unwrap();
    wait(|| auto::node_link(&a.store, &b.peer_id()).unwrap().filter(|x| x.status.active()));
    auto::settle(&a.store, a.node(), &[], "cli");
    let me = user::my_card(&a.store).unwrap();
    wait(|| b.store.connection(&me.id).unwrap());

    // the phone asks to link; the laptop's owner has to approve
    let l = auto::link_to(&c.store, c.node(), &c.kp, &a.addr(), "phone", vec![], &[], "cli").unwrap();
    assert_eq!(l.status, LinkStatus::Pending, "{}", l.detail);
    let reqs = wait(|| Some(auto::requests(&a.store).unwrap()).filter(|r| !r.is_empty()));
    assert_eq!(auto::purpose(&reqs[0]), "link node");
    assert_ne!(user::my_card(&c.store).unwrap().id, me.id);
    auto::accept(&a.store, Some(a.node()), &a.kp, "phone", &a.label, vec![], "cli").unwrap();
    wait(|| auto::kind_link(&c.store, &a.peer_id(), "link").unwrap().filter(|x| x.status.active()));
    let lines = auto::settle(&c.store, c.node(), &[], "cli");
    assert!(lines.iter().any(|l| l.starts_with("linked")), "{lines:?}");
    let cu = c.store.self_user().unwrap().unwrap();
    assert_eq!(cu.id, me.id);
    assert!(c.store.self_user_keypair().unwrap().is_some());
    assert!(a.store.user_nodes(&me.id).unwrap().iter().any(|n| n.peer_id == c.peer_id() && n.active()));
    // the laptop's card changed: ben hears of the phone with the next settle
    auto::settle(&a.store, a.node(), &[], "cli");
    wait(|| b.store.user_of_node(&c.peer_id()).unwrap());

    // ben's org knows the laptop as a member: the phone, the same user, acts with that role
    let o = crate::org::create(&b.store, "cli", "Comet lab", None, "").unwrap();
    let role = b.store.find_role(&o.id, crate::org::USER).unwrap();
    b.store.upsert_member(&o.id, &a.peer_id(), "laptop", Some(&role.id), crate::store::MemberStatus::Member, None, "", Some("owner")).unwrap();
    b.store.tag_members(&a.peer_id(), &me.id).unwrap();
    let bx = auto::node_link(&b.store, &a.peer_id()).unwrap().unwrap();
    let mut bs = Store::open(&b.db(), None).unwrap();
    let phone: libp2p::PeerId = c.peer_id().parse().unwrap();
    let st = crate::org::net::serve(&mut bs, &bx, &phone, &o.id, crate::org::net::OrgReq::Status);
    assert!(st.is_ok(), "{:?}", st.err());
    let stranger: libp2p::PeerId = Keypair::generate_ed25519().public().to_peer_id();
    assert!(crate::org::net::serve(&mut bs, &bx, &stranger, &o.id, crate::org::net::OrgReq::Status).is_err());

    // revoke the phone from the laptop: ben learns it, and the phone's card is refused
    user::revoke_node(&a.store, &c.peer_id(), "cli").unwrap();
    assert!(user::revoke_node(&a.store, &a.peer_id(), "cli").is_err(), "not the node you are on");
    auto::settle(&a.store, a.node(), &[], "cli");
    wait(|| b.store.user_of_node(&c.peer_id()).unwrap().is_none().then_some(()));
    let card = user::my_card(&c.store).unwrap();
    assert!(user::take_card(&b.store, &c.peer_id(), &card, "cli").is_err());
    // and a revoked node no longer acts for the user in orgs
    assert!(crate::org::net::serve(&mut bs, &bx, &phone, &o.id, crate::org::net::OrgReq::Status).is_err());
}

#[test]
fn cards_need_a_certificate_for_the_sending_node() {
    let a = Peer::new("anna");
    let b = Peer::new("ben");
    let card = user::my_card(&a.store).unwrap();
    // claimed by another node
    assert!(user::take_card(&b.store, "12D3KooWnotanna", &card, "cli").is_err());
    // the real sender
    assert!(matches!(user::take_card(&b.store, &a.peer_id(), &card, "cli").unwrap(), user::Met::Connection { new: true, .. }));
    // a key that is not the user's
    let mut bad = card.clone();
    bad.key = user::my_card(&b.store).unwrap().key;
    assert!(user::take_card(&b.store, &a.peer_id(), &bad, "cli").is_err());
}

#[test]
fn linked_nodes_sync_everything_keep_conflicts_fetch_bars_and_stop_when_revoked() {
    use super::sync;
    use crate::model::{Bar, Duration as BarDuration, Symbol};
    let a = Peer::new("laptop");
    let c = Peer::new("phone");
    let stranger = Peer::new("ben");
    // the phone links to the laptop's user
    auto::link_to(&c.store, c.node(), &c.kp, &a.addr(), "phone", vec![], &[], "cli").unwrap();
    wait(|| Some(auto::requests(&a.store).unwrap()).filter(|r| !r.is_empty()));
    auto::accept(&a.store, Some(a.node()), &a.kp, "phone", &a.label, vec![], "cli").unwrap();
    wait(|| auto::kind_link(&c.store, &a.peer_id(), "link").unwrap().filter(|x| x.status.active()));
    auto::settle(&c.store, c.node(), &[], "cli");
    assert!(c.store.self_user().unwrap().is_some());

    // what the laptop has reaches the phone, sealed with the user's sync key
    let j = a.store.create_journal("notes", None, EntryFormat::Markdown).unwrap();
    let mut a_bars = Store::open(&a.db(), None).unwrap();
    let src = a_bars.get_or_create_source("binance", "api", None).unwrap();
    let sym = Symbol { base: "BTC".into(), quote: "USDT".into() };
    let bars: Vec<Bar> = (0..5).map(|i| Bar { ts: 60_000 * i, open: 1.0, high: 2.0, low: 0.5, close: 1.5, volume: 10.0, flags: 0 }).collect();
    a_bars.upsert_bars(&src, &sym, BarDuration(60), &bars).unwrap();
    let nodes = sync::linked(&c.store).unwrap();
    assert_eq!(nodes.len(), 1, "{nodes:?}");
    let x = nodes[0].1.clone().expect("exchange");
    let got = sync::pull(&c.store, c.node(), &x, &[], "cli").unwrap();
    assert!(got.taken >= 1, "{got:?}");
    assert_eq!(c.store.journal(&j.id).unwrap().unwrap().name, "notes");
    // bars stay put until asked for
    assert!(c.store.series().unwrap().is_empty());
    let mut c_bars = Store::open(&c.db(), None).unwrap();
    let series = sync::remote_series(&c.store, c.node(), &x, &[]).unwrap();
    assert_eq!(series.len(), 1, "{series:?}");
    assert_eq!(sync::pull_bars(&mut c_bars, c.node(), &x, &series[0], &[]).unwrap(), 5);
    assert_eq!(sync::pull_bars(&mut c_bars, c.node(), &x, &series[0], &[]).unwrap(), 0);

    // the laptop pulls back what the phone added (the assets the bars made), with no conflicts,
    // and the echo of that is quiet
    let ax = sync::linked(&a.store).unwrap().into_iter().find_map(|(n, x)| (n.peer_id == c.peer_id()).then_some(x).flatten()).unwrap();
    let back = sync::pull(&a.store, a.node(), &ax, &[], "cli").unwrap();
    assert_eq!(back.conflicts, 0, "{back:?}");
    let echo = sync::pull(&c.store, c.node(), &x, &[], "cli").unwrap();
    assert_eq!((echo.changed(), echo.conflicts), (0, 0), "{echo:?}");

    // both rename it: the newer is kept, the other saved as a conflict
    a.store.update_journal(&j.id, Some("from laptop"), None).unwrap();
    std::thread::sleep(Duration::from_millis(5));
    c.store.update_journal(&j.id, Some("from phone"), None).unwrap();
    let r = sync::pull(&a.store, a.node(), &ax, &[], "cli").unwrap();
    assert_eq!(r.conflicts, 1, "{r:?}");
    assert_eq!(a.store.journal(&j.id).unwrap().unwrap().name, "from phone");
    let open = a.store.sync_conflicts(false).unwrap();
    assert_eq!(open.len(), 1);
    assert!(open[0].mine.as_deref().unwrap_or_default().contains("from laptop"));
    let done = a.store.sync_resolve(&open[0].id, true).unwrap();
    assert!(done.resolved_at.is_some());
    assert_eq!(a.store.journal(&j.id).unwrap().unwrap().name, "from laptop");
    assert!(a.store.sync_conflicts(false).unwrap().is_empty());

    // a stranger is not served
    let sp: libp2p::PeerId = stranger.peer_id().parse().unwrap();
    let as_ = Store::open(&a.db(), None).unwrap();
    assert!(sync::serve(&as_, &sp, sync::SyncReq::Series).is_err());
    // a revoked node is not served any more
    user::revoke_node(&a.store, &c.peer_id(), "cli").unwrap();
    assert!(sync::pull(&c.store, c.node(), &x, &[], "cli").is_err());
}

#[test]
fn shared_models_arrive_as_drafts_to_approve() {
    let a = Store::open_in_memory().unwrap();
    let (m, _) = a.create_model("signals", "", Default::default(), false).unwrap();
    let d = crate::store::Design { fields: crate::designer::parse_fields(&a, &m, "price:real!; note:text").unwrap(), ..Default::default() };
    a.set_model_draft(&m.id, Some(d)).unwrap();
    a.apply_model(&m.id, &Default::default()).unwrap();
    let it = super::service::item_of(&a, ShareKind::Model, &m.id, "signals", None).unwrap();
    let mut b = Store::open_in_memory().unwrap();
    b.create_model("signals", "", Default::default(), true).unwrap();
    let (id, name, made) = client::import_item_as(&mut b, &it, "ada", "test").unwrap();
    assert_eq!(name, "signals_2");
    assert!(made.contains("mnnn model apply signals_2"), "{made}");
    let got = b.model(&id).unwrap().unwrap();
    assert_eq!(got.version, 0, "nothing applied until it is approved");
    assert_eq!(got.pending().fields.len(), 2);
    // a template file shared as JSON imports the same way
    let t = a.export_template("signals", false).unwrap();
    let it = super::proto::Item { kind: ShareKind::Model, name: "signals".into(), format: "json".into(), data: t.to_json().unwrap().into_bytes() };
    let (_, name, _) = client::import_item_as(&mut b, &it, "ada", "test").unwrap();
    assert_eq!(name, "signals_3");
}
