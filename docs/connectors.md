# Ingest connectors

An ingest connector is a [code space](code-spaces.md) of kind `ingest-connector`. Its
main gathers data (usually over HTTP), stores each piece **raw**, pipes this run's raw
items through a [pipeline](pipelines.md) into a model, then runs a rule set. Muninn does
none of that for it: the run's status is what the connector returns.

The scheduler runs connectors on a schedule (`start_at` / `run_every` / `ends_at`; see
[Schedules](#schedules), since 2.23.0), and `mnnn spaces run NAME --connector-dry-run`
tries one without storing anything ([Dry runs](#dry-runs)). Outside a connector run the
methods fail with "… runs from a schedule or in a dry run".

## Start one

```sh
mnnn code create feed --template ingest-connector      # Rust, SDK 1.3
mnnn spaces create feed --kind ingest-connector --main feed
mnnn -y spaces approve feed
mnnn spaces run feed --connector-dry-run --option url=https://example.com/data.csv
```

## The host methods (main only)

| SDK 1.3 (`use muninn_plugin::prelude::*`) | ABI method · request → reply | does |
|---|---|---|
| `host.context() -> Result<Context>` | `context` · `null` → `{schedule_id, run_id, ruleset_id, source, pipeline, last_success_at, options, dry_run}` | what this run is for |
| `host.option(name) -> Option<String>` | `options` | a schedule option (a step's own options come first) |
| `host.store_raw(name, format, &[u8]) -> Result<String>` | `store_raw` · `{name, format, bytes_base64}` → `{id}` | one raw item into the run's source, never parsed |
| `host.pipe() -> Result<Piped>` | `pipe` · `null` → `{inserted, landed, duplicates, sinks}` | the schedule's pipeline over this run's items; `inserted` counts **new** records only (a re-run over data already stored says 0), `landed` every row the sinks were given; a run that stored nothing pipes nothing (a parser would refuse an empty frame) and answers `inserted: 0, landed: 0, empty: true`, still counting as piped |
| `host.run_ruleset(Option<&str>) -> Result<RulesetRun>` | `run_ruleset` · `{ruleset_id?}` → `{rules_run, errors, skipped}` | records `connector.run`, runs the rule set (default: the schedule's); a rule whose `on` or `if` does not fit the action is skipped, not run |
| `host.fetch(&Request) -> Result<Response>` | `fetch` · `{method, url, headers, body_base64?}` → `{status, headers, body_base64}` | HTTP with Muninn's client (user agent, 30 s timeout, 64 MB) |
| `host.log(text)` | `log` · `{text}` | a line for the run: in a scheduled run it is kept as a `log: …` warning of the run (`mnnn schedule runs NAME`, `--json`), failed runs too |
| `host.state_get(key) -> Result<Option<Json>>` | `state_get` · `{key}` → `{value}` | a small per-schedule cursor |
| `host.state_set(key, value) -> Result<()>` | `state_set` · `{key, value}` | set it (`Json::Null` removes); kept only when the run succeeds (a failed run, or one that never called a configured `pipe` / `run_ruleset`, leaves the old state). Read or reset it with [`mnnn schedule state`](schedules.md#a-connector-schedules-state) |

Types: `Context { schedule_id, run_id, ruleset_id, source, pipeline: Option<String>,
last_success_at: Option<i64>, options: Vec<(String, String)>, dry_run: bool }`,
`Piped { inserted: usize, landed: usize, sinks: Vec<Json> }`, `RulesetRun { rules_run: usize, errors:
Vec<String>, skipped: Vec<String> }`, `Request::get(url)` / `Request::post(url, body)` `.header(k, v)`, and
`Response { status: u16, headers, body: Vec<u8> }` with `.text()`, `.json()`,
`.header(name)`. Bytes cross the JSON ABI as base64 (`connector::b64_encode/decode`).

A connector built elsewhere (a prebuilt `plugin` library) declares its kind with
`muninn_plugin::plugin!(run, kind = "ingest-connector");`, which puts
`"kind": "ingest-connector"` in its info. In a Rust code space Muninn writes that export
for you.

## Dry runs

`mnnn spaces run NAME --connector-dry-run` runs an ingest-connector space with an
in-memory context: `store_raw`, `pipe` and `run_ruleset` are **recorded, not done**
(nothing is stored, piped or run; `pipe` answers `inserted: 0, landed: 0`), `state_*` use an
in-memory map, and `fetch` is real. The space is approved first (asking, as for any run),
then its main runs, its view is printed, and afterwards every call is listed. A connector
takes no `--input`. Context flags:

| flag | sets |
|------|------|
| `--source NAME` | `source` (default: the space's name) |
| `--pipeline NAME` | `pipeline` |
| `--ruleset ID` | `ruleset_id` |
| `--option KEY=VALUE` | `options` (repeatable) |
| `--state KEY=JSON` | starting state (repeatable) |

With `--json` a second line, `{"connector_dry_run": {context, calls, state, stored, bytes}}`,
follows the run's JSON.

## A full example

This is the template (`mnnn code create NAME --template ingest-connector`):

```rust
// An ingest connector (SDK 1.3). Make it a code space's main:
//   mnnn spaces create NAME --kind ingest-connector --main THIS
// Try it without storing anything:
//   mnnn spaces run NAME --connector-dry-run --option url=https://example.com/data.csv
// The scheduler runs it with a real context.
use muninn_plugin::prelude::*;

pub fn run(host: &Host) -> Result<View> {
    let ctx = host.context()?;
    let url = host.option("url").ok_or_else(|| Error::new("give the schedule a url option"))?;

    // 1. gather: fetch, and keep a cursor so the next run asks only for what's new
    let since = host.state_get("since")?.and_then(|v| v.as_i64()).or(ctx.last_success_at).unwrap_or(0);
    let r = host.fetch(&Request::get(&format!("{url}?since={since}")))?;
    if r.status != 200 {
        return Err(Error::new(format!("{url}: HTTP {}", r.status)));
    }

    // 2. store it raw (never parsed here; the pipeline parses)
    let id = host.store_raw("page-1", "csv", &r.body)?;

    // 3. pipe this run's raw items through the schedule's pipeline into its model
    let piped = host.pipe()?;

    // 4. run the rule set (rules see a `connector.run` action: `--if "inserted > 0"`)
    let rules = host.run_ruleset(None)?;

    if let Some(next) = r.header("x-next-since").and_then(|v| v.parse::<i64>().ok()) {
        host.state_set("since", next)?;
    }
    Ok(View::table("Connector run", &["item", "bytes", "inserted", "rules run", "dry run"]).row(vec![
        id.as_str().into(),
        r.body.len().into(),
        piped.inserted.into(),
        rules.rules_run.into(),
        ctx.dry_run.into(),
    ]))
}
// Muninn exports `run` for you. A connector built elsewhere (a prebuilt `plugin` library)
// ends with: muninn_plugin::plugin!(run, kind = "ingest-connector");
```

## Schedules

```
mnnn schedule add NAME 15m --plugin FEED --source SRC [--pipeline PL] [--connector-ruleset R] [--option k=v]
mnnn schedule edit NAME --option k=v --unset-option k2     # same schedule: id, runs and state stay
mnnn schedule state NAME [--set KEY=JSON] [--clear [KEY]]  # read or reset its cursor
```

[Schedules](schedules.md#editing-a-connector-schedule) has the rest of `edit` (target flags)
and `state`. `--plugin`, `--source`, `--pipeline`, `--connector-ruleset` (and `--ruleset` on
`edit`) take an id, a unique id prefix or a name; what is stored is the id.

When the schedule runs, the connector's main gets a live context (`host.context`,
`store_raw` → `source_data` with this run's id, `pipe` over those items only,
`run_ruleset`, `fetch`, `state_get`/`state_set`, `log`). Members cannot call those methods.
Removing a connector that a schedule uses is refused.

**The connector must call what the schedule has.** A schedule with a pipeline fails unless
the main calls `host.pipe()`; one with a rule set fails unless it calls
`host.run_ruleset()`. The run is `failed` with `plugin did not call pipe` / `run_ruleset` (or
`pipe or run_ruleset`), its raw items stay for a re-pipe, its state is not saved, and a
`connector.run` action with an `error` is recorded (it does not run the rule set). In a quiet
run (nothing stored) call them anyway: `pipe` then does nothing and a gap alarm can fire. A
schedule with neither needs neither call.

**Ids and names.** In a live run `host.context().ruleset_id` and `pipeline` are **ids**;
`source` is a name. The argument of `host.run_ruleset(Some(id))` is a rule set **id** (a name
fails with "its rule set was removed"). The `schedule add` and `edit` flags above accept
names and ids.

### The pipeline: start it with `-`

`host.pipe()` hands the pipeline **a frame of only this run's raw items**, as the standard
input of the pipeline, so its first node is `-`. A `source:NAME` first node does not: it
re-reads every item ever stored in that source, on every run, and only the model's
de-duplication keeps the old rows from landing again (and `inserted` stays honest only
because of it).

```sh
mnnn code parsers --install
mnnn pipeline create candles --node src=- --node p=space:parse-ndjson \
    --node 'b=space:to-bars?duration=5m&rejects=fail' --node m=model:bars --edge 'src>p>b>m'
mnnn schedule add candles 5m --plugin candles --source candles-src --pipeline candles ...
```

The frame has one row per `host.store_raw` of the run, oldest first, with these columns
(the same ones `source:NAME` gives):

| column | type | holds |
|--------|------|-------|
| `text` | text | the item's bytes as UTF-8 text (invalid bytes become U+FFFD) |
| `name` | text | the `name` given to `store_raw` |
| `format` | text | the `format` given to `store_raw` (`csv`, `ndjson`, …) |
| `locator` | text | `connector:RUN:NAME` |
| `received_at` | time | when it was stored: epoch **milliseconds**, UTC |
| `item` | text | the item's id (also the row id) |
| `source` | text | the source's name |

A parser (`parse-ndjson`, `parse-csv`, `parse-json`) turns `text` into rows. Give `to-bars`
`rejects=fail`, so a bad row fails the run (and keeps the cursor back) instead of landing in
its `Rejected` table. The pipeline may have `space:` steps and the rule set `run-space`
rules: they run inside the connector's `pipe` / `run_ruleset` call, as a main calling.
Members still can't call these methods.

### Where a run says what happened

A run is a row of `mnnn schedule runs NAME` (`--json` for all of it): `stored`, `bytes`,
`inserted` (new records), `piped`, `ruleset_ran`, the `error` of a failed one and its
**warnings**: skipped rules, a failed rule set or pipeline, and the connector's `host.log`
lines as `log: …` (a failed run keeps them too). Failures are `schedule.fail` actions and
every `run_ruleset` a `connector.run` action in the [history](history.md).

## Rules react to `connector.run`

`host.run_ruleset` records a `connector.run` action and runs the schedule's rule set on it.
It is a rule event like any other (`mnnn rule events connector.run`), with the fields
`schedule`, `name` (the schedule's), `run`, `connector`, `ruleset`, `stored`, `bytes`,
`inserted` (new records; `Piped.inserted` summed over the run's `pipe` calls) and `landed`
(every row the pipeline gave its sinks), so a rule can name it with `--on` and test those
fields with `--if`. A rule of the set runs only when its `--on` matches (`schedule`, what
`rule create --ruleset` stores when you give no `--on`, is the set's own trigger) and its
`--if` holds; otherwise it is skipped and listed in the run's warnings
([rules](rules.md#rule-sets-and-schedules-225)).

A **gap alarm**: tell me when a run brought nothing new.

```sh
mnnn ruleset create feed-alarms
mnnn rule create "Gap alarm" --ruleset feed-alarms --on connector.run --if "inserted = 0" \
    --then 'notify "{name}: nothing new" body="run {run}: stored {stored}, landed {landed}" level=warn'
mnnn schedule add feed 15m --plugin feed --source feed --pipeline feed-pipe --connector-ruleset feed-alarms
```

`inserted = 0` is also true for a re-run over data that is already stored (`landed` is not 0
then) and for a run that stored nothing; add `--if "stored > 0"` or `--if "landed = 0"`
to tell those apart. The `notify` is a [notification](notifications.md).

## Example: an exchange's 5-minute candles

A connector that keeps a cursor per symbol, keeps only **closed** candles, stores them raw
and lets a `-` pipeline make bars of them. It reads a Binance-style `/api/v3/klines` endpoint
(arrays of `[openTime, open, high, low, close, volume, …]`; set the `base` option to point it
elsewhere, e.g. at a local server to try it):

```rust
// A candle connector (SDK 1.3): an exchange's public 5-minute candles into the bars model.
//   mnnn code create candles --lang rust -f candles.rs
//   mnnn spaces create candles --kind ingest-connector --main candles
use muninn_plugin::prelude::*;
use std::time::{SystemTime, UNIX_EPOCH};

const BAR_MS: i64 = 300_000; // 5 minutes

pub fn run(host: &Host) -> Result<View> {
    let base = host.option("base").unwrap_or_else(|| "https://api.binance.com".into());
    let symbols = host.option("symbols").ok_or_else(|| Error::new("give the schedule a symbols option (BTCUSDT,ETHUSDT)"))?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);

    // 1. gather: one request per symbol, from just after that symbol's cursor
    let mut cursors = vec![];
    let mut view = View::table("Candles", &["symbol", "closed bars kept"]);
    for sym in symbols.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let from = match host.state_get(sym)?.and_then(|v| v.as_i64()) {
            Some(t) => format!("&startTime={}", t + BAR_MS),
            None => String::new(),
        };
        let r = host.fetch(&Request::get(&format!("{base}/api/v3/klines?symbol={sym}&interval=5m&limit=1000{from}")))?;
        if r.status != 200 {
            return Err(Error::new(format!("{sym}: HTTP {}", r.status)));
        }
        // 2. keep only closed bars, as one NDJSON row each (the last candle may still be forming)
        let (mut rows, mut kept, mut last) = (String::new(), 0usize, 0i64);
        for k in r.json()?.as_array().unwrap_or(&[]) {
            let c = k.as_array().unwrap_or(&[]);
            let open_time = c.first().and_then(Json::as_i64).unwrap_or(0);
            if c.len() < 6 || open_time + BAR_MS > now {
                continue;
            }
            let num = |i: usize| c[i].as_f64().unwrap_or(0.0);
            let row = Json::obj()
                .with("symbol", sym)
                .with("ts", open_time)
                .with("o", num(1))
                .with("h", num(2))
                .with("l", num(3))
                .with("c", num(4))
                .with("v", num(5));
            rows.push_str(&format!("{row}\n"));
            (kept, last) = (kept + 1, last.max(open_time));
        }
        if kept > 0 {
            host.store_raw(&format!("{sym}-{last}"), "ndjson", rows.as_bytes())?; // raw, parsed by the pipeline
            cursors.push((sym.to_string(), last));
        }
        view = view.row(vec![sym.into(), kept.into()]);
    }

    // 3. this run's raw items through the schedule's pipeline, then 4. its rule set
    host.pipe()?;
    host.run_ruleset(None)?;

    // 5. move the cursors; they are kept only because the run succeeded
    for (sym, last) in cursors {
        host.state_set(&sym, last)?;
    }
    Ok(view)
}
```

Set it up:

```sh
mnnn code create candles --lang rust -f candles.rs
mnnn spaces create candles --kind ingest-connector --main candles
mnnn -y spaces approve candles
mnnn sources add candles-src
mnnn code parsers --install
mnnn pipeline create candles --node src=- --node p=space:parse-ndjson \
    --node 'b=space:to-bars?duration=5m&rejects=fail' --node m=model:bars --edge 'src>p>b>m'
mnnn ruleset create candle-alarms
mnnn rule create "Gap alarm" --ruleset candle-alarms --on connector.run --if "landed = 0" \
    --then 'notify "{name}: no new candles" level=warn'
mnnn schedule add candles 5m --plugin candles --source candles-src --pipeline candles \
    --connector-ruleset candle-alarms --option symbols=BTCUSDT,ETHUSDT
mnnn schedule run-now candles     # the first run takes the latest 1000 candles
mnnn schedule state candles       # BTCUSDT = 1700000400000, ETHUSDT = ...
```

- **A cursor per symbol**: `state_set(symbol, open time of the last closed bar kept)`; the
  next run asks the exchange from that time plus 5 minutes. State is saved only when the
  whole run succeeded, so a failed run asks the same range again. `mnnn schedule state
  candles --set BTCUSDT=1699999800000` moves one back to refetch it; `--clear BTCUSDT` starts
  that symbol over.
- **Closed bars only**: a candle whose open time + 5 minutes is still ahead is the one
  forming; it is dropped and comes back, complete, in a later run.
- **No duplicates**: each row has `symbol`, `ts` and `o h l c v`, which `to-bars` reads by
  name. Bars have a fixed id, so a re-run (or a cursor moved back) adds nothing: `inserted`
  is 0 while `landed` counts the rows given.
- **A quiet run** (nothing new closed) stores nothing; `pipe()` and `run_ruleset()` are still
  called, so the gap alarm can say so. The alarm tests `landed = 0`, not `inserted = 0`: the
  retry after a failed run re-lands bars already stored (`inserted` 0, `landed` > 0), which is
  not a gap.
- Change what it fetches without losing the cursors: `mnnn schedule edit candles --option
  symbols=BTCUSDT,ETHUSDT,SOLUSDT` (a new symbol starts without a cursor).

A complete, tested version for Coinbase and Kraken (which answer from the US, where Binance
does not), with raw-JSON parsers, an idempotent installer, offline parser tests and a macOS
launch agent for the scheduler, is in the repository at `examples/connectors/exchange-bars/`.
