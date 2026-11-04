//! Exchange backups: the node identity, every key exchange (with its keys) and known
//! peers, age-encrypted with a password. They restore exchanges into a database, or let
//! `mnnn net shard restore` reach custodians when the database itself cannot be opened.
//! (Full database backups carry all of this too.)
use crate::crypto;
use crate::error::{Error, Result};
use crate::store::{Exchange, NetPeer, NewAction, Store, now_ms};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

/// What a backup holds (inside the age encryption).
#[derive(Serialize, Deserialize)]
struct Backup {
    format: String,
    created_at: i64,
    peer_id: String,
    /// The node keypair (libp2p protobuf), hex.
    identity: String,
    exchanges: Vec<serde_json::Value>,
    peers: Vec<NetPeer>,
}

const FORMAT: &str = "muninn-net-backup-v1";

/// The encrypted backup bytes.
pub fn export(s: &Store, password: &SecretString) -> Result<Vec<u8>> {
    let kp = super::identity::load_or_create(s)?;
    let bytes = Zeroizing::new(kp.to_protobuf_encoding().map_err(|e| Error::Crypto(format!("node identity: {e}")))?);
    let b = Backup {
        format: FORMAT.into(),
        created_at: now_ms(),
        peer_id: kp.public().to_peer_id().to_string(),
        identity: hex::encode(&*bytes),
        exchanges: s.exchange_rows_raw()?,
        peers: s.net_peers()?,
    };
    let json = Zeroizing::new(serde_json::to_vec(&b)?);
    crypto::age_encrypt(password, &json)
}

/// What a restore did.
#[derive(Debug, Clone, Serialize)]
pub struct Restored {
    pub peer_id: String,
    pub exchanges: usize,
    pub peers: usize,
    /// This database had another node identity, now replaced.
    pub identity_replaced: bool,
}

fn open(bytes: &[u8], password: &SecretString) -> Result<Backup> {
    let plain = crypto::age_decrypt(password, bytes)?;
    let b: Backup = serde_json::from_slice(&plain).map_err(|e| Error::invalid("backup", format!("not a Muninn network backup ({e})")))?;
    if b.format != FORMAT {
        return Err(Error::invalid("backup", format!("unknown format {}", b.format)));
    }
    Ok(b)
}

/// The node id in a backup and whether restoring it would replace this database's.
pub fn peek(s: &Store, bytes: &[u8], password: &SecretString) -> Result<(String, usize, bool)> {
    let b = open(bytes, password)?;
    let cur = super::identity::peer_id(s)?.map(|p| p.to_string());
    Ok((b.peer_id.clone(), b.exchanges.len(), cur.is_some_and(|c| c != b.peer_id)))
}

/// Restore the identity, exchanges and peers (exchanges with the same id are replaced).
pub fn restore(s: &Store, bytes: &[u8], password: &SecretString, origin: &str) -> Result<Restored> {
    let b = open(bytes, password)?;
    let key = Zeroizing::new(hex::decode(&b.identity).map_err(|_| Error::invalid("backup", "damaged identity"))?);
    let kp = libp2p::identity::Keypair::from_protobuf_encoding(&key).map_err(|e| Error::invalid("backup", format!("identity: {e}")))?;
    if kp.public().to_peer_id().to_string() != b.peer_id {
        return Err(Error::invalid("backup", "identity does not match its peer id"));
    }
    let cur = super::identity::peer_id(s)?.map(|p| p.to_string());
    let identity_replaced = cur.as_ref().is_some_and(|c| *c != b.peer_id);
    s.set_node_identity(&key, &b.peer_id)?;
    let unhex = |v: &serde_json::Value| -> Result<Option<Zeroizing<Vec<u8>>>> {
        match v.as_str() {
            Some(h) => Ok(Some(Zeroizing::new(hex::decode(h).map_err(|_| Error::invalid("backup", "damaged exchange key"))?))),
            None => Ok(None),
        }
    };
    for v in &b.exchanges {
        let x: Exchange = serde_json::from_value(v.clone()).map_err(|e| Error::invalid("backup", format!("exchange: {e}")))?;
        let secret = unhex(&v["secret"])?;
        let k = unhex(&v["key"])?;
        s.restore_exchange_raw(&x, secret.as_deref().map(|v| &v[..]), k.as_deref().map(|v| &v[..]))?;
    }
    for p in &b.peers {
        s.upsert_net_peer(&p.peer_id, Some(&p.label), &p.addrs, Some(&p.caps), Some(&p.agent), &p.source)?;
    }
    s.note_action(
        &NewAction::new("net.exchange.restore", origin, format!("{} key exchange(s) restored from a backup", b.exchanges.len()))
            .params(serde_json::json!({ "count": b.exchanges.len(), "peer": b.peer_id, "identity_replaced": identity_replaced })),
    );
    Ok(Restored { peer_id: b.peer_id, exchanges: b.exchanges.len(), peers: b.peers.len(), identity_replaced })
}

/// A throwaway in-memory database holding a backup (for reaching custodians when the
/// real database cannot be opened).
pub fn scratch(bytes: &[u8], password: &SecretString) -> Result<Store> {
    let s = Store::open_in_memory()?;
    restore(&s, bytes, password, "cli")?;
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{ExchangeStatus, NewExchange};

    #[test]
    fn backups_round_trip_identity_and_exchange_keys() {
        let s = Store::open_in_memory().unwrap();
        let kp = super::super::identity::load_or_create(&s).unwrap();
        s.insert_exchange(
            &NewExchange {
                id: "x1".into(),
                label: "lab".into(),
                role: "offerer".into(),
                status: Some(ExchangeStatus::Offered),
                peer_id: None,
                peer_label: String::new(),
                scope: super::super::exchange::node_scope(),
                offer: "tok".into(),
                expires_at: None,
            },
            Some(&[7u8; 32]),
        )
        .unwrap();
        s.complete_exchange("x1", "12D3KooWPeer", "ben", "ans", &crypto::Key32([9u8; 32]), "11111-22222", "ab").unwrap();
        let pw = SecretString::from("pw");
        let bytes = export(&s, &pw).unwrap();
        assert!(crypto::is_age(&bytes));
        assert!(open(&bytes, &SecretString::from("bad")).is_err());
        let t = Store::open_in_memory().unwrap();
        let _other = super::super::identity::load_or_create(&t).unwrap();
        assert!(peek(&t, &bytes, &pw).unwrap().2);
        let r = restore(&t, &bytes, &pw, "cli").unwrap();
        assert!(r.identity_replaced);
        assert_eq!(r.exchanges, 1);
        assert_eq!(super::super::identity::load_or_create(&t).unwrap().public(), kp.public());
        assert_eq!(t.exchange_key("x1").unwrap().unwrap().0, [9u8; 32]);
        assert_eq!(t.exchange("x1").unwrap().unwrap().short_code.as_deref(), Some("11111-22222"));
        let m = scratch(&bytes, &pw).unwrap();
        assert_eq!(m.exchanges().unwrap().len(), 1);
    }
}
