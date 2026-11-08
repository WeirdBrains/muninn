# Muninn Plan

Muninn 2.0 is complete (v2.0.0 through v2.20.0). Its full plan, announcement and
changelog are archived in [`docs/historical/2.0/`](docs/historical/2.0/), and 1.0's in
[`docs/historical/1.0/`](docs/historical/1.0/).

## Phases

| Phase | Feature set | Version | Status |
| ----- | ----------- | ------- | ------ |
| 1 | [Ingest Connector Plugins + Scheduler](#ingest-connector-plugins--scheduler): scheduled ingest plugins that store raw data, pipe it to a model and call a rule set; a system scheduler with `schedules` and `schedule_runs` | v2.23.0 | 📝 Planned (open questions below) |

## Next phase

To be planned with Michael.

---

## Ingest Connector Plugins + Scheduler

> **Phase 1 · v2.23.0 · 📝 Planned: open questions first**

An ingest connector plugin fetches data on a schedule, pipes it into a model, then runs a rule set. A system scheduler service runs every schedule.

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
| ingest connection | a **connector**: an approved code space of a new kind `connector` (any language, or a prebuilt plugin), plus its options and a target `source` |
| ingest plugin name | the connector code space's name (stored by id, shown by name) |
| it gathers data | the plugin calls new host methods to fetch and to **store raw items** into its source's `source_data`, tagged with the run id |
| pipes it to a model | the raw items of this run flow through a saved **pipeline** (or a pipe spec) whose leaves are model sinks, for example `source:NAME > code:parse-json > code:to-bars > model:bars` |
| ruleset id, calls the rule set | after the model step succeeds, the scheduler runs that rule set (see Q2), with a `connector.run` action as the trigger, so rules can `--if "inserted > 0"` |
| system scheduler service | a scheduler inside the running node (TUI or `mnnn net start`), with a headless `mnnn scheduler run` and `mnnn scheduler tick` for servers, cron and systemd |

### Flow of one run

1. **Due:** for each enabled schedule, the scheduler finds `next_run_at <= now`.
2. **Claim:** in one transaction it inserts a `schedule_runs` row (`pending`) and advances `next_run_at`.
3. **Start:** `pending → running`, setting `started_at`. It records a `connector.run.start` action.
4. **Gather:** it loads the approved connector plugin and calls it with the run context (`schedule_id`, `run_id`, `ruleset_id`, options, source, `last_success_at`). The plugin fetches and calls `host.store_raw(name, format, bytes)` per item. Items land in `source_data` with `run_id` set, unchanged.
5. **Pipe:** the run's items go through the schedule's pipeline into its model sinks. Only this run's items are used: `source:NAME?run=ID`.
6. **Rule set:** it records `connector.run` (params `schedule`, `connector`, `run`, `stored`, `bytes`, `inserted`, `status`), then runs the rule set on it.
7. **Finish:** `running → success` (or `failed`, with `error`), setting `finished_at`.

The ruleset id is passed to the plugin as context: the spec says so, and it lets a plugin read the rule set. Muninn still calls the rule set itself after the pipe, so a plugin can't skip or reorder that step (see Q3).

### Tables

```sql
CREATE TABLE schedules (
    id TEXT PRIMARY KEY,                       -- uuid
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,  -- for the CLI/TUI
    plugin_id TEXT NOT NULL REFERENCES code_spaces(id) ON DELETE CASCADE,  -- "ingest plugin name" (by id, shown by name)
    ruleset_id TEXT,                           -- "ruleset id": the project whose rules run (Q2); NULL = none
    source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,      -- where raw data lands
    pipeline_id TEXT REFERENCES pipelines(id) ON DELETE SET NULL,          -- how it reaches a model (Q4)
    options TEXT NOT NULL DEFAULT '{}',        -- JSON, read by the plugin with host.option()
    run_every TEXT NOT NULL,                   -- as written, e.g. "15m", "1mo"
    every_ms INTEGER,                          -- parsed fixed interval; NULL for calendar months
    every_months INTEGER,                      -- parsed months; NULL otherwise
    enabled INTEGER NOT NULL DEFAULT 1,
    overlap TEXT NOT NULL DEFAULT 'skip' CHECK (overlap IN ('skip','queue')),
    catch_up TEXT NOT NULL DEFAULT 'once' CHECK (catch_up IN ('once','all','none')),
    max_attempts INTEGER NOT NULL DEFAULT 3,
    next_run_at INTEGER NOT NULL,              -- epoch ms, UTC
    author_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX schedules_due ON schedules(enabled, next_run_at);

CREATE TABLE schedule_runs (
    id INTEGER PRIMARY KEY,                    -- an append-only log, like actions
    schedule_id TEXT NOT NULL REFERENCES schedules(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending','running','success','failed')),
    timestamp INTEGER NOT NULL,                -- the spec's "timestamp (in UTC)": when the run was due, epoch ms
    attempt INTEGER NOT NULL DEFAULT 1,
    claimed_by TEXT,                           -- node/process id holding the run
    heartbeat_at INTEGER,                      -- epoch ms, refreshed while running
    started_at INTEGER,
    finished_at INTEGER,
    stored INTEGER NOT NULL DEFAULT 0,         -- raw items stored
    bytes INTEGER NOT NULL DEFAULT 0,
    inserted INTEGER NOT NULL DEFAULT 0,       -- model records added
    error TEXT,
    action_id INTEGER REFERENCES actions(id)   -- the connector.run action
);
CREATE INDEX schedule_runs_schedule ON schedule_runs(schedule_id, timestamp);
CREATE INDEX schedule_runs_open ON schedule_runs(status) WHERE status IN ('pending','running');
```

`source_data.run_id` (already present) holds `schedule_runs.id` for connector items. This is one new migration, appended to `MIGRATIONS`. Timestamps are stored as epoch ms (UTC) and shown in UTC with `Z` in the CLI and JSON, and in local time in the TUI if Michael wants (Q8).

### run_every

`N` followed by a unit: `ms`, `s`, `m` (minutes), `h`, `d`, and months (`mo` proposed, Q1). N is a positive integer.

- Fixed units become `every_ms`. Months become `every_months` and step by calendar month in UTC, clamped to the month's last day (Jan 31 → Feb 28/29 → Mar 31 keeps the original day-of-month).
- A floor guards against busy loops: `ms` and `s` are allowed, but anything under the scheduler's tick (proposed 1 s) runs at most once per tick (Q6).
- `next_run_at` advances from the **due time**, not the finish time, so a schedule doesn't drift.

### The scheduler service

- **Where it runs:** a scheduler task in the running node, the TUI or `mnnn net start`, which hosts the network and the org host today. A new headless command, `mnnn scheduler run [--for SECS]`, gives servers the same loop without a TUI. `mnnn scheduler tick` runs what is due once, for cron or systemd. Only one scheduler is active per database. Like org hosting, the first one to take a lease wins and the others stand by (`scheduler_lease` in the store's settings, refreshed every tick).
- **Tick:** every second it wakes for the earliest `next_run_at` (or on a nudge when a schedule changes).
- **Claiming:** a single `BEGIN IMMEDIATE` transaction selects due schedules, inserts `pending` runs and advances `next_run_at`. A schedule is never claimed twice.
- **Transitions:** `pending → running` (sets `started_at`, `claimed_by`, `heartbeat_at`) → `success` or `failed` (sets `finished_at`, `error`). There are no other edges. All times are epoch ms (UTC).
- **Retries:** a failed attempt with `attempt < max_attempts` inserts a new `pending` run (`attempt + 1`, same `timestamp`) after backoff of 30 s, doubling up to 1 h. Rules see only the final outcome (Q5).
- **Crash recovery:** a `running` run whose heartbeat is older than 2 minutes is marked `failed` (`error = "abandoned"`) and retried like any failure. `pending` runs left by a dead scheduler are picked up as-is.
- **Missed runs** (nothing was running): `catch_up = once` (default, matching today's scheduled rules) runs one run for the latest missed time. `all` runs each missed slot, oldest first, capped at 100. `none` skips to the next future slot.
- **Overlap:** if the previous run of the same schedule is still `pending` or `running`, `skip` (default) records nothing and moves on, and `queue` leaves one pending run behind it. Different schedules run concurrently, with a cap of 4 workers (`scheduler.workers`).
- **Safety:** a connector runs only while its code space is approved. A changed, unapproved connector fails the run with "approve it first" and notifies. Every run is in the action history.

### Plugin interface (ABI v1, new host methods; the ABI version doesn't change)

| host method | does |
|-------------|------|
| `host.context()` | `{schedule_id, run_id, ruleset_id, source, last_success_at, options}` |
| `host.option(name)` | (exists) the schedule's options |
| `host.store_raw(name, format, bytes)` | stores one raw item in the run's source: `source_data` with `run_id`; never parsed |
| `host.fetch(request)` | HTTP with the service ingest's defaults (user agent, retries, rate limit), so connectors don't need their own TLS stack (Q7) |
| `host.state_get/state_set(key, value)` | a small per-schedule cursor (for example the last timestamp) |

A connector's `run` returns OK or an error. It doesn't write models itself: the pipe step does. This keeps the raw-data rule (ingest assumes nothing) and lets the same raw data be re-piped later. The ruleset id is passed for information. Muninn calls the rule set after the pipe (Q3).

### Surfaces

- **CLI:**
  - `mnnn connector list|show|create NAME --lang … [--file F]` (a code space of kind connector)
  - `mnnn schedule add NAME --plugin P --every 15m [--ruleset PROJECT] [--source S] [--pipeline PL] [--option k=v]… [--overlap skip|queue] [--catch-up once|all|none]`
  - `mnnn schedule list|show|edit|enable|disable|remove|run-now NAME`
  - `mnnn schedule runs NAME [--status S] [--json]`
  - `mnnn scheduler run|tick|status`
- **TUI:** a Projects ▸ Schedules… window, with `n` new, `e` edit, Space on/off, `r` run now, and Enter for the runs, each run's status, times, stored, inserted and error. The scheduler's state shows in the status bar.
- **Flutter app:** the same window through `muninn-ffi` (Matematik's area). The keys and other secrets for a connector stay in Rust.
- **Help, docs, man pages:** `docs/connectors.md`, `docs/scheduler.md`, with updates to `docs/rules.md` and `docs/code-spaces.md`.

### Tests

- Store: the migration on a 2.22 database, CHECKs, cascades, and the due index.
- run_every: parsing, printing and stepping, including month-end clamping, the minute/month suffix, and rejects such as `0m`, `-1h`, `5x`.
- Scheduler, with a fake clock:
  - claims once under two concurrent schedulers (lease)
  - each status edge
  - retries with backoff
  - heartbeat abandonment
  - catch_up once/all/none
  - overlap skip/queue
  - the worker cap
- End to end: a test connector stores two raw items, then the pipeline makes bars, then the rule set runs on `connector.run` with `inserted > 0`, then the run ends `success`. A failing connector ends `failed` and runs no rules. An unapproved connector doesn't run.
- CLI and TUI tests for each command and the window; FFI and app tests against the fake API.

### Open questions for Michael

1. **Months vs minutes.** `run_every` lists `m` for both minutes and months. Proposal: `m` = minutes, `mo` = months (and accept `min` as an alias for minutes). Is that right?
2. **What is a "ruleset id"?** Muninn has no rule-set table: a project's rules are its rule set, and single rules can be scheduled. Should `ruleset_id` be a project id (run the project's rules), a single rule id, or should we add a first-class `rulesets` table?
3. **Who calls the rule set?** The spec says the plugin "pipes it to a model, then calls the rule set". Proposal: Muninn does the pipe and the rule-set call after the plugin returns, so a plugin can't skip them; the plugin still receives the ruleset id. Or should the plugin call them through host methods?
4. **Should the ingest plugin's raw data still land in `source_data`?** His raw-data rule says ingest never assumes anything about the data. Proposal: yes. Connectors only store raw items, and a pipeline (chosen per schedule) turns them into model records. This means each schedule also needs a source and a pipeline, which the spec's table doesn't list. Is that OK, or should a connector write straight to a model?
5. **Retries:** should a failed run retry automatically (proposed: 3 attempts, 30 s backoff doubling to 1 h), and does each retry get its own `schedule_runs` row (proposed) or reuse one?
6. **Smallest interval:** `ms` is in the spec. Is a 1 s floor OK, so a sub-second schedule runs at most once per tick?
7. **Network access for connectors:** should connectors get `host.fetch` (Muninn's HTTP client), or open their own connections? Plugins today can't reach the network through the host.
8. **The existing scheduled rules** (`--every`/`--cron`, run by the TUI, org host and `rule tick`): should they move onto the new `schedules` table and scheduler, or stay separate? Should `schedules` also accept `cron`?
9. **Where the scheduler runs headless:** is a new `mnnn scheduler run` the right home, or should it only run inside `mnnn net start` / the org host, as scheduled rules do today?
10. **Missed runs and overlap defaults:** are `catch_up = once` and `overlap = skip` right?
11. **Version:** is this v2.23.0 (a minor release, new feature), and does it ship whole, or do the scheduler and tables come first and connectors after?
