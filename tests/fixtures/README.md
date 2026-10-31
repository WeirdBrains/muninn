Fixtures cut from `~/Projects/kommandosaldat/raw/data` (Binance 5m klines, UTC open times).

- `BTCUSDT_5m.csv`: rows 1-200 (contains zero-volume bars) + rows 5801-6001, which span the
  2017-09-06 15:55 -> 23:00 gap, plus one deliberately invalid row
  (`2017-09-06 15:57:00`, high below low and not aligned) inserted before the gap.
  401 valid rows + 1 invalid.
- `ETHUSDT_5m.csv`: first 300 rows.
- `unnamed.csv`: first 60 BTC rows; the file name carries no symbol or duration, so the
  symbol must come from `--symbol` (or a prompt) and the duration is derived from the data.
- `bars-2.14/`: a Muninn 2.14.0 home (password `correct horse battery staple`) with bars from
  three sources, four symbols and 5m/1h/1d periods, flags, fields added to bars (one built-in
  field retyped), a model referring to a bar, a project and history; beside it what 2.14
  reported (`bars.json`, `model.json`, …). Made by `scripts/make-bars-2.14-fixture.sh`;
  `tests/cli_upgrade_bars.rs` checks every row after the upgrade.
