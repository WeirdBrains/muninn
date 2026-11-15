# Schedules and the scheduler (2.25)

A **schedule** runs a target on time. In 2.25 the target is a [rule set](rules.md#rule-sets-and-schedules-225);
it can also be an ingest connector ([connectors.md](connectors.md)).

```
mnnn schedule add NAME [RUN_EVERY] --ruleset SET [--cron "M H D Mo W"]
                  [--start-at DATE] [--ends-at DATE | --for INTERVAL] [--tz ZONE]
                  [--past next-interval] [--overlap skip|queue] [--catch-up once|all|none]
                  [--max-attempts N] [--disabled]
mnnn schedule parse [RUN_EVERY] [same timing flags] [--json]   # what it means; makes nothing
mnnn schedule list | show NAME | runs NAME [--limit N] [--json]
mnnn schedule edit NAME [--every I] [--ruleset SET] [timing / policy flags] [--no-end]
                    [--plugin FEED] [--source SRC] [--pipeline PL | --no-pipeline] [--no-ruleset]   # connector schedules
                    [--option KEY=VALUE]... [--unset-option KEY]... [--clear-options]
mnnn schedule enable NAME | disable NAME | remove NAME | run-now NAME
mnnn schedule state NAME [--json] [--clear [KEY]] [--set KEY=JSON]...   # a connector's saved cursor
mnnn scheduler tick [--json]       # one pass (cron, a systemd timer)
mnnn scheduler run [--every 15]    # keep running
mnnn scheduler status [--json]
mnnn ruleset list | show | create | edit | remove | add | drop | run
```

## Timing

- **run_every**: an interval. Date items (`2 hours 30 minutes`, `1 month`, `1 year`,
  `fortnight`), short codes `ms s m h d w M y` (`90m`, `1h30m`, `2w`, `1M`; case matters:
  `S`, `H`, `D` and `mo` are refused), bare milliseconds (`5000`), or `once`. Times such
  as `tomorrow`, `9am` or `next monday` are refused with a pointer to `--start-at`. It is
  stored as given *and* in long form (`1 hour 30 minutes`).
- **--cron**: instead of run_every; five fields (minute hour day month weekday).
- **--start-at**: when the first run is due, in GNU `date -d` style (`tomorrow 9am`,
  `2026-12-01 08:00`, `next monday 06:00`, `+2 hours`, `@1767225600`, with an explicit
  zone such as `Z` or `+01:00` winning). Without it, a run_every schedule runs right
  away and a cron one at its next time. The text you gave is kept next to the time.
- **--ends-at** / **--for**: no run *starts* at or after ends_at; `--for 2w` is start + 2
  weeks. A run under way at ends_at finishes.
- **--tz**: the zone dates are read in and months and cron step in. Not given: **UTC**
  (`local` takes this node's zone; migrated rules got the node's zone).
- **once**: one run at start_at; ends_at is set to start_at + 1 ms.

Months add calendar months first and the fixed part after, keeping the day of month when
the month has it: Jan 31 → Feb 28 → Mar 31. Fixed parts are exact milliseconds (a day is
24 h even across a DST change; use cron or months for wall-clock times).

A **past start_at** asks on a terminal: the next interval (the first slot after now) or a
corrected start. Without a terminal it is an error naming both; give `--past
next-interval`. A one-off can only be corrected.

Every add, edit and parse prints what it understood:

```
every 1 hour 30 minutes
starts Mon 2099-01-05 09:00:00 UTC (2099-01-05 09:00:00Z)  (from "next monday 9am")
next   Mon 2099-01-05 09:00:00 UTC (2099-01-05 09:00:00Z)
```

## Missed one-offs and restarts
A one-off (`once`) uses autofilled `ends_at = start_at + 1 ms` (`auto (once)`). There is no
once-specific disable path: after its run, the generic `ends_at` rule turns it off.

If the node was **off** at `start_at` (restart after the deadline: no tick within the last
120 s), the generic ended-while-missed rule records one `failed` `missed` run and disables
it. If the daemon was **alive** (same `scheduler.last_tick_holder` as this session, and
`scheduler.last_tick_at` within 120s) and crossed the due time on a tick, the run still starts
even when that tiny window has already closed — so a 15–60 s tick interval can still fire
an on-time one-off. There is no grace that would run a one-off hours after a restart.

## The scheduler

The TUI (about once a minute, unless an org host runs here), a running org host
(every minute) and `mnnn scheduler run|tick` run the scheduler. Only one runs at a time:
it holds a **lease** (30 s, renewed each pass); others stand by.

To keep a scheduler running without a terminal, run `mnnn --non-interactive scheduler run`
from a service manager with `MUNINN_PASSWORD` in its environment. A macOS launch agent that
reads the password from the login Keychain is in the repository at
`examples/connectors/exchange-bars/launchd/` (see that folder's README).

Each pass:

1. **Abandoned runs**: a `running` run without a heartbeat for 2 minutes fails
   (`abandoned`) and may retry.
2. **Due slots**: for each enabled schedule whose next run is due, the slots up to now:
   - `catch_up once` (default) queues the latest; `all` every missed one (up to 100);
     `none` only one due within the last minute.
   - `overlap skip` (default) queues nothing while a run of it is open; `queue` keeps one
     waiting.
   - **Ended while missed**: when ends_at has passed and no slot was seen on time (within
     a minute), one failed `missed` run is recorded (a `schedule.missed` action), the
     schedule is disabled and its history kept. This is how a missed one-off is skipped.
   - A schedule past its last slot is disabled (`schedule.end`).
3. **Runs**: up to 4 pending runs start, one per schedule at a time. A run claims its row
   (`pending` → `running`), runs the target, and ends `success` or `failed`.
4. **Retries**: a failed run is tried again up to `--max-attempts` (default 3), after
   30 s, 1 min, 2 min, … (at most an hour), never at or after ends_at.

A rule set run records a `schedule` action (`schedule`, `name`, `run`, `due`, `attempt`,
`ruleset`) and runs the set's enabled rules on it. A rule whose `on` does not match
`schedule` or whose `if` does not hold for those params is skipped (named in the run's
warnings), not failed; see [rules](rules.md#rule-sets-and-schedules-225). It fails when the
set is removed or disabled, or a rule fails. Failures record `schedule.fail`.

A run's **warnings** (`schedule runs NAME`, one indented line each; `--json`: `warnings`)
hold what it has to say besides its status: rules skipped because their `on` or `if` did
not fit, a rule set that failed, and a connector's `host.log` lines (`log: …`, kept for
failed runs too).

`schedule enable` picks the next slot from now (an ended schedule needs a new
`--ends-at` or `--no-end` first). `run-now` runs once outside the timing.

## Editing a connector schedule

`schedule edit` changes a connector schedule in place: it keeps its id, so its runs and its
[state](#a-connector-schedules-state) stay.

```
mnnn schedule edit feed --option symbols=BTC,ETH --option closed=true   # set, or replace, options
mnnn schedule edit feed --unset-option closed                            # an error when it is not set
mnnn schedule edit feed --clear-options --option url=https://example.com/v2   # clear first, then set
mnnn schedule edit feed --plugin feed-v2 --source feed-src --pipeline candles   # swap the target
mnnn schedule edit feed --no-pipeline --no-ruleset                       # stop running one
```

`--ruleset` is the rule set the connector's `host.run_ruleset` runs (names, ids and id prefixes
work). The options are text, as `host.option` reads them. The connector flags are refused on a
schedule that runs a rule set. The new settings are printed after the edit.

## A connector schedule's state

An [ingest connector](connectors.md) keeps a small cursor between runs with
`host.state_get` / `host.state_set`. It lives in the setting `schedule.state.<schedule id>`,
so renaming or editing the schedule keeps it, and removing the schedule removes it.
**It moves only when the run succeeds**: a run that fails (the connector returns an error,
or never calls the `pipe` / `run_ruleset` it was given) leaves it where it was, so the
retry starts from the old cursor.

```
mnnn schedule state feed                      # key = JSON per line (--json: one object)
mnnn schedule state feed --set since=1700000000000 --set 'BTC/USDT={"last":5}'
mnnn schedule state feed --clear since        # one key (an error when it is not there)
mnnn schedule state feed --clear              # all of it
mnnn schedule state feed --clear --set since=0   # reset, then seed
```

`--set` values are JSON (`null` removes the key; text needs quotes: `note='"hi"'`).
`--clear` runs before `--set`; give NAME first (`--clear` takes an optional key).
Changes are refused while a run of the schedule is going, and for schedules that run a
rule set (they keep none). Each change is a `schedule.state` action.

## TUI

Rules (`R`) ▸ `Tab` cycles Rules, Rule sets and Schedules. In Rule sets and Schedules:
`Enter` shows (a schedule's timing and last runs), `Space` switches on/off, `r` runs now.

## Tables

`rulesets`, `ruleset_rules` (position), `schedules` (target, timing as given and parsed,
policy, next_run_at), `schedule_runs` (status, due timestamp, attempt, not_before,
claim and heartbeat, counts, warnings, error, action), `scheduler_lease`.
