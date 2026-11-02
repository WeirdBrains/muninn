//! Running a peer's served executables (2.19): the guest asks the host to call one of its
//! org endpoints (records, rules, code); the host checks the guest's membership and role and
//! the endpoint's consent rules exactly as an org call (`crate::org::call`), then keeps the
//! output as a `run` session of `RunChunk`s, which the guest reads like any live session.
use super::proto::{Batch, Kind, Op, RunChunk};
use super::wire::{CollabReq, CollabResp, Transport, unexpected};
use crate::error::{Error, Result};
use crate::store::{LiveSession, NewAction, Store, now_ms};
use serde_json::{Value, json};

/// The most output one chunk carries.
pub const CHUNK: usize = 16 * 1024;
/// The most output a run keeps.
pub const MAX_OUTPUT: usize = 8 * 1024 * 1024;

fn chunks(text: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    for c in text.chars() {
        if cur.len() + c.len_utf8() > CHUNK {
            out.push(std::mem::take(&mut cur));
        }
        cur.push(c);
    }
    if !cur.is_empty() || out.is_empty() {
        out.push(cur);
    }
    out
}

/// Host: keep a call's outcome for `peer` as a run session; its id.
pub fn record(s: &Store, me: &str, peer: &str, org: &str, endpoint: &str, out: Result<crate::net::proto::Resp>) -> Result<String> {
    use crate::net::proto::Resp;
    let (body, error) = match out {
        Ok(Resp::Org { body }) => (body, String::new()),
        Ok(Resp::Error { msg }) => (Value::Null, msg),
        Ok(_) => (Value::Null, "unexpected answer".to_string()),
        Err(e) => (Value::Null, e.to_string()),
    };
    let now = now_ms();
    let ls = LiveSession {
        id: crate::store::new_id(),
        kind: "run".into(),
        item_id: format!("{org}/{endpoint}"),
        field: String::new(),
        title: format!("{org} {endpoint}"),
        role: "host".into(),
        host_peer: String::new(),
        exchange_id: None,
        status: "open".into(),
        seen: 0,
        peers: vec![peer.into()],
        created_at: now,
        updated_at: now,
    };
    s.put_live_session(&ls)?;
    let text = if error.is_empty() { body.to_string() } else { String::new() };
    let text = if text.len() > MAX_OUTPUT { String::new() } else { text };
    let error = if error.is_empty() && body.to_string().len() > MAX_OUTPUT { "the output is too large to send".into() } else { error };
    let parts = chunks(&text);
    let last = parts.len() - 1;
    for (i, part) in parts.into_iter().enumerate() {
        let c = RunChunk { json: part, done: i == last, error: if i == last { error.clone() } else { String::new() } };
        s.append_live_op(&ls.id, me, "", &Op::new(Kind::Run(c)).bytes())?;
    }
    s.append_live_op(&ls.id, me, "", &Op::new(Kind::Closed(true)).bytes())?;
    s.set_live_session(&ls.id, Some("closed"), None)?;
    s.note_action(
        &NewAction::new("live.run", "network", format!("ran {endpoint} of {org} for a peer{}", if error.is_empty() { "" } else { " (failed)" }))
            .params(json!({ "session": ls.id, "org": org, "endpoint": endpoint, "peer": peer, "error": error })),
    );
    Ok(ls.id)
}

/// Guest: run `endpoint` of `org` on the peer behind `t`, handing each chunk to `each` as it
/// arrives; the whole result.
#[allow(clippy::too_many_arguments)]
pub fn run(
    s: &Store,
    t: &dyn Transport,
    host: &str,
    exchange: &str,
    org: &str,
    endpoint: &str,
    input: Value,
    mut each: impl FnMut(&RunChunk),
) -> Result<Value> {
    let session = match t.call(CollabReq::Run { org: org.into(), endpoint: endpoint.into(), input })? {
        CollabResp::Started { session } => session,
        _ => return Err(unexpected()),
    };
    let ls = super::session::joined(s, &session, host, exchange, &json!({ "kind": "run", "title": format!("{org} {endpoint}") }))?;
    let mut seen = 0;
    let mut text = String::new();
    for round in 0..600 {
        let (st, _) = super::session::sync_with(s, &ls, vec![], |b: Batch| t.live(b))?;
        for c in &st.runs[seen..] {
            each(c);
            text.push_str(&c.json);
            if c.done && !c.error.is_empty() {
                return Err(Error::Other(c.error.clone()));
            }
        }
        seen = st.runs.len();
        if st.runs.last().is_some_and(|c| c.done) {
            return serde_json::from_str(&text).map_err(|e| Error::Other(format!("the run's output: {e}")));
        }
        if st.closed {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(if round < 10 { 100 } else { 500 }));
    }
    Err(Error::Other("the run ended without a result".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_is_chunked_on_char_boundaries() {
        let t = "é".repeat(CHUNK);
        let c = chunks(&t);
        assert!(c.len() == 2 && c.iter().all(|p| p.len() <= CHUNK));
        assert_eq!(c.concat(), t);
        assert_eq!(chunks(""), vec![String::new()]);
    }
}
