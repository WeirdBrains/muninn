# Key exchanges

Two nodes share data only over a completed key exchange: an authenticated X25519
Diffie-Hellman between their Ed25519 node identities. **The nodes do it on their own.**
You connect, apply, invite, join a review or share; what you see is the action and where
it stands: *pending*, *requested*, *connected*, *ended* or *failed*.

## What you do

| You want to | CLI | TUI |
|---|---|---|
| Connect to a node | `mnnn net connect ADDR\|PEER\|NAME` | Net ▸ Nodes, `c` |
| Invite a node | `mnnn invite create` or `mnnn net invite` (prints `muninn://join/…`) | Net ▸ Invites, `n` |
| Use an invite link | `mnnn invite accept LINK` or `mnnn net connect LINK` | Invites, `p` (paste) |
| See requests | `mnnn invite list`, `mnnn net requests` | Invites, or Nodes: rows marked *requested* |
| Accept / decline | `mnnn net accept X`, `mnnn net decline X` | `a` / `d` on a request |
| Disconnect | `mnnn net disconnect X` | `r`, then `y` |

The same happens for [organizations](organizations.md) (apply, invite, join with a
link or by the org's name) and [reviews](reviews.md) (join with a link, by title, or
by accepting the invitation that arrived).

- A node you were already connected to (its key is pinned here), or one you asked
  first, is connected **at once**.
- Anyone else **asks first**: their request waits for your *accept* or *decline*. With
  `mnnn net config --accept anyone` (TUI: Settings ▸ *connect at once*) every node is
  connected at once; the default is `known`.
- Joining an org or a review still needs the owner's or author's yes where it did
  before; the keys never need anything from you.
- Nothing is shared by connecting: you still choose what to [share](network.md#sharing).

An invite link holds only public keys, addresses and a signature, so it is safe to
send by mail or chat. It is single-use, expires after 7 days, and is revoked with
`mnnn net disconnect` (or `mnnn org kick`, `mnnn review drop`). When the other node
cannot be reached at that moment, the answer is kept and delivered on its own once it
can be (the running node retries every 20 seconds).

## Who is who: trust on first use

A node is its Ed25519 key. Its peer id is a hash of that key, and libp2p's Noise
handshake proves the other end holds it, so a connection to `12D3KooW…` cannot be
answered by anyone else.

- **Pinned on first contact.** The first completed exchange pins the node's key to the
  name you know it by. Later connects, shares, org syncs and review syncs go to that
  pinned node; a name resolves to the pinned peer before anything else.
- **Signed links.** Every offer and answer is signed by the node key and bound to the
  exchange transcript; a tampered, replayed or expired link is refused.
- **A changed key is a different node.** When a node with a new key uses a name you
  already know, you are warned (`warning: … is not the node you know as ben`) and it
  is treated as a stranger: it has to ask, and nothing shared with the known one
  reaches it.
- **Checking by hand** stays possible: both sides still derive the same short code,
  shown by `mnnn net exchange show X`, and `mnnn net exchange verify X CODE` marks
  the exchange verified.

## The protocol

```text
offer      = {node key, X25519 public A, addrs, scope, expiry}  signed by the offerer
answer     = {node key, X25519 public B, offer hash, addrs}      signed by the answerer
transcript = SHA-256("muninn/net/exchange/v1" ‖ offer ‖ answer)
key        = HKDF-SHA256(X25519(a, B), salt = transcript, info = "muninn/net/exchange/v1/key")
short code = HKDF(…, info = "…/sas") as ten digits
fingerprint= first 8 bytes of the transcript, grouped
```

Offers travel as `Connect` requests (`/muninn/net/1`): the other node answers at once
(`Answered`) or keeps it as a request; the asking node polls for the answer later.
Older nodes get the offer as before. Each request over an exchange is sealed with its
key (aad `muninn/net/msg/v1|exchange|from|to`), on top of libp2p's Noise channel.
Scopes are carried in every exchange: `{"kind":"node"}`, an organization's, or a
review's.

## Advanced: by hand

`mnnn net exchange …` (hidden from the command list, but always there) does each step
by hand: `create` (a token), `accept TOKEN`, `verify X CODE`, `list`, `show X
[--tokens]`, `label`, `revoke`. Tokens `mnnn-x1-offer:` / `mnnn-x1-answer:` and links
`muninn://join/` / `muninn://answer/` are the same thing in two spellings.

## Backups

`mnnn net exchange export` writes the node identity, the exchange keys and known peers
to an age-encrypted file (your export password). `mnnn net exchange restore` brings
them back — on the same machine or a new one, where the node keeps its peer id and its
exchanges keep working. With it, [dark shards](dark-shards.md) can be gathered even
when the database itself is lost. See [backup and recovery](backup-recovery.md).
