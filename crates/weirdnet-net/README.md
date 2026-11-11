# weirdnet-net

Peer-to-peer networking for applications that keep their own data: a libp2p node, key
exchanges with invitation tokens and links, sealed messaging over those exchanges, node
identities, and user certificates and cards. The application brings its own request and
response types, its own storage and its own policy; the crate brings the wire.

- `node`: a `Node<Q, R>` over TCP and QUIC (or an in-memory transport for tests) with Noise
  and Yamux, mDNS, Kademlia, identify, ping, a relay client and an opt-in relay server,
  DCUtR hole punching and AutoNAT. The swarm runs on its own thread; the handle is blocking.
  Requests are the application's `Q`, answers its `R` (CBOR), answered by a `Handler`.
- `exchange`: signed offers and answers (Ed25519 node keys, fresh X25519 keys), the derived
  exchange key, a ten-digit short code and a fingerprint to compare out of band, tokens and
  links (invitations), scopes with an expiry, and sealed messages bound to the exchange and
  direction.
- `sealed`: send a sealed request through the application's envelope and open the answer
  (`call`), and answer one only for the exchange's own, Noise-authenticated peer (`serve`).
- `invite`: invitation automation over the application's records (`Host`) and messages
  (`Ask`, `Say`): `connect` (hello, then an offer delivered and answered on the spot or kept
  for the owner), answering on arrival when the host's `decide` says so, `poll` and
  `settle` for kept offers and undelivered answers, crossing offers settled on one exchange,
  trust on first use (`known`, `pinned`, `impostor`, names resolving to the pinned node),
  relay addresses for a peer and the addresses fit for invite links.
- `identity`, `user`, `sync`, `shard`, `addr`: node identities through an `IdentityStore`,
  user certificates, revocations, cards and link grants, the linked-node sync key, dark-shard
  blobs as custodians carry them, and address helpers.

Every identifier on the wire or under a signature comes from a `Profile`. `WEIRDNET` is the
default (`/weirdnet/net/1`, `wnet-x1-offer:`, `weirdnet://join/`, port 4747 and so on). An
application with nodes already in the field passes its own profile with the values those
nodes use, so nothing changes for them.

Key cryptography is not here: every derivation, seal and open goes through `weirdnet-keys`,
and key backups and recovery stay there. Shard blobs and backups cross the network as opaque
bytes. See `SECURITY.md` for the boundaries and what has not been checked.

```rust
use weirdnet_net::{WEIRDNET, Keypair, exchange::Token, profile::invite_scope};

let (anna, ben) = (Keypair::generate_ed25519(), Keypair::generate_ed25519());
let p = &WEIRDNET;
let (offer, secret) = p.offer(&anna, "anna", vec![], invite_scope("garden", "member"), 0, 60_000).unwrap();
let link = p.link(&p.offer_token(&offer).unwrap());
let Token::Offer(seen) = p.parse_token(&link).unwrap() else { unreachable!() };
p.verify_offer(&seen, 1).unwrap();
let (answer, on_ben) = p.answer(&ben, &seen, "ben", vec![], 1).unwrap();
let on_anna = p.complete(&answer, &offer, &secret[..]).unwrap();
assert_eq!(on_anna.short_code, on_ben.short_code);
```
