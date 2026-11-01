//! Tests for scheduled rules and the built-in cert expiry watcher (2.16).
use crate::org::host::self_signed;
use crate::org::tls;
use crate::rules::*;
use crate::store::{NewRule, RuleChange, RuleSpec, Schedule, Store, now_ms};

fn org(s: &Store) -> crate::store::Org {
    crate::org::create(s, "cli", "Comet Lab", None, "").unwrap()
}

#[test]
fn every_and_cron_parse_print_and_step() {
    assert_eq!(parse_every("6h").unwrap(), Schedule::Every(6 * 3_600_000));
    assert_eq!(parse_every("30m").unwrap(), Schedule::Every(1_800_000));
    assert_eq!(schedule_text(&parse_every("1d").unwrap()), "every 1d");
    assert_eq!(schedule_text(&parse_every("90m").unwrap()), "every 90m");
    assert!(parse_every("10s").is_err() && parse_every("6x").is_err() && parse_every("h").is_err());
    assert!(parse_cron("0 3 * * *").is_ok() && parse_cron("*/15 9-17 * * 1-5").is_ok() && parse_cron("0,30 * 1 1 0").is_ok());
    assert!(parse_cron("0 3 * *").is_err() && parse_cron("60 * * * *").is_err() && parse_cron("* 25 * * *").is_err());
    assert!(parse_schedule(Some("1h"), Some("0 3 * * *")).is_err());
    use chrono::{Local, TimeZone, Timelike};
    let at = Local.with_ymd_and_hms(2026, 11, 1, 10, 7, 0).unwrap().timestamp_millis();
    let n = parse_cron("0 3 * * *").unwrap().next_after(at).unwrap();
    let t = Local.timestamp_millis_opt(n).unwrap();
    assert_eq!((t.hour(), t.minute()), (3, 0));
    assert!(n > at && n - at <= 24 * 3_600_000 + 3_600_000);
    let n = parse_cron("*/15 * * * *").unwrap().next_after(at).unwrap();
    assert_eq!(Local.timestamp_millis_opt(n).unwrap().minute(), 15);
    // weekdays: Sunday Nov 1 2026, so Monday 09:00 is the next "0 9 * * 1"
    let n = parse_cron("0 9 * * 1").unwrap().next_after(at).unwrap();
    assert_eq!(Local.timestamp_millis_opt(n).unwrap().format("%a %H:%M").to_string(), "Mon 09:00");
    // the TUI's On field
    let (on, s) = parse_on_text("every 6h; import").unwrap();
    assert_eq!((on, s.clone()), (vec!["import".to_string()], Some(Schedule::Every(6 * 3_600_000))));
    let spec = RuleSpec { on: vec![], schedule: Some(Schedule::Cron("0 3 * * *".into())), ..Default::default() };
    assert_eq!(on_text(&spec), "cron \"0 3 * * *\"");
    assert_eq!(parse_on_text(&on_text(&spec)).unwrap().1, spec.schedule);
    assert_eq!(parse_on_text("import").unwrap(), (vec!["import".to_string()], None));
}

#[test]
fn scheduled_rules_run_when_due_once_and_only_on_their_own_schedule() {
    let s = Store::open_in_memory().unwrap();
    let spec =
        RuleSpec { schedule: Some(Schedule::Every(3_600_000)), then: vec![parse_then(&s, "status tick {kind}").unwrap()], ..Default::default() };
    let r = s.create_rule(&NewRule { name: "Hourly".into(), spec, enabled: true, ..Default::default() }).unwrap();
    // an everything rule does not react to another rule's schedule
    let all = RuleSpec { on: vec!["*".into()], then: vec![parse_then(&s, "status any").unwrap()], ..Default::default() };
    s.create_rule(&NewRule { name: "Any".into(), spec: all, enabled: true, ..Default::default() }).unwrap();
    assert!(brief(&s.find_rule("Hourly").unwrap()).starts_with("every 1h →"));
    // dry runs see the schedule action
    let a = schedule_action(&r);
    assert!(check(&s, &r, &a).matched && !check(&s, &s.find_rule("Any").unwrap(), &a).matched);
    assert_eq!(tick(&s, now_ms()).unwrap(), vec!["Hourly".to_string()], "never ran: due at once");
    let r = s.find_rule("Hourly").unwrap();
    assert_eq!((r.runs, r.last_status.as_deref()), (1, Some("ok")));
    assert_eq!(s.find_rule("Any").unwrap().runs, 0);
    let runs = s.rule_runs(&r.id, 5).unwrap();
    assert!(runs[0].summary.contains("tick schedule"), "{}", runs[0].summary);
    assert!(tick(&s, now_ms()).unwrap().is_empty(), "not again within the hour");
    assert_eq!(tick(&s, now_ms() + 3_600_001).unwrap().len(), 1);
    // off: not run; a rule needs on or a schedule
    s.set_rule_enabled(&r.id, false).unwrap();
    assert!(tick(&s, now_ms() + 10 * 3_600_000).unwrap().is_empty());
    let mut sp = r.spec.clone();
    sp.schedule = None;
    assert!(s.update_rule(&r.id, &RuleChange { spec: Some(sp), ..Default::default() }).is_err());
    // the spec reads back the same, and old specs without a schedule still read
    let v = serde_json::to_value(&r.spec).unwrap();
    assert_eq!(v["schedule"]["every"], 3_600_000);
    assert!(serde_json::to_value(RuleSpec::default()).unwrap().get("schedule").is_none());
}

#[test]
fn the_cert_watcher_is_built_in_once_editable_and_notifies_for_your_certificates() {
    let s = Store::open_in_memory().unwrap();
    let o = org(&s);
    assert!(s.rules().unwrap().is_empty());
    tls::add_domain(&s, &o, "lab.example.org").unwrap();
    let w = s.find_rule(CERT_WATCH).unwrap();
    assert_eq!(w.spec, cert_watch_spec());
    assert_eq!(then_text(&s, &w.spec.then[0]), "cert-watch renew=30 notify=30,14,3");
    assert_eq!(parse_then(&s, "cert-watch renew=20 notify=7,1").unwrap(), crate::store::Then::CertWatch { renew_days: 20, notify_days: vec![7, 1] });
    // your certificate with 10 days left: the 14-day notice, once
    tls::set_user_cert(&s, &o, &self_signed(&["lab.example.org".into()], 10).unwrap()).unwrap();
    assert!(tick(&s, now_ms()).unwrap().contains(&CERT_WATCH.to_string()));
    let notes: Vec<_> = s.actions_of_kind("notify").unwrap();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].params["about"], "cert.expiry");
    assert!(notes[0].summary.contains("expires in 9 days") || notes[0].summary.contains("expires in 10 days"), "{}", notes[0].summary);
    assert!(tls::watch(&s, "rule", 30, &[30, 14, 3], false).unwrap().is_empty(), "the same notice is not repeated");
    // an ACME org without a host: told to start it; with a host: asked of it
    let o2 = crate::org::create(&s, "cli", "Nebula", None, "").unwrap();
    tls::add_domain(&s, &o2, "nebula.example.org").unwrap();
    tls::set_acme(&s, &o2, &tls::OrgAcme { enabled: true, email: "a@example.org".into() }).unwrap();
    let v = tls::watch(&s, "rule", 30, &[30, 14, 3], false).unwrap();
    assert!(v.iter().any(|m| m.contains("start the host")), "{v:?}");
    let v = tls::watch(&s, "rule", 30, &[30, 14, 3], true).unwrap();
    assert!(v.iter().any(|m| m.contains("asked the host")) && tls::renewal_requested(&s, &o2).unwrap());
    // removed, it stays removed
    s.remove_rule(&w.id).unwrap();
    tls::add_domain(&s, &o, "www.lab.example.org").unwrap();
    assert!(s.find_rule(CERT_WATCH).is_err());
}
