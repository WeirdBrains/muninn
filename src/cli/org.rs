//! `mnnn org`: organizations. Thin over [`crate::org`].
use super::net::{Net, kind, read_token, sanitize};
use super::{
    Ctx, OrgCertCommand, OrgCommand, OrgEndpointCommand, OrgHostCommand, OrgMemberCommand, OrgProjectCommand, OrgResourceCommand, OrgRoleCommand,
    OrgTemplateCommand,
};
use crate::ingest::time::fmt_ts;
use crate::net::client;
use crate::org::host::{self, Acme, Host, HostOptions};
use crate::org::net::{self as onet, ManageOp, OrgReq};
use crate::org::{self, Actor, Leftovers};
use crate::store::{Access, EndpointKind, Joined, NewAction, Org, OrgChange, Published, ResourceState, Store, Visibility};
use anyhow::{Context, anyhow, bail};
use serde_json::{Value, json};
use std::io::Read;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const ORIGIN: &str = "cli";
const OWNER: Actor = Actor::Owner;

/// An org of mine, or one I joined on another node.
enum Target {
    Mine(Org),
    Joined(Joined),
}

fn target(s: &Store, key: &str) -> anyhow::Result<Target> {
    match s.find_org(key) {
        Ok(o) => Ok(Target::Mine(o)),
        Err(e) => s.find_joined(key).map(Target::Joined).map_err(|_| anyhow!(e)),
    }
}

fn mine(s: &Store, key: &str) -> anyhow::Result<Org> {
    match target(s, key)? {
        Target::Mine(o) => Ok(o),
        Target::Joined(j) => bail!("{} is run by another node ({}); this is done there", j.name, short(&j.node_peer)),
    }
}

fn short(p: &str) -> &str {
    &p[p.len().saturating_sub(8)..]
}

fn pretty(v: &impl serde::Serialize) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string_pretty(v)?);
    Ok(())
}

fn read_arg(input: &str) -> anyhow::Result<String> {
    if input == "-" {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        return Ok(s);
    }
    Ok(input.to_string())
}

/// Ask a joined org's node to do something (it checks my permissions there).
fn remote(ctx: &Ctx, j: &Joined, req: OrgReq) -> anyhow::Result<Value> {
    let mut n = Net::open(ctx)?;
    let relays = n.relays();
    n.node()?;
    Ok(onet::request(&n.store, n.node.as_ref().expect("node"), j, req, &relays)?)
}

fn manage(ctx: &Ctx, j: &Joined, op: ManageOp) -> anyhow::Result<()> {
    let v = remote(ctx, j, OrgReq::Manage { op })?;
    ctx.io.info(format!("done at {}", j.name));
    if !v.is_null() && v != json!({}) {
        pretty(&v)?;
    }
    Ok(())
}

pub fn run(ctx: &Ctx, cmd: OrgCommand) -> anyhow::Result<()> {
    match cmd {
        OrgCommand::Create { name, slug, description, public, open } => {
            let n = Net::open(ctx)?;
            let s = &n.store;
            let mut o = org::create(s, ORIGIN, &name, slug.as_deref(), &description)?;
            if public || open {
                let c = OrgChange { visibility: public.then_some(Visibility::Public), access: open.then_some(Access::Open), ..Default::default() };
                o = org::edit(s, ORIGIN, &OWNER, &o, &c)?;
            }
            ctx.io.info(format!("created {} ({}, {}, {})", o.name, o.slug, o.visibility.as_str(), o.access.as_str()));
            ctx.io.note("Invite with `mnnn org member invite`; members reach it while `mnnn net start` runs.");
            Ok(())
        }
        OrgCommand::Edit { org: key, name, slug, description, visibility, access, hosted, member_auth, template } => {
            let n = Net::open(ctx)?;
            let s = &n.store;
            let o = mine(s, &key)?;
            let template_id = match template.as_deref() {
                None => None,
                Some("default" | "none" | "") => Some(None),
                Some(t) => Some(Some(s.find_template(&o.id, t)?.id)),
            };
            let c = OrgChange {
                name,
                slug,
                description,
                visibility: visibility.as_deref().map(Visibility::parse).transpose()?,
                access: access.as_deref().map(Access::parse).transpose()?,
                hosted: hosted.map(|h| h.on()),
                member_auth: member_auth.map(|h| h.on()),
                template_id,
                ..Default::default()
            };
            let o = org::edit(s, ORIGIN, &OWNER, &o, &c)?;
            ctx.io.info(format!(
                "{} ({}): {}, {}{}",
                o.name,
                o.slug,
                o.visibility.as_str(),
                o.access.as_str(),
                if o.hosted { ", hosted" } else { "" }
            ));
            Ok(())
        }
        OrgCommand::Remove { org: key } => {
            let n = Net::open(ctx)?;
            let s = &n.store;
            let o = mine(s, &key)?;
            let members = s.members(&o.id)?.len();
            if !ctx.io.confirm(&format!("Delete {} and its {members} memberships? (its resources stay here)", o.name))? {
                return Ok(());
            }
            let o = org::remove(s, ORIGIN, &OWNER, &o)?;
            ctx.io.info(format!("removed {}", o.name));
            Ok(())
        }
        OrgCommand::List { json } => {
            let n = Net::open(ctx)?;
            let s = &n.store;
            let orgs = s.orgs()?;
            let joined = s.joined()?;
            let mirrors = s.mirrors()?;
            if json {
                let mine: Vec<Value> = orgs.iter().map(|o| json!({ "card": org::card(s, o), "hosted": o.hosted })).collect();
                let mirrors: Vec<Value> =
                    mirrors.iter().map(|m| json!({ "org": m.org_id, "slug": m.slug, "node": m.node_peer, "fetched_at": m.fetched_at })).collect();
                return pretty(&json!({ "mine": mine, "joined": joined, "mirrors": mirrors }));
            }
            if orgs.is_empty() && joined.is_empty() {
                ctx.io.note("No organizations yet: `mnnn org create NAME`, or `mnnn org browse NODE` and `mnnn org apply`.");
            }
            for o in &orgs {
                let c = org::card(s, o);
                println!(
                    "{:<8} {:<24} {:<16} {:<7} {:<7} {:>3} members{}",
                    o.short_id(),
                    o.name,
                    o.slug,
                    o.visibility.as_str(),
                    o.access.as_str(),
                    c.members,
                    if o.hosted { "  hosted" } else { "" }
                );
            }
            for j in &joined {
                println!("joined   {:<24} {:<16} {:<8} {:<8} at {}", j.name, j.slug, j.status.as_str(), j.role, short(&j.node_peer));
            }
            for m in &mirrors {
                println!("mirror   {:<24} fetched {}", m.slug, fmt_ts(m.fetched_at));
            }
            Ok(())
        }
        OrgCommand::Show { org: key, json } => {
            let n = Net::open(ctx)?;
            let s = &n.store;
            match target(s, &key)? {
                Target::Joined(j) => {
                    if json {
                        return pretty(&j);
                    }
                    println!("{} ({}) on node {}", j.name, j.slug, j.node_peer);
                    println!("  you: {} as {}  [{}]", j.status.as_str(), j.role, j.perms.join(", "));
                    if !j.info.is_null() {
                        println!("{}", serde_json::to_string_pretty(&j.info)?);
                    }
                    Ok(())
                }
                Target::Mine(o) => show(s, &o, json),
            }
        }
        OrgCommand::Role(c) => role(ctx, c),
        OrgCommand::Member(c) => member(ctx, c),
        OrgCommand::Resource(c) => resource(ctx, c),
        OrgCommand::Publish { org: key, resource, to } => {
            let to = Published::parse(&to)?;
            publish(ctx, &key, &resource, to)
        }
        OrgCommand::Unpublish { org: key, resource } => publish(ctx, &key, &resource, Published::No),
        OrgCommand::Project(c) => project(ctx, c),
        OrgCommand::Endpoint(c) => endpoint(ctx, c),
        OrgCommand::Call { org: key, endpoint, input } => {
            let input: Value = serde_json::from_str(&read_arg(&input)?).context("--input is not JSON")?;
            let n = Net::open(ctx)?;
            let v = match target(&n.store, &key)? {
                Target::Mine(o) => org::call(&n.store, ORIGIN, &OWNER, &o, &endpoint, &input)?,
                Target::Joined(j) => {
                    drop(n);
                    remote(ctx, &j, OrgReq::Call { endpoint, input })?
                }
            };
            pretty(&v)
        }
        OrgCommand::Template(c) => template(ctx, c),
        OrgCommand::Browse { target: t, org: o, json } => {
            let mut n = Net::open(ctx)?;
            let relays = n.relays();
            n.node()?;
            let v = onet::browse(&n.store, n.node.as_ref().expect("node"), &t, o.as_deref(), &relays)?;
            if json {
                return pretty(&v);
            }
            if v.is_empty() {
                ctx.io.note("That node lists no organizations.");
            }
            for c in v {
                println!(
                    "{:<24} {:<16} {:<7} {:<7} {:>3} members  {}",
                    c.name,
                    c.slug,
                    c.visibility.as_str(),
                    c.access.as_str(),
                    c.members,
                    c.description
                );
            }
            Ok(())
        }
        OrgCommand::Apply { target: t, org: o, note } => {
            let mut n = Net::open(ctx)?;
            let relays = n.relays();
            let label = n.label.clone();
            n.node()?;
            let j = onet::apply(&n.store, n.node.as_ref().expect("node"), &n.kp, &t, &o, &note, &label, &relays, ORIGIN)?;
            ctx.io.info(format!("applied to {} ({})", j.name, j.status.as_str()));
            ctx.io.note("Check for an answer with `mnnn org sync`.");
            Ok(())
        }
        OrgCommand::Join { token } => {
            let mut n = Net::open(ctx)?;
            let token = read_token(&token)?;
            // an invitation that came over the network: by org name, slug or request id
            let token = if crate::net::exchange::is_token(&token) { token } else { invitation(&n.store, &token)?.id };
            let addrs = n.addrs();
            let label = n.label.clone();
            n.node()?;
            let a = client::accept(&n.store, n.node.as_ref(), &n.kp, &token, "", &label, addrs, ORIGIN)?;
            match n.store.joined_by_exchange(&a.exchange.id)? {
                Some(j) => ctx.io.info(format!("joined {} ({}) as {}", j.name, j.slug, j.role)),
                None => ctx.io.warn("the key exchange is done, but it was not an organization invitation"),
            }
            if let Some(ans) = &a.answer
                && !matches!(a.delivered, Some(Ok(_)))
            {
                ctx.io.note("Their node was not reachable; you are connected once it is (or give them this link):");
                println!("{}", crate::net::exchange::link(ans));
            }
            Ok(())
        }
        OrgCommand::Sync { org: key } => {
            let mut n = Net::open(ctx)?;
            let relays = n.relays();
            let j = n.store.find_joined(&key)?;
            n.node()?;
            let j = onet::sync(&n.store, n.node.as_ref().expect("node"), &j, &relays, ORIGIN)?;
            ctx.io.info(format!("{}: {} as {}", j.name, j.status.as_str(), j.role));
            Ok(())
        }
        OrgCommand::Leave { org: key } => {
            let mut n = Net::open(ctx)?;
            let relays = n.relays();
            let j = n.store.find_joined(&key)?;
            if !ctx.io.confirm(&format!("Leave {}?", j.name))? {
                return Ok(());
            }
            n.node()?;
            let j = onet::leave(&n.store, n.node.as_ref(), &j, &relays, ORIGIN)?;
            ctx.io.info(format!("left {}", j.name));
            Ok(())
        }
        OrgCommand::Fetch { org: key, resource, format, out } => {
            let mut n = Net::open(ctx)?;
            let relays = n.relays();
            let j = n.store.find_joined(&key)?;
            n.node()?;
            let it = onet::fetch(&n.store, n.node.as_ref().expect("node"), &j, &resource, format.as_deref(), &relays)?;
            let binary = std::str::from_utf8(&it.data).is_err() || matches!(it.format.as_str(), "pdf" | "docx" | "mnnp");
            match out {
                Some(p) => {
                    std::fs::write(&p, &it.data).with_context(|| p.display().to_string())?;
                    ctx.io.info(format!("wrote {} ({} bytes)", p.display(), it.data.len()));
                }
                None if binary => {
                    let p = std::path::PathBuf::from(format!("{}.{}", sanitize(&it.name), it.format));
                    std::fs::write(&p, &it.data).with_context(|| p.display().to_string())?;
                    ctx.io.info(format!("wrote {} ({} bytes)", p.display(), it.data.len()));
                }
                None => println!("{}", String::from_utf8_lossy(&it.data)),
            }
            Ok(())
        }
        OrgCommand::Give { org: key, kind: k, name } => {
            let k = kind(&k)?;
            let mut n = Net::open(ctx)?;
            let relays = n.relays();
            let j = n.store.find_joined(&key)?;
            n.node()?;
            onet::transfer(&n.store, n.node.as_ref().expect("node"), &j, k, &name, &relays, ORIGIN)?;
            ctx.io.info(format!("gave {} {name} to {} (yours stays as it is)", k.label(), j.name));
            Ok(())
        }
        OrgCommand::Mirror { org: key } => {
            let mut n = Net::open(ctx)?;
            let relays = n.relays();
            let j = n.store.find_joined(&key)?;
            n.node()?;
            let m = onet::mirror(&n.store, n.node.as_ref().expect("node"), &j, &relays, ORIGIN)?;
            ctx.io.info(format!("mirrored {} ({} bytes); `mnnn org host start` serves it at /o/{}/", j.name, m.snapshot.len(), m.slug));
            Ok(())
        }
        OrgCommand::Sign { org: key, method, path, body } => {
            let body = match body.as_deref() {
                None => vec![],
                Some("-") => {
                    let mut b = vec![];
                    std::io::stdin().read_to_end(&mut b)?;
                    b
                }
                Some(f) => std::fs::read(f).with_context(|| f.to_string())?,
            };
            let n = Net::open(ctx)?;
            let h = host::authorization(&n.store, &key, &method.to_ascii_uppercase(), &path, &body)?;
            println!("Authorization: {h}");
            ctx.io.note("Valid for five minutes, once, for exactly this method, path and body.");
            Ok(())
        }
        OrgCommand::Host(c) => host_cmd(ctx, c),
        OrgCommand::Cert(c) => cert(ctx, c),
    }
}

fn show(s: &Store, o: &Org, json: bool) -> anyhow::Result<()> {
    let roles = s.roles(&o.id)?;
    let members = s.members(&o.id)?;
    let resources = s.resources(&o.id)?;
    let projects = s.org_projects(&o.id)?;
    let endpoints = s.endpoints(&o.id)?;
    let templates = s.templates(&o.id)?;
    let role =
        |id: &Option<String>| id.as_deref().and_then(|r| roles.iter().find(|x| x.id == r)).map(|r| r.name.clone()).unwrap_or_else(|| "-".into());
    if json {
        let ms: Vec<Value> = members.iter().map(|m| json!({ "member": m, "role": role(&m.role_id) })).collect();
        let ps: Vec<Value> = projects.iter().map(|p| json!({ "id": p.id, "name": p.name })).collect();
        let ts: Vec<Value> = templates.iter().map(|t| json!({ "id": t.id, "name": t.name })).collect();
        return pretty(&json!({
            "org": o, "roles": roles, "members": ms, "resources": resources, "projects": ps, "endpoints": endpoints, "templates": ts,
            "apply_role": role(&o.apply_role), "invite_role": role(&o.invite_role),
        }));
    }
    println!("{} ({})  {}", o.name, o.slug, o.short_id());
    if !o.description.is_empty() {
        println!("  {}", o.description);
    }
    println!("  {}, {}; new members: {} (apply) / {} (invite)", o.visibility.as_str(), o.access.as_str(), role(&o.apply_role), role(&o.invite_role));
    let tpl = o.template_id.as_deref().and_then(|t| templates.iter().find(|x| x.id == t)).map(|t| t.name.as_str()).unwrap_or("default");
    println!(
        "  site: {}; member sign-in {}; template {tpl}",
        if o.hosted { "hosted" } else { "not hosted" },
        if o.member_auth { "on" } else { "off" }
    );
    println!("roles:");
    for r in &roles {
        println!("  {:<12} rank {:>3}  {}", r.name, r.rank, r.perms.join(" "));
    }
    println!("members:");
    for m in &members {
        println!("  {:<8} {:<20} {:<10} {:<9} {}", m.short_id(), m.title(), role(&m.role_id), m.status.as_str(), m.note);
    }
    if !resources.is_empty() {
        println!("resources:");
        for r in &resources {
            println!("  {:<8} {:<8} {:<24} {:<8} published: {}", r.short_id(), r.kind.label(), r.name, r.state.as_str(), r.published.as_str());
        }
    }
    if !projects.is_empty() {
        println!("projects:");
        for p in &projects {
            let pm = s.project_members(&o.id, &p.id)?;
            let names: Vec<String> = pm.iter().filter_map(|x| members.iter().find(|m| m.id == x.member_id)).map(|m| m.title()).collect();
            println!("  {:<24} {}", p.name, names.join(", "));
        }
    }
    if !endpoints.is_empty() {
        println!("endpoints:");
        for e in &endpoints {
            println!("  {:<16} {:<8} needs {}{}", e.name, e.kind.as_str(), e.perm, if e.anyone { " (or anyone)" } else { "" });
        }
    }
    if !templates.is_empty() {
        println!("templates: {}", templates.iter().map(|t| t.name.as_str()).collect::<Vec<_>>().join(", "));
    }
    Ok(())
}

fn role(ctx: &Ctx, cmd: OrgRoleCommand) -> anyhow::Result<()> {
    if let OrgRoleCommand::Perms = cmd {
        for (p, what) in org::PERMS {
            println!("{p:<20} {what}");
        }
        return Ok(());
    }
    let n = Net::open(ctx)?;
    let s = &n.store;
    match cmd {
        OrgRoleCommand::Perms => unreachable!(),
        OrgRoleCommand::List { org: key, json } => {
            let o = mine(s, &key)?;
            let roles = s.roles(&o.id)?;
            if json {
                return pretty(&roles);
            }
            for r in &roles {
                println!("{:<12} rank {:>3}{}  {}", r.name, r.rank, if r.builtin { " (default)" } else { "" }, r.perms.join(" "));
            }
            Ok(())
        }
        OrgRoleCommand::Create { org: key, name, rank, perms } => {
            let o = mine(s, &key)?;
            let r = org::create_role(s, ORIGIN, &OWNER, &o, &name, rank, &perms)?;
            ctx.io.info(format!("role {} (rank {}): {}", r.name, r.rank, r.perms.join(" ")));
            Ok(())
        }
        OrgRoleCommand::Edit { org: key, role, name, rank, perms, no_perms } => {
            let o = mine(s, &key)?;
            let r = s.find_role(&o.id, &role)?;
            let perms = if no_perms { Some(vec![]) } else { (!perms.is_empty()).then_some(perms) };
            let r = org::edit_role(s, ORIGIN, &OWNER, &o, &r, name.as_deref(), rank, perms.as_deref())?;
            ctx.io.info(format!("role {} (rank {}): {}", r.name, r.rank, r.perms.join(" ")));
            Ok(())
        }
        OrgRoleCommand::Remove { org: key, role } => {
            let o = mine(s, &key)?;
            let r = s.find_role(&o.id, &role)?;
            let r = org::remove_role(s, ORIGIN, &OWNER, &o, &r)?;
            ctx.io.info(format!("removed role {}", r.name));
            Ok(())
        }
    }
}

fn member(ctx: &Ctx, cmd: OrgMemberCommand) -> anyhow::Result<()> {
    let mut n = Net::open(ctx)?;
    let relays = n.relays();
    let key = match &cmd {
        OrgMemberCommand::List { org, .. }
        | OrgMemberCommand::Approve { org, .. }
        | OrgMemberCommand::Reject { org, .. }
        | OrgMemberCommand::Invite { org, .. }
        | OrgMemberCommand::Kick { org, .. }
        | OrgMemberCommand::Role { org, .. }
        | OrgMemberCommand::Defaults { org, .. } => org.clone(),
    };
    let o = match target(&n.store, &key)? {
        Target::Mine(o) => o,
        Target::Joined(j) => {
            drop(n);
            return member_remote(ctx, &j, cmd);
        }
    };
    match cmd {
        OrgMemberCommand::List { json, .. } => {
            let s = &n.store;
            let ms = s.members(&o.id)?;
            if json {
                let v: Vec<Value> = ms.iter().map(|m| json!({ "member": m, "role": org::role_name(s, m) })).collect();
                return pretty(&v);
            }
            for m in &ms {
                println!(
                    "{:<8} {:<20} {:<10} {:<9} {:<10} {}",
                    m.short_id(),
                    m.title(),
                    org::role_name(s, m),
                    m.status.as_str(),
                    fmt_ts(m.updated_at),
                    m.note
                );
            }
            Ok(())
        }
        OrgMemberCommand::Approve { member, role, .. } => {
            let m = n.store.find_member(&o.id, &member)?;
            let r = role.as_deref().map(|r| n.store.find_role(&o.id, r)).transpose()?;
            n.node()?;
            let (m, told) = onet::approve(&n.store, n.node.as_ref(), &n.kp, &OWNER, &o, &m, r.as_ref(), ORIGIN)?;
            ctx.io.info(format!("{} is a member of {} ({})", m.title(), o.name, org::role_name(&n.store, &m)));
            if !matches!(told, Some(Ok(_))) {
                ctx.io.note("They will see it the next time they run `mnnn org sync`.");
            }
            Ok(())
        }
        OrgMemberCommand::Reject { member, .. } => {
            let m = n.store.find_member(&o.id, &member)?;
            let m = onet::reject(&n.store, ORIGIN, &OWNER, &o, &m)?;
            ctx.io.info(format!("rejected {}", m.title()));
            Ok(())
        }
        OrgMemberCommand::Invite { peer, label, role, .. } => {
            let r = role.as_deref().map(|r| n.store.find_role(&o.id, r)).transpose()?;
            if peer.is_some() {
                n.node()?;
            }
            let (m, c) = onet::invite(&n.store, n.node.as_ref(), &n.kp, &OWNER, &o, r.as_ref(), peer.as_deref(), &label, &relays, ORIGIN)?;
            ctx.io.info(format!("invited {} to {} as {}", m.title(), o.name, org::role_name(&n.store, &m)));
            match &c.delivered {
                Some(Ok(_)) => ctx.io.note("Sent; they join with `mnnn org join` (or the TUI)."),
                other => {
                    if let Some(Err(e)) = other {
                        ctx.io.warn(format!("could not reach them ({e})"));
                    }
                    ctx.io.note("Send them this invite link (single use, expires in 7 days; they run `mnnn org join LINK`):");
                    println!("{}", crate::net::exchange::link(&c.token));
                }
            }
            Ok(())
        }
        OrgMemberCommand::Kick { member, resources, .. } => {
            let left = Leftovers::parse(&resources)?;
            let m = n.store.find_member(&o.id, &member)?;
            if !ctx.io.confirm(&format!("Remove {} from {}?", m.title(), o.name))? {
                return Ok(());
            }
            n.node()?;
            let m = onet::kick(&n.store, n.node.as_ref(), &OWNER, &o, &m, left, &relays, ORIGIN)?;
            ctx.io.info(format!("removed {}; what they gave stays with {} ({resources})", m.title(), o.name));
            Ok(())
        }
        OrgMemberCommand::Role { member, role, .. } => {
            let m = n.store.find_member(&o.id, &member)?;
            let r = n.store.find_role(&o.id, &role)?;
            let m = org::set_role(&n.store, ORIGIN, &OWNER, &o, &m, &r)?;
            ctx.io.info(format!("{} is now {}", m.title(), r.name));
            Ok(())
        }
        OrgMemberCommand::Defaults { apply, invite, .. } => {
            let id = |r: Option<String>| r.map(|r| n.store.find_role(&o.id, &r).map(|r| r.id)).transpose();
            let c = OrgChange { apply_role: id(apply)?, invite_role: id(invite)?, ..Default::default() };
            let o = onet::defaults(&n.store, ORIGIN, &OWNER, &o, &c)?;
            let name = |id: &Option<String>| id.as_deref().and_then(|r| n.store.role(r).ok().flatten()).map(|r| r.name).unwrap_or_else(|| "-".into());
            ctx.io.info(format!("new members: {} (apply), {} (invite)", name(&o.apply_role), name(&o.invite_role)));
            Ok(())
        }
    }
}

fn member_remote(ctx: &Ctx, j: &Joined, cmd: OrgMemberCommand) -> anyhow::Result<()> {
    match cmd {
        OrgMemberCommand::List { json, .. } => {
            let v = remote(ctx, j, OrgReq::Members)?;
            if json {
                return pretty(&v);
            }
            for m in v["members"].as_array().into_iter().flatten() {
                let st = |k: &str| m[k].as_str().unwrap_or("").to_string();
                println!("{:<8} {:<20} {:<10} {:<9} {}", short(&st("id")), st("label"), st("role"), st("status"), st("note"));
            }
            Ok(())
        }
        OrgMemberCommand::Approve { member, role, .. } => manage(ctx, j, ManageOp::Approve { member, role }),
        OrgMemberCommand::Reject { member, .. } => manage(ctx, j, ManageOp::Reject { member }),
        OrgMemberCommand::Kick { member, resources, .. } => manage(ctx, j, ManageOp::Kick { member, resources: Leftovers::parse(&resources)? }),
        OrgMemberCommand::Role { member, role, .. } => manage(ctx, j, ManageOp::Role { member, role }),
        OrgMemberCommand::Defaults { apply, invite, .. } => manage(ctx, j, ManageOp::Defaults { apply, invite }),
        OrgMemberCommand::Invite { .. } => bail!("invitations are made at {}'s own node", j.name),
    }
}

fn publish(ctx: &Ctx, key: &str, resource: &str, to: Published) -> anyhow::Result<()> {
    let n = Net::open(ctx)?;
    match target(&n.store, key)? {
        Target::Mine(o) => {
            let r = n.store.find_resource(&o.id, resource)?;
            let r = org::set_published(&n.store, ORIGIN, &OWNER, &o, &r, to)?;
            ctx.io.info(format!("{} {}: published {}", r.kind.label(), r.name, r.published.as_str()));
            if to != Published::No && !o.hosted {
                ctx.io.note("The site is off: `mnnn org edit ORG --hosted on`, then `mnnn org host start`.");
            }
            Ok(())
        }
        Target::Joined(j) => {
            drop(n);
            manage(ctx, &j, ManageOp::Publish { resource: resource.to_string(), to })
        }
    }
}

fn resource(ctx: &Ctx, cmd: OrgResourceCommand) -> anyhow::Result<()> {
    let n = Net::open(ctx)?;
    let s = &n.store;
    match cmd {
        OrgResourceCommand::List { org: key, json } => match target(s, &key)? {
            Target::Mine(o) => {
                let rs = s.resources(&o.id)?;
                if json {
                    return pretty(&rs);
                }
                for r in &rs {
                    let by = r.transferred_by.as_deref().and_then(|m| s.member(m).ok().flatten()).map(|m| m.title()).unwrap_or_else(|| "here".into());
                    println!(
                        "{:<8} {:<8} {:<24} {:<8} published: {:<8} from {by}",
                        r.short_id(),
                        r.kind.label(),
                        r.name,
                        r.state.as_str(),
                        r.published.as_str()
                    );
                }
                Ok(())
            }
            Target::Joined(j) => {
                drop(n);
                let v = remote(ctx, &j, OrgReq::Resources)?;
                if json {
                    return pretty(&v);
                }
                for r in v["resources"].as_array().into_iter().flatten() {
                    let st = |k: &str| r[k].as_str().unwrap_or("").to_string();
                    println!("{:<8} {:<8} {:<24} {:<8} published: {}", short(&st("id")), st("kind"), st("name"), st("state"), st("published"));
                }
                Ok(())
            }
        },
        OrgResourceCommand::Add { org: key, kind: k, name } => {
            let o = mine(s, &key)?;
            let k = kind(&k)?;
            let (r, new) = org::transfer(s, ORIGIN, &OWNER, &o, k, &name, None)?;
            ctx.io.info(format!("{} {} {} {}", if new { "added" } else { "already has" }, k.label(), r.name, o.name));
            Ok(())
        }
        OrgResourceCommand::Archive { org: key, resource } => state(ctx, n, &key, &resource, true),
        OrgResourceCommand::Restore { org: key, resource } => state(ctx, n, &key, &resource, false),
        OrgResourceCommand::Release { org: key, resource } => {
            let o = mine(s, &key)?;
            let r = s.find_resource(&o.id, &resource)?;
            let r = org::release(s, ORIGIN, &OWNER, &o, &r)?;
            ctx.io.info(format!("{} {} is no longer {}'s", r.kind.label(), r.name, o.name));
            Ok(())
        }
    }
}

fn state(ctx: &Ctx, n: Net, key: &str, resource: &str, archived: bool) -> anyhow::Result<()> {
    match target(&n.store, key)? {
        Target::Mine(o) => {
            let r = n.store.find_resource(&o.id, resource)?;
            let st = if archived { ResourceState::Archived } else { ResourceState::Active };
            let r = org::set_state(&n.store, ORIGIN, &OWNER, &o, &r, st)?;
            ctx.io.info(format!("{} {}: {}", r.kind.label(), r.name, r.state.as_str()));
            Ok(())
        }
        Target::Joined(j) => {
            drop(n);
            manage(ctx, &j, ManageOp::State { resource: resource.to_string(), archived })
        }
    }
}

fn project(ctx: &Ctx, cmd: OrgProjectCommand) -> anyhow::Result<()> {
    let n = Net::open(ctx)?;
    let s = &n.store;
    match cmd {
        OrgProjectCommand::List { org: key } => {
            let o = mine(s, &key)?;
            let members = s.members(&o.id)?;
            for p in s.org_projects(&o.id)? {
                let pm = s.project_members(&o.id, &p.id)?;
                let names: Vec<String> = pm
                    .iter()
                    .filter_map(|x| {
                        let m = members.iter().find(|m| m.id == x.member_id)?;
                        let r = x.role_id.as_deref().and_then(|r| s.role(r).ok().flatten()).map(|r| r.name).unwrap_or_else(|| org::role_name(s, m));
                        Some(format!("{} ({r})", m.title()))
                    })
                    .collect();
                println!("{:<24} {}", p.name, names.join(", "));
            }
            Ok(())
        }
        OrgProjectCommand::Add { org: key, project } => {
            let o = mine(s, &key)?;
            let p = org::add_project(s, ORIGIN, &OWNER, &o, &project)?;
            ctx.io.info(format!("{} is a project of {}", p.name, o.name));
            Ok(())
        }
        OrgProjectCommand::Remove { org: key, project } => {
            let o = mine(s, &key)?;
            let p = org::remove_project(s, ORIGIN, &OWNER, &o, &project)?;
            ctx.io.info(format!("{} is no longer a project of {} (the project stays here)", p.name, o.name));
            Ok(())
        }
        OrgProjectCommand::Member { org: key, project, member, role } => {
            let o = mine(s, &key)?;
            let m = s.find_member(&o.id, &member)?;
            let r = role.as_deref().map(|r| s.find_role(&o.id, r)).transpose()?;
            org::add_project_member(s, ORIGIN, &OWNER, &o, &project, &m, r.as_ref())?;
            ctx.io.info(format!("{} is on {project}", m.title()));
            Ok(())
        }
    }
}

fn endpoint(ctx: &Ctx, cmd: OrgEndpointCommand) -> anyhow::Result<()> {
    let n = Net::open(ctx)?;
    let s = &n.store;
    match cmd {
        OrgEndpointCommand::List { org: key, json } => {
            let o = mine(s, &key)?;
            let es = s.endpoints(&o.id)?;
            if json {
                return pretty(&es);
            }
            for e in &es {
                println!("{:<16} {:<8} needs {:<14}{}", e.name, e.kind.as_str(), e.perm, if e.anyone { " or anyone" } else { "" });
            }
            Ok(())
        }
        OrgEndpointCommand::Add { org: key, name, kind: k, target: t, perm, anyone } => {
            let o = mine(s, &key)?;
            let e = org::add_endpoint(s, ORIGIN, &OWNER, &o, &name, EndpointKind::parse(&k)?, &t, perm.as_deref(), anyone)?;
            ctx.io.info(format!("endpoint {} ({}, needs {}); POST /o/{}/api/call/{}", e.name, e.kind.as_str(), e.perm, o.slug, e.name));
            Ok(())
        }
        OrgEndpointCommand::Remove { org: key, name } => {
            let o = mine(s, &key)?;
            let e = org::remove_endpoint(s, ORIGIN, &OWNER, &o, &name)?;
            ctx.io.info(format!("removed endpoint {}", e.name));
            Ok(())
        }
    }
}

fn template(ctx: &Ctx, cmd: OrgTemplateCommand) -> anyhow::Result<()> {
    let n = Net::open(ctx)?;
    let s = &n.store;
    match cmd {
        OrgTemplateCommand::List { org: key } => {
            let o = mine(s, &key)?;
            for t in s.templates(&o.id)? {
                let assets = s.template_assets(&t.id)?;
                let used = if o.template_id.as_deref() == Some(t.id.as_str()) { " (in use)" } else { "" };
                println!("{:<16}{used} {} assets: {}", t.name, assets.len(), assets.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", "));
            }
            Ok(())
        }
        OrgTemplateCommand::Save { org: key, name, html, css, r#use } => {
            let o = mine(s, &key)?;
            let h = std::fs::read_to_string(&html).with_context(|| html.display().to_string())?;
            let c = match &css {
                Some(p) => std::fs::read_to_string(p).with_context(|| p.display().to_string())?,
                None => String::new(),
            };
            let t = org::save_template(s, ORIGIN, &OWNER, &o, &name, &h, &c)?;
            if r#use {
                org::edit(s, ORIGIN, &OWNER, &o, &OrgChange { template_id: Some(Some(t.id.clone())), ..Default::default() })?;
            }
            ctx.io.info(format!("template {}{}", t.name, if r#use { ", in use" } else { "" }));
            Ok(())
        }
        OrgTemplateCommand::Asset { org: key, template: tk, file, name } => {
            let o = mine(s, &key)?;
            let t = s.find_template(&o.id, &tk)?;
            let data = std::fs::read(&file).with_context(|| file.display().to_string())?;
            let name = name.unwrap_or_else(|| file.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default());
            let at = org::put_asset(s, ORIGIN, &OWNER, &o, &t, &name, &data)?;
            ctx.io.info(format!("asset {name} ({} bytes) at {at}", data.len()));
            Ok(())
        }
    }
}

/// An org invitation waiting here, by org name or slug, request id or inviter.
fn invitation(s: &crate::store::Store, key: &str) -> anyhow::Result<crate::store::Exchange> {
    let reqs: Vec<_> = crate::net::auto::requests(s)?
        .into_iter()
        .filter(|x| {
            x.scope.get("kind").and_then(|v| v.as_str()) == Some("organization") && x.scope.get("as").and_then(|v| v.as_str()) == Some("invite")
        })
        .collect();
    let k = key.trim().to_lowercase();
    let field = |x: &crate::store::Exchange, f: &str| x.scope.get(f).and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
    reqs.iter()
        .find(|x| field(x, "name") == k || field(x, "slug") == k)
        .or_else(|| reqs.iter().find(|x| x.id.starts_with(&k) || x.title().to_lowercase() == k))
        .cloned()
        .ok_or_else(|| anyhow!("no invitation to `{key}` here (`mnnn net requests` lists them; or pass the invite link)"))
}

fn host_cmd(ctx: &Ctx, cmd: OrgHostCommand) -> anyhow::Result<()> {
    let home = ctx.paths.home.clone();
    match cmd {
        OrgHostCommand::Start { bind, port, for_secs } => {
            if let Some(f) = host::running(&home) {
                bail!("already serving at {} (process {})", f.url, f.pid);
            }
            let n = Net::open(ctx)?;
            let mut opts = HostOptions::from_prefs(&n.prefs.hosting)?;
            if let Some(b) = bind {
                opts.bind = b;
            }
            if let Some(p) = port {
                opts.port = p;
            }
            let store = n.second_store()?;
            let hosted: Vec<String> = n.store.orgs()?.into_iter().filter(|o| o.hosted).map(|o| o.slug).collect();
            if hosted.is_empty() && n.store.mirrors()?.is_empty() {
                ctx.io.warn("no org is hosted yet (`mnnn org edit ORG --hosted on`); serving an empty index");
            }
            let h = Host::start(store, opts)?;
            let st = h.status();
            n.store.note_action(
                &NewAction::new("org.host.start", ORIGIN, format!("site served at {}", st.url))
                    .params(json!({ "addr": st.addr, "orgs": st.orgs, "mirrors": st.mirrors, "cert": st.cert.source })),
            );
            println!("serving at {}", st.url);
            for o in &st.orgs {
                println!("  {}o/{o}/", st.url);
            }
            for o in &st.mirrors {
                println!("  {}o/{o}/  (mirror)", st.url);
            }
            print_cert(&st.cert);
            if let Some(a) = &st.http_addr {
                println!("ACME challenges on http://{a}/");
            }
            ctx.io.note("Ctrl-C (or `mnnn org host stop`) stops it.");
            let stop = Arc::new(AtomicBool::new(false));
            let s2 = stop.clone();
            let _ = ctrlc::set_handler(move || s2.store(true, Ordering::SeqCst));
            let deadline = for_secs.map(|s| Instant::now() + Duration::from_secs(s));
            let mut last = Instant::now() - Duration::from_secs(10);
            while !stop.load(Ordering::SeqCst) && deadline.is_none_or(|d| Instant::now() < d) {
                if last.elapsed() >= Duration::from_secs(5) {
                    host::write_status(&home, &h.status())?;
                    last = Instant::now();
                }
                if host::stop_requested(&home) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(200));
            }
            h.stop();
            host::clear(&home);
            n.store.note_action(&NewAction::new("org.host.stop", ORIGIN, "site stopped"));
            ctx.io.note("site stopped");
            Ok(())
        }
        OrgHostCommand::Autostart { state } => {
            let mut prefs = ctx.prefs()?;
            if prefs.hosting.autostart != state.on() {
                prefs.hosting.autostart = state.on();
                prefs.save(&ctx.paths)?;
                if let Some(s) = ctx.store_if_unlocked() {
                    s.note_action(
                        &NewAction::new("org.host.config", ORIGIN, format!("site autostart {}", if state.on() { "on" } else { "off" }))
                            .params(json!({ "autostart": state.on() })),
                    );
                }
            }
            ctx.io.info(if state.on() { "the site starts with the node" } else { "the site starts only when asked (`mnnn org host start`)" });
            Ok(())
        }
        OrgHostCommand::Stop => match host::running(&home) {
            Some(f) => {
                host::request_stop(&home)?;
                ctx.io.info(format!("asked the host at {} (process {}) to stop", f.url, f.pid));
                Ok(())
            }
            None => {
                ctx.io.note("not serving");
                Ok(())
            }
        },
        OrgHostCommand::Status { json } => {
            let f = host::running(&home);
            if json {
                return pretty(&json!({ "running": f.is_some(), "host": f }));
            }
            match f {
                Some(f) => {
                    println!("serving at {} (process {})", f.url, f.pid);
                    for o in &f.orgs {
                        println!("  {}o/{o}/", f.url);
                    }
                    for o in &f.mirrors {
                        println!("  {}o/{o}/  (mirror)", f.url);
                    }
                    print_cert(&f.cert);
                }
                None => {
                    let p = ctx.prefs()?;
                    println!("not serving (would listen on https://{}:{}/)", p.hosting.bind, p.hosting.port);
                }
            }
            Ok(())
        }
    }
}

fn print_cert(c: &host::CertInfo) {
    println!("certificate: {} for {} until {} ({})", c.source, c.names.join(", "), fmt_ts(c.not_after), c.issuer);
}

fn cert(ctx: &Ctx, cmd: OrgCertCommand) -> anyhow::Result<()> {
    match cmd {
        OrgCertCommand::Status { json } => {
            let home = &ctx.paths.home;
            let n = Net::open(ctx)?;
            let p = &n.prefs.hosting;
            let acme = Acme::from_prefs(&p.acme).ok().flatten();
            let info = match host::running(home) {
                Some(f) => Some(f.cert),
                None => match &acme {
                    Some(a) => host::stored_cert(&n.store, &a.domain)?.map(|c| host::cert_info(&c, "acme")).transpose()?,
                    None => None,
                },
            };
            if json {
                return pretty(&json!({
                    "certificate": info,
                    "acme": { "enabled": p.acme.enabled, "domain": p.acme.domain, "email": p.acme.email, "directory": p.acme.directory_url(), "profile": p.acme.profile, "http_port": p.acme.http_port },
                    "self_signed_days": p.cert_days,
                }));
            }
            match &info {
                Some(c) => print_cert(c),
                None => println!("certificate: self-signed, made when the host starts, valid {} days", p.cert_days),
            }
            if p.acme.enabled {
                println!(
                    "ACME: {} for {} ({}), profile {}, challenges on port {}",
                    p.acme.directory_url(),
                    p.acme.domain,
                    p.acme.email,
                    if p.acme.profile.is_empty() { "default" } else { &p.acme.profile },
                    p.acme.http_port
                );
            } else {
                println!("ACME: off (`mnnn org cert acme --domain D --email E`)");
            }
            Ok(())
        }
        OrgCertCommand::Acme { domain, email, directory, profile, http_port, off } => {
            let mut prefs = ctx.prefs()?;
            let a = &mut prefs.hosting.acme;
            if let Some(d) = domain {
                a.domain = d;
            }
            if let Some(e) = email {
                a.email = e;
            }
            if let Some(d) = directory {
                a.directory = d;
            }
            if let Some(p) = profile {
                a.profile = p;
            }
            if let Some(p) = http_port {
                a.http_port = p;
            }
            a.enabled = !off;
            Acme::from_prefs(a)?;
            let summary = if a.enabled { format!("ACME on for {} ({})", a.domain, a.directory_url()) } else { "ACME off".to_string() };
            let params =
                json!({ "enabled": a.enabled, "domain": a.domain, "directory": a.directory_url(), "profile": a.profile, "http_port": a.http_port });
            prefs.save(&ctx.paths)?;
            let n = Net::open(ctx)?;
            n.store.note_action(&NewAction::new("org.host.acme", ORIGIN, summary.clone()).params(params));
            ctx.io.info(summary);
            if !off {
                ctx.io.note("The host gets a certificate when it starts and renews it by itself; the domain must point here and port 80 reach the challenge port.");
            }
            Ok(())
        }
        OrgCertCommand::Renew => {
            let home = &ctx.paths.home;
            if let Some(f) = host::running(home) {
                bail!("the host at {} renews by itself; stop it first to renew now", f.url);
            }
            let n = Net::open(ctx)?;
            let mut opts = HostOptions::from_prefs(&n.prefs.hosting)?;
            if opts.acme.is_none() {
                bail!("ACME is off; set it up with `mnnn org cert acme --domain D --email E`");
            }
            opts.auto_renew = false;
            let h = Host::start(n.second_store()?, opts)?;
            let r = h.renew();
            h.stop();
            match r? {
                Some(c) => print_cert(&c),
                None => ctx.io.note("nothing to renew"),
            }
            Ok(())
        }
    }
}
