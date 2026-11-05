// to-bars: rows into price bars, one shape among many, for a pipeline's `model:bars`
// leaf. Nothing is asked: what the rows are is told by options, else found by column
// names. Bars that fail the checks are not passed on; they are the run's second table,
// "Rejected" (row, reason), and counted in its log.
//
// Options (`code:to-bars?symbol=BTC/USDT&duration=5m`):
//   symbol    BASE/QUOTE for every row (else a `symbol` column, or `base` and `quote`)
//   source    the bars' source (else a `source` or `_source` column, else `pipeline`)
//   duration  bar length: 5m, 1h, 1d, 300 (seconds) (else a `period` column, else the
//             most common step between timestamps)
//   map       column names: `map=ts:timestamp,px:close` (fields: timestamp, open, high,
//             low, close, volume)
//   rejects   `fail`: stop the run when any row is rejected (default: pass the good ones)
//   drop_zero_volume  `true`: reject bars with volume 0 (default: keep and flag them)
//   check     `false`: no checks (default true)
// Columns found by name when not mapped: timestamp/ts/time/date/datetime/open_time,
// open/high/low/close, or one price/px/last column (then open = high = low = close), and
// volume/vol/qty/size (else 0). Timestamps: epoch seconds, ms, µs or ns (by size),
// `YYYY-MM-DD[ HH:MM[:SS]]` or RFC 3339; UTC unless an offset is given.
// A built-in example: copy it (`mnnn code create my-bars --from to-bars`) and change it.
use muninn_plugin::prelude::*;

fn duration_secs(s: &str) -> Option<f64> {
    let t = s.trim().to_ascii_lowercase();
    let i = t.find(|c: char| !c.is_ascii_digit() && c != '.').unwrap_or(t.len());
    let n: f64 = t[..i].parse().ok()?;
    let mult = match &t[i..] {
        "" | "s" | "sec" | "secs" => 1.0,
        "m" | "min" | "mins" | "t" => 60.0,
        "h" | "hr" | "hour" => 3600.0,
        "d" | "day" => 86_400.0,
        "w" | "wk" | "week" => 604_800.0,
        _ => return None,
    };
    (n > 0.0).then_some(n * mult)
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Epoch ms from a number (by magnitude) or a date-time text.
fn epoch_ms(v: &Json) -> Option<i64> {
    if let Some(n) = v.as_f64() {
        let a = n.abs();
        return Some(if a < 1e11 {
            n * 1000.0
        } else if a < 1e14 {
            n
        } else if a < 1e17 {
            n / 1000.0
        } else {
            n / 1e6
        } as i64);
    }
    let s = v.as_str()?.trim();
    let num = |a: usize, b: usize| s.get(a..b).and_then(|x| x.parse::<i64>().ok());
    let (y, mo, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (mut h, mut mi, mut se, mut ms) = (0, 0, 0.0f64, 0i64);
    let mut rest = &s[10.min(s.len())..];
    if let Some(r) = rest.strip_prefix(['T', ' ', 't']) {
        h = r.get(0..2)?.parse().ok()?;
        mi = r.get(3..5)?.parse().ok()?;
        rest = &r[5.min(r.len())..];
        if let Some(r) = rest.strip_prefix(':') {
            let end = r.find(|c: char| !c.is_ascii_digit() && c != '.').unwrap_or(r.len());
            se = r[..end].parse().ok()?;
            rest = &r[end..];
        }
    }
    let rest = rest.trim();
    if !rest.is_empty() && !rest.eq_ignore_ascii_case("z") && !rest.eq_ignore_ascii_case("utc") {
        let sign = if rest.starts_with('-') { -1 } else { 1 };
        let r = rest.trim_start_matches(['+', '-']).replace(':', "");
        let oh: i64 = r.get(0..2)?.parse().ok()?;
        let om: i64 = r.get(2..4).and_then(|x| x.parse().ok()).unwrap_or(0);
        ms = -sign * (oh * 3600 + om * 60) * 1000;
    }
    Some((days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60) * 1000 + (se * 1000.0) as i64 + ms)
}

fn find<'a>(f: &Frame, names: &[&'a str]) -> Option<String> {
    names.iter().copied().find(|n| f.has(n)).map(str::to_string).or_else(|| {
        // case-insensitively too
        let have: Vec<&String> = f.columns.iter().map(|(n, _)| n).collect();
        names.iter().find_map(|n| have.iter().find(|h| h.eq_ignore_ascii_case(n)).map(|h| h.to_string()))
    })
}

pub fn run(host: &Host) -> Result<View> {
    let f = host.input()?;
    let opt = |k: &str| host.option(k).filter(|v| !v.trim().is_empty());
    let truthy = |k: &str, d: bool| opt(k).map(|v| !matches!(v.to_ascii_lowercase().as_str(), "false" | "no" | "0" | "off")).unwrap_or(d);
    let mut map: Vec<(String, String)> = vec![];
    if let Some(m) = opt("map") {
        for pair in m.split(',').filter(|p| !p.trim().is_empty()) {
            let (c, field) = pair.split_once([':', '=']).ok_or_else(|| Error::new(format!("map: `{pair}` is not COLUMN:FIELD")))?;
            map.push((field.trim().to_ascii_lowercase(), c.trim().to_string()));
        }
    }
    let col = |field: &str, names: &[&str]| -> Option<String> {
        map.iter().find(|(k, _)| k == field).map(|(_, c)| c.clone()).or_else(|| find(&f, names))
    };
    let ts = col("timestamp", &["timestamp", "timestamp_utc", "ts", "time", "date", "datetime", "open_time", "_created_at"])
        .ok_or_else(|| Error::new("no timestamp column (timestamp/ts/time/date; or map=COLUMN:timestamp)"))?;
    let price = col("close", &["close", "c", "price", "px", "last"]).ok_or_else(|| Error::new("no close/price column (or map=COLUMN:close)"))?;
    let num_col = |n: Option<String>, fallback: &str| -> Result<Vec<Json>> {
        Ok(match n {
            Some(n) => f.column(&n)?.to_vec(),
            None => f.column(fallback)?.to_vec(),
        })
    };
    let open = num_col(col("open", &["open", "o"]), &price)?;
    let high = num_col(col("high", &["high", "h"]), &price)?;
    let low = num_col(col("low", &["low", "l"]), &price)?;
    let close = f.column(&price)?.to_vec();
    let vol = match col("volume", &["volume", "vol", "v", "qty", "size"]) {
        Some(v) => f.column(&v)?.to_vec(),
        None => vec![Json::Num(0.0); f.len()],
    };
    let t: Vec<Option<i64>> = f.column(&ts)?.iter().map(epoch_ms).collect();
    // symbol and source: options, else columns
    let symbol: Vec<Option<String>> = match opt("symbol") {
        Some(s) => vec![Some(s.trim().to_uppercase()); f.len()],
        None if f.has("symbol") => f.column("symbol")?.iter().map(|v| v.as_str().map(str::to_string)).collect(),
        None if f.has("base") && f.has("quote") => (0..f.len())
            .map(|i| match (f.get(i, "base").as_str(), f.get(i, "quote").as_str()) {
                (Some(b), Some(q)) => Some(format!("{b}/{q}")),
                _ => None,
            })
            .collect(),
        None => return Err(Error::new("no symbol: give the option symbol=BASE/QUOTE (code:to-bars?symbol=BTC/USDT) or a symbol column")),
    };
    let source: Vec<String> = match opt("source") {
        Some(s) => vec![s; f.len()],
        None => match find(&f, &["source", "_source"]) {
            Some(c) => f.column(&c)?.iter().map(|v| v.as_str().unwrap_or("pipeline").to_string()).collect(),
            None => vec!["pipeline".to_string(); f.len()],
        },
    };
    let dur: f64 = match opt("duration") {
        Some(d) => duration_secs(&d).ok_or_else(|| Error::new(format!("duration: `{d}` (e.g. 5m, 1h, 1d or seconds)")))?,
        None => {
            // most common step between consecutive timestamps
            let mut v: Vec<i64> = t.iter().flatten().copied().collect();
            v.sort_unstable();
            v.dedup();
            let mut counts: Vec<(i64, usize)> = vec![];
            for w in v.windows(2) {
                let d = w[1] - w[0];
                match counts.iter_mut().find(|(x, _)| *x == d) {
                    Some(c) => c.1 += 1,
                    None => counts.push((d, 1)),
                }
            }
            match counts.iter().max_by_key(|(_, n)| *n) {
                Some((d, _)) if *d >= 1000 => (*d / 1000) as f64,
                _ => return Err(Error::new("no duration: give the option duration=5m (too few timestamps to tell)")),
            }
        }
    };
    let check = truthy("check", true);
    let drop_zero = truthy("drop_zero_volume", false);
    let mut view = View::table("Bars", &["source", "symbol", "period", "timestamp", "open", "high", "low", "close", "volume", "flags"]);
    let mut rejected = View::table("Rejected", &["row", "reason"]);
    let (mut good, mut bad) = (0usize, 0usize);
    for i in 0..f.len() {
        let n = |c: &[Json]| c[i].as_f64().filter(|x| x.is_finite());
        let (o, h, l, c, v) = (n(&open), n(&high), n(&low), n(&close), n(&vol).or(Some(0.0)));
        let reason = match (t[i], &symbol[i], o, h, l, c, v) {
            (None, ..) => Some("timestamp is not a time".to_string()),
            (_, None, ..) => Some("no symbol".into()),
            (_, _, None, ..) | (_, _, _, None, ..) | (_, _, _, _, None, ..) | (_, _, _, _, _, None, _) => Some("missing or non-numeric price".into()),
            (Some(ts), _, Some(o), Some(h), Some(l), Some(c), Some(v)) if check => {
                if h < o.max(c) || l > o.min(c) || h < l {
                    Some(format!("high/low do not hold open and close (o {o} h {h} l {l} c {c})"))
                } else if v < 0.0 {
                    Some("negative volume".into())
                } else if v == 0.0 && drop_zero {
                    Some("zero volume".into())
                } else if dur <= 86_400.0 && ts.rem_euclid((dur * 1000.0) as i64) != 0 {
                    Some(format!("timestamp not aligned to {dur}s bars"))
                } else {
                    None
                }
            }
            _ => None,
        };
        match reason {
            Some(r) => {
                bad += 1;
                rejected = rejected.row(vec![((i + 1) as f64).into(), Json::Str(r)]);
            }
            None => {
                good += 1;
                let v = n(&vol).unwrap_or(0.0);
                view = view.row(vec![
                    Json::Str(source[i].clone()),
                    Json::Str(symbol[i].clone().unwrap_or_default()),
                    dur.into(),
                    (t[i].unwrap_or(0) as f64).into(),
                    o.unwrap_or(0.0).into(),
                    h.unwrap_or(0.0).into(),
                    l.unwrap_or(0.0).into(),
                    c.unwrap_or(0.0).into(),
                    v.into(),
                    (if v == 0.0 { 1.0 } else { 0.0 }).into(),
                ]);
            }
        }
    }
    host.log(format!("{good} bars from {ts} and {price} ({dur}s), {bad} rejected"));
    if bad > 0 && opt("rejects").is_some_and(|r| r.eq_ignore_ascii_case("fail")) {
        return Err(Error::new(format!("{bad} row(s) rejected and rejects=fail (run without it to see them in the Rejected table)")));
    }
    Ok(if bad > 0 { View::tiles("to-bars", true, vec![view, rejected]) } else { view })
}
