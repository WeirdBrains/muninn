//! Timestamp parsing: epoch s/ms/us/ns and ISO / `YYYY-MM-DD HH:MM:SS`, UTC unless an offset is given.
use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeZone, Utc};

/// Interpret an integer epoch by magnitude.
pub fn epoch_to_ms(v: i64) -> i64 {
    let a = v.unsigned_abs();
    if a < 100_000_000_000 {
        v * 1000
    } else if a < 100_000_000_000_000 {
        v
    } else if a < 100_000_000_000_000_000 {
        v / 1000
    } else {
        v / 1_000_000
    }
}

/// Parse a timestamp string to epoch milliseconds (UTC).
pub fn parse_ts(s: &str) -> Option<i64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    if let Ok(i) = t.parse::<i64>() {
        return Some(epoch_to_ms(i));
    }
    if let Ok(f) = t.parse::<f64>() {
        if f.is_finite() {
            return Some(if f.abs() < 1e11 { (f * 1000.0).round() as i64 } else { epoch_to_ms(f as i64) });
        }
        return None;
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(t) {
        return Some(dt.timestamp_millis());
    }
    for fmt in ["%Y-%m-%d %H:%M:%S%.f%:z", "%Y-%m-%d %H:%M:%S%:z", "%Y-%m-%dT%H:%M:%S%.f%z"] {
        if let Ok(dt) = DateTime::parse_from_str(t, fmt) {
            return Some(dt.timestamp_millis());
        }
    }
    let t2 = t.trim_end_matches('Z').trim_end_matches(" UTC");
    for fmt in ["%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%d %H:%M", "%Y-%m-%dT%H:%M", "%Y/%m/%d %H:%M:%S"] {
        if let Ok(n) = NaiveDateTime::parse_from_str(t2, fmt) {
            return Some(Utc.from_utc_datetime(&n).timestamp_millis());
        }
    }
    if let Ok(d) = NaiveDate::parse_from_str(t2, "%Y-%m-%d") {
        return Some(Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0)?).timestamp_millis());
    }
    None
}

pub fn fmt_ts(ms: i64) -> String {
    match Utc.timestamp_millis_opt(ms) {
        chrono::LocalResult::Single(d) => d.format("%Y-%m-%d %H:%M:%S").to_string(),
        _ => ms.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        let want = 1_502_942_400_000; // 2017-08-17 04:00:00 UTC
        assert_eq!(parse_ts("2017-08-17 04:00:00"), Some(want));
        assert_eq!(parse_ts("2017-08-17T04:00:00Z"), Some(want));
        assert_eq!(parse_ts("2017-08-17T06:00:00+02:00"), Some(want));
        assert_eq!(parse_ts("1502942400"), Some(want));
        assert_eq!(parse_ts("1502942400000"), Some(want));
        assert_eq!(parse_ts("1502942400000000"), Some(want));
        assert_eq!(parse_ts("2017-08-17"), Some(1_502_928_000_000));
        assert_eq!(parse_ts("nope"), None);
        assert_eq!(fmt_ts(want), "2017-08-17 04:00:00");
    }
}
