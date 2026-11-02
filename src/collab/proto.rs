//! `muninn.live.v1`: the protobuf messages live collaboration sends (see
//! `proto/muninn/live/v1/live.proto`, kept in step with these by a test).
use prost::Message;

pub const PACKAGE: &str = "muninn.live.v1";

/// A character's id: the site (node) that wrote it and a Lamport counter.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Message)]
pub struct Id {
    #[prost(uint64, tag = "2")]
    pub counter: u64,
    #[prost(uint64, tag = "1")]
    pub site: u64,
}

/// Insert `text` after `after` (none: at the start); its characters get ids
/// `(site, counter)`, `(site, counter + 1)`, ….
#[derive(Clone, PartialEq, Message)]
pub struct Insert {
    #[prost(message, optional, tag = "1")]
    pub id: Option<Id>,
    #[prost(message, optional, tag = "2")]
    pub after: Option<Id>,
    #[prost(string, tag = "3")]
    pub text: String,
}

/// Characters `(site, start)` … `(site, start + len - 1)`.
#[derive(Clone, PartialEq, Message)]
pub struct Span {
    #[prost(uint64, tag = "1")]
    pub site: u64,
    #[prost(uint64, tag = "2")]
    pub start: u64,
    #[prost(uint64, tag = "3")]
    pub len: u64,
}

#[derive(Clone, PartialEq, Message)]
pub struct Delete {
    #[prost(message, repeated, tag = "1")]
    pub spans: Vec<Span>,
}

/// A record field set to a JSON value, last writer (Lamport, then site) wins.
#[derive(Clone, PartialEq, Message)]
pub struct SetField {
    #[prost(string, tag = "1")]
    pub field: String,
    #[prost(string, tag = "2")]
    pub value_json: String,
    #[prost(uint64, tag = "3")]
    pub lamport: u64,
    #[prost(uint64, tag = "4")]
    pub site: u64,
}

/// Where someone is: a character offset in the visible text.
#[derive(Clone, PartialEq, Message)]
pub struct Cursor {
    #[prost(string, tag = "1")]
    pub label: String,
    #[prost(int64, optional, tag = "2")]
    pub offset: Option<i64>,
}

/// Part of a remote run's output: JSON rows (or the final result), `done` on the last.
#[derive(Clone, PartialEq, Message)]
pub struct RunChunk {
    #[prost(string, tag = "1")]
    pub json: String,
    #[prost(bool, tag = "2")]
    pub done: bool,
    #[prost(string, tag = "3")]
    pub error: String,
}

#[derive(Clone, PartialEq, prost::Oneof)]
pub enum Kind {
    #[prost(message, tag = "1")]
    Insert(Insert),
    #[prost(message, tag = "2")]
    Delete(Delete),
    #[prost(message, tag = "3")]
    Set(SetField),
    #[prost(message, tag = "4")]
    Cursor(Cursor),
    #[prost(message, tag = "5")]
    Run(RunChunk),
    /// The host closed the session.
    #[prost(bool, tag = "6")]
    Closed(bool),
}

#[derive(Clone, PartialEq, Message)]
pub struct Op {
    #[prost(oneof = "Kind", tags = "1, 2, 3, 4, 5, 6")]
    pub kind: Option<Kind>,
}

/// One operation in a session's log.
#[derive(Clone, PartialEq, Message)]
pub struct Frame {
    #[prost(uint64, tag = "1")]
    pub seq: u64,
    #[prost(uint64, tag = "2")]
    pub site: u64,
    #[prost(message, optional, tag = "3")]
    pub op: Option<Op>,
    /// Which text: `body`, `source`, an entry id or a record field.
    #[prost(string, tag = "4")]
    pub doc: String,
}

/// What travels each way: a guest's new operations (seq 0) and its cursor; the host's
/// frames after the guest's `since`, its head and who is there.
#[derive(Clone, PartialEq, Message)]
pub struct Batch {
    #[prost(string, tag = "1")]
    pub session: String,
    #[prost(uint64, tag = "2")]
    pub since: u64,
    #[prost(message, repeated, tag = "3")]
    pub frames: Vec<Frame>,
    #[prost(uint64, tag = "4")]
    pub head: u64,
    #[prost(message, repeated, tag = "5")]
    pub present: Vec<Cursor>,
}

impl Op {
    pub fn new(k: Kind) -> Op {
        Op { kind: Some(k) }
    }
    pub fn bytes(&self) -> Vec<u8> {
        self.encode_to_vec()
    }
    pub fn decode_bytes(b: &[u8]) -> crate::error::Result<Op> {
        Op::decode(b).map_err(|e| crate::error::Error::invalid("live operation", e.to_string()))
    }
}

impl Batch {
    pub fn bytes(&self) -> Vec<u8> {
        self.encode_to_vec()
    }
    pub fn decode_bytes(b: &[u8]) -> crate::error::Result<Batch> {
        Batch::decode(b).map_err(|e| crate::error::Error::invalid("live batch", e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The published .proto names the same messages and field tags as the Rust types.
    #[test]
    fn the_proto_file_matches() {
        let proto = include_str!("../../proto/muninn/live/v1/live.proto");
        let rust = include_str!("proto.rs");
        assert!(proto.contains(&format!("package {PACKAGE};")));
        let mut msg = "";
        for l in proto.lines().map(str::trim) {
            if let Some(m) = l.strip_prefix("message ").and_then(|m| m.strip_suffix(" {")) {
                msg = m;
                assert!(rust.contains(&format!("pub struct {m} {{")) || m == "Op", "{m} missing in proto.rs");
            } else if let Some((_, tag)) = l.strip_suffix(';').and_then(|l| l.rsplit_once(" = ")) {
                if l.starts_with("package") || l.starts_with("syntax") {
                    continue;
                }
                let t = format!("tag = \"{tag}\"");
                assert!(rust.contains(&t) || rust.contains("tags = \"1, 2, 3, 4, 5, 6\""), "{msg}: tag {tag}");
            }
        }
        // a round trip
        let b = Batch {
            session: "s".into(),
            since: 3,
            frames: vec![Frame { seq: 1, site: 7, op: Some(Op::new(Kind::Closed(true))), doc: "d".into() }],
            ..Default::default()
        };
        assert_eq!(Batch::decode_bytes(&b.bytes()).unwrap(), b);
    }
}
