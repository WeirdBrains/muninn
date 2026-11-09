# Schedules and the scheduler (2.25)

A **schedule** runs a target on time. In 2.25 the target is a [rule set](rules.md#rule-sets-and-schedules-225);
from 2.26 it can also be an ingest connector ([connectors.md](connectors.md)).

```
mnnn schedule add NAME [RUN_EVERY] --ruleset SET [--cron "M H D Mo W"]
                  [--start-at DATE] [--ends-at DATE | --for INTERVAL] [--tz ZONE]
                  [--past next-interval] [--overlap skip|queue] [--catch-up once|all|none]
                  [--max-attempts N] [--disabled]
mnnn schedule parse [RUN_EVERY] [same timing flags] [--json]   # what it means; makes nothing
mnnn schedule list | show NAME | runs NAME [--limit N] [--json]
mnnn schedule edit NAME [--every I] [--ruleset SET] [timing / policy flags] [--no-end]
mnnn schedule enable NAME | disable NAME | remove NAME | run-now NAME
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

## The scheduler

The TUI (about once a minute, unless an org host runs here), a running org host
(every minute) and `mnnn scheduler run|tick` run the scheduler. Only one runs at a time:
it holds a **lease** (30 s, renewed each pass); others stand by.

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
`ruleset`) and runs the set's enabled rules on it. It fails when the set is removed or
disabled, or a rule fails. Failures record `schedule.fail`.

`schedule enable` picks the next slot from now (an ended schedule needs a new
`--ends-at` or `--no-end` first). `run-now` runs once outside the timing.

## TUI

Rules (`R`) ▸ `Tab` cycles Rules, Rule sets and Schedules. In Rule sets and Schedules:
`Enter` shows (a schedule's timing and last runs), `Space` switches on/off, `r` runs now.

## Tables

`rulesets`, `ruleset_rules` (position), `schedules` (target, timing as given and parsed,
policy, next_run_at), `schedule_runs` (status, due timestamp, attempt, not_before,
claim and heartbeat, counts, warnings, error, action), `scheduler_lease`.
