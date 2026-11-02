//! `mnnn msg`, `mnnn live` and `mnnn peers` (2.19). Thin over [`crate::collab`]; every peer
//! is reached through [`crate::collab::wire`].
use super::net::Net;
use super::{Ctx, LiveCommand, MsgCommand, PeersArgs, PeersCommand};
use crate::collab::{msg as cmsg, peers as cpeers, run as crun, session, stream, wire};
use crate::ingest::time::fmt_ts;
use crate::store::{LiveSession, Message, Store};
use anyhow::{anyhow, bail};
use std::collections::BTreeMap;

const ORIGIN: &str = "cli";

fn me(n: &Net) -> String {
    n.kp.public().to_peer_id().to_string()
}

/// A peer argument as a peer id (an active connection, else taken as given).
fn peer_id(s: &Store, key: &str) -> String {
    wire::exchange_to(s, key, None).ok().and_then(|x| x.peer_id).unwrap_or_else(|| key.to_string())
}

fn msg_line(m: &Message) -> String {
    let mark = if m.direction == "in" && m.read_at.is_none() { "●" } else { " " };
    let arrow = if m.direction == "in" { "←" } else { "→" };
    let state = match (&m.direction[..], m.delivered_at, &m.error) {
        ("out", None, Some(e)) => format!("\t(waiting: {e})"),
        ("out", None, None) => "\t(waiting)".into(),
        _ => String::new(),
    };
    let about = m.about.as_deref().map(|a| format!(" [{a}]")).unwrap_or_default();
    format!("{mark} {}\t{arrow} {}{about}\t{}{state}", fmt_ts(m.created_at), m.peer_label, m.text)
}

pub fn msg(ctx: &Ctx, cmd: MsgCommand) -> anyhow::Result<()> {
    match cmd {
        MsgCommand::Send { peer, text, about, json } => {
            let mut n = Net::open(ctx)?;
            let relays = n.relays();
            let x = wire::exchange_to(&n.store, &peer, None)?;
            let (me, label) = (me(&n), n.label.clone());
            n.node()?;
            let t = wire::open(&n.store, n.node.as_ref().expect("node"), &x, &relays, stream::mode(&n.prefs.net));
            let m = cmsg::send(&n.store, t.as_ref(), &x, &me, &label, about.as_deref(), &text, ORIGIN)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&m)?);
            } else if m.delivered_at.is_some() {
                ctx.io.info(format!("delivered to {}", x.title()));
            } else {
                ctx.io.warn(format!(
                    "kept: {} is not reachable now ({}); the running node keeps trying",
                    x.title(),
                    m.error.as_deref().unwrap_or("no answer")
                ));
            }
        }
        MsgCommand::List { peer, about, unread, json } => {
            let s = ctx.open_store()?;
            let p = peer.as_deref().map(|k| peer_id(&s, k));
            let v: Vec<Message> =
                s.messages(p.as_deref(), about.as_deref())?.into_iter().filter(|m| !unread || (m.direction == "in" && m.read_at.is_none())).collect();
            if json {
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else if v.is_empty() {
                ctx.io.info("no messages");
            } else {
                for m in &v {
                    println!("{}", msg_line(m));
                }
            }
        }
        MsgCommand::Read { peer } => {
            let s = ctx.open_store()?;
            let p = peer.as_deref().map(|k| peer_id(&s, k));
            let k = s.mark_messages_read(p.as_deref())?;
            ctx.io.info(format!("{k} marked read"));
        }
    }
    Ok(())
}

fn session_line(l: &LiveSession) -> String {
    let with = if l.role == "host" { format!("{} invited", l.peers.len()) } else { format!("host {}", short(&l.host_peer)) };
    format!("{}\t{}\t{}\t{}\t{}\t{with}", &l.id[..l.id.len().min(8)], l.role, l.status, l.kind, l.title)
}

fn short(p: &str) -> &str {
    &p[p.len().saturating_sub(8)..]
}

/// A guest's round with its host: send `frames`, return the state.
fn guest_sync(
    n: &mut Net,
    ls: &LiveSession,
    frames: Vec<crate::collab::proto::Frame>,
) -> anyhow::Result<(session::State, Vec<crate::collab::proto::Cursor>, &'static str)> {
    let relays = n.relays();
    let xid = ls.exchange_id.clone().ok_or_else(|| anyhow!("this session has no connection to its host"))?;
    let x = n.store.exchange(&xid)?.ok_or_else(|| anyhow!("the connection to the host is gone"))?;
    n.node()?;
    let t = wire::open(&n.store, n.node.as_ref().expect("node"), &x, &relays, stream::mode(&n.prefs.net));
    let (st, here) = session::sync_with(&n.store, ls, frames, |b| t.live(b))?;
    Ok((st, here, t.name()))
}

pub fn live(ctx: &Ctx, cmd: LiveCommand) -> anyhow::Result<()> {
    match cmd {
        LiveCommand::Share { kind, key, with } => {
            let n = Net::open(ctx)?;
            let mut peers = vec![];
            for w in &with {
                let x = wire::exchange_to(&n.store, w, None)?;
                peers.push(x.peer_id.expect("active"));
            }
            let ls = session::host(&n.store, ORIGIN, &me(&n), &kind, &key, &peers)?;
            ctx.io.info(format!("hosting {} {} live as {} (the running node answers guests)", ls.kind, ls.title, &ls.id[..8]));
            println!("{}", ls.id);
        }
        LiveCommand::List { json } => {
            let s = ctx.open_store()?;
            let v: Vec<LiveSession> = s.live_sessions()?.into_iter().filter(|l| l.kind != "run").collect();
            if json {
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else if v.is_empty() {
                ctx.io.info("no live sessions");
            } else {
                for l in &v {
                    println!("{}", session_line(l));
                }
            }
        }
        LiveCommand::Join { peer, session: id } => {
            let mut n = Net::open(ctx)?;
            let x = wire::exchange_to(&n.store, &peer, None)?;
            let relays = n.relays();
            n.node()?;
            let t = wire::open(&n.store, n.node.as_ref().expect("node"), &x, &relays, stream::mode(&n.prefs.net));
            let offers = cpeers::offers(t.as_ref())?;
            let Some(id) = offers["live"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|o| o["session"].as_str())
                .find(|s| s.starts_with(id.as_str()))
                .map(String::from)
            else {
                bail!("{} has not invited you to a live session `{id}` (see `mnnn peers show {peer}`)", x.title());
            };
            drop(t);
            let ls = session::joined(&n.store, &id, x.peer_id.as_deref().unwrap_or_default(), &x.id, &serde_json::json!({}))?;
            let (st, present, via) = guest_sync(&mut n, &ls, vec![])?;
            let ls = n.store.live_session(&id)?.unwrap_or(ls);
            ctx.io.info(format!("joined {} ({} document(s), {} here, over {via})", ls.title, st.docs().len(), present.len()));
        }
        LiveCommand::Show { session: key, json } => {
            let mut n = Net::open(ctx)?;
            let ls = n.store.find_live_session(&key)?;
            let (st, present, via) = if ls.role == "guest" && ls.status == "open" {
                guest_sync(&mut n, &ls, vec![])?
            } else {
                session::absorb(&n.store, &ls, &me(&n))?;
                let p = n.store.presence(&ls.id, session::PRESENCE_MS)?;
                (
                    session::state(&n.store, &ls.id)?,
                    p.into_iter().map(|p| crate::collab::proto::Cursor { label: p.label, offset: p.cursor }).collect(),
                    "local",
                )
            };
            let ls = n.store.live_session(&ls.id)?.unwrap_or(ls);
            if json {
                let here: Vec<_> = present.iter().map(|c| serde_json::json!({ "label": c.label, "cursor": c.offset })).collect();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({ "session": ls, "docs": st.docs(), "present": here, "transport": via }))?
                );
            } else {
                println!("{}", session_line(&ls));
                for c in &present {
                    println!("  here: {}{}", c.label, c.offset.map(|o| format!(" at {o}")).unwrap_or_default());
                }
                for (doc, text) in st.docs() {
                    println!("--- {doc}\n{text}");
                }
            }
        }
        LiveCommand::Edit { session: key, doc, value, cursor } => {
            let mut n = Net::open(ctx)?;
            let ls = n.store.find_live_session(&key)?;
            if ls.status != "open" || ls.kind == "run" {
                bail!("this session is closed");
            }
            let st = if ls.role == "guest" { guest_sync(&mut n, &ls, vec![])?.0 } else { session::state(&n.store, &ls.id)? };
            let docs = st.docs();
            let doc = match doc.or_else(|| session::default_doc(&ls.kind, &docs)) {
                Some(d) => d,
                None => bail!("name the document: {}", docs.keys().cloned().collect::<Vec<_>>().join(", ")),
            };
            if ls.kind == "journal" && !docs.contains_key(&doc) {
                bail!("`{doc}` is not an entry of this journal (live sessions edit existing entries)");
            }
            let old = docs.get(&doc).cloned().unwrap_or_default();
            let new = match value {
                Some(v) => v,
                None => super::code::edit_text(&old, if ls.kind == "record" { "json" } else { "md" })?,
            };
            let changes = BTreeMap::from([(doc.clone(), new)]);
            let me = me(&n);
            if ls.role == "host" {
                session::host_edit(&n.store, &ls, &me, &changes)?;
            } else {
                let frames = session::edits(&st, &me, &n.label.clone(), &ls.kind, &changes, cursor)?;
                guest_sync(&mut n, &ls, frames)?;
            }
            ctx.io.info(format!("edited {doc} of {}", ls.title));
        }
        LiveCommand::Watch { session: key, json } => watch(ctx, &key, json)?,
        LiveCommand::Stop { session: key } => {
            let n = Net::open(ctx)?;
            let ls = n.store.find_live_session(&key)?;
            if ls.role == "host" {
                session::stop(&n.store, ORIGIN, &ls, &me(&n))?;
                ctx.io.info(format!("closed {}", ls.title));
            } else {
                n.store.set_live_session(&ls.id, Some("closed"), None)?;
                ctx.io.info(format!("left {}", ls.title));
            }
        }
    }
    Ok(())
}

/// `mnnn live watch`: print the session at every change until it closes.
fn watch(ctx: &Ctx, key: &str, json: bool) -> anyhow::Result<()> {
    let mut n = Net::open(ctx)?;
    let ls = n.store.find_live_session(key)?;
    let show = |st: &session::State, here: &[crate::collab::proto::Cursor], via: &str| {
        if json {
            let here: Vec<_> = here.iter().map(|c| serde_json::json!({ "label": c.label, "cursor": c.offset })).collect();
            println!("{}", serde_json::json!({ "head": st.head, "docs": st.docs(), "present": here, "closed": st.closed, "transport": via }));
        } else {
            let who: Vec<String> = here.iter().map(|c| format!("{}{}", c.label, c.offset.map(|o| format!(" at {o}")).unwrap_or_default())).collect();
            println!("── #{} · here: {} · {via}", st.head, if who.is_empty() { "-".into() } else { who.join(", ") });
            for (doc, text) in st.docs() {
                println!("--- {doc}\n{text}");
            }
        }
        use std::io::Write;
        let _ = std::io::stdout().flush();
        true
    };
    if ls.role == "host" {
        let me = me(&n);
        let mut last = -1;
        loop {
            session::absorb(&n.store, &ls, &me)?;
            let st = session::state(&n.store, &ls.id)?;
            let here: Vec<_> = n
                .store
                .presence(&ls.id, session::PRESENCE_MS)?
                .into_iter()
                .map(|p| crate::collab::proto::Cursor { label: p.label, offset: p.cursor })
                .collect();
            if st.head != last {
                last = st.head;
                show(&st, &here, "local");
            }
            if st.closed {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }
    let relays = n.relays();
    let xid = ls.exchange_id.clone().ok_or_else(|| anyhow!("this session has no connection to its host"))?;
    let x = n.store.exchange(&xid)?.ok_or_else(|| anyhow!("the connection to the host is gone"))?;
    n.node()?;
    let t = wire::open(&n.store, n.node.as_ref().expect("node"), &x, &relays, stream::mode(&n.prefs.net));
    let via = t.name();
    t.follow(&n.store, &ls, &mut |st, here| show(st, here, via))?;
    Ok(())
}

pub fn peers(ctx: &Ctx, a: PeersArgs) -> anyhow::Result<()> {
    match a.cmd {
        None => {
            let s = ctx.open_store()?;
            let v = cpeers::list(&s)?;
            if a.json {
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else if v.is_empty() {
                ctx.io.info("no peers yet (exchange keys with `mnnn connection` or `mnnn net exchange`)");
            } else {
                for p in &v {
                    let unread = if p.unread > 0 { format!("\t{} unread", p.unread) } else { String::new() };
                    let sites = if p.sites.is_empty() { String::new() } else { format!("\t{} site(s)", p.sites.len()) };
                    println!("{}\t{}\t{}{sites}{unread}", p.label, short(&p.peer), p.scopes.join(","));
                }
            }
        }
        Some(PeersCommand::Show { peer, json }) => {
            let mut n = Net::open(ctx)?;
            let relays = n.relays();
            let x = wire::exchange_to(&n.store, &peer, None)?;
            n.node()?;
            let node = n.node.as_ref().expect("node");
            let shares = match crate::net::client::send(&n.store, node, &x, crate::net::proto::Req::List, &relays) {
                Ok(crate::net::proto::Resp::Shared { items }) => serde_json::to_value(items)?,
                _ => serde_json::json!([]),
            };
            let t = wire::open(&n.store, node, &x, &relays, stream::mode(&n.prefs.net));
            let offers = cpeers::offers(t.as_ref())?;
            let pid = x.peer_id.clone().unwrap_or_default();
            let sites = cpeers::list(&n.store)?.into_iter().find(|p| p.peer == pid).map(|p| p.sites).unwrap_or_default();
            let v = serde_json::json!({ "peer": pid, "label": x.title(), "shares": shares, "live": offers["live"], "executables": offers["executables"], "sites": sites });
            if json || a.json {
                println!("{}", serde_json::to_string_pretty(&v)?);
                return Ok(());
            }
            println!("{} ({})", x.title(), short(&pid));
            let list = |title: &str, items: &serde_json::Value, f: &dyn Fn(&serde_json::Value) -> String| {
                let items = items.as_array().cloned().unwrap_or_default();
                if !items.is_empty() {
                    println!("{title}:");
                    for i in &items {
                        println!("  {}", f(i));
                    }
                }
            };
            let st = |i: &serde_json::Value, k: &str| i.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
            list("shared", &v["shares"], &|i| {
                format!("{}\t{}\t(open: mnnn net fetch {} {})", st(i, "kind"), st(i, "name"), x.title(), st(i, "name"))
            });
            list("live", &v["live"], &|i| {
                format!("{}\t{}\t(join: mnnn live join {} {})", st(i, "kind"), st(i, "title"), x.title(), st(i, "session"))
            });
            list("executables", &v["executables"], &|i| {
                format!(
                    "{} {}\t{}\t(run: mnnn peers run {} {} {})",
                    st(i, "org"),
                    st(i, "endpoint"),
                    st(i, "kind"),
                    x.title(),
                    st(i, "org"),
                    st(i, "endpoint")
                )
            });
            list("sites", &v["sites"], &|i| {
                format!("{}\t{}", st(i, "name"), i["urls"].as_array().and_then(|u| u.first()).and_then(|u| u.as_str()).unwrap_or(""))
            });
        }
        Some(PeersCommand::Run { peer, org, endpoint, input }) => {
            let input: serde_json::Value = serde_json::from_str(&input).map_err(|e| anyhow!("--input: {e}"))?;
            let mut n = Net::open(ctx)?;
            let relays = n.relays();
            let x = wire::exchange_to(&n.store, &peer, Some(&org))?;
            n.node()?;
            let t = wire::open(&n.store, n.node.as_ref().expect("node"), &x, &relays, stream::mode(&n.prefs.net));
            let host = x.peer_id.clone().unwrap_or_default();
            let mut got = 0usize;
            let out = crun::run(&n.store, t.as_ref(), &host, &x.id, &org, &endpoint, input, |c| {
                got += c.json.len();
                if !ctx.global.quiet && !c.done {
                    eprintln!("… {got} bytes");
                }
            })?;
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        Some(PeersCommand::Request { peer, text }) => {
            let mut n = Net::open(ctx)?;
            let relays = n.relays();
            let x = wire::exchange_to(&n.store, &peer, None)?;
            let (me, label) = (me(&n), n.label.clone());
            n.node()?;
            let t = wire::open(&n.store, n.node.as_ref().expect("node"), &x, &relays, stream::mode(&n.prefs.net));
            let m = cmsg::send(&n.store, t.as_ref(), &x, &me, &label, Some("request:peers"), &text, ORIGIN)?;
            if m.delivered_at.is_some() {
                ctx.io.info(format!("asked {}", x.title()));
            } else {
                ctx.io.warn(format!("kept: {} is not reachable now; the running node keeps trying", x.title()));
            }
        }
    }
    Ok(())
}
