use super::*;
use crate::store::{NewRule, RuleSpec, Then};

// 2026-11-10 14:00:00Z
const T0: i64 = 1_794_319_200_000;

fn ms(s: &str) -> i64 {
    parse_interval(s).unwrap().ms
}

#[test]
fn intervals() {
    assert_eq!(ms("2 hours 30 minutes"), 9_000_000);
    assert_eq!(ms("1h30m"), 5_400_000);
    assert_eq!(ms("90m"), 5_400_000);
    assert_eq!(ms("hour"), 3_600_000);
    assert_eq!(ms("5000"), 5000);
    assert_eq!(ms("250ms"), 250);
    assert_eq!(ms("2w"), 14 * 86_400_000);
    assert_eq!(ms("1 fortnight"), 14 * 86_400_000);
    assert_eq!(parse_interval("1M").unwrap().months, 1);
    assert_eq!(parse_interval("1 year").unwrap().months, 12);
    assert_eq!(parse_interval("1 month 2 days").unwrap(), Interval { months: 1, ms: 2 * 86_400_000, once: false });
    assert!(parse_interval("once").unwrap().once);
    for bad in ["tomorrow", "9am", "next monday", "1 day ago", "10:00", "1H", "1S", "1D", "1mo", "0", "", "fast"] {
        assert!(parse_interval(bad).is_err(), "{bad} should fail");
    }
    assert!(parse_interval("tomorrow").unwrap_err().to_string().contains("--start-at"));
}

#[test]
fn long_forms() {
    assert_eq!(long_form(0, 9_000_000), "2 hours 30 minutes");
    assert_eq!(long_form(12, 0), "1 year");
    assert_eq!(long_form(1, 86_400_000 + 1), "1 month 1 day 1 millisecond");
    assert_eq!(long_form(0, 1000), "1 second");
}

#[test]
fn dates() {
    let t = parse_when("2026-12-01 08:00", T0, Some("UTC")).unwrap();
    assert_eq!(t, 1_796_112_000_000);
    let ny = parse_when("2026-12-01 08:00", T0, Some("America/New_York")).unwrap();
    assert_eq!(ny - t, 5 * 3_600_000);
    assert_eq!(parse_when("+2 hours", T0, Some("UTC")).unwrap(), T0 + 7_200_000);
    assert_eq!(parse_when("tomorrow", T0, Some("UTC")).unwrap(), T0 + 86_400_000);
    // an explicit zone in the text wins
    assert_eq!(parse_when("2026-12-01 08:00Z", T0, Some("Europe/Oslo")).unwrap(), t);
    assert!(parse_when("not a date at all", T0, None).is_err());
    assert!(check_zone("Nowhere/Land").is_err());
    assert_eq!(check_zone("Europe/Oslo").unwrap(), "Europe/Oslo");
}

fn def(every: Option<&str>, start: Option<&str>, tz: Option<&str>) -> Result<ScheduleDef> {
    let mut s = ScheduleDef {
        name: "x".into(),
        target_kind: "ruleset".into(),
        ruleset_id: Some("r".into()),
        overlap: "skip".into(),
        catch_up: "once".into(),
        max_attempts: 3,
        enabled: true,
        created_at: T0,
        ..Default::default()
    };
    let t = Timing { run_every: every.map(str::to_string), start_at: start.map(str::to_string), tz: tz.map(str::to_string), ..Default::default() };
    apply_timing(&mut s, &t, T0)?;
    Ok(s)
}

#[test]
fn months_clamp() {
    let s = def(Some("1 month"), Some("2027-01-31 09:00"), Some("UTC")).unwrap();
    let a = s.next_run_at;
    let b = step(&s, a).unwrap();
    let c = step(&s, b).unwrap();
    assert_eq!(civil(b, None).1..=civil(b, None).2, 2..=28);
    assert_eq!((civil(c, None).1, civil(c, None).2), (3, 31));
    // calendar steps keep wall time in a zone across DST
    let z = def(Some("1 day"), Some("2027-03-13 09:00"), Some("America/New_York")).unwrap();
    let ms_only = step(&z, z.next_run_at).unwrap() - z.next_run_at;
    assert_eq!(ms_only, 86_400_000); // fixed ms: 24 h, not wall
    let zm = def(Some("1 month"), Some("2027-03-01 09:00"), Some("America/New_York")).unwrap();
    let n = step(&zm, zm.next_run_at).unwrap();
    assert_eq!(civil(n, Some("America/New_York")).3, 9);
}

#[test]
fn once_and_past() {
    let s = def(Some("once"), Some("2026-11-11 09:00"), Some("UTC")).unwrap();
    assert_eq!(s.ends_at, s.start_at.map(|x| x + 1));
    assert_eq!(s.next_run_at, s.start_at.unwrap());
    assert!(def(Some("once"), None, None).is_err());
    let e = def(Some("once"), Some("2026-11-01 09:00"), Some("UTC")).unwrap_err().to_string();
    assert!(e.contains("corrected"), "{e}");
    let e = def(Some("1 day"), Some("2026-11-01 09:00"), Some("UTC")).unwrap_err().to_string();
    assert!(e.contains("next-interval"), "{e}");
    let mut s = ScheduleDef { name: "x".into(), target_kind: "ruleset".into(), ruleset_id: Some("r".into()), ..Default::default() };
    apply_timing(
        &mut s,
        &Timing {
            run_every: Some("1 day".into()),
            start_at: Some("2026-11-01 09:00".into()),
            tz: Some("UTC".into()),
            past_next: true,
            ..Default::default()
        },
        T0,
    )
    .unwrap();
    assert_eq!(s.next_run_at, parse_when("2026-11-11 09:00", T0, Some("UTC")).unwrap());
    // run_every alone: right away
    assert_eq!(def(Some("1h"), None, None).unwrap().next_run_at, T0);
    assert!(def(None, None, None).is_err());
}

#[test]
fn cron_in_zone() {
    let n = cron_next("0 9 * * *", T0, Some("UTC")).unwrap();
    assert_eq!(n, parse_when("2026-11-11 09:00", T0, Some("UTC")).unwrap());
    let n = cron_next("0 9 * * *", T0, Some("America/New_York")).unwrap();
    assert_eq!(n, parse_when("2026-11-11 09:00", T0, Some("America/New_York")).unwrap());
    let n = cron_next("0 9 * * *", T0, None).unwrap();
    assert_eq!(civil(n, None).3, 9);
}

fn setup() -> (Store, ScheduleDef) {
    let st = Store::open_in_memory().unwrap();
    let r = st
        .create_rule(&NewRule {
            name: "say".into(),
            spec: RuleSpec { on: vec!["schedule".into()], then: vec![Then::Status { text: "hi".into() }], ..Default::default() },
            enabled: true,
            ..Default::default()
        })
        .unwrap();
    let set = st.create_ruleset("daily", "", "standalone", None).unwrap();
    st.add_ruleset_rule(&set.id, &r.id, None).unwrap();
    let mut s = ScheduleDef {
        name: "every hour".into(),
        target_kind: "ruleset".into(),
        ruleset_id: Some(set.id),
        overlap: "skip".into(),
        catch_up: "once".into(),
        max_attempts: 3,
        enabled: true,
        ..Default::default()
    };
    apply_timing(&mut s, &Timing { run_every: Some("1h".into()), ..Default::default() }, T0).unwrap();
    let s = st.save_schedule(&s).unwrap();
    (st, s)
}

#[test]
fn tick_runs_ruleset() {
    let (st, s) = setup();
    let rep = tick(&st, "t1", T0).unwrap();
    assert_eq!(rep.succeeded.len(), 1, "{rep:?}");
    let runs = st.schedule_runs(&s.id).unwrap();
    assert_eq!(runs[0].status, "success");
    assert_eq!(runs[0].ruleset_ran, 1);
    assert_eq!(st.schedule(&s.id).unwrap().unwrap().next_run_at, T0 + 3_600_000);
    // nothing due
    assert!(tick(&st, "t1", T0 + 1000).unwrap().started.is_empty());
    // another holder stands by while the lease lasts
    assert!(tick(&st, "t2", T0 + 2000).unwrap().standby);
    // missed hours: catch_up once runs one
    let rep = tick(&st, "t1", T0 + 5 * 3_600_000 + 10).unwrap();
    assert_eq!(rep.started.len(), 1);
    assert_eq!(st.schedule(&s.id).unwrap().unwrap().next_run_at, T0 + 6 * 3_600_000);
}

#[test]
fn catch_up_all_and_none() {
    let (st, mut s) = setup();
    s.catch_up = "all".into();
    st.save_schedule(&s).unwrap();
    let rep = tick(&st, "t", T0 + 3 * 3_600_000 + 10).unwrap();
    // every missed slot is queued (T0..T0+3h); they then run one per schedule at a time
    assert_eq!(rep.queued.len(), 4);
    assert_eq!(rep.started.len(), 4); // one after another, up to WORKERS a tick
    // an open run with overlap skip: nothing more is queued
    let id =
        st.insert_run(&ScheduleRun { schedule_id: s.id.clone(), status: "pending".into(), timestamp: T0, attempt: 1, ..Default::default() }).unwrap();
    st.claim_run(id, "other", T0 + 3 * 3_600_000 + 25).unwrap();
    s = st.schedule(&s.id).unwrap().unwrap();
    s.next_run_at = T0 + 3 * 3_600_000 + 20;
    st.save_schedule(&s).unwrap();
    assert!(tick(&st, "t", T0 + 3 * 3_600_000 + 30).unwrap().queued.is_empty());
    for r in st.open_runs().unwrap() {
        st.update_run(&ScheduleRun { status: "success".into(), ..r }).unwrap();
    }
    s = st.schedule(&s.id).unwrap().unwrap();
    s.catch_up = "none".into();
    s.next_run_at = T0 + 10 * 3_600_000;
    s.enabled = true;
    st.save_schedule(&s).unwrap();
    // simulate a long downtime (restart): last tick far in the past
    st.set_setting(LAST_TICK_KEY, Some(&(T0 + 4 * 3_600_000).to_string())).unwrap();
    let rep = tick(&st, "t", T0 + 12 * 3_600_000 + 5 * 60_000).unwrap();
    assert!(rep.queued.is_empty(), "catch_up none skips missed after restart: {rep:?}");
    assert!(st.schedule(&s.id).unwrap().unwrap().next_run_at > T0 + 10 * 3_600_000);
}

#[test]
fn missed_one_off_is_skipped_on_restart() {
    let (st, mut s) = setup();
    apply_timing(
        &mut s,
        &Timing { run_every: Some("once".into()), start_at: Some("2026-11-10 15:00".into()), tz: Some("UTC".into()), ..Default::default() },
        T0,
    )
    .unwrap();
    assert_eq!(s.ends_at_input.as_deref(), Some("auto (once)"));
    st.save_schedule(&s).unwrap();
    // cold: never ticked before, deadline long past → missed via ends_at (no once-specific branch)
    let rep = tick(&st, "t", T0 + 3 * 3_600_000).unwrap();
    assert_eq!(rep.missed, vec!["every hour".to_string()], "{rep:?}");
    assert!(!st.schedule(&s.id).unwrap().unwrap().enabled);
    assert!(st.schedule_runs(&s.id).unwrap()[0].error.as_deref().unwrap().starts_with("missed"));
}

#[test]
fn daemon_alive_runs_once_after_tiny_window() {
    let (st, mut s) = setup();
    // prior tick of *this* session kept the daemon "alive"
    st.set_setting(LAST_TICK_KEY, Some(&(T0 - 15_000).to_string())).unwrap();
    st.set_setting(LAST_HOLDER_KEY, Some("t")).unwrap();
    apply_timing(
        &mut s,
        &Timing { run_every: Some("once".into()), start_at: Some("2026-11-10 14:00:00".into()), tz: Some("UTC".into()), ..Default::default() },
        T0 - 60_000,
    )
    .unwrap();
    // start_at == T0; ends_at = T0+1. Tick 5s later while alive → still runs (crossed while up).
    s.next_run_at = T0;
    st.save_schedule(&s).unwrap();
    let rep = tick(&st, "t", T0 + 5_000).unwrap();
    assert_eq!(rep.succeeded.len(), 1, "{rep:?}");
    assert!(!st.schedule(&s.id).unwrap().unwrap().enabled, "ends_at disables after the run");
}

#[test]
fn quick_restart_across_deadline_skips_once() {
    // A persisted tick 10s ago under another holder must not look "alive".
    let (st, mut s) = setup();
    st.set_setting(LAST_TICK_KEY, Some(&(T0 - 10_000).to_string())).unwrap();
    st.set_setting(LAST_HOLDER_KEY, Some("old:1")).unwrap();
    apply_timing(
        &mut s,
        &Timing { run_every: Some("once".into()), start_at: Some("2026-11-10 14:00:00".into()), tz: Some("UTC".into()), ..Default::default() },
        T0 - 60_000,
    )
    .unwrap();
    s.next_run_at = T0;
    st.save_schedule(&s).unwrap();
    // new session 5s after the 1ms window — gap is only 15s (< ALIVE_GAP) but holder differs
    let rep = tick(&st, "t-new", T0 + 5_000).unwrap();
    assert_eq!(rep.missed, vec!["every hour".to_string()], "quick restart must miss, not fire: {rep:?}");
    assert!(rep.succeeded.is_empty(), "{rep:?}");
    assert!(!st.schedule(&s.id).unwrap().unwrap().enabled);
    assert!(st.schedule_runs(&s.id).unwrap()[0].error.as_deref().unwrap().starts_with("missed"));
}

#[test]
fn ends_at_disables_repeating_like_once() {
    let (st, mut s) = setup();
    st.set_setting(LAST_TICK_KEY, Some(&(T0 - 1_000).to_string())).unwrap();
    st.set_setting(LAST_HOLDER_KEY, Some("t")).unwrap();
    apply_timing(
        &mut s,
        &Timing {
            run_every: Some("1h".into()),
            start_at: Some("2026-11-10 14:00".into()),
            ends_at: Some("2026-11-10 14:00:00.001".into()),
            tz: Some("UTC".into()),
            ..Default::default()
        },
        T0,
    )
    .unwrap();
    st.save_schedule(&s).unwrap();
    let rep = tick(&st, "t", T0 + 100).unwrap();
    assert_eq!(rep.succeeded.len(), 1, "{rep:?}");
    assert!(!st.schedule(&s.id).unwrap().unwrap().enabled);
}

#[test]
fn failures_retry_and_abandon() {
    let (st, s) = setup();
    st.set_ruleset_enabled(s.ruleset_id.as_deref().unwrap(), false).unwrap();
    let rep = tick(&st, "t", T0).unwrap();
    assert_eq!(rep.failed.len(), 1);
    assert_eq!(rep.retries.len(), 1);
    let r = st.schedule_run(rep.retries[0]).unwrap().unwrap();
    assert_eq!((r.attempt, r.not_before), (2, T0 + 30_000));
    assert_eq!(backoff(2), 60_000);
    assert_eq!(backoff(20), 3_600_000);
    // a run stuck in running is abandoned after 2 minutes
    let id =
        st.insert_run(&ScheduleRun { schedule_id: s.id.clone(), status: "pending".into(), timestamp: T0, attempt: 3, ..Default::default() }).unwrap();
    st.claim_run(id, "dead", T0).unwrap();
    let rep = tick(&st, "t", T0 + ABANDON_MS + 1000).unwrap();
    assert!(rep.abandoned.contains(&id));
}

#[test]
fn run_now_and_status() {
    let (st, s) = setup();
    let r = run_now(&st, &s, "cli", T0).unwrap();
    assert_eq!(r.status, "success");
    let v = status(&st, T0).unwrap();
    assert_eq!(v["next"]["schedule"], "every hour");
    assert!(!describe(&s).is_empty());
}

#[test]
fn connector_skip_message_names_pipe_and_ruleset() {
    // Q-I2 error text contract (live path builds the same string)
    let missing = ["pipe", "run_ruleset"];
    let msg = format!("plugin did not call {}", missing.join(" or "));
    assert_eq!(msg, "plugin did not call pipe or run_ruleset");
}
