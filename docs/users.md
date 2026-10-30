# Users and connections

People own their work, not machines. Your **user** is an identity of its own: an Ed25519
signing key, made on first use and named after the current [author](authors.md). It
certifies each node that acts for you, so a second computer or a phone is recognised as
you. Other people's users become your **connections**, trusted or untrusted.

## You and your nodes

| You want to | CLI | TUI |
|---|---|---|
| See who you are | `mnnn user show`, `mnnn user list` | Net ▸ Connections… (top line) |
| Change your name | `mnnn user label "Ada Lovelace"` | |
| Link a new node | on the new node: `mnnn user link ADDR\|PEER\|LINK` | |
| Approve it | on one of your nodes: `mnnn user approve [REQUEST]` | Net ▸ Invites…, `a` on a *link node* request, then `y` |
| See your nodes | `mnnn user nodes [USER]` | Connections, Enter |
| Revoke a node | `mnnn user revoke NODE` | |

Linking is an invite plus an approval:

1. The new node asks one of your nodes to link (`mnnn user link`, with that node's
   address, peer id or an invite link it made). The request shows as *link node*.
2. You approve it on that node. Approving says *let this node act as you*: the new node
   gets your user key and a certificate for its peer id, signed by your user.
3. The new node takes the key on its own, the next time it talks to the approver. From
   then on `mnnn user show` is the same user on both. Orgs this node owned move to the
   user.

A revoked node stays revoked: its certificate is withdrawn with a signed revocation,
your peers learn it the next time your nodes talk to them, and it no longer acts for
you anywhere (orgs included). You cannot revoke the node you are on.

## Connections

When two nodes connect ([invite link](key-exchanges.md) or `mnnn net connect`), they
also tell each other whom they act for: a card with the user's name, key and node
certificates. Cards are only taken when the certificate for the sending node checks out.
Another user becomes a connection, **untrusted** until you trust it.

| You want to | CLI | TUI |
|---|---|---|
| List connections | `mnnn connection list [--json]` | Net ▸ Connections… |
| Trust / untrust | `mnnn connection trust USER [--off]` | `t` on a connection |
| Forget one | `mnnn connection remove USER` | `d`, then `y` |

Trust is a decision you make; every change is in [history](history.md)
(`connection.trust`, `connection.add`, `connection.remove`) and rules can react to it.
Pickers follow it: shares and reviews go to any connection, [dark shards](dark-shards.md)
only to trusted ones (`mnnn net shard entrust` asks before using an untrusted one; the
TUI lists only trusted ones).

When a user links another node, their card changes and your node hears of it the next
time they talk, quietly.

## Invites

One invite system covers connections, org membership, reviews and node links. An
invite is a signed, expiring, single-use link (`muninn://join/…`) that holds no secret.

| You want to | CLI | TUI |
|---|---|---|
| Invite someone to connect | `mnnn invite create [--label NAME] [--peer PEER]` | Net ▸ Invites…, `n` |
| … to an org | `mnnn invite create --org ORG [--role ROLE]` | Orgs ▸ Members |
| … to a review | `mnnn invite create --review REVIEW` | Reviews |
| Use a link | `mnnn invite accept LINK` | Invites, `p` (paste) |
| Accept / decline a request | `mnnn invite accept X`, `mnnn invite decline X` | `a` / `d` on a request |
| See open invites and requests | `mnnn invite list [--all] [--json]` | Net ▸ Invites… |
| Withdraw an invite | `mnnn invite revoke ID` | `r` on an invite, then `y` |

A withdrawn, used or expired link is turned down by your node; whoever uses it is told
so and nothing is connected. `mnnn net invite`, `mnnn org member invite` and
`mnnn review invite` keep working.

## Orgs and reviews

Orgs belong to users: existing orgs move to the current user when it is made, and a
member's other linked nodes act with the member's role. Reviewers are named after their
user when they have set a name.

## Security

- The user key is separate from node keys and kept in the encrypted database, like
  the node key. It only leaves a node when you approve a link, sealed with that
  exchange's key.
- Certificates sign `muninn-node-cert/1`, the user id, the peer id and the time;
  revocations sign `muninn-node-revoke/1` the same way. A card claiming a node it has no
  valid certificate for is refused.
- Nothing about keys is printed, logged or put in `-v` output.

See also: [network](network.md), [key exchanges](key-exchanges.md),
[organizations](organizations.md), [reviews](reviews.md).
