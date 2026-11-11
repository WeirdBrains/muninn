//! Sealed exchange messaging over a node: a request sealed under an exchange key goes out
//! inside the application's own envelope, and the answer comes back sealed the other way.
//!
//! The receiving side ([`serve`]) opens a message only when the exchange is known, active,
//! and belongs to the Noise-authenticated sender; the exchange id, both peer ids and the
//! direction are bound as associated data. What is inside (the application's request and
//! answer types, what each may reach) stays the application's.
use crate::error::{NetError, Result};
use crate::node::{Message, Node};
use crate::profile::Profile;
use libp2p::{Multiaddr, PeerId};
use weirdnet_keys::{Key32, Sealed};
use zeroize::Zeroizing;

/// The application's request envelope: how a sealed request travels.
pub trait Envelope: Message {
    fn sealed(exchange: String, sealed: Sealed) -> Self;
}

/// The application's answer envelope: a sealed answer, or something else (an error, say)
/// handed back as it is.
pub trait Reply: Message {
    fn sealed(sealed: Sealed) -> Self;
    fn into_sealed(self) -> std::result::Result<Sealed, Self>;
}

/// What came back from [`call`].
pub enum Answered<R> {
    /// The opened answer.
    Opened(Zeroizing<Vec<u8>>),
    /// Not a sealed answer; the application reads it.
    Other(R),
}

/// One side's view of an exchange it sends over.
pub struct Exchange<'a> {
    pub id: &'a str,
    pub key: &'a Key32,
    pub peer: PeerId,
}

/// Seal `plain` for the exchange's peer, send it, and open the sealed answer.
pub fn call<Q: Envelope, R: Reply>(p: &Profile, node: &Node<Q, R>, x: &Exchange, addrs: &[Multiaddr], plain: &[u8]) -> Result<Answered<R>> {
    let me = node.peer_id();
    let sealed = p.seal_msg(x.key, x.id, &me, &x.peer, plain)?;
    Ok(match node.request(x.peer, addrs, Q::sealed(x.id.to_string(), sealed))?.into_sealed() {
        Ok(s) => Answered::Opened(p.open_msg(x.key, x.id, &x.peer, &me, &s)?),
        Err(other) => Answered::Other(other),
    })
}

/// Where the receiving side looks exchanges up.
pub trait Exchanges {
    /// The application's exchange record.
    type Exchange;
    type Error: From<NetError>;
    fn exchange(&self, id: &str) -> std::result::Result<Option<Self::Exchange>, Self::Error>;
    /// Completed and not revoked.
    fn active(x: &Self::Exchange) -> bool;
    /// The peer id on the other side.
    fn peer(x: &Self::Exchange) -> Option<&str>;
    fn key(&self, x: &Self::Exchange) -> std::result::Result<Option<Key32>, Self::Error>;
}

/// Answer a sealed request from `from` (the Noise-authenticated peer): look the exchange
/// up, refuse unknown, inactive or someone else's, open, let `answer` make the reply (with
/// the store, the exchange and the opened bytes), and seal that back. Errors are the caller's to put
/// in its own envelope.
pub fn serve<S: Exchanges>(
    p: &Profile,
    store: &mut S,
    local: &PeerId,
    from: &PeerId,
    exchange: &str,
    sealed: &Sealed,
    answer: impl FnOnce(&mut S, &S::Exchange, &[u8]) -> std::result::Result<Vec<u8>, S::Error>,
) -> std::result::Result<Sealed, S::Error> {
    let x = store.exchange(exchange)?.ok_or_else(|| NetError::invalid("exchange", "unknown exchange"))?;
    if !S::active(&x) {
        return Err(NetError::invalid("exchange", "this exchange is not active (revoked or never completed)").into());
    }
    if S::peer(&x) != Some(&from.to_string()) {
        return Err(NetError::invalid("exchange", "this exchange belongs to another node").into());
    }
    let id = exchange.to_string();
    let key = store.key(&x)?.ok_or_else(|| NetError::invalid("exchange", "no key"))?;
    let plain = p.open_msg(&key, &id, from, local, sealed)?;
    let out = answer(store, &x, &plain)?;
    Ok(p.seal_msg(&key, &id, local, from, &out)?)
}
