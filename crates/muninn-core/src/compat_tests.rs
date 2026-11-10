//! Characterization (golden) tests for every Muninn-branded identifier that is part of a
//! wire protocol, a signature or KDF domain, an at-rest key format, or the plugin ABI.
//!
//! These pin today's bytes before the Weirdnet crate extraction (see
//! `docs/architecture.md`, "Crate extraction"). The extraction must keep them byte-identical
//! until a separately approved rollout introduces Weirdnet identifiers; renaming one of these
//! without a migration breaks peers, signatures or existing databases, so a failure here is
//! a compatibility break, not a test to update.
//!
//! Where possible the expected value is recomputed independently (raw HKDF, AEAD, BLAKE3,
//! Argon2) from the literal label, so the label and the construction are both pinned.

use crate::crypto::{self, Key32};
use secrecy::SecretString;

fn hkdf_raw(ikm: &[u8; 32], salt: &[u8], info: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    hkdf::Hkdf::<sha2::Sha256>::new(Some(salt), ikm).expand(info, &mut out).unwrap();
    out
}

#[test]
fn libp2p_protocol_ids() {
    use crate::net::proto::*;
    assert_eq!(PROTOCOL, "/muninn/net/1");
    assert_eq!(KAD_PROTOCOL, "/muninn/kad/1");
    assert_eq!(RELAY_KEY, "/muninn/relay/1");
    assert_eq!(IDENTIFY_PROTOCOL, "/muninn/id/1");
}

#[test]
fn exchange_tokens_links_and_domains() {
    use crate::net::exchange::*;
    assert_eq!(OFFER_PREFIX, "mnnn-x1-offer:");
    assert_eq!(ANSWER_PREFIX, "mnnn-x1-answer:");
    assert_eq!(JOIN_LINK, "muninn://join/");
    assert_eq!(ANSWER_LINK, "muninn://answer/");
    assert_eq!(OFFER_DOMAIN, b"muninn/net/offer/v1");
    assert_eq!(ANSWER_DOMAIN, b"muninn/net/answer/v1");
    assert_eq!(TRANSCRIPT_DOMAIN, b"muninn/net/exchange/v1");
    assert_eq!(KEY_INFO, b"muninn/net/exchange/v1/key");
    assert_eq!(SAS_INFO, b"muninn/net/exchange/v1/sas");
}

#[test]
fn exchange_message_aad() {
    use crate::net::exchange::open_msg;
    let key = Key32([9u8; 32]);
    let (a, b) = (libp2p::PeerId::random(), libp2p::PeerId::random());
    let aad = format!("muninn/net/msg/v1|ex1|{a}|{b}");
    let sealed = crypto::seal(&key, b"hello", aad.as_bytes()).unwrap();
    assert_eq!(&open_msg(&key, "ex1", &a, &b, &sealed).unwrap()[..], b"hello");
    // the direction is bound too
    assert!(open_msg(&key, "ex1", &b, &a, &sealed).is_err());
}

#[test]
fn node_certificate_domains() {
    use crate::net::user::{cert_bytes, revoke_bytes};
    assert_eq!(cert_bytes("u1", "p1", 42), b"muninn-node-cert/1\nu1\np1\n42");
    assert_eq!(revoke_bytes("u1", "p1", 42), b"muninn-node-revoke/1\nu1\np1\n42");
}

#[test]
fn linked_node_sync_key_and_aad() {
    let s = crate::store::Store::open_in_memory().unwrap();
    let (u, kp) = crate::net::user::load_or_create(&s).unwrap();
    let ed = kp.try_into_ed25519().unwrap().to_bytes();
    let ikm: [u8; 32] = ed[..32].try_into().unwrap();
    let want = hkdf_raw(&ikm, u.id.as_bytes(), b"muninn/sync/v1/key");
    assert_eq!(crate::net::sync::sync_key(&s).unwrap().0, want);
    assert_eq!(crate::net::sync::aad("p9"), b"muninn/sync/v1|p9");
}

#[test]
fn live_stream_identifiers() {
    assert_eq!(crate::collab::proto::PACKAGE, "muninn.live.v1");
    assert_eq!(crate::collab::stream::GRPC_AUTHORITY, "http://muninn.live");
}

#[test]
fn key_derivation_labels() {
    let m = Key32([7u8; 32]);
    assert_eq!(crypto::service_key(&m, "db-1", 3).unwrap().0, hkdf_raw(&m.0, b"db-1", b"muninn/service/v3"));
    assert_eq!(crypto::master_kek(&m, "db-1").unwrap().0, hkdf_raw(&m.0, b"db-1", b"muninn/kek"));
    assert_eq!(crypto::keyring::dek_aad(), b"muninn/dek");
    assert_eq!(crypto::keyring::svc_aad("db-1", 2), b"muninn/service-key/db-1/v2");
}

#[test]
fn shard_text_format() {
    assert_eq!(crypto::SHARD_PREFIX, "muninn-shard-v1");
    let m = Key32([3u8; 32]);
    for s in crypto::split_shards(&m).unwrap() {
        let (body, chk) = s.rsplit_once(':').unwrap();
        assert!(body.starts_with("muninn-shard-v1:"), "{body}");
        assert_eq!(chk, &blake3::hash(body.as_bytes()).to_hex()[..8]);
    }
}

#[test]
fn dark_shard_claim_tag() {
    let pw = SecretString::from("correct horse".to_string());
    let params = argon2::Params::new(19 * 1024, 2, 1, Some(32)).unwrap();
    let mut k = [0u8; 32];
    argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params)
        .hash_password_into(b"correct horse", b"muninn/net/shard-claim/v1", &mut k)
        .unwrap();
    assert_eq!(crypto::shard_claim(&pw).unwrap(), hex::encode(blake3::keyed_hash(&k, b"claim").as_bytes()));
}

/// The at-rest envelope: re-derive the keys from the recovery words with the literal labels
/// and open the wrapped DEK and the password-wrapped service key by hand.
#[test]
fn envelope_and_service_key_file_format() {
    use crate::crypto::keyring;
    let t = tempfile::tempdir().unwrap();
    let paths = crate::paths::Paths::resolve(Some(t.path().join("h")), None).unwrap();
    let pw = SecretString::from("pw-golden".to_string());
    let out = keyring::init(&paths, &pw).unwrap();
    let master = crypto::from_words(&out.words).unwrap();
    let env = keyring::Envelope::require(&paths).unwrap();
    assert_eq!(env.format, 1);
    let dek = keyring::unlock_with_password(&paths, &pw).unwrap();

    let svc = Key32(hkdf_raw(&master.0, env.db_id.as_bytes(), b"muninn/service/v1"));
    assert_eq!(&crypto::open(&svc, &env.service_wrap, b"muninn/dek").unwrap()[..], &dek.0);
    let kek = Key32(hkdf_raw(&master.0, env.db_id.as_bytes(), b"muninn/kek"));
    assert_eq!(&crypto::open(&kek, &env.master_wrap, b"muninn/dek").unwrap()[..], &dek.0);

    let f = keyring::load_service_file(&paths, 1).unwrap();
    let aad = format!("muninn/service-key/{}/v1", env.db_id);
    let w = f.wrapped.unwrap();
    assert_eq!(w.kdf, "argon2id");
    assert_eq!(&crypto::password_open(&pw, &w, aad.as_bytes()).unwrap()[..], &svc.0);
}

#[test]
fn code_trust_hash_domain() {
    use crate::store::code::Lang;
    let src = "x = 1\n";
    let mut h = blake3::Hasher::new();
    h.update(b"muninn-code-trust\0");
    h.update(Lang::Math.as_str().as_bytes());
    h.update(&[0]);
    h.update(src.as_bytes());
    assert_eq!(crate::code::trust_hash_of(Lang::Math, src).unwrap(), h.finalize().to_hex()[..32].to_string());
}

#[test]
fn plugin_abi_v1_symbols() {
    use crate::code::abi::*;
    assert_eq!(ABI_VERSION, 1);
    assert_eq!(SYM_ABI, b"muninn_plugin_abi\0");
    assert_eq!(SYM_INFO, b"muninn_plugin_info\0");
    assert_eq!(SYM_RUN, b"muninn_plugin_run\0");
}

/// App-owned identifiers (they stay Muninn's; pinned so a crate move does not rename them).
#[test]
fn app_identifiers() {
    assert_eq!(crate::org::host::GRPC_SERVICE, "muninn.org.v1.Org");
    assert_eq!(crate::org::host::SESSION_COOKIE, "muninn_session");
    assert_eq!(crate::paths::DB_FILE, "muninn.database");
}

/// Key files written by the pre-extraction keyring (f123f855; synthetic, see the fixture
/// README) open with today's code, and the envelope and service files save back unchanged.
#[test]
fn keys_written_before_the_extraction_still_open() {
    use crate::crypto::keyring::{self, Envelope, KeyFiles};
    macro_rules! fx {
        ($f:literal) => {
            include_str!(concat!("compat_fixtures/keys-f123f855/", $f))
        };
    }
    macro_rules! fxb {
        ($f:literal) => {
            include_bytes!(concat!("compat_fixtures/keys-f123f855/", $f))
        };
    }
    let t = tempfile::tempdir().unwrap();
    let paths = crate::paths::Paths::resolve(Some(t.path().join("h")), None).unwrap();
    paths.ensure().unwrap();
    std::fs::create_dir_all(KeyFiles::service_keys_dir(&paths)).unwrap();
    std::fs::write(KeyFiles::envelope(&paths), fx!("envelope.toml")).unwrap();
    std::fs::write(KeyFiles::service_keys_dir(&paths).join("v1.toml"), fx!("service-v1.toml")).unwrap();
    std::fs::write(KeyFiles::service_keys_dir(&paths).join("v2.toml"), fx!("service-v2.toml")).unwrap();

    let pw = SecretString::from("fixture-pw".to_string());
    let dek = Key32::from_hex(fx!("dek.hex")).unwrap();
    assert_eq!(keyring::unlock_with_password(&paths, &pw).unwrap(), dek);
    assert!(keyring::unlock_with_password(&paths, &SecretString::from("fixture-pw!".to_string())).is_err());
    let master = crypto::from_words(fx!("words.txt")).unwrap();
    assert_eq!(keyring::unlock_with_master(&paths, &master).unwrap(), dek);
    let v1 = keyring::load_service_file(&paths, 1).unwrap();
    assert!(v1.retired && v1.wrapped.is_none());

    let shards: Vec<String> = fx!("shards.txt").lines().map(str::to_string).collect();
    assert_eq!(shards.len(), 5);
    assert_eq!(crypto::combine_shards(&[shards[0].clone(), shards[2].clone(), shards[4].clone()]).unwrap(), master);
    assert_eq!(keyring::recover_master(&paths, &shards[1..4]).unwrap(), master);
    assert!(crypto::combine_shards(&shards[..2]).is_err());

    let spw = SecretString::from("fixture-shard-pw".to_string());
    assert_eq!(crypto::shard_claim(&spw).unwrap(), fx!("claim.txt"));
    let dark: serde_json::Value = serde_json::from_str(fx!("dark-shard.json")).unwrap();
    let sealed: crypto::PasswordSealed = serde_json::from_str(dark["blob"].as_str().unwrap()).unwrap();
    let (set, index) = (dark["set"].as_str().unwrap(), dark["index"].as_u64().unwrap() as u32);
    assert_eq!(*weirdnet_keys::dark::open(&crypto::MUNINN_PROFILE, &spw, set, index, &sealed).unwrap(), shards[0]);

    let epw = SecretString::from("fixture-export-pw".to_string());
    assert_eq!(Key32::from_hex(std::str::from_utf8(&crypto::age_decrypt(&epw, fxb!("master.key.age")).unwrap()).unwrap()).unwrap(), master);
    let exported: Vec<String> = [&fxb!("shard-1.age")[..], fxb!("shard-2.age"), fxb!("shard-3.age")]
        .iter()
        .map(|b| String::from_utf8(crypto::age_decrypt(&epw, b).unwrap().to_vec()).unwrap())
        .collect();
    assert_eq!(crypto::combine_shards(&exported).unwrap(), master);

    assert_eq!(
        crate::org::host::sign_with(&Key32([7u8; 32]), "x-fixture", "POST", "/api/v1/thing", b"{\"a\":1}", 1_700_000_000_000),
        fx!("org-sig.txt")
    );

    // written back byte for byte (no profile field appears)
    Envelope::require(&paths).unwrap().save(&paths).unwrap();
    assert_eq!(std::fs::read_to_string(KeyFiles::envelope(&paths)).unwrap(), fx!("envelope.toml"));
    for (v, want) in [(1, fx!("service-v1.toml")), (2, fx!("service-v2.toml"))] {
        assert_eq!(toml::to_string_pretty(&keyring::load_service_file(&paths, v).unwrap()).unwrap(), want);
    }
    // and today's operations keep working on it
    assert_eq!(keyring::rotate(&paths, &master, &pw).unwrap(), 3);
    assert_eq!(keyring::unlock_with_password(&paths, &pw).unwrap(), dek);
    assert!(!std::fs::read_to_string(KeyFiles::envelope(&paths)).unwrap().contains("profile"));
}
