//! Live editing sessions. The host keeps the operation log; guests send their operations and
//! get back everything after what they have, so every copy applies the same operations in
//! the host's order. The host writes the merged text back to the item (which syncs to its
//! linked nodes as usual), and turns any edit made to the item some other way (the TUI, the
//! CLI, a rule) into operations too, so nobody's change is lost.
use super::proto::{Batch, Cursor, Frame, Kind, Op, SetField};
use super::text::Text;
use crate::error::{Error, Result};
use crate::store::{CodeChange, LiveOp, LiveSession, NewAction, Store, now_ms};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// The kinds of item a session can edit.
pub const KINDS: &[&str] = &["entry", "journal", "code", "record"];
/// Presence older than this is gone.
pub const PRESENCE_MS: i64 = 30_000;
/// The most operations one batch may carry.
pub const MAX_FRAMES: usize = 500;
/// The most text one operation may carry.
pub const MAX_TEXT: usize = 256 * 1024;
const COALESCE_MS: i64 = 60_000;

/// A node's site number in the replicated texts.
pub fn site_of(peer: &str) -> u64 {
    let h = Sha256::digest(peer.as_bytes());
    u64::from_be_bytes(h[..8].try_into().expect("8 bytes")) | 1
}

/// Resolve `KIND KEY` (names, ids or prefixes; a record is `MODEL/RECORD`) to its id and
/// a title.
pub fn resolve(s: &Store, kind: &str, key: &str) -> Result<(String, String)> {
    Ok(match kind {
        "entry" => {
            let e = s.find_entry(key)?;
            (e.id.clone(), if e.title.is_empty() { format!("entry {}", &e.id[..8.min(e.id.len())]) } else { e.title })
        }
        "journal" => {
            let j = s.find_journal(key)?;
            (j.id, j.name)
        }
        "code" => {
            let c = s.find_code_space(key)?;
            (c.id, c.name)
        }
        "record" => {
            let (m, r) = key.split_once('/').ok_or_else(|| Error::invalid("record", "give MODEL/RECORD"))?;
            let m = s.find_model(m)?;
            let r = s.find_record(&m, r)?;
            (format!("{}/{}", m.id, r.id), format!("{} {}", m.name, &r.id[..8.min(r.id.len())]))
        }
        k => return Err(Error::invalid("kind", format!("`{k}` (entry, journal, code or record)"))),
    })
}

/// The texts an item has now: doc → text (records: field → JSON value).
fn current(s: &Store, kind: &str, item: &str) -> Result<BTreeMap<String, String>> {
    let mut m = BTreeMap::new();
    match kind {
        "entry" => {
            let e = s.entry(item)?.ok_or_else(|| Error::Other("the entry is gone".into()))?;
            m.insert("body".into(), e.body);
        }
        "journal" => {
            for e in s.entries(item)? {
                m.insert(e.id, e.body);
            }
        }
        "code" => {
            let c = s.code_space(item)?.ok_or_else(|| Error::Other("the code space is gone".into()))?;
            m.insert("source".into(), c.source);
        }
        "record" => {
            let (mid, rid) = item.split_once('/').unwrap_or((item, ""));
            let model = s.model(mid)?.ok_or_else(|| Error::Other("the model is gone".into()))?;
            let r = s.find_record(&model, rid)?;
            for (k, v) in r.values {
                m.insert(k, v.to_string());
            }
        }
        _ => {}
    }
    Ok(m)
}

fn write(s: &Store, kind: &str, item: &str, doc: &str, text: &str) -> Result<()> {
    match kind {
        "entry" => {
            s.update_entry(item, None, Some(text), None)?;
        }
        "journal" => {
            if s.entry(doc)?.is_some_and(|e| e.journal_id == item) {
                s.update_entry(doc, None, Some(text), None)?;
            }
        }
        "code" => {
            s.update_code_space(item, &CodeChange { source: Some(text.to_string()), ..Default::default() })?;
        }
        _ => {}
    }
    Ok(())
}

fn write_fields(s: &Store, item: &str, fields: &BTreeMap<String, Value>) -> Result<()> {
    let (mid, rid) = item.split_once('/').unwrap_or((item, ""));
    let model = s.model(mid)?.ok_or_else(|| Error::Other("the model is gone".into()))?;
    let r = s.find_record(&model, rid)?;
    let mut changed = serde_json::Map::new();
    for (k, v) in fields {
        if r.values.get(k) != Some(v) {
            changed.insert(k.clone(), v.clone());
        }
    }
    if !changed.is_empty() {
        s.update_record(&model, rid, &changed)?;
    }
    Ok(())
}

/// A session's state, replayed from its log.
#[derive(Default)]
pub struct State {
    pub texts: BTreeMap<String, Text>,
    /// Record fields: field → (lamport, site, value).
    pub fields: BTreeMap<String, (u64, u64, Value)>,
    pub closed: bool,
    pub runs: Vec<super::proto::RunChunk>,
    pub head: i64,
}

impl State {
    pub fn apply(&mut self, doc: &str, op: &Op) {
        match &op.kind {
            Some(Kind::Insert(i)) => {
                self.texts.entry(doc.to_string()).or_default().insert(i);
            }
            Some(Kind::Delete(d)) => self.texts.entry(doc.to_string()).or_default().delete(d),
            Some(Kind::Set(f)) => {
                let v: Value = serde_json::from_str(&f.value_json).unwrap_or(Value::Null);
                let e = self.fields.entry(f.field.clone()).or_insert((0, 0, Value::Null));
                if (f.lamport, f.site) > (e.0, e.1) {
                    *e = (f.lamport, f.site, v);
                }
            }
            Some(Kind::Run(r)) => self.runs.push(r.clone()),
            Some(Kind::Closed(_)) => self.closed = true,
            Some(Kind::Cursor(_)) | None => {}
        }
    }
    pub fn lamport(&self) -> u64 {
        self.fields.values().map(|f| f.0).max().unwrap_or(0)
    }
    /// The visible texts (records: the winning values as JSON).
    pub fn docs(&self) -> BTreeMap<String, String> {
        let mut m: BTreeMap<String, String> = self.texts.iter().map(|(k, t)| (k.clone(), t.text())).collect();
        for (k, f) in &self.fields {
            m.insert(k.clone(), f.2.to_string());
        }
        m
    }
}

pub fn state(s: &Store, session: &str) -> Result<State> {
    let mut st = State::default();
    for o in s.live_ops(session, 0)? {
        st.apply(&o.doc, &Op::decode_bytes(&o.body)?);
        st.head = o.seq;
    }
    Ok(st)
}

/// Start (or reuse) a session hosting an item, inviting `peers`.
pub fn host(s: &Store, origin: &str, me: &str, kind: &str, key: &str, peers: &[String]) -> Result<LiveSession> {
    let (item, title) = resolve(s, kind, key)?;
    if let Some(mut ls) = s.hosted_session(kind, &item, "")? {
        for p in peers {
            if !ls.peers.contains(p) {
                ls.peers.push(p.clone());
            }
        }
        ls.updated_at = now_ms();
        s.put_live_session(&ls)?;
        return Ok(ls);
    }
    let now = now_ms();
    let ls = LiveSession {
        id: crate::store::new_id(),
        kind: kind.into(),
        item_id: item.clone(),
        field: String::new(),
        title,
        role: "host".into(),
        host_peer: String::new(),
        exchange_id: None,
        status: "open".into(),
        seen: 0,
        peers: peers.to_vec(),
        created_at: now,
        updated_at: now,
    };
    s.put_live_session(&ls)?;
    absorb(s, &ls, me)?;
    s.note_action(
        &NewAction::new("live.host", origin, format!("live session on {kind} {}", ls.title))
            .params(json!({ "session": ls.id, "kind": kind, "item": item, "peers": peers })),
    );
    Ok(ls)
}

/// Host: turn changes made to the item outside the session into operations.
pub fn absorb(s: &Store, ls: &LiveSession, me: &str) -> Result<usize> {
    if ls.kind == "run" || ls.status != "open" {
        return Ok(0);
    }
    let mut st = state(s, &ls.id)?;
    let site = site_of(me);
    let mut n = 0;
    for (doc, now) in current(s, &ls.kind, &ls.item_id)? {
        if ls.kind == "record" {
            let v: Value = serde_json::from_str(&now).unwrap_or(Value::Null);
            if st.fields.get(&doc).map(|f| &f.2) != Some(&v) {
                let op = Op::new(Kind::Set(SetField { field: doc.clone(), value_json: now, lamport: st.lamport() + 1, site }));
                st.apply(&doc, &op);
                s.append_live_op(&ls.id, me, &doc, &op.bytes())?;
                n += 1;
            }
            continue;
        }
        let t = st.texts.entry(doc.clone()).or_default();
        if t.text() == now {
            continue;
        }
        let (d, i) = t.local_edit(site, &now);
        for op in [d.map(Kind::Delete), i.map(Kind::Insert)].into_iter().flatten() {
            s.append_live_op(&ls.id, me, &doc, &Op::new(op).bytes())?;
            n += 1;
        }
    }
    Ok(n)
}

/// Host: write the merged texts back to the item.
fn write_back(s: &Store, ls: &LiveSession) -> Result<()> {
    let st = state(s, &ls.id)?;
    let now = current(s, &ls.kind, &ls.item_id)?;
    if ls.kind == "record" {
        let f: BTreeMap<String, Value> = st.fields.iter().map(|(k, v)| (k.clone(), v.2.clone())).collect();
        return write_fields(s, &ls.item_id, &f);
    }
    for (doc, t) in &st.texts {
        let text = t.text();
        if now.get(doc) != Some(&text) {
            write(s, &ls.kind, &ls.item_id, doc, &text)?;
        }
    }
    Ok(())
}

fn check(op: &Op) -> Result<()> {
    let big = match &op.kind {
        Some(Kind::Insert(i)) => i.text.len() > MAX_TEXT || i.id.is_none(),
        Some(Kind::Set(f)) => f.value_json.len() > MAX_TEXT || serde_json::from_str::<Value>(&f.value_json).is_err(),
        Some(Kind::Delete(d)) => d.spans.iter().map(|s| s.len).sum::<u64>() > MAX_TEXT as u64,
        Some(Kind::Cursor(_)) => false,
        // only the host closes or streams runs
        Some(Kind::Run(_)) | Some(Kind::Closed(_)) | None => true,
    };
    if big { Err(Error::invalid("live operation", "not allowed or too large")) } else { Ok(()) }
}

/// Host: a guest's batch. Checks the guest was invited, applies its operations (each must
/// be its own site's), writes the result back and answers with what the guest lacks.
pub fn receive(s: &Store, me: &str, peer: &str, label: &str, b: &Batch) -> Result<(Batch, Value)> {
    let ls = s.live_session(&b.session)?.filter(|l| l.role == "host").ok_or_else(|| Error::Other("no such live session here".into()))?;
    if !ls.peers.iter().any(|p| p == peer) {
        return Err(Error::Other("you were not invited to this live session".into()));
    }
    if b.frames.len() > MAX_FRAMES {
        return Err(Error::invalid("live batch", format!("at most {MAX_FRAMES} operations")));
    }
    let meta = json!({ "kind": ls.kind, "title": ls.title, "status": ls.status, "host": me });
    let site = site_of(peer);
    let mut wrote = false;
    if ls.status == "open" {
        absorb(s, &ls, me)?;
        for f in &b.frames {
            let op = f.op.clone().unwrap_or_default();
            if let Some(Kind::Cursor(c)) = &op.kind {
                s.set_presence(&ls.id, peer, if c.label.is_empty() { label } else { &c.label }, c.offset)?;
                continue;
            }
            check(&op)?;
            let own = match &op.kind {
                Some(Kind::Insert(i)) => i.id.is_some_and(|id| id.site == site),
                Some(Kind::Set(x)) => x.site == site,
                _ => true,
            };
            if !own || (ls.kind == "record") != matches!(op.kind, Some(Kind::Set(_))) {
                return Err(Error::invalid("live operation", "not this node's, or not for this kind of item"));
            }
            s.append_live_op(&ls.id, peer, &f.doc, &op.bytes())?;
            wrote = true;
        }
        if wrote {
            write_back(s, &ls)?;
            note_edit(s, &ls, peer, label);
        }
        s.set_presence(
            &ls.id,
            peer,
            label,
            b.frames.iter().rev().find_map(|f| match &f.op.as_ref()?.kind {
                Some(Kind::Cursor(c)) => c.offset,
                _ => None,
            }),
        )?;
    }
    let frames = s.live_ops(&ls.id, b.since as i64)?.into_iter().map(frame).collect::<Result<Vec<_>>>()?;
    let present = s.presence(&ls.id, PRESENCE_MS)?.into_iter().map(|p| Cursor { label: p.label, offset: p.cursor }).collect();
    Ok((Batch { session: ls.id.clone(), since: b.since, frames, head: s.live_head(&ls.id)? as u64, present }, meta))
}

fn note_edit(s: &Store, ls: &LiveSession, peer: &str, label: &str) {
    let recent = s
        .actions_of_kind("live.edit")
        .unwrap_or_default()
        .into_iter()
        .rev()
        .take(20)
        .any(|a| a.params["session"] == ls.id && a.params["peer"] == peer && now_ms() - a.created_at < COALESCE_MS);
    if !recent {
        s.note_action(&NewAction::new("live.edit", "network", format!("{label} edited {} {} live", ls.kind, ls.title)).params(json!({
            "session": ls.id, "kind": ls.kind, "item": ls.item_id, "peer": peer, "from": label,
        })));
    }
}

fn frame(o: LiveOp) -> Result<Frame> {
    Ok(Frame { seq: o.seq as u64, site: site_of(&o.site), op: Some(Op::decode_bytes(&o.body)?), doc: o.doc })
}

/// Host: close a session (guests see it on their next sync).
pub fn stop(s: &Store, origin: &str, ls: &LiveSession, me: &str) -> Result<()> {
    if ls.role == "host" && ls.status == "open" {
        absorb(s, ls, me)?;
        s.append_live_op(&ls.id, me, "", &Op::new(Kind::Closed(true)).bytes())?;
    }
    s.set_live_session(&ls.id, Some("closed"), None)?;
    s.note_action(
        &NewAction::new("live.stop", origin, format!("live session on {} {} closed", ls.kind, ls.title)).params(json!({ "session": ls.id })),
    );
    Ok(())
}

/// Guest: record a session joined on `host` (over exchange `x`).
pub fn joined(s: &Store, session: &str, host: &str, exchange: &str, meta: &Value) -> Result<LiveSession> {
    let now = now_ms();
    let st = |k: &str| meta.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let ls = LiveSession {
        id: session.into(),
        kind: st("kind"),
        item_id: String::new(),
        field: String::new(),
        title: st("title"),
        role: "guest".into(),
        host_peer: host.into(),
        exchange_id: Some(exchange.into()),
        status: if st("status") == "closed" { "closed".into() } else { "open".into() },
        seen: s.live_session(session)?.map(|l| l.seen).unwrap_or(0),
        peers: vec![],
        created_at: now,
        updated_at: now,
    };
    s.put_live_session(&ls)?;
    Ok(ls)
}

/// Guest: what to send for local edits (`doc` → the whole new text, or a record field's
/// JSON value) and a cursor, given the state now.
pub fn edits(st: &State, me: &str, label: &str, kind: &str, changes: &BTreeMap<String, String>, cursor: Option<i64>) -> Result<Vec<Frame>> {
    let site = site_of(me);
    let mut out = vec![];
    let mut lam = st.lamport();
    for (doc, new) in changes {
        if kind == "record" {
            let v: Value = serde_json::from_str(new).map_err(|e| Error::invalid("value", format!("{doc}: {e}")))?;
            if st.fields.get(doc).map(|f| &f.2) == Some(&v) {
                continue;
            }
            lam += 1;
            let op = Op::new(Kind::Set(SetField { field: doc.clone(), value_json: v.to_string(), lamport: lam, site }));
            out.push(Frame { seq: 0, site, op: Some(op), doc: doc.clone() });
            continue;
        }
        let mut t = st.texts.get(doc).cloned().unwrap_or_default();
        let (d, i) = t.local_edit(site, new);
        for k in [d.map(Kind::Delete), i.map(Kind::Insert)].into_iter().flatten() {
            out.push(Frame { seq: 0, site, op: Some(Op::new(k)), doc: doc.clone() });
        }
    }
    out.push(Frame { seq: 0, site, op: Some(Op::new(Kind::Cursor(Cursor { label: label.into(), offset: cursor }))), doc: String::new() });
    Ok(out)
}

/// Guest: one round with the host: send `frames`, store what comes back; the new state.
pub fn sync_with(
    s: &Store,
    ls: &LiveSession,
    frames: Vec<Frame>,
    send: impl FnOnce(Batch) -> Result<(Batch, Value)>,
) -> Result<(State, Vec<Cursor>)> {
    let since = s.live_head(&ls.id)?;
    let (b, meta) = send(Batch { session: ls.id.clone(), since: since as u64, frames, head: 0, present: vec![] })?;
    for f in &b.frames {
        let op = f.op.clone().unwrap_or_default();
        s.put_live_op(&ls.id, &LiveOp { seq: f.seq as i64, site: f.site.to_string(), doc: f.doc.clone(), body: op.bytes(), created_at: now_ms() })?;
    }
    let st = state(s, &ls.id)?;
    let status = if st.closed || meta.get("status").and_then(|v| v.as_str()) == Some("closed") { "closed" } else { "open" };
    s.set_live_session(&ls.id, Some(status), Some(st.head))?;
    Ok((st, b.present))
}

/// Host: edits made through the session (the CLI or TUI) rather than to the item.
pub fn host_edit(s: &Store, ls: &LiveSession, me: &str, changes: &BTreeMap<String, String>) -> Result<State> {
    if ls.role != "host" || ls.status != "open" {
        return Err(Error::Other("this session is not open here".into()));
    }
    absorb(s, ls, me)?;
    let st = state(s, &ls.id)?;
    for f in edits(&st, me, "", &ls.kind, changes, None)? {
        let op = f.op.unwrap_or_default();
        if !matches!(op.kind, Some(Kind::Cursor(_))) {
            s.append_live_op(&ls.id, me, &f.doc, &op.bytes())?;
        }
    }
    write_back(s, ls)?;
    state(s, &ls.id)
}

/// The document an edit means when none is named: an entry's body, a code space's source,
/// or the only one there is.
pub fn default_doc(kind: &str, docs: &BTreeMap<String, String>) -> Option<String> {
    match kind {
        "entry" => Some("body".into()),
        "code" => Some("source".into()),
        _ if docs.len() == 1 => docs.keys().next().cloned(),
        _ => None,
    }
}

/// The open sessions this node hosts that invite `peer` (the peers view's offers).
pub fn offers(s: &Store, peer: &str) -> Result<Vec<Value>> {
    Ok(s.live_sessions()?
        .into_iter()
        .filter(|l| l.role == "host" && l.status == "open" && l.kind != "run" && l.peers.iter().any(|p| p == peer))
        .map(|l| json!({ "session": l.id, "kind": l.kind, "title": l.title }))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::EntryFormat;

    #[test]
    fn host_and_guests_merge_concurrent_edits_into_the_entry() {
        let host_s = Store::open_in_memory().unwrap();
        let j = host_s.create_journal("Lab", None, EntryFormat::Markdown).unwrap();
        let e = host_s.add_entry(&j.id, "Plan", "buy low", None).unwrap();
        let ls = host(&host_s, "cli", "H", "entry", &e.id, &["G1".into(), "G2".into()]).unwrap();
        let (g1, g2) = (Store::open_in_memory().unwrap(), Store::open_in_memory().unwrap());
        let send = |peer: &'static str, label: &'static str| {
            let h = &host_s;
            move |b: Batch| receive(h, "H", peer, label, &b)
        };
        // both join
        let l1 = joined(&g1, &ls.id, "H", "x1", &json!({"kind": "entry", "title": "Plan"})).unwrap();
        let l2 = joined(&g2, &ls.id, "H", "x2", &json!({"kind": "entry", "title": "Plan"})).unwrap();
        let (st1, _) = sync_with(&g1, &l1, vec![], send("G1", "ana")).unwrap();
        let (st2, _) = sync_with(&g2, &l2, vec![], send("G2", "ben")).unwrap();
        assert_eq!(st1.docs()["body"], "buy low");
        // concurrent edits from both guests, and the host edits the entry directly
        let f1 = edits(&st1, "G1", "ana", "entry", &BTreeMap::from([("body".into(), "buy low, sell high".into())]), Some(18)).unwrap();
        let f2 = edits(&st2, "G2", "ben", "entry", &BTreeMap::from([("body".into(), "always buy low".into())]), Some(6)).unwrap();
        host_s.update_entry(&e.id, None, Some("buy low!"), None).unwrap();
        sync_with(&g1, &l1, f1, send("G1", "ana")).unwrap();
        let (_, present) = sync_with(&g2, &l2, f2, send("G2", "ben")).unwrap();
        let (st1, _) = sync_with(&g1, &l1, vec![], send("G1", "ana")).unwrap();
        let body = host_s.entry(&e.id).unwrap().unwrap().body;
        assert_eq!(st1.docs()["body"], body, "guest and host agree");
        for part in ["always ", "buy low", ", sell high", "!"] {
            assert!(body.contains(part), "{part} kept: {body}");
        }
        assert!(present.iter().any(|c| c.label == "ana") && present.iter().any(|c| c.label == "ben" && c.offset == Some(6)));
        // strangers are refused; the host closes it
        assert!(receive(&host_s, "H", "X", "eve", &Batch { session: ls.id.clone(), ..Default::default() }).is_err());
        stop(&host_s, "cli", &host_s.live_session(&ls.id).unwrap().unwrap(), "H").unwrap();
        let (st1, _) = sync_with(&g1, &l1, vec![], send("G1", "ana")).unwrap();
        assert!(st1.closed);
        assert_eq!(g1.live_session(&ls.id).unwrap().unwrap().status, "closed");
        assert_eq!(host_s.actions_of_kind("live.edit").unwrap().len(), 2, "one per guest, coalesced");
    }

    #[test]
    fn guests_cannot_forge_or_close() {
        let s = Store::open_in_memory().unwrap();
        let j = s.create_journal("Lab", None, EntryFormat::Markdown).unwrap();
        let e = s.add_entry(&j.id, "T", "x", None).unwrap();
        let ls = host(&s, "cli", "H", "entry", &e.id, &["G".into()]).unwrap();
        let close = Frame { seq: 0, site: site_of("G"), op: Some(Op::new(Kind::Closed(true))), doc: String::new() };
        assert!(receive(&s, "H", "G", "g", &Batch { session: ls.id.clone(), frames: vec![close], ..Default::default() }).is_err());
        // an insert claiming the host's site
        let st = state(&s, &ls.id).unwrap();
        let forged = edits(&st, "H", "g", "entry", &BTreeMap::from([("body".into(), "xy".into())]), None).unwrap();
        assert!(receive(&s, "H", "G", "g", &Batch { session: ls.id.clone(), frames: forged, ..Default::default() }).is_err());
    }
}
