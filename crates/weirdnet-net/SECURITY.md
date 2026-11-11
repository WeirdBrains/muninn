# Security notes for weirdnet-net

This is a description of the boundaries, reviewed while extracting the crate. It is not an
audit; nobody has audited this crate.

What the crate enforces:

- Offers and answers are signed with the node's Ed25519 key under the profile's domains; a
  changed field, a key that is not the peer id's, a wrong version, an unknown scope or an
  expired offer is refused. An answer is bound to the offer's hash, and the node that made
  an offer can't answer it.
- Both sides derive the exchange key and the short code from the X25519 secret and the
  transcript of both signed messages (in `weirdnet-keys`). Only comparing the short code or
  fingerprint out of band rules out a man in the middle; the crate can't do that for you.
- Sealed messages (XChaCha20-Poly1305, in `weirdnet-keys`) bind the exchange id and both
  peer ids in order, so a message can't be moved to another exchange or turned around.
  `sealed::serve` opens only for a known, active exchange whose peer is the sender Noise
  authenticated.
- Certificates and revocations are checked against the user key; a revocation time without
  its signature fails. Cards must certify the very node that sends them.
- Invitation automation (`invite`): a delivered offer is kept only if the node that signed
  it is the Noise-authenticated sender, and after the host's `admit`; at most
  `Host::max_incoming` wait. It is answered on arrival only when the host's `decide` says so
  (the default host policy answers nothing by itself). An answer completes only an offer
  this node made and still has open, and only if signed by the peer it was sent to; only an
  offer's offerer may poll it. When both nodes offered at once and this one answers, its own
  offers of that kind are ended. A node using the name of another node whose key is pinned
  is flagged, and a name shared by several nodes resolves to the pinned one; the flag is a
  warning for the application to show, not a refusal.
- Profiles don't mix: tokens, signatures, sealed messages, certificates and the sync key of
  one profile don't verify or open under another, and nodes on different protocol ids don't
  talk.

What the crate leaves to the application:

- Whether to answer an offer on the spot (`decide`), which offers to let in (`admit`), and
  any bookkeeping of its own (`kept`, `completed`, `poll`).
- Who may do what over an exchange, which users are trusted, what an invitation grants,
  which offers to accept, and every limit on what peers may store or ask for.
- Keeping secrets: node identities, exchange keys and user keypairs are handed to the
  application's store and should live encrypted at rest.
- A link grant hands the user's signing key to the node being linked (sealed under the link
  exchange); that is by design and only for the user's own nodes.

Tested over libp2p's in-memory transport (real Noise, Yamux and request-response); TCP and
QUIC sockets, relays, hole punching, mDNS and AutoNAT are not exercised by the tests.

Not verified: an audit, fuzzing, Windows, macOS, iOS and Android, an MSRV run and minimal
dependency versions. Requests up to 8 MiB and answers up to 256 MiB are accepted, with a
60-second timeout.
