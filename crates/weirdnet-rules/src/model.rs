//! What a rule is: when it runs, what must hold, and what it does.
use serde::{Deserialize, Serialize};

/// Spec version written by this crate.
pub const SPEC_VERSION: u32 = 1;

/// How a condition compares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Op {
    #[serde(rename = "=")]
    Eq,
    #[serde(rename = "!=")]
    Ne,
    #[serde(rename = "~")]
    Contains,
    #[serde(rename = "!~")]
    NotContains,
    #[serde(rename = ">")]
    Gt,
    #[serde(rename = ">=")]
    Ge,
    #[serde(rename = "<")]
    Lt,
    #[serde(rename = "<=")]
    Le,
    #[serde(rename = "?")]
    Exists,
    #[serde(rename = "!?")]
    Missing,
}

impl Op {
    /// Longest first, so `>=` is not read as `>`.
    pub const ALL: [Op; 10] = [Op::Ne, Op::NotContains, Op::Ge, Op::Le, Op::Missing, Op::Eq, Op::Contains, Op::Gt, Op::Lt, Op::Exists];
    pub fn as_str(self) -> &'static str {
        match self {
            Op::Eq => "=",
            Op::Ne => "!=",
            Op::Contains => "~",
            Op::NotContains => "!~",
            Op::Gt => ">",
            Op::Ge => ">=",
            Op::Lt => "<",
            Op::Le => "<=",
            Op::Exists => "?",
            Op::Missing => "!?",
        }
    }
    pub fn unary(self) -> bool {
        matches!(self, Op::Exists | Op::Missing)
    }
}

/// `field op value`, e.g. `inserted > 0`, `format = pdf`, `summary ~ btc`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Condition {
    pub field: String,
    pub op: Op,
    #[serde(default)]
    pub value: String,
}

/// When a scheduled rule runs: every so often (`every 6h`) or at cron times
/// (`cron "0 3 * * *"`). See [`crate::schedule::next_run`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Schedule {
    /// Milliseconds between runs.
    Every(i64),
    Cron(String),
}

/// A then-action of the application's own type: the engine only needs its verb (for run
/// results and failure messages).
pub trait ThenAction {
    fn verb(&self) -> &'static str;
}

fn spec_version() -> u32 {
    SPEC_VERSION
}

/// When a rule runs and what it does; `T` is the application's then-action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleSpec<T> {
    #[serde(default = "spec_version")]
    pub version: u32,
    /// Event kinds (`import`, `journal.*`, `*`); any one matching is enough.
    pub on: Vec<String>,
    /// All must hold.
    #[serde(default, rename = "if")]
    pub when: Vec<Condition>,
    pub then: Vec<T>,
    /// Only for events about where the rule lives (the host decides what that means).
    #[serde(default)]
    pub related: bool,
    /// Run on a schedule as well.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule: Option<Schedule>,
}

impl<T> Default for RuleSpec<T> {
    fn default() -> Self {
        Self { version: SPEC_VERSION, on: vec![], when: vec![], then: vec![], related: false, schedule: None }
    }
}
