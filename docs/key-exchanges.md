# Key exchanges

Two nodes share data only over a completed key exchange: an authenticated X25519
Diffie-Hellman between their Ed25519 node identities.

## How it goes

1. **Offer.** One side runs `mnnn net exchange create [--label ben] [--peer ADDR]`
   (TUI: Key exchanges page, `n`). This prints a token `mnnn-x1-offer:…`; with `--peer`
   it is also delivered straight to the other node, which lists it as *incoming*.
2. **Accept.** The other side runs `mnnn net exchange accept TOKEN-or-ID` (TUI: `a`,
   on an incoming exchange or with a pasted token). Its answer goes back to the
   offerer when it can be reached; otherwise it prints `mnnn-x1-answer:…` to pass back,
   and the offerer completes with `mnnn net exchange accept ANSWER`.
3. **Verify.** Both now hold the same key and the same short code, `12345-67890`.
   Read it to each other (phone, in person) and run
   `mnnn net exchange verify ben 12345-67890` (TUI: `v`). Matching codes rule out a
   man in the middle.

Tokens hold only public keys, addresses and signatures, so they are safe to paste in
mail or chat. Offers expire after 7 days. Replayed or tampered tokens are refused.

## The protocol

```text
offer      = {node key, X25519 public A, addrs, scope, expiry}  signed by the offerer
answer     = {node key, X25519 public B, offer hash, addrs}      signed by the answerer
transcript = SHA-256("muninn/net/exchange/v1" ‖ offer ‖ answer)
key        = HKDF-SHA256(X25519(a, B), salt = transcript, info = "muninn/net/exchange/v1/key")
short code = HKDF(…, info = "…/sas") as ten digits
fingerprint= first 8 bytes of the transcript, grouped
```

Each request over the exchange is sealed with that key (aad
`muninn/net/msg/v1|exchange|from|to`), on top of libp2p's Noise channel.

## Managing exchanges

| | CLI | TUI (Key exchanges page) |
|---|---|---|
| List / show | `mnnn net exchange list`, `show X [--tokens]` | list; Enter shows code, fingerprint, tokens, shares |
| Rename | `mnnn net exchange label X NAME` | `l` |
| Revoke | `mnnn net exchange revoke X` | `r`, then `y` |
| Back up | `mnnn net exchange export --out FILE` | `e` shows the command |
| Restore | `mnnn net exchange restore FILE` | — |

Revoking erases the key here and tells the peer, which erases it too; the record stays
in the [history](history.md).

## Backups

`mnnn net exchange export` writes the node identity, the exchange keys and known peers
to an age-encrypted file (your export password). `mnnn net exchange restore` brings
them back — on the same machine or a new one, where the node keeps its peer id and its
exchanges keep working. With it, [dark shards](dark-shards.md) can be gathered even
when the database itself is lost. See [backup and recovery](backup-recovery.md).
