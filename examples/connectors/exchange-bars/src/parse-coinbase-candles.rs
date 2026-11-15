// parse-coinbase-candles: raw Coinbase Exchange candle responses into CLOSED 5-minute bars.
// Input: the frame `host.pipe()` hands a pipeline (columns text name format locator received_at item source),
// one row per raw item. Each text is `[[time_s, low, high, open, close, volume], ...]` (newest first;
// NOTE low comes before high). Output: timestamp (ms, bar open), open, high, low, close, volume, symbol.
use muninn_plugin::prelude::*;

const BAR_MS: f64 = 300_000.0;

/// Latest instant (ms) the item's data can be trusted as of: the earlier of the fetch time encoded in the
/// item name ("<exchange> <BASE/QUOTE> <epoch s>[ pN]", read by the connector BEFORE the request) and the
/// `received_at` column (when store_raw ran). A bar with open + 300 s after it may still have been forming.
fn cutoff_ms(name: &str, received_at: &Json) -> Option<f64> {
    let from_name = name.split_whitespace().nth(2).map(|t| t.trim_end_matches(".json")).and_then(|t| t.parse::<f64>().ok()).map(|s| s * 1000.0);
    let from_recv = received_at.as_f64().map(|r| if r < 1e11 { r * 1000.0 } else { r });
    match (from_name, from_recv) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// BASE/QUOTE from the item name. `BTC-USD` and `BTC_USD` are read as BTC/USD too, because an item that
/// came from a file (`mnnn ingest json`, used to test this parser offline) cannot have a `/` in its name.
fn symbol_of(name: &str) -> Option<String> {
    let s = name.split_whitespace().nth(1)?.replace(['-', '_'], "/");
    s.contains('/').then_some(s)
}

pub fn run(host: &Host) -> Result<View> {
    let f = host.input()?;
    let texts = f.text("text")?;
    let names = f.text("name")?;
    let has_recv = f.has("received_at");
    let mut view = View::table("Bars", &["timestamp", "open", "high", "low", "close", "volume", "symbol"]);
    let (mut kept, mut dropped, mut seen) = (0usize, 0usize, std::collections::HashSet::new());
    for (i, text) in texts.iter().enumerate() {
        let name = &names[i];
        let symbol = symbol_of(name).ok_or_else(|| Error::new(format!("item name `{name}` is not `<exchange> <BASE/QUOTE> <epoch s>`")))?;
        let recv = if has_recv { f.get(i, "received_at") } else { Json::Null };
        // no usable fetch time at all: refuse rather than risk a forming bar reaching model:bars
        let cutoff = cutoff_ms(name, &recv).ok_or_else(|| Error::new(format!("item `{name}`: no fetch time (name or received_at)")))?;
        let body = Json::parse(text).map_err(|e| Error::new(format!("item `{name}`: bad JSON: {e}")))?;
        for row in rows_of(&body).map_err(|e| Error::new(format!("item `{name}`: {e}")))? {
            let c = row.as_array().ok_or_else(|| Error::new(format!("item `{name}`: a candle is not an array")))?;
            let (open_ms, o, h, l, cl, v) = candle(c).ok_or_else(|| Error::new(format!("item `{name}`: a candle row is malformed")))?;
            if open_ms + BAR_MS > cutoff {
                dropped += 1; // still forming when it was fetched
                continue;
            }
            if !seen.insert((symbol.clone(), open_ms as i64)) {
                continue; // the same bar twice in one run (overlapping pages): keep the first
            }
            view = view.row(vec![open_ms.into(), o.into(), h.into(), l.into(), cl.into(), v.into(), symbol.as_str().into()]);
            kept += 1;
        }
    }
    host.log(format!("{kept} closed bars, {dropped} forming bars dropped (cutoff: fetch time from the item name{})", if has_recv { " and received_at, whichever is earlier" } else { "; no received_at column" }));
    Ok(view)
}

fn rows_of(body: &Json) -> std::result::Result<&[Json], String> {
    body.as_array().ok_or_else(|| "not a candle array".to_string())
}

/// [time_s, low, high, open, close, volume] -> (open ms, open, high, low, close, volume)
fn candle(c: &[Json]) -> Option<(f64, f64, f64, f64, f64, f64)> {
    if c.len() < 6 {
        return None;
    }
    let n = |i: usize| c[i].as_f64().filter(|x| x.is_finite());
    Some((n(0)? * 1000.0, n(3)?, n(2)?, n(1)?, n(4)?, n(5)?))
}
