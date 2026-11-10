//! If-this-then-that over an event history: rule specs generic over the application's
//! then-actions, the condition syntax and comparison, templates, every/cron schedules, and a
//! matching, loop-protected engine that reaches the application only through [`engine::Host`].
#![forbid(unsafe_code)]

pub mod compare;
pub mod engine;
mod error;
mod model;
pub mod schedule;
pub mod syntax;
pub mod template;

pub use error::{Result, RulesError};
pub use model::{Condition, Op, RuleSpec, SPEC_VERSION, Schedule, ThenAction};
