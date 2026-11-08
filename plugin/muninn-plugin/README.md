# muninn-plugin

The SDK for [Muninn](https://github.com/WeirdBrains/muninn) code space plugins: the
stable C ABI (v1), the `Host` API (load, update and create records, create models, call
other code), `Frame`s of records, result `View`s, and the `plugin!` macro that exports it
all. No dependencies.

SDK 1.3 adds the **ingest connector** methods: `context`, `store_raw`, `pipe`,
`run_ruleset`, `fetch`, `state_get` and `state_set`.

```toml
[lib]
crate-type = ["cdylib"]

[dependencies]
muninn-plugin = "1.3"
```

```rust
use muninn_plugin::prelude::*;

pub fn run(host: &Host) -> Result<View> {
    let ctx = host.context()?;
    let url = host.option("url").ok_or_else(|| Error::new("give the schedule a url option"))?;
    let r = host.fetch(&Request::get(&url))?;
    let id = host.store_raw("page-1", "csv", &r.body)?;   // raw, never parsed here
    let piped = host.pipe()?;                              // the schedule's pipeline
    let rules = host.run_ruleset(None)?;                   // the schedule's rule set
    Ok(View::table("Connector run", &["item", "inserted", "rules", "dry run"])
        .row(vec![id.as_str().into(), piped.inserted.into(), rules.rules_run.into(), ctx.dry_run.into()]))
}
muninn_plugin::plugin!(run, kind = "ingest-connector");
```

Try it with `mnnn spaces run NAME --connector-dry-run --option url=…`. See
[docs/connectors.md](https://github.com/WeirdBrains/muninn/blob/main/docs/connectors.md)
and [docs/code-spaces.md](https://github.com/WeirdBrains/muninn/blob/main/docs/code-spaces.md).

License: BSD-3-Clause.
