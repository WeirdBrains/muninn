# Ingest connectors

An ingest connector is a [code space](code-spaces.md) of kind `ingest-connector`. Its
main gathers data (usually over HTTP), stores each piece **raw**, pipes this run's raw
items through a [pipeline](pipelines.md) into a model, then runs a rule set. Muninn does
none of that for it: the run's status is what the connector returns.

The SDK side (`muninn-plugin` 1.3) is ready now. The scheduler that runs connectors on a
schedule (`start_at` / `run_every` / `ends_at`) arrives in v2.26; until then a connector
runs as a **dry run** (below). Outside a connector run the methods fail with
"available in v2.26 … or in a dry run".

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
| `host.pipe() -> Result<Piped>` | `pipe` · `null` → `{inserted, sinks}` | the schedule's pipeline over this run's items |
| `host.run_ruleset(Option<&str>) -> Result<RulesetRun>` | `run_ruleset` · `{ruleset_id?}` → `{rules_run, errors}` | records `connector.run`, runs the rule set (default: the schedule's) |
| `host.fetch(&Request) -> Result<Response>` | `fetch` · `{method, url, headers, body_base64?}` → `{status, headers, body_base64}` | HTTP with Muninn's client (user agent, 30 s timeout, 64 MB) |
| `host.state_get(key) -> Result<Option<Json>>` | `state_get` · `{key}` → `{value}` | a small per-schedule cursor |
| `host.state_set(key, value) -> Result<()>` | `state_set` · `{key, value}` | set it (`Json::Null` removes); kept when the run succeeds |

Types: `Context { schedule_id, run_id, ruleset_id, source, pipeline: Option<String>,
last_success_at: Option<i64>, options: Vec<(String, String)>, dry_run: bool }`,
`Piped { inserted: usize, sinks: Vec<Json> }`, `RulesetRun { rules_run: usize, errors:
Vec<String> }`, `Request::get(url)` / `Request::post(url, body)` `.header(k, v)`, and
`Response { status: u16, headers, body: Vec<u8> }` with `.text()`, `.json()`,
`.header(name)`. Bytes cross the JSON ABI as base64 (`connector::b64_encode/decode`).

A connector built elsewhere (a prebuilt `plugin` library) declares its kind with
`muninn_plugin::plugin!(run, kind = "ingest-connector");`, which puts
`"kind": "ingest-connector"` in its info. In a Rust code space Muninn writes that export
for you.

## Dry runs

`mnnn spaces run NAME --connector-dry-run` runs an ingest-connector space with an
in-memory context: `store_raw`, `pipe` and `run_ruleset` are **recorded, not done**
(nothing is stored, piped or run; `pipe` answers `inserted: 0`), `state_*` use an
in-memory map, and `fetch` is real. Afterwards it lists every call. Context flags:

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
// The scheduler (v2.26) runs it with a real context.
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

## Schedules (2.26)

```
mnnn schedule add NAME 15m --plugin FEED --source SRC [--pipeline PL] [--connector-ruleset R] [--option k=v]
```

When the schedule runs, the connector's main gets a live context (`host.context`,
`store_raw` → `source_data` with this run's id, `pipe` over those items only,
`run_ruleset`, `fetch`, `state_get`/`state_set`). Members cannot call those methods.
If the schedule has a pipeline or rule set and the plugin never calls `pipe` /
`run_ruleset`, the run is `failed` (`plugin did not call …`); raw items stay for a
re-pipe. Removing a plugin that a schedule uses is refused.
