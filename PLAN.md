# Muninn 2.0 Plan

Muninn 2.0 adds the next big feature set: Authorship, Action History, Live Searches, Projects, Journals, Reports, Rule Sets, the Model Designer, Code Spaces, a Decentralized Network, Organizations, Reviews, and more.

## Phases

2.0 launches with all nine phases. Versioning starts at 2.0 so that 1.x keeps headroom for backported hot and security fixes (1.16.x patches).

| Phase | Feature set | Version | Status |
| ----- | ----------- | ------- | ------ |
| 1 | Foundations: [Authorship](#authorship) + [Action History](#action-history) | v2.0.0 | ✅ Released |
| 2 | [Live Searches](#live-searches), [Projects](#projects), [Journals](#journals) | v2.1.0 | ✅ Released |
| 3 | [Reports](#reports) + exports (HTML, JSON, CSV, LaTeX, PDF, Docx) + project archives | v2.2.0 | ✅ Released |
| 4 | [Rule Sets](#rule-sets) | v2.3.0 | ✅ Released |
| 5 | [Model Designer](#model-designer) | v2.4.0 | ✅ Released |
| 6 | [Code Spaces](#code-spaces) | v2.5.0 | ✅ Released |
| 7 | [Decentralized Network](#decentralized-network) | v2.6.0 | ✅ Released |
| 8 | [Organizations](#organizations) | v2.7.0 | ✅ Released |
| 9 | [Reviews](#reviews) | v2.8.0 | ✅ Released |
| 10 | [Automatic Key Exchanges](#automatic-key-exchanges): network on by default, hosting autostart, invite links, TUI dark shards | v2.9.0 | ✅ Released |
| 11 | [Users and Connections](#users-and-connections): user identities, linked nodes, user-owned orgs, connections with trust, unified invites | v2.10.0 | 🚧 In progress |
| 12 | [Linked-Node Sync](#linked-node-sync): all of a user's data on all of their nodes | v2.11.0 | 📋 Planned |
| 13 | [Private Sites and Code Access](#private-sites-and-code-access): gated sites for private orgs, code-space role permissions | v2.12.0 | 📋 Planned |
| 14 | [Review Copies](#review-copies): what is reviewed lives on the reviewer's node in review mode | v2.13.0 | 📋 Planned |
| — | 2.0 launch wrap-up | — | 📋 Planned |

**2.0 launch wrap-up:** snapshot to `docs/historical/2.0` with this plan, the announcement, and the full changelog; then clear `PLAN.md`.

## Decisions

- **Document generation is internal, LaTeX first.** Exports are generated as LaTeX, and PDF and Docx are rendered from it inside the app, with no system tools required.
- **Code Spaces languages:** mathematical algorithms, Polars, Rust, and Python.
  - Python goes through Rython, which compiles Python to Rust and can run Python in Rust.
  - Every language ends up as a compiled binary, loaded through a Rust FFI plugin approach.
- **Key exchanges are a black box.** Nodes exchange keys by themselves when the user acts (connect, apply, invite, review, share); the user sees the action and its status (pending, connecting, connected, declined, failed). The cryptography is unchanged (Ed25519, X25519 + HKDF, XChaCha20-Poly1305, scoped exchanges, revocation).
- **Identity is trust on first use, not compared codes.** Peer ids are hashes of node keys and every connection is authenticated by them; the first exchange pins a key to a name, and a different key under a known name is a warning. Invite links are signed by the inviter and single use. Short codes stay available as an advanced check (`mnnn net exchange`, hidden).
- **Human decisions stay.** Owners approve applicants and people accept invitations; nobody handles keys. Unknown nodes ask first (`net.accept = "known"`); `anyone` connects at once.
- **The network is on by default.** The node starts with the TUI (`net.autostart = true`, older files turned on once); sharing stays opt-in per item. The web server stays off unless `hosting.autostart` is on.
- **Users own things, nodes carry them.** A user identity (its own signing key) certifies the nodes it links; orgs, memberships, reviews and connections belong to users, so any linked node can act with the user's role. Revoking a node ends its access.
- **All of a user's data is on all of their nodes.** Linked nodes sync everything end to end encrypted with keys only the user's nodes hold. Conflicts never lose data: every record keeps its versions in history, the newest wins and conflicts are shown for the user to settle. Bars sync lazily.
- **Connections replace friends.** A connection is mutual user-to-user trust made through an invite; each is trusted or untrusted (default untrusted). Shares and reviews may use any connection; dark shards go only to trusted ones.
- **Python stays on the external `rythonc`.** Python code spaces are translated by the Rython compiler run as a separate program (`MUNINN_RYTHONC`, else `rythonc` on `PATH`); Muninn does not embed a Python parser or link libpython. Packages do not bundle `rythonc`; the install docs say how to get a Rython release or `cargo install rythonc`.

---

## Authorship

> **Phase 1 · v2.0.0 · ✅ Released**

Authorship allows us to tie our name and contact info to the work that we've done.

1. An author's name, email, and other misc. details can be created
2. All these details can be applied by various features in the application

## Action History

> **Phase 1 · v2.0.0 · ✅ Released**

Action history allows us to see the steps that brought us to our current state, step back, and continue our journey.

1. Actions are saved in the database
2. When you step back and do something different, a fork of the action history is created; all forks are saved
3. They live on the UI in a collapsible bar on the left

## Live Searches

> **Phase 2 · v2.1.0 · ✅ Released**

Live searches are the means of storing your questions. This is not the results of a search, but the search itself.

1. Any search can be saved as a live search
2. Any live search can be forked and optimized
3. Live searches can be added to projects
4. Anything that is searchable can be pulled up by a live search
5. Live searches should be accessible from the main menu, through shortcuts, and in projects
6. They always act on data, not old results; hence, they always update results when data changes
7. Live searches stay with the database; no intentions are lost

## Projects

> **Phase 2 · v2.1.0 · ✅ Released** (project exports/archives: Phase 3 · v2.2.0)

Projects are a way to organize everything that you are doing. There should be:

1. A projects page
   - With a menu category in the top bar
   - With a list of current projects
   - With a button to create a new project
2. An "add to project" button on all:
   - Search results
   - Live searches
   - Results
   - Rule sets
   - Journals
3. The ability to export a project:
   - Data gets exported like a full export should, with format choices: HTML, JSON, LaTeX, Docx, PDF
   - Journal entries are exported in a format chosen by the user: HTML, JSON, PDF, LaTeX, Docx
   - Projects get exported with the full set of attachments, data, journals, etc. saved in them. They must be importable later.

## Journals

> **Phase 2 · v2.1.0 · ✅ Released** (exports and printing: Phase 3 · v2.2.0)

A journal is a place for me to write what I'm working on, thoughts, frustrations, and just how your day is going, etc.

1. Journals can live:
   - On their own
   - In projects (one to many)
2. A journal can be exported and imported
3. They can contain images and be saved in a preferred format:
   - Markdown
   - LaTeX
4. Journal entries and the entire journal can be saved as: HTML, JSON, LaTeX, PDF, Docx
5. Journal entries and even an entire journal can be printed
6. Journals can be forked and saved into a project-specific journal
7. Journal entries can be selected and added to other journals or a new one
8. Notes can be added to a journal globally, to specific entries, or even to selected words on an entry
9. Highlights can be added to selected words on an entry
10. All notes, highlights, entries, and journals will be attributed to the author

## Reports

> **Phase 3 · v2.2.0 · ✅ Released**

Reports are meant for humans to read and use. That's the goal of all this.

1. Reports will be exportable in the following formats: HTML, JSON, CSV, PDF, LaTeX, Docx
2. Reports are made of various pieces of data:
   - Model records
   - Journal entries
   - Project-related data
   - Authorship
   - Output from code spaces

## Rule Sets

> **Phase 4 · v2.3.0 · 🚧 In progress**

Rule sets define triggers that are processed when an action is done. An action could be a data import, an export, etc. Rule sets act as our "if this, then that" for the data world.

1. Emulates if-this/then-that style rules
2. They have fields and filters that meet the particular input/output actions we support
3. They are parsed and executed (if matching) every time an action is taken
4. They can live in a project, journal, live search, or on their own
5. They are stored with the database so that they live beyond one device

## Model Designer

> **Phase 5 · v2.4.0 · 📋 Planned**

The model designer allows you to create new shapes and forms for your data to be stored. It is your data playground that shapes your database.

1. Model designs create, modify, or delete new or current tables for records
2. Model designs can be created, edited, or deleted
3. Model designs act as versioned database migrations
4. They are built on top of rule sets
5. They can be used, created, modified, and deleted, in turn, by rule sets
6. Data designs must migrate with the data itself: if data is transferred, the model designs tied to it go as well, along with all rule sets that created or modified them
7. This implies that every piece of data is versioned to conform to its model design; there is a relationship between a table and its model design, as well as all the data in the table
8. There will be an easy-to-use editor to design models
9. The editor will build and execute rule sets to fit the design

## Code Spaces

> **Phase 6 · v2.5.0 · 📋 Planned**

Code spaces allow us to create algorithms that work with the data we ingest. Supported languages: mathematical algorithms, Polars, Rust, and Python (via Rython), all compiled to binaries loaded through a Rust FFI plugin approach. There should be:

1. Variables for all features (or attributes) in a model (or row) for current model types (or tables)
2. Functions to:
   1. Load records from imported data into a vector of model inputs
      - We should be able to apply filters, grouping, and sorting (all by feature) to the load request
   2. Update records
      - Update one model record by id
      - Update many model records by an attribute filter match
   3. Create new records
      - This can be useful as a result of calculations
   4. Create new model types with possible relationships to other model types
      - This implies being able to create, update, and load records for the new model types as mentioned above
3. The ability to apply your algorithms against a set of model records as defined by the load function
   - The end result will be either:
     - An error
     - A view of the results (all content view rules apply to this: tiling, etc.)

More functionality to come in the following features.

## Decentralized Network

> **Phase 7 · v2.6.0 · 📋 Planned**

The decentralized network frees us from relying on any one system. Peers can carry our data, relays can broadcast our capabilities, and live searches can reach across the network.

1. Uses libp2p for maximum support, globally and locally
2. Network becomes a main menu bar category with supporting items
3. Key exchanges are the mechanism for decrypting private data between different nodes
4. Key exchanges are always stored, done between nodes, and made restorable
5. Dark shards can be entrusted to or restored from other nodes after key exchanges

## Organizations

> **Phase 8 · v2.7.0 · 📋 Planned**

Organizations allow us to compartmentalize our work. They act as a publishing house, a gatekeeper, and a source of membership.

1. An organization can be created, modified, and deleted by the owner
2. It can have many roles (default: owner, admin, user, viewer)
3. Roles control access to an organization's resources and actions
4. It can have many memberships that each have a role (default: user)
5. Membership role defaults can be managed by members with the proper roles (default: owner, admin)
6. An organization can be public or private (default: private)
7. They can have members or be open
8. All organizations that require membership (public or private) will need Diffie-Hellman key exchanges between applying members and the organization nodes
9. Potential members can apply for membership
10. Organization members with the proper roles (default: owner, admin) can approve or reject potential members
11. Organizations will have web hosting:
    - Reports, journals, data, and record exports will be served from every organization node via HTTPS
    - Short-term certificates will be generated per node from Let's Encrypt
    - Members, if configured that way, can gain access using their shared key to authorize them
    - Organizations can select which artifacts they want published
    - Each organization node can define and execute code from code spaces
      - Members share (give) their code spaces to an organization; org nodes then choose which to execute, gated by code-space role permissions, built per node from source (builds cached) and asking before the first run of new or changed code
    - The end result is gRPC, JSON, HTML, and shared assets servable
12. Organizations can design templates to change the look and feel of their public web hosting
    - All assets used for the template will be stored with the templates and made accessible via hosting
13. Organizations can use their hosting to execute requests that may retrieve or affect stored model records
    - This can involve service plugins installed on the node
    - Or rule sets exposed over gRPC
14. Organizations can have many projects
15. Projects, in turn, can have many members
16. Organizational roles and rules apply to projects and their membership
17. Any resource transferred to an organization by a member is owned by the organization if the member is kicked out of the organization
    - It can be archived, published, or left as is by the organization

## Reviews

> **Phase 9 · v2.8.0 · 📋 Planned**

Our work is better when we involve others. The reviews system allows us to share a link with others to our journal entries, projects, live searches, and even model records.

1. Review requests create a link that initiates a key exchange for that resource
2. Reviewers can view, comment on, suggest changes to, vote on, and highlight the resource
3. Authors can accept or reject all changes, etc.
4. Reviews can be merged into the model record changes as a versioned record

## Automatic Key Exchanges

> **Phase 10 · v2.9.0 · ✅ Released**

Key exchanges happen between nodes by themselves. The user connects, applies, invites, reviews or shares, and sees what happens; there is nothing to copy back, read out or verify.

1. Connecting to a node exchanges keys automatically
    - A node that knows you, or that asked you too, connects at once; otherwise its owner sees a request to accept or decline
    - Requests, answers and declines settle by themselves whenever the nodes can reach each other
2. Keys are pinned to a node's name on first contact; another node using a known name is flagged
3. Org applications and invitations, review links and shares ride on the same flow, with only the action and status shown
4. Invitations that have to travel out of band are links (`muninn://join/…`): signed, single use, expiring and revocable
5. The token and short-code commands stay, hidden and marked advanced
6. The node is on by default with a preference to keep it off; the web server has its own start-with-the-node preference (off by default)
7. The TUI entrusts dark shards to peers picked from those with a completed exchange, and checks that they restore the key

## Users and Connections

> **Phase 11 · v2.10.0 · 🚧 In progress**

People, not machines, own their work. A user identity ties a person's nodes together, so a phone or a second computer can see and manage everything the user can.

1. A user identity has its own signing key, separate from node keys, and certifies each node it links
2. Linking a node is an invite plus an approve on an existing node; linked nodes can be revoked
3. Org ownership and membership belong to users; any linked node acts with the user's role (existing orgs move to the current user)
4. Reviews and shares recognise a user across their nodes; reviewer names come from the user identity
5. Connections are mutual user-to-user trust, made through invites, and are trusted or untrusted (default untrusted); trust changes are recorded in history
6. One invite system covers connections, org membership, reviews and node links: signed, expiring, revocable, single or limited use links aimed at a user, accepted or declined with status shown
7. Pickers use connections: shares and reviews any, dark shards only trusted ones

## Linked-Node Sync

> **Phase 12 · v2.11.0 · 📋 Planned**

Every linked node has everything on all of the user's nodes.

1. Journals, entries, models and records, projects, searches, reports, code spaces, rules, orgs, reviews, history and sensible preferences sync between linked nodes
2. Sync is end to end encrypted with keys only the user's nodes hold
3. Records keep their versions; the newest wins, conflicts are kept and surfaced, and nothing is lost silently
4. Large data (bars) syncs lazily
5. Revoking a node stops sync to it

## Private Sites and Code Access

> **Phase 13 · v2.12.0 · 📋 Planned**

Private organizations can still have a front door.

1. A private org can host a public, gated site: a landing and sign-in page for guests, everything else for signed-in members only
2. Private orgs stay invisible to guests otherwise, and invite users through links that start the key exchange
3. Roles include access to the org's published code spaces (view source, run, edit or manage), enforced on every node and endpoint
4. Each org node defines which of the org's code spaces it executes as part of hosting (per-node config), served through hosting, endpoints and gRPC, gated by those code-space permissions
5. Members give their code spaces to an org with a clear action (CLI `mnnn org give … code`, TUI); every org node syncs the source, builds it itself (builds cached) and asks before the first run of new or changed code
6. Executables are described in protobuf (`muninn.code.v1.Executable`: id, name, language, version hash, typed inputs, output shape) with a discovery call listing what an org node serves, filtered by the caller's role; each served code space gets generated message and service definitions from its declared inputs and outputs, exposed over gRPC with reflection and downloadable `.proto` files, plus JSON, all generated in pure Rust
7. A built-in global code space ships with Muninn on every node, versioned with it, fully viewable and editable or overridable (asking first on changes), and served through the same executable descriptors; web invite redemption and web membership registration and apply are executables in it, called by the gated sign-in and apply pages of org hosting
8. Every code space has a scope: global (the built-in one), user (all existing and new personal ones) or organization (with the org id, for spaces given to an org); it is shown and filterable (`--scope`) in the CLI and TUI, carried in the executable descriptor, and used in permission checks

## Review Copies

> **Phase 14 · v2.13.0 · 📋 Planned**

Reviewers work on their own node, even offline.

1. Joining a review copies what is reviewed to the reviewer's node in review mode: not editable as such, but comments, suggestions, highlights and votes work against it
2. Models and fields the records need are created or extended on the reviewer's node, additively and recorded in history as from the review
3. Review work syncs back to the author when their node is reachable; new revisions refresh the copy
4. Closing, dropping or leaving a review keeps the copy read-only and archived (removed only when asked)
