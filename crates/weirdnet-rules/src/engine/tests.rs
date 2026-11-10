//! The engine against a fake application: rules in memory, events that then-actions emit
//! re-entering the engine the way recorded actions do in a real host.
use super::*;
use crate::syntax::parse_condition_with;
use crate::{SPEC_VERSION, Schedule};
use serde::Deserialize;
use std::cell::Cell;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "do", rename_all = "snake_case")]
enum Do {
    Say { text: String },
    Fail { why: String },
    Emit { kind: String },
}

impl ThenAction for Do {
    fn verb(&self) -> &'static str {
        match self {
            Do::Say { .. } => "say",
            Do::Fail { .. } => "fail",
            Do::Emit { .. } => "emit",
        }
    }
}

#[derive(Clone)]
struct R {
    id: String,
    enabled: bool,
    spec: RuleSpec<Do>,
    last: Option<i64>,
}

impl Rule for R {
    type Then = Do;
    fn id(&self) -> &str {
        &self.id
    }
    fn name(&self) -> &str {
        &self.id
    }
    fn enabled(&self) -> bool {
        self.enabled
    }
    fn spec(&self) -> &RuleSpec<Do> {
        &self.spec
    }
    fn created_at(&self) -> i64 {
        1_000
    }
    fn last_run_at(&self) -> Option<i64> {
        self.last
    }
}

struct Ev {
    id: i64,
    kind: String,
    params: Value,
}

impl Event for Ev {
    fn id(&self) -> i64 {
        self.id
    }
    fn kind(&self) -> &str {
        &self.kind
    }
    fn param(&self, key: &str) -> Option<&Value> {
        self.params.get(key)
    }
}

#[derive(Default)]
struct Fake {
    rt: Runtime,
    rules: Vec<R>,
    next: Cell<i64>,
    log: RefCell<Vec<String>>,
    runs: RefCell<Vec<Run>>,
    read: RefCell<Vec<String>>,
    broken: bool,
}

impl Fake {
    fn with(rules: Vec<R>) -> Self {
        Fake { rules, ..Default::default() }
    }
    fn emit(&self, kind: &str, params: Value) -> usize {
        self.next.set(self.next.get() + 1);
        dispatch(self, &Ev { id: self.next.get(), kind: kind.into(), params })
    }
    fn log(&self) -> Vec<String> {
        self.log.borrow().clone()
    }
}

impl Host for Fake {
    type Rule = R;
    type Event = Ev;
    type Error = String;
    fn runtime(&self) -> &Runtime {
        &self.rt
    }
    fn enabled_rules(&self) -> Result<Vec<R>, String> {
        if self.broken {
            return Err("disk on fire".into());
        }
        Ok(self.rules.iter().filter(|r| r.enabled).cloned().collect())
    }
    fn field(&self, _r: &R, e: &Ev, field: &str) -> Option<Value> {
        self.read.borrow_mut().push(field.to_string());
        match field {
            "kind" => Some(e.kind.clone().into()),
            "svc:costly" => Some(1.into()),
            f => crate::compare::path(&e.params, f).cloned(),
        }
    }
    fn deferred(&self, field: &str) -> bool {
        field.starts_with("svc:")
    }
    fn is_related(&self, r: &R, e: &Ev) -> bool {
        e.params.get("home").and_then(Value::as_str) == Some(r.id.as_str())
    }
    fn begin_run(&self, r: &R, e: &Ev) -> Option<i64> {
        self.log.borrow_mut().push(format!("begin {} on #{} {} chain={:?}", r.id, e.id, e.kind, self.rt.chain()));
        Some(e.id * 100)
    }
    fn exec(&self, r: &R, _e: &Ev, t: &Do) -> Result<String, String> {
        assert_eq!(self.rt.chain().last(), Some(&r.id), "the running rule is in the chain");
        match t {
            Do::Say { text } => Ok(text.clone()),
            Do::Fail { why } => Err(why.clone()),
            Do::Emit { kind } => {
                self.emit(kind, json!({}));
                Ok(format!("emitted {kind}"))
            }
        }
    }
    fn finish_run(&self, r: &R, _e: &Ev, run: &Run) {
        self.log.borrow_mut().push(format!("end {} {}", r.id, run.status));
        self.runs.borrow_mut().push(run.clone());
    }
    fn blocked(&self, r: &R, e: &Ev, why: &str) {
        self.log.borrow_mut().push(format!("blocked {} on {}: {why}", r.id, e.kind));
    }
    fn rules_unavailable(&self, e: &String) {
        self.log.borrow_mut().push(format!("unavailable: {e}"));
    }
}

fn rule(id: &str, on: &[&str], when: &[&str], then: Vec<Do>) -> R {
    R {
        id: id.into(),
        enabled: true,
        spec: RuleSpec {
            on: on.iter().map(|s| s.to_string()).collect(),
            when: when.iter().map(|c| parse_condition_with(c, &["svc:"]).unwrap()).collect(),
            then,
            ..Default::default()
        },
        last: None,
    }
}

fn say(t: &str) -> Do {
    Do::Say { text: t.into() }
}

fn emit(k: &str) -> Do {
    Do::Emit { kind: k.into() }
}

#[test]
fn matching_runs_in_order_and_collects_results() {
    let f = Fake::with(vec![
        rule("a", &["import"], &["inserted > 0"], vec![say("hi"), Do::Fail { why: "boom".into() }, say("after")]),
        rule("b", &["journal.*"], &[], vec![say("j")]),
        R { enabled: false, ..rule("off", &["*"], &[], vec![say("never")]) },
    ]);
    assert_eq!(f.emit("import", json!({"inserted": 0})), 0);
    assert_eq!(f.emit("import", json!({"inserted": 3})), 1);
    let runs = f.runs.borrow();
    assert_eq!(runs[0].status, "failed", "one failing then-action fails the run");
    assert_eq!(runs[0].messages, ["hi", "fail failed: boom", "after"], "and the rest still ran");
    assert_eq!(runs[0].results[1], json!({"do": "fail", "ok": false, "error": "boom"}));
    assert_eq!(runs[0].results[0], json!({"do": "say", "ok": true, "message": "hi"}));
    assert_eq!(runs[0].action, Some(200));
    drop(runs);
    assert_eq!(f.emit("journal.export", json!({})), 1);
    assert_eq!(f.runs.borrow()[1].status, "ok");
    assert!(f.log().iter().all(|l| !l.contains("off")), "disabled rules never run");
    assert!(!f.rt.running());
}

#[test]
fn engine_events_and_unreadable_rules_run_nothing() {
    let f = Fake::with(vec![rule("all", &["*"], &[], vec![say("x")])]);
    assert_eq!(f.emit("rule.run", json!({})), 0, "rule.* never triggers");
    let broken = Fake { broken: true, ..Fake::with(vec![rule("all", &["*"], &[], vec![say("x")])]) };
    assert_eq!(broken.emit("import", json!({})), 0);
    assert_eq!(broken.log(), ["unavailable: disk on fire"]);
}

#[test]
fn a_rule_never_reenters_itself() {
    let f = Fake::with(vec![rule("ping", &["ping"], &[], vec![emit("ping")])]);
    f.emit("ping", json!({}));
    assert_eq!(f.log(), ["begin ping on #1 ping chain=[]", "blocked ping on ping: loop: ping is already running for this action", "end ping ok"]);
}

#[test]
fn chains_stop_at_max_depth() {
    let f = Fake::with(vec![
        rule("a", &["a"], &[], vec![emit("b")]),
        rule("b", &["b"], &[], vec![emit("c")]),
        rule("c", &["c"], &[], vec![emit("d")]),
        rule("d", &["d"], &[], vec![say("deep")]),
    ]);
    f.emit("a", json!({}));
    let log = f.log();
    assert!(log.contains(&"begin c on #3 c chain=[\"a\", \"b\"]".to_string()), "{log:?}");
    assert!(log.contains(&format!("blocked d on d: too deep: {MAX_DEPTH} rules already ran in a chain")), "{log:?}");
    assert!(!log.iter().any(|l| l.starts_with("begin d")));
}

#[test]
fn one_outermost_event_starts_at_most_max_runs() {
    let f = Fake::with(vec![rule("fan", &["start"], &[], (0..60).map(|_| emit("y")).collect()), rule("y", &["y"], &[], vec![say("y")])]);
    f.emit("start", json!({}));
    let log = f.log();
    assert_eq!(log.iter().filter(|l| l.starts_with("begin y")).count(), MAX_RUNS as usize - 1, "fan used one run");
    assert_eq!(log.iter().filter(|l| l.ends_with(&format!("limit: {MAX_RUNS} rule runs for one action"))).count(), 11);
    // a new outermost event gets a fresh budget
    f.log.borrow_mut().clear();
    f.emit("y", json!({}));
    assert_eq!(f.log(), ["begin y on #62 y chain=[]", "end y ok"]);
}

#[test]
fn related_and_deferred_fields() {
    let f = Fake::with(vec![R {
        spec: RuleSpec { related: true, ..rule("home", &["import"], &["svc:costly > 0"], vec![say("x")]).spec },
        ..rule("home", &[], &[], vec![])
    }]);
    let r = &f.rules[0];
    let off = Ev { id: 1, kind: "import".into(), params: json!({"home": "elsewhere"}) };
    let c = check(&f, r, &off);
    assert_eq!((c.related, c.matched, c.conditions[0].ok), (Some(false), false, false));
    assert!(f.read.borrow().is_empty(), "a costly field is not read for an unrelated event");
    let other = Ev { id: 2, kind: "export".into(), params: json!({"home": "home"}) };
    assert!(!check(&f, r, &other).matched && f.read.borrow().is_empty(), "nor for another kind");
    let on = Ev { id: 3, kind: "import".into(), params: json!({"home": "home"}) };
    let c = check(&f, r, &on);
    assert!(c.matched && c.on.as_deref() == Some("import"));
    assert_eq!(c.conditions[0].condition, "svc:costly > 0");
    assert_eq!(c.conditions[0].actual, Some(json!(1)));
    assert_eq!(*f.read.borrow(), ["svc:costly"]);
}

#[test]
fn schedule_events_name_their_rule() {
    let mut hourly = rule("hourly", &[], &[], vec![say("tick")]);
    hourly.spec.schedule = Some(Schedule::Every(3_600_000));
    let listener = rule("listener", &["schedule"], &[], vec![say("any schedule")]);
    let all = rule("all", &["*"], &[], vec![say("all")]);
    let f = Fake::with(vec![hourly.clone(), listener.clone(), all.clone()]);
    let named = |id: &str| Ev { id: 1, kind: SCHEDULE.into(), params: json!({ "rule": id }) };
    assert!(check(&f, &hourly, &named("hourly")).matched);
    assert!(!check(&f, &all, &named("hourly")).matched, "a named schedule is only for its rule");
    assert!(check(&f, &listener, &named("listener")).matched, "listens to schedule");
    assert!(check(&f, &all, &named("all")).matched, "`*` includes schedule");
    assert!(!check(&f, &hourly, &named("missing")).matched, "a schedule naming a missing rule runs nothing");
    let unnamed = Ev { id: 2, kind: SCHEDULE.into(), params: json!({}) };
    assert!(check(&f, &listener, &unnamed).matched && check(&f, &all, &unnamed).matched && !check(&f, &hourly, &unnamed).matched);
    assert_eq!(f.emit(SCHEDULE, json!({ "rule": "nobody" })), 0);
}

#[test]
fn due_next_run_and_brief_with_a_fake_clock() {
    let mut r = rule("r", &["import"], &[], vec![say("a"), say("b")]);
    assert!(!due(&r, i64::MAX) && next_run(&r).is_none());
    assert_eq!(brief(&r), "on import → 2 actions");
    r.spec.schedule = Some(Schedule::Every(60_000));
    assert!(due(&r, 1_000) && !due(&r, 999), "never ran: due from creation");
    r.last = Some(10_000);
    assert!(!due(&r, 69_999) && due(&r, 70_000));
    r.enabled = false;
    r.spec.then.pop();
    assert_eq!(brief(&r), "every 1m, on import → 1 action (off)");
    r.spec.on.clear();
    r.spec.schedule = Some(Schedule::Cron("0 31 2 * *".into()));
    assert_eq!(brief(&r), "cron \"0 31 2 * *\" → 1 action (off)");
    r.spec.schedule = Some(Schedule::Cron("0 0 31 2 *".into()));
    assert!(next_run(&r).is_none() && !due(&r, i64::MAX), "a cron that never fires is never due");
}

/// Specs written by the first version (`version` 1, the `if` key, `do`-tagged then-actions)
/// still read, and write back the same bytes.
#[test]
fn version_one_specs_round_trip() {
    let v1 = r#"{"version":1,"on":["import"],"if":[{"field":"inserted","op":">","value":"0"},{"field":"out","op":"?","value":""}],"then":[{"do":"say","text":"hi {inserted}"},{"do":"emit","kind":"x"}],"related":true,"schedule":{"cron":"0 3 * * *"}}"#;
    let spec: RuleSpec<Do> = serde_json::from_str(v1).unwrap();
    assert_eq!(spec.when[1].op, crate::Op::Exists);
    assert_eq!(serde_json::to_string(&spec).unwrap(), v1);
    // older or hand-written specs: no version, no conditions, no value on a unary op
    let bare: RuleSpec<Do> = serde_json::from_str(r#"{"on":["*"],"if":[{"field":"x","op":"!?"}],"then":[]}"#).unwrap();
    assert_eq!((bare.version, bare.when[0].value.as_str(), bare.related, bare.schedule), (SPEC_VERSION, "", false, None));
    let every: RuleSpec<Do> = serde_json::from_str(r#"{"on":[],"then":[],"schedule":{"every":21600000}}"#).unwrap();
    assert_eq!(serde_json::to_string(&every).unwrap(), r#"{"version":1,"on":[],"if":[],"then":[],"related":false,"schedule":{"every":21600000}}"#);
    assert!(serde_json::from_str::<RuleSpec<Do>>(r#"{"on":[],"then":[{"do":"unknown"}]}"#).is_err());
}
