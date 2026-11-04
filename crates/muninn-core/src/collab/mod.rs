//! Live collaboration (2.19): messages between nodes, live editing of entries, journals,
//! code and records, presence and cursors, and running a peer's served executables with
//! streamed results. The wire format is `muninn.live.v1` ([`proto`]); edits merge as a
//! replicated text ([`text`]) or, for record fields, last writer wins. Everything reaches
//! peers through [`wire`], the transport boundary. See `docs/collaboration.md`.
pub mod grpc;
pub mod msg;
pub mod peers;
pub mod proto;
pub mod run;
pub mod session;
pub mod stream;
pub mod text;
pub mod wire;
