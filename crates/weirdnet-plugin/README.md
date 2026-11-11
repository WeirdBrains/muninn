# weirdnet-plugin

The SDK for Weirdnet code space plugins: the stable C ABI (version 2), the safe [`Host`]
API over the host's JSON methods, frames of loaded records, series helpers, result views,
the ingest-connector methods, and the `plugin!` macro that exports the ABI.

```rust,ignore
use weirdnet_plugin::prelude::*;

pub fn run(host: &Host) -> Result<View> {
    let bars = host.input()?;
    let close = bars.f64("close")?;
    Ok(View::chart("close").x("timestamp", bars.f64("timestamp")?, true).line("close", close))
}
weirdnet_plugin::plugin!(run);
```

A plugin is a `cdylib` exporting `weirdnet_plugin_abi`, `weirdnet_plugin_info` and
`weirdnet_plugin_run` (see `src/abi.rs`). Version 2 keeps version 1's layout; only the
symbol names and the version number differ. The crate has no dependencies, so plugins
build offline. Hosts load plugins with `weirdnet-code-spaces`.
