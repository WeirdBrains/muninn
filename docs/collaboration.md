# Live collaboration and peers

Since 2.19, nodes that exchanged keys can work together as it happens. You can message
each other, edit the same entry, journal, code space or record at once and see each
other's cursors, and run each other's served executables. The **peers** view shows who
you're connected to and what each of them has for you.

Collaboration works over any active key exchange that is a connection (node scope), a
linked node of yours, or an org membership. Share-only exchanges can't collaborate.

## Peers

```sh
mnnn peers                     # your peers: label, how you're connected, sites, unread messages
mnnn peers show ben            # what ben has for you
mnnn peers run comet-lab comet-lab sightings --input '{"op": "query"}'
mnnn peers request ben "could you share the field notes journal?"
```

`peers show` asks the peer what it offers you:

- items it shares with you (open them with `mnnn net fetch`)
- live sessions it invited you to (`mnnn live join`)
- org executables you may run (`mnnn peers run`)
- sites it announced

A peer is named by its connection label, its exchange, its node label, its peer id, or
(for an org membership) the org's slug or name.

`peers request` sends a message about `request:peers`; that's how you ask for a share, an
invitation or access.

## Messages

```sh
mnnn msg send ben "the comet is back" --about journal:4f2a
mnnn msg list                 # everything, oldest first (● unread, → sent, ← received)
mnnn msg list ben --unread
mnnn msg list --about journal:4f2a
mnnn msg read                 # mark all read (or: mnnn msg read ben)
```

Each message is kept on both nodes and syncs to each side's linked nodes. If the peer
can't be reached, the message stays marked *waiting*, and your running node retries it
(only the node that wrote a message sends it, so linked nodes never send it twice).

An arriving message is a `net.message` action. It shows up in history and as a
[notification](notifications.md) that links to the conversation, and rule sets can
trigger on it (`--on net.message`, with the fields `from`, `peer`, `about` and `text`).

## Live sessions

The node that has the item hosts the session and invites peers:

```sh
mnnn live share entry 4f2a --with ben --with cleo
mnnn live share journal "field notes" --with ben     # every entry of the journal
mnnn live share code calc --with ben
mnnn live share record Sightings/9c1e --with ben     # MODEL/RECORD
mnnn live list
mnnn live stop 7d0c
```

The host's node must be running (`mnnn net start`) to answer guests. A guest joins the
session and then edits it:

```sh
mnnn peers show anna                       # lists the session
mnnn live join anna 7d0c
mnnn live show 7d0c                        # the text now and who is here
mnnn live edit 7d0c                        # in $VISUAL/$EDITOR
mnnn live edit 7d0c --value "new text" --cursor 8
mnnn live edit 7d0c 1e9b --value "…"       # one entry of a journal session
mnnn live edit 7d0c mag --value 4.7        # a record field (a JSON value)
mnnn live stop 7d0c                        # a guest leaves
```

- **Merging.** Every edit becomes a set of operations, and they merge without loss.
  - Text (entry bodies, journal entries, code sources) is a replicated text: if two people
    type at the same place, both insertions are kept, and a deletion never removes text it
    didn't see.
  - Record fields are last writer wins, per field.
- **Writing back.** The host writes the merged text back to the item, so it syncs to the
  host's linked nodes like any edit. Edits the host makes to the item some other way
  (the TUI, the CLI, a rule) become operations too.
- **Presence.** Each round also says where your cursor is. `live show` lists who was here
  in the last 30 seconds and at which offset; the TUI marks their cursors in the text.
- **Who may join.** Only the peers the host invited can join. A guest can't forge another
  node's operations, close the session or send run results.
- **Journals.** A journal session covers the journal's existing entries. New entries
  written on the host join it; guests can't create entries.
- **Code.** A code space edited live changes its source, so it has to be approved again
  before it runs, as with any edit.
- **History.** History records `live.host`, `live.stop`, and `live.edit` (at most one per
  guest per minute).

## Running a peer's executables

`mnnn peers run PEER ORG ENDPOINT --input JSON` calls one of the org endpoints on the
peer's node, the same way an org call does: the peer checks your membership and your
role's permission for that endpoint (`records.read`, `records.write`, …), and code
endpoints only run approved code. The output comes back in chunks of a `run` session
(you'll see progress on stderr), and the whole result is printed as JSON. The peer
records a `live.run` action.

## In the TUI

`W` (or Net ▸ Peers, messages & live…) opens the Peers window with three tabs:

- **Peers**: Enter asks the selected peer what it offers. `m` messages it, `a` asks it for
  something, `J` joins an offered live session, `s` shares one of your items live with
  it, and `x` runs one of its executables (`ORG ENDPOINT [JSON]`).
- **Messages**: the conversation with the selected peer. `m` writes and `r` marks it read.
- **Live**: your sessions. Enter shows the text with everyone's cursors, `e` edits a
  document, and `x` stops (host) or leaves (guest).

## How it travels (design notes)

- **Wire format.** Sessions speak `muninn.live.v1`, protobuf messages defined in
  `proto/muninn/live/v1/live.proto`: `Batch`, `Frame` and `Op` (insert, delete, set field,
  cursor, run chunk, closed).
- **Transport.** Today a guest's round is one request over the existing key-exchange
  connection (libp2p with Noise, sealed with the exchange's keys), so it works through
  NAT and relays with the same consent and trust as the rest of the network. Guests poll
  about once a round rather than holding a gRPC bidirectional stream open.
- **The boundary.** Everything goes through `crate::collab::wire`: a `Transport` trait
  for reaching a peer, and one `serve` function for answering. Swapping in a real gRPC
  stream would only change that module.
- **Ordering.** The host keeps the operation log and its order. Guests send their
  operations and get back everything after the last one they have, so every copy applies
  the same operations in the same order.
- **Node-local data.** Sessions, operations and presence stay on the node. Only the
  merged item and the messages sync to linked nodes.
- **Run results.** A remote run is computed on the host, then its output streams back in
  chunks. The computation itself isn't streamed while it runs.
