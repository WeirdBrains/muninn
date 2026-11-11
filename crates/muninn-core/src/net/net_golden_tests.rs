//! Golden (old-version) tests for Muninn's network formats, captured on e0c9bbe0 (v2.23.0
//! formats) before the network code moved into weirdnet-net. Everything Muninn signs,
//! derives, seals or puts in a token is built here from fixed synthetic keys, so the bytes
//! are deterministic: offers and answers (tokens, links, hashes, signatures), the derived
//! exchange key, short code and fingerprint, node certificates and revocations, cards, the
//! sync key and the JSON of the sealed requests that carry them, plus every refusal message.
//! Random nonces can't be pinned, so sealed messages captured on the old code are opened
//! instead. None of these keys is real.
use super::client::one_circuit_per_relay;
use super::exchange::*;
use super::node::{reach_rank, routable, shareable};
use super::proto::{Req, Resp, ShardBlob, Wire, WireResp};
use super::user::{Card, cert_bytes, revoke_bytes, user_id, verify};
use crate::crypto::{self, Key32, Sealed};
use crate::store::Store;
use crate::store::users::UserNode;
use base64::Engine;
use libp2p::Multiaddr;
use libp2p::identity::Keypair;
use serde_json::{Value, json};
use weirdnet_keys::exchange::ExchangeSecret;

const GOLDEN: &str = include_str!("net_golden.json");

fn b64() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
}

fn kp(n: u8) -> Keypair {
    Keypair::ed25519_from_bytes([n; 32]).unwrap()
}

fn signed(domain: &[u8], v: &impl serde::Serialize) -> Vec<u8> {
    let mut b = domain.to_vec();
    b.push(0);
    b.extend(serde_json::to_vec(v).unwrap());
    b
}

fn err<T>(r: crate::error::Result<T>) -> String {
    match r {
        Ok(_) => "ok".into(),
        Err(e) => e.to_string(),
    }
}

/// A signed offer with fixed fields (Offer::new picks a random id and key).
fn offer(a: &Keypair, secret: &ExchangeSecret, id: &str, scope: Value, created: i64) -> Offer {
    let mut o = Offer {
        v: 1,
        id: id.into(),
        peer: a.public().to_peer_id().to_string(),
        key: b64().encode(a.public().encode_protobuf()),
        eph: hex::encode(secret.public()),
        addrs: vec!["/ip4/127.0.0.1/tcp/4741".into(), "/dns4/node.example/udp/4741/quic-v1".into()],
        label: "anna".into(),
        scope,
        created,
        expires: created + DEFAULT_TTL_MS,
        sig: String::new(),
    };
    o.sig = b64().encode(a.sign(&signed(OFFER_DOMAIN, &o)).unwrap());
    o
}

fn answer(b: &Keypair, secret: &ExchangeSecret, o: &Offer, created: i64) -> Answer {
    let mut a = Answer {
        v: 1,
        offer: o.id.clone(),
        offer_hash: o.hash().unwrap(),
        peer: b.public().to_peer_id().to_string(),
        key: b64().encode(b.public().encode_protobuf()),
        eph: hex::encode(secret.public()),
        addrs: vec![],
        label: "ben".into(),
        created,
        sig: String::new(),
    };
    a.sig = b64().encode(b.sign(&signed(ANSWER_DOMAIN, &a)).unwrap());
    a
}

fn cert(u: &Keypair, user: &str, peer: &str, label: &str, at: i64) -> UserNode {
    UserNode {
        user_id: user.into(),
        peer_id: peer.into(),
        label: label.into(),
        cert: u.sign(&cert_bytes(user, peer, at)).unwrap(),
        issued_at: at,
        revoked_at: None,
        revocation: None,
    }
}

/// A store whose user key is the fixed synthetic key 3.
fn user_store() -> (Store, Keypair, String) {
    let s = Store::open_in_memory().unwrap();
    let u = kp(3);
    let id = user_id(&u.public());
    s.init_self_user(&id, "me", &u.public().encode_protobuf(), &u.to_protobuf_encoding().unwrap(), None).unwrap();
    (s, u, id)
}

fn deterministic() -> Value {
    let (a, b, m) = (kp(1), kp(2), kp(9));
    let (sa, sb) = (ExchangeSecret::from_bytes([5; 32]), ExchangeSecret::from_bytes([6; 32]));
    let o = offer(&a, &sa, "x-golden", node_scope(), 1_000);
    let tok = o.token().unwrap();
    let ans = answer(&b, &sb, &o, 2_000);
    let d = ans.complete(&o, &sa.to_bytes()[..]).unwrap();
    let scoped: Vec<Value> = [
        link_scope(),
        json!({"kind": "organization", "org": "o1", "as": "invite"}),
        json!({"kind": "organization", "org": "o1", "as": "apply"}),
        json!({"kind": "resource", "review": "r1", "as": "review"}),
    ]
    .into_iter()
    .map(|sc| {
        let x = offer(&a, &sa, "x-scoped", sc.clone(), 1_000);
        json!({"scope": sc, "kind": scope_kind(&sc), "token": x.token().unwrap(), "verify": err(x.verify(1_000).map(|p| p.to_string()))})
    })
    .collect();

    // refusals, each with its message
    let mut r = serde_json::Map::new();
    let mut bad = o.clone();
    bad.addrs.push("/ip4/6.6.6.6/tcp/1".into());
    r.insert("offer.tampered".into(), json!(err(bad.verify(2_000))));
    let mut bad = o.clone();
    bad.key = b64().encode(b.public().encode_protobuf());
    r.insert("offer.wrong_key".into(), json!(err(bad.verify(2_000))));
    let mut bad = o.clone();
    bad.key = "!!".into();
    r.insert("offer.bad_key".into(), json!(err(bad.verify(2_000))));
    let mut bad = o.clone();
    bad.sig = "!!".into();
    r.insert("offer.bad_sig".into(), json!(err(bad.verify(2_000))));
    let mut bad = o.clone();
    bad.v = 2;
    r.insert("offer.version".into(), json!(err(bad.verify(2_000))));
    r.insert("offer.expired".into(), json!(err(o.verify(1_000 + DEFAULT_TTL_MS + 1))));
    for (k, sc) in [
        ("scope.unknown", json!({"kind": "galaxy"})),
        ("scope.none", json!({})),
        ("scope.org", json!({"kind": "organization", "org": "o1"})),
        ("scope.org_long", json!({"kind": "organization", "org": "o".repeat(65), "as": "invite"})),
        ("scope.resource", json!({"kind": "resource", "review": "r1"})),
    ] {
        let x = offer(&a, &sa, "x-bad", sc.clone(), 1_000);
        r.insert(format!("{k}.verify"), json!(err(x.verify(1_000))));
        r.insert(format!("{k}.new"), json!(err(Offer::new(&a, "", vec![], sc, 0, 60_000))));
    }
    r.insert("answer.own_offer".into(), json!(err(Answer::new(&a, &o, "", vec![], 2_000))));
    let other = offer(&a, &sa, "x-other", node_scope(), 1_000);
    r.insert("answer.other_offer".into(), json!(err(ans.verify(&other))));
    let mut bad = ans.clone();
    bad.label = "mallory".into();
    r.insert("answer.tampered".into(), json!(err(bad.complete(&o, &sa.to_bytes()[..]))));
    let mut bad = ans.clone();
    bad.v = 3;
    r.insert("answer.version".into(), json!(err(bad.verify(&o))));
    let self_ans = answer(&a, &sb, &o, 2_000);
    r.insert("answer.by_offerer".into(), json!(err(self_ans.verify(&o))));
    r.insert("answer.short_secret".into(), json!(err(ans.complete(&o, &[1u8; 3]))));
    let mut bad = ans.clone();
    bad.eph = "zz".into();
    r.insert("answer.bad_eph".into(), json!(err(bad.complete(&o, &sa.to_bytes()[..]))));
    let blank = Sealed { nonce: String::new(), ct: String::new() };
    r.insert(
        "msg.wrong_direction".into(),
        json!(err(open_msg(&Key32([8; 32]), "x-golden", &b.public().to_peer_id(), &a.public().to_peer_id(), &blank))),
    );
    r.insert("token.hello".into(), json!(err(parse_token("hello"))));
    r.insert("token.base64".into(), json!(err(parse_token(&format!("{OFFER_PREFIX}***")))));
    r.insert("token.json".into(), json!(err(parse_token(&format!("{ANSWER_PREFIX}{}", b64().encode(b"{}"))))));
    // a man in the middle answering with his own key gets another short code
    let mitm = answer(&m, &ExchangeSecret::from_bytes([7; 32]), &o, 2_000).complete(&o, &sa.to_bytes()[..]).unwrap();

    let u = kp(3);
    let uid = user_id(&u.public());
    let pa = a.public().to_peer_id().to_string();
    let pb = b.public().to_peer_id().to_string();
    let ca = cert(&u, &uid, &pa, "this node", 42);
    let mut cb = cert(&u, &uid, &pb, "phone", 43);
    cb.revoked_at = Some(50);
    cb.revocation = Some(u.sign(&revoke_bytes(&uid, &pb, 50)).unwrap());
    let card = Card { id: uid.clone(), label: "me".into(), key: u.public().encode_protobuf(), nodes: vec![ca.clone(), cb.clone()] };
    let mut forged = cert(&m, &uid, &pa, "", 42);
    forged.label = "x".into();
    let mut moved = ca.clone();
    moved.peer_id = pb.clone();
    let mut half = cb.clone();
    half.revocation = None;
    let mut late = cb.clone();
    late.revoked_at = Some(51);
    let (st, _, _) = user_store();
    let shard = ShardBlob { set: "set1".into(), index: 2, threshold: 3, total: 5, blob: "opaque-sealed-blob".into() };

    let addrs = [
        "/ip4/0.0.0.0/tcp/4741",
        "/ip4/172.17.0.2/tcp/4741",
        "/ip6/fe80::1/tcp/4741",
        "/ip4/127.0.0.1/tcp/4741",
        "/ip4/192.168.1.5/udp/4741/quic-v1",
        "/ip4/100.64.1.1/tcp/4741",
        "/ip4/8.8.8.8/tcp/4741",
        "/ip6/fd00::1/tcp/4741",
        "/dns4/relay.example/tcp/4741",
    ];
    let addr_rows: Vec<Value> = addrs
        .iter()
        .map(|s| {
            let m: Multiaddr = s.parse().unwrap();
            json!([s, shareable(&m), routable(&m), reach_rank(&m)])
        })
        .collect();
    let relay = kp(4).public().to_peer_id();
    let me = kp(5).public().to_peer_id();
    let circuits: Vec<Multiaddr> = [
        format!("/ip4/10.0.0.1/tcp/4741/p2p/{relay}/p2p-circuit/p2p/{me}"),
        format!("/ip4/8.8.4.4/tcp/4741/p2p/{relay}/p2p-circuit/p2p/{me}"),
        format!("/dns4/relay.example/tcp/4741/p2p/{relay}/p2p-circuit/p2p/{me}"),
        "/ip4/8.8.8.8/tcp/4741".to_string(),
    ]
    .iter()
    .map(|s| s.parse().unwrap())
    .collect();
    let configured: Vec<Multiaddr> = vec![format!("/dns4/relay.example/tcp/4741/p2p/{relay}").parse().unwrap()];

    json!({
        "offer": {
            "json": serde_json::to_value(&o).unwrap(),
            "token": tok,
            "link": link(&tok),
            "unlink": unlink(&format!(" {}\n{} ", &link(&tok)[..30], &link(&tok)[30..])) == tok,
            "hash": o.hash().unwrap(),
            "verify": o.verify(2_000).unwrap().to_string(),
            "parsed_equal": matches!(parse_token(&tok).unwrap(), Token::Offer(x) if x == o),
        },
        "scoped": scoped,
        "answer": {
            "json": serde_json::to_value(&ans).unwrap(),
            "token": ans.token().unwrap(),
            "link": link(&ans.token().unwrap()),
            "verify": ans.verify(&o).unwrap().to_string(),
            "parsed_equal": matches!(parse_token(&link(&ans.token().unwrap())).unwrap(), Token::Answer(x) if x == ans),
        },
        "derived": { "key": hex::encode(d.key.0), "short_code": d.short_code, "fingerprint": d.fingerprint, "mitm_differs": mitm.short_code != d.short_code },
        "refused": r,
        "tokens": {
            "is_token": (["mnnn-x1-offer:abc", "muninn://join/abc", " mnnn-x1-x", "ben", "weirdnet://join/abc"].iter().map(|t| json!([t, is_token(t)])).collect::<Vec<_>>()),
            "codes": (["1234567890", "12345 67890", "12-34", " 123 "].iter().map(|c| json!([c, normalize_code(c)])).collect::<Vec<_>>()),
            "unlink_plain": unlink("abc def"),
            "link_plain": link("abc"),
        },
        "identity": {
            "peer": pa,
            "fingerprint": super::identity::fingerprint(&a.public()),
            "group": super::identity::group("abcd1234ef"),
        },
        "users": {
            "user_id": uid,
            "cert_bytes": String::from_utf8(cert_bytes(&uid, &pa, 42)).unwrap(),
            "revoke_bytes": String::from_utf8(revoke_bytes(&uid, &pb, 50)).unwrap(),
            "cert_a": hex::encode(&ca.cert),
            "cert_b": hex::encode(&cb.cert),
            "revocation_b": hex::encode(cb.revocation.as_ref().unwrap()),
            "verify": [verify(&u.public(), &ca), verify(&u.public(), &cb), verify(&m.public(), &ca), verify(&u.public(), &forged), verify(&u.public(), &moved), verify(&u.public(), &half), verify(&u.public(), &late)],
            "card_json": serde_json::to_value(&card).unwrap(),
            "card_digest": card.digest(),
        },
        "sync": {
            "key": hex::encode(super::sync::sync_key(&st).unwrap().0),
            "aad": String::from_utf8(super::sync::aad("p9")).unwrap(),
        },
        "wire": {
            "introduce": serde_json::to_value(Req::Introduce { card: card.clone() }).unwrap(),
            "shard_store": serde_json::to_value(Req::ShardStore { shard: shard.clone(), claim: "c1".into() }).unwrap(),
            "shards": serde_json::to_value(Resp::Shards { shards: vec![shard.clone()] }).unwrap(),
            "linked": serde_json::to_value(Resp::Linked { card: card.clone(), keypair: "opaque".into() }).unwrap(),
            "wire_offer": serde_json::to_value(Wire::Offer { token: tok.clone() }).unwrap(),
            "wire_sealed": serde_json::to_value(Wire::Sealed { exchange: "x-golden".into(), sealed: Sealed { nonce: "n".into(), ct: "c".into() } }).unwrap(),
            "wire_hello": serde_json::to_value(WireResp::Hello { label: "anna".into(), agent: "muninn/2.23.0 (anna)".into(), caps: vec!["muninn".into(), "relay".into()] }).unwrap(),
        },
        "addrs": addr_rows,
        "circuits": one_circuit_per_relay(circuits, &configured).iter().map(|a| a.to_string()).collect::<Vec<_>>(),
    })
}

/// Messages sealed now (random nonces), opened by the compare run.
fn sealed_now() -> Value {
    let k = Key32([8; 32]);
    let (a, b) = (kp(1).public().to_peer_id(), kp(2).public().to_peer_id());
    let (st, _, _) = user_store();
    let sk = super::sync::sync_key(&st).unwrap();
    json!({
        "msg": seal_msg(&k, "x-golden", &a, &b, b"{\"Ping\":null}").unwrap(),
        "sync": crypto::seal(&sk, b"{\"Page\":[]}", &super::sync::aad(&a.to_string())).unwrap(),
    })
}

#[test]
fn network_formats_are_byte_identical_to_the_old_version() {
    let det = deterministic();
    if let Ok(path) = std::env::var("MUNINN_NET_GOLDEN_WRITE") {
        let all = json!({ "deterministic": det, "sealed": sealed_now() });
        std::fs::write(path, serde_json::to_string_pretty(&all).unwrap() + "\n").unwrap();
        return;
    }
    let golden: Value = serde_json::from_str(GOLDEN).unwrap();
    assert_eq!(det, golden["deterministic"], "network formats changed");
    // what the old version sealed still opens, and only with the same exchange, direction and key
    let k = Key32([8; 32]);
    let (a, b) = (kp(1).public().to_peer_id(), kp(2).public().to_peer_id());
    let msg: Sealed = serde_json::from_value(golden["sealed"]["msg"].clone()).unwrap();
    assert_eq!(&open_msg(&k, "x-golden", &a, &b, &msg).unwrap()[..], b"{\"Ping\":null}");
    assert_eq!(err(open_msg(&k, "x-golden", &b, &a, &msg)), golden["deterministic"]["refused"]["msg.wrong_direction"].as_str().unwrap());
    assert!(open_msg(&k, "x-other", &a, &b, &msg).is_err());
    assert!(open_msg(&Key32([9; 32]), "x-golden", &a, &b, &msg).is_err());
    let (st, _, _) = user_store();
    let sk = super::sync::sync_key(&st).unwrap();
    let page: Sealed = serde_json::from_value(golden["sealed"]["sync"].clone()).unwrap();
    assert_eq!(&crypto::open(&sk, &page, &super::sync::aad(&a.to_string())).unwrap()[..], b"{\"Page\":[]}");
    assert!(crypto::open(&sk, &page, &super::sync::aad(&b.to_string())).is_err());
    // what is sealed now opens too
    let now: Sealed = serde_json::from_value(sealed_now()["msg"].clone()).unwrap();
    assert_eq!(&open_msg(&k, "x-golden", &a, &b, &now).unwrap()[..], b"{\"Ping\":null}");
}
