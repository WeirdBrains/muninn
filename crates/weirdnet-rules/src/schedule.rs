//! Schedules: `every 6h` and five-field cron, their text forms, and when a rule runs next.
use crate::error::{Result, RulesError};
use crate::model::{RuleSpec, Schedule};
use crate::syntax::parse_on;

/// `every 6h`: a number and s, m, h, d or w (no unit: hours; at least a minute).
pub fn parse_every(s: &str) -> Result<Schedule> {
    let t = s.trim().to_ascii_lowercase();
    let split = t.find(|c: char| !c.is_ascii_digit()).unwrap_or(t.len());
    let (n, u) = t.split_at(split);
    let n: i64 = n.parse().map_err(|_| RulesError::invalid("--every", format!("`{s}`: like 30m, 6h, 1d")))?;
    let unit = match u.trim() {
        "s" | "sec" | "secs" => 1_000,
        "m" | "min" | "mins" => 60_000,
        "h" | "hr" | "hrs" | "" => 3_600_000,
        "d" | "day" | "days" => 86_400_000,
        "w" | "week" | "weeks" => 7 * 86_400_000,
        _ => return Err(RulesError::invalid("--every", format!("`{s}`: use s, m, h, d or w"))),
    };
    let ms = n.saturating_mul(unit);
    if ms < 60_000 {
        return Err(RulesError::invalid("--every", "at least a minute"));
    }
    Ok(Schedule::Every(ms))
}

/// A five-field cron expression (minute hour day-of-month month day-of-week; `*`, lists,
/// ranges and `/steps`; Sunday is 0 or 7). When both day fields are restricted either may
/// match, as in classic cron.
#[derive(Debug, Clone, PartialEq)]
pub struct Cron {
    min: Vec<bool>,
    hour: Vec<bool>,
    dom: Vec<bool>,
    month: Vec<bool>,
    dow: Vec<bool>,
    dom_any: bool,
    dow_any: bool,
}

fn cron_field(f: &str, lo: u32, hi: u32, what: &str) -> Result<Vec<bool>> {
    let bad = || RulesError::invalid("--cron", format!("`{f}` is not a valid {what} ({lo}-{hi}, *, a-b, */n, lists)"));
    let mut v = vec![false; hi as usize + 1];
    for part in f.split(',') {
        let (range, step) = match part.split_once('/') {
            Some((r, s)) => (r, s.parse::<u32>().ok().filter(|s| *s > 0).ok_or_else(bad)?),
            None => (part, 1),
        };
        let (a, b) = if range == "*" {
            (lo, hi)
        } else if let Some((a, b)) = range.split_once('-') {
            (a.parse().map_err(|_| bad())?, b.parse().map_err(|_| bad())?)
        } else {
            let a: u32 = range.parse().map_err(|_| bad())?;
            (a, if part.contains('/') { hi } else { a })
        };
        if a < lo || b > hi || a > b {
            return Err(bad());
        }
        let mut i = a;
        while i <= b {
            v[i as usize] = true;
            i += step;
        }
    }
    Ok(v)
}

pub fn parse_cron(s: &str) -> Result<Cron> {
    let f: Vec<&str> = s.split_whitespace().collect();
    if f.len() != 5 {
        return Err(RulesError::invalid("--cron", format!("`{s}`: five fields, minute hour day month weekday (like \"0 3 * * *\")")));
    }
    let mut dow = cron_field(f[4], 0, 7, "weekday")?;
    if dow[7] {
        dow[0] = true;
    }
    Ok(Cron {
        min: cron_field(f[0], 0, 59, "minute")?,
        hour: cron_field(f[1], 0, 23, "hour")?,
        dom: cron_field(f[2], 1, 31, "day")?,
        month: cron_field(f[3], 1, 12, "month")?,
        dow,
        dom_any: f[2] == "*",
        dow_any: f[4] == "*",
    })
}

impl Cron {
    /// The first matching minute after `after` (epoch ms) in local time, within about four years.
    pub fn next_after(&self, after: i64) -> Option<i64> {
        self.next_after_in(after, &chrono::Local)
    }

    /// [`Cron::next_after`] evaluated in zone `z` (UTC, a fixed offset, or local).
    ///
    /// Times are stepped as wall-clock times in `z` and only then turned into instants, so
    /// daylight-saving changes neither loop nor skip days: a time that does not exist
    /// (the hour clocks jump over) does not fire that day, and one that happens twice (the
    /// hour clocks repeat) fires once, at its first occurrence.
    pub fn next_after_in<Z: chrono::TimeZone>(&self, after: i64, z: &Z) -> Option<i64> {
        use chrono::{Datelike, Duration, NaiveTime, Timelike};
        let start = z.timestamp_millis_opt(after).earliest()?.naive_local();
        let mut t = start.with_second(0)?.with_nanosecond(0)? + Duration::minutes(1);
        let end = start + Duration::days(366 * 4);
        let next_day = |t: chrono::NaiveDateTime| t.date().succ_opt().map(|d| d.and_time(NaiveTime::MIN));
        while t < end {
            if !self.month[t.month() as usize] {
                t = next_day(t)?;
                continue;
            }
            let dom = self.dom[t.day() as usize];
            let dow = self.dow[t.weekday().num_days_from_sunday() as usize];
            let day_ok = match (self.dom_any, self.dow_any) {
                (true, true) => true,
                (false, true) => dom,
                (true, false) => dow,
                (false, false) => dom || dow,
            };
            if !day_ok {
                t = next_day(t)?;
                continue;
            }
            if !self.hour[t.hour() as usize] {
                t = t.with_minute(0)? + Duration::hours(1);
                continue;
            }
            if self.min[t.minute() as usize] {
                let at = z.from_local_datetime(&t);
                // the first occurrence still ahead (a repeated time already passed once is not run again)
                if let Some(ms) = at.earliest().map(|d| d.timestamp_millis()).filter(|ms| *ms > after) {
                    return Some(ms);
                }
            }
            t += Duration::minutes(1);
        }
        None
    }
}

/// `--every` and `--cron` (at most one) into a schedule; cron is stored with single spaces.
pub fn parse_schedule(every: Option<&str>, cron: Option<&str>) -> Result<Option<Schedule>> {
    match (every, cron) {
        (Some(_), Some(_)) => Err(RulesError::invalid("schedule", "give --every or --cron, not both")),
        (Some(e), None) => parse_every(e).map(Some),
        (None, Some(c)) => {
            parse_cron(c)?;
            Ok(Some(Schedule::Cron(c.split_whitespace().collect::<Vec<_>>().join(" "))))
        }
        (None, None) => Ok(None),
    }
}

/// `every 6h`, `cron "0 3 * * *"`.
pub fn schedule_text(s: &Schedule) -> String {
    match s {
        Schedule::Every(ms) => {
            let ms = *ms;
            let (n, u) = [(7 * 86_400_000, "w"), (86_400_000, "d"), (3_600_000, "h"), (60_000, "m"), (1_000, "s")]
                .iter()
                .find(|(u, _)| ms % u == 0)
                .map(|(u, n)| (ms / u, *n))
                .unwrap_or((ms, "ms"));
            format!("every {n}{u}")
        }
        Schedule::Cron(c) => format!("cron \"{c}\""),
    }
}

/// When a schedule runs next (`None`: a cron that never fires, or does not parse), in local
/// time. Every: an interval after the last run, at once (`created_at`) if it never ran.
/// Cron: the first time after the last run (or `created_at`).
pub fn next_run(s: &Schedule, created_at: i64, last_run_at: Option<i64>) -> Option<i64> {
    next_run_in(s, created_at, last_run_at, &chrono::Local)
}

/// [`next_run`] with cron times evaluated in zone `z`.
pub fn next_run_in<Z: chrono::TimeZone>(s: &Schedule, created_at: i64, last_run_at: Option<i64>, z: &Z) -> Option<i64> {
    match s {
        Schedule::Every(ms) => Some(last_run_at.map_or(created_at, |l| l + ms)),
        Schedule::Cron(c) => parse_cron(c).ok()?.next_after_in(last_run_at.unwrap_or(created_at), z),
    }
}

/// One-line "On" text: event kinds, or a schedule (`every 6h`, `cron "0 3 * * *"`)
/// optionally followed by `; KINDS`. Kinds are checked against `kinds` (see [`parse_on`]).
pub fn parse_on_text(s: &str, kinds: &[&str], hint: &str) -> Result<(Vec<String>, Option<Schedule>)> {
    let t = s.trim();
    let low = t.to_ascii_lowercase();
    if !(low.starts_with("every ") || low.starts_with("cron ")) {
        return Ok((parse_on(&[t.to_string()], kinds, hint)?, None));
    }
    let (sched, rest) = t.split_once(';').unwrap_or((t, ""));
    let (word, arg) = sched.trim().split_once(' ').unwrap_or((sched, ""));
    let arg = arg.trim().trim_matches('"');
    let schedule = if word.eq_ignore_ascii_case("every") { parse_schedule(Some(arg), None)? } else { parse_schedule(None, Some(arg))? };
    let on = if rest.trim().is_empty() { vec![] } else { parse_on(&[rest.to_string()], kinds, hint)? };
    Ok((on, schedule))
}

/// A spec's "On" in the form [`parse_on_text`] reads.
pub fn on_text<T>(spec: &RuleSpec<T>) -> String {
    match &spec.schedule {
        Some(s) if spec.on.is_empty() => schedule_text(s),
        Some(s) => format!("{}; {}", schedule_text(s), spec.on.join(", ")),
        None => spec.on.join(", "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, TimeZone, Timelike, Utc};

    const H: i64 = 3_600_000;

    fn utc(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> i64 {
        Utc.with_ymd_and_hms(y, mo, d, h, mi, 0).unwrap().timestamp_millis()
    }

    #[test]
    fn every_parse_and_print() {
        assert_eq!(parse_every("6h").unwrap(), Schedule::Every(6 * H));
        assert_eq!(parse_every("30m").unwrap(), Schedule::Every(1_800_000));
        assert_eq!(parse_every(" 2 ").unwrap(), Schedule::Every(2 * H), "no unit: hours");
        assert_eq!(parse_every("60s").unwrap(), Schedule::Every(60_000));
        assert_eq!(schedule_text(&parse_every("1d").unwrap()), "every 1d");
        assert_eq!(schedule_text(&parse_every("90m").unwrap()), "every 90m");
        assert_eq!(schedule_text(&parse_every("2w").unwrap()), "every 2w");
        assert_eq!(schedule_text(&Schedule::Every(61_500)), "every 61500ms");
        assert_eq!(parse_every("10s").unwrap_err().to_string(), "invalid --every: at least a minute");
        assert_eq!(parse_every("6x").unwrap_err().to_string(), "invalid --every: `6x`: use s, m, h, d or w");
        assert_eq!(parse_every("h").unwrap_err().to_string(), "invalid --every: `h`: like 30m, 6h, 1d");
        assert!(parse_every("99999999999999999999h").is_err());
        assert_eq!(parse_every("9999999999999w").unwrap(), Schedule::Every(i64::MAX), "saturates");
    }

    #[test]
    fn cron_parse_and_errors() {
        assert!(parse_cron("0 3 * * *").is_ok() && parse_cron("*/15 9-17 * * 1-5").is_ok() && parse_cron("0,30 * 1 1 0").is_ok());
        assert!(parse_cron("0 3 * *").is_err() && parse_cron("60 * * * *").is_err() && parse_cron("* 25 * * *").is_err());
        assert!(parse_cron("* * 0 * *").is_err() && parse_cron("* * * 13 *").is_err() && parse_cron("* * * * 8").is_err());
        assert!(parse_cron("*/0 * * * *").is_err() && parse_cron("5-1 * * * *").is_err());
        assert_eq!(parse_cron("x * * * *").unwrap_err().to_string(), "invalid --cron: `x` is not a valid minute (0-59, *, a-b, */n, lists)");
        assert_eq!(parse_schedule(Some("1h"), Some("0 3 * * *")).unwrap_err().to_string(), "invalid schedule: give --every or --cron, not both");
        assert_eq!(parse_schedule(None, Some(" 0  3 * *   * ")).unwrap(), Some(Schedule::Cron("0 3 * * *".into())));
        assert_eq!(parse_schedule(None, None).unwrap(), None);
    }

    #[test]
    fn cron_steps_in_fixed_zones() {
        // Sunday 1 Nov 2026 10:07 UTC
        let at = utc(2026, 11, 1, 10, 7);
        let c = parse_cron("0 3 * * *").unwrap();
        assert_eq!(c.next_after_in(at, &Utc), Some(utc(2026, 11, 2, 3, 0)));
        // the same instant is 05:07 at UTC-5, so 03:00 there is 08:00 UTC the next day
        let est = FixedOffset::west_opt(5 * 3600).unwrap();
        assert_eq!(c.next_after_in(at, &est), Some(utc(2026, 11, 2, 8, 0)));
        // and at UTC+14 it is already 2 Nov 00:07, so 03:00 that day is 13:00 UTC on the 1st
        let kiri = FixedOffset::east_opt(14 * 3600).unwrap();
        assert_eq!(c.next_after_in(at, &kiri), Some(utc(2026, 11, 1, 13, 0)));
        assert_eq!(parse_cron("*/15 * * * *").unwrap().next_after_in(at, &Utc), Some(utc(2026, 11, 1, 10, 15)));
        // strictly after: a run exactly on a matching minute goes to the next one
        assert_eq!(parse_cron("15 10 * * *").unwrap().next_after_in(utc(2026, 11, 1, 10, 15), &Utc), Some(utc(2026, 11, 2, 10, 15)));
        // weekdays: Monday 09:00; Sunday as 7
        assert_eq!(parse_cron("0 9 * * 1").unwrap().next_after_in(at, &Utc), Some(utc(2026, 11, 2, 9, 0)));
        assert_eq!(parse_cron("0 12 * * 7").unwrap().next_after_in(at, &Utc), Some(utc(2026, 11, 1, 12, 0)));
        // day of month and weekday both given: either (the 15th, or a Friday)
        assert_eq!(parse_cron("0 0 15 * 5").unwrap().next_after_in(at, &Utc), Some(utc(2026, 11, 6, 0, 0)));
        // year end rolls over; leap day waits for 2028
        assert_eq!(parse_cron("0 0 1 1 *").unwrap().next_after_in(at, &Utc), Some(utc(2027, 1, 1, 0, 0)));
        assert_eq!(parse_cron("0 0 29 2 *").unwrap().next_after_in(at, &Utc), Some(utc(2028, 2, 29, 0, 0)));
        // never: 31 February
        assert_eq!(parse_cron("0 0 31 2 *").unwrap().next_after_in(at, &Utc), None);
        // the local-time entry point agrees with itself in local time
        let n = c.next_after(at).unwrap();
        let t = chrono::Local.timestamp_millis_opt(n).unwrap();
        assert_eq!((t.hour(), t.minute()), (3, 0));
        assert!(n > at && n - at <= 25 * H);
    }

    /// Wall-clock stepping across daylight saving (America/New_York: clocks went back at
    /// 02:00 EDT on 1 Nov 2026 and go forward at 02:00 EST on 14 Mar 2027). Stepping by
    /// instants used to loop forever on the day clocks went back.
    #[test]
    fn cron_across_daylight_saving() {
        let ny = chrono_tz::America::New_York;
        let at = utc(2026, 10, 25, 12, 0); // 08:00 EDT
        // day skipping over the fall-back day
        assert_eq!(parse_cron("0 0 1 1 *").unwrap().next_after_in(at, &ny), Some(utc(2027, 1, 1, 5, 0)));
        assert_eq!(parse_cron("0 9 * * 1").unwrap().next_after_in(at, &ny), Some(utc(2026, 10, 26, 13, 0)));
        assert_eq!(parse_cron("0 9 * * 1").unwrap().next_after_in(utc(2026, 10, 27, 0, 0), &ny), Some(utc(2026, 11, 2, 14, 0)), "09:00 EST");
        assert_eq!(parse_cron("0 0 29 2 *").unwrap().next_after_in(at, &ny), Some(utc(2028, 2, 29, 5, 0)));
        assert_eq!(parse_cron("0 0 31 2 *").unwrap().next_after_in(at, &ny), None);
        // the repeated hour: 01:30 happens at 05:30 and 06:30 UTC; it runs once, then tomorrow
        let c = parse_cron("30 1 * * *").unwrap();
        let first = c.next_after_in(utc(2026, 11, 1, 4, 0), &ny).unwrap();
        assert_eq!(first, utc(2026, 11, 1, 5, 30));
        assert_eq!(c.next_after_in(first, &ny), Some(utc(2026, 11, 2, 6, 30)));
        assert_eq!(c.next_after_in(utc(2026, 11, 1, 6, 0), &ny), Some(utc(2026, 11, 2, 6, 30)), "inside the repeat: not again");
        // hourly through the repeated hour: 01:00 EDT, then 02:00 EST (the second 01:00 is not repeated)
        let h = parse_cron("0 * * * *").unwrap();
        assert_eq!(h.next_after_in(utc(2026, 11, 1, 4, 30), &ny), Some(utc(2026, 11, 1, 5, 0)));
        assert_eq!(h.next_after_in(utc(2026, 11, 1, 5, 0), &ny), Some(utc(2026, 11, 1, 7, 0)));
        // every 15 minutes keeps going through the second 01:xx
        let q = parse_cron("*/15 * * * *").unwrap();
        assert_eq!(q.next_after_in(utc(2026, 11, 1, 6, 20), &ny), Some(utc(2026, 11, 1, 7, 0)), "no run in the repeat after its first pass");
        // the skipped hour: 02:30 does not exist on 14 Mar 2027, so that day has no run
        let s = parse_cron("30 2 * * *").unwrap();
        assert_eq!(s.next_after_in(utc(2027, 3, 13, 12, 0), &ny), Some(utc(2027, 3, 15, 6, 30)));
        assert_eq!(parse_cron("0 3 * * *").unwrap().next_after_in(utc(2027, 3, 14, 6, 0), &ny), Some(utc(2027, 3, 14, 7, 0)), "03:00 EDT");
    }

    #[test]
    fn next_run_with_a_fake_clock() {
        let created = utc(2026, 11, 1, 10, 7);
        let every = Schedule::Every(6 * H);
        assert_eq!(next_run(&every, created, None), Some(created), "never ran: due at once");
        assert_eq!(next_run(&every, created, Some(created + H)), Some(created + 7 * H));
        let cron = Schedule::Cron("30 * * * *".into());
        assert_eq!(next_run_in(&cron, created, None, &Utc), Some(utc(2026, 11, 1, 10, 30)));
        assert_eq!(next_run_in(&cron, created, Some(utc(2026, 11, 1, 10, 30)), &Utc), Some(utc(2026, 11, 1, 11, 30)));
        assert_eq!(next_run_in(&Schedule::Cron("bad".into()), created, None, &Utc), None);
    }

    #[test]
    fn on_text_round_trips() {
        let kinds = &["import", "schedule"];
        let (on, s) = parse_on_text("every 6h; import", kinds, "").unwrap();
        assert_eq!((on, s), (vec!["import".to_string()], Some(Schedule::Every(6 * H))));
        let spec: RuleSpec<()> = RuleSpec { schedule: Some(Schedule::Cron("0 3 * * *".into())), ..Default::default() };
        assert_eq!(on_text(&spec), "cron \"0 3 * * *\"");
        assert_eq!(parse_on_text(&on_text(&spec), kinds, "").unwrap().1, spec.schedule);
        let spec: RuleSpec<()> = RuleSpec { on: vec!["import".into()], schedule: Some(Schedule::Every(H)), ..Default::default() };
        assert_eq!(on_text(&spec), "every 1h; import");
        assert_eq!(parse_on_text(&on_text(&spec), kinds, "").unwrap(), (spec.on.clone(), spec.schedule.clone()));
        assert_eq!(parse_on_text("import", kinds, "").unwrap(), (vec!["import".to_string()], None));
        assert!(parse_on_text("every 6h; nope", kinds, "").is_err());
    }
}
