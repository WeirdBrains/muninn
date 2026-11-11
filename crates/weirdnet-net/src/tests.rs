use crate::exchange::Token;
use crate::profile::{Profile, WEIRDNET, invite_scope, link_scope, node_scope};
use crate::sealed::{self, Answered, Envelope, Exchanges, Reply};
use crate::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::{Arc, Mutex};

fn kp(n: u8) -> Keypair {
    Keypair::ed25519_from_bytes([n; 32]).unwrap()
}

/// Another application's profile: other ids, labels and scopes.
fn other_scopes(s: &serde_json::Value) -> Result<()> {
    if profile::scope_kind(s) == "node" { Ok(()) } else { Err(NetError::invalid("exchange", "only node exchanges here")) }
}
static OTHER: Profile = Profile {
    name: "other",
    protocol: "/other/net/1",
    kad: "/other/kad/1",
    relay_key: "/other/relay/1",
    identify: "/other/id/1",
    offer_prefix: "oth-offer:",
    answer_prefix: "oth-answer:",
    token_mark: "oth-",
    join_link: "other://join/",
    answer_link: "other://answer/",
    link_mark: "other://",
    offer_domain: "other/offer",
    answer_domain: "other/answer",
    transcript_domain: "other/exchange",
    key_info: "other/key",
    sas_info: "other/sas",
    msg_aad: "other/msg",
    cert: "other-cert",
    revoke: "other-revoke",
    sync_key: "other/sync/key",
    sync_aad: "other/sync",
    default_port: 9,
    thread: "other-net",
    scopes: other_scopes,
};

#[test]
fn weirdnet_defaults_carry_no_application_brand() {
    let p = WEIRDNET;
    for s in [
        p.protocol,
        p.kad,
        p.relay_key,
        p.identify,
        p.offer_prefix,
        p.answer_prefix,
        p.join_link,
        p.answer_link,
        p.offer_domain,
        p.msg_aad,
        p.cert,
        p.revoke,
        p.sync_key,
        p.sync_aad,
    ] {
        assert!(s.contains("weirdnet") || s.starts_with("wnet-"), "{s}");
        assert!(!s.to_lowercase().contains("muninn") && !s.contains("mnnn"), "{s}");
    }
    assert_eq!(p.agent("1.2.3", " lab "), "weirdnet/1.2.3 (lab)");
    assert_eq!(p.agent("1.2.3", ""), "weirdnet/1.2.3");
}

#[test]
fn both_sides_derive_the_same_key_code_and_fingerprint() {
    let p = &WEIRDNET;
    let (a, b) = (kp(1), kp(2));
    let (offer, secret) = p.offer(&a, "anna", vec!["/ip4/127.0.0.1/tcp/4747".into()], node_scope(), 1_000, DEFAULT_TTL_MS).unwrap();
    let tok = p.offer_token(&offer).unwrap();
    assert!(tok.starts_with("wnet-x1-offer:") && p.is_token(&tok));
    let link = p.link(&tok);
    assert!(link.starts_with("weirdnet://join/") && p.is_token(&link));
    let Token::Offer(o2) = p.parse_token(&format!(" {}\n{} ", &link[..20], &link[20..])).unwrap() else { panic!() };
    assert_eq!(o2, offer);
    assert_eq!(p.verify_offer(&o2, 2_000).unwrap(), a.public().to_peer_id());
    let (answer, d_b) = p.answer(&b, &o2, "ben", vec![], 2_000).unwrap();
    let Token::Answer(a2) = p.parse_token(&p.link(&p.answer_token(&answer).unwrap())).unwrap() else { panic!() };
    let d_a = p.complete(&a2, &offer, &secret[..]).unwrap();
    assert_eq!((d_a.key.0, &d_a.short_code, &d_a.fingerprint), (d_b.key.0, &d_b.short_code, &d_b.fingerprint));
    assert_eq!(normalize_code(&d_a.short_code.replace('-', " ")), d_a.short_code);
}

#[test]
fn tampering_expiry_scopes_and_self_answers_are_refused() {
    let p = &WEIRDNET;
    let (a, b, m) = (kp(1), kp(2), kp(9));
    let (offer, secret) = p.offer(&a, "anna", vec![], node_scope(), 1_000, 1).unwrap();
    assert_eq!(offer.expires, 61_000, "at least a minute");
    let mut bad = offer.clone();
    bad.addrs.push("/ip4/6.6.6.6/tcp/1".into());
    assert_eq!(p.verify_offer(&bad, 1_000).unwrap_err().to_string(), "invalid exchange: the offer's signature does not verify");
    let mut bad = offer.clone();
    bad.key = base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, b.public().encode_protobuf());
    assert!(p.verify_offer(&bad, 1_000).unwrap_err().to_string().contains("does not match the peer id"));
    assert!(p.verify_offer(&offer, 62_000).unwrap_err().to_string().contains("expired"));
    assert!(p.answer(&a, &offer, "", vec![], 1_000).is_err(), "own offer");
    let (answer, _) = p.answer(&b, &offer, "ben", vec![], 1_000).unwrap();
    let mut bad = answer.clone();
    bad.label = "mallory".into();
    assert!(p.complete(&bad, &offer, &secret[..]).is_err());
    let (other, _) = p.offer(&a, "anna", vec![], node_scope(), 1_000, 60_000).unwrap();
    assert!(p.complete(&answer, &other, &secret[..]).err().unwrap().to_string().contains("different offer"));
    // a man in the middle answering with his own key gets another short code
    let (_, d_m) = p.answer(&m, &offer, "ben", vec![], 1_000).unwrap();
    assert_ne!(p.complete(&answer, &offer, &secret[..]).unwrap().short_code, d_m.short_code);
    // invitations: shape checked, meaning left to the application
    assert!(p.offer(&a, "", vec![], invite_scope("garden", "member"), 0, 60_000).is_ok());
    assert!(p.offer(&a, "", vec![], link_scope(), 0, 60_000).is_ok());
    for bad in
        [json!({"kind": "invite", "to": "garden"}), json!({"kind": "invite", "to": "x".repeat(65), "as": "m"}), json!({"kind": "galaxy"}), json!({})]
    {
        assert!(p.offer(&a, "", vec![], bad, 0, 60_000).is_err());
    }
    assert!(p.parse_token("hello").unwrap_err().to_string().contains("wnet-x1-offer:"));
    assert_eq!(p.parse_token("wnet-x1-offer:***").unwrap_err().to_string(), "invalid token: damaged (base64)");
}

#[test]
fn profiles_do_not_mix() {
    let a = kp(1);
    let (o, _) = WEIRDNET.offer(&a, "", vec![], node_scope(), 0, 60_000).unwrap();
    let tok = WEIRDNET.offer_token(&o).unwrap();
    // another profile neither reads the token nor accepts the signature
    assert!(OTHER.parse_token(&tok).is_err() && !OTHER.is_token(&tok));
    assert!(OTHER.verify_offer(&o, 0).unwrap_err().to_string().contains("signature"));
    let (o2, _) = OTHER.offer(&a, "", vec![], node_scope(), 0, 60_000).unwrap();
    assert!(WEIRDNET.verify_offer(&o2, 0).is_err());
    assert!(OTHER.offer(&a, "", vec![], invite_scope("g", "m"), 0, 60_000).is_err(), "its own scopes");
    // sealed messages and certificates are bound to the profile's labels too
    let k = Key32([3; 32]);
    let (x, y) = (kp(1).public().to_peer_id(), kp(2).public().to_peer_id());
    let s = WEIRDNET.seal_msg(&k, "ex", &x, &y, b"hi").unwrap();
    assert_eq!(&WEIRDNET.open_msg(&k, "ex", &x, &y, &s).unwrap()[..], b"hi");
    assert!(OTHER.open_msg(&k, "ex", &x, &y, &s).is_err());
    assert!(WEIRDNET.open_msg(&k, "ex", &y, &x, &s).is_err());
    assert!(WEIRDNET.open_msg(&k, "ex2", &x, &y, &s).is_err());
    let u = kp(3);
    let id = user::user_id(&u.public());
    let c = WEIRDNET.certify(&u, &id, "P1", "", 5).unwrap();
    assert!(WEIRDNET.verify_cert(&u.public(), &c) && !OTHER.verify_cert(&u.public(), &c));
    assert_ne!(WEIRDNET.sync_key(&u, &id).unwrap().0, OTHER.sync_key(&u, &id).unwrap().0);
}

#[test]
fn certificates_cards_and_grants() {
    let p = &WEIRDNET;
    let (u, m) = (kp(3), kp(9));
    let id = user::user_id(&u.public());
    let (pa, pb) = (kp(1).public().to_peer_id().to_string(), kp(2).public().to_peer_id().to_string());
    let ca = p.certify(&u, &id, &pa, "a", 10).unwrap();
    let mut cb = p.certify(&u, &id, &pb, "b", 11).unwrap();
    assert!(p.verify_cert(&u.public(), &ca));
    cb.revoked_at = Some(20);
    assert!(!p.verify_cert(&u.public(), &cb), "a revocation time without its signature");
    cb.revocation = Some(p.revoke(&u, &id, &pb, 20).unwrap());
    assert!(p.verify_cert(&u.public(), &cb));
    cb.revoked_at = Some(21);
    assert!(!p.verify_cert(&u.public(), &cb));
    cb.revoked_at = Some(20);
    let card = user::Card { id: id.clone(), label: "me".into(), key: u.public().encode_protobuf(), nodes: vec![ca.clone(), cb.clone()] };
    assert_eq!(p.check_card(&pa, &card).unwrap(), u.public());
    assert!(p.check_card(&pb, &card).unwrap_err().to_string().contains("no valid certificate"), "revoked sender");
    let mut forged = card.clone();
    forged.key = m.public().encode_protobuf();
    assert!(p.check_card(&pa, &forged).unwrap_err().to_string().contains("does not match"));
    let mut other_cert = card.clone();
    other_cert.nodes = vec![p.certify(&m, &id, &pa, "", 10).unwrap()];
    assert!(p.check_card(&pa, &other_cert).is_err());
    // a grant to a node the card certifies, from the user's own node
    let pc = kp(4).public().to_peer_id().to_string();
    let mut grant = card.clone();
    grant.nodes.push(p.certify(&u, &id, &pc, "c", 12).unwrap());
    let bytes = u.to_protobuf_encoding().unwrap();
    assert_eq!(p.check_grant(&pc, &pa, &grant, &bytes).unwrap().public(), u.public());
    assert!(p.check_grant(&pc, &pb, &grant, &bytes).unwrap_err().to_string().contains("approving node"));
    assert!(p.check_grant("NOPE", &pa, &grant, &bytes).unwrap_err().to_string().contains("this node"));
    assert!(p.check_grant(&pc, &pa, &grant, &m.to_protobuf_encoding().unwrap()).unwrap_err().to_string().contains("does not match"));
    assert_ne!(card.digest(), grant.digest());
}

#[test]
fn identities_are_made_once_by_whoever_stores_first() {
    #[derive(Default)]
    struct Mem(Mutex<Option<(Vec<u8>, String)>>);
    impl identity::IdentityStore for Mem {
        type Error = NetError;
        fn node_identity(&self) -> Result<Option<(zeroize::Zeroizing<Vec<u8>>, String)>> {
            Ok(self.0.lock().unwrap().clone().map(|(b, p)| (zeroize::Zeroizing::new(b), p)))
        }
        fn init_node_identity(&self, keypair: &[u8], peer: &str) -> Result<()> {
            self.0.lock().unwrap().get_or_insert((keypair.to_vec(), peer.to_string()));
            Ok(())
        }
    }
    let s = Mem::default();
    assert!(identity::peer_id(&s).unwrap().is_none() && identity::fingerprint_of(&s).unwrap().is_none());
    let a = identity::load_or_create(&s).unwrap();
    assert_eq!(a.public(), identity::load_or_create(&s).unwrap().public());
    assert_eq!(identity::peer_id(&s).unwrap(), Some(a.public().to_peer_id()));
    assert_eq!(identity::fingerprint_of(&s).unwrap().unwrap().len(), 24);
}

// ---------- two nodes on the in-memory transport ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
enum Ask {
    Hi,
    Sealed { exchange: String, sealed: Sealed },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
enum Say {
    Hi(String),
    Sealed(Sealed),
    No(String),
}
impl Envelope for Ask {
    fn sealed(exchange: String, sealed: Sealed) -> Self {
        Ask::Sealed { exchange, sealed }
    }
}
impl Reply for Say {
    fn sealed(sealed: Sealed) -> Self {
        Say::Sealed(sealed)
    }
    fn into_sealed(self) -> std::result::Result<Sealed, Self> {
        match self {
            Say::Sealed(s) => Ok(s),
            o => Err(o),
        }
    }
}

struct Book {
    id: String,
    peer: String,
    key: Key32,
    active: bool,
}
impl Exchanges for Book {
    type Exchange = (String, String, bool);
    type Error = NetError;
    fn exchange(&self, id: &str) -> Result<Option<Self::Exchange>> {
        Ok((id == self.id).then(|| (self.id.clone(), self.peer.clone(), self.active)))
    }
    fn active(x: &Self::Exchange) -> bool {
        x.2
    }
    fn peer(x: &Self::Exchange) -> Option<&str> {
        Some(&x.1)
    }
    fn key(&self, _: &Self::Exchange) -> Result<Option<Key32>> {
        Ok(Some(self.key.clone()))
    }
}

struct App {
    me: PeerId,
    book: Mutex<Option<Book>>,
}
impl Handler<Ask, Say> for App {
    fn request(&self, from: &PeerId, req: Ask) -> Say {
        match req {
            Ask::Hi => Say::Hi(format!("hi {}", short_peer_id(from))),
            Ask::Sealed { exchange, sealed } => {
                let mut g = self.book.lock().unwrap();
                let Some(b) = g.as_mut() else { return Say::No("no exchanges".into()) };
                match sealed::serve(&WEIRDNET, b, &self.me, from, &exchange, &sealed, |_, _, plain| Ok([b"echo:".as_slice(), plain].concat())) {
                    Ok(s) => Say::Sealed(s),
                    Err(e) => Say::No(e.to_string()),
                }
            }
        }
    }
}

fn node(k: &Keypair) -> (Node<Ask, Say>, Arc<App>) {
    let app = Arc::new(App { me: k.public().to_peer_id(), book: Mutex::new(None) });
    let cfg = NodeConfig { listen: vec!["/memory/0".parse().unwrap()], memory: true, label: "t".into(), ..Default::default() };
    (Node::start(&WEIRDNET, k.clone(), cfg, app.clone()).unwrap(), app)
}

#[test]
fn two_nodes_exchange_keys_and_talk_sealed() {
    let (ka, kb, km) = (kp(11), kp(12), kp(13));
    let (na, _) = node(&ka);
    let (nb, app_b) = node(&kb);
    let addr: Multiaddr = nb.reachable_addrs()[0].parse().unwrap();
    assert_eq!(na.dial(addr.clone()).unwrap(), kb.public().to_peer_id());
    match na.request(kb.public().to_peer_id(), std::slice::from_ref(&addr), Ask::Hi).unwrap() {
        Say::Hi(s) => assert_eq!(s, format!("hi {}", short_peer_id(&ka.public().to_peer_id()))),
        o => panic!("{o:?}"),
    }
    // A invites B, B answers, both derive the key
    let p = &WEIRDNET;
    let (offer, secret) = p.offer(&ka, "a", na.reachable_addrs(), invite_scope("garden", "member"), now_ms(), DEFAULT_TTL_MS).unwrap();
    let Token::Offer(o) = p.parse_token(&p.link(&p.offer_token(&offer).unwrap())).unwrap() else { panic!() };
    p.verify_offer(&o, now_ms()).unwrap();
    let (answer, d_b) = p.answer(&kb, &o, "b", vec![], now_ms()).unwrap();
    let d_a = p.complete(&answer, &offer, &secret[..]).unwrap();
    let ida = ka.public().to_peer_id().to_string();
    *app_b.book.lock().unwrap() = Some(Book { id: offer.id.clone(), peer: ida.clone(), key: d_b.key.clone(), active: true });
    let x = sealed::Exchange { id: &offer.id, key: &d_a.key, peer: kb.public().to_peer_id() };
    match sealed::call(p, &na, &x, std::slice::from_ref(&addr), b"ping").unwrap() {
        Answered::Opened(b) => assert_eq!(&b[..], b"echo:ping"),
        Answered::Other(o) => panic!("{o:?}"),
    }
    // the wrong key fails to authenticate on B, an unknown exchange is refused
    let wrong = Key32([0; 32]);
    let refused = |x: &sealed::Exchange| match sealed::call(p, &na, x, std::slice::from_ref(&addr), b"ping").unwrap() {
        Answered::Other(Say::No(m)) => m,
        _ => panic!("not refused"),
    };
    assert_eq!(refused(&sealed::Exchange { id: &offer.id, key: &wrong, peer: x.peer }), "crypto: a message failed to authenticate");
    assert_eq!(refused(&sealed::Exchange { id: "nope", key: &d_a.key, peer: x.peer }), "invalid exchange: unknown exchange");
    // another node holding the exchange id is not its peer
    let (nm, _) = node(&km);
    let xm = sealed::Exchange { id: &offer.id, key: &d_a.key, peer: x.peer };
    match sealed::call(p, &nm, &xm, std::slice::from_ref(&addr), b"ping").unwrap() {
        Answered::Other(Say::No(m)) => assert_eq!(m, "invalid exchange: this exchange belongs to another node"),
        _ => panic!("not refused"),
    }
    app_b.book.lock().unwrap().as_mut().unwrap().active = false;
    assert!(refused(&x).contains("not active"));
    let st = na.status();
    assert_eq!(st.peer_id, ida);
    assert!(st.connected.contains(&kb.public().to_peer_id().to_string()));
    na.stop();
    assert!(nb.running());
}

#[test]
fn address_helpers() {
    use crate::addr::*;
    let m = |s: &str| -> Multiaddr { s.parse().unwrap() };
    assert!(!shareable(&m("/ip4/0.0.0.0/tcp/1")) && !shareable(&m("/ip4/172.17.0.2/tcp/1")) && shareable(&m("/ip4/127.0.0.1/tcp/1")));
    assert!(routable(&m("/ip4/8.8.8.8/tcp/1")) && !routable(&m("/ip4/10.1.1.1/tcp/1")) && routable(&m("/dns4/x.example/tcp/1")));
    assert!(reach_rank(&m("/dns4/x/tcp/1")) < reach_rank(&m("/ip4/8.8.8.8/tcp/1")));
    assert!(reach_rank(&m("/ip4/8.8.8.8/tcp/1")) < reach_rank(&m("/ip4/8.8.8.8/udp/1/quic-v1")));
    assert_eq!(parse_addrs(&["/ip4/1.2.3.4/tcp/1", "junk"]).len(), 1);
    let pid = kp(1).public().to_peer_id();
    assert_eq!(addr_peer(&m(&format!("/ip4/1.2.3.4/tcp/1/p2p/{pid}"))), Some(pid));
}
