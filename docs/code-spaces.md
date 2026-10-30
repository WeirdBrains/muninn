# Code spaces

A code space is a program kept in your Muninn database. It reads records (bars or any
[model](models.md)), can change and create records and models, and returns a **result**:
a table, a chart, text, or tiles of these. Code spaces are written in one of four
languages and always end up as a **native plugin**, a shared library that Muninn loads
and runs.

| language | `--lang` | you write | needs |
|----------|----------|-----------|-------|
| Math | `math` | one formula per line: `fast = sma(close, 10)` | cargo |
| Polars | `polars` | one Polars expression per line: `ret = col("close").pct_change(lit(1))` | cargo (first build compiles polars, a few minutes) |
| Rust | `rust` | a `run(host)` function against the [SDK](#the-sdk) | cargo |
| Python | `python` | typed functions, one column each | cargo and [Rython](#python-through-rython) |
| prebuilt | `plugin` | the path of a library built elsewhere | nothing |

```
mnnn code create fast --lang math --model bars --where "symbol = BTC/USDT" --sort timestamp \
    --source $'@title SMA cross\nfast = sma(close, 10)\nslow = sma(close, 30)\ncross = if(fast > slow, 1, 0)'
mnnn code run fast -y
```

## Trust: what a code space can do

A code space runs **as native code inside Muninn, with your rights**: it can do anything
your user can (read files, open connections), not only what the host API offers. So:

- a new code space, and every change to its source (or to the library file of a prebuilt
  plugin), has to be **approved** before it runs. `mnnn code run` asks (`-y` says yes;
  `--non-interactive` without `-y` refuses), `mnnn code approve` approves without running,
  and the TUI asks with `y`/`n`. The approval is bound to a hash of the exact source (and
  language), shown by `mnnn code show`.
- code spaces from a [project archive](projects.md) arrive **not approved**, without
  builds or results: read them before you run them.
- rules run only approved code spaces (`run-code` fails otherwise).
- the host API has no way to read your keys, passwords or secrets, and run logs never
  contain them. A code space still runs as you, so only approve code you trust.

Nothing a code space does is hidden: the run is a `code.run` action in the
[action history](history.md) (origin `cli`, `tui` or `rule`, the current
[author](authors.md)), and every record it changes or creates is recorded under it, with
the previous values of changed fields.

## Input

A code space may name its **input**, the records it gets with `host.input()` (math,
Polars and Python always work on it):

```
--model MODEL [--where "FIELD OP VALUE"]… [--matching TEXT] [--group F] [--sort F|-F] [--limit N]
```

In the TUI (and in `mnnn code show`) the input is one line:
`bars where symbol = BTC/USDT; close > 3 group symbol sort -timestamp limit 500`
(`none` for no input). Conditions are those of [model records](models.md); `-F` sorts
descending. With `--group`, math, Polars and Python compute per group (for example per
symbol), so a moving average never runs across two symbols.

## Math

One line per formula, `NAME = EXPRESSION`. A name is an input field (`close`, `volume`,
a designed field) or an earlier line. Every value is a column: one number per record,
missing values are `NaN`. Operators: `+ - * / %`, comparisons, `and`, `or`, `not`.

| functions | |
|-----------|-|
| rolling, window `N` | `sma(x, N)` `ema(x, N)` `rolling_sum` `rolling_min` `rolling_max` `rolling_std` `lag(x, N)` `lead(x, N)` |
| along the column | `diff(x)` `pct_change(x)` `cumsum(x)` `zscore(x)` |
| one value per group | `sum` `mean` (`avg`) `min` `max` `std` `count` `first` `last` |
| per value | `abs` `sqrt` `ln` (`log`) `log10` `exp` `round` `floor` `ceil` `sign` `missing(x)` `min(a, b)` `max(a, b)` `pow(a, b)` `coalesce(a, b)` `if(c, a, b)` |

Directives shape the result and write values back:

| directive | |
|-----------|-|
| `@title TEXT` | the result's title |
| `@view chart\|table\|value` | a chart (the default; one stacked tile per group), every row as a table, or one row per group with its last value |
| `@x FIELD` | the chart's x axis (default `timestamp` when there is one) |
| `@show a, b` | the columns shown (default: every computed one) |
| `@update FIELD = NAME` | write a computed column into a field of the input records |

`#` starts a comment.

## Polars

One expression of the Polars Rust DSL per line (a `pl.` prefix is allowed):
`NAME = EXPR` adds a column, `where EXPR` keeps the rows where it holds. The input is a
`DataFrame`: numbers as `Float64`, integers and timestamps as `Int64`, text as `String`,
plus `id`. Directives are those of math.

```
@title Returns
where col("volume").gt(lit(0))
ret = col("close").pct_change(lit(1)) * lit(100)
```

The first Polars build compiles the polars crate (minutes); later builds reuse it.

## Rust

A Rust code space is the body of a plugin crate: a `pub fn run(host: &Host) -> Result<View>`.
Muninn adds the crate, the [SDK](#the-sdk) and the export macro around it.

```rust
use muninn_plugin::prelude::*;

pub fn run(host: &Host) -> Result<View> {
    let bars = host.input()?;
    let close = bars.f64("close")?;
    host.create_model("signals", "symbol:text; score:real", "made by a plugin")?;
    for g in bars.groups() { /* per symbol */ }
    host.update_where("bars", &["symbol = BTC/USDT"], Json::obj().with("note", "seen"))?;
    Ok(View::chart("Close").x("timestamp", bars.f64("timestamp")?, true).line("close", close))
}
```

## Python through Rython

Python code spaces are translated to Rust by [Rython](https://crates.io/crates/rythonc)
(`rythonc`), then built like the others; Python itself is never needed. Every top-level
`def` without a leading `_` is a column: it runs once per record, its parameters are the
record's fields by name (`float`, `int`, `bool` or `str`) and it returns `float`, `int`
or `bool`. `_helpers` are plain functions. Directives are math's, as `# @…` comments.

```python
# @title Range
# @update spread = spread
def spread(high: float, low: float) -> float:
    return high - low
```

Rython runs as a separate program: Muninn looks for `MUNINN_RYTHONC` first, then
`rythonc` on `PATH`, and never embeds Python or links libpython. Muninn's packages do
not bundle it: install it with `cargo install rythonc` (building code spaces needs cargo
anyway), or download a Rython release and point `MUNINN_RYTHONC` at its `rythonc`
binary. `mnnn code check` shows whether it is found.

## Prebuilt plugins

`--lang plugin` takes the path of a library (`.so`, `.dylib`, `.dll`) built elsewhere,
for example with the SDK from `mnnn code sdk --out DIR`. Nothing is compiled, so it needs
no Rust toolchain. Muninn copies the library into its cache before loading it, and the
approval covers the file's contents: a rebuilt library has to be approved again.

## Without a Rust toolchain

Muninn itself installs and runs without Rust. Building math, Polars, Rust and Python code
spaces needs `cargo` (from [rustup](https://rustup.rs)); without it these fail with a
message saying so, while prebuilt plugins still run. `mnnn code check` lists what is
there:

```
cargo    cargo 1.92.0
rythonc  missing: …
ABI      1 (SDK 1.0.0)
```

`MUNINN_CARGO` points at a cargo binary that is not on `PATH`.

## Builds and the cache

Builds live under `MUNINN_HOME/code` ([storage layout](storage-layout.md)):

| path | |
|------|-|
| `code/sdk/muninn-plugin-HASH/` | the SDK crate the plugins build against |
| `code/build/KEY/` | the generated crate of one source |
| `code/target/` | cargo's shared target directory (`MUNINN_CODE_TARGET` moves it) |
| `code/plugins/KEY.so` | built libraries, by content hash |

`KEY` is a hash of the language, the source, the SDK and the code generator, so an
unchanged source is never built twice, and changing it back finds the old build. A build
is a `code.build` action; a cached one is not. Deleting `code/` is safe: it is rebuilt
when needed. `mnnn code build SPACE` builds without running.

## Results

A run ends with an **error** (the plugin's error, a panic, which is caught, or a build
error) or a **result view**:

| view | |
|------|-|
| table | columns and rows |
| chart | lines over an x axis (time or numbers) |
| text | Markdown |
| tiles | views side by side or stacked |

The last run is kept with the code space: its status, time, duration, writes, logs and
view. Views follow the content view rules: tiles split side by side or stacked like
[tiling](tiling.md), tables and charts look as they do elsewhere. [Reports](reports.md)
show them (`report add --code SPACE`: the last result, then the source; charts become
images in HTML, PDF, LaTeX and Word, plus a table of each line; tables stop at 200 rows),
the TUI shows them in the result tile, and `mnnn code run` prints them as text
(`--rows N`, `--json`).

## Projects, reports, rules and archives

- `mnnn project add P --code SPACE` (or `p` in the TUI) puts a code space in a project;
  the project's documents show it like a report section.
- `mnnn report add R --code SPACE` adds a section with its last result and its source.
- the then-action `run-code SPACE` runs it from a [rule](rules.md) (approved only); its
  result line is the rule's message. Rules can react to `code.run`, `code.build` and the
  other `code.*` events (`mnnn rule events`).
- [project archives](export-formats.md) (`.mnnp`) carry the code spaces of the project,
  of its report sections and of its rules: the source and input only, never builds,
  approvals or results. Imported spaces get `(imported)` added to a name that is taken.

## Changing data

Muninn is a general research store, so code spaces are not fenced in: they can change
any field of any model, **including bars and their built-in fields** (`open` … `flags`,
and the key fields `source`, `symbol`, `period`, `timestamp`), create records and bars,
and create models. Bars fields go by the names the bars model gives them (see
[models](models.md)). Nothing is deleted or silently replaced: changing a key field re-keys
the bar, and is refused when another bar already has that key. Previous values are kept
in the history entry of the change (up to 10,000 records per call).

## The host API

Through the SDK's `Host` (and in ABI terms, a method name and a JSON request):

| method | SDK | does |
|--------|-----|------|
| `input` | `host.input()` | the code space's input as a `Frame` |
| `input_request` | `host.input_request()` | the input's definition |
| `models` | `host.models()` | the models and their fields |
| `load` | `host.load(&Load::model("bars").filter("close > 3").sort("-timestamp").group("symbol").limit(100))` | any records, filtered, grouped and sorted |
| `update` | `host.update(model, id, values)` | change one record by id |
| `update_where` | `host.update_where(model, &[conditions], values)` | change every record that matches |
| `update_each` | `host.update_each(model, vec![(id, values)])` | many single-record changes in one call |
| `create` / `create_many` | `host.create(model, values)` | new records (bars too) |
| `model_create` | `host.create_model(name, "a:text; b:real", about)` | a model through the [designer](models.md) (false when it exists) |
| `log` | `host.log(text)` | a line in the run's log |
| `result` / `error` | returned by `run` | the view, or the error |
| `methods` | `host.methods()` | the methods this host knows |

A `Frame` holds records as columns: `len()`, `f64(name)`, `text(name)`, `column(name)`,
`get(row, name)`, `ids`, `groups()`, `slice`, `take`. `series::` has the rolling and
column functions math uses. `View::table(title, &[cols]).row(…)`, `View::chart(title)
.x(…).line(…)`, `View::text(title, markdown)` and `View::tiles(title, below, views)`
build results.

## The ABI

A plugin is a shared library exporting three C symbols; this is **ABI version 1**:

| symbol | signature | |
|--------|-----------|-|
| `muninn_plugin_abi` | `extern "C" fn() -> u32` | the ABI version it was built for |
| `muninn_plugin_info` | `extern "C" fn() -> MnStr` | static JSON: `{"sdk": "1.0.0", "lang": "rust"}` |
| `muninn_plugin_run` | `unsafe extern "C" fn(*const MnHost) -> i32` | run once: 0 ok, 1 error, 2 panic |

Everything else goes through the `MnHost` table: `call(ctx, method, request, out)` takes
a method name and a JSON request and fills `out` with the JSON reply (or an error
message), which the plugin hands back with `free`. Memory never crosses allocators.
Within an ABI version the types and symbols never change; new methods may be added (a
plugin can ask with `methods`; an unknown method is an error, never a crash). Muninn
refuses a library whose ABI version it does not support. The SDK's `plugin!(run)` macro
writes the three symbols and catches panics.

## The SDK

The `muninn-plugin` crate (version 1.0.0) is embedded in Muninn: it is written next to
every build, and `mnnn code sdk --out DIR` writes a copy to build prebuilt plugins with
(`crate-type = ["cdylib"]`, depend on it by path, end with `muninn_plugin::plugin!(run);`).

## Command line

```
mnnn code list [--json]
mnnn code create NAME --lang math|polars|rust|python|plugin [--file F | --source S | --editor]
                 [--model M --where C… --matching T --group F --sort F --limit N]
                 [--description D] [--project P]          # no source: a starting template
mnnn code edit SPACE [--name N] [--description D] [--lang L] [--file F | --source S | --editor]
                 [--model M …] [--no-input]
mnnn code show SPACE [--json | --source]       # input, approval, trust hash, last run, source
mnnn code approve SPACE                        # asks; -y skips
mnnn code build SPACE                          # compile (cached)
mnnn code run SPACE [--json] [--rows N]        # asks to approve a new or changed source
mnnn code check [SPACE]                        # the toolchains, or check a source without running it
mnnn code remove SPACE                         # asks; -y skips (the source stays in the history)
mnnn code sdk [--out DIR]                      # the plugin SDK
```

`SPACE` is an id, a unique id prefix or a name; `mnnn codes` is an alias. `--file -`
reads standard input; `--editor` opens `$VISUAL` or `$EDITOR`.

## Terminal UI

`C` or **Projects ▸ Code spaces…** opens the list: each space with `●` approved / `○`
not, its language, input and last run.

| key | list |
|-----|------|
| `j` / `k`, ↑ / ↓ | move |
| Enter / `e` | open the editor |
| `n` | new code space: a name, then a language (starts from a template) |
| `r` | run |
| `m` | rename |
| `p` | add to a project |
| `d` | delete (`y` confirms) |
| Esc | close |

The editor window has the source on the left, the **result tile** on the right and the
**build output** below it. Keys go to the source while you edit (Esc stops editing);
otherwise:

| key | editor |
|-----|--------|
| `e` / Enter | edit the source (arrows, Home/End, PgUp/PgDn, Tab = 4 spaces) |
| `s` (Ctrl+S while editing) | save |
| `b` | build (in the background; the window stays usable) |
| `r` (Ctrl+R while editing) | save, build if needed, and run; a new or changed source asks `y`/`n` first |
| `c` | check the source without running it |
| `i` | edit the input line |
| `t` | widen the result tile / back |
| `p` | add to a project |
| Esc | back to the list (asks when there are unsaved changes: `y` discards, `s` saves) |

Opening a code space item in a project opens its editor. Runs happen in the foreground
(the plugin works on the open database); builds happen in the background.
