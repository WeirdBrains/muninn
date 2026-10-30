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
| 6 | [Code Spaces](#code-spaces) | v2.5.0 | 📋 Planned |
| 7 | [Decentralized Network](#decentralized-network) | v2.6.0 | 📋 Planned |
| 8 | [Organizations](#organizations) | v2.7.0 | 📋 Planned |
| 9 | [Reviews](#reviews) | v2.8.0 | 📋 Planned |
| — | 2.0 launch wrap-up | — | 📋 Planned |

**2.0 launch wrap-up:** snapshot to `docs/historical/2.0` with this plan, the announcement, and the full changelog; then clear `PLAN.md`.

## Decisions

- **Document generation is internal, LaTeX first.** Exports are generated as LaTeX, and PDF and Docx are rendered from it inside the app, with no system tools required.
- **Code Spaces languages:** mathematical algorithms, Polars, Rust, and Python.
  - Python goes through Rython, which compiles Python to Rust and can run Python in Rust.
  - Every language ends up as a compiled binary, loaded through a Rust FFI plugin approach.

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
    - Each organization node … *(to be completed)*
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
