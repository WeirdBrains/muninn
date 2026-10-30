//! `mnnn user`, `mnnn connection` and `mnnn invite` (2.10). Thin over [`crate::net::user`]
//! and [`crate::net::auto`].
use super::net::{Net, print_link};
use super::{ConnectionCommand, Ctx, InviteCommand, OrgCommand, OrgMemberCommand, ReviewCommand, UserCommand};
use crate::ingest::time::fmt_local;
use crate::net::{self, auto, client, user};
use crate::store::ExchangeStatus;
use crate::store::users::User;
use anyhow::anyhow;

const ORIGIN: &str = "cli";

fn short(p: &str) -> &str {
    crate::store::net::short_peer(p)
}

pub fn user(ctx: &Ctx, cmd: UserCommand) -> anyhow::Result<()> {
    match cmd {
        UserCommand::Show { json } => {
            let n = Net::open(ctx)?;
            let (u, _) = user::load_or_create(&n.store)?;
            let nodes = n.store.user_nodes(&u.id)?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &serde_json::json!({ "id": u.id, "label": u.label, "nodes": nodes.iter().filter(|x| x.active()).count() })
                    )?
                );
                return Ok(());
            }
            println!("user:   {} ({})", u.name(), u.id);
            println!("nodes:  {} linked, {} revoked", nodes.iter().filter(|x| x.active()).count(), nodes.iter().filter(|x| !x.active()).count());
            Ok(())
        }
        UserCommand::List { json } => {
            let n = Net::open(ctx)?;
            user::load_or_create(&n.store)?;
            let users = n.store.users()?;
            if json {
                let v: Vec<_> = users
                    .iter()
                    .map(|u| {
                        let c = n.store.connection(&u.id).ok().flatten();
                        serde_json::json!({ "id": u.id, "label": u.label, "mine": u.mine, "connection": c.is_some(), "trusted": c.is_some_and(|c| c.trusted) })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&v)?);
                return Ok(());
            }
            for u in &users {
                let what = if u.mine {
                    "you".to_string()
                } else {
                    match n.store.connection(&u.id)? {
                        Some(c) if c.trusted => "trusted".into(),
                        Some(_) => "untrusted".into(),
                        None => "known".into(),
                    }
                };
                println!("{}\t{}\t{}", u.short_id(), u.name(), what);
            }
            Ok(())
        }
        UserCommand::Label { name } => {
            let n = Net::open(ctx)?;
            let (u, _) = user::load_or_create(&n.store)?;
            n.store.label_user(&u.id, name.trim())?;
            ctx.io.info(format!("you are now {} (peers see it the next time they talk)", name.trim()));
            Ok(())
        }
        UserCommand::Nodes { user: who, json } => {
            let n = Net::open(ctx)?;
            let (me, _) = user::load_or_create(&n.store)?;
            let u: User = match who {
                Some(k) => n.store.find_user(&k)?,
                None => me,
            };
            let here = net::identity::peer_id(&n.store)?.map(|p| p.to_string()).unwrap_or_default();
            let nodes = n.store.user_nodes(&u.id)?;
            if json {
                let v: Vec<_> = nodes
                    .iter()
                    .map(|x| serde_json::json!({ "peer": x.peer_id, "label": x.label, "linked_at": x.issued_at, "revoked_at": x.revoked_at, "this": x.peer_id == here }))
                    .collect();
                println!("{}", serde_json::to_string_pretty(&v)?);
                return Ok(());
            }
            for x in &nodes {
                let state = match x.revoked_at {
                    Some(t) => format!("revoked {}", fmt_local(t)),
                    None if x.peer_id == here => "this node".into(),
                    None => format!("linked {}", fmt_local(x.issued_at)),
                };
                println!("{}\t{}\t{}", short(&x.peer_id), if x.label.is_empty() { "-" } else { &x.label }, state);
            }
            Ok(())
        }
        UserCommand::Link { target } => {
            let mut n = Net::open(ctx)?;
            let relays = n.relays();
            n.node()?;
            let addrs = n.addrs();
            let l = auto::link_to(&n.store, n.node.as_ref().expect("node"), &n.kp, &target, &n.label, addrs, &relays, ORIGIN)?;
            print_link(ctx, &l);
            if l.status == auto::LinkStatus::Failed {
                return Err(anyhow!("{}", l.detail));
            }
            Ok(())
        }
        UserCommand::Approve { request } => {
            let mut n = Net::open(ctx)?;
            let links: Vec<_> = auto::requests(&n.store)?.into_iter().filter(|x| net::exchange::scope_kind(&x.scope) == "link").collect();
            let x = match &request {
                Some(k) => {
                    let x = auto::incoming(&n.store, k)?;
                    if net::exchange::scope_kind(&x.scope) != "link" {
                        return Err(anyhow!("{} asks to connect, not to link (use `mnnn invite accept`)", x.title()));
                    }
                    x
                }
                None => match links.as_slice() {
                    [x] => x.clone(),
                    [] => return Err(anyhow!("no node is asking to link to you")),
                    _ => return Err(anyhow!("{} nodes ask to link; name one (see `mnnn invite list`)", links.len())),
                },
            };
            if !ctx.io.confirm(&format!("Let {} act as you (it gets your user key)?", x.title()))? {
                return Err(anyhow!("cancelled"));
            }
            n.node()?;
            let addrs = n.addrs();
            let l = auto::accept(&n.store, n.node.as_ref(), &n.kp, &x.id, &n.label, addrs, ORIGIN)?;
            ctx.io.info(format!("approved: {} links to you once it asks again (its node does this on its own)", l.peer));
            Ok(())
        }
        UserCommand::Revoke { node } => {
            let n = Net::open(ctx)?;
            if !ctx.io.confirm(&format!("Revoke node {node}? It no longer acts for you; this cannot be undone."))? {
                return Err(anyhow!("cancelled"));
            }
            let x = user::revoke_node(&n.store, &node, ORIGIN)?;
            // end the link exchange with it, if any
            if let Ok(Some(l)) = auto::kind_link(&n.store, &x.peer_id, "link") {
                let _ = client::revoke(&n.store, None, &l.id, &[], ORIGIN);
            }
            ctx.io.info(format!("revoked {}; peers learn it the next time they talk", if x.label.is_empty() { short(&x.peer_id) } else { &x.label }));
            Ok(())
        }
    }
}

pub fn connection(ctx: &Ctx, cmd: ConnectionCommand) -> anyhow::Result<()> {
    match cmd {
        ConnectionCommand::List { json } => {
            let n = Net::open(ctx)?;
            let cs = n.store.connections()?;
            let rows: Vec<(User, bool, usize, i64)> = cs
                .iter()
                .filter_map(|c| {
                    let u = n.store.user(&c.user_id).ok().flatten()?;
                    let nodes = n.store.user_nodes(&u.id).ok()?.iter().filter(|x| x.active()).count();
                    Some((u, c.trusted, nodes, c.created_at))
                })
                .collect();
            if json {
                let v: Vec<_> = rows
                    .iter()
                    .map(|(u, t, k, at)| serde_json::json!({ "user": u.id, "label": u.label, "trusted": t, "nodes": k, "since": at }))
                    .collect();
                println!("{}", serde_json::to_string_pretty(&v)?);
                return Ok(());
            }
            for (u, t, k, at) in &rows {
                println!("{}\t{}\t{}\t{} node(s)\tsince {}", u.short_id(), u.name(), if *t { "trusted" } else { "untrusted" }, k, fmt_local(*at));
            }
            if rows.is_empty() {
                ctx.io.info("no connections yet; connect with `mnnn invite create` / `mnnn invite accept LINK`");
            }
            Ok(())
        }
        ConnectionCommand::Trust { user: who, off } => {
            let n = Net::open(ctx)?;
            let (u, changed) = user::set_trust(&n.store, &who, !off, ORIGIN)?;
            let word = if off { "untrusted" } else { "trusted" };
            ctx.io.info(if changed { format!("{} is now {word}", u.name()) } else { format!("{} was already {word}", u.name()) });
            Ok(())
        }
        ConnectionCommand::Remove { user: who } => {
            let n = Net::open(ctx)?;
            let u = user::remove_connection(&n.store, &who, ORIGIN)?;
            ctx.io.info(format!("connection with {} removed", u.name()));
            Ok(())
        }
    }
}

pub fn invite(ctx: &Ctx, cmd: InviteCommand) -> anyhow::Result<()> {
    match cmd {
        InviteCommand::Create { label, org: Some(org), role, peer, .. } => {
            super::org::run(ctx, OrgCommand::Member(OrgMemberCommand::Invite { org, peer, label, role }))
        }
        InviteCommand::Create { label, review: Some(review), peer, .. } => super::review::run(ctx, ReviewCommand::Invite { review, label, peer }),
        InviteCommand::Create { label, peer: Some(peer), .. } => {
            super::net::run(ctx, super::NetCommand::Connect { target: peer, label: Some(label).filter(|l| !l.is_empty()), hello: false })
        }
        InviteCommand::Create { label, json, .. } => {
            super::net::run(ctx, super::NetCommand::Invite { label: Some(label).filter(|l| !l.is_empty()), json })
        }
        InviteCommand::List { all, json } => {
            let n = Net::open(ctx)?;
            let now = crate::store::now_ms();
            let mine: Vec<_> = n
                .store
                .exchanges()?
                .into_iter()
                .filter(|x| x.role == "offerer")
                .filter(|x| all || (x.status == ExchangeStatus::Offered && x.expires_at.is_none_or(|e| e > now)))
                .collect();
            let reqs = auto::requests(&n.store)?;
            let state = |x: &crate::store::Exchange| match x.status {
                ExchangeStatus::Offered if x.expires_at.is_some_and(|e| e <= now) => "expired",
                ExchangeStatus::Offered if x.peer_id.is_some() => "sent",
                ExchangeStatus::Offered => "unused",
                ExchangeStatus::Revoked => "revoked",
                _ => "used",
            };
            if json {
                let v = serde_json::json!({
                    "invites": mine.iter().map(|x| serde_json::json!({ "id": x.id, "for": auto::purpose(x), "to": x.title(), "status": state(x), "expires_at": x.expires_at })).collect::<Vec<_>>(),
                    "requests": reqs.iter().map(|x| serde_json::json!({ "id": x.id, "from": x.title(), "for": auto::purpose(x), "at": x.created_at })).collect::<Vec<_>>(),
                });
                println!("{}", serde_json::to_string_pretty(&v)?);
                return Ok(());
            }
            for x in &mine {
                let to = if x.peer_id.is_some() || !x.label.is_empty() { x.title() } else { "-".into() };
                let exp = x.expires_at.map(|e| format!("expires {}", fmt_local(e))).unwrap_or_default();
                println!("invite\t{}\t{}\t{to}\t{}\t{exp}", x.short_id(), auto::purpose(x), state(x));
            }
            for x in &reqs {
                println!("request\t{}\t{}\t{}\twaiting\tsince {}", x.short_id(), auto::purpose(x), x.title(), fmt_local(x.created_at));
            }
            if mine.is_empty() && reqs.is_empty() {
                ctx.io.info("no open invites or requests (`mnnn invite create` makes one)");
            }
            Ok(())
        }
        InviteCommand::Revoke { invite } => {
            let n = Net::open(ctx)?;
            let x = n.store.find_exchange(&invite)?;
            if x.role != "offerer" {
                return Err(anyhow!("that is not an invite you made (decline a request with `mnnn invite decline`)"));
            }
            if x.status != ExchangeStatus::Offered {
                return Err(anyhow!(
                    "that invite was already {} (end a connection with `mnnn net disconnect`)",
                    if x.status == ExchangeStatus::Revoked { "revoked" } else { "used" }
                ));
            }
            let (x, _) = client::revoke(&n.store, None, &x.id, &[], ORIGIN)?;
            ctx.io.info(format!("invite {} revoked; the link no longer works", x.short_id()));
            Ok(())
        }
        InviteCommand::Accept { invite } if net::exchange::is_token(&net::exchange::unlink(&invite)) => {
            super::net::run(ctx, super::NetCommand::Connect { target: invite, label: None, hello: false })
        }
        InviteCommand::Accept { invite } => super::net::run(ctx, super::NetCommand::Accept { request: invite }),
        InviteCommand::Decline { request } => super::net::run(ctx, super::NetCommand::Decline { request }),
    }
}
