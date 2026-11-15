// exchange-bars: closed 5-minute OHLCV bars from Coinbase Exchange or Kraken public APIs.
// Ingest connector (SDK 1.3). Schedule options:
//   exchange        coinbase | kraken
//   symbols         comma list of BASE/QUOTE, e.g. BTC/USD,ETH/USD
//   lookback_hours  first run only (no cursor yet), default 24
// Per symbol: fetch what is newer than the cursor and, only when the response holds at least one new CLOSED bar,
// store its body unchanged with store_raw (name "<exchange> <BASE/QUOTE> <fetch epoch s>[ pN]"). The cursor
// (state `cursor:<BASE/QUOTE>` = open time in ms of the newest CLOSED bar) moves only past bars that are closed
// (open + 300 s <= now). The pipeline's parser (parse-<exchange>-...) turns the raw items into bars; the
// connector parses only to find the cursor.
// A quiet run (no symbol has a new closed bar) stores nothing and pipes nothing (`landed = 0`). The connector
// still calls pipe() and run_ruleset() on every run: the schedule's rule set holds a conditional gap alarm
// (`--on connector.run --if "landed = 0"`) that decides whether to fire. The connector has no alarm logic.
// The run's state is saved by Muninn only when the whole run succeeds, so a failed symbol means a re-fetch
// of every symbol's range next time (bars deduplicate by id).
use muninn_plugin::prelude::*;
use std::time::{SystemTime, UNIX_EPOCH};

const BAR_S: i64 = 300;
const CB_PAGE_BARS: i64 = 299; // Coinbase returns at most 300 rows per call
const CB_MAX_PAGES: i64 = 12; // per symbol per run (a long backfill continues next run)

fn now_s() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn iso(s: i64) -> String {
    // epoch seconds -> YYYY-MM-DDTHH:MM:SSZ (UTC)
    let days = s.div_euclid(86_400);
    let rem = s.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

fn split_symbol(sym: &str) -> Result<(String, String)> {
    let (b, q) = sym.split_once('/').ok_or_else(|| Error::new(format!("symbol `{sym}` is not BASE/QUOTE")))?;
    let ok = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric());
    if !ok(b) || !ok(q) {
        return Err(Error::new(format!("symbol `{sym}` is not BASE/QUOTE (letters and digits)")));
    }
    Ok((b.to_ascii_uppercase(), q.to_ascii_uppercase()))
}

/// Kraken's own names: BTC is XBT, DOGE is XDG (it accepts BTCUSD too, but XBTUSD is canonical there).
fn kraken_asset(a: &str) -> &str {
    match a {
        "BTC" => "XBT",
        "DOGE" => "XDG",
        o => o,
    }
}

struct Outcome {
    pages: Vec<(Vec<u8>, usize)>, // (raw body, closed bars in it)
    fetched: usize,
    closed: usize,
    newest_closed_ms: Option<i64>,
}

fn open_times_cb(body: &Json) -> Result<Vec<i64>> {
    let rows = body.as_array().ok_or_else(|| Error::new(format!("not a candle array: {}", body.to_string().chars().take(120).collect::<String>())))?;
    rows.iter()
        .map(|r| r.as_array().and_then(|c| c.first()).and_then(Json::as_i64).ok_or_else(|| Error::new("candle row without a time")))
        .collect()
}

fn open_times_kr(body: &Json) -> Result<Vec<i64>> {
    if let Some(errs) = body.get("error").and_then(Json::as_array) {
        if !errs.is_empty() {
            return Err(Error::new(format!("Kraken error: {}", errs.iter().map(|e| e.text()).collect::<Vec<_>>().join("; "))));
        }
    }
    let res = body.get("result").and_then(Json::as_object).ok_or_else(|| Error::new("Kraken reply has no result"))?;
    // the key is Kraken's internal pair name (XXBTZUSD): do not rely on it
    let (_, rows) = res.iter().find(|(k, v)| k != "last" && v.as_array().is_some()).ok_or_else(|| Error::new("Kraken result has no OHLC array"))?;
    rows.as_array()
        .unwrap_or(&[])
        .iter()
        .map(|r| r.as_array().and_then(|c| c.first()).and_then(Json::as_i64).ok_or_else(|| Error::new("OHLC row without a time")))
        .collect()
}

fn get(host: &Host, url: &str) -> Result<Json> {
    let r = host.fetch(&Request::get(url))?;
    if r.status != 200 {
        let t = r.text();
        return Err(Error::new(format!("HTTP {} {}", r.status, t.chars().take(100).collect::<String>())));
    }
    Json::parse(&r.text()).map_err(|e| Error::new(format!("bad JSON: {e}")))
}

fn fetch_coinbase(host: &Host, base: &str, quote: &str, cursor_ms: Option<i64>, lookback_s: i64, now: i64) -> Result<Outcome> {
    let id = format!("{base}-{quote}");
    let mut s = match cursor_ms {
        Some(c) => c / 1000 + BAR_S,
        None => (now - lookback_s).div_euclid(BAR_S) * BAR_S,
    };
    let mut out = Outcome { pages: vec![], fetched: 0, closed: 0, newest_closed_ms: None };
    let mut n = 0;
    // One request at least, even when nothing new can be closed yet; such a response is not stored (see `run`).
    while s <= now && n < CB_MAX_PAGES {
        let e = (s + CB_PAGE_BARS * BAR_S).min(now);
        let e = if e <= s { s + 1 } else { e };
        let url = format!("https://api.exchange.coinbase.com/products/{id}/candles?granularity={BAR_S}&start={}&end={}", iso(s), iso(e));
        let r = host.fetch(&Request::get(&url))?;
        if r.status != 200 {
            return Err(Error::new(format!("HTTP {} {}", r.status, r.text().chars().take(100).collect::<String>())));
        }
        let body = Json::parse(&r.text()).map_err(|e| Error::new(format!("bad JSON: {e}")))?;
        let times = open_times_cb(&body)?;
        let closed: Vec<i64> = times.iter().copied().filter(|t| t + BAR_S <= now && cursor_ms.map_or(true, |c| t * 1000 > c)).collect();
        out.fetched += times.len();
        out.closed += closed.len();
        if let Some(m) = closed.iter().max() {
            out.newest_closed_ms = out.newest_closed_ms.max(Some(m * 1000));
        }
        out.pages.push((r.body.clone(), closed.len()));
        n += 1;
        s = e + BAR_S;
    }
    Ok(out)
}

fn fetch_kraken(host: &Host, base: &str, quote: &str, cursor_ms: Option<i64>, lookback_s: i64, now: i64) -> Result<Outcome> {
    let pair = format!("{}{}", kraken_asset(base), kraken_asset(quote));
    // `since` is exclusive: rows with open time > since
    let since = match cursor_ms {
        Some(c) => c / 1000,
        None => (now - lookback_s).div_euclid(BAR_S) * BAR_S - 1,
    };
    let mut out = Outcome { pages: vec![], fetched: 0, closed: 0, newest_closed_ms: None };
    let url = format!("https://api.kraken.com/0/public/OHLC?pair={pair}&interval=5&since={since}");
    let r = host.fetch(&Request::get(&url))?;
    if r.status != 200 {
        return Err(Error::new(format!("HTTP {} {}", r.status, r.text().chars().take(100).collect::<String>())));
    }
    let body = Json::parse(&r.text()).map_err(|e| Error::new(format!("bad JSON: {e}")))?;
    let times = open_times_kr(&body)?; // fails on a non-empty `error` array even with HTTP 200
    let closed: Vec<i64> = times.iter().copied().filter(|t| t + BAR_S <= now && cursor_ms.map_or(true, |c| t * 1000 > c)).collect();
    out.fetched = times.len();
    out.closed = closed.len();
    out.newest_closed_ms = closed.iter().max().map(|m| m * 1000);
    out.pages.push((r.body.clone(), closed.len()));
    Ok(out)
}

pub fn run(host: &Host) -> Result<View> {
    let ctx = host.context()?;
    let exchange = host.option("exchange").unwrap_or_default().trim().to_ascii_lowercase();
    if exchange != "coinbase" && exchange != "kraken" {
        return Err(Error::new("give the schedule an option exchange=coinbase or exchange=kraken"));
    }
    let symbols: Vec<String> = host
        .option("symbols")
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_ascii_uppercase())
        .filter(|s| !s.is_empty())
        .collect();
    if symbols.is_empty() {
        return Err(Error::new("give the schedule an option symbols=BTC/USD,ETH/USD"));
    }
    let lookback_h = host.option("lookback_hours").and_then(|v| v.trim().parse::<f64>().ok()).filter(|h| *h > 0.0).unwrap_or(24.0);
    let lookback_s = (lookback_h * 3600.0) as i64;

    let mut view = View::table("Exchange bars", &["symbol", "bars fetched", "closed", "new cursor", "status"]);
    let mut failed: Vec<String> = vec![];
    let mut cursors: Vec<(String, i64)> = vec![];
    let mut stored = 0usize;
    let mut new_closed = 0usize;

    for sym in &symbols {
        let key = format!("cursor:{sym}");
        let cursor = host.state_get(&key)?.and_then(|v| v.as_i64());
        let now = now_s(); // read before the request: floor-second, so "closed" is conservative
        let res = split_symbol(sym).and_then(|(b, q)| match exchange.as_str() {
            "coinbase" => fetch_coinbase(host, &b, &q, cursor, lookback_s, now),
            _ => fetch_kraken(host, &b, &q, cursor, lookback_s, now),
        });
        match res {
            Ok(o) => {
                new_closed += o.closed;
                // store only responses that hold a new closed bar: a quiet run stores (and pipes) nothing
                for (i, (body, n_closed)) in o.pages.iter().enumerate() {
                    if *n_closed == 0 {
                        continue;
                    }
                    let name = if i == 0 { format!("{exchange} {sym} {now}") } else { format!("{exchange} {sym} {now} p{}", i + 1) };
                    host.store_raw(&name, "json", body)?;
                    stored += 1;
                }
                let newc = o.newest_closed_ms.or(cursor);
                if let Some(c) = newc {
                    if Some(c) != cursor {
                        cursors.push((key, c));
                    }
                }
                host.log(format!("{exchange} {sym}: {} fetched, {} new closed bars, cursor {}", o.fetched, o.closed, newc.map(|c| c.to_string()).unwrap_or_else(|| "none".into())));
                let status = if o.closed == 0 { "up to date" } else { "ok" };
                view = view.row(vec![
                    sym.as_str().into(),
                    o.fetched.into(),
                    o.closed.into(),
                    newc.map(|c| c.to_string()).unwrap_or_default().into(),
                    status.into(),
                ]);
            }
            Err(e) => {
                host.log(format!("warning: {exchange} {sym}: {e}"));
                failed.push(format!("{sym} ({e})"));
                view = view.row(vec![sym.as_str().into(), 0usize.into(), 0usize.into(), cursor.map(|c| c.to_string()).unwrap_or_default().into(), format!("FAILED: {e}").into()]);
            }
        }
    }

    // Cursors are set only after the raw items are stored. Muninn saves state only when the run succeeds, so a
    // run with a failed symbol re-fetches (and dedups) next time.
    for (k, c) in &cursors {
        host.state_set(k, *c)?;
    }
    // Always pipe (even if some symbols failed: what was stored must still become bars). With nothing stored Muninn
    // pipes nothing and answers `inserted: 0`. The pipeline must start with a `-` node: host.pipe() hands it only
    // this run's raw items; a `source:NAME` node would re-read every item ever stored.
    let mut inserted = 0usize;
    if ctx.pipeline.is_some() {
        inserted = host.pipe()?.inserted;
    }
    // Always run the schedule's rule set: it holds the gap alarm, whose `--if "landed = 0"` decides. (A schedule with
    // a rule set fails if the connector never calls run_ruleset.)
    if ctx.ruleset_id.is_some() {
        host.run_ruleset(None)?;
    }
    host.log(format!("{exchange}: {stored} raw items stored, {new_closed} new closed bars, {inserted} new rows piped, {} symbols failed", failed.len()));
    if !failed.is_empty() {
        return Err(Error::new(format!("{} symbol(s) failed: {}", failed.len(), failed.join("; "))));
    }
    Ok(view)
}
