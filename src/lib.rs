//! Muninn (`mnnn`): the binary crate. All logic lives in `muninn-core`; this crate
//! re-exports it so `muninn::…` paths keep working for the CLI, TUI and tests.
pub use muninn_core::*;
