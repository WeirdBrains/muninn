Fixtures cut from `~/Projects/kommandosaldat/raw/data` (Binance 5m klines, UTC open times).

- `BTCUSDT_5m.csv`: rows 1-200 (contains zero-volume bars) + rows 5801-6001, which span the
  2017-09-06 15:55 -> 23:00 gap, plus one deliberately invalid row
  (`2017-09-06 15:57:00`, high below low and not aligned) inserted before the gap.
  401 valid rows + 1 invalid.
- `ETHUSDT_5m.csv`: first 300 rows.
- `unnamed.csv`: first 60 BTC rows; the file name carries no symbol or duration, so the
  symbol must come from `--symbol` (or a prompt) and the duration is derived from the data.
