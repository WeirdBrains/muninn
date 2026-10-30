# Network

Muninn nodes find each other, exchange keys and share what their owners choose:
journals, projects, reports, models (their records, and the imported bars), live
searches and code spaces. A search can run across every peer at once, and the master
key can be split into [dark shards](dark-shards.md) held by peers.

**Nothing is shared by default.** The node starts with the TUI (turn off *start with
the TUI* to stop that), and a peer sees an item only once your nodes are connected —
the [key exchange](key-exchanges.md) happens on its own — and after you share that
item. A node you do not know asks before it is connected.

## The node

Built on libp2p: TCP and QUIC, Noise encryption, Yamux, mDNS on the local network,
Kademlia for the wide network, identify, ping, AutoNAT, relays (client and server) with
DCUtR hole punching, and Muninn's own request-response protocol `/muninn/net/1`
(CBOR). The node's identity is an Ed25519 key kept in the encrypted database; its peer
id (`12D3KooW…`) is how others know it.

| | CLI | TUI |
|---|---|---|
| Start | `mnnn net start [--port N] [--listen ADDR] [--no-mdns] [--relay-server] [--for SECS]` | Net ▸ Node ▸ Start node, or `S` in the Network window |
| Stop | `mnnn net stop` (or Ctrl-C) | Net ▸ Node ▸ Stop node, or `X` |
| Status | `mnnn net status [--json]` | Net ▸ Node ▸ Status… (`K`) |
| Peers | `mnnn net peers`, `forget PEER` | Peers page: `c` connect, `l` label, `d` forget |
| Nodes | `mnnn net connect X`, `invite`, `requests`, `accept X`, `decline X`, `disconnect X` | Nodes page: `c` `n` `p` `a` `d` `r` |
| Users, invites | `mnnn user …`, `mnnn connection …`, `mnnn invite …` ([users](users.md)) | Connections and Invites pages |
| Sync your nodes | `mnnn sync …` ([sync](sync.md)) | runs on its own with the node |
| Relays | `mnnn net relay list/add/remove/serve on\|off` | Relays page: `a` add, `d` remove, `r` serve |
| Settings | `mnnn net config [--label] [--port] [--listen] [--mdns] [--autostart] [--accept] [--bootstrap]` | Settings page |

`mnnn net start` runs in the foreground. While it runs it writes
`MUNINN_HOME/net/node.json` (no secrets: pid, label, addresses), so other commands and
the TUI show its status, and `mnnn net stop` asks it to stop. Only one node runs per
home. Other `mnnn net` commands that need to talk bring up a short-lived node with the
same identity that dials out but does not listen.

Settings live in [preferences](preferences.md) under `net`: `label` (empty: the
current [author](authors.md)), `port` (4741; 0 picks one), `listen`, `mdns` (on),
`relay_server` (off), `relays`, `bootstrap`, `autostart` (on: the TUI starts the
node), `accept` (`known`: nodes you do not know ask first; `anyone`). With
`hosting.autostart` (off; `mnnn org host autostart on`, TUI Settings ▸ *web server with
the node*) the [org web server](org-hosting.md) starts with the node too.

A running node (the TUI's or `mnnn net start`) also settles exchanges in the background:
it asks for answers to requests you made, notices declines, and re-delivers answers its
peers missed. Muninn has no separate daemon: the TUI or `mnnn net start` is the node.

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

`--with NAME` connects to that node first if needed. In the TUI: Shared page `a` shares
with every connection, `d` stops; on the Nodes page `s` shares with the selected
one, `L` lists what that node shares, and Enter imports an item.

## Searching across the network

```
mnnn net search --journals --query comet          # here and on every peer
mnnn net search --saved "btc 5m" --only ben
mnnn search run "btc 5m" --network
```

Each result line starts with where it came from (`local` or the peer's label). A peer
runs the search only over what it shares with you and never returns anything else. In
the TUI the Search page runs a saved [live search](live-searches.md) across the network.

## The Net menu

Node ▸ (Status… `K`, Start node, Stop node, Settings…) · Connections… · Invites… ·
Invite or connect… · Nodes… · Peers… · Relays… · Shared data… · Network search… · Dark shards ▸ (Held &
entrusted…, Entrust…)

All open one window with ten pages: Tab (or ← →, or `1`–`9`, `0` for the tenth) moves
between them.
Work that talks to other nodes runs in the background; the title shows it, and the
window stays usable. `g` refreshes. See [keybindings](keybindings.md).

## Security

- Requests between nodes travel inside libp2p's Noise channel **and** are sealed with
  the exchange key (XChaCha20-Poly1305, bound to the exchange id and both peer ids);
  a node answers sealed requests only from the peer of that exchange.
- Invite links (and the tokens behind them) hold only public keys and signatures.
- A node's key is pinned the first time; a node with another key using a known name
  gets a warning and is a stranger (see [trust on first use](key-exchanges.md#who-is-who-trust-on-first-use)).
- Secrets (node key, exchange keys, words, shards, passwords) are never logged or
  written outside the encrypted database and encrypted backups.
- Every network action is in the [history](history.md) (`net.*`) under the current
  author, and [rules](rules.md) can react to them.
- Scopes are carried in every exchange: `{"kind":"node"}`, an organization's, or
  `{"kind":"resource","as":"review"}` for a [review](reviews.md) link.
