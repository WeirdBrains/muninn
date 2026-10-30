# Linked-node sync

Every node [linked to your user](users.md) keeps all of your data. A running node
(`mnnn net start`, the TUI, or `mnnn host`) pulls what changed on each of your other
nodes about once a minute; `mnnn sync now` does it right away.

| You want to | CLI | TUI |
|---|---|---|
| See when each node last synced | `mnnn sync status [--json]` | Net ▸ Connections… (top lines) |
| Sync now | `mnnn sync now [NODE]` | |
| See conflicts | `mnnn sync conflicts [--all] [--json]` | |
| See both sides of one | `mnnn sync show ID` | |
| Settle one | `mnnn sync resolve ID [--take-other]` | |
| Fetch bars | `mnnn sync bars [--from NODE] [--source S] [--symbol BASE/QUOTE]` | |

## What syncs

Journals and entries, models and their records, projects, live searches, reports, code
spaces, rules, orgs, reviews, revisions, authors, assets, users and connections, and
history. Your other nodes' history is added to each node's own (it does not move the
history position, so `mnnn history back` still steps through what was done here). Each node keeps its own copy of:

- secrets: your user key, the node's identity, key exchanges, shards and passwords;
- settings and `preferences.toml`;
- what is about this node only: rule run counts, code-space approvals and last runs,
  whether this node hosts an org, which exchange an org member or reviewer is reached by;
- imported sources and bars (see below).

## How it works

Every change to a synced row is noted in a change log. A node asks each linked node for
the changes after the last one it has, a page at a time, over the [key exchange](key-exchanges.md).
Pages are sealed a second time with a key derived from your user key, so only your own
nodes can read them, and a node answers only nodes that are linked to its user and not
revoked. A revoked node gets nothing more.

## Conflicts

For every row the two nodes remember what they last agreed on, so a node knows who
changed what:

- changed only there: it is taken;
- changed only here: it stays, and the other node takes it when it asks;
- changed on both: the newer one is kept and the other is saved as a **conflict**;
- deleted on one and edited on the other: the edit is kept, the delete becomes a conflict.

Nothing is lost: `mnnn sync conflicts` lists what was set aside, `mnnn sync show ID`
shows both rows, and `mnnn sync resolve ID --take-other` puts the set-aside row back
(`mnnn sync resolve ID` keeps what is there). A row that cannot be applied (for example
a model this node cannot rebuild) is kept as a conflict too.

## Bars

Bars can be large, so they do not sync on their own. `mnnn sync bars` asks your linked
nodes what series they have and fetches the bars you do not have yet, all series or the
ones you name with `--source` and `--symbol`.
