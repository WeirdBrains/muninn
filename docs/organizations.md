# Organizations

An organization (org) compartmentalizes work. It is a publishing house (it owns
resources and decides what is published), a gatekeeper (roles decide who may do what)
and a source of membership (nodes join it through key exchanges).

An org lives on the node that made it, and the person at that node is its **owner**.
Everyone else reaches it over the [network](network.md): they apply or answer an
invitation, then act with the role they were given. Every change is recorded in the
[action history](history.md) with who made it.

## Creating and changing an org

| | CLI | TUI (`G`, Orgs menu) |
|---|---|---|
| Create | `mnnn org create NAME [--slug S] [--description D] [--public] [--open]` | Orgs page `n` |
| Change | `mnnn org edit ORG [--name] [--slug] [--description] [--visibility public\|private] [--access members\|open] [--hosted on\|off] [--member-auth on\|off] [--template T]` | `r` rename, `v` visibility, `o` access, `h` hosted, `m` member sign-in |
| Delete | `mnnn org remove ORG` (asks; `-y` skips) | `d`, then `y` |
| List, show | `mnnn org list [--json]`, `mnnn org show ORG [--json]` | Orgs page, Enter picks |

- **Visibility** is `private` by default: the org is invisible to anyone who is not a
  member, even when browsing its node. `public` orgs show up in `mnnn org browse` and on
  the org's site.
- **Access** is `members` by default. `open` lets anyone (a guest, with no membership)
  act with the org's *apply* role, minus every managing permission.
- Removing an org removes its roles, memberships, endpoints and templates. The resources
  it held (journals, projects, models…) stay in your database.

## Roles

Each org starts with four roles. Ranks order them: you can only manage members and roles
ranked below your own role (the owner is above everyone).

| role | rank | permissions |
|------|------|-------------|
| owner | 100 | everything (`*`); cannot be changed or removed |
| admin | 80 | every permission |
| user | 50 | `members.list`, `read`, `records.read`, `records.write`, `call`, `resources.transfer` |
| viewer | 10 | `members.list`, `read`, `records.read` |

Custom roles take any of these permissions (`mnnn org role perms` lists them):
`org.edit`, `roles.manage`, `members.list`, `members.approve`, `members.invite`,
`members.kick`, `members.role`, `members.defaults`, `publish`, `host`,
`projects.manage`, `resources.transfer`, `resources.manage`, `read`, `records.read`,
`records.write`, `call`.

| | CLI | TUI Roles page |
|---|---|---|
| List | `mnnn org role list ORG` | the page |
| New | `mnnn org role create ORG NAME [--rank 30] [--perm P]…` | `n` "name [rank] perms…" |
| Change | `mnnn org role edit ORG ROLE [--name] [--rank] [--perm P]… [--no-perms]` | `e` / Enter edits the permissions |
| Remove | `mnnn org role remove ORG ROLE` | `d` |
| Defaults | `mnnn org member defaults ORG [--apply ROLE] [--invite ROLE]` | `a` sets the apply default, `i` the invite default |

The defaults are the roles new members get when they apply (`user` unless changed) or
are invited (`user`); owners and admins (`members.defaults`) set them.

## Members

Membership is a [key exchange](key-exchanges.md) scoped to the org, so every member's
requests are authenticated by that exchange's key.

- **Apply:** `mnnn org browse NODE [ORG]` lists the public orgs on a node;
  `mnnn org apply NODE ORG [--note TEXT]` asks to join. The owner or an admin approves
  (`mnnn org member approve ORG MEMBER [--role R]`) or rejects (`mnnn org member reject`).
  Nothing is approved automatically. The keys are exchanged on their own: the
  applicant sees *pending*, then *connected* once approved (its running node picks up
  the answer; `mnnn org sync` does it at once).
- **Invite:** `mnnn org member invite ORG [--peer P] [--label L] [--role R]` delivers the
  invitation to that node (it shows in `mnnn net requests` and the TUI), or prints an
  invite link `muninn://join/…`; the invitee runs `mnnn org join LINK` or
  `mnnn org join ORG` for one that arrived.
- **Roles:** `mnnn org member role ORG MEMBER ROLE`.
- **Kick:** `mnnn org member kick ORG MEMBER [--resources keep|archive|publish]` ends the
  membership and revokes its key exchange. What the member gave the org stays with the
  org: kept as it is (default), archived, or published.
- **Leave:** `mnnn org leave ORG` (asks).

In the TUI: Members page `i` invite, Enter details (the token), `a` approve, `x` reject,
`o` change role, `k` kick (asks what happens to their resources); Joined page `b` browse
a node, `A` apply ("node org [note]"), `j` join with a link or the org's name, `s` sync, `m` mirror,
`l` leave, Enter details.

Member commands work on orgs of yours and on joined orgs through their node, as far as
your role there allows (an admin of a joined org can approve, kick or publish from their
own node).

## Resources and projects

An org owns resources: journals, projects, reports, models, live searches and code
spaces. Each is `active` or `archived`, and published to nobody (`none`), the org's
`members`, or everyone (`public`).

| | CLI | TUI Resources page |
|---|---|---|
| Add one of yours | `mnnn org resource add ORG KIND NAME` | `a` "kind name" |
| Give one to a joined org | `mnnn org give ORG KIND NAME` (needs `resources.transfer`; a copy) | |
| List | `mnnn org resource list ORG` | the page |
| Publish | `mnnn org publish ORG RESOURCE [--to members\|public]`, `mnnn org unpublish ORG RESOURCE` | `p` / Enter cycles none → members → public |
| Archive, restore | `mnnn org resource archive\|restore ORG RESOURCE` | `z` toggles |
| Release | `mnnn org resource release ORG RESOURCE` (the org stops holding it) | `d` |
| Fetch from a joined org | `mnnn org fetch ORG RESOURCE [--format F] [-o FILE]` | |

Org projects have their own members: `mnnn org project add ORG PROJECT`,
`mnnn org project member ORG PROJECT MEMBER [--role R]`, `mnnn org project list|remove`.

## Endpoints

Requests to an org run rules or code, gated by roles:

- `mnnn org endpoint add ORG NAME --kind rule|code|records --target T [--perm P] [--anyone]`
  - `rule`: runs a [rule](rules.md)'s then-actions with the request as fields;
  - `code`: runs a [code space](code-spaces.md) over a model;
  - `records`: queries a [model](models.md)'s records (`{"op":"query"}`), and creates or
    updates them with `records.write`.
- `--perm` is the permission a caller needs (`call` by default); `--anyone` lets guests
  call it on public or open orgs.
- `mnnn org call ORG ENDPOINT [--input JSON|-]` calls one, here or on a joined org's node.

## Templates and the site

Orgs can be published as a web site (HTML, JSON and gRPC) over HTTPS; see
[org hosting](org-hosting.md). Templates give the site its look:
`mnnn org template save ORG NAME [--html FILE] [--css FILE] [--use]`,
`mnnn org template asset ORG TEMPLATE FILE [--name N]`, `mnnn org template list ORG`.
A template fills `{{title}}`, `{{org.name}}`, `{{org.description}}`, `{{org.slug}}`,
`{{base}}`, `{{assets}}`, `{{css}}` and `{{content}}`.

## Designed with reviews in mind

Roles, ranks, published states and the history of who did what are the pieces
[reviews](reviews.md) build on: a review link is its own key exchange scoped to the one
thing under review, so a reviewer gets no other access to your node or orgs.
