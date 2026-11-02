//! Live collaboration (2.19): messages between nodes, live editing of entries, journals,
//! code and records, presence and cursors, and running a peer's served executables with
//! streamed results. The wire format is `muninn.live.v1` ([`proto`]); edits merge as a
//! replicated text ([`text`]) or, for record fields, last writer wins. See
//! `docs/collaboration.md`.
pub mod proto;
pub mod session;
pub mod text;
