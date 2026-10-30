# Network

Muninn nodes find each other, exchange keys and share what their owners choose:
journals, projects, reports, models (their records, and the imported bars), live
searches and code spaces. A search can run across every peer at once, and the master
key can be split into [dark shards](dark-shards.md) held by peers.

**Nothing runs and nothing is shared by default.** The node starts only when you start
it (or turn on *start with the TUI*), and a peer sees an item only after a completed
[key exchange](key-exchanges.md) and after you share that item.

## The node

Built on libp2p: TCP and QUIC, Noise encryption, Yamux, mDNS on the local network,
Kademlia for the wide network, identify, ping, AutoNAT, relays (client and server) with
DCUtR hole punching, and Muninn's own request-response protocol `/muninn/net/1`
(CBOR). The node's identity is an Ed25519 key kept in the encrypted database; its peer
id (`12D3KooW…`) is how others know it.

| | CLI | TUI |
|---|---|---|
| Start | `mnnn net start [--port N] [--listen ADDR] [--no-mdns] [--relay-server] [--for SECS]` | Network ▸ Start node, or `S` in the Network window |
| Stop | `mnnn net stop` (or Ctrl-C) | Network ▸ Stop node, or `X` |
| Status | `mnnn net status [--json]` | Network ▸ Node status… (`K`) |
| Peers | `mnnn net peers`, `connect ADDR`, `forget PEER` | Peers page: `c` connect, `l` label, `d` forget |
| Relays | `mnnn net relay list/add/remove/serve on\|off` | Relays page: `a` add, `d` remove, `r` serve |
| Settings | `mnnn net config [--label] [--port] [--listen] [--mdns] [--autostart] [--bootstrap]` | Settings page |

`mnnn net start` runs in the foreground. While it runs it writes
`MUNINN_HOME/net/node.json` (no secrets: pid, label, addresses), so other commands and
the TUI show its status, and `mnnn net stop` asks it to stop. Only one node runs per
home. Other `mnnn net` commands that need to talk bring up a short-lived node with the
same identity that dials out but does not listen.

Settings live in [preferences](preferences.md) under `net`: `label` (empty: the
current [author](authors.md)), `port` (4741; 0 picks one), `listen`, `mdns` (on),
`relay_server` (off), `relays`, `bootstrap`, `autostart` (off).

### Behind NAT

AutoNAT tells the node whether others can reach it. A node that cannot be reached
reserves a slot on a relay (`mnnn net relay add /ip4/…/tcp/4741/p2p/ID`, or one it
learns about) and is reachable through it; DCUtR then tries a direct connection. Any
node can serve as a relay for others with `--relay-server` (or `relay serve on`).

## Sharing

```
mnnn net share journal "field notes"             # with every exchange
mnnn net share project lab --with ben            # with one exchange
mnnn net unshare journal "field notes"
mnnn net shared                                  # what you share, and with whom
mnnn net list ben                                # what ben shares with you
mnnn net fetch ben "field notes" --import        # copy it into your database
```

Kinds: `model`, `search` (live search), `journal`, `project`, `report`, `code`. Sharing
a project shares its journals and models for searches too. Sharing the `bars` model
lets peers search your imported series. Fetched items come as JSON (or a project
archive, or a document format with `--format`); `--import` adds journals, projects,
code spaces and live searches to your database, under a free name, recorded in the
[history](history.md) as `net.import`.

In the TUI: Shared page `a` shares with every exchange, `d` stops; on the Key exchanges
page `s` shares with the selected exchange, `L` lists what that peer shares, and Enter
imports an item.

## Searching across the network

```
mnnn net search --journals --query comet          # here and on every peer
mnnn net search --saved "btc 5m" --only ben
mnnn search run "btc 5m" --network
```

Each result line starts with where it came from (`local` or the peer's label). A peer
runs the search only over what it shares with you and never returns anything else. In
the TUI the Search page runs a saved [live search](live-searches.md) across the network.

## The Network menu

Node status… (`K`) · Start node · Stop node · Peers… · Relays… · Key exchanges… ·
Shared data… · Dark shards… · Network search… · Settings…

All open one window with eight pages: Tab (or ← →, or `1`–`8`) moves between them.
Work that talks to other nodes runs in the background; the title shows it, and the
window stays usable. `g` refreshes. See [keybindings](keybindings.md).

## Security

- Requests between nodes travel inside libp2p's Noise channel **and** are sealed with
  the exchange key (XChaCha20-Poly1305, bound to the exchange id and both peer ids);
  a node answers sealed requests only from the peer of that exchange.
- Offer and answer tokens hold only public keys and signatures.
- Secrets (node key, exchange keys, words, shards, passwords) are never logged or
  written outside the encrypted database and encrypted backups.
- Every network action is in the [history](history.md) (`net.*`) under the current
  author, and [rules](rules.md) can react to them.
- Scopes are carried in every exchange (`{"kind":"node"}` for now), ready for
  organizations and reviews.
