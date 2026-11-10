//! Templates: `{rule}`, `{date}`, `{time}`, `{now}` and `{FIELD}` from a lookup.
use crate::compare::text;
use serde_json::Value;

/// Fill `{rule}` (the rule's name), `{date}` (`2026-11-01`), `{time}` (`030000`), `{now}`
/// (`2026-11-01 03:00`) from `now`, and any other `{FIELD}` from `field`; unknown ones and
/// ones with spaces stay as written. An unclosed `{` ends the template as written.
pub fn expand<Tz>(t: &str, now: &chrono::DateTime<Tz>, rule: &str, field: impl Fn(&str) -> Option<Value>) -> String
where
    Tz: chrono::TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let mut out = String::new();
    let mut rest = t;
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        let Some(j) = after.find('}') else {
            out.push_str(&rest[i..]);
            return out;
        };
        let key = &after[..j];
        let v = match key {
            "rule" => Some(rule.to_string()),
            "date" => Some(now.format("%Y-%m-%d").to_string()),
            "time" => Some(now.format("%H%M%S").to_string()),
            "now" => Some(now.format("%Y-%m-%d %H:%M").to_string()),
            k if !k.is_empty() && !k.contains(char::is_whitespace) => field(k).map(|v| text(&v)),
            _ => None,
        };
        match v {
            Some(v) => out.push_str(&v),
            None => out.push_str(&rest[i..i + j + 2]),
        }
        rest = &after[j + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, TimeZone};
    use serde_json::json;

    #[test]
    fn fills_from_a_fake_clock_and_lookup() {
        let now = FixedOffset::west_opt(5 * 3600).unwrap().with_ymd_and_hms(2026, 11, 1, 3, 4, 5).unwrap();
        let f = |k: &str| match k {
            "inserted" => Some(json!(401)),
            "out" => Some(json!("/tmp/a.pdf")),
            _ => None,
        };
        assert_eq!(expand("{rule}: {inserted} at {date} {time} ({now})", &now, "Nightly", f), "Nightly: 401 at 2026-11-01 030405 (2026-11-01 03:04)");
        assert_eq!(expand("{out}{missing}{ spaced }{}{open", &now, "r", f), "/tmp/a.pdf{missing}{ spaced }{}{open");
        assert_eq!(expand("no fields", &now, "r", f), "no fields");
    }
}
