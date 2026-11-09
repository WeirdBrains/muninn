# Muninn Plan

Muninn 2.0 is complete (v2.0.0 through v2.20.0). Its full plan, announcement and
changelog are archived in [`docs/historical/2.0/`](docs/historical/2.0/), and 1.0's in
[`docs/historical/1.0/`](docs/historical/1.0/).


## Phases

| Phase | Feature set | Version | Status |
| ----- | ----------- | ------- | ------ |
| 1 | [Code Spaces as Collections of Code](#code-spaces-as-collections-of-code): today's code spaces become **code** (single units). A new **code space** is a collection of code with a required **main**, which can import and call other code. Pipelines run spaces (`space:NAME`), and an offered `mnnn code migrate` turns each code into a space | v2.23.0 | 📝 Planned (reworked Nov 8, 9:43 AM) |
| 2 | [Plugins](#plugins): a required `kind` on every **code**, `mnnn plugins` with kind migration, a Plugins menu per kind; and the [Code spaces + Code menus and CLI](#code-spaces-menu-and-cli) | v2.24.0 | ✅ Done (Nov 8, on main; no tag) |
| 3 | [Rule Sets + Scheduler](#rule-sets--scheduler): a first-class `rulesets` table, the system scheduler with `schedules` and `schedule_runs`, scheduled rules moved onto it, `rule tick` deprecated | v2.25.0 | ✅ Done (unreleased) |
| 4 | [Ingest Connector Plugins](#ingest-connector-plugins): code spaces whose main is of kind `ingest-connector`, managed under Plugins ▸ Ingest connectors, run on a schedule; they store raw data, call a pipeline and a rule set | v2.26.0 | ✅ Done (unreleased) |
| 5 | [Shares: Create Then Share](#shares-create-then-share): wherever a share can be added, the thing can be created there too (code and code spaces both) | v2.27.0 | ✅ Done (unreleased) |

**Order:** phase 1 now comes first, because it renames the table every later phase builds on (`code_spaces` → `code`) and changes what a pipeline step runs. Kinds (phase 2) attach to **code** after the rename, so the paused kind work is rebased onto the `code` table rather than shipped against `code_spaces` and moved a release later. The scheduler (3) and connectors (4) follow as before, and shares (5) stay independent. Each phase is a minor release. App work in every phase (the Flutter app, `app/`) is **Matematik's area**: here it is planned only as surfaces matching the TUI, through `muninn-ffi`.

**Paused implementation:** the phase-2 kind work started on Nov 8 (`kind`/`enabled` columns on `code_spaces`, SDK 1.1.0 `plugin!(run, kind = "…")`, `mnnn plugins` with migrate, the `code-spaces` alias) is on a local branch, unpushed. It gets rebased onto the `code` table after phase 1. The `code-spaces` alias on `mnnn code` is dropped (see Q-C1, revised).

## Next phase

Phase 1, [Code Spaces as Collections of Code](#code-spaces-as-collections-of-code), now that Michael has answered Q-CS1 to Q-CS10 (9:50 AM). The paused branch `phase1` (`caa1a60`, `/tmp/p1` on alien-blue) is **superseded**: it is redone on this design and kept until then.

---

## Code Spaces as Collections of Code

> **Phase 1 · v2.23.0 · 📝 Planned**

### Michael's words (Nov 8, 9:38 and 9:43 AM)

"## Code spaces
Code spaces should be a collection of code.
- the current code spaces should be changed to code
  - a main code must be created
    - it can import and execute code that's already been created
    - it can also just be code
    - it is the entry point for a code space to run
  - the database needs to change to accommodate
  - pipelines need to change to accommodate this
  - pipelines now run code spaces, but the main code is triggered.
- a code space can be created that has a collection of code."

"Also, a migrate option should be open to migrate current code to a code space.
- a code space is created and named as the code
- the code is migrated to become the main of the code space
- name collisions in the code spaces should be handled by adding a -migration postfix to the code space name
- if there is already a code space with a -migration postfix, the code should be skipped in the migration"

### Terms

- **Code:** one unit, which is what a "code space" is today. It has a source (math, Polars, Rust, Python, or a prebuilt library path), a language, an approval hash and a cached built library. **Code has no kind** (Michael: Q-CS7, the space carries it). It can run on its own as before (`mnnn code run`).
- **Code space:** a named collection of code with a **kind** (`parser`, `transform`, `analysis` or `ingest-connector`) and exactly one **main**. Running a space means running its main. A piece of code can belong to several spaces, because membership doesn't copy it.
- **Main:** the space's entry point. It can be plain code that imports nothing (the common case, and what migrate makes), or it can import other members and call them.

### Schema (one migration, ids kept)

1. `ALTER TABLE code_spaces RENAME TO code;`
   - Every id stays the same, so references by id keep working unchanged:
     - project items (`ItemKind::Code`, `ref_id`)
     - `code_scopes` (global/user/org)
     - org resources and org code syncs
     - shares
     - the approval hash, which is a column on the row
     - history params (`code.*` actions keep `"id"`)
     - pipelines, which refer to code by **name** in `code:NAME` and are unaffected by the rename
   - `code_scopes` keeps its name and now means code. Peer and org wire formats keep sending the same JSON (`CodeSpace`, renamed `Code` in Rust with a serde alias), so 2.22 peers interoperate.
2. New tables, following the repo's conventions (TEXT uuid ids, epoch ms UTC):

```sql
CREATE TABLE code_spaces (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    description TEXT NOT NULL DEFAULT '',
    main_code_id TEXT NOT NULL REFERENCES code(id) ON DELETE RESTRICT,   -- a space always has a main
    kind TEXT NOT NULL CHECK (kind IN ('parser','transform','analysis','ingest-connector')),  -- Q-CS7
    enabled INTEGER NOT NULL DEFAULT 1,
    approved_hash TEXT,              -- the space's approval: its membership (below, Q-CS3)
    author_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE code_space_members (
    space_id TEXT NOT NULL REFERENCES code_spaces(id) ON DELETE CASCADE,
    code_id TEXT NOT NULL REFERENCES code(id) ON DELETE RESTRICT,
    alias TEXT NOT NULL COLLATE NOCASE,   -- the name main imports it by (default: the code's name)
    position INTEGER NOT NULL,            -- display order; main is position 0
    pin_hash TEXT,                        -- NULL (default): follow the current source; set by --pin (Q-CS5)
    added_at INTEGER NOT NULL,
    PRIMARY KEY (space_id, code_id),
    UNIQUE (space_id, alias)
);
```

   - Main is always a member, at position 0. A trigger, or the store's insert path, adds the row, and removing main from the members is refused: set another main first.
   - Deleting code that is a member of any space is refused with the list of spaces ("remove it from spaces A, B first"), hence `ON DELETE RESTRICT`.
   - `code_space_scopes` mirrors `code_scopes` so spaces can be user, org or global. Org resources gain the kind `space` next to `code`.

### Main, imports and calling code (Michael: Q-CS1, Q-CS2, Q-CS3, Q-CS5)

- **What main can call (Q-CS1, resolved):**
  - **members of its own space:** `host.call(alias, frame) -> Frame` (and `host.call_view`)
  - **whole other spaces:** `host.call_space(name, frame) -> Frame`, which runs that space's main, with its own members, as if it were a `space:` pipe step
  - In Python these are `call(...)`/`call_space(...)`; in math and Polars, `call(alias)`/`call_space(name)` step functions.
- **Who may call (Q-CS2, resolved):** only a space's **main**, for both member calls and cross-space calls. Members can't call anything: their `call`/`call_space` fail with "only a space's main calls code".
- **Declared imports:** main lists what it calls in a header, `@imports parse-csv, space:cleaner`. Build and approve check that each plain alias is a member and each `space:NAME` exists. An undeclared call fails at run time.
- **Mechanism:** a runtime host call through the existing JSON `host.call(name, args)` channel. ABI 1 is unchanged; the SDK goes to 1.2.0. Each code is its own cached library, so calls work across languages.
- **Cycles and depth (Q-CS1, resolved):**
  - Spaces form a graph through main's `space:` imports.
  - Creating or editing a space, setting its main, and approving it all run a DFS over that graph. A cycle is refused, with the path named (`a → b → a`).
  - At run time a call stack of space ids enforces the same rule (re-entry is an error), plus a **depth limit of 8** nested spaces, for edits that race the check.
  - Member calls add no depth, since members can't call.
- **Following and pinning (Q-CS5, resolved):**
  - Members follow their code by default (`pin_hash` NULL).
  - Following means the member runs the code's **current** source, including unreviewed edits (Michael: Q-CS11).
  - `--pin` (`mnnn spaces add SPACE CODE --pin`, `mnnn spaces pin SPACE CODE`) records the code's current trust hash. A pinned member runs only while the code's current hash equals the pin; otherwise calls fail with "X changed since pinned: `mnnn spaces pin SPACE X` to accept". `pin`/`unpin` change it. No old versions are stored.
  - There are no version numbers: the trust hash is the version.
- **Approval (Q-CS3, resolved; reconciled with following):** a space approval covers **the membership, not the sources**. Sources are already covered by each code's own approval.
  - `approved_hash` = blake3 over:
    - the main's code id
    - the space's kind
    - the sorted `(alias, code id, pin_hash)` of every member
    - the sorted `space:` names main imports
  - Adding or removing a member, changing an alias, a pin, the main, the kind or main's `@imports` needs the space approved again.
  - Editing a member's source does **not**, and **an edited member runs its edited code immediately**, with no approval wait (Michael: Q-CS11, reversed at 9:55 AM). Unpinned members always run the current source.
  - Main follows the same rule. If main's edit changes its `@imports`, that changes the line-up, so the space must be approved again before it runs.
  - **Security implication:** approval governs only the space's line-up. Code edits inside an approved space run unreviewed. Every edit is logged in history (`code.edit` with the new trust hash), and the CLI, TUI and app flag such members as "edited since the space was approved (unreviewed)" until someone approves the code. Pinning is the opt-in for a reviewed, frozen member.
  - Running code **alone** (`mnnn code run`, `code:` steps) still requires that code's own approval, as today.
  - Approving a space in the CLI or TUI shows main, every member and the called spaces in one review, and offers to approve any of them that are unapproved in the same step (each recorded in history).
- **Running a space** therefore checks that:
  - the space is enabled and approved for its current membership
  - each member called (main included) matches its pin, if pinned; unpinned members run their current source, approved or not
  - each called space passes the same checks, recursively, within depth 8
- **Kinds of called spaces:** a called space must be `parser`, `transform` or `analysis`. Calling an `ingest-connector` space is refused: connectors run only from schedules.

### Pipelines

- **New step and source:** `space:NAME` runs the space, which triggers its main, with the step's input frame as `host.input()` and `?options` as today. Pipelines (graph JSON) gain `NodeKind::Space { space }`, and `pipe` accepts `space:NAME`.
- **`code:NAME` (Michael: Q-CS4, resolved):** it keeps working for one release, deprecated. It runs that code alone, with a note in `pipeline check`/`show` and the TUI, and then becomes an error pointing to `space:`.
- **`mnnn migrate` rewrites stored pipelines** from `code:NAME` to `space:<the space made for NAME>`, keeping options. Each rewritten pipeline gets a new version (the old graph stays in pipeline history), plus a `pipeline.edit` action. `--no-rewrite-pipelines` skips this, and skipped code keeps its `code:` steps.
- `Graph::code_spaces()` is split into `codes()` and `spaces()`. Pipeline shares (`pipeline_share.rs`) bundle the spaces, their members and pins.

### `mnnn migrate`: one command (Michael: Q-CS8, 9:43 AM)

`mnnn migrate` is the **only** migrate command: `mnnn plugins migrate` and `mnnn code migrate` are dropped. The schema rename is automatic; this command is **offered, not forced** (Q-CS9, default accepted pending objection).

```
mnnn migrate [CODE…] [--kind K] [--dry-run] [--no-rewrite-pipelines] [--no-backup] [--json]
```

It does three things in order, with one combined report:

1. **Code → code spaces:** for every code that is **not a member of any space** (or only those named):
   - It creates a space named as the code, with the code as its main and only member (following, unpinned).
   - If the name is taken (case-insensitive), it names the space `NAME-migration`.
   - If `NAME-migration` is also taken, it **skips** that code.
   - If `NAME-migration` would exceed the 80-character name limit, it skips and reports "name too long" (Q-CS10, default accepted pending objection).
   - If the code was approved, the new space is approved for its one-member membership, so nothing that ran before stops running.
2. **Kinds for the new spaces.** It infers each kind:
   - the built-in parsers → `parser`
   - `to-bars`, or code used only as a pipeline step → `transform`
   - code that runs on its own → `analysis`
   - Unclear ones are asked about interactively, take `--kind`, or are skipped with `-y` (the space isn't created; reported).
   - Spaces always have a kind, so only spaces being created need one.
3. **Pipeline rewrite:** `code:NAME` becomes `space:…` (above), unless `--no-rewrite-pipelines`.

- **Report:** **created** (`NAME`, kind), **renamed** (`NAME` → `NAME-migration`), **skipped** (`NAME`: why) and **pipelines rewritten**. `--json` gives the same.
- **`--dry-run`:** the same report, nothing changed.
- **Backup:** `MUNINN_HOME/backups/migrate-<UTC>.json` (affected pipelines' graphs and the code rows) is written first, unless `--no-backup`, and a `migrate` action is recorded.
- **Idempotent:** code that is already a member isn't picked again, so a second run changes nothing.
- **Offered on upgrade** (Q-CS9):
  - CLI: a one-time notice on the first code-related command, and a reminder in `mnnn status`.
  - TUI: a one-time dialog (Migrate now… / Later / Don't ask again), with **Code ▸ Migrate…** kept.
  - App (Matematik): a banner and a Settings entry.
  - Unmigrated code keeps running through `code:` and `mnnn code run`.

### Surfaces

- **CLI (Michael: Q-CS6, resolved):** `mnnn code` = single code (create, edit, show, approve, build, run, check, remove, sdk, parsers). **`mnnn spaces`** (aliases `code-spaces`, `space`) = collections. `mnnn migrate` is top level:

```
mnnn spaces list [--kind K]|show|create NAME --kind K --main CODE [--add CODE[=ALIAS]…]|add SPACE CODE [--as ALIAS] [--pin]
mnnn spaces remove-code SPACE CODE|set-main SPACE CODE|pin|unpin SPACE CODE|approve SPACE|run SPACE [--input SRC]|enable|disable|remove SPACE
```

  - `create` without `--main` asks (or `--new-main LANG` writes a fresh main from a template, which imports nothing).
  - Q-C1 is revised accordingly (see Open Questions).
- **TUI:** a **Code** main menu (Alt+C) with **Code spaces…** (collections: kind, members list, main marked ★, `a` add code, `M` set main, `P` pin/unpin, `r` run, approve) and **Code…** (single units, today's window), plus New code space…, New code…, Migrate…, Live pipes…, Pipelines…, Plugin SDK…. Projects ▸ Code spaces… stays one release as a pointer.
- **App** (Matematik): Code spaces and Code destinations with the same items; the migrate banner.
- **MCP:** the tools gain `space_run`/`space_list`, and `code_*` tools keep their names.
- **Docs:** `docs/code-spaces.md` is split into `docs/code.md` (units, SDK) and `docs/code-spaces.md` (collections, main, imports, approval, migrate). Plus `docs/pipelines.md` (`space:`), menus, keybindings, help topics and man pages.

### Tests

- **Schema:** the rename keeps ids and every reference (project items, scopes, org resources, shares, history lookups, approvals). A 2.22 database with code, pipelines and org shares opens on 2.23 and everything still runs.
- **Spaces:**
  - main is required and always a member
  - main can't be removed while it is main
  - member code can't be deleted
  - aliases are unique per space
- **Imports:** `host.call` of a member returns its frame; an undeclared or non-member alias fails; a member calling is refused; cross-language calls work (a Python main calling a Rust parser).
- **Approval:** the space hash covers the line-up; an edited unpinned member or main runs its edit at once without re-approval, is flagged unreviewed, and has a `code.edit` history entry; an edited pinned member fails until re-pinned; an `@imports` change needs the space approved again; running code alone still needs the code approved.
- **Pipelines:** `space:` steps and sources run main; `code:` still runs alone, with a deprecation note.
- **Migrate:**
  - one space per non-member code, named as the code
  - NAME taken → `NAME-migration`
  - both taken → skipped and reported
  - the name-length limit
  - `--dry-run` changes nothing
  - a second run creates nothing
  - approvals carry over
  - pipelines are rewritten with a version bump, and `--no-rewrite-pipelines` leaves them
  - the one-time notice and dialog are shown once
- **Peers and orgs:** 2.22 peers still exchange code, and spaces share with their members.

---

---

## Plugins

> **Phase 2 · v2.24.0 · ✅ Done (Nov 8).** Shipped as reworked: `mnnn plugins` over spaces, the Plug menu, the prebuilt declared-kind check. Choices: no `n`/`d` keys in the Plugins window (install and remove are CLI; the window keeps the code spaces keys); `remove` refusing while a schedule uses it comes with phase 3; ffi gains `plugins_list(kind)` and `plugin_set_enabled(name, on)` (frb bindings not regenerated: Michael's app work).

**Reworked for phase 1 (Michael: Q-CS7, Q-CS8, resolved):**
- **The space carries the kind; code has none.** This is the simplest coherent option: one place to look, and code can be shared by spaces of different kinds.
- **Validation:**
  - a space's kind is required and is one of the four
  - an `ingest-connector` space gets the connector host methods (`host.pipe`, `host.run_ruleset`, `host.fetch`) for its **main** only
  - parser, transform and analysis spaces never get them
  - members can be any code, because code has no kind
  - a space can only `call_space` parser, transform or analysis spaces
  - the "manifest" is the space row (kind, main, members); a prebuilt library's SDK 1.1+ `kind` in its info, if present, must equal the kind of every space it is main of
- **Menus:** the Plugins ▸ <kind> menus list **spaces** of that kind, and Plugins ▸ All plugins lists all spaces.
- **CLI:** `mnnn plugins list/info/enable/disable` work on spaces, and `install` adds a prebuilt library as code plus a one-member space of `--kind`. There is **no `mnnn plugins migrate`**: kinds are assigned by `mnnn migrate`.
- The paused branch's `kind` column on code is dropped in the redo.

### Michael's words

"Put the ability to manage ingestion plugins in a new menu category: plugins -> ingest connectors.
- All plugins can be managed from plugins -> <plugin kind>
- All plugins must now have a kind field in their manifest
- A plugins subcommands should be added to the CLI
  - It should include a migrate subcommand to migrate old plugins"

### Today

- There is no manifest file. A plugin is a code space (`code_spaces` row: `lang`, `source`, approval hash) built into a native library. Its only self-description is `muninn_plugin_info()`, static JSON `{"sdk": "1.0.0", "lang": "rust"}`. A prebuilt plugin (`--lang plugin`) is a library made elsewhere with `mnnn code sdk`.
- Kinds exist only by convention:
  - the built-in **parsers** `parse-csv`, `parse-json`, `parse-ndjson` (`mnnn code parsers`)
  - the **transform** `to-bars`, used as `code:NAME` steps in pipes and pipelines
  - everything else: **analysis** code spaces run for a table/chart view (`mnnn code run`, `@view`)
- They are managed as code spaces: `mnnn code …` (alias `codes`), the TUI's **Projects ▸ Code spaces…**, and the app's Projects windows.

### The manifest and `kind`

- **Manifest** = the plugin's info JSON, extended. The SDK adds `"kind"` (and `name`, `version`, `description`), read from a `@kind` header in the source or set at build time: `{"sdk": "1.1.0", "lang": "rust", "kind": "parser", "name": "parse-csv", "version": "1"}`. The `code_spaces` table gets `kind TEXT` to match, so lists don't load libraries.
- **One kind per plugin (Michael: Q-P2).** Kinds (v1): `parser` (raw bytes → rows), `transform` (frame → frame, a pipe step), `analysis` (runs for a view), `ingest-connector` (phase 3).
- **Validation:**
  - `kind` is required: lowercase, one of the known kinds, and the same in the library and its `code_spaces` row.
  - An unknown kind is rejected and the error lists the known ones.
  - The kind also decides where a plugin may be used. A parser or transform can be a `code:` pipe step; an `ingest-connector` only runs from a schedule; `analysis` runs on its own or over a share.
- **Without a kind:** loading, running, building or approving a plugin with no kind is **rejected**: "NAME has no kind; run `mnnn migrate` (or `mnnn migrate NAME --kind KIND`)" *(superseded by Q-CS7/Q-CS8: spaces always have a kind; unmigrated code runs alone until `mnnn migrate`)*. Nothing runs half-migrated. Until then, the TUI and app show a "needs migrating" badge with a Migrate button.
- **SDK:** the SDK goes to 1.1.0 with `plugin!(run, kind = "parser")`. ABI version 1 is unchanged: only the info JSON gains fields.

### `mnnn plugins`

```
mnnn plugins list [--kind K] [--json]            # name, kind, lang, approved, enabled, source of truth
mnnn plugins info NAME [--json]                  # the manifest, approval, where used (pipes, pipelines, schedules)
mnnn plugins install PATH|SPACE --kind K [--name N]   # a prebuilt library, or a code space as a plugin
mnnn plugins remove NAME                         # asks; -y skips; refuses while a schedule uses it unless --force
mnnn plugins enable|disable NAME                 # disabled plugins don't run (pipes, pipelines and schedules fail clearly)
# migrate: removed (Q-CS8); see `mnnn migrate` in phase 1
```

`migrate`:
- **Infers the kind** for each plugin without one:
  - the built-in parsers → `parser`
  - `to-bars`, and any plugin used only as `code:` steps → `transform`
  - one with an `@view` or run on its own → `analysis`
  - Anything ambiguous is asked about, unless `--kind`; with `--yes`, unclear ones are skipped and listed.
- **Rewrites:**
  - sets `code_spaces.kind`
  - adds `@kind` to the source of source-built plugins
  - rebuilds them; the approval stays when only the kind was added (Michael: Q-P3); any other change needs approving again
  - prebuilt libraries can't be rewritten: their kind is recorded on the row, and `info` says to rebuild with SDK 1.1.0
- **Backups:** writes `MUNINN_HOME/backups/plugins-migrate-<UTC timestamp>.tar.zst` first (rows and sources), unless `--no-backup`.
- **`--dry-run`:** prints each plugin, the kind it would get and why, and changes nothing.
- **History:** a `plugins.migrate` action is recorded.
- The first start of 2.23 offers to migrate once.

### Menus

- **TUI:** a new **Plugins** main menu, with one entry per kind: **Parsers…**, **Transforms…**, **Analysis…**, **Ingest connectors…** (phase 3), plus **All plugins…** and **Migrate…**. Each opens a Plugins window filtered to the kind, with `n` new or install, Enter info, `e` edit (opens the code space), Space enable or disable, `d` remove and `m` migrate.
  - **Short labels (Michael: Q-P1).** The bar becomes **File · View · Data · Proj · Code · Plug · Net · Orgs · Keys · Theme · Help**.
    - Width: each label plus 2, plus 1 = 67 columns, leaving 13 of 80 for the `F10 menu` hint (dropped when there is no room, as now).
    - The full names would take 74 columns and leave almost no room for the hint.
    - Mnemonics: Alt+P Proj, Alt+C Code, Alt+L Plug; the others are unchanged.
    - The full names stay as dropdown headers, in help, in the docs and in the app.
- **App** (Matematik): a **Plugins** destination with the same per-kind sub-entries, under "More" on phones.

### Docs and tests

- **Docs:** `docs/plugins.md` (kinds, the manifest, `mnnn plugins`, migrate); `docs/code-spaces.md` (SDK 1.1, `@kind`); `docs/menus.md`; `docs/keybindings.md`; man pages.
- **Tests:**
  - kind validation, including rejecting a plugin without a kind with the migrate hint
  - inference for each built-in and for usage patterns
  - migrate `--dry-run` changes nothing, the backup round-trips, and running migrate twice changes nothing
  - a disabled plugin fails pipes clearly
  - TUI menu, window and key tests; FFI and app tests against the fake API

## Code Spaces Menu and CLI

> **Phase 2 · v2.24.0 · 📝 Planned (menus land with phase 1's Code menu; see below)**

**Reworked for phase 1 (Q-C1 and Q-CS6, resolved):** the Code menu, holding **Code spaces…** (collections) and **Code…** (units), now ships in phase 1, because the split needs it. `mnnn code` is the units and `mnnn spaces` (alias `code-spaces`) is the collections. That replaces Q-C1's "`code-spaces` as an alias of `code`" (see Q-C1, revised). The items below stand, with "code spaces" meaning collections wherever a space is run or shared.

### Michael's words

"There should be a main menu category for code spaces in the TUI and App, and a subcommand in the CLI."

### Today and plan

- **CLI:** `mnnn code` already exists (alias `mnnn codes`) with list, create, edit, show, approve, build, run, check, remove, sdk and parsers. It stays `mnnn code` (alias `codes`), with `mnnn code-spaces` added as a further alias (Michael: Q-C1).
- **TUI:** code spaces move from **Projects ▸ Code spaces…** to a main **Code** menu, Alt+C, with:
  - Code spaces…
  - New code space…
  - New from a parser ▸
  - Run…
  - Build…
  - Check toolchains
  - Live pipes…
  - Pipelines…
  - Plugin SDK…

  Live pipes and Pipelines move from Projects into Code (Michael: Q-C2); Projects ▸ Pipes and Pipelines stay one release as pointers. Projects ▸ Code spaces… stays one release as a pointer.
- **App** (Matematik): a **Code spaces** main destination with the same items.
- **Code spaces and Plugins:** a code space is the source you write; a plugin is what it builds, plus prebuilt libraries. Code ▸ manages sources, and Plugins ▸ manages kinds and installs. Both open the same item.
- **Tests:** menu and keybinding tests, and the alias in the CLI tests. Help topics and the docs (`menus.md`, `code-spaces.md`) are updated.

## Rule Sets + Scheduler

> **Phase 3 · v2.25.0 · ✅ Done on main (unreleased) (Michael answered Q1, Q2 and Q5)**
>
> **Built:** tables `rulesets`, `ruleset_rules`, `schedules`, `schedule_runs`, `scheduler_lease` with the 2.25 move; `crate::schedule` (parsers, stepping, scheduler); CLI `ruleset`, `schedule`, `scheduler`; TUI Rules ▸ Tab (Rule sets, Schedules); ffi `schedules` (list, runs, run-now, enable, tick); docs/schedules.md.
> **Choices (corrected):** a continuous daemon (last tick within 120 s) that crosses a due time runs it even after a tiny `once` window has closed; a restart after `ends_at` records missed (Q-T6) — no grace that fires one-offs late. `PAST_SLACK_MS` (60 s) is only for add/edit past-start UX. `catch_up all` queues every missed slot (max 100); `overlap` looks at runs open before the pass. Runs execute in-process, one after another, up to 4 a pass. Retries: 30 s doubling, max 1 h, never at/after ends_at; `schedule_runs.not_before` holds the wait. Fixed-ms parts are exact (a day is 24 h across DST); months and cron use wall time in the zone; cron in a zone uses the zone's offset, rechecked after a DST change. `rule edit --every/--cron` migrates onto a schedule (clears `spec.schedule`). A rule created only into a set (`--ruleset`, no `--on`) reacts to `schedule`. Migrated schedules use the node's IANA zone. Schedule names default to the rule name (suffix 2, 3 … on a clash). Removing a rule set used by schedules needs `--force` (removes them). Connector targets arrive in phase 4.

**Reworked for phase 1:** unchanged, except that a schedule's plugin target is a **code space** (`space_id`), and Muninn triggers its main.

### Rule sets as a table (Michael: "add a first-class `rulesets` table")

```sql
CREATE TABLE rulesets (
    id TEXT PRIMARY KEY,                        -- uuid
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    description TEXT NOT NULL DEFAULT '',
    owner_kind TEXT NOT NULL DEFAULT 'standalone'
        CHECK (owner_kind IN ('standalone','project','journal','search','model','org')),
    owner_id TEXT,                              -- NULL when standalone
    enabled INTEGER NOT NULL DEFAULT 1,
    author_id TEXT,
    created_at INTEGER NOT NULL,                -- epoch ms, UTC
    updated_at INTEGER NOT NULL
);
CREATE TABLE ruleset_rules (
    ruleset_id TEXT NOT NULL REFERENCES rulesets(id) ON DELETE CASCADE,
    rule_id TEXT NOT NULL REFERENCES rules(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,                  -- run order
    PRIMARY KEY (ruleset_id, rule_id)
);
CREATE INDEX ruleset_rules_rule ON ruleset_rules(rule_id);
```

- A rule can be in any number of rule sets, and a rule set runs its enabled rules in `position` order.
- **Running a rule set** on an action means each rule's `--if` is checked against it, then its `then` actions run. This is today's rule engine, scoped to the set's members.
- Event-triggered rules (`--on import`, …) still react to every action, whatever sets they are in.
- **Migration from project rule sets:** each project's rules (`project_items` of kind rule) become a rule set owned by that project, named after it, keeping the order they were added in. Projects keep showing "Rules" as before, now from their rule set. This runs once and is recorded as a `ruleset.migrate` action.
- Rule sets are shareable and an org resource kind, like rules (phase 4 adds create-then-share).

**CLI:**

```
mnnn ruleset list|show|create|edit|remove NAME
mnnn ruleset add|drop NAME RULE [--position N]
mnnn ruleset run NAME [--action ID] [--dry-run]
```

`mnnn rulesets` is an alias, and `mnnn rule create --ruleset NAME` adds the new rule to a set.

**Menus:**
- **TUI:** Projects ▸ **Rule sets…** sits next to Rules…. Its window has `n` new, `a` add rule, `x` drop rule, `J`/`K` reorder, `r` run (dry-run first) and `s` schedule.
- **App** (Matematik): the same window.

### Schedules (Michael: scheduled rules move onto the new table; cron supported; a schedule targets a connector or a rule set)

```sql
CREATE TABLE schedules (
    id TEXT PRIMARY KEY,                       -- uuid
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    target_kind TEXT NOT NULL CHECK (target_kind IN ('connector','ruleset')),
    plugin_id TEXT REFERENCES code_spaces(id) ON DELETE CASCADE,      -- the "ingest plugin name" (connector schedules)
    ruleset_id TEXT REFERENCES rulesets(id) ON DELETE SET NULL,        -- the "ruleset id"
    source_id TEXT REFERENCES sources(id) ON DELETE CASCADE,           -- connector: where raw items land
    pipeline_id TEXT REFERENCES pipelines(id) ON DELETE SET NULL,      -- connector: turns them into model records
    options TEXT NOT NULL DEFAULT '{}',        -- JSON for host.option()
    start_at INTEGER,                          -- epoch ms, UTC: first (or only) run; entered in `date -d` formats
    start_at_input TEXT,                       -- as typed, e.g. "tomorrow 9am" (shown in `schedule show`)
    ends_at INTEGER,                           -- epoch ms, UTC: no run starts at or after this; NULL = no end
    ends_at_input TEXT,                        -- as typed, e.g. "in 2 weeks", or "for 2 weeks" when given with --for
    tz TEXT,                                   -- optional IANA zone ("America/New_York"); NULL = UTC for cron, local for reading input
    run_every TEXT,                            -- as written: "2 hours 30 minutes", "1 month", "250 milliseconds", "15m", "1M", or 'once'
    every_ms INTEGER,                          -- the fixed part in ms (ms..weeks); NULL when none or for 'once'
    every_months INTEGER,                      -- calendar months (and years × 12); NULL when none or for 'once'
    cron TEXT,                                 -- five fields, as scheduled rules use today
    enabled INTEGER NOT NULL DEFAULT 1,
    overlap TEXT NOT NULL DEFAULT 'skip' CHECK (overlap IN ('skip','queue')),
    catch_up TEXT NOT NULL DEFAULT 'once' CHECK (catch_up IN ('once','all','none')),
    max_attempts INTEGER NOT NULL DEFAULT 3,
    next_run_at INTEGER NOT NULL,              -- epoch ms, UTC
    author_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    CHECK ((run_every IS NULL) <> (cron IS NULL)),                 -- exactly one: run_every (incl. 'once') or cron
    CHECK (run_every IS NOT 'once' OR (start_at IS NOT NULL AND cron IS NULL
           AND every_ms IS NULL AND every_months IS NULL
           AND ends_at IS NOT NULL)),                                -- 'once': start_at, an autofilled ends_at, nothing else
    CHECK (ends_at IS NULL OR start_at IS NULL OR ends_at > start_at),          -- an end after the start
    CHECK (target_kind <> 'connector' OR (plugin_id IS NOT NULL AND source_id IS NOT NULL)),
    CHECK (target_kind <> 'ruleset' OR ruleset_id IS NOT NULL)
);
CREATE INDEX schedules_due ON schedules(enabled, next_run_at);

CREATE TABLE schedule_runs (
    id INTEGER PRIMARY KEY,
    schedule_id TEXT NOT NULL REFERENCES schedules(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending','running','success','failed')),
    timestamp INTEGER NOT NULL,                -- the spec's "timestamp (in UTC)": when the run was due, epoch ms
    attempt INTEGER NOT NULL DEFAULT 1,
    claimed_by TEXT,
    heartbeat_at INTEGER,
    started_at INTEGER,
    finished_at INTEGER,
    stored INTEGER NOT NULL DEFAULT 0,         -- raw items stored (connector)
    bytes INTEGER NOT NULL DEFAULT 0,
    inserted INTEGER NOT NULL DEFAULT 0,       -- model records added by host.pipe
    piped INTEGER NOT NULL DEFAULT 0,          -- host.pipe calls that succeeded
    ruleset_ran INTEGER NOT NULL DEFAULT 0,    -- host.run_ruleset calls that succeeded
    warnings TEXT NOT NULL DEFAULT '[]',       -- JSON: failed host calls the plugin carried on past
    error TEXT,
    action_id INTEGER REFERENCES actions(id)
);
CREATE INDEX schedule_runs_schedule ON schedule_runs(schedule_id, timestamp);
CREATE INDEX schedule_runs_open ON schedule_runs(status) WHERE status IN ('pending','running');
```

### When a schedule runs: `start_at`, `run_every`, `cron` (Michael: "follow the `date` command format")

**`start_at`** (Michael: "run a schedule on a particular date and/or time … a start_at field that takes a timestamp")

- It is entered in GNU `date -d` input formats and stored as epoch ms, UTC. The text as typed is kept in `start_at_input`.
  - Absolute: `2026-11-10 09:00`, `2026-11-10T09:00:00Z`, `Nov 10 2026 9am`, `@1762765200` (Unix seconds).
  - Relative: `tomorrow 9am`, `next monday`, `next monday 14:30`, `2 hours`, `now + 90 minutes`.
- **Time zone (Michael: Q-S1):** the input is read in the schedule's `tz` if set, otherwise local time, unless the text has a zone (`Z`, `+01:00`, `UTC`, `TZ="Europe/Paris" …`), as `date` does. It is converted to UTC when entered and stored as epoch ms, UTC. Relative inputs are resolved once, when the schedule is saved (`tomorrow 9am` is a fixed time, not every day).
- **Confirmation:** the parsed time is always shown back before saving, in local time and in UTC: `start_at: Tue 2026-11-10 09:00 EST (14:00Z) — in 1 d 23 h`. The CLI prints it and asks (`-y` skips, `--json` returns it), the TUI form shows it live as you type, and the app does the same.
- **In the past (Michael: Q-T5, revised 9:17 AM):** Muninn tells the user and asks what to do. There is no `--allow-past`.
  - **The message** shows the parsed time in local and UTC and how long ago it was: `start_at Tue 2026-11-03 09:00 EST (14:00Z) is in the past (5 days 0 h ago).`
  - **Two choices** when the schedule repeats:
    - **(a) Start at the next interval:** the first `start_at + k × run_every` (k ≥ 1, calendar months clamped as usual) that is in the future, or with cron, the next cron match not before `start_at` that is in the future. That time is shown: `Next interval: Mon 2026-11-10 09:00 EST (14:00Z), in 1 day 23 h.` The stored `start_at` becomes that time, and the original text stays in `start_at_input`.
    - **(b) Correct `start_at`:** enter it again. The new value is parsed and confirmed as usual, and checked again.
  - **One-off (`run_every = 'once'`):** there is no interval, so the only choice is (b), correct it. The next-interval option isn't offered, and `--past next-interval` is an error. The message says so: "a one-off has no interval to move to; enter a future time".
  - **CLI:** an interactive prompt, `[n] next interval (Mon 2026-11-10 09:00 EST) · [c] correct start_at · [q] cancel`; `c` asks for the time again.
    - **Non-interactive** (`-y`, no TTY, scripts, MCP or the FFI without a prompt) never hangs. It needs **`--past next-interval`** to take choice (a). Otherwise it fails with: "start_at 2026-11-03 09:00 EST (14:00Z) is in the past. Choose one: start at the next interval (`--past next-interval`, would be Mon 2026-11-10 09:00 EST) or correct `--start-at`."
    - `--past next-interval` on a one-off is an error ("a one-off has no interval; correct --start-at").
    - `--past` without a past `start_at` does nothing.
  - **TUI and app forms:** an inline warning under Start at with the same text and two buttons, **Next interval (time)** and **Correct start at** (focuses the field). A one-off shows only Correct. Saving is blocked until one is picked.
  - **Edit:** the same applies when `schedule edit` sets a past `start_at`, and when a disabled schedule is re-enabled with a `start_at` that has since passed.

**`run_every`** (Michael: `date` format, plus the earlier short codes)

- It uses `date` relative-item syntax: `N unit`, items can be chained, and units take an optional plural `s`.
  - Examples: `5 minutes`, `90 seconds`, `1 month`, `2 hours 30 minutes`, `1 week`, `1 fortnight`, `1 year`, `1 day 12 hours`.
  - Units: `year`, `month`, `fortnight`, `week`, `day`, `hour`, `minute`/`min`, `second`/`sec`.
- **Milliseconds,** which `date` lacks: `millisecond`/`milliseconds`/`msec`, for example `250 milliseconds`.
- **Short codes (Michael's earlier answer), still accepted as shorthand and case-sensitive:** `ms`, `s`, `m` (minutes), `h`, `d`, capital `M` (months). They can be written joined or spaced: `15m`, `15 m`, `1M`, `500ms`. `1m` is a minute and `1M` is a month.
  - `MS`, `S`, `H`, `D` and `mo` are rejected with a hint listing both forms.
  - `w` (week), allowed by today's scheduled rules, is kept as a short code for `1 week`.
- **Not allowed in an interval:** `ago`, signs, `next`, `last`, `tomorrow`, weekdays and dates (these are points in time, not intervals) are rejected with "that is a time, use --start-at". Zero and negative totals are rejected.
- **`once` (Michael, 9:22 AM):** `run_every` also accepts the literal `once`, a run at `start_at` and nothing more.
  - `once` is valid **only** as a `run_every` value (Michael, 9:30 AM). It isn't accepted by `--for`, `--ends-at`, `--start-at` or cron.
  - A schedule given `start_at` without `--every` or `--cron` is stored with `run_every = 'once'` automatically; `--every once` is accepted explicitly too.
  - **Muninn autofills `ends_at = start_at + 1 ms`** for `once`, with `ends_at_input = "auto (once)"`.
    - 1 ms is the storage's smallest step (epoch milliseconds), so `[start_at, ends_at)` holds exactly one due time, `start_at`, and no other time can be due inside it.
    - A larger epsilon such as 1 s would add nothing and would read as a real window.
    - `ends_at` only limits when a run *starts*, so a run started at `start_at` can take as long as it needs.
    - When `start_at` is corrected, the autofilled `ends_at` moves with it.
  - **The scheduler has no `once`-specific logic.** The single run is due at `start_at`, and the next due time is past `ends_at`, so the generic `ends_at` rule disables the schedule and keeps the history.
  - `every_ms` and `every_months` are NULL for it.
  - **`once` requires `start_at`:** `--every once` without `--start-at` is an error ("once needs --start-at; to run now, use `mnnn schedule run-now NAME`"). Running something once right now is what `run-now` is for, so `once` never means "now".
  - `once` can't be mixed with other units (`once 5 minutes` is refused).
- **Stored as:**
  - the fixed part (milliseconds through weeks) → `every_ms`
  - months (and years × 12) → `every_months`
  - Mixed values such as `1 month 2 days` add months first, then the fixed part.
- **Months are calendar months:** each run is the previous due time plus N months, clamped to the end of the month, keeping the original day of month (Jan 31 → Feb 28/29 → Mar 31). Calendar steps (months, days, weeks) happen **in UTC** unless `tz` is set, even if that shifts the local clock time across DST: a 14:00Z schedule is 9am EST in winter and 10am EDT in summer (Michael: Q-T2). `tz` is never filled in automatically and stays NULL unless `--tz` is given; with `tz`, steps keep the wall-clock time in that zone.
- **Printing:** `run_every` is shown in the long form (`every 2 hours 30 minutes`), and the short form is accepted on input.
- **No drift:** `next_run_at` advances from the due time, not the finish time.
- **Floor (default accepted pending objection):** the scheduler ticks every second, so anything under 1 s runs at most once per tick.

**`ends_at` (Michael, 9:23 AM: "run something for a while, then stop it")**

- **Input:** the same `date -d` formats as `start_at`, absolute or relative: `2026-12-31 23:59`, `in 2 weeks`, `next friday 17:00`, `@1767225540`. It is read in `tz` if set, otherwise local time (unless the text has a zone), stored as epoch ms UTC, and the text as typed is kept in `ends_at_input`. The parsed time is shown back for confirmation in local time, UTC and "in 2 weeks 0 days".
- **`--for DURATION` (sugar):** a `date` interval (`--for 2 weeks`, `--for 90 minutes`, `--for 1 month`) that sets `ends_at = (start_at, or the creation time when there is none) + DURATION`. Months are calendar months, in UTC or `tz`. The result is shown like any `ends_at`, and `ends_at_input` records `for 2 weeks`. `--for` and `--ends-at` together are refused.
- **What it does:**
  - No run is started at or after `ends_at`. That covers the regular schedule, catch-up runs and retries: a retry or catch-up slot due at or after `ends_at` is dropped, with no row made.
  - A run already `running` at `ends_at` finishes normally.
  - When the next due time would be at or after `ends_at`, the schedule is disabled and keeps its `schedule_runs` history, like `once`. `schedule show` says "ended at …".
  - `next_run_at` is never set to `ends_at` or later.
- **Validation:**
  - `ends_at` must be after `start_at`, or after now when there is no `start_at`.
  - An `ends_at` in the past, or not after `start_at`, is reported in the past-`start_at` style: the parsed time in local and UTC, how long ago it was (or that it is before `start_at`), then the only choice, **correct `ends_at`** (re-enter it). There is no "next interval" for an end.
  - Non-interactive use (`-y`, no TTY, scripts, MCP) fails with that message. There is no flag to accept a past end.
  - Editing a schedule, or re-enabling one whose `ends_at` has passed, is checked the same way. Re-enabling asks for a new `ends_at`, or `--no-end` to clear it.
- **With `once`: a user-supplied `--ends-at` or `--for` is refused** ("a once schedule runs a single time at start_at; its end is set for you"), not ignored with a warning, so nobody believes a window is in force. Muninn fills `ends_at` itself (see `once` above). Plain `start_at` + `ends_at` without `--every` or `--cron` is also refused: there is no repeat to end.
- **Works with** `run_every` (fixed and months) and `cron`.

**`cron`:** the same five-field syntax as scheduled rules today, evaluated **in UTC**, or in the schedule's `tz` when set (Michael: Q-S1). `--tz ZONE` on `schedule add|edit`; `schedule show` gives both the zone's and UTC times. **Migration exception:** scheduled rules migrated from 2.23 (local-time `--cron` and `--every` today) are the one case where `tz` is set without `--tz`. They get the node's local zone, so they keep firing at the same wall-clock times instead of shifting with DST. New schedules never get `tz` automatically (Q-T2).

**How they combine** (`next_run_at` when saved):

| fields | runs |
|--------|------|
| `start_at` only = `run_every = 'once'` (set automatically, or `--every once`) | `ends_at` is autofilled to `start_at + 1 ms` (`auto (once)`). It runs once at `start_at`; the next due time is at or after `ends_at`, so the generic `ends_at` rule disables it and keeps its run history (Michael: Q-T4, 9:30 AM). If missed while nothing was running, the generic ended-while-missed rule records one `failed` "missed" row and disables it (Q-T6) |
| `start_at` + `run_every` | first at `start_at`, then every interval after it |
| `run_every` only | **immediately** on creation, then every interval; re-enabling a schedule also runs it at once (Michael: Q-T1, matching today's scheduled rules) |
| `cron` only | at the next cron time |
| `start_at` + `cron` | at the first cron time at or after `start_at` ("not before then") |
| `run_every` + `cron` | refused (one repeat rule) |
| `once` without `start_at` | refused: "once needs --start-at" (use `schedule run-now` to run now) |
| `once` + `cron` | refused |
| `run_every` or `cron` + `ends_at` (or `--for`) | as above, but no run starts at or after `ends_at`; when the next due time would be at or after it, the schedule is disabled and keeps its history |
| `once` + a user `--ends-at`/`--for`, or `start_at` + `ends_at` with no repeat | refused (`once` gets its end automatically; a plain start has nothing to end) |
| `ends_at` in the past, or not after `start_at` | the user is told (local, UTC, how long ago) and corrects `ends_at`; non-interactive fails |
| `start_at` in the past | the user is told and chooses: next interval (repeating schedules only; never for `once`) or correct `start_at`; see "In the past" above |

**Parsing (nothing installed now; added as a dependency when phase 2 is built):**

- **`start_at`:** `parse_datetime` (uutils, MIT; version 0.17.0 on crates.io), the uutils `date -d` parser, built on `jiff`. It handles absolute dates and times, ISO 8601, `@epoch`, zones, `TZ="…"`, `tomorrow`, `next monday`, weekdays and relative offsets, resolved against a base time we pass, so tests use a fixed clock. Its result is converted to epoch ms.
- **`run_every`:** Muninn's own small parser (a few dozen lines, next to `parse_every` in `rules.rs`). It implements the `date` relative-item grammar for intervals, plus `milliseconds` and the short codes, and returns `(every_months, every_ms)`. It is ours because the result is an interval, not a point in time, months must stay calendar months (not a fixed length), and `ms` is not in `date`. `parse_datetime`'s documented relative units don't include `month` or `year`.
- The repo uses `chrono` today, and `parse_datetime` brings `jiff`. The conversion stays at the parse boundary. **Approved (Michael: Q-T3):** `parse_datetime` is added to `crates/muninn-core/Cargo.toml` when phase 2 is implemented, not before.

### The scheduler service

- **Where it runs (default accepted pending objection):** inside the running node (the TUI or `mnnn net start`, which also hosts the network and the org site). Headless, it runs as `mnnn scheduler run [--for SECS]`, and `mnnn scheduler tick` runs what is due once (cron or systemd). One active scheduler per database takes a lease (`scheduler_lease`, refreshed every tick) and the others stand by.
- **Claiming:** a single `BEGIN IMMEDIATE` transaction selects due schedules, inserts `pending` runs and advances `next_run_at`. A schedule is never claimed twice.
- **Transitions:** `pending → running` (sets `started_at`, `claimed_by`, `heartbeat_at`) → `success` or `failed` (sets `finished_at`, `error`). There are no other edges. All times are epoch ms, UTC.
- **Retries (default accepted pending objection):** a failed attempt with `attempt < max_attempts` gets a new `pending` row (`attempt + 1`, same `timestamp`), with backoff of 30 s doubling up to 1 h. A retry that would start at or after `ends_at` is not made.
- **Crash recovery:** a `running` run whose heartbeat is more than 2 minutes old becomes `failed` (`error = "abandoned"`) and is retried.
- **Missed runs and overlap (default accepted pending objection):**
  - `catch_up = once` runs one run for the latest missed slot; `all` runs each missed slot, oldest first, capped at 100; `none` skips ahead.
  - Catch-up never starts a run at or after `ends_at`: missed slots are only caught up while it is still before `ends_at`.
  - **Ended while missed (generic, every schedule):** when the scheduler finds a schedule whose `next_run_at` < `ends_at` <= now (it ended while nothing was running, so slots before the end never started), it doesn't run them. It records **one** `failed` row for the latest such slot (`timestamp` = that slot, `started_at = NULL`, `finished_at` = now, `error = "missed: ended at <ends_at> before it could run"`), plus a `schedule.missed` action, and then disables the schedule, keeping its history. The status enum has no "skipped", so `failed` is used.
  - **One-off schedules (`run_every = 'once'`)** need no rule of their own (Michael: Q-T6, 9:30 AM). Their window is `[start_at, start_at + 1 ms)`. If the node was off at `start_at`, by the time anything checks, `ends_at` <= now. So the generic "ended while missed" rule applies: one `failed` "missed" row for `start_at`, a `schedule.missed` action, then disabled, with no retry, whatever `catch_up` says.
  - `overlap = skip` skips a slot while the previous run of the same schedule is open; `queue` keeps one pending run behind it.
  - At most 4 schedules run at once (`scheduler.workers`).
- **Rule-set target:** a `schedule` action naming the schedule is recorded, then the rule set runs on it (each rule's `--if` sees `schedule` and `name`). The run is `success` when the set ran; a then-action error makes it `failed`.

### Moving scheduled rules onto it (Michael: Q5)

- **Migration (once, on opening a 2.24 database):** each rule whose `spec.schedule` is set (`--every` or `--cron`) becomes:
  - a rule set `NAME (scheduled)` containing just that rule, plus a schedule targeting it with the same interval or cron
  - `next_run_at` taken from the rule's `last_run_at` and interval, so nothing runs early or twice; `start_at` stays NULL (today's scheduled rules always repeat, so none migrate to `once`)
  - the interval rewritten in the long form (`6h` → `run_every = "6 hours"`, `1w` → `"1 week"`), with the same `every_ms`; cron copied as is
  - the `schedule` field removed from the rule's spec
- The built-in **cert-expiry-watch** moves the same way: rule set `cert-expiry-watch`, a schedule every 6 hours. It is still made the first time an org gets a domain.
- The migration is recorded as a `schedule.migrate` action and covered by the database backup taken before migrations.
- **Compatibility:** `mnnn rule create|edit --every/--cron` keeps working for one release. It creates or edits the rule set and schedule for you, and prints a one-line deprecation note pointing to `mnnn schedule add`. The TUI's "on: every 6h" field does the same.
- **`rule tick` deprecation:** in 2.24 `mnnn rule tick` runs `mnnn scheduler tick` and warns "deprecated: use `mnnn scheduler tick`". It is removed in 3.0. The docs change cron and systemd examples to `scheduler tick`. The TUI and org host stop checking rules themselves and run the scheduler instead.

### Surfaces

- **CLI:**
  - `mnnn schedule add NAME (--plugin P --source S [--pipeline PL] | --ruleset R) [--start-at WHEN] [--every INTERVAL | --cron "…"] [--ends-at WHEN | --for DURATION] [--past next-interval] [--ruleset R] [--option k=v]… [--overlap skip|queue] [--catch-up once|all|none] [--max-attempts N] [-y]`
  - WHEN is any `date -d` input, and INTERVAL is `date` relative items or a short code. Examples:
    - `mnnn schedule add weekly-report --ruleset Weekly --start-at "next monday 9am" --every "1 week"`
    - `mnnn schedule add month-end --ruleset Close --start-at "2026-11-30 18:00" --every "1 month"` (then Dec 31, Jan 31, Feb 28…)
    - `mnnn schedule add once --ruleset Audit --start-at "2026-11-10T14:00:00Z"` (a single run)
    - `mnnn schedule add binance --plugin binance --source binance --pipeline to-bars-5m --ruleset Alerts --every "5 minutes"`
    - `mnnn schedule add fast --plugin ticker --source t --every 250ms` (short code)
    - `mnnn schedule add nightly --ruleset Nightly --cron "0 3 * * *" --start-at "2026-12-01"` (cron, not before Dec 1)
    - `mnnn schedule add sprint --ruleset Daily --start-at "next monday 9am" --every "1 day" --for "2 weeks"` (14 runs, then disabled)
    - `mnnn schedule add holiday --plugin ticker --source t --every 5m --ends-at "2026-12-31 23:59"`
  - `mnnn schedule parse [--start-at WHEN] [--every INTERVAL | --cron C] [--ends-at WHEN | --for D] [--json]` prints what the inputs mean without saving: local and UTC times, the next three runs, `ends_at`, and the **number of runs remaining** when it is finite (with `ends_at`; for cron counted up to 10,000, beyond that "10,000+"; without `ends_at`, "no end")
  - `mnnn schedule list|show|edit|enable|disable|remove|run-now NAME` (`run-now` runs it once immediately, without changing its schedule)
    - `mnnn schedule add audit --ruleset Audit --start-at "2026-11-10 14:00"` stores `run_every = once`; `--every once` says the same explicitly
  - `mnnn schedule runs NAME [--status S] [--json]`
  - `mnnn scheduler run|tick|status`
- **TUI:** Projects ▸ **Schedules…** (a schedule window: `n`, `e`, Space, `r` run now, Enter runs), with the scheduler's state in the status bar.
  - **The new/edit form** has the fields Name · Target (connector or rule set) · Start at · Repeat (every / cron / once).
  - Start at takes `date` input, with the parsed time shown under the field in local and UTC as you type (red with the error when it doesn't parse).
  - An **Ends at** field (optional; `date` input, or `for 2 weeks`) shows the parsed time and the runs remaining. For once it shows `auto (once)` read-only, and a past or too-early end shows an inline warning with **Correct ends at**.
  - Repeat offers **once** (the default when Start at is filled and Repeat is empty; it needs Start at, and the form says so), **every** (`date` intervals or short codes) or **cron**. Every takes `date` intervals or short codes, shown back in the long form, with the next three run times listed before you save.
- **App** (Matematik): the same.

### Tests

- The migration of project rule sets and of scheduled rules, on a 2.22 database:
  - the same next runs
  - nothing runs twice
  - the cert watcher keeps working
- `start_at` parsing with a fixed clock and zone:
  - absolute (`2026-11-10 09:00`, ISO with `Z` and offsets, `@1762765200`) and relative (`tomorrow 9am`, `next monday`)
  - local vs explicit zones, and DST edges (a nonexistent or repeated local time)
  - a past `start_at`:
    - the message (local, UTC, how long ago)
    - the next-interval time for `run_every` (fixed and months, k ≥ 1, clamping) and for cron (next match, not before)
    - correcting it by re-entering
    - a `once` schedule offering only correction; `--past next-interval` with `once` failing
    - non-interactive without `--past next-interval` failing with the message naming both choices; with it, storing the next interval
    - `--past next-interval` on a one-off failing
    - the TUI and app inline warning and buttons
  - the confirmation text
- `run_every` parsing:
  - `date` items (`5 minutes`, `90 seconds`, `2 hours 30 minutes`, `1 week`, `1 fortnight`, `1 month`, `1 year`, plurals and `min`/`sec`), and `250 milliseconds`
  - the short codes, case-sensitive (`1m` vs `1M`, rejecting `1MS`, `1mo`, `S`)
  - rejecting `ago`, `next`, dates and zero
  - long-form printing round-trips
- Month stepping: clamping (Jan 31 → Feb 28 → Feb 29 in a leap year → Mar 31), `1 month 2 days`. With `tz` NULL, steps stay in UTC across DST (the local hour shifts); with `tz` set, the local hour stays. `tz` is never set automatically, except for migrated rules (node zone, same wall-clock times as before).
- `once`:
  - `start_at` alone is stored as `run_every = 'once'` with `every_ms` and `every_months` NULL; `--every once` is the same
  - `once` without `start_at` fails pointing to `run-now`; `once` + cron and `once 5 minutes` fail
  - the CHECK constraints reject `once` rows without `start_at` or with cron or interval values
  - `ends_at` is autofilled to `start_at + 1 ms` with `ends_at_input = "auto (once)"`, and moves when `start_at` is corrected; a user `--ends-at`/`--for` with `once` is refused; `once` given anywhere but `--every` is refused
  - the CHECK rejects a `once` row without `ends_at`
  - after its run the schedule is disabled with its `schedule_runs` history kept, **through the generic `ends_at` path**: the scheduler code has no `once` branch (a test asserts the same behaviour for a repeating schedule whose `ends_at` falls just after one run)
- `ends_at`:
  - absolute and relative input (`in 2 weeks`), with `tz` and local time, and the confirmation text
  - `--for` from `start_at` and from the creation time (calendar months), and refused together with `--ends-at`
  - no run, retry or catch-up starts at or after `ends_at`; a run in progress at `ends_at` finishes
  - disabled with history kept when the next due time reaches `ends_at`, for fixed intervals, months and cron
  - refused with `once` and with no repeat
  - a past or not-after-`start_at` end is reported and corrected interactively, and fails non-interactively
  - re-enabling after the end asks for a new one or `--no-end`
  - the CHECK constraints
  - `schedule parse` runs-remaining counts (finite, none, cron capped at 10,000)
- Combinations: `start_at` alone (`once`: one run, then disabled with history kept), `start_at` + `run_every`, `run_every` alone (runs immediately, and again on re-enable), `cron` with `start_at` (not before), `run_every` + `cron` refused. A missed one-off produces one `failed` "missed" row and a `schedule.missed` action, doesn't run, and is disabled, even with `catch_up = all`, through the generic ended-while-missed rule. A repeating schedule that ended while the node was off gets the same single row and is disabled. cron stepping (existing tests move).
- The migration writes the long forms and the same `every_ms`.
- Scheduler, with a fake clock:
  - lease and claim-once under two schedulers
  - each status edge
  - retries with backoff
  - abandonment
  - catch_up
  - overlap
  - the worker cap
- `rule tick` deprecation warning. CLI, TUI, FFI and app tests.

## Ingest Connector Plugins

> **Phase 4 · v2.26.0 · ✅ Done on main (unreleased) (Michael answered Q1–Q5)**
>
> **Built:** live connector host context on schedule runs (`store_raw` → `source_data` with `run_id`, `pipe` over this run's items, `run_ruleset`, `fetch`, `state_*`); main-only enforcement; fail if required pipe/ruleset skipped (Q-I2); `schedule add --plugin --source [--pipeline] [--connector-ruleset]`; plugins remove refuses while a schedule uses the connector.
> **Phase 3 corrections in this commit:** `rule edit --every/--cron` migrates onto schedules (no new legacy `spec.schedule`); missed one-offs use daemon-alive-at-deadline vs restart-after-deadline (`scheduler.last_tick_at` + `ALIVE_GAP_MS`), not a 60s grace that would fire late; `once` disables only via autofilled `ends_at` (`auto (once)`).


**SDK shipped early (Nov 8, Michael):** SDK 1.3's typed connector methods, `plugin!(run, kind = …)`, the `ingest-connector` template and `spaces run --connector-dry-run` are on main ahead of this phase; the host methods answer only in a dry run until the scheduler lands here. Bytes cross the ABI as base64.

**Reworked for phase 1:** an ingest connector is a **code space of kind `ingest-connector`** (Q-CS7). Main can import parser or transform members to shape what it fetches before `host.pipe(...)`. Schedules reference the space. Plugins ▸ Ingest connectors lists those spaces. The host methods (`host.pipe`, `host.run_ruleset`, `host.fetch`) are available to main only.

An ingest connector plugin fetches data on a schedule, stores it raw, pipes it into a model with a pipeline and calls a rule set. Connectors are managed under **Plugins ▸ Ingest connectors** (Michael).

### Michael's spec (verbatim)

- An ingest connection is added
- It gets a schedule
- When the plugin runs a ruleset id is passed to it
- It gathers data and pipes it to and model, then calls the rule set

A system scheduler service needs to be created to support this.

- It runs any schedules added to a schedules table
- A schedules table has: an id; an ingest plugin name; a ruleset id; a supporting schedule_runs table; a run_every (n ms|s|m|h|d|m), which stands for every n milliseconds, seconds, minutes, hours, days, or months
- A schedule_runs table has: an id; a schedule_id; a status (pending|running|success|failed); a timestamp (in UTC)

### What exists today (origin/main, 2.22.x)

- **Ingest stores raw data only.** `mnnn ingest csv|json|path|service …` keeps each file, page or read byte for byte in `source_data`, under a `sources` row. It makes no bars and asks nothing about the data. `source_data.run_id` already links an item to the run that stored it.
- **Pipes and pipelines** turn raw data into model records: `source:NAME > code:parse-csv > code:to-bars?… > model:bars`. Pipelines are saved, versioned DAGs (`pipelines`, `pipeline_versions`) whose every leaf is a model sink. A `model:X` save adds only records that aren't there yet.
- **Code spaces** always end as native plugins (ABI v1: `muninn_plugin_abi`, `muninn_plugin_info`, `muninn_plugin_run(host)`). Languages are math, polars, rust, python, or a prebuilt `plugin` library. A code space must be approved and is re-approved whenever it changes. Options arrive as `code:SPACE?key=value`, read with `host.option(name)`.
- **Rules and "rule sets".** A rule is `on` / `if` / `then`, stored in `rules` (TEXT uuid id, `spec` JSON). A project's rules are its **rule set**; there is no `rulesets` table, and a "rule set id" today means a project id or a rule id. Rules can already be **scheduled** (`--every 30m|6h|1d|1w`, minimum one minute, or `--cron`). When one is due, a `schedule` action is recorded and that rule runs on it.
- **Who runs scheduled work today:** the TUI (about once a minute), a running org host (`mnnn org host start`), or `mnnn rule tick` called from cron or a systemd timer. Muninn has no separate daemon: the TUI or `mnnn net start` is the node. `mcp serve` is a stdio server per client and is not a host for this.
- **Schema conventions:** ordered `MIGRATIONS` in `store/mod.rs`, with `SCHEMA_VERSION = MIGRATIONS.len()`. Entity ids are TEXT uuid; append-only logs (`actions`) use INTEGER PRIMARY KEY. Times are INTEGER epoch milliseconds, which are UTC by definition, named `*_at`. Enums are TEXT with a CHECK. Child rows use `REFERENCES … ON DELETE CASCADE`. Everything that changes data records an action (author, origin) in the history.


### Mapping the spec onto Muninn

| Spec | Muninn |
|------|--------|
| ingest connection | a **connector**: a plugin of kind `ingest-connector` (any language, or prebuilt), managed under **Plugins ▸ Ingest connectors** |
| it gets a schedule | a `schedules` row with `target_kind = 'connector'` (phase 2), plus `source_id` and `pipeline_id` (Michael: keep both) |
| ingest plugin name | `schedules.plugin_id` (shown by name) |
| ruleset id passed to it | `schedules.ruleset_id` → `rulesets` (phase 2), given to the plugin in `host.context()` |
| it gathers data | `host.store_raw(...)`: raw items in `source_data` with `run_id`, never parsed (Michael: Q4) |
| pipes it to a model | the plugin calls `host.pipe()`, which runs the schedule's pipeline over this run's items (Michael: Q3) |
| then calls the rule set | the plugin calls `host.run_ruleset(ruleset_id)` (Michael: Q3) |

### Flow of one run

1. The scheduler claims the due schedule (`pending`), then sets it `running`, and records `connector.run.start`.
2. It loads the approved, enabled connector and calls it with the run context.
3. The plugin gathers data. For each item it calls `host.store_raw(name, format, bytes)`, which writes `source_data` (`run_id` = the run).
4. The plugin calls `host.pipe()`. The schedule's pipeline runs over `source:NAME?run=ID` (only this run's items) into its model sinks, and the call returns `{inserted, sinks}`.
5. The plugin calls `host.run_ruleset(ruleset_id)`. A `connector.run` action is recorded (params `schedule`, `connector`, `run`, `stored`, `bytes`, `inserted`) and the rule set runs on it, so rules can `--if "inserted > 0"`. The call returns what ran and any error.
6. The plugin returns. The run becomes `success` or `failed` (see below).

### Plugin interface (host methods; ABI version 1 is unchanged, SDK 1.1+)

| host method | does |
|-------------|------|
| `host.context()` | `{schedule_id, run_id, ruleset_id, source, pipeline, last_success_at, options}` |
| `host.option(name)` | (exists) the schedule's options |
| `host.store_raw(name, format, bytes)` | one raw item into the run's source; returns its id |
| `host.pipe()` | runs the schedule's pipeline (only that one, default accepted pending objection) over this run's raw items; returns `{inserted, sinks}` or an error |
| `host.run_ruleset([ruleset_id])` | records `connector.run` and runs the rule set (default: the schedule's); returns `{rules_run, errors}` or an error |
| `host.fetch(request)` | HTTP with Muninn's client (user agent, retries, rate limit). **Default accepted pending objection.** |
| `host.state_get/state_set(key, value)` | a small per-schedule cursor (for example the last timestamp) |

### How the status is recorded (Michael: the plugin calls the pipeline and the rule set)

- Muninn does **not** pipe or run the rule set after the plugin returns. The run's status is the plugin's result: `Ok` means `success`, and `Err` or a panic means `failed` with the error.
- Every host call is counted on the run: `stored` and `bytes` (store_raw), `inserted` and `piped` (pipe), `ruleset_ran` (run_ruleset).
- **The plugin skips `pipe` or `run_ruleset` (Michael: Q-I2):** if the schedule has a pipeline and `piped = 0`, or a rule set and `ruleset_ran = 0`, when the plugin returns `Ok`, the run is **`failed`**, with `error = "plugin did not call pipe"` (or `run_ruleset`, or both). The raw items it stored stay in `source_data` with the run id, so they can be re-piped. A schedule with neither a pipeline nor a rule set only needs `Ok`. Retries apply as for any failure.
- **A host call fails:** the error is returned to the plugin. If the plugin returns it, the run is `failed` with that error, and rules don't run unless the plugin already called them. If the plugin carries on, the run can end `success`, with the failed call in `warnings` (JSON list: method, error, UTC time).
- `connector.run` is always recorded, even when `run_ruleset` was never called, so history and notifications see every run. Rules only run through `run_ruleset`.

### Safety

- A connector runs only while it is approved and enabled. A changed connector fails with "approve it first" and notifies.
- Without the `ingest-connector` kind it can't be scheduled. A connector can't be used as a pipe step.

### Surfaces

- **CLI:** `mnnn plugins list --kind ingest-connector`, `mnnn plugins install … --kind ingest-connector`, and `mnnn code create NAME --lang … --kind ingest-connector` (template with `store_raw`, `pipe`, `run_ruleset`). Scheduling is `mnnn schedule add NAME --plugin P --source S --pipeline PL --ruleset R --every 15m`.
- **TUI:** **Plugins ▸ Ingest connectors…** lists connectors, each with its schedules, last run and status. `n` new connector, `s` schedule it, `r` run now, Enter shows its runs.
- **App** (Matematik): the same under Plugins.

### Tests

- A test connector stores two items, calls `pipe` (bars appear) and calls `run_ruleset` (a rule on `inserted > 0` fires); the run ends `success` with counts.
- It skips `pipe` and gets `success` with `piped = 0`, flagged. `pipe` errors and the plugin returns it: `failed`, no rules. `pipe` errors and the plugin ignores it: `success` with a warning.
- A panic means `failed`. An unapproved or disabled connector doesn't run. A connector can't be a `code:` step.
- `source_data` items carry the run id, and `source:NAME?run=ID` sees only them.

## Shares: Create Then Share

> **Phase 5 · v2.27.0 · ✅ Done on main (unreleased)**

**Checklist vs implementation:** CLI `--create` / `--create-to` on `mnnn share`, `org resource add`, `org give`, `org project add`, `project add`, `net share`, `review request`, `org code serve`; TUI **+ New …** on the share target picker, org Resources `n`, Net Shared `n`, reviews `+kind NAME`, pipe `>` picker (code/space/journal/project), project picker already had **+ New project…**. Creating a code space without `--main` makes `{name}-main` first. History records create then share.

**Checklist (closed):** net `--create-to` / **+ New exchange…** offers a key exchange then shares; review `--create` / `+KIND` for every `ReviewKind` (entry writes into `--journal` or `{name}-journal`; record creates in `--model` / `MODEL`); `project add --create result SERIES` attaches an **existing** imported series (Muninn does not invent bars — genuine data blocker); FFI `share_create_*` for app consumption (Dart/frb regen and Flutter `+ New…` UI remain Matematik’s `app/` work). Phase 4 holder liveness stays as shipped.

**Remaining outside this tree:** Flutter `app/` pickers (`+ New…` UI) — Matematik; FRB Dart bindings regen after FFI adds.


**Reworked for phase 1:** every share point that takes a code space now takes both **code** and **code spaces** (Michael, Nov 8 11:50: "all the share options need to be on codespaces"; 2.23 ships the sharing itself, this phase adds creating inline), and each gets its own **+ New code…** / **+ New code space…** (a new space asks for its main: pick existing code or write a new one). Sharing a space carries its members and pins.

### Michael's words

"Everywhere that a share can be added to something, you should be able to create that thing too.
Example: Organizations -> code spaces - a code space should be able to be created then shared from the shares section."

### Every place a share can be added today, and the create flow planned there

| Where | Adds / shares | Today | Create inline (planned) |
|-------|---------------|-------|-------------------------|
| Share action (`mnnn share`, `S` in Models, Code spaces, Journals, Live searches, Reports, Projects windows) | model, search, code, journal, report, project, pipeline → project / journal / code / net | pick an existing item and an existing target | the item picker and the target picker each start with **+ New KIND…**: it opens that kind's create dialog, then continues the share with the new one |
| Orgs ▸ Resources… (`mnnn org resource add ORG KIND NAME`) | journal, project, report, model, search, code space (and rule set, phase 2) | `a` takes an existing "kind name" | `n` **new KIND…**: create it (for a code space: name, language, template or parser, editor), then add it to the org in one step. CLI `mnnn org resource add ORG KIND NAME --create [create flags]` |
| Orgs ▸ give to a joined org (`mnnn org give`) | a copy of a resource | existing only | **+ New…** creates then gives |
| Org projects (`mnnn org project add ORG PROJECT`) | a project into an org | existing only | `--create`, and **+ New project…** |
| Project items (`project add --rule/--search/…`, Add to project…) | rules, searches, journals, reports, code, pipelines, series | existing only | **+ New KIND…** in the picker; CLI `mnnn project add P --create KIND NAME …` |
| Net ▸ Shared data… (`mnnn net share`) | items to exchanges | existing only | **+ New KIND…** |
| Reviews ▸ Ask for a review… | an item for review | existing only | **+ New KIND…** |
| Org code (`code give-to-org`, org code serve) | code and code spaces | existing only | **+ New code…** / **+ New code space…** (Michael's example) |
| Code spaces (shipped in 2.23: share, `p` project, `g` give, files, net, org resources, endpoints, serve, sync; data shared into a space) | code spaces (main + members + imported spaces) | existing only | **+ New code space…** wherever a space is picked as an item or as a target; a new space asks for its main |

The inventory is checked against the code before implementing; any share point found later gets the same flow.

### Rules

- **Create inline:** the create dialog is the same one used everywhere else for that kind (no second copy). Finishing it returns to the share with the new item selected. Cancelling returns to the picker with nothing created.
- **History:** the create action and the share action are both recorded, in that order, with the same author.
- **Permissions:** in an org, creating needs the role that adding needs (for example `resources.manage`, or `code.manage` for code spaces).
- **App** (Matematik): the same "+ New…" first entry in every share and add picker.

### Tests

- At each share point, creating then sharing makes one item and one share with both actions in history.
- Cancelling creates nothing; missing permissions refuse before creating.
- CLI `--create` paths; TUI picker tests; FFI and app tests.

## Open Questions

### Code spaces as collections (answered by Michael, Nov 8, 9:50 AM)

- **Q-CS1 Import scope:** **Resolved.** Main imports code (members) **and whole other spaces** (`host.call_space`, which runs their main). Cross-space cycles are refused by a DFS when the space is edited or approved and by a call stack at run time, with a depth limit of 8. A called space is checked like a run: enabled, approved for its membership, main and called members approved, pins matched.
- **Q-CS2 Who may call:** **Resolved.** Only main calls, inside the space and across spaces.
- **Q-CS3 Approval model:** **Resolved.** A space has its own approval, covering the **line-up** (main, members, aliases, pins, kind, imported spaces). Member and main edits run immediately without re-approval (Q-CS11) and are flagged as unreviewed.
- **Q-CS4 `code:` steps:** **Resolved.** Kept one release, deprecated; `mnnn migrate` rewrites pipelines to `space:`.
- **Q-CS5 Pinning:** **Resolved.** Members follow their code's current source by default (`pin_hash` NULL; edits run at once, Q-CS11); `--pin` is opt-in.
- **Q-CS6 CLI naming:** **Resolved.** `mnnn code` = single code; `mnnn spaces` (aliases `code-spaces`, `space`) = collections.
- **Q-CS7 Where kind lives:** **Resolved.** On the **space**. Code has no kind. An ingest connector is a space of kind `ingest-connector`.
- **Q-CS8 Migrate:** **Resolved.** A single `mnnn migrate` (code → spaces, kinds, pipeline rewrite), with `--dry-run`, a backup, idempotency, the `-migration`/skip rules and one report. `mnnn plugins migrate` and `mnnn code migrate` are removed.
- **Q-CS9 Offer on upgrade:** default accepted pending objection (offered once in the CLI, TUI and app; never automatic).
- **Q-CS10 Name too long:** default accepted pending objection (skip and report).
- **Q-C1:** **Resolved by Q-CS6.** The `code-spaces` alias moves from `mnnn code` to `mnnn spaces`.

- **Q-CS11 Unapproved member edit:** **Resolved (reversed by Michael at 9:55 AM).** An edited member runs the edited code immediately, without waiting for approval. No old versions are kept. Space approval governs the line-up, and edits are logged and flagged as unreviewed. Pinned members still fail on change until re-pinned.

### Answered by Michael (Nov 8, 2026)

1. **Months vs minutes:** **Resolved.** `m` = minutes, capital `M` = months, case-sensitive (`ms`, `s`, `m`, `h`, `d`, `M`). The `mo` proposal is dropped. Later (9:11 AM): `run_every` and the new `start_at` follow the `date` command format; the short codes stay as shorthand (phase 2).
2. **What is a "ruleset id"?** **Resolved.** A first-class `rulesets` table (designed in phase 2); `ruleset_id` references it.
3. **Who calls the rule set?** **Resolved.** The plugin calls the pipeline and the rule set through host methods (`host.pipe(…)`, `host.run_ruleset(ruleset_id)`); Muninn doesn't do it after the plugin returns. Status recording is covered in phase 3.
4. **Raw data in `source_data`?** **Resolved.** Yes: connectors store raw items in `source_data`, and a pipeline per schedule turns them into model records. Schedules keep `source_id` and `pipeline_id`.
5. **Retries:** proposed 3 attempts, 30 s backoff doubling to 1 h, a row per attempt. **Default accepted pending objection.**
6. **Smallest interval:** a 1 s floor, so sub-second schedules run at most once per tick. **Default accepted pending objection.**
7. **Network access for connectors:** `host.fetch` with Muninn's HTTP client. **Default accepted pending objection.**
8. **The existing scheduled rules:** **Resolved.** They move onto the new `schedules` table and scheduler, which supports cron too. A schedule targets a connector plugin or a rule set. Their migration and the deprecation of `rule tick` are planned in phase 2.
9. **Where the scheduler runs headless:** inside the running node, plus `mnnn scheduler run` and `mnnn scheduler tick`. **Default accepted pending objection.**
10. **Missed runs and overlap defaults:** `catch_up = once`, `overlap = skip`. **Default accepted pending objection.**
11. **Version:** the scheduler, rule sets and tables first (v2.24.0), connectors after (v2.25.0), following phase 1 (v2.23.0). **Default accepted pending objection.**

### Answered by Michael (Nov 8, 2026, second round)

- **Q-P1 Menu width:** **Resolved.** Shorten the TUI menu labels so the bar fits 80 columns: File · View · Data · Proj · Code · Plug · Net · Orgs · Keys · Theme · Help (67 columns).
- **Q-P2 Kinds:** **Resolved.** One kind per plugin. The first set is `parser`, `transform`, `analysis`, `ingest-connector`.
- **Q-P3 Approval on migrate:** **Resolved.** `migrate` keeps the approval when it only adds the kind.
- **Q-P4 Prebuilt libraries without a kind:** they run as-is once `migrate` records the kind on the row. **Default accepted pending objection.**
- **Q-P5 `install`:** local libraries and code spaces only for now. **Default accepted pending objection.**
- **Q-C1 CLI name:** **Resolved.** Keep `mnnn code` (alias `codes`) and add `code-spaces` as an alias.
- **Q-C2 Pipes and pipelines:** **Resolved.** Live pipes and Pipelines move into the new Code menu.
- **Q-S1 cron time zone:** **Resolved.** cron runs in UTC, with an optional per-schedule `tz` column. `date`-style input in local time is converted to UTC when entered, using `tz` when it is set.
- **Q-S2 Project rule sets:** each project's rules become a project-owned rule set. **Default accepted pending objection.**
- **Q-I1 `host.pipe` target:** a connector pipes only to its schedule's pipeline. **Default accepted pending objection.**
- **Q-I2 Skipped calls:** **Resolved.** If a schedule has a pipeline or rule set and the plugin never calls `pipe` or `run_ruleset`, the run is `failed`.
- **Q-SH1 Scope of create-then-share:** **Resolved.** It covers creating the target too.

### Answered by Michael (Nov 8, 2026, third round)

- **Q-T1 `run_every` without `start_at`:** **Resolved.** It runs immediately on creation and on re-enable, matching today's scheduled rules.
- **Q-T2 Filling `tz`:** **Resolved.** Steps happen in UTC even if the local time shifts. `tz` is never filled in automatically; it stays NULL unless `--tz` is given. The one exception is migrated local-time scheduled rules, which get the node's zone so they don't shift.
- **Q-T3 Dependencies:** **Resolved.** Add `parse_datetime` for `start_at`. Approved for the plan; it goes into `Cargo.toml` when phase 2 is implemented.
- **Q-T4 One-off schedules after they run:** **Resolved.** Disabled after the run, keeping its run history. A one-off is stored as `run_every = 'once'` (Michael, 9:22 AM). `once` requires `start_at`, and Muninn autofills `ends_at = start_at + 1 ms`, so the generic `ends_at` rule does the disabling (9:30 AM).
- **Q-T5 `start_at` in the past:** **Resolved, revised 9:17 AM.** Tell the user (local, UTC, how long ago) and ask: start at the next interval (shown), or correct `start_at`. A one-off can only be corrected. Non-interactive use needs `--past next-interval`, otherwise it fails naming both choices. `--allow-past` is removed.
- **Q-T6 Missed one-off:** **Resolved.** Skipped, not caught up. It is recorded as a `failed` run with a "missed" error (the status enum has no "skipped"), then the schedule is disabled. Since 9:30 AM this comes from the generic "ended while missed" rule, not one-off code.

### Added by Michael (Nov 8, 2026, 9:22 and 9:23 AM)

- **`once`:** **Resolved.** `once` is valid only as a `run_every` value. A schedule with `start_at` and no repeat is stored as `run_every = 'once'`, Muninn autofills `ends_at = start_at + 1 ms` (`auto (once)`), and the generic `ends_at` logic runs it once, then disables it and keeps its history. There is no `once` logic in the scheduler. `once` requires `start_at` (to run now, use `run-now`). A user `ends_at` with `once` is refused.
- **`ends_at`:** **Resolved.** Schedules get `ends_at` (and `ends_at_input`), in the same `date -d` formats, stored in UTC. No run, retry or catch-up starts at or after it. A run in progress finishes. When the next due time reaches it, the schedule is disabled and keeps its history. `--for DURATION` is sugar for start + duration. `ends_at` with `once` (or with no repeat) is refused as meaningless. A past end is reported and corrected, like a past `start_at`, with no "next interval" choice.

### Still open

None. Every remaining item is marked "default accepted pending objection" above (retries, the 1 s floor, `host.fetch`, the headless scheduler, catch-up and overlap defaults, versions, Q-P4, Q-P5, Q-S2, Q-I1).
