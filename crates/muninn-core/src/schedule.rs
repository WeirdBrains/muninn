//! 2.25 schedules: the interval and date parsers, next-due stepping and the
//! scheduler (`mnnn scheduler tick|run`). Tables and storage are in
//! [`crate::store::schedules`]; see `docs/schedules.md`.
use crate::error::{Error, Result};
use crate::store::{NewAction, ScheduleDef, ScheduleRun, Store, now_ms};
use serde::Serialize;
use serde_json::{Value, json};

pub const ORIGIN: &str = "scheduler";
/// Setting: last scheduler tick (epoch ms). Distinguishes a continuous daemon from a restart.
pub const LAST_TICK_KEY: &str = "scheduler.last_tick_at";
/// Who wrote [`LAST_TICK_KEY`] (`cli:PID`, `tui:PID`, …). A new process/session must not
/// look "alive" just because a persisted tick is recent (quick restart across a deadline).
pub const LAST_HOLDER_KEY: &str = "scheduler.last_tick_holder";
/// start_at this far in the past counts as "past" when adding/editing (UX only).
pub const PAST_SLACK_MS: i64 = 60_000;
/// Max gap between ticks of the *same* holder still counts as one continuous session.
pub const ALIVE_GAP_MS: i64 = 120_000;
/// A running run whose heartbeat is older than this is abandoned.
pub const ABANDON_MS: i64 = 120_000;
/// How long a tick's lease lasts.
pub const LEASE_MS: i64 = 30_000;
/// Runs started per tick at most.
pub const WORKERS: usize = 4;
/// `catch_up all` starts this many missed runs at most.
pub const CATCH_UP_MAX: usize = 100;

// ------------------------------------------------------------------ zones and times

/// `ms` as a zoned time in `tz` (local when none).
fn zoned(ms: i64, tz: Option<&str>) -> Option<parse_datetime::ParsedDateTime> {
    let at = format!("@{}.{:03}", ms.div_euclid(1000), ms.rem_euclid(1000));
    match tz {
        Some(z) => {
            // the zone itself, then the instant placed in it
            let zone = parse_datetime::parse_datetime(format!("TZ=\"{z}\" 2000-01-01")).ok()?.into_zoned()?;
            let inst = parse_datetime::parse_datetime(at).ok()?.into_zoned()?;
            Some(parse_datetime::ParsedDateTime::InRange(inst.with_time_zone(zone.time_zone().clone())))
        }
        None => parse_datetime::parse_datetime(at).ok(),
    }
}

/// The machine's IANA zone (`America/New_York`), when it has a name.
pub fn local_zone() -> Option<String> {
    let z = parse_datetime::parse_datetime("now").ok()?.into_zoned()?;
    z.time_zone().iana_name().map(str::to_string)
}

/// Check a zone name (`UTC`, `Europe/Oslo`).
pub fn check_zone(tz: &str) -> Result<String> {
    // an unknown name falls back to UTC in the parser: check the name came through
    let named = zoned(0, Some(tz)).and_then(|z| z.into_zoned()).and_then(|z| z.time_zone().iana_name().map(str::to_string));
    if tz.contains('"') || !named.is_some_and(|n| n.eq_ignore_ascii_case(tz)) {
        return Err(Error::invalid("tz", format!("`{tz}` is not a time zone (try UTC or Europe/Oslo)")));
    }
    Ok(tz.to_string())
}

/// Wall clock of `ms` in `tz` (UTC when none): (y, mo, d, h, mi, s, ms).
fn civil(ms: i64, tz: Option<&str>) -> (i32, u32, u32, u32, u32, u32, u32) {
    if let Some(z) = tz.and_then(|t| zoned(ms, Some(t))).and_then(|z| z.into_zoned()) {
        let d = z.datetime();
        return (
            d.year() as i32,
            d.month() as u32,
            d.day() as u32,
            d.hour() as u32,
            d.minute() as u32,
            d.second() as u32,
            (d.subsec_nanosecond() / 1_000_000) as u32,
        );
    }
    use chrono::{Datelike, TimeZone, Timelike};
    let d = chrono::Utc.timestamp_millis_opt(ms).single().unwrap_or_default();
    (d.year(), d.month(), d.day(), d.hour(), d.minute(), d.second(), d.timestamp_subsec_millis())
}

fn from_civil(c: (i32, u32, u32, u32, u32, u32, u32), tz: Option<&str>) -> Option<i64> {
    let (y, mo, d, h, mi, s, ms) = c;
    let text = format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}.{ms:03}");
    match tz {
        Some(z) => parse_datetime::parse_datetime(format!("TZ=\"{z}\" {text}")).ok()?.into_zoned().map(|z| z.timestamp().as_millisecond()),
        None => {
            use chrono::TimeZone;
            let n = chrono::NaiveDate::from_ymd_opt(y, mo, d)?.and_hms_milli_opt(h, mi, s, ms)?;
            Some(chrono::Utc.from_utc_datetime(&n).timestamp_millis())
        }
    }
}

fn days_in(y: i32, m: u32) -> u32 {
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    chrono::NaiveDate::from_ymd_opt(ny, nm, 1).and_then(|d| d.pred_opt()).map_or(28, |d| chrono::Datelike::day(&d))
}

/// `ms` plus `months` in `tz`, the day kept at `day` when the month has it (Jan 31 →
/// Feb 28 → Mar 31).
pub fn add_months(ms: i64, months: i64, day: u32, tz: Option<&str>) -> i64 {
    let (y, mo, _, h, mi, s, sub) = civil(ms, tz);
    let total = y as i64 * 12 + (mo as i64 - 1) + months;
    let (ny, nm) = (total.div_euclid(12) as i32, total.rem_euclid(12) as u32 + 1);
    let nd = day.min(days_in(ny, nm));
    from_civil((ny, nm, nd, h, mi, s, sub), tz).unwrap_or(ms + months * 30 * 86_400_000)
}

/// `Tue 2026-11-10 09:00:00 EST (14:00:00Z)` style text for `ms`.
pub fn show_time(ms: i64, tz: Option<&str>) -> String {
    use chrono::TimeZone;
    let utc = chrono::Utc.timestamp_millis_opt(ms).single().map(|d| d.format("%Y-%m-%d %H:%M:%SZ").to_string()).unwrap_or_default();
    let z = zoned(ms, tz).and_then(|z| z.into_zoned());
    match z {
        Some(z) if tz.is_some_and(|t| t != "UTC") || tz.is_none() => format!("{} ({utc})", z.strftime("%a %Y-%m-%d %H:%M:%S %Z")),
        _ => utc,
    }
}

// ------------------------------------------------------------------ run_every

/// A parsed `run_every`: months plus fixed milliseconds, or `once`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct Interval {
    pub months: i64,
    pub ms: i64,
    pub once: bool,
}

const TIME_WORDS: &[&str] = &[
    "ago",
    "next",
    "last",
    "this",
    "now",
    "today",
    "tomorrow",
    "yesterday",
    "noon",
    "midnight",
    "am",
    "pm",
    "at",
    "on",
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
    "mon",
    "tue",
    "wed",
    "thu",
    "fri",
    "sat",
    "sun",
    "weekday",
    "weekdays",
];

fn unit(u: &str) -> Option<(i64, i64)> {
    // (months, ms); short codes are case sensitive, words are not
    let short = match u {
        "ms" => Some((0, 1)),
        "s" => Some((0, 1000)),
        "m" => Some((0, 60_000)),
        "h" => Some((0, 3_600_000)),
        "d" => Some((0, 86_400_000)),
        "w" => Some((0, 7 * 86_400_000)),
        "M" => Some((1, 0)),
        "y" => Some((12, 0)),
        _ => None,
    };
    if short.is_some() {
        return short;
    }
    let l = u.to_ascii_lowercase();
    let l = l.strip_suffix('s').filter(|x| !x.is_empty()).unwrap_or(&l);
    Some(match l {
        "millisecond" | "msec" => (0, 1),
        "second" | "sec" => (0, 1000),
        "minute" | "min" => (0, 60_000),
        "hour" => (0, 3_600_000),
        "day" => (0, 86_400_000),
        "week" => (0, 7 * 86_400_000),
        "fortnight" => (0, 14 * 86_400_000),
        "month" => (1, 0),
        "year" => (12, 0),
        _ => return None,
    })
}

/// Parse `run_every`: date items (`2 hours 30 minutes`, `1 month`), short codes (`90m`,
/// `1h30m`, `2w`, `1M`, `1y`), bare milliseconds (`5000`), or `once`. Times (`tomorrow`,
/// `9am`, `next monday`) are refused: they belong in `--start-at`.
pub fn parse_interval(s: &str) -> Result<Interval> {
    let t = s.trim();
    let bad = |why: &str| Error::invalid("run_every", format!("`{s}`: {why}"));
    if t.eq_ignore_ascii_case("once") {
        return Ok(Interval { once: true, ..Default::default() });
    }
    if t.is_empty() {
        return Err(bad("empty (try 1h, 2 hours, 1 month or once)"));
    }
    if let Ok(ms) = t.parse::<i64>() {
        return if ms > 0 { Ok(Interval { ms, ..Default::default() }) } else { Err(bad("must be more than 0")) };
    }
    let mut out = Interval::default();
    let mut words = t.split(|c: char| c.is_whitespace() || c == ',').filter(|w| !w.is_empty() && !w.eq_ignore_ascii_case("and")).peekable();
    let mut any = false;
    while let Some(w) = words.next() {
        let lw = w.to_ascii_lowercase();
        if TIME_WORDS.contains(&lw.as_str())
            || lw.ends_with("am") && lw[..lw.len() - 2].parse::<u32>().is_ok()
            || lw.ends_with("pm") && lw[..lw.len() - 2].parse::<u32>().is_ok()
            || w.contains(':')
        {
            return Err(bad("that is a time, not an interval; put it in --start-at and give run_every as a length (1 day, 1w)"));
        }
        // a bare unit word: `hour` = 1 hour
        if let Some((mo, ms)) = unit(w).filter(|_| w.chars().all(|c| c.is_ascii_alphabetic()) && w.len() > 1 && w != "ms") {
            out.months += mo;
            out.ms += ms;
            any = true;
            continue;
        }
        // compact runs: 1h30m, 90m, 2 hours
        let mut rest = w;
        while !rest.is_empty() {
            let n_end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
            if n_end == 0 {
                return Err(bad(&format!("`{rest}` is not a number and unit")));
            }
            let n: i64 = rest[..n_end].parse().map_err(|_| bad("number too large"))?;
            rest = &rest[n_end..];
            let u_end = rest.find(|c: char| c.is_ascii_digit()).unwrap_or(rest.len());
            let u = if u_end == 0 {
                match words.peek() {
                    Some(nw) if unit(nw).is_some() => words.next().unwrap_or_default(),
                    _ => return Err(bad("a number needs a unit (ms, s, m, h, d, w, M, y or words)")),
                }
            } else {
                let u = &rest[..u_end];
                rest = &rest[u_end..];
                u
            };
            let (mo, ms) = unit(u).ok_or_else(|| bad(&format!("`{u}` is not a unit (ms, s, m, h, d, w, M, y; S, H, D and mo are not)")))?;
            out.months += n * mo;
            out.ms += n * ms;
            any = true;
        }
    }
    if !any || out.months == 0 && out.ms == 0 {
        return Err(bad("must be more than 0"));
    }
    Ok(out)
}

/// The long form stored next to a schedule: `1 month 2 days 3 hours`.
pub fn long_form(months: i64, ms: i64) -> String {
    let mut parts = vec![];
    let mut push = |n: i64, u: &str| {
        if n > 0 {
            parts.push(format!("{n} {u}{}", if n == 1 { "" } else { "s" }));
        }
    };
    if months % 12 == 0 {
        push(months / 12, "year");
    } else {
        push(months, "month");
    }
    let mut r = ms;
    for (size, u) in [(7 * 86_400_000, "week"), (86_400_000, "day"), (3_600_000, "hour"), (60_000, "minute"), (1000, "second"), (1, "millisecond")] {
        push(r / size, u);
        r %= size;
    }
    if parts.is_empty() { "0 milliseconds".into() } else { parts.join(" ") }
}

// ------------------------------------------------------------------ start_at / ends_at

/// Parse a date for `--start-at` / `--ends-at` (GNU `date -d` style: `tomorrow 9am`,
/// `2026-12-01 08:00`, `next monday`, `+2 hours`, `@1767225600`), relative to `now`, read
/// in `tz` (local when none) unless it names its own zone.
pub fn parse_when(input: &str, now: i64, tz: Option<&str>) -> Result<i64> {
    let base = zoned(now, tz).and_then(|z| z.into_zoned()).ok_or_else(|| Error::invalid("date", "bad base time".to_string()))?;
    let text = match tz {
        Some(z) if !input.trim_start().starts_with("TZ=") => format!("TZ=\"{z}\" {input}"),
        _ => input.to_string(),
    };
    let p = parse_datetime::parse_datetime_at_date(base, &text).map_err(|e| Error::invalid("date", format!("`{input}`: {e}")))?;
    let z = p.into_zoned().ok_or_else(|| Error::invalid("date", format!("`{input}` is not a date")))?;
    Ok(z.timestamp().as_millisecond())
}

// ------------------------------------------------------------------ stepping

fn anchor_day(s: &ScheduleDef) -> u32 {
    civil(s.start_at.unwrap_or(s.created_at.max(0)), s.tz.as_deref()).2
}

/// The slot after `t` (a due time) for `s`, ignoring `ends_at`. `None` for `once`.
pub fn step(s: &ScheduleDef, t: i64) -> Option<i64> {
    if s.is_once() {
        return None;
    }
    if let Some(c) = &s.cron {
        return cron_next(c, t, s.tz.as_deref());
    }
    let months = s.every_months.unwrap_or(0);
    let ms = s.every_ms.unwrap_or(0);
    if months == 0 && ms <= 0 {
        return None;
    }
    let mut n = if months > 0 { add_months(t, months, anchor_day(s), s.tz.as_deref()) } else { t };
    n += ms;
    Some(n.max(t + 1))
}

/// Next cron minute after `after` in `tz` (UTC when none).
pub fn cron_next(cron: &str, after: i64, tz: Option<&str>) -> Option<i64> {
    let c = crate::rules::parse_cron(cron).ok()?;
    let Some(z) = tz else {
        return c.next_after_in(after, &chrono::Utc);
    };
    let offset = |ms: i64| zoned(ms, Some(z)).and_then(|z| z.into_zoned()).map(|z| z.offset().seconds()).and_then(chrono::FixedOffset::east_opt);
    let off = offset(after)?;
    let n = c.next_after_in(after, &off)?;
    let real = offset(n)?;
    if real == off {
        return Some(n);
    }
    // the offset changed (DST) before `n`: evaluate again in the new offset
    c.next_after_in(after, &real)
}

/// The first slot of a new or re-enabled schedule at `now`.
pub fn first_due(s: &ScheduleDef, now: i64) -> Option<i64> {
    match (s.start_at, &s.cron) {
        (Some(st), _) if st >= now => {
            if s.cron.is_some() {
                cron_next(s.cron.as_deref()?, st - 1, s.tz.as_deref())
            } else {
                Some(st)
            }
        }
        (Some(st), None) => {
            if s.is_once() {
                return None;
            }
            let mut t = st;
            let mut i = 0;
            while t < now && i < 1_000_000 {
                t = step(s, t)?;
                i += 1;
            }
            Some(t)
        }
        (_, Some(c)) => cron_next(c, now - 1, s.tz.as_deref()),
        (None, None) => Some(now),
    }
}

// ------------------------------------------------------------------ building a schedule

/// What the user gave for a schedule's timing.
#[derive(Debug, Clone, Default)]
pub struct Timing {
    pub run_every: Option<String>,
    pub cron: Option<String>,
    pub start_at: Option<String>,
    pub ends_at: Option<String>,
    /// `--for 2 weeks`: ends_at = start (or now) + this.
    pub for_: Option<String>,
    pub tz: Option<String>,
    /// `--past next-interval`: a past start moves to the next slot.
    pub past_next: bool,
}

/// A past start_at: what can be done.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Past {
    /// Fine (not in the past).
    No,
    /// In the past; `next` is the next slot (none for a one-off: correct it).
    Yes { next: Option<i64> },
}

/// Fill `s`'s timing fields from `t` at `now`. A past start_at is an error unless
/// `past_next` (and not `once`); [`past_of`] tells the CLI what to offer.
pub fn apply_timing(s: &mut ScheduleDef, t: &Timing, now: i64) -> Result<()> {
    if let Some(z) = &t.tz {
        s.tz = if z.eq_ignore_ascii_case("local") { local_zone() } else { Some(check_zone(z)?) };
    }
    let tz = s.tz.clone();
    match (&t.run_every, &t.cron) {
        (Some(_), Some(_)) => return Err(Error::invalid("schedule", "give run_every or --cron, not both".to_string())),
        (Some(e), None) => {
            let iv = parse_interval(e)?;
            s.cron = None;
            if iv.once {
                s.run_every = Some("once".into());
                s.every_ms = None;
                s.every_months = None;
            } else {
                s.run_every = Some(long_form(iv.months, iv.ms));
                s.every_ms = Some(iv.ms);
                s.every_months = Some(iv.months).filter(|m| *m > 0);
            }
        }
        (None, Some(c)) => {
            crate::rules::parse_cron(c)?;
            s.cron = Some(c.trim().to_string());
            s.run_every = None;
            s.every_ms = None;
            s.every_months = None;
        }
        (None, None) => {}
    }
    if s.run_every.is_none() && s.cron.is_none() {
        if t.start_at.is_some() || s.start_at.is_some() {
            s.run_every = Some("once".into());
            s.every_ms = None;
            s.every_months = None;
        } else {
            return Err(Error::missing("run_every", "give an interval (1h, 2 days, 1 month, once) or --cron \"0 9 * * 1-5\"".to_string()));
        }
    }
    if let Some(st) = &t.start_at {
        s.start_at = Some(parse_when(st, now, tz.as_deref())?);
        s.start_at_input = Some(st.clone());
    }
    if s.is_once() && s.start_at.is_none() {
        return Err(Error::missing("start_at", "a one-off (once) needs --start-at, e.g. --start-at \"tomorrow 9am\"".to_string()));
    }
    if s.is_once() && (t.ends_at.is_some() || t.for_.is_some()) {
        return Err(Error::invalid("ends_at", "a once schedule runs a single time at start_at; its end is filled in automatically".to_string()));
    }
    if let Some(e) = &t.ends_at {
        if t.for_.is_some() {
            return Err(Error::invalid("schedule", "give --ends-at or --for, not both".to_string()));
        }
        s.ends_at = Some(parse_when(e, now, tz.as_deref())?);
        s.ends_at_input = Some(e.clone());
    }
    if let Some(f) = &t.for_ {
        let iv = parse_interval(f)?;
        if iv.once {
            return Err(Error::invalid("for", "--for takes a length (2 weeks)".to_string()));
        }
        let from = s.start_at.unwrap_or(now);
        s.ends_at = Some(add_months(from, iv.months, civil(from, tz.as_deref()).2, tz.as_deref()) + iv.ms);
        s.ends_at_input = Some(format!("for {f}"));
    }
    if s.is_once() {
        // `once` ends 1 ms after it starts — generic ends_at path disables after the run
        s.ends_at = s.start_at.map(|x| x + 1);
        s.ends_at_input = Some("auto (once)".into());
    }
    if let (Some(st), Some(en)) = (s.start_at, s.ends_at)
        && en <= st
    {
        return Err(Error::invalid("ends_at", format!("{} is not after start_at {}", show_time(en, tz.as_deref()), show_time(st, tz.as_deref()))));
    }
    if let Some(en) = s.ends_at
        && en <= now
        && !s.is_once()
    {
        return Err(Error::invalid("ends_at", format!("{} is in the past", show_time(en, tz.as_deref()))));
    }
    match past_of(s, now) {
        Past::No => {}
        Past::Yes { next: None } => {
            return Err(Error::invalid(
                "start_at",
                format!(
                    "{} is in the past; a one-off can only be corrected: give a new --start-at",
                    show_time(s.start_at.unwrap_or(now), tz.as_deref())
                ),
            ));
        }
        Past::Yes { next: Some(n) } => {
            if !t.past_next {
                return Err(Error::invalid(
                    "start_at",
                    format!(
                        "{} is in the past; the next interval would be {} (--past next-interval), or give a new --start-at",
                        show_time(s.start_at.unwrap_or(now), tz.as_deref()),
                        show_time(n, tz.as_deref())
                    ),
                ));
            }
            s.start_at = Some(n);
        }
    }
    s.next_run_at = first_due(s, now).ok_or_else(|| Error::invalid("schedule", "it would never run".to_string()))?;
    if let Some(en) = s.ends_at
        && s.next_run_at >= en
        && !s.is_once()
    {
        return Err(Error::invalid(
            "ends_at",
            format!("it ends ({}) before its first run ({})", show_time(en, tz.as_deref()), show_time(s.next_run_at, tz.as_deref())),
        ));
    }
    Ok(())
}

/// Whether `s.start_at` is in the past at `now` (a minute's slack), and the next slot.
pub fn past_of(s: &ScheduleDef, now: i64) -> Past {
    match s.start_at {
        Some(st) if st < now - PAST_SLACK_MS => Past::Yes { next: if s.is_once() { None } else { first_due(s, now) } },
        _ => Past::No,
    }
}

/// Confirmation lines for a schedule (what the CLI prints after add/edit/parse).
pub fn describe(s: &ScheduleDef) -> Vec<String> {
    let tz = s.tz.as_deref();
    let mut out = vec![];
    let zone = tz.map_or("UTC".to_string(), str::to_string);
    if s.is_once() {
        out.push(format!("once at {}", show_time(s.start_at.unwrap_or(s.next_run_at), tz)));
    } else if let Some(c) = &s.cron {
        out.push(format!("cron `{c}` in {zone}"));
    } else if let Some(e) = &s.run_every {
        out.push(format!("every {e}{}", if s.every_months.is_some() { format!(" (calendar months in {zone})") } else { String::new() }));
    }
    if let Some(st) = s.start_at.filter(|_| !s.is_once()) {
        out.push(format!("starts {}{}", show_time(st, tz), s.start_at_input.as_ref().map(|i| format!("  (from \"{i}\")")).unwrap_or_default()));
    }
    if let Some(en) = s.ends_at.filter(|_| !s.is_once()) {
        out.push(format!("ends   {}{}", show_time(en, tz), s.ends_at_input.as_ref().map(|i| format!("  (from \"{i}\")")).unwrap_or_default()));
    }
    out.push(format!("next   {}", if s.enabled { show_time(s.next_run_at, tz) } else { "- (disabled)".into() }));
    out
}

// ------------------------------------------------------------------ rule sets

/// What running a rule set did.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SetRun {
    pub ran: Vec<String>,
    pub failed: Vec<String>,
    pub messages: Vec<String>,
}

/// Run each enabled rule of a rule set, in order, for action `a`.
pub fn run_ruleset(store: &Store, set_id: &str, a: &crate::store::ActionRecord) -> Result<SetRun> {
    let mut out = SetRun::default();
    for r in store.ruleset_rules(set_id)? {
        if !r.enabled {
            continue;
        }
        let res = crate::rules::run(store, &r, a);
        if res.status == "failed" {
            out.failed.push(r.name.clone());
        } else {
            out.ran.push(r.name.clone());
        }
        out.messages.extend(res.messages);
    }
    Ok(out)
}

// ------------------------------------------------------------------ the scheduler

/// What one tick did.
#[derive(Debug, Clone, Default, Serialize)]
pub struct TickReport {
    /// Another scheduler holds the lease.
    pub standby: bool,
    pub queued: Vec<String>,
    pub started: Vec<i64>,
    pub succeeded: Vec<i64>,
    pub failed: Vec<i64>,
    pub missed: Vec<String>,
    pub ended: Vec<String>,
    pub abandoned: Vec<i64>,
    pub retries: Vec<i64>,
    /// Old rules still carrying their own schedule (fired the 2.24 way).
    pub legacy: Vec<String>,
}

/// Wait before retry `attempt` (1-based count of failures so far): 30 s, 1 min, 2 min, ... ≤ 1 h.
pub fn backoff(attempt: i64) -> i64 {
    (30_000i64 << (attempt - 1).clamp(0, 7)).min(3_600_000)
}

fn record(store: &Store, kind: &str, summary: String, params: Value) -> Option<i64> {
    store.record_action(&NewAction::new(kind, ORIGIN, summary).params(params)).ok()
}

/// One pass: take the lease, fail abandoned runs, queue due slots, start up to
/// [`WORKERS`] pending runs. `holder` names this scheduler (`cli:PID`, `tui:PID`).
pub fn tick(store: &Store, holder: &str, now: i64) -> Result<TickReport> {
    let mut rep = TickReport::default();
    if !store.take_lease(holder, now, now + LEASE_MS)? {
        rep.standby = true;
        return Ok(rep);
    }
    // abandoned
    for mut r in store.open_runs()? {
        if r.status == "running" && r.heartbeat_at.unwrap_or(0) < now - ABANDON_MS {
            r.status = "failed".into();
            r.finished_at = Some(now);
            r.error = Some(format!("abandoned (no heartbeat for {} s)", ABANDON_MS / 1000));
            store.update_run(&r)?;
            rep.abandoned.push(r.id);
            if let Some(s) = store.schedule(&r.schedule_id)? {
                retry(store, &s, &r, now, &mut rep)?;
            }
        }
    }
    let last_tick = last_tick_at(store)?;
    let last_holder = last_tick_holder(store)?;
    // Continuous only if *this* scheduler session wrote the last tick recently.
    // A quick restart (new holder) within ALIVE_GAP_MS must not inherit aliveness.
    let alive = last_tick > 0 && last_holder.as_deref() == Some(holder) && now.saturating_sub(last_tick) <= ALIVE_GAP_MS;
    // due slots
    for mut s in store.schedules()? {
        if !s.enabled || s.next_run_at > now {
            continue;
        }
        queue_due(store, &mut s, now, last_tick, alive, &mut rep)?;
    }
    // leftover legacy per-rule schedules (pre-2.25; edit --every migrates them off)
    rep.legacy = crate::rules::tick(store, now)?;
    set_last_tick(store, holder, now)?;
    // start pending
    let pending: Vec<ScheduleRun> = store.open_runs()?.into_iter().filter(|r| r.status == "pending" && r.not_before <= now).collect();
    let mut busy: Vec<String> = store.open_runs()?.into_iter().filter(|r| r.status == "running").map(|r| r.schedule_id).collect();
    for r in pending {
        if rep.started.len() >= WORKERS {
            break;
        }
        if busy.contains(&r.schedule_id) {
            continue; // one at a time per schedule; a queued run waits
        }
        let Some(s) = store.schedule(&r.schedule_id)? else { continue };
        if !store.claim_run(r.id, holder, now)? {
            continue;
        }
        busy.push(s.id.clone());
        rep.started.push(r.id);
        let done = execute(store, &s, r.id, holder, now)?;
        if done.status == "success" {
            rep.succeeded.push(done.id);
        } else {
            rep.failed.push(done.id);
            retry(store, &s, &done, now, &mut rep)?;
        }
        busy.retain(|b| b != &s.id);
    }
    store.take_lease(holder, now, now + LEASE_MS)?;
    Ok(rep)
}

fn last_tick_at(store: &Store) -> Result<i64> {
    Ok(store.setting(LAST_TICK_KEY)?.as_deref().and_then(|s| s.parse().ok()).unwrap_or(0))
}

fn last_tick_holder(store: &Store) -> Result<Option<String>> {
    store.setting(LAST_HOLDER_KEY)
}

fn set_last_tick(store: &Store, holder: &str, now: i64) -> Result<()> {
    store.set_setting(LAST_TICK_KEY, Some(&now.to_string()))?;
    store.set_setting(LAST_HOLDER_KEY, Some(holder))
}

/// Queue due slots.
///
/// A continuous *session* (`alive`: same holder as the last tick, within [`ALIVE_GAP_MS`])
/// that crosses a due time runs it even when a tiny `once` window has closed. A new
/// process/session after the deadline (`!alive` and `ends_at <= now`) takes the generic
/// ended-while-missed path — a quick restart must not look alive from a stale tick alone.
fn queue_due(store: &Store, s: &mut ScheduleDef, now: i64, last_tick: i64, alive: bool, rep: &mut TickReport) -> Result<()> {
    let end = s.ends_at;
    if !alive && end.is_some_and(|e| e <= now) && s.next_run_at < end.unwrap_or(0) && s.next_run_at > last_tick {
        let due = s.next_run_at;
        store.insert_run(&ScheduleRun {
            schedule_id: s.id.clone(),
            status: "failed".into(),
            timestamp: due,
            attempt: 1,
            finished_at: Some(now),
            error: Some(format!(
                "missed: due {} and the node was not running (ended {})",
                show_time(due, s.tz.as_deref()),
                show_time(end.unwrap_or(due), s.tz.as_deref())
            )),
            ..Default::default()
        })?;
        record(
            store,
            "schedule.missed",
            format!("schedule {} missed its run and ended", s.name),
            json!({ "schedule": s.id, "name": s.name, "due": due }),
        );
        rep.missed.push(s.name.clone());
        s.enabled = false;
        store.save_schedule(s)?;
        rep.ended.push(s.name.clone());
        return Ok(());
    }
    let mut slots = vec![];
    let mut t = s.next_run_at;
    let mut guard = 0;
    loop {
        if t > now || guard > 1_000_000 {
            break;
        }
        if end.is_some_and(|e| t >= e) {
            break;
        }
        let take = if alive { t > last_tick } else { t > last_tick && end.is_none_or(|e| now < e) };
        if take {
            slots.push(t);
        } else if !alive && end.is_some_and(|e| now >= e) {
            break;
        }
        guard += 1;
        match step(s, t) {
            Some(n) => t = n,
            None => {
                t = i64::MAX;
                break;
            }
        }
    }
    let ended = end.is_some_and(|e| t >= e) || t == i64::MAX;
    let pick: Vec<i64> = match s.catch_up.as_str() {
        "all" => slots.iter().copied().take(CATCH_UP_MAX).collect(),
        "none" => {
            // skip missed: while alive, the crossed slot is on time; after a restart, queue nothing
            if alive { slots.last().copied().into_iter().collect() } else { vec![] }
        }
        _ => slots.last().copied().into_iter().collect(),
    };
    let open: Vec<ScheduleRun> = store.open_runs()?.into_iter().filter(|r| r.schedule_id == s.id).collect();
    for due in &pick {
        let has_open = !open.is_empty();
        if has_open && s.overlap == "skip" {
            break;
        }
        if has_open && open.iter().any(|r| r.status == "pending") {
            break;
        }
        store.insert_run(&ScheduleRun { schedule_id: s.id.clone(), status: "pending".into(), timestamp: *due, attempt: 1, ..Default::default() })?;
        rep.queued.push(s.name.clone());
        if has_open {
            break;
        }
    }
    if ended {
        s.enabled = false;
        if !rep.ended.iter().any(|n| n == &s.name) {
            rep.ended.push(s.name.clone());
            record(store, "schedule.end", format!("schedule {} ended", s.name), json!({ "schedule": s.id, "name": s.name }));
        }
    } else if s.enabled {
        s.next_run_at = t;
    }
    store.save_schedule(s)?;
    Ok(())
}

fn retry(store: &Store, s: &ScheduleDef, r: &ScheduleRun, now: i64, rep: &mut TickReport) -> Result<()> {
    if r.attempt >= s.max_attempts || r.error.as_deref().is_some_and(|e| e.starts_with("missed")) {
        return Ok(());
    }
    let not_before = now + backoff(r.attempt);
    if s.ends_at.is_some_and(|e| not_before >= e) && !s.is_once() {
        return Ok(());
    }
    let id = store.insert_run(&ScheduleRun {
        schedule_id: s.id.clone(),
        status: "pending".into(),
        timestamp: r.timestamp,
        attempt: r.attempt + 1,
        not_before,
        ..Default::default()
    })?;
    rep.retries.push(id);
    Ok(())
}

/// Queue and run one run of `s` now (`schedule run-now`), outside its timing.
pub fn run_now(store: &Store, s: &ScheduleDef, holder: &str, now: i64) -> Result<ScheduleRun> {
    let id =
        store.insert_run(&ScheduleRun { schedule_id: s.id.clone(), status: "pending".into(), timestamp: now, attempt: 1, ..Default::default() })?;
    if !store.claim_run(id, holder, now)? {
        return Err(Error::invalid("schedule", "the run was claimed by another scheduler".to_string()));
    }
    execute(store, s, id, holder, now)
}

/// Run a claimed run and record how it went.
fn execute(store: &Store, s: &ScheduleDef, run_id: i64, holder: &str, now: i64) -> Result<ScheduleRun> {
    let mut r = store.schedule_run(run_id)?.ok_or_else(|| Error::invalid("run", format!("#{run_id} is gone")))?;
    r.claimed_by = Some(holder.to_string());
    let base = json!({ "schedule": s.id, "name": s.name, "run": run_id, "due": r.timestamp, "attempt": r.attempt });
    let outcome: Result<(Option<i64>, i64, Vec<String>)> = match s.target_kind.as_str() {
        "ruleset" => (|| {
            let set_id = s.ruleset_id.clone().ok_or_else(|| Error::invalid("schedule", "its rule set was removed".to_string()))?;
            let set = store.ruleset(&set_id)?.ok_or_else(|| Error::invalid("schedule", "its rule set was removed".to_string()))?;
            if !set.enabled {
                return Err(Error::invalid("schedule", format!("rule set {} is disabled", set.name)));
            }
            let mut p = base.clone();
            p["ruleset"] = set.id.clone().into();
            let aid = store.record_action(
                &NewAction::new(crate::rules::SCHEDULE, ORIGIN, format!("schedule {} runs rule set {}", s.name, set.name)).params(p),
            )?;
            let a = store.action(aid)?.ok_or_else(|| Error::invalid("action", "gone".to_string()))?;
            let res = run_ruleset(store, &set.id, &a)?;
            if !res.failed.is_empty() {
                return Err(Error::invalid("rules", format!("failed: {}; {}", res.failed.join(", "), res.messages.join("; "))));
            }
            Ok((Some(aid), res.ran.len() as i64, res.messages))
        })(),
        "connector" => connector_target(store, s, &mut r, now),
        k => Err(Error::invalid("schedule", format!("unknown target {k}"))),
    };
    r.heartbeat_at = Some(now_ms().max(now));
    r.finished_at = Some(now_ms().max(now));
    match outcome {
        Ok((aid, ran, msgs)) => {
            r.status = "success".into();
            r.action_id = aid.or(r.action_id);
            r.ruleset_ran = r.ruleset_ran.max(ran);
            r.warnings.extend(msgs);
        }
        Err(e) => {
            r.status = "failed".into();
            r.error = Some(e.to_string());
            record(
                store,
                "schedule.fail",
                format!("schedule {} run #{run_id} failed: {e}", s.name),
                json!({ "schedule": s.id, "name": s.name, "run": run_id, "error": e.to_string() }),
            );
        }
    }
    store.update_run(&r)?;
    Ok(r)
}

/// Connector schedules (2.26): run the ingest-connector space with a live host context.
fn connector_target(store: &Store, s: &ScheduleDef, r: &mut ScheduleRun, _now: i64) -> Result<(Option<i64>, i64, Vec<String>)> {
    use crate::code::connector::{self, Live};
    use crate::store::SpaceKind;
    let plugin_id = s.plugin_id.clone().ok_or_else(|| Error::invalid("schedule", "its connector was removed".to_string()))?;
    let space = store.space(&plugin_id)?.ok_or_else(|| Error::invalid("schedule", "its connector was removed".to_string()))?;
    if space.kind != SpaceKind::IngestConnector {
        return Err(Error::invalid("schedule", format!("{} is not an ingest connector", space.name)));
    }
    if !space.enabled {
        return Err(Error::invalid("schedule", format!("connector {} is disabled", space.name)));
    }
    crate::code::spaces::ready_main(store, &space).map_err(|e| Error::invalid("schedule", e.to_string()))?;
    let source_id = s.source_id.clone().ok_or_else(|| Error::invalid("schedule", "its source was removed".to_string()))?;
    let source = store.source_by_id(&source_id)?.ok_or_else(|| Error::invalid("schedule", "its source was removed".to_string()))?;
    let last_success_at = store.schedule_runs(&s.id)?.into_iter().rev().find(|x| x.status == "success").map(|x| x.finished_at.unwrap_or(x.timestamp));
    let live = Live {
        schedule_id: s.id.clone(),
        schedule_name: s.name.clone(),
        run_id: r.id,
        plugin_id: plugin_id.clone(),
        source_id: source.id.clone(),
        source_name: source.name.clone(),
        pipeline_id: s.pipeline_id.clone(),
        ruleset_id: s.ruleset_id.clone(),
        options: s.options.clone(),
        last_success_at,
        stored: 0,
        bytes: 0,
        inserted: 0,
        piped: 0,
        ruleset_ran: 0,
        warnings: vec![],
        state: connector::load_state(store, &s.id)?,
    };
    let env = crate::code::CodeEnv::for_store(store)?;
    let (info, live) =
        connector::live_run(live, || crate::code::spaces::run_connector(store, &env, ORIGIN, &space, &format!("schedule:{}", s.name), &mut |_| {}));
    let info = info?;
    let _ = connector::save_state(store, &s.id, &live.state);
    r.stored = live.stored;
    r.bytes = live.bytes;
    r.inserted = live.inserted;
    r.piped = live.piped;
    r.ruleset_ran = live.ruleset_ran;
    r.warnings.extend(live.warnings);
    if info.status != "ok" {
        return Err(Error::invalid("connector", info.error.clone().unwrap_or_else(|| info.status.clone())));
    }
    // Q-I2: required pipe / run_ruleset must have been called
    let mut missing = vec![];
    if s.pipeline_id.is_some() && live.piped == 0 {
        missing.push("pipe");
    }
    if s.ruleset_id.is_some() && live.ruleset_ran == 0 {
        missing.push("run_ruleset");
    }
    if !missing.is_empty() {
        let msg = format!("plugin did not call {}", missing.join(" or "));
        // always record connector.run for history even when run_ruleset was skipped
        if live.ruleset_ran == 0 {
            let _ = store.record_action(
                &NewAction::new("connector.run", ORIGIN, format!("connector schedule {} run #{} ({msg})", s.name, r.id)).params(json!({
                    "schedule": s.id, "name": s.name, "run": r.id, "connector": plugin_id,
                    "stored": live.stored, "bytes": live.bytes, "inserted": live.inserted, "error": msg
                })),
            );
        }
        return Err(Error::invalid("connector", msg));
    }
    Ok((None, live.ruleset_ran, vec![]))
}

/// Enable or disable `s` (saved). Enabling picks the next slot from now (a run_every
/// alone runs right away); an ended schedule needs a new ends_at first.
pub fn set_enabled(store: &Store, s: &mut ScheduleDef, on: bool, now: i64) -> Result<()> {
    if on && !s.enabled {
        if s.ends_at.is_some_and(|e| e <= now) {
            return Err(Error::invalid(
                "schedule",
                format!(
                    "{} ended {}; give a new end first (schedule edit {} --ends-at ... or --no-end)",
                    s.name,
                    show_time(s.ends_at.unwrap_or(now), s.tz.as_deref()),
                    s.name
                ),
            ));
        }
        s.next_run_at = first_due(s, now).ok_or_else(|| Error::invalid("schedule", format!("{} has no run left; give a new --start-at", s.name)))?;
    }
    s.enabled = on;
    *s = store.save_schedule(s)?;
    Ok(())
}

/// This process as a scheduler (`cli:1234`, `tui:1234`, `host:1234`).
pub fn holder(kind: &str) -> String {
    format!("{kind}:{}", std::process::id())
}

/// A rule's old-style `--every` / `--cron`, as a rule set (`set_name`, made when missing)
/// holding the rule plus a schedule running it (the node's zone).
pub fn schedule_rule(
    store: &Store,
    r: &crate::store::Rule,
    sched: &crate::store::Schedule,
    set_name: &str,
) -> Result<(crate::store::Ruleset, ScheduleDef)> {
    let set = match store.rulesets()?.into_iter().find(|x| x.name.eq_ignore_ascii_case(set_name)) {
        Some(x) => x,
        None => store.create_ruleset(set_name, &format!("rule {} on a schedule", r.name), "standalone", None)?,
    };
    if !store.ruleset_rules(&set.id)?.iter().any(|x| x.id == r.id) {
        store.add_ruleset_rule(&set.id, &r.id, None)?;
    }
    let existing = store.schedules_using("ruleset_id", &set.id)?.into_iter().find(|x| x.target_kind == "ruleset");
    let mut s = existing.unwrap_or_else(|| {
        let mut name = r.name.clone();
        let mut i = 2;
        let taken = store.schedules().unwrap_or_default();
        while taken.iter().any(|x| x.name.eq_ignore_ascii_case(&name)) {
            name = format!("{} {i}", r.name);
            i += 1;
        }
        ScheduleDef {
            name,
            target_kind: "ruleset".into(),
            ruleset_id: Some(set.id.clone()),
            tz: local_zone(),
            overlap: "skip".into(),
            catch_up: "once".into(),
            max_attempts: 3,
            enabled: true,
            ..Default::default()
        }
    });
    s.ruleset_id = Some(set.id.clone());
    s.target_kind = "ruleset".into();
    s.enabled = true;
    if s.tz.is_none() {
        s.tz = local_zone();
    }
    let now = now_ms();
    let timing = match sched {
        crate::store::Schedule::Every(ms) => {
            Timing { run_every: Some(long_form(0, *ms)), past_next: s.start_at.is_some_and(|x| x < now), ..Default::default() }
        }
        crate::store::Schedule::Cron(c) => Timing { cron: Some(c.clone()), past_next: s.start_at.is_some_and(|x| x < now), ..Default::default() },
    };
    apply_timing(&mut s, &timing, now)?;
    let s = store.save_schedule(&s)?;
    Ok((set, s))
}

/// Remove schedules that exist only to run this rule (edit --no-schedule).
pub fn unschedule_rule(store: &Store, r: &crate::store::Rule) -> Result<Vec<String>> {
    let mut removed = vec![];
    for set in store.rulesets_of_rule(&r.id)? {
        let alone = store.ruleset_rules(&set.id)?.len() <= 1;
        let ours = set.name.eq_ignore_ascii_case(&format!("{} (scheduled)", r.name)) || set.name == crate::rules::CERT_WATCH;
        if !(alone || ours) {
            continue;
        }
        for s in store.schedules_using("ruleset_id", &set.id)? {
            store.remove_schedule(&s.id)?;
            removed.push(s.name);
        }
    }
    Ok(removed)
}

/// `scheduler status`: the lease and what is open or next.
pub fn status(store: &Store, now: i64) -> Result<Value> {
    let lease = store.lease()?.filter(|(_, until)| *until > now);
    let next = store.schedules()?.into_iter().filter(|s| s.enabled).min_by_key(|s| s.next_run_at);
    Ok(json!({
        "lease": lease.map(|(h, u)| json!({ "holder": h, "until": u })),
        "open": store.open_runs()?.len(),
        "next": next.map(|s| json!({ "schedule": s.name, "at": s.next_run_at })),
    }))
}

#[cfg(test)]
#[path = "schedule_tests.rs"]
mod tests;
