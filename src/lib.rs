//! Muninn: ingest and normalize crypto market data into bars.
pub mod cli;
pub mod error;
pub mod interact;
pub mod paths;
pub mod prefs;

pub use error::{Error, Result};
