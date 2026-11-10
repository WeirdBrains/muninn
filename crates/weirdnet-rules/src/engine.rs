//! Matching and running rules against events, with loop protection.
//!
//! - Events of kind `rule.*` (the engine's own bookkeeping) never trigger rules.
//! - A rule already running in the current chain is not started again.
//! - Chains stop at [`MAX_DEPTH`] rules; one outermost event starts at most [`MAX_RUNS`] runs.
//! - Blocked runs are reported to [`Host::blocked`] with the reason.
//!
//! The application supplies the rules, the events' fields, what "related" means, and what
//! a then-action does, through [`Host`]. Whether someone may create, change or run a rule
//! is decided by the application before it gets here.
use crate::compare::compare;
use crate::model::{RuleSpec, ThenAction};
use crate::schedule::schedule_text;
use crate::syntax::{condition_text, matches_kind};
use serde::Serialize;
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};

/// Most rules in one chain (a rule whose action triggers a rule whose action triggers …).
pub const MAX_DEPTH: usize = 3;
/// Most rule runs started by one outermost event.
pub const MAX_RUNS: u32 = 50;
/// The event kind a scheduled rule runs on. With a `rule` param naming a rule, only that
/// rule (if it has a schedule or listens to `schedule`) reacts.
pub const SCHEDULE: &str = "schedule";

/// A stored rule, as the engine sees it.
pub trait Rule {
    type Then: ThenAction;
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn enabled(&self) -> bool;
    fn spec(&self) -> &RuleSpec<Self::Then>;
    fn created_at(&self) -> i64;
    fn last_run_at(&self) -> Option<i64>;
}

/// Something that happened, as the engine sees it.
pub trait Event {
    fn id(&self) -> i64;
    fn kind(&self) -> &str;
    /// One of the event's parameters (the engine reads only `rule`, on [`SCHEDULE`] events).
    fn param(&self, key: &str) -> Option<&Value>;
}

/// The chain of rules running now (outermost first) and the runs left for the outermost
/// event. Owned by the application (one per store or session) and handed out by [`Host::runtime`].
#[derive(Debug, Default)]
pub struct Runtime {
    chain: RefCell<Vec<String>>,
    budget: Cell<u32>,
}

impl Runtime {
    pub fn new() -> Self {
        Self::default()
    }
    /// Ids of the rules running now, outermost first (empty outside a run). Applications
    /// record it as provenance on what a run does.
    pub fn chain(&self) -> Vec<String> {
        self.chain.borrow().clone()
    }
    pub fn running(&self) -> bool {
        !self.chain.borrow().is_empty()
    }
    /// Runs left for the current outermost event.
    pub fn budget(&self) -> u32 {
        self.budget.get()
    }
}

/// The run of one rule, handed to [`Host::finish_run`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Run {
    /// What [`Host::begin_run`] returned.
    pub action: Option<i64>,
    /// `ok` or `failed`.
    pub status: &'static str,
    /// One per then-action: `{do, ok, message}` or `{do, ok: false, error}`.
    pub results: Vec<Value>,
    /// One per then-action: its message, or `VERB failed: ERROR`.
    pub messages: Vec<String>,
}

/// The outcome of one run.
#[derive(Debug, Clone, Serialize)]
pub struct RunResult {
    pub action: Option<i64>,
    pub status: String,
    pub messages: Vec<String>,
}

/// What the engine needs from the application.
pub trait Host {
    type Rule: Rule;
    type Event: Event;
    type Error: std::fmt::Display;

    fn runtime(&self) -> &Runtime;
    /// The enabled rules, in the order they should run.
    fn enabled_rules(&self) -> Result<Vec<Self::Rule>, Self::Error>;
    /// The value of a field on an event (`None`: absent).
    fn field(&self, rule: &Self::Rule, event: &Self::Event, field: &str) -> Option<Value>;
    /// Fields that cost something to read (an outside call): read only when the rule could
    /// otherwise run, never in a dry check of a rule that does not apply.
    fn deferred(&self, _field: &str) -> bool {
        false
    }
    /// For `related` rules: is the event about where the rule lives?
    fn is_related(&self, rule: &Self::Rule, event: &Self::Event) -> bool;
    /// A run starts (record it, say); the returned id comes back in [`Run::action`].
    fn begin_run(&self, rule: &Self::Rule, event: &Self::Event) -> Option<i64>;
    /// Do one then-action; its message on success.
    fn exec(&self, rule: &Self::Rule, event: &Self::Event, then: &<Self::Rule as Rule>::Then) -> Result<String, Self::Error>;
    /// A run ended (every then-action was tried, in order, whatever the others did).
    fn finish_run(&self, rule: &Self::Rule, event: &Self::Event, run: &Run);
    /// A matching rule was not run (`loop: …`, `too deep: …`, `limit: …`).
    fn blocked(&self, rule: &Self::Rule, event: &Self::Event, why: &str);
    /// The rules could not be listed; nothing runs for this event.
    fn rules_unavailable(&self, _error: &Self::Error) {}
}

/// How a rule fares against one event (a dry run).
#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub rule: String,
    pub name: String,
    pub action: i64,
    pub kind: String,
    pub enabled: bool,
    /// The `on` pattern that matched.
    pub on: Option<String>,
    /// `None` when the rule is not `related`.
    pub related: Option<bool>,
    pub conditions: Vec<CondCheck>,
    pub matched: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CondCheck {
    pub condition: String,
    pub actual: Option<Value>,
    pub ok: bool,
}

/// Does the rule apply to the event, and why (not)? Does not look at `enabled`.
pub fn check<H: Host>(host: &H, r: &H::Rule, e: &H::Event) -> Check {
    let spec = r.spec();
    let kind = e.kind();
    let on = if kind.starts_with("rule.") {
        None
    } else if kind == SCHEDULE {
        // a schedule event naming a rule: that rule only; one naming none matches via `on`
        match e.param("rule").and_then(Value::as_str) {
            Some(id) if id == r.id() => {
                if spec.schedule.is_some() || spec.on.iter().any(|p| matches_kind(p, SCHEDULE)) {
                    Some(SCHEDULE.to_string())
                } else {
                    None
                }
            }
            Some(_) => None,
            None => spec.on.iter().find(|p| matches_kind(p, SCHEDULE)).cloned(),
        }
    } else {
        spec.on.iter().find(|p| matches_kind(p, kind)).cloned()
    };
    let related = spec.related.then(|| host.is_related(r, e));
    let conditions: Vec<CondCheck> = spec
        .when
        .iter()
        .map(|c| {
            if host.deferred(&c.field) && (on.is_none() || related == Some(false)) {
                return CondCheck { condition: condition_text(c), ok: false, actual: None };
            }
            let actual = host.field(r, e, &c.field);
            CondCheck { condition: condition_text(c), ok: compare(actual.as_ref(), c.op, &c.value), actual }
        })
        .collect();
    let matched = on.is_some() && related != Some(false) && conditions.iter().all(|c| c.ok);
    Check {
        rule: r.id().to_string(),
        name: r.name().to_string(),
        action: e.id(),
        kind: kind.to_string(),
        enabled: r.enabled(),
        on,
        related,
        conditions,
        matched,
    }
}

/// Run every enabled rule that matches the event (call after each event is recorded).
/// Returns how many runs started.
pub fn dispatch<H: Host>(host: &H, e: &H::Event) -> usize {
    if e.kind().starts_with("rule.") {
        return 0;
    }
    let rt = host.runtime();
    let chain = rt.chain();
    if chain.is_empty() {
        rt.budget.set(MAX_RUNS);
    }
    let rules = match host.enabled_rules() {
        Ok(r) => r,
        Err(err) => {
            host.rules_unavailable(&err);
            return 0;
        }
    };
    let mut started = 0;
    for r in rules {
        if !check(host, &r, e).matched {
            continue;
        }
        let blocked = if chain.iter().any(|id| id == r.id()) {
            Some(format!("loop: {} is already running for this action", r.name()))
        } else if chain.len() >= MAX_DEPTH {
            Some(format!("too deep: {MAX_DEPTH} rules already ran in a chain"))
        } else if rt.budget.get() == 0 {
            Some(format!("limit: {MAX_RUNS} rule runs for one action"))
        } else {
            None
        };
        match blocked {
            Some(why) => host.blocked(&r, e, &why),
            None => {
                rt.budget.set(rt.budget.get() - 1);
                run(host, &r, e);
                started += 1;
            }
        }
    }
    started
}

/// Run a rule's then-actions for an event, in order; a failing one does not stop the rest.
/// Does not check whether the rule matches (see [`check`]) or may run.
pub fn run<H: Host>(host: &H, r: &H::Rule, e: &H::Event) -> RunResult {
    let rt = host.runtime();
    let action = host.begin_run(r, e);
    rt.chain.borrow_mut().push(r.id().to_string());
    let mut results = vec![];
    let mut messages = vec![];
    let mut failed = false;
    for t in &r.spec().then {
        match host.exec(r, e, t) {
            Ok(m) => {
                results.push(json!({ "do": t.verb(), "ok": true, "message": m }));
                messages.push(m);
            }
            Err(err) => {
                failed = true;
                let m = format!("{} failed: {err}", t.verb());
                results.push(json!({ "do": t.verb(), "ok": false, "error": err.to_string() }));
                messages.push(m);
            }
        }
    }
    rt.chain.borrow_mut().pop();
    let status = if failed { "failed" } else { "ok" };
    let done = Run { action, status, results, messages };
    host.finish_run(r, e, &done);
    RunResult { action, status: status.into(), messages: done.messages }
}

/// When a rule's schedule runs next (`None`: no schedule, or a cron that never fires).
pub fn next_run<R: Rule>(r: &R) -> Option<i64> {
    crate::schedule::next_run(r.spec().schedule.as_ref()?, r.created_at(), r.last_run_at())
}

/// Is a scheduled rule due at `now` (epoch ms)?
pub fn due<R: Rule>(r: &R, now: i64) -> bool {
    r.spec().schedule.is_some() && next_run(r).is_some_and(|t| t <= now)
}

/// One line about a rule: `on import → 2 actions`, `every 6h, on import → 1 action (off)`.
pub fn brief<R: Rule>(r: &R) -> String {
    let spec = r.spec();
    let n = spec.then.len();
    let when = match (&spec.schedule, spec.on.is_empty()) {
        (Some(s), true) => schedule_text(s),
        (Some(s), false) => format!("{}, on {}", schedule_text(s), spec.on.join(", ")),
        (None, _) => format!("on {}", spec.on.join(", ")),
    };
    format!("{when} → {n} action{}{}", if n == 1 { "" } else { "s" }, if r.enabled() { "" } else { " (off)" })
}

#[cfg(test)]
mod tests;
