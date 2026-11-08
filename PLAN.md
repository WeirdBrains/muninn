# Muninn Plan

Muninn 2.0 is complete (v2.0.0 through v2.20.0). Its full plan, announcement and
changelog are archived in [`docs/historical/2.0/`](docs/historical/2.0/), and 1.0's in
[`docs/historical/1.0/`](docs/historical/1.0/).


## Phases

| Phase | Feature set | Version | Status |
| ----- | ----------- | ------- | ------ |
| 1 | [Plugins](#plugins): a required `kind` in every plugin manifest, `mnnn plugins` with `migrate`, a Plugins menu per kind; and [Code Spaces](#code-spaces-menu-and-cli) as a main menu and CLI entry | v2.23.0 | 📝 Planned |
| 2 | [Rule Sets + Scheduler](#rule-sets--scheduler): a first-class `rulesets` table, the system scheduler with `schedules` and `schedule_runs`, scheduled rules moved onto it, `rule tick` deprecated | v2.24.0 | 📝 Planned |
| 3 | [Ingest Connector Plugins](#ingest-connector-plugins): plugins of kind `ingest-connector`, managed under Plugins ▸ Ingest connectors, run on a schedule; they store raw data, call a pipeline and a rule set | v2.25.0 | 📝 Planned |
| 4 | [Shares: Create Then Share](#shares-create-then-share): wherever a share can be added, the thing can be created there too | v2.26.0 | 📝 Planned |

**Order:** phase 1 comes first because connectors are a plugin kind and need `kind` and `migrate`. Code spaces get their main menu at the same time, because both reshape the menus. Phase 2 (rule sets and the scheduler) is the engine connectors run on; it also moves today's scheduled rules. Phase 3 then needs little more than the plugin kind and host methods. Phase 4 is independent and can move earlier if Michael wants. Each phase is a minor release. App work in every phase (the Flutter app, `app/`) is **Matematik's area**: here it is planned only as surfaces matching the TUI, through `muninn-ffi`.

## Next phase

Phase 1, [Plugins](#plugins), once Michael confirms the open questions.

---

## Plugins

> **Phase 1 · v2.23.0 · 📝 Planned**

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
- **Without a kind:** loading, running, building or approving a plugin with no kind is **rejected**: "plugin NAME has no kind; run `mnnn plugins migrate` (or `mnnn plugins migrate NAME --kind KIND`)". Nothing runs half-migrated. Until then, the TUI and app show a "needs migrating" badge with a Migrate button.
- **SDK:** the SDK goes to 1.1.0 with `plugin!(run, kind = "parser")`. ABI version 1 is unchanged: only the info JSON gains fields.

### `mnnn plugins`

```
mnnn plugins list [--kind K] [--json]            # name, kind, lang, approved, enabled, source of truth
mnnn plugins info NAME [--json]                  # the manifest, approval, where used (pipes, pipelines, schedules)
mnnn plugins install PATH|SPACE --kind K [--name N]   # a prebuilt library, or a code space as a plugin
mnnn plugins remove NAME                         # asks; -y skips; refuses while a schedule uses it unless --force
mnnn plugins enable|disable NAME                 # disabled plugins don't run (pipes, pipelines and schedules fail clearly)
mnnn plugins migrate [NAME…] [--kind K] [--dry-run] [--yes] [--no-backup]
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

> **Phase 1 · v2.23.0 · 📝 Planned**

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

> **Phase 2 · v2.24.0 · 📝 Planned (Michael answered Q1, Q2 and Q5)**

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
    tz TEXT,                                   -- optional IANA zone ("America/New_York"); NULL = UTC for cron, local for reading input
    run_every TEXT,                            -- as written: "2 hours 30 minutes", "1 month", "250 milliseconds", "15m", "1M"
    every_ms INTEGER,                          -- the fixed part in ms (ms..weeks); NULL when none
    every_months INTEGER,                      -- calendar months (and years × 12); NULL when none
    cron TEXT,                                 -- five fields, as scheduled rules use today
    enabled INTEGER NOT NULL DEFAULT 1,
    overlap TEXT NOT NULL DEFAULT 'skip' CHECK (overlap IN ('skip','queue')),
    catch_up TEXT NOT NULL DEFAULT 'once' CHECK (catch_up IN ('once','all','none')),
    max_attempts INTEGER NOT NULL DEFAULT 3,
    next_run_at INTEGER NOT NULL,              -- epoch ms, UTC
    author_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    CHECK (NOT (run_every IS NOT NULL AND cron IS NOT NULL)),      -- one repeat rule at most
    CHECK (run_every IS NOT NULL OR cron IS NOT NULL OR start_at IS NOT NULL),  -- something to run on
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
- **In the past:** a `start_at` in the past is refused unless `--allow-past` is given (then it runs at once, then follows `run_every`).

**`run_every`** (Michael: `date` format, plus the earlier short codes)

- It uses `date` relative-item syntax: `N unit`, items can be chained, and units take an optional plural `s`.
  - Examples: `5 minutes`, `90 seconds`, `1 month`, `2 hours 30 minutes`, `1 week`, `1 fortnight`, `1 year`, `1 day 12 hours`.
  - Units: `year`, `month`, `fortnight`, `week`, `day`, `hour`, `minute`/`min`, `second`/`sec`.
- **Milliseconds,** which `date` lacks: `millisecond`/`milliseconds`/`msec`, for example `250 milliseconds`.
- **Short codes (Michael's earlier answer), still accepted as shorthand and case-sensitive:** `ms`, `s`, `m` (minutes), `h`, `d`, capital `M` (months). They can be written joined or spaced: `15m`, `15 m`, `1M`, `500ms`. `1m` is a minute and `1M` is a month.
  - `MS`, `S`, `H`, `D` and `mo` are rejected with a hint listing both forms.
  - `w` (week), allowed by today's scheduled rules, is kept as a short code for `1 week`.
- **Not allowed in an interval:** `ago`, signs, `next`, `last`, `tomorrow`, weekdays and dates (these are points in time, not intervals) are rejected with "that is a time, use --start-at". Zero and negative totals are rejected.
- **Stored as:**
  - the fixed part (milliseconds through weeks) → `every_ms`
  - months (and years × 12) → `every_months`
  - Mixed values such as `1 month 2 days` add months first, then the fixed part.
- **Months are calendar months:** each run is the previous due time plus N months, clamped to the end of the month, keeping the original day of month (Jan 31 → Feb 28/29 → Mar 31). Calendar steps (months, days, weeks) follow `tz` when it is set, so 9am stays 9am through DST; without `tz` they step in UTC (Q-T2 asks whether to fill `tz` automatically).
- **Printing:** `run_every` is shown in the long form (`every 2 hours 30 minutes`), and the short form is accepted on input.
- **No drift:** `next_run_at` advances from the due time, not the finish time.
- **Floor (default accepted pending objection):** the scheduler ticks every second, so anything under 1 s runs at most once per tick.

**`cron`:** the same five-field syntax as scheduled rules today, evaluated **in UTC**, or in the schedule's `tz` when set (Michael: Q-S1). `--tz ZONE` on `schedule add|edit`; `schedule show` gives both the zone's and UTC times. Migrated scheduled rules (local-time cron today) get `tz` set to the node's local zone, so they keep firing at the same wall-clock times.

**How they combine** (`next_run_at` when saved):

| fields | runs |
|--------|------|
| `start_at` only | once, at `start_at`; then the schedule is done (disabled; `schedule show` says "ran once") |
| `start_at` + `run_every` | first at `start_at`, then every interval after it |
| `run_every` only | first one interval after creation (proposed; Q-T1 asks about running immediately instead) |
| `cron` only | at the next cron time |
| `start_at` + `cron` | at the first cron time at or after `start_at` ("not before then") |
| `run_every` + `cron` | refused (one repeat rule) |

**Parsing (nothing installed now; added as a dependency when phase 2 is built):**

- **`start_at`:** `parse_datetime` (uutils, MIT; version 0.17.0 on crates.io), the uutils `date -d` parser, built on `jiff`. It handles absolute dates and times, ISO 8601, `@epoch`, zones, `TZ="…"`, `tomorrow`, `next monday`, weekdays and relative offsets, resolved against a base time we pass, so tests use a fixed clock. Its result is converted to epoch ms.
- **`run_every`:** Muninn's own small parser (a few dozen lines, next to `parse_every` in `rules.rs`). It implements the `date` relative-item grammar for intervals, plus `milliseconds` and the short codes, and returns `(every_months, every_ms)`. It is ours because the result is an interval, not a point in time, months must stay calendar months (not a fixed length), and `ms` is not in `date`. `parse_datetime`'s documented relative units don't include `month` or `year` (Q-T3).
- The repo uses `chrono` today, and `parse_datetime` brings `jiff`. The conversion stays at the parse boundary. Q-T3 asks whether a second date library is acceptable.

### The scheduler service

- **Where it runs (default accepted pending objection):** inside the running node (the TUI or `mnnn net start`, which also hosts the network and the org site). Headless, it runs as `mnnn scheduler run [--for SECS]`, and `mnnn scheduler tick` runs what is due once (cron or systemd). One active scheduler per database takes a lease (`scheduler_lease`, refreshed every tick) and the others stand by.
- **Claiming:** a single `BEGIN IMMEDIATE` transaction selects due schedules, inserts `pending` runs and advances `next_run_at`. A schedule is never claimed twice.
- **Transitions:** `pending → running` (sets `started_at`, `claimed_by`, `heartbeat_at`) → `success` or `failed` (sets `finished_at`, `error`). There are no other edges. All times are epoch ms, UTC.
- **Retries (default accepted pending objection):** a failed attempt with `attempt < max_attempts` gets a new `pending` row (`attempt + 1`, same `timestamp`), with backoff of 30 s doubling up to 1 h.
- **Crash recovery:** a `running` run whose heartbeat is more than 2 minutes old becomes `failed` (`error = "abandoned"`) and is retried.
- **Missed runs and overlap (default accepted pending objection):**
  - `catch_up = once` runs one run for the latest missed slot; `all` runs each missed slot, oldest first, capped at 100; `none` skips ahead.
  - `overlap = skip` skips a slot while the previous run of the same schedule is open; `queue` keeps one pending run behind it.
  - At most 4 schedules run at once (`scheduler.workers`).
- **Rule-set target:** a `schedule` action naming the schedule is recorded, then the rule set runs on it (each rule's `--if` sees `schedule` and `name`). The run is `success` when the set ran; a then-action error makes it `failed`.

### Moving scheduled rules onto it (Michael: Q5)

- **Migration (once, on opening a 2.24 database):** each rule whose `spec.schedule` is set (`--every` or `--cron`) becomes:
  - a rule set `NAME (scheduled)` containing just that rule, plus a schedule targeting it with the same interval or cron
  - `next_run_at` taken from the rule's `last_run_at` and interval, so nothing runs early or twice; `start_at` stays NULL
  - the interval rewritten in the long form (`6h` → `run_every = "6 hours"`, `1w` → `"1 week"`), with the same `every_ms`; cron copied as is
  - the `schedule` field removed from the rule's spec
- The built-in **cert-expiry-watch** moves the same way: rule set `cert-expiry-watch`, a schedule every 6 hours. It is still made the first time an org gets a domain.
- The migration is recorded as a `schedule.migrate` action and covered by the database backup taken before migrations.
- **Compatibility:** `mnnn rule create|edit --every/--cron` keeps working for one release. It creates or edits the rule set and schedule for you, and prints a one-line deprecation note pointing to `mnnn schedule add`. The TUI's "on: every 6h" field does the same.
- **`rule tick` deprecation:** in 2.24 `mnnn rule tick` runs `mnnn scheduler tick` and warns "deprecated: use `mnnn scheduler tick`". It is removed in 3.0. The docs change cron and systemd examples to `scheduler tick`. The TUI and org host stop checking rules themselves and run the scheduler instead.

### Surfaces

- **CLI:**
  - `mnnn schedule add NAME (--plugin P --source S [--pipeline PL] | --ruleset R) [--start-at WHEN] [--every INTERVAL | --cron "…"] [--allow-past] [--ruleset R] [--option k=v]… [--overlap skip|queue] [--catch-up once|all|none] [--max-attempts N] [-y]`
  - WHEN is any `date -d` input, and INTERVAL is `date` relative items or a short code. Examples:
    - `mnnn schedule add weekly-report --ruleset Weekly --start-at "next monday 9am" --every "1 week"`
    - `mnnn schedule add month-end --ruleset Close --start-at "2026-11-30 18:00" --every "1 month"` (then Dec 31, Jan 31, Feb 28…)
    - `mnnn schedule add once --ruleset Audit --start-at "2026-11-10T14:00:00Z"` (a single run)
    - `mnnn schedule add binance --plugin binance --source binance --pipeline to-bars-5m --ruleset Alerts --every "5 minutes"`
    - `mnnn schedule add fast --plugin ticker --source t --every 250ms` (short code)
    - `mnnn schedule add nightly --ruleset Nightly --cron "0 3 * * *" --start-at "2026-12-01"` (cron, not before Dec 1)
  - `mnnn schedule parse --start-at WHEN | --every INTERVAL [--json]` prints what an input means (local and UTC, the next three runs) without saving
  - `mnnn schedule list|show|edit|enable|disable|remove|run-now NAME`
  - `mnnn schedule runs NAME [--status S] [--json]`
  - `mnnn scheduler run|tick|status`
- **TUI:** Projects ▸ **Schedules…** (a schedule window: `n`, `e`, Space, `r` run now, Enter runs), with the scheduler's state in the status bar.
  - **The new/edit form** has the fields Name · Target (connector or rule set) · Start at · Repeat (every / cron / once).
  - Start at takes `date` input, with the parsed time shown under the field in local and UTC as you type (red with the error when it doesn't parse).
  - Repeat takes `date` intervals or short codes, shown back in the long form, with the next three run times listed before you save.
- **App** (Matematik): the same.

### Tests

- The migration of project rule sets and of scheduled rules, on a 2.22 database:
  - the same next runs
  - nothing runs twice
  - the cert watcher keeps working
- `start_at` parsing with a fixed clock and zone:
  - absolute (`2026-11-10 09:00`, ISO with `Z` and offsets, `@1762765200`) and relative (`tomorrow 9am`, `next monday`)
  - local vs explicit zones, and DST edges (a nonexistent or repeated local time)
  - refusing the past without `--allow-past`
  - the confirmation text
- `run_every` parsing:
  - `date` items (`5 minutes`, `90 seconds`, `2 hours 30 minutes`, `1 week`, `1 fortnight`, `1 month`, `1 year`, plurals and `min`/`sec`), and `250 milliseconds`
  - the short codes, case-sensitive (`1m` vs `1M`, rejecting `1MS`, `1mo`, `S`)
  - rejecting `ago`, `next`, dates and zero
  - long-form printing round-trips
- Month stepping: clamping (Jan 31 → Feb 28 → Feb 29 in a leap year → Mar 31), `1 month 2 days`, and the local clock time across DST.
- Combinations: `start_at` alone (one run, then done), `start_at` + `run_every`, `run_every` alone (the first run one interval later), `cron` with `start_at` (not before), `run_every` + `cron` refused. cron stepping (existing tests move).
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

> **Phase 3 · v2.25.0 · 📝 Planned (Michael answered Q1–Q5)**

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

> **Phase 4 · v2.26.0 · 📝 Planned**

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
| Org code (`code give-to-org`, org code serve) | code spaces | existing only | **+ New code space…** (Michael's example) |

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

### Still open

- **Q-T1 `run_every` without `start_at`:** run first one interval after the schedule is created (proposed: no surprise run while setting up), or immediately, as today's scheduled rules do ("a new one runs at once")? Same question when a schedule is re-enabled.
- **Q-T2 Which clock for calendar steps:** months, days and weeks keep the wall-clock time in the zone the schedule was entered in (proposed: a `9am` schedule stays 9am through DST; needs a `tz` column, the IANA zone or the local one at entry), or step in UTC (simpler, but 9am becomes 8am or 10am after a DST change)?
- **Q-T3 Dependencies:** OK to add `parse_datetime` (MIT, which pulls in `jiff`) next to our `chrono` for `start_at`, with `run_every` parsed by our own code? The alternative is to write the `date -d` subset ourselves on `chrono`, which is more work and a weaker match to `date`.
- **Q-T4 One-off schedules after they run:** a `start_at`-only schedule is disabled and kept with its run history (proposed), deleted, or kept enabled but inert?
- **Q-T5 `start_at` in the past:** refuse unless `--allow-past` (proposed), or accept and run at once?
- **Q-T6 `start_at` with `catch_up`:** when the node was off at `start_at`, does a one-off run when it next starts (proposed: yes, following `catch_up = once`), or is it skipped?
