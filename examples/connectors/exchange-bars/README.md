# exchange-bars: closed 5-minute bars from Coinbase and Kraken into Muninn

Every 5 minutes (20 s after each bar closes) a Muninn schedule asks a public exchange API for the new candles of a
few symbols, keeps the response bodies untouched in a source, and a pipeline turns them into `model:bars` rows
(`period` 5m). Only CLOSED bars ever reach `model:bars`: a bar id is deduplicated, so a partial bar would stick.
See [docs/connectors.md](../../../docs/connectors.md) for how ingest connectors, schedules and `connector.run`
rules work; this folder is a complete, tested example of them.

| thing | name | what it is |
|---|---|---|
| connector space | `exchange-bars` | Rust, SDK 1.3, kind ingest-connector (`src/exchange-bars.rs`) |
| parser spaces | `parse-coinbase-candles`, `parse-kraken-ohlc` | raw JSON to closed bars (`src/parse-*.rs`) |
| built-in space | `to-bars` | from `mnnn code parsers --install` (rows to Bars) |
| sources | `coinbase`, `kraken` | where the raw responses are kept |
| pipelines | `coinbase-5m`, `kraken-5m` | `-` (this run's raw items) > parser > `to-bars?duration=5m&source=X` > `model:bars` |
| rule set | `bars-gap` (one rule, also named `bars-gap`) | the gap alarm, see "Gap alarm" |
| schedules | `coinbase-5m`, `kraken-5m` | every 5m, first run at the next 5-minute boundary + 20 s (UTC) |

## Requirements

* `mnnn` **2.26.0 or later** (conditional rules on `connector.run`, `host.pipe()` that counts only new records and
  tolerates a run that stored nothing, `schedule edit --option`, `schedule state`). `install.sh` refuses to run when
  `schedule state` or `schedule edit --unset-option` is missing.
* The process that runs the schedules must be 2.26.0 or later too: the TUI, `mnnn scheduler run`, or the launch agent
  below. Do not run two schedulers at once (only one holds the lease; the other stands by).
* A Rust toolchain (`cargo`): code spaces are compiled on first run.
* Network access to `api.exchange.coinbase.com` and `api.kraken.com` only. No keys, no account. (Binance and Bybit
  refuse requests from the US, HTTP 451 / 403, which is why this example does not use them.)

## Install

```sh
export MNNN=/path/to/mnnn                           # default: mnnn on PATH
export MUNINN_HOME=/path/to/home                    # optional; default is mnnn's own home
./install.sh                                        # SYMBOLS="BTC/USD,ETH/USD" by default; asks for the password
```

The script asks for your Muninn password once (not echoed), checks that it unlocks the database before changing
anything, and passes it to its `mnnn` calls (all `--non-interactive`). It never prints or stores it. If
`MUNINN_PASSWORD` is already exported it uses that and does not ask.

The script is idempotent: it checks for each object before creating it (a re-run creates and changes nothing),
updates a code space whose source file changed, and approves the code it installs (`mnnn -y code approve`): read
`src/*.rs` first, it runs as native code with your rights. An existing schedule is brought in line **in place** with
`schedule edit` (same id, same cursors) when its pipeline, rule set, plugin, source, exchange or (if you set
`SYMBOLS`) symbols differ.

**Upgrading an install made by the first version** (it had a second, empty rule set `bars-ok` and a schedule option
`ok_ruleset`): just run the new `install.sh`. It changes the old `bars-gap` rule to the conditional one, drops the
`ok_ruleset` option with `schedule edit --unset-option`, removes `bars-ok`, and updates the connector source.
Cursors survive.

Self-check (one request to api.kraken.com, nothing stored; `SKIP_SELFCHECK=1` skips it): it runs
`spaces run exchange-bars --connector-dry-run --option exchange=kraken --option symbols=BTC/USD`. Look for a table row
`BTC/USD ... ok` and the calls `fetch`, `store_raw`, `state_set` listed as "not stored". That proves the connector
builds and the exchange answers. A dry run only records `pipe`; the real proof:

```sh
$MNNN schedule run-now kraken-5m && $MNNN schedule runs kraken-5m     # "success" and the connector's `log:` lines
```

Then start whatever runs schedules (the TUI, `$MNNN scheduler run`, or the launch agent below). Nothing runs by
itself: `install.sh` only creates the schedules.

## Run the scheduler at login (macOS)

`launchd/` holds a launch agent that runs `mnnn --non-interactive scheduler run` at login and restarts it if it
exits. The wrapper reads the Muninn password from the login Keychain, so it is never in a file.

```sh
security add-generic-password -a "$USER" -s muninn-scheduler -U -w     # type the password (asked twice, not shown)
install -m 755 launchd/muninn-scheduler ~/.local/bin/muninn-scheduler   # MNNN in it defaults to ~/.local/bin/mnnn
sed "s#__HOME__#$HOME#g" launchd/com.weirdbrains.muninn.scheduler.plist \
  > ~/Library/LaunchAgents/com.weirdbrains.muninn.scheduler.plist
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.weirdbrains.muninn.scheduler.plist
tail -f ~/Library/Logs/muninn-scheduler.log                             # "starting mnnn ...", then a line per pass that ran
```

* Stop any other scheduler (a TUI, a terminal `scheduler run`); the agent takes the lease within about 30 s.
* The first time the agent reads the Keychain item, macOS may ask; choose Always Allow.
* While the Mac sleeps nothing runs; on wake the missed slot runs once and the cursors fetch the bars it skipped.
* The password is in the scheduler process's environment while it runs.
* Without the Keychain item the wrapper exits 78 and launchd retries every 60 s (`ThrottleInterval`).
* Turn it off: `launchctl bootout gui/$(id -u)/com.weirdbrains.muninn.scheduler`; forget the password:
  `security delete-generic-password -a "$USER" -s muninn-scheduler`.

## Change symbols or the lookback

Symbols are canonical `BASE/QUOTE`. Re-run the installer with the new list; it edits the schedules in place:

```sh
SYMBOLS="BTC/USD,ETH/USD,SOL/USD" ./install.sh        # = schedule edit NAME --option symbols=...
$MNNN schedule edit coinbase-5m --option lookback_hours=48     # by hand: first-run lookback of a NEW symbol (default 24)
```

The schedule keeps its id, its runs and its cursors: symbols already there carry on from their cursor, a new symbol
starts with `lookback_hours` of history. A symbol you remove keeps its cursor entry in the state (harmless;
`schedule state NAME --clear KEY` removes it). Running `install.sh` without `SYMBOLS` leaves the symbols of an
existing schedule alone.
Symbol mapping: Coinbase `BTC-USD`; Kraken `XBTUSD` (BTC is `XBT`, DOGE is `XDG`, otherwise the same letters; Kraken
also accepts `BTCUSD` and `BTC/USD`, and the key of its reply is its internal name such as `XXBTZUSD`, which the code
never relies on). An unknown symbol fails that symbol only (see "Failures").

## Is it working?

```sh
$MNNN schedule runs coinbase-5m                  # newest first: success / failed, plus the connector's `log:` lines
$MNNN schedule state coinbase-5m                 # the cursors: cursor:BTC/USD = open time (ms) of the newest closed bar kept
$MNNN schedule show coinbase-5m --json          # per run: stored, bytes, inserted, piped, ruleset_ran, error, warnings
$MNNN model record list bars --where source=coinbase --where symbol=BTC/USD --sort -timestamp --limit 3
$MNNN sources data coinbase | tail                # the raw responses; `sources cat ITEM` prints one unchanged
$MNNN history list --all | head -30               # connector.run, rule.run, record.create ("N new bars into bars (M already there)")
```

`schedule runs` shows what the connector `host.log`ged as `log:` lines: one line per symbol (rows fetched, new closed
bars, cursor) and a summary (`N raw items stored, N new closed bars, N new rows piped, N symbols failed`). A failed
symbol is a `log: warning: ...` line too.

Steady state: each run stores one small response per symbol that has a new closed bar, and `record.create` says
"N new bars". `inserted` in `schedule show` counts NEW records only (a run that only re-sees known bars says 0). A
cursor can be moved by hand: `schedule state NAME --set 'cursor:BTC/USD=1794774900000'` (JSON value) or
`--clear cursor:BTC/USD` (that symbol starts over from `lookback_hours`); both are refused while a run is going.

## Gap alarm

One rule set, `bars-gap`, with one rule, `bars-gap`, an ordinary conditional rule:

```sh
mnnn ruleset create bars-gap
mnnn rule create bars-gap --ruleset bars-gap --on connector.run --if "landed = 0" \
  --then 'notify "{name}: nothing new" body="run {run}: stored {stored}, landed {landed}" level=warn'
```

The schedules run that rule set (`--connector-ruleset bars-gap`); the connector calls `run_ruleset` on every run and
Muninn runs the rule only when its `on` and `if` hold, otherwise it skips it. A quiet run (no symbol had a new closed
bar) stores nothing and pipes nothing, so `landed` is 0 and the alarm fires: a "nothing new" notification, one per such
run (see them in the TUI or `mnnn history list --all | grep notification`).

* A normal run does not alarm; the first run after install and a run that adds a new bar do not either.
* A manual `run-now` right after a run alarms (nothing new yet), once per run, never twice.
* `landed` counts all symbols of the run together: a thin symbol that has no trade in an interval (Coinbase omits
  empty intervals) does not alarm unless every symbol of that exchange is quiet.
* The condition is `landed = 0`, not `inserted = 0`: after a failed run the cursors are not saved, so the retry
  re-lands bars that are already stored (`inserted` 0, `landed` > 0). That is not a gap, and `landed` does not alarm.

## Backfill

* Coinbase: first run takes `lookback_hours`; set it big (`--option lookback_hours=720` at schedule creation, or
  `schedule edit NAME --option lookback_hours=720` before the symbol's first run) and the connector pages 299 bars per
  request, at most 12 requests per symbol per run (about 12 days), the cursor advancing, so a long backfill finishes
  over successive runs. Paging is implemented but has only been exercised with the default 24 h. Coinbase can go back
  years.
* Kraken: only the latest 720 bars (60 h) exist; older history needs Kraken's trade download, not this API.
* Gaps (scheduler down for hours): the cursor makes the next run fetch everything since it, within the limits above.

## Failures

Per symbol, any of: HTTP not 200 (404 unknown product, 429 rate limit), Kraken `error` array non-empty (also HTTP 200),
bad JSON. The connector skips that symbol, still stores and pipes the others, and at the end returns an error naming
the failed symbols, so the run shows as `failed` (the scheduler retries: 3 attempts, 30 s, 1 min, 2 min backoff;
`run-now` makes one attempt). Example (Kraken `FOO/USD`): the good symbol's new bar lands, the run is `failed: ...
FOO/USD (Kraken error: EQuery:Unknown asset pair)`, and **no cursor moves**: Muninn saves state only when the run
succeeds. The next successful run re-fetches the good symbols' range from the old cursors (bars deduplicate) and then
saves them. Re-fetching stores those raw responses again, which is harmless. 429 handling has not been seen live.

## Known limits

* Latency is about 20 s after the bar closes plus the scheduler pass (every 15 s).
* Coinbase omits intervals with no trades: those bars simply do not exist (no zero-volume filler).
* A bar is "closed" if open + 300 s is not after the moment the connector read the clock, just before the request
  (floored to the second). The parser applies the same cutoff again, from the fetch time in the item name and
  `received_at` (whichever is earlier).
* Item names are `<exchange> <BASE/QUOTE> <fetch epoch seconds>[ pN]`; the parsers also read `-`/`_` in place of `/` and
  a trailing `.json`, so they can be tested from files (`mnnn ingest json`, see `tests/`).
* Kraken's `since` returns the cursor bar again (and the forming bar); the connector filters both, so a quiet Kraken run
  fetches 2 rows and finds 0 new closed ones.
* Requests: per run one request per symbol (Coinbase: more only when backfilling). Muninn's default user agent is used.
* A quiet run stores nothing: raw items accumulate only when there is a new closed bar (about 288 per day per symbol
  per exchange when the market trades every interval).

## Tests

Everything runs in a throwaway home under `tests/test-home/` (git-ignored, with a random throwaway password); the
scripts refuse `~/.muninn`.

```sh
MNNN=/path/to/mnnn tests/test_parsers.sh      # offline: installs the kit in the test home, stores fixtures/, checks both parsers
MNNN=/path/to/mnnn tests/check_bars.sh        # bars per source/symbol, period, oldest/newest, closed, duplicates, alignment
```

`test_parsers.sh` checks, per exchange: the columns, 5-minute alignment, no forming bar, high/low bounds, both
symbols, every bar equal to its raw row, and the bar count. `fixtures/` holds real Coinbase and Kraken responses for
BTC/USD and ETH/USD fetched at 1794770106 (2026-11-15 19:15 UTC). For a live end-to-end run in the test home:
`. tests/env.sh && MUNINN_HOME="$H" ./install.sh && m schedule run-now kraken-5m && tests/check_bars.sh`.
