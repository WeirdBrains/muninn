//! Organizations (`mnnn org` / TUI Orgs): list and writes.
//! Create, edit, remove, roles, members, resources, join/apply/leave and hosting,
//! plus templates, domains, certificates/ACME, browse, fetch/give/mirror, endpoints,
//! org projects, code serve and member sign-in.
//! The store opens read-only for the overview and for writes via [`open_write`].
//! Invite tokens, host certificates and private keys stay in Rust; Dart sees public status only.
use super::data::{open, open_write};
use super::info::paths;
use anyhow::Context;
use muninn_core::ingest::time::fmt_ts;
use muninn_core::net::{self, Node, client, identity, local};
use muninn_core::org::host::{self, Acme, Host, HostOptions};
use muninn_core::org::net as onet;
use muninn_core::org::tls;
use muninn_core::org::{self, Actor, Leftovers};
use muninn_core::prefs::Preferences;
use muninn_core::store::net::short_peer;
use muninn_core::store::{Access, EndpointKind, NewAction, Org, OrgChange, Published, ResourceState, ShareKind, Store, Visibility};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

const ORIGIN: &str = "app";
const OWNER: Actor = Actor::Owner;

pub struct OrgRoleInfo {
    pub name: String,
    pub rank: i64,
    pub perms: Vec<String>,
}

pub struct OrgMemberInfo {
    pub short_id: String,
    pub title: String,
    pub role: String,
    pub status: String,
    pub note: String,
}

pub struct OrgResourceInfo {
    pub kind: String,
    pub name: String,
    pub state: String,
    pub published: String,
}

pub struct OrgEndpointInfo {
    pub name: String,
    pub kind: String,
    pub perm: String,
    pub anyone: bool,
}

/// One organization this node owns, with the fields `mnnn org show` prints.
pub struct OrgInfo {
    pub id: String,
    pub short_id: String,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub visibility: String,
    pub access: String,
    pub hosted: bool,
    pub member_auth: bool,
    pub gated: bool,
    pub members: i64,
    pub apply_role: String,
    pub invite_role: String,
    pub template: String,
    pub roles: Vec<OrgRoleInfo>,
    pub member_list: Vec<OrgMemberInfo>,
    pub resources: Vec<OrgResourceInfo>,
    pub projects: Vec<String>,
    pub endpoints: Vec<OrgEndpointInfo>,
    pub templates: Vec<String>,
}

/// An org on another node that this node applied to or belongs to.
pub struct OrgJoined {
    pub name: String,
    pub slug: String,
    pub status: String,
    pub role: String,
    pub node: String,
    pub perms: Vec<String>,
}

/// A public copy of another node's org, served here (no snapshot bytes).
pub struct OrgMirror {
    pub slug: String,
    pub fetched_at: i64,
}

pub struct OrgOverview {
    pub mine: Vec<OrgInfo>,
    pub joined: Vec<OrgJoined>,
    pub mirrors: Vec<OrgMirror>,
}

/// What `mnnn org host status` shows (no certificate private key).
pub struct OrgHostStatus {
    pub running: bool,
    /// This process started the site (TUI in-process host).
    pub ours: bool,
    pub url: String,
    pub addr: String,
    pub pid: Option<i64>,
    pub orgs: Vec<String>,
    pub mirrors: Vec<String>,
    pub cert_source: String,
    pub http_addr: Option<String>,
    pub autostart: bool,
}

fn role_named(roles: &[muninn_core::store::Role], id: &Option<String>) -> String {
    id.as_deref().and_then(|r| roles.iter().find(|x| x.id == r)).map(|r| r.name.clone()).unwrap_or_else(|| "-".into())
}

fn role_info(r: &muninn_core::store::Role) -> OrgRoleInfo {
    OrgRoleInfo { name: r.name.clone(), rank: r.rank, perms: r.perms.clone() }
}

fn mine_of(s: &Store, o: &Org) -> anyhow::Result<OrgInfo> {
    let card = org::card(s, o);
    let roles = s.roles(&o.id)?;
    let members = s.members(&o.id)?;
    let resources = s.resources(&o.id)?;
    let projects = s.org_projects(&o.id)?;
    let endpoints = s.endpoints(&o.id)?;
    let templates = s.templates(&o.id)?;
    let template =
        o.template_id.as_deref().and_then(|t| templates.iter().find(|x| x.id == t)).map(|t| t.name.clone()).unwrap_or_else(|| "default".into());
    Ok(OrgInfo {
        id: o.id.clone(),
        short_id: o.short_id().to_string(),
        name: o.name.clone(),
        slug: o.slug.clone(),
        description: o.description.clone(),
        visibility: o.visibility.as_str().into(),
        access: o.access.as_str().into(),
        hosted: o.hosted,
        member_auth: o.member_auth,
        gated: o.gated,
        members: card.members as i64,
        apply_role: role_named(&roles, &o.apply_role),
        invite_role: role_named(&roles, &o.invite_role),
        template,
        roles: roles.iter().map(role_info).collect(),
        member_list: members
            .iter()
            .map(|m| OrgMemberInfo {
                short_id: m.short_id().to_string(),
                title: m.title(),
                role: org::role_name(s, m),
                status: m.status.as_str().into(),
                note: m.note.clone(),
            })
            .collect(),
        resources: resources
            .iter()
            .map(|r| OrgResourceInfo {
                kind: r.kind.label().into(),
                name: r.name.clone(),
                state: r.state.as_str().into(),
                published: r.published.as_str().into(),
            })
            .collect(),
        projects: projects.into_iter().map(|p| p.name).collect(),
        endpoints: endpoints
            .into_iter()
            .map(|e| OrgEndpointInfo { name: e.name, kind: e.kind.as_str().into(), perm: e.perm, anyone: e.anyone })
            .collect(),
        templates: templates.into_iter().map(|t| t.name).collect(),
    })
}

fn joined_of(j: &muninn_core::store::Joined) -> OrgJoined {
    OrgJoined {
        name: j.name.clone(),
        slug: j.slug.clone(),
        status: j.status.as_str().into(),
        role: j.role.clone(),
        node: short_peer(&j.node_peer).to_string(),
        perms: j.perms.clone(),
    }
}

pub(crate) fn org_overview_of(s: &Store) -> anyhow::Result<OrgOverview> {
    let mut mine = Vec::new();
    for o in s.orgs()? {
        mine.push(mine_of(s, &o)?);
    }
    let joined = s.joined()?.into_iter().map(|j| joined_of(&j)).collect();
    let mirrors = s.mirrors()?.into_iter().map(|m| OrgMirror { slug: m.slug, fetched_at: m.fetched_at }).collect();
    Ok(OrgOverview { mine, joined, mirrors })
}

/// `mnnn org list` plus the fields `mnnn org show` prints for orgs this node owns.
pub fn org_overview() -> anyhow::Result<OrgOverview> {
    org_overview_of(&open()?)
}

fn prefs_of(home: &Path) -> Preferences {
    std::fs::read_to_string(home.join("preferences.toml")).ok().and_then(|s| toml::from_str(&s).ok()).unwrap_or_default()
}

fn edit_prefs(home: Option<String>, f: impl FnOnce(&mut Preferences)) -> anyhow::Result<()> {
    let p = paths(home)?;
    let mut prefs = Preferences::load_or_create(&p)?;
    f(&mut prefs);
    prefs.save(&p)?;
    Ok(())
}

/// Hosted node if this process started one, otherwise an ephemeral node (same as `mnnn org join`).
macro_rules! with_live_node {
    ($home:expr, $store:expr, |$s:ident, $n:ident, $kp:ident| $body:expr) => {{
        let _prefs = prefs_of($home);
        let $kp = identity::load_or_create($store)?;
        let _my = local::my_label($store, &_prefs.net);
        let _owned = super::net::hosted_node();
        let _ephemeral;
        let $n: &Node = if let Some(n) = _owned.as_ref() {
            n
        } else {
            _ephemeral = local::ephemeral(&$kp, open_write()?, &_my, &_prefs.net)?;
            &_ephemeral
        };
        let $s = $store;
        $body
    }};
}

fn announcement(store: &Store, home: &Path, addr: &str, open: bool) {
    let Some(node) = super::net::hosted_node() else { return };
    let prefs = prefs_of(home);
    if !org::announce::worth_it(store, &prefs.hosting, &prefs.net.external_addresses, addr, open).unwrap_or(false) {
        return;
    }
    let _ = org::announce::announce_hosted(store, &node, &prefs.hosting, &prefs.net.external_addresses, addr, open, &prefs.net.relays);
}

fn invitation(s: &Store, key: &str) -> anyhow::Result<muninn_core::store::Exchange> {
    let reqs: Vec<_> = net::auto::requests(s)?
        .into_iter()
        .filter(|x| {
            x.scope.get("kind").and_then(|v| v.as_str()) == Some("organization") && x.scope.get("as").and_then(|v| v.as_str()) == Some("invite")
        })
        .collect();
    let k = key.trim().to_lowercase();
    let field = |x: &muninn_core::store::Exchange, f: &str| x.scope.get(f).and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
    reqs.iter()
        .find(|x| field(x, "name") == k || field(x, "slug") == k)
        .or_else(|| reqs.iter().find(|x| x.id.starts_with(&k) || x.title().to_lowercase() == k))
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("no invitation to `{key}` here (Net ▸ Invites lists them; or paste the invite link)"))
}

pub(crate) fn create_of(s: &Store, name: &str, slug: Option<&str>, description: &str, public: bool, open: bool) -> anyhow::Result<OrgInfo> {
    let name = name.trim();
    if name.is_empty() {
        return Err(anyhow::anyhow!("give the organization a name"));
    }
    let mut o = org::create(s, ORIGIN, name, slug.filter(|x| !x.trim().is_empty()), description)?;
    if public || open {
        let c = OrgChange { visibility: public.then_some(Visibility::Public), access: open.then_some(Access::Open), ..Default::default() };
        o = org::edit(s, ORIGIN, &OWNER, &o, &c)?;
    }
    mine_of(s, &o)
}

/// `mnnn org create` / TUI New organization.
pub fn org_create(name: String, slug: Option<String>, description: String, public: bool, open: bool) -> anyhow::Result<OrgInfo> {
    create_of(&open_write()?, &name, slug.as_deref(), &description, public, open)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn edit_of(
    s: &Store,
    org: &str,
    name: Option<String>,
    slug: Option<String>,
    description: Option<String>,
    visibility: Option<String>,
    access: Option<String>,
    hosted: Option<bool>,
    member_auth: Option<bool>,
    gated: Option<bool>,
    template: Option<String>,
    apply_role: Option<String>,
    invite_role: Option<String>,
) -> anyhow::Result<OrgInfo> {
    let o = s.find_org(org)?;
    let template_id = match template.as_deref().map(str::trim) {
        None => None,
        Some("default" | "none" | "") => Some(None),
        Some(t) => Some(Some(s.find_template(&o.id, t)?.id)),
    };
    let apply = match apply_role.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(r) => Some(s.find_role(&o.id, r)?.id),
    };
    let invite = match invite_role.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(r) => Some(s.find_role(&o.id, r)?.id),
    };
    let c = OrgChange {
        name,
        slug,
        description,
        visibility: visibility.as_deref().map(Visibility::parse).transpose()?,
        access: access.as_deref().map(Access::parse).transpose()?,
        hosted,
        member_auth,
        gated,
        template_id,
        apply_role: apply,
        invite_role: invite,
    };
    let o = org::edit(s, ORIGIN, &OWNER, &o, &c)?;
    mine_of(s, &o)
}

/// `mnnn org edit` / TUI rename, visibility, access, hosted, member sign-in, gated, defaults.
#[allow(clippy::too_many_arguments)]
pub fn org_edit(
    org: String,
    name: Option<String>,
    slug: Option<String>,
    description: Option<String>,
    visibility: Option<String>,
    access: Option<String>,
    hosted: Option<bool>,
    member_auth: Option<bool>,
    gated: Option<bool>,
    template: Option<String>,
    apply_role: Option<String>,
    invite_role: Option<String>,
) -> anyhow::Result<OrgInfo> {
    edit_of(&open_write()?, &org, name, slug, description, visibility, access, hosted, member_auth, gated, template, apply_role, invite_role)
}

pub(crate) fn remove_of(s: &Store, org: &str) -> anyhow::Result<String> {
    let o = s.find_org(org)?;
    let o = org::remove(s, ORIGIN, &OWNER, &o)?;
    Ok(format!("removed {} (its resources stay here)", o.name))
}

/// `mnnn org remove` / TUI delete (owner only). Resources stay on this node.
pub fn org_remove(org: String) -> anyhow::Result<String> {
    remove_of(&open_write()?, &org)
}

pub(crate) fn create_role_of(s: &Store, org: &str, name: &str, rank: i64, perms: &[String]) -> anyhow::Result<OrgRoleInfo> {
    let o = s.find_org(org)?;
    let name = name.trim();
    if name.is_empty() {
        return Err(anyhow::anyhow!("give the role a name"));
    }
    Ok(role_info(&org::create_role(s, ORIGIN, &OWNER, &o, name, rank, perms)?))
}

/// `mnnn org role add` / TUI new role (`name [rank] perms…`).
pub fn org_role_create(org: String, name: String, rank: i64, perms: Vec<String>) -> anyhow::Result<OrgRoleInfo> {
    create_role_of(&open_write()?, &org, &name, rank, &perms)
}

pub(crate) fn edit_role_of(
    s: &Store,
    org: &str,
    role: &str,
    name: Option<String>,
    rank: Option<i64>,
    perms: Option<Vec<String>>,
) -> anyhow::Result<OrgRoleInfo> {
    let o = s.find_org(org)?;
    let r = s.find_role(&o.id, role)?;
    Ok(role_info(&org::edit_role(s, ORIGIN, &OWNER, &o, &r, name.as_deref(), rank, perms.as_deref())?))
}

/// `mnnn org role edit` / TUI role permissions.
pub fn org_role_edit(org: String, role: String, name: Option<String>, rank: Option<i64>, perms: Option<Vec<String>>) -> anyhow::Result<OrgRoleInfo> {
    edit_role_of(&open_write()?, &org, &role, name, rank, perms)
}

pub(crate) fn remove_role_of(s: &Store, org: &str, role: &str) -> anyhow::Result<String> {
    let o = s.find_org(org)?;
    let r = s.find_role(&o.id, role)?;
    let r = org::remove_role(s, ORIGIN, &OWNER, &o, &r)?;
    Ok(format!("removed role {}", r.name))
}

pub fn org_role_remove(org: String, role: String) -> anyhow::Result<String> {
    remove_role_of(&open_write()?, &org, &role)
}

pub(crate) fn member_role_of(s: &Store, org: &str, member: &str, role: &str) -> anyhow::Result<String> {
    let o = s.find_org(org)?;
    let m = s.find_member(&o.id, member)?;
    let r = s.find_role(&o.id, role)?;
    let m = org::set_role(s, ORIGIN, &OWNER, &o, &m, &r)?;
    Ok(format!("{} is now {}", m.title(), r.name))
}

pub fn org_member_role(org: String, member: String, role: String) -> anyhow::Result<String> {
    member_role_of(&open_write()?, &org, &member, &role)
}

pub(crate) fn member_reject_of(s: &Store, org: &str, member: &str) -> anyhow::Result<String> {
    let o = s.find_org(org)?;
    let m = s.find_member(&o.id, member)?;
    let m = onet::reject(s, ORIGIN, &OWNER, &o, &m)?;
    Ok(format!("rejected {}", m.title()))
}

pub fn org_member_reject(org: String, member: String) -> anyhow::Result<String> {
    member_reject_of(&open_write()?, &org, &member)
}

pub(crate) fn resource_add_of(s: &Store, org: &str, kind: &str, name: &str) -> anyhow::Result<String> {
    let o = s.find_org(org)?;
    let Some(kind) = ShareKind::parse(kind) else {
        return Err(anyhow::anyhow!("what kind is {kind:?}? model, search, journal, project, report or code"));
    };
    if name.trim().is_empty() {
        return Err(anyhow::anyhow!("name the {} to give the org", kind.label()));
    }
    let (r, _) = org::transfer(s, ORIGIN, &OWNER, &o, kind, name.trim(), None)?;
    Ok(format!("{} {} belongs to {}", kind.label(), r.name, o.name))
}

/// TUI "Give the org: kind name" / `mnnn org resource add`.
pub fn org_resource_add(org: String, kind: String, name: String) -> anyhow::Result<String> {
    resource_add_of(&open_write()?, &org, &kind, &name)
}

pub(crate) fn resource_publish_of(s: &Store, org: &str, resource: &str) -> anyhow::Result<String> {
    let o = s.find_org(org)?;
    let r = s.find_resource(&o.id, resource)?;
    let to = match r.published {
        Published::No => Published::Members,
        Published::Members => Published::Public,
        Published::Public => Published::No,
    };
    let r = org::set_published(s, ORIGIN, &OWNER, &o, &r, to)?;
    Ok(format!("{}: published {}", r.name, r.published.as_str()))
}

/// TUI cycle publish (no → members → public → no).
pub fn org_resource_publish(org: String, resource: String) -> anyhow::Result<String> {
    resource_publish_of(&open_write()?, &org, &resource)
}

pub(crate) fn resource_archive_of(s: &Store, org: &str, resource: &str) -> anyhow::Result<String> {
    let o = s.find_org(org)?;
    let r = s.find_resource(&o.id, resource)?;
    let to = if r.state == ResourceState::Archived { ResourceState::Active } else { ResourceState::Archived };
    let r = org::set_state(s, ORIGIN, &OWNER, &o, &r, to)?;
    Ok(format!("{}: {}", r.name, r.state.as_str()))
}

/// TUI archive / restore toggle.
pub fn org_resource_archive(org: String, resource: String) -> anyhow::Result<String> {
    resource_archive_of(&open_write()?, &org, &resource)
}

pub(crate) fn resource_release_of(s: &Store, org: &str, resource: &str) -> anyhow::Result<String> {
    let o = s.find_org(org)?;
    let r = s.find_resource(&o.id, resource)?;
    let r = org::release(s, ORIGIN, &OWNER, &o, &r)?;
    Ok(format!("{} is no longer {}'s (it stays here)", r.name, o.name))
}

pub fn org_resource_release(org: String, resource: String) -> anyhow::Result<String> {
    resource_release_of(&open_write()?, &org, &resource)
}

/// `mnnn org join` / TUI join with an invitation token or link.
pub fn org_join(home: Option<String>, token: String) -> anyhow::Result<String> {
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err(anyhow::anyhow!("paste an invitation token or link"));
    }
    let p = paths(home)?;
    let store = open_write()?;
    let raw = net::exchange::unlink(&token);
    let token = if net::exchange::is_token(&raw) { raw } else { invitation(&store, &token)?.id };
    let prefs = prefs_of(&p.home);
    let my = local::my_label(&store, &prefs.net);
    with_live_node!(&p.home, &store, |s, n, kp| {
        let addrs = local::offer_addrs(&p.home, s, Some(n));
        let a = client::accept(s, Some(n), &kp, &token, "", &my, addrs, ORIGIN)?;
        let mut msg = match s.joined_by_exchange(&a.exchange.id)? {
            Some(j) => format!("joined {} as {}", j.name, j.role),
            None => "the key exchange is done, but it was not an invitation to an organization".into(),
        };
        if let Some(t) = &a.answer
            && !a.delivered.as_ref().is_some_and(|d| d.is_ok())
        {
            msg.push_str(&format!("; their node was not reachable. Answer: {}", net::exchange::link(t)));
        }
        Ok(msg)
    })
}

/// `mnnn org apply` / TUI apply to an org on another node.
pub fn org_apply(home: Option<String>, target: String, org: String, note: String) -> anyhow::Result<String> {
    let target = target.trim().to_string();
    let org = org.trim().to_string();
    if target.is_empty() || org.is_empty() {
        return Err(anyhow::anyhow!("apply to NODE and name the organization"));
    }
    let p = paths(home)?;
    let store = open_write()?;
    let prefs = prefs_of(&p.home);
    let my = local::my_label(&store, &prefs.net);
    let relays = prefs.net.relays.clone();
    with_live_node!(&p.home, &store, |s, n, kp| {
        let j = onet::apply(s, n, &kp, &target, &org, &note, &my, &relays, ORIGIN)?;
        Ok(format!("applied to {} ({})", j.name, j.status.as_str()))
    })
}

/// `mnnn org sync`.
pub fn org_sync(home: Option<String>, org: String) -> anyhow::Result<String> {
    let p = paths(home)?;
    let store = open_write()?;
    let j = store.find_joined(&org)?;
    let relays = prefs_of(&p.home).net.relays;
    with_live_node!(&p.home, &store, |s, n, kp| {
        let _ = kp;
        let j = onet::sync(s, n, &j, &relays, ORIGIN)?;
        Ok(format!("{}: {} as {}", j.name, j.status.as_str(), j.role))
    })
}

/// `mnnn org leave` / TUI leave.
pub fn org_leave(home: Option<String>, org: String) -> anyhow::Result<String> {
    let p = paths(home)?;
    let store = open_write()?;
    let j = store.find_joined(&org)?;
    let relays = prefs_of(&p.home).net.relays;
    with_live_node!(&p.home, &store, |s, n, kp| {
        let _ = kp;
        let j = onet::leave(s, Some(n), &j, &relays, ORIGIN)?;
        Ok(format!("left {}", j.name))
    })
}

/// `mnnn org member invite` / TUI invite (empty `to`: a token they join with).
pub fn org_member_invite(home: Option<String>, org: String, to: Option<String>) -> anyhow::Result<String> {
    let p = paths(home)?;
    let store = open_write()?;
    let o = store.find_org(&org)?;
    let prefs = prefs_of(&p.home);
    let relays = prefs.net.relays.clone();
    let my = local::my_label(&store, &prefs.net);
    let to = to.as_deref().map(str::trim).filter(|s| !s.is_empty());
    with_live_node!(&p.home, &store, |s, n, kp| {
        let (m, c) = onet::invite(s, to.map(|_| n), &kp, &OWNER, &o, None, to, "", &relays, ORIGIN)?;
        let mut msg = format!("invited {} to {} as {}", m.title(), o.name, org::role_name(s, &m));
        if let Some(d) = client::describe_delivery(&c.delivered) {
            msg.push_str(&format!("; {d}"));
        }
        msg.push_str(&format!(". Invitation token from {my}: {}", c.token));
        Ok(msg)
    })
}

/// `mnnn org member approve` / TUI approve an application.
pub fn org_member_approve(home: Option<String>, org: String, member: String) -> anyhow::Result<String> {
    let p = paths(home)?;
    let store = open_write()?;
    let o = store.find_org(&org)?;
    let m = store.find_member(&o.id, &member)?;
    with_live_node!(&p.home, &store, |s, n, kp| {
        let (m, told) = onet::approve(s, Some(n), &kp, &OWNER, &o, &m, None, ORIGIN)?;
        let mut msg = format!("approved {} as {}", m.title(), org::role_name(s, &m));
        if let Some(Err(e)) = told {
            msg.push_str(&format!("; they were not reachable ({e})"));
        }
        Ok(msg)
    })
}

/// `mnnn org member kick` / TUI kick (`leftovers`: keep, archive or publish).
pub fn org_member_kick(home: Option<String>, org: String, member: String, leftovers: String) -> anyhow::Result<String> {
    let left = Leftovers::parse(if leftovers.trim().is_empty() { "keep" } else { leftovers.trim() })?;
    let p = paths(home)?;
    let store = open_write()?;
    let o = store.find_org(&org)?;
    let m = store.find_member(&o.id, &member)?;
    let relays = prefs_of(&p.home).net.relays;
    with_live_node!(&p.home, &store, |s, n, kp| {
        let _ = kp;
        let m = onet::kick(s, Some(n), &OWNER, &o, &m, left, &relays, ORIGIN)?;
        Ok(format!("removed {}", m.title()))
    })
}

struct HostedSite {
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

static SITE: LazyLock<Mutex<Option<HostedSite>>> = LazyLock::new(|| Mutex::new(None));

fn reap_site() {
    let mut g = SITE.lock().unwrap_or_else(|e| e.into_inner());
    let dead = g.as_ref().is_some_and(|d| d.thread.as_ref().is_some_and(|t| t.is_finished()));
    if !dead {
        return;
    }
    if let Some(mut d) = g.take() {
        d.stop.store(true, Ordering::SeqCst);
        if let Some(t) = d.thread.take() {
            let _ = t.join();
        }
    }
}

fn ours_running() -> bool {
    reap_site();
    SITE.lock().ok().is_some_and(|g| g.is_some())
}

fn site_status(home: &Path) -> OrgHostStatus {
    let prefs = prefs_of(home);
    let ours = ours_running();
    match host::running(home) {
        Some(f) => OrgHostStatus {
            running: true,
            ours,
            url: f.url,
            addr: f.addr,
            pid: Some(f.pid as i64),
            orgs: f.orgs,
            mirrors: f.mirrors,
            cert_source: f.cert.source,
            http_addr: f.http_addr,
            autostart: prefs.hosting.autostart,
        },
        None => OrgHostStatus {
            running: false,
            ours: false,
            url: String::new(),
            addr: String::new(),
            pid: None,
            orgs: vec![],
            mirrors: vec![],
            cert_source: String::new(),
            http_addr: None,
            autostart: prefs.hosting.autostart,
        },
    }
}

/// `mnnn org host status`.
pub fn org_host_status(home: Option<String>) -> anyhow::Result<OrgHostStatus> {
    Ok(site_status(&paths(home)?.home))
}

/// `mnnn org host start` / TUI Start site: serve hosted orgs in this process.
pub fn org_host_start(home: Option<String>) -> anyhow::Result<OrgHostStatus> {
    reap_site();
    let p = paths(home)?;
    let home = p.home.clone();
    if ours_running() {
        return Ok(site_status(&home));
    }
    if let Some(f) = host::running(&home) {
        return Err(anyhow::anyhow!("already served at {} by process {}", f.url, f.pid));
    }
    let _ = open_write()?;
    let (tx, rx) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    let home2 = home.clone();
    let thread = thread::spawn(move || {
        let h = match (|| -> anyhow::Result<Host> {
            let store = open_write()?;
            let opts = HostOptions::from_prefs(&prefs_of(&home2).hosting)?;
            Ok(Host::start(store, opts)?)
        })() {
            Ok(h) => h,
            Err(e) => {
                let _ = tx.send(Err(e.to_string()));
                return;
            }
        };
        let st = h.status();
        let _ = host::write_status(&home2, &st);
        let addr = st.addr.clone();
        let _ = tx.send(Ok(st));
        let mut last = Instant::now();
        while !stop2.load(Ordering::SeqCst) && !host::stop_requested(&home2) {
            if last.elapsed() >= Duration::from_secs(5) {
                let _ = host::write_status(&home2, &h.status());
                last = Instant::now();
            }
            thread::sleep(Duration::from_millis(200));
        }
        h.stop();
        host::clear(&home2);
        if let Ok(store) = open_write() {
            store.note_action(&NewAction::new("org.host.stop", ORIGIN, "site stopped"));
            announcement(&store, &home2, &addr, false);
        }
    });
    match rx.recv() {
        Ok(Ok(st)) => {
            *SITE.lock().unwrap_or_else(|e| e.into_inner()) = Some(HostedSite { stop, thread: Some(thread) });
            if let Ok(store) = open_write() {
                store.note_action(
                    &NewAction::new("org.host.start", ORIGIN, format!("site served at {}", st.url))
                        .params(serde_json::json!({ "addr": st.addr, "orgs": st.orgs, "mirrors": st.mirrors, "cert": st.cert.source })),
                );
                announcement(&store, &home, &st.addr, true);
            }
            Ok(site_status(&home))
        }
        Ok(Err(e)) => {
            let _ = thread.join();
            Err(anyhow::anyhow!("{e}"))
        }
        Err(_) => {
            let _ = thread.join();
            Err(anyhow::anyhow!("the site failed to start"))
        }
    }
}

/// `mnnn org host stop` / TUI Stop site.
pub fn org_host_stop(home: Option<String>) -> anyhow::Result<String> {
    reap_site();
    let home = paths(home)?.home;
    {
        let mut g = SITE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(mut d) = g.take() {
            d.stop.store(true, Ordering::SeqCst);
            if let Some(t) = d.thread.take() {
                let _ = t.join();
            }
            return Ok("site stopped".into());
        }
    }
    if let Some(f) = host::running(&home) {
        host::request_stop(&home)?;
        return Ok(format!("asked the site at {} to stop", f.url));
    }
    Ok("the site is not being served".into())
}

/// `mnnn org host autostart` (the site comes up with `net start` when on).
pub fn org_host_autostart(home: Option<String>, on: bool) -> anyhow::Result<String> {
    edit_prefs(home, |prefs| {
        prefs.hosting.autostart = on;
    })?;
    if let Ok(s) = open_write() {
        s.note_action(
            &NewAction::new("org.host.config", ORIGIN, format!("site autostart {}", if on { "on" } else { "off" }))
                .params(serde_json::json!({ "autostart": on })),
        );
    }
    Ok(if on { "the site starts with the node".into() } else { "the site starts only when asked".into() })
}

/// A site template (`mnnn org template show`); HTML/CSS are the user's own markup, not secrets.
pub struct OrgTemplateInfo {
    pub name: String,
    pub in_use: bool,
    pub updated_at: String,
    pub asset_names: Vec<String>,
    pub build_cmd: String,
    pub build_src: String,
    pub build_out: String,
    pub build_approved: bool,
    pub html: String,
    pub css: String,
}

pub struct OrgDomainInfo {
    pub org: String,
    pub domain: String,
}

/// Certificate metadata only. No PEM chain or private key.
pub struct OrgCertInfo {
    pub org: String,
    pub slug: String,
    pub domains: Vec<String>,
    pub source: String,
    pub names: Vec<String>,
    pub not_after: String,
    pub issuer: String,
    pub acme_enabled: bool,
    pub acme_email: String,
    pub retry_attempts: i64,
    pub retry_next: String,
    pub retry_error: String,
}

/// The node's site certificate and ACME settings (no key material).
pub struct OrgNodeCertInfo {
    pub source: String,
    pub names: Vec<String>,
    pub not_after: String,
    pub issuer: String,
    pub acme_enabled: bool,
    pub acme_domain: String,
    pub acme_email: String,
    pub acme_directory: String,
    pub acme_profile: String,
    pub acme_challenge: String,
    pub acme_http_port: i64,
    pub self_signed_days: i64,
}

pub struct OrgCertOverview {
    pub node: OrgNodeCertInfo,
    pub orgs: Vec<OrgCertInfo>,
}

pub struct OrgBrowseItem {
    pub name: String,
    pub slug: String,
    pub visibility: String,
    pub access: String,
    pub members: i64,
    pub description: String,
}

/// A fetched resource. Binary bytes stay in Rust (written to `out` when given).
pub struct OrgFetchResult {
    pub name: String,
    pub format: String,
    pub size: i64,
    pub text: Option<String>,
    pub written: String,
}

pub struct OrgSyncCodeItem {
    pub name: String,
    pub state: String,
}

pub struct OrgCodeServedInfo {
    pub name: String,
    pub lang: String,
    pub short_id: String,
}

fn template_info(s: &Store, o: &Org, t: &muninn_core::store::Template, full: bool) -> anyhow::Result<OrgTemplateInfo> {
    let assets = s.template_assets(&t.id)?;
    Ok(OrgTemplateInfo {
        name: t.name.clone(),
        in_use: o.template_id.as_deref() == Some(t.id.as_str()),
        updated_at: fmt_ts(t.updated_at),
        asset_names: assets.iter().map(|a| a.name.clone()).collect(),
        build_cmd: t.build_cmd.clone().unwrap_or_default(),
        build_src: t.build_src.clone().unwrap_or_default(),
        build_out: t.build_out.clone().unwrap_or_else(|| org::build::DEFAULT_OUT.to_string()),
        build_approved: org::build::approved(s, t)?,
        html: if full { t.html.clone() } else { String::new() },
        css: if full { t.css.clone() } else { String::new() },
    })
}

fn cert_info_of(c: &tls::OrgCert) -> OrgCertInfo {
    let (names, not_after, issuer) = match &c.cert {
        Some(i) => (i.names.clone(), fmt_ts(i.not_after), i.issuer.clone()),
        None => (vec![], String::new(), String::new()),
    };
    let (retry_attempts, retry_next, retry_error) = match &c.retry {
        Some(r) => (r.attempts as i64, fmt_ts(r.next_at), r.error.clone()),
        None => (0, String::new(), String::new()),
    };
    OrgCertInfo {
        org: c.org.clone(),
        slug: c.slug.clone(),
        domains: c.domains.clone(),
        source: c.source.clone(),
        names,
        not_after,
        issuer,
        acme_enabled: c.acme.enabled,
        acme_email: c.acme.email.clone(),
        retry_attempts,
        retry_next,
        retry_error,
    }
}

fn node_cert_of(home: &Path, s: Option<&Store>, prefs: &Preferences) -> OrgNodeCertInfo {
    let p = &prefs.hosting;
    let acme = Acme::from_prefs(&p.acme).ok().flatten();
    let info = match host::running(home) {
        Some(f) => Some(f.cert),
        None => match (&acme, s) {
            (Some(a), Some(store)) => host::stored_cert(store, &a.domain).ok().flatten().and_then(|c| host::cert_info(&c, "acme").ok()),
            _ => None,
        },
    };
    OrgNodeCertInfo {
        source: info.as_ref().map(|c| c.source.clone()).unwrap_or_else(|| "self-signed".into()),
        names: info.as_ref().map(|c| c.names.clone()).unwrap_or_default(),
        not_after: info.as_ref().map(|c| fmt_ts(c.not_after)).unwrap_or_default(),
        issuer: info.as_ref().map(|c| c.issuer.clone()).unwrap_or_default(),
        acme_enabled: p.acme.enabled,
        acme_domain: p.acme.domain.clone(),
        acme_email: p.acme.email.clone(),
        acme_directory: p.acme.directory_url(),
        acme_profile: p.acme.profile.clone(),
        acme_challenge: if p.acme.tls_alpn() { "tls-alpn-01".into() } else { p.acme.challenge.clone() },
        acme_http_port: i64::from(p.acme.http_port),
        self_signed_days: i64::from(p.cert_days),
    }
}

pub(crate) fn templates_of(s: &Store, org: &str) -> anyhow::Result<Vec<OrgTemplateInfo>> {
    let o = s.find_org(org)?;
    let mut out = vec![];
    for t in s.templates(&o.id)? {
        out.push(template_info(s, &o, &t, false)?);
    }
    Ok(out)
}

/// `mnnn org template list`.
pub fn org_templates(org: String) -> anyhow::Result<Vec<OrgTemplateInfo>> {
    templates_of(&open()?, &org)
}

pub(crate) fn template_show_of(s: &Store, org: &str, template: &str) -> anyhow::Result<OrgTemplateInfo> {
    let o = s.find_org(org)?;
    let t = s.find_template(&o.id, template)?;
    template_info(s, &o, &t, true)
}

/// `mnnn org template show`.
pub fn org_template_show(org: String, template: String) -> anyhow::Result<OrgTemplateInfo> {
    template_show_of(&open()?, &org, &template)
}

pub(crate) fn template_save_of(s: &Store, org: &str, name: &str, html: &str, css: &str, use_it: bool) -> anyhow::Result<OrgTemplateInfo> {
    let o = s.find_org(org)?;
    let name = name.trim();
    if name.is_empty() {
        return Err(anyhow::anyhow!("give the template a name"));
    }
    let (h, c) = if html.trim().is_empty() && css.trim().is_empty() && s.find_template(&o.id, name).is_err() {
        (org::serve::DEFAULT_HTML.to_string(), org::serve::DEFAULT_CSS.to_string())
    } else {
        (html.to_string(), css.to_string())
    };
    let t = org::save_template(s, ORIGIN, &OWNER, &o, name, &h, &c)?;
    if use_it {
        org::edit(s, ORIGIN, &OWNER, &o, &OrgChange { template_id: Some(Some(t.id.clone())), ..Default::default() })?;
    }
    let o = s.find_org(org)?;
    template_info(s, &o, &t, true)
}

/// `mnnn org template save|edit`. Empty HTML+CSS on a new name starts from the built-in look.
pub fn org_template_save(org: String, name: String, html: String, css: String, use_it: bool) -> anyhow::Result<OrgTemplateInfo> {
    template_save_of(&open_write()?, &org, &name, &html, &css, use_it)
}

pub(crate) fn template_remove_of(s: &Store, org: &str, template: &str) -> anyhow::Result<String> {
    let o = s.find_org(org)?;
    let t = s.find_template(&o.id, template)?;
    let name = t.name.clone();
    org::remove_template(s, ORIGIN, &OWNER, &o, &t)?;
    Ok(format!("template {name} removed"))
}

/// `mnnn org template remove`.
pub fn org_template_remove(org: String, template: String) -> anyhow::Result<String> {
    template_remove_of(&open_write()?, &org, &template)
}

pub(crate) fn template_add_asset_of(s: &Store, org: &str, template: &str, path: &str, name: Option<&str>) -> anyhow::Result<String> {
    let o = s.find_org(org)?;
    let t = s.find_template(&o.id, template)?;
    let p = Path::new(path);
    let data = std::fs::read(p).with_context(|| p.display().to_string())?;
    let name = name
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| p.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default());
    if name.is_empty() {
        return Err(anyhow::anyhow!("name the asset"));
    }
    let at = org::put_asset(s, ORIGIN, &OWNER, &o, &t, &name, &data)?;
    Ok(format!("asset {name} ({} bytes) at {at}", data.len()))
}

/// `mnnn org template asset`. Rust reads the file; Dart only passes a path.
pub fn org_template_add_asset(org: String, template: String, path: String, name: Option<String>) -> anyhow::Result<String> {
    template_add_asset_of(&open_write()?, &org, &template, &path, name.as_deref())
}

pub(crate) fn template_remove_asset_of(s: &Store, org: &str, template: &str, name: &str) -> anyhow::Result<String> {
    let o = s.find_org(org)?;
    let t = s.find_template(&o.id, template)?;
    org::remove_asset(s, ORIGIN, &OWNER, &o, &t, name)?;
    Ok(format!("asset {name} removed from {}", t.name))
}

/// `mnnn org template asset --remove`.
pub fn org_template_remove_asset(org: String, template: String, name: String) -> anyhow::Result<String> {
    template_remove_asset_of(&open_write()?, &org, &template, &name)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn template_build_of(
    s: &Store,
    org: &str,
    template: &str,
    cmd: Option<String>,
    src: Option<String>,
    out: Option<String>,
    clear: bool,
    no_run: bool,
    approve: bool,
) -> anyhow::Result<String> {
    let o = s.find_org(org)?;
    let mut t = s.find_template(&o.id, template)?;
    if clear {
        org::build::configure(s, ORIGIN, &OWNER, &o, &t, None)?;
        return Ok(format!("build step of {} removed", t.name));
    }
    if cmd.is_some() || src.is_some() || out.is_some() {
        let c = cmd.or(t.build_cmd.clone()).ok_or_else(|| anyhow::anyhow!("give the build command"))?;
        let dir = match src {
            Some(p) => p,
            None => match &t.build_src {
                Some(p) => p.clone(),
                None => std::env::current_dir()?.to_string_lossy().into_owned(),
            },
        };
        let dir = std::fs::canonicalize(&dir).with_context(|| dir.clone())?;
        let dir = dir.to_string_lossy().into_owned();
        let o2 = out.or(t.build_out.clone());
        t = org::build::configure(s, ORIGIN, &OWNER, &o, &t, Some(org::build::Step { cmd: &c, src: &dir, out: o2.as_deref() }))?;
    }
    if no_run {
        return Ok(format!("build step of {} saved", t.name));
    }
    let approved = org::build::approved(s, &t)?;
    if !approve && !approved {
        return Err(anyhow::anyhow!("{}", org::build::question(&t)));
    }
    let b = org::build::build(s, ORIGIN, &OWNER, &o, &t, approve || approved)?;
    let mut msg = format!("built {}: index.html{}, {} other file(s)", t.name, if b.css.is_empty() { "" } else { ", style.css" }, b.assets.len());
    if !b.log.trim().is_empty() {
        msg.push_str(&format!("\n{}", b.log.trim()));
    }
    for n in &b.notes {
        msg.push_str(&format!("\n{n}"));
    }
    Ok(msg)
}

/// `mnnn org template build`. A new or changed command needs `approve`.
#[allow(clippy::too_many_arguments)]
pub fn org_template_build(
    org: String,
    template: String,
    cmd: Option<String>,
    src: Option<String>,
    out: Option<String>,
    clear: bool,
    no_run: bool,
    approve: bool,
) -> anyhow::Result<String> {
    template_build_of(&open_write()?, &org, &template, cmd, src, out, clear, no_run, approve)
}

pub(crate) fn template_preview_of(s: &Store, home: &Path, org: &str, template: &str) -> anyhow::Result<String> {
    let o = s.find_org(org)?;
    let t = s.find_template(&o.id, template)?;
    let token = org::preview(s, ORIGIN, &OWNER, &o, &t)?;
    let path = org::serve::preview_path(&o.slug, &token);
    match host::running(home) {
        Some(f) => Ok(format!("{}{}", f.url, path.trim_start_matches('/'))),
        None => Ok(format!("{path} (the site is not running here; start it to serve this preview for an hour)")),
    }
}

/// `mnnn org template preview`.
pub fn org_template_preview(home: Option<String>, org: String, template: String) -> anyhow::Result<String> {
    let p = paths(home)?;
    template_preview_of(&open_write()?, &p.home, &org, &template)
}

pub(crate) fn domains_of(s: &Store, org: Option<&str>) -> anyhow::Result<Vec<OrgDomainInfo>> {
    let orgs = match org {
        Some(k) => vec![s.find_org(k)?],
        None => s.orgs()?,
    };
    let mut rows = vec![];
    for o in orgs {
        for d in tls::domains(s, &o)? {
            rows.push(OrgDomainInfo { org: o.slug.clone(), domain: d });
        }
    }
    Ok(rows)
}

/// `mnnn org domain list`.
pub fn org_domains(org: Option<String>) -> anyhow::Result<Vec<OrgDomainInfo>> {
    domains_of(&open()?, org.as_deref())
}

pub(crate) fn domain_add_of(s: &Store, org: &str, domain: &str) -> anyhow::Result<String> {
    let o = s.find_org(org)?;
    let d = tls::check_domain(domain)?;
    if tls::add_domain(s, &o, &d)? {
        s.note_action(&NewAction::new("org.domain.add", ORIGIN, format!("{} served at {d}", o.name)).params(json!({ "org": o.id, "domain": d })));
        Ok(format!("{} is served at {d}", o.name))
    } else {
        Ok(format!("{} is served at {d} already", o.name))
    }
}

/// `mnnn org domain add`.
pub fn org_domain_add(org: String, domain: String) -> anyhow::Result<String> {
    domain_add_of(&open_write()?, &org, &domain)
}

pub(crate) fn domain_remove_of(s: &Store, org: &str, domain: &str) -> anyhow::Result<String> {
    let o = s.find_org(org)?;
    if !tls::remove_domain(s, &o, domain)? {
        return Err(anyhow::anyhow!("{} is not served at {domain}", o.name));
    }
    s.note_action(
        &NewAction::new("org.domain.remove", ORIGIN, format!("{} no longer served at {domain}", o.name))
            .params(json!({ "org": o.id, "domain": domain })),
    );
    Ok(format!("{} is no longer served at {domain}", o.name))
}

/// `mnnn org domain remove`.
pub fn org_domain_remove(org: String, domain: String) -> anyhow::Result<String> {
    domain_remove_of(&open_write()?, &org, &domain)
}

fn cert_overview_of(home: &Path, s: &Store, prefs: &Preferences, org: Option<&str>) -> anyhow::Result<OrgCertOverview> {
    let node = node_cert_of(home, Some(s), prefs);
    let orgs = match org {
        Some(k) => vec![cert_info_of(&tls::status(s, &s.find_org(k)?)?)],
        None => tls::all(s)?.into_iter().map(|c| cert_info_of(&c)).collect(),
    };
    Ok(OrgCertOverview { node, orgs })
}

/// `mnnn org cert status`. Metadata only; PEM and keys stay in the store.
pub fn org_cert_status(home: Option<String>, org: Option<String>) -> anyhow::Result<OrgCertOverview> {
    let p = paths(home)?;
    let prefs = prefs_of(&p.home);
    cert_overview_of(&p.home, &open()?, &prefs, org.as_deref())
}

pub(crate) fn cert_set_of(s: &Store, org: &str, cert: &str, key: &str) -> anyhow::Result<String> {
    let o = s.find_org(org)?;
    let pem = host::CertPem {
        chain: std::fs::read_to_string(cert).with_context(|| format!("reading the certificate {cert}"))?,
        key: std::fs::read_to_string(key).with_context(|| format!("reading the key {key}"))?,
    };
    let info = tls::set_user_cert(s, &o, &pem)?;
    let missing: Vec<String> = tls::domains(s, &o)?.into_iter().filter(|d| !info.names.contains(d)).collect();
    s.note_action(
        &NewAction::new("org.cert", ORIGIN, format!("your certificate set for {}", o.name))
            .params(json!({ "org": o.id, "source": tls::USER, "names": info.names, "not_after": info.not_after })),
    );
    let mut msg = format!("{}: your certificate for {} until {}", o.name, info.names.join(", "), fmt_ts(info.not_after));
    if !missing.is_empty() {
        msg.push_str(&format!("; it does not name {}", missing.join(", ")));
    }
    Ok(msg)
}

/// `mnnn org cert set`. Paths only; Rust reads the PEM files and never returns them.
pub fn org_cert_set(org: String, cert: String, key: String) -> anyhow::Result<String> {
    cert_set_of(&open_write()?, &org, &cert, &key)
}

pub(crate) fn cert_clear_of(s: &Store, org: &str) -> anyhow::Result<String> {
    let o = s.find_org(org)?;
    if !tls::clear_user_cert(s, &o)? {
        return Err(anyhow::anyhow!("{} has no certificate of yours", o.name));
    }
    s.note_action(&NewAction::new("org.cert.clear", ORIGIN, format!("your certificate removed from {}", o.name)).params(json!({ "org": o.id })));
    Ok(format!("{}: back to {}", o.name, if tls::acme(s, &o)?.enabled { "ACME" } else { "a self-signed certificate" }))
}

/// `mnnn org cert clear`.
pub fn org_cert_clear(org: String) -> anyhow::Result<String> {
    cert_clear_of(&open_write()?, &org)
}

pub(crate) fn cert_acme_org_of(s: &Store, org: &str, email: Option<String>, off: bool) -> anyhow::Result<String> {
    let o = s.find_org(org)?;
    let mut a = tls::acme(s, &o)?;
    if let Some(e) = email {
        a.email = e.trim().to_string();
    }
    a.enabled = !off;
    if a.enabled && tls::domains(s, &o)?.is_empty() {
        return Err(anyhow::anyhow!("{} has no domains; add one first", o.name));
    }
    tls::set_acme(s, &o, &a)?;
    let summary = format!("ACME {} for {}", if a.enabled { "on" } else { "off" }, o.name);
    s.note_action(&NewAction::new("org.cert.acme", ORIGIN, summary.clone()).params(json!({ "org": o.id, "enabled": a.enabled })));
    Ok(summary)
}

/// `mnnn org cert acme` for an org (`--email` / `--off`) or the node's settings.
#[allow(clippy::too_many_arguments)]
pub fn org_cert_acme(
    home: Option<String>,
    org: Option<String>,
    challenge: Option<String>,
    email: Option<String>,
    domain: Option<String>,
    directory: Option<String>,
    profile: Option<String>,
    http_port: Option<i64>,
    off: bool,
) -> anyhow::Result<String> {
    if let Some(k) = org {
        if challenge.is_some() || domain.is_some() || directory.is_some() || profile.is_some() || http_port.is_some() {
            return Err(anyhow::anyhow!("for an org only email and off apply (directory, profile, challenge and port are the node's)"));
        }
        return cert_acme_org_of(&open_write()?, &k, email, off);
    }
    let p = paths(home)?;
    let mut prefs = Preferences::load_or_create(&p)?;
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
    if let Some(pr) = profile {
        a.profile = pr;
    }
    if let Some(port) = http_port {
        a.http_port = u16::try_from(port).map_err(|_| anyhow::anyhow!("http port out of range"))?;
    }
    if let Some(c) = challenge {
        let c = c.trim().to_ascii_lowercase();
        if !matches!(c.as_str(), "http-01" | "tls-alpn-01") {
            return Err(anyhow::anyhow!("challenge: http-01 or tls-alpn-01"));
        }
        a.challenge = c;
    }
    let settings_only = a.domain.trim().is_empty() && !off;
    a.enabled = !off && !settings_only;
    Acme::from_prefs(a)?;
    let summary = if a.enabled {
        format!("ACME on for {} ({})", a.domain, a.directory_url())
    } else if settings_only {
        format!("ACME settings kept for orgs ({}, {})", a.directory_url(), if a.tls_alpn() { "tls-alpn-01" } else { "http-01" })
    } else {
        "ACME off".into()
    };
    let params = json!({ "enabled": a.enabled, "domain": a.domain, "directory": a.directory_url(), "profile": a.profile, "http_port": a.http_port });
    prefs.save(&p)?;
    let store = open_write()?;
    store.note_action(&NewAction::new("org.host.acme", ORIGIN, summary.clone()).params(params));
    Ok(summary)
}

/// `mnnn org cert renew`. Needs ACME set up and the challenge reachable.
pub fn org_cert_renew(home: Option<String>, org: Option<String>) -> anyhow::Result<String> {
    let p = paths(home)?;
    let home = &p.home;
    match org {
        Some(k) => {
            let store = open_write()?;
            let o = store.find_org(&k)?;
            if host::running(home).is_some() {
                tls::request_renewal(&store, &o)?;
                return Ok(format!("asked the running host to renew {}'s certificate", o.name));
            }
            if !tls::acme(&store, &o)?.enabled {
                return Err(anyhow::anyhow!("ACME is off for {}", o.name));
            }
            let prefs = prefs_of(home);
            let mut opts = HostOptions::from_prefs(&prefs.hosting)?;
            opts.auto_renew = false;
            let h = Host::start(open_write()?, opts)?;
            let r = h.renew_orgs();
            h.stop();
            let certs = r?;
            if certs.is_empty() {
                Ok("nothing to renew".into())
            } else {
                Ok(certs
                    .into_iter()
                    .map(|c| format!("certificate: {} for {} until {}", c.source, c.names.join(", "), fmt_ts(c.not_after)))
                    .collect::<Vec<_>>()
                    .join("\n"))
            }
        }
        None => {
            if let Some(f) = host::running(home) {
                return Err(anyhow::anyhow!("the host at {} renews by itself; stop it first to renew now", f.url));
            }
            let prefs = prefs_of(home);
            let mut opts = HostOptions::from_prefs(&prefs.hosting)?;
            if opts.acme.is_none() {
                return Err(anyhow::anyhow!("ACME is off; set it up first"));
            }
            opts.auto_renew = false;
            let h = Host::start(open_write()?, opts)?;
            let r = h.renew();
            h.stop();
            match r? {
                Some(c) => Ok(format!("certificate: {} for {} until {}", c.source, c.names.join(", "), fmt_ts(c.not_after))),
                None => Ok("nothing to renew".into()),
            }
        }
    }
}

pub(crate) fn endpoint_add_of(
    s: &Store,
    org: &str,
    name: &str,
    kind: &str,
    target: &str,
    perm: Option<&str>,
    anyone: bool,
) -> anyhow::Result<String> {
    let o = s.find_org(org)?;
    let e = org::add_endpoint(s, ORIGIN, &OWNER, &o, name, EndpointKind::parse(kind)?, target, perm, anyone)?;
    Ok(format!("endpoint {} ({}, needs {}); POST /o/{}/api/call/{}", e.name, e.kind.as_str(), e.perm, o.slug, e.name))
}

/// `mnnn org endpoint add`.
pub fn org_endpoint_add(org: String, name: String, kind: String, target: String, perm: Option<String>, anyone: bool) -> anyhow::Result<String> {
    endpoint_add_of(&open_write()?, &org, &name, &kind, &target, perm.as_deref(), anyone)
}

pub(crate) fn endpoint_remove_of(s: &Store, org: &str, name: &str) -> anyhow::Result<String> {
    let o = s.find_org(org)?;
    let e = org::remove_endpoint(s, ORIGIN, &OWNER, &o, name)?;
    Ok(format!("removed endpoint {}", e.name))
}

/// `mnnn org endpoint remove`.
pub fn org_endpoint_remove(org: String, name: String) -> anyhow::Result<String> {
    endpoint_remove_of(&open_write()?, &org, &name)
}

/// `mnnn org call` (your org locally; a joined org through its node).
pub fn org_call(home: Option<String>, org: String, endpoint: String, input: String) -> anyhow::Result<String> {
    let input: Value = serde_json::from_str(if input.trim().is_empty() { "{}" } else { input.trim() }).context("input is not JSON")?;
    let p = paths(home)?;
    let store = open_write()?;
    match store.find_org(&org) {
        Ok(o) => {
            let v = org::call(&store, ORIGIN, &OWNER, &o, &endpoint, &input)?;
            Ok(serde_json::to_string_pretty(&v)?)
        }
        Err(_) => {
            let j = store.find_joined(&org)?;
            let relays = prefs_of(&p.home).net.relays;
            with_live_node!(&p.home, &store, |s, n, kp| {
                let _ = kp;
                let v = onet::request(s, n, &j, onet::OrgReq::Call { endpoint, input }, &relays)?;
                Ok(serde_json::to_string_pretty(&v)?)
            })
        }
    }
}

pub(crate) fn project_add_of(s: &Store, org: &str, project: &str) -> anyhow::Result<String> {
    let o = s.find_org(org)?;
    let p = org::add_project(s, ORIGIN, &OWNER, &o, project)?;
    Ok(format!("{} is a project of {}", p.name, o.name))
}

/// `mnnn org project add`.
pub fn org_project_add(org: String, project: String) -> anyhow::Result<String> {
    project_add_of(&open_write()?, &org, &project)
}

pub(crate) fn project_remove_of(s: &Store, org: &str, project: &str) -> anyhow::Result<String> {
    let o = s.find_org(org)?;
    let p = org::remove_project(s, ORIGIN, &OWNER, &o, project)?;
    Ok(format!("{} is no longer a project of {} (the project stays here)", p.name, o.name))
}

/// `mnnn org project remove`.
pub fn org_project_remove(org: String, project: String) -> anyhow::Result<String> {
    project_remove_of(&open_write()?, &org, &project)
}

pub(crate) fn project_member_of(s: &Store, org: &str, project: &str, member: &str, role: Option<&str>) -> anyhow::Result<String> {
    let o = s.find_org(org)?;
    let m = s.find_member(&o.id, member)?;
    let r = role.map(|r| s.find_role(&o.id, r)).transpose()?;
    org::add_project_member(s, ORIGIN, &OWNER, &o, project, &m, r.as_ref())?;
    Ok(format!("{} is on {project}", m.title()))
}

/// `mnnn org project member`.
pub fn org_project_member(org: String, project: String, member: String, role: Option<String>) -> anyhow::Result<String> {
    project_member_of(&open_write()?, &org, &project, &member, role.as_deref())
}

fn served_of(s: &Store, org: &str) -> anyhow::Result<Vec<OrgCodeServedInfo>> {
    let id = match s.find_org(org) {
        Ok(o) => o.id,
        Err(_) => s.find_joined(org)?.org_id,
    };
    let mut out = vec![];
    for c in s.served_code(&id)? {
        if let Some(cs) = s.code_space(&c)? {
            out.push(OrgCodeServedInfo { name: cs.name.clone(), lang: cs.lang.as_str().into(), short_id: cs.short_id().to_string() });
        }
    }
    Ok(out)
}

/// `mnnn org code list`.
pub fn org_code_list(org: String) -> anyhow::Result<Vec<OrgCodeServedInfo>> {
    served_of(&open()?, &org)
}

/// `mnnn org code serve`.
pub fn org_code_serve(org: String, space: String) -> anyhow::Result<String> {
    let s = open_write()?;
    match s.find_org(&org) {
        Ok(o) => {
            let c = org::serve_code(&s, ORIGIN, &OWNER, &o, &space)?;
            Ok(format!("this node serves {} for {}", c.name, o.name))
        }
        Err(_) => {
            let j = s.find_joined(&org)?;
            let c = org::serve_joined_code(&s, ORIGIN, &j, &space, true)?;
            Ok(format!("this node serves {} for {}", c.name, j.name))
        }
    }
}

/// `mnnn org code stop`.
pub fn org_code_stop(org: String, space: String) -> anyhow::Result<String> {
    let s = open_write()?;
    match s.find_org(&org) {
        Ok(o) => {
            let c = org::unserve_code(&s, ORIGIN, &OWNER, &o, &space)?;
            Ok(format!("this node no longer serves {} for {}", c.name, o.name))
        }
        Err(_) => {
            let j = s.find_joined(&org)?;
            let c = org::serve_joined_code(&s, ORIGIN, &j, &space, false)?;
            Ok(format!("this node no longer serves {} for {}", c.name, j.name))
        }
    }
}

/// `mnnn org browse`. Needs a running or ephemeral node and a reachable peer.
pub fn org_browse(home: Option<String>, target: String, org: Option<String>) -> anyhow::Result<Vec<OrgBrowseItem>> {
    let target = target.trim().to_string();
    if target.is_empty() {
        return Err(anyhow::anyhow!("browse a node (…/p2p/ID or a known peer)"));
    }
    let p = paths(home)?;
    let store = open_write()?;
    let relays = prefs_of(&p.home).net.relays;
    with_live_node!(&p.home, &store, |s, n, kp| {
        let _ = kp;
        let v = onet::browse(s, n, &target, org.as_deref(), &relays)?;
        Ok(v.into_iter()
            .map(|c| OrgBrowseItem {
                name: c.name,
                slug: c.slug,
                visibility: c.visibility.as_str().into(),
                access: c.access.as_str().into(),
                members: c.members as i64,
                description: c.description,
            })
            .collect())
    })
}

/// `mnnn org fetch`. Binary bytes are written to `out` in Rust, never returned.
pub fn org_fetch(home: Option<String>, org: String, resource: String, format: Option<String>, out: Option<String>) -> anyhow::Result<OrgFetchResult> {
    let p = paths(home)?;
    let store = open_write()?;
    let j = store.find_joined(&org)?;
    let relays = prefs_of(&p.home).net.relays;
    with_live_node!(&p.home, &store, |s, n, kp| {
        let _ = kp;
        let it = onet::fetch(s, n, &j, &resource, format.as_deref(), &relays)?;
        let binary = std::str::from_utf8(&it.data).is_err() || matches!(it.format.as_str(), "pdf" | "docx" | "mnnp");
        let mut written = String::new();
        if let Some(path) = &out {
            std::fs::write(path, &it.data).with_context(|| path.clone())?;
            written = path.clone();
        } else if binary {
            return Err(anyhow::anyhow!("binary fetch ({} bytes, {}); give a path to write it", it.data.len(), it.format));
        }
        let text = if binary { None } else { Some(String::from_utf8_lossy(&it.data).into_owned()) };
        Ok(OrgFetchResult { name: it.name, format: it.format, size: it.data.len() as i64, text, written })
    })
}

/// `mnnn org give`.
pub fn org_give(home: Option<String>, org: String, kind: String, name: String) -> anyhow::Result<String> {
    let Some(kind) = ShareKind::parse(&kind) else {
        return Err(anyhow::anyhow!("what kind is {kind:?}? model, search, journal, project, report or code"));
    };
    let p = paths(home)?;
    let store = open_write()?;
    let j = store.find_joined(&org)?;
    let relays = prefs_of(&p.home).net.relays;
    with_live_node!(&p.home, &store, |s, n, kp| {
        let _ = kp;
        onet::transfer(s, n, &j, kind, &name, &relays, ORIGIN)?;
        Ok(format!("gave {} {name} to {} (yours stays as it is)", kind.label(), j.name))
    })
}

/// `mnnn org mirror`. Snapshot bytes stay in the store; Dart sees the slug only.
pub fn org_mirror(home: Option<String>, org: String) -> anyhow::Result<String> {
    let p = paths(home)?;
    let store = open_write()?;
    let j = store.find_joined(&org)?;
    let relays = prefs_of(&p.home).net.relays;
    with_live_node!(&p.home, &store, |s, n, kp| {
        let _ = kp;
        let m = onet::mirror(s, n, &j, &relays, ORIGIN)?;
        Ok(format!("mirrored {} ({} bytes); start the site to serve it at /o/{}/", j.name, m.snapshot.len(), m.slug))
    })
}

/// `mnnn org sync-code`.
pub fn org_sync_code(home: Option<String>, org: String) -> anyhow::Result<Vec<OrgSyncCodeItem>> {
    let p = paths(home)?;
    let store = open_write()?;
    let j = store.find_joined(&org)?;
    let relays = prefs_of(&p.home).net.relays;
    with_live_node!(&p.home, &store, |s, n, kp| {
        let _ = kp;
        let out = onet::sync_code(s, n, &j, &relays, ORIGIN)?;
        Ok(out.into_iter().map(|x| OrgSyncCodeItem { name: x.name, state: x.state.into() }).collect())
    })
}

/// `mnnn org sign-in`: a one-time browser link (valid five minutes). The signing key stays in Rust.
pub fn org_sign_in(org: String, site: String) -> anyhow::Result<String> {
    let store = open()?;
    let j = store.find_joined(&org)?;
    let path = format!("/o/{}/sign-in", j.slug);
    let h = host::authorization(&store, &org, "GET", &path, b"")?;
    let site = site.trim().trim_end_matches('/');
    if site.is_empty() {
        return Err(anyhow::anyhow!("give the site's address, e.g. https://lab.example.org:4743"));
    }
    Ok(format!("{site}{path}?auth={}", host::query_escape(&h)))
}

/// A site a peer announced.
pub struct AnnouncedSite {
    pub peer: String,
    pub slug: String,
    pub name: String,
    pub urls: Vec<String>,
    pub open: bool,
}

/// Sites peers have announced (`mnnn org announced`).
pub fn org_announced() -> anyhow::Result<Vec<AnnouncedSite>> {
    let store = open()?;
    let mut out = Vec::new();
    for (a, c) in org::announce::received(&store)? {
        out.push(AnnouncedSite { peer: a.peer, slug: c.slug, name: c.name, urls: c.urls, open: c.open });
    }
    Ok(out)
}

fn announce_lines(name: &str, r: &org::announce::Report) -> Vec<String> {
    if let Some(why) = &r.skipped {
        return vec![format!("{name} not announced: {why}")];
    }
    let mut v = Vec::new();
    if !r.sent.is_empty() {
        v.push(format!("{name} announced to {}", r.sent.join(", ")));
    }
    for (to, e) in &r.failed {
        v.push(format!("{name}: {to} did not hear it: {e}"));
    }
    if v.is_empty() {
        v.push(format!("{name}: nothing to tell"));
    }
    v
}

/// `mnnn org announce`. An empty org announces every hosted org. `closed` announces the site as shut.
pub fn org_announce(home: Option<String>, org_key: String, closed: bool) -> anyhow::Result<Vec<String>> {
    let p = paths(home.clone())?;
    let running = host::running(&p.home);
    if running.is_none() && !closed {
        anyhow::bail!("the site is not running; start it, or announce it closed");
    }
    let store = open_write()?;
    let prefs = prefs_of(&p.home);
    let addr = running.map(|f| f.addr).unwrap_or_default();
    let held = crate::hold::hold_node(home)?;
    let key = org_key.trim();
    let orgs: Vec<Org> = if key.is_empty() { store.orgs()?.into_iter().filter(|o| o.hosted).collect() } else { vec![store.find_org(key)?] };
    if orgs.is_empty() {
        anyhow::bail!("no org is hosted");
    }
    let mut lines = Vec::new();
    for o in orgs {
        let u = org::announce::urls(
            &store,
            &o,
            &org::announce::bind_of(&addr, &prefs.hosting.bind),
            org::announce::port_of(&addr, prefs.hosting.port),
            &prefs.hosting.names,
            &prefs.net.external_addresses,
        )?;
        let c = org::announce::card(&store, &o, u, !closed)?;
        let r = org::announce::announce(&store, held.node(), &o, &c, &prefs.net.relays)?;
        lines.extend(announce_lines(&o.name, &r));
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::data::lock;
    use muninn_core::store::{Joined, MemberStatus, Mirror, NewRole, Store};

    #[test]
    fn empty_store_has_no_orgs() {
        let s = Store::open_in_memory().unwrap();
        let o = org_overview_of(&s).unwrap();
        assert!(o.mine.is_empty() && o.joined.is_empty() && o.mirrors.is_empty());
    }

    #[test]
    fn lists_mine_joined_and_mirrors_without_snapshots() {
        let s = Store::open_in_memory().unwrap();
        let lab = s.insert_org("Lab", None, "we look", "12D3KooWowner").unwrap();
        assert_eq!(lab.slug, "lab");
        s.save_joined(&Joined {
            org_id: "other".into(),
            name: "Desk".into(),
            slug: "desk".into(),
            node_peer: "12D3KooWabcdefgh".into(),
            exchange_id: None,
            status: MemberStatus::Member,
            role: "user".into(),
            perms: vec!["read".into()],
            info: serde_json::Value::Null,
            created_at: 1,
            updated_at: 1,
        })
        .unwrap();
        s.save_mirror(&Mirror {
            org_id: "m1".into(),
            slug: "copy".into(),
            node_peer: "12D3KooWxyz".into(),
            snapshot: b"not-for-dart".to_vec(),
            fetched_at: 1_704_067_200_000,
        })
        .unwrap();
        let o = org_overview_of(&s).unwrap();
        assert_eq!(o.mine.len(), 1);
        assert_eq!((o.mine[0].name.as_str(), o.mine[0].slug.as_str(), o.mine[0].description.as_str()), ("Lab", "lab", "we look"));
        assert_eq!((o.mine[0].visibility.as_str(), o.mine[0].access.as_str(), o.mine[0].members), ("private", "members", 0));
        assert!(!o.mine[0].hosted);
        assert!(o.mine[0].member_auth, "insert_org leaves member sign-in on (the store default)");
        assert!(o.mine[0].short_id.len() <= 8);
        assert_eq!((o.joined[0].name.as_str(), o.joined[0].status.as_str(), o.joined[0].role.as_str()), ("Desk", "member", "user"));
        assert_eq!(o.joined[0].node, "abcdefgh");
        assert_eq!(o.joined[0].perms, ["read"]);
        assert_eq!((o.mirrors[0].slug.as_str(), o.mirrors[0].fetched_at), ("copy", 1_704_067_200_000));
    }

    #[test]
    fn show_fields_include_roles_and_members() {
        let s = Store::open_in_memory().unwrap();
        let lab = s.insert_org("Lab", None, "", "me").unwrap();
        let owner = s.create_role(&lab.id, &NewRole { name: "owner".into(), rank: 100, perms: vec!["*".into()], builtin: true }).unwrap();
        s.upsert_member(&lab.id, "12D3KooWme", "alice", Some(&owner.id), MemberStatus::Member, None, "", Some("owner")).unwrap();
        let o = org_overview_of(&s).unwrap();
        assert_eq!(o.mine[0].members, 1);
        assert_eq!(o.mine[0].roles[0].name, "owner");
        assert_eq!(o.mine[0].member_list[0].title, "alice");
        assert_eq!(o.mine[0].member_list[0].role, "owner");
        assert_eq!(o.mine[0].member_list[0].status, "member");
    }

    #[test]
    fn create_edit_remove_and_roles() {
        let s = Store::open_in_memory().unwrap();
        let o = create_of(&s, "Lab", None, "we look", false, false).unwrap();
        assert_eq!((o.name.as_str(), o.slug.as_str(), o.description.as_str()), ("Lab", "lab", "we look"));
        assert_eq!((o.visibility.as_str(), o.access.as_str()), ("private", "members"));
        assert!(o.members >= 1);
        let o = create_of(&s, "Open Desk", Some("desk"), "", true, true).unwrap();
        assert_eq!((o.name.as_str(), o.slug.as_str(), o.visibility.as_str(), o.access.as_str()), ("Open Desk", "desk", "public", "open"));
        let o = edit_of(
            &s,
            "lab",
            Some("Research".into()),
            None,
            Some("notes".into()),
            Some("public".into()),
            Some("open".into()),
            Some(true),
            Some(false),
            Some(true),
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(o.name, "Research");
        assert_eq!(o.description, "notes");
        assert!(o.hosted && o.gated && !o.member_auth);
        assert_eq!((o.visibility.as_str(), o.access.as_str()), ("public", "open"));
        let r = create_role_of(&s, "lab", "editor", 40, &["read".into(), "records.write".into()]).unwrap();
        assert_eq!((r.name.as_str(), r.rank), ("editor", 40));
        assert!(r.perms.contains(&"read".into()));
        let r = edit_role_of(&s, "lab", "editor", None, None, Some(vec!["read".into()])).unwrap();
        assert_eq!(r.perms, ["read"]);
        let o = edit_of(&s, "lab", None, None, None, None, None, None, None, None, None, Some("editor".into()), None).unwrap();
        assert_eq!(o.apply_role, "editor");
        edit_of(&s, "lab", None, None, None, None, None, None, None, None, None, Some("user".into()), None).unwrap();
        assert!(remove_role_of(&s, "lab", "editor").unwrap().contains("editor"));
        assert!(remove_of(&s, "desk").unwrap().contains("Open Desk"));
        assert_eq!(org_overview_of(&s).unwrap().mine.len(), 1);
    }

    #[test]
    fn resource_publish_archive_release() {
        let s = Store::open_in_memory().unwrap();
        let o = create_of(&s, "Lab", None, "", false, false).unwrap();
        let (r, _) = s.add_resource(&o.id, ShareKind::Journal, "jid", "Field notes", None).unwrap();
        assert_eq!(r.published.as_str(), "no");
        assert!(resource_publish_of(&s, "lab", "Field notes").unwrap().contains("published members"));
        assert!(resource_publish_of(&s, "lab", "Field notes").unwrap().contains("published public"));
        assert!(resource_archive_of(&s, "lab", "Field notes").unwrap().contains("archived"));
        assert!(resource_archive_of(&s, "lab", "Field notes").unwrap().contains("active"));
        assert!(resource_release_of(&s, "lab", "Field notes").unwrap().contains("no longer"));
        assert!(org_overview_of(&s).unwrap().mine[0].resources.is_empty());
    }

    #[test]
    fn writes_need_unlock() {
        lock();
        assert!(org_create("Lab".into(), None, "".into(), false, false).is_err());
        assert!(org_overview().is_err());
    }

    #[test]
    fn host_status_idle_and_stop_asks_the_other_process() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let st = site_status(home);
        assert!(!st.running && !st.ours && !st.autostart);
        assert!(org_host_stop(Some(home.to_string_lossy().into())).unwrap().contains("not being served"));
        // a status file from another process: stop asks it
        let fake = muninn_core::org::host::HostFile {
            pid: 1,
            url: "https://127.0.0.1:8443/".into(),
            addr: "127.0.0.1:8443".into(),
            http_addr: None,
            cert: muninn_core::org::host::CertInfo {
                source: "self-signed".into(),
                names: vec![],
                not_before: 0,
                not_after: 0,
                issuer: String::new(),
            },
            org_certs: vec![],
            orgs: vec!["lab".into()],
            mirrors: vec![],
            updated_at: 1_704_067_200_000,
        };
        host::write_status(home, &fake).unwrap();
        // running() requires a fresh file (updated in the last 15s)
        let mut fresh = fake.clone();
        fresh.updated_at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
        host::write_status(home, &fresh).unwrap();
        let msg = org_host_stop(Some(home.to_string_lossy().into())).unwrap();
        assert!(msg.contains("asked"), "{msg}");
        assert!(host::stop_requested(home));
    }

    #[test]
    fn templates_domains_endpoints_and_certs_without_running() {
        let s = Store::open_in_memory().unwrap();
        let o = create_of(&s, "Lab", None, "", false, false).unwrap();
        let t = template_save_of(&s, "lab", "home", "<h1>{{title}}</h1>", "body{color:red}", true).unwrap();
        assert_eq!(t.name, "home");
        assert!(t.in_use);
        assert!(t.html.contains("{{title}}"));
        assert_eq!(templates_of(&s, "lab").unwrap()[0].name, "home");
        let shown = template_show_of(&s, "lab", "home").unwrap();
        assert_eq!(shown.css, "body{color:red}");
        let dir = tempfile::tempdir().unwrap();
        let asset = dir.path().join("logo.txt");
        std::fs::write(&asset, b"hi").unwrap();
        assert!(template_add_asset_of(&s, "lab", "home", &asset.to_string_lossy(), None).unwrap().contains("logo.txt"));
        assert!(template_show_of(&s, "lab", "home").unwrap().asset_names.iter().any(|n| n == "logo.txt"));
        assert!(template_remove_asset_of(&s, "lab", "home", "logo.txt").unwrap().contains("removed"));
        assert!(template_remove_of(&s, "lab", "home").unwrap().contains("home"));

        assert!(domain_add_of(&s, "lab", "lab.example.org").unwrap().contains("lab.example.org"));
        assert_eq!(domains_of(&s, Some("lab")).unwrap()[0].domain, "lab.example.org");
        assert!(domain_remove_of(&s, "lab", "lab.example.org").unwrap().contains("no longer"));

        let (m, _) = s.create_model("signals", "", Default::default(), false).unwrap();
        assert!(endpoint_add_of(&s, "lab", "query", "records", &m.name, None, false).unwrap().contains("query"));
        assert!(endpoint_remove_of(&s, "lab", "query").unwrap().contains("query"));

        let p = s.create_project("Field", "").unwrap();
        assert!(project_add_of(&s, "lab", &p.name).unwrap().contains("Field"));
        assert!(project_remove_of(&s, "lab", &p.name).unwrap().contains("no longer"));

        let pem = muninn_core::org::host::self_signed(&["lab.example.org".into()], 10).unwrap();
        let cert = dir.path().join("cert.pem");
        let key = dir.path().join("key.pem");
        std::fs::write(&cert, &pem.chain).unwrap();
        std::fs::write(&key, &pem.key).unwrap();
        domain_add_of(&s, "lab", "lab.example.org").unwrap();
        let msg = cert_set_of(&s, "lab", &cert.to_string_lossy(), &key.to_string_lossy()).unwrap();
        assert!(msg.contains("your certificate"), "{msg}");
        assert!(!msg.contains("BEGIN"), "{msg}");
        assert!(!msg.contains(&pem.key), "private key must not appear in the message");
        let st = tls::status(&s, &s.find_org("lab").unwrap()).unwrap();
        assert_eq!(st.source, "user");
        assert!(cert_clear_of(&s, "lab").unwrap().contains("self-signed"));
        assert!(cert_acme_org_of(&s, "lab", Some("ops@example.org".into()), false).unwrap().contains("ACME on"));
        let _ = o;
    }

    #[test]
    fn rest_writes_need_unlock() {
        lock();
        assert!(org_templates("lab".into()).is_err());
        assert!(org_template_save("lab".into(), "home".into(), "".into(), "".into(), false).is_err());
        assert!(org_domain_add("lab".into(), "lab.example.org".into()).is_err());
        assert!(org_cert_status(None, None).is_err());
        assert!(org_endpoint_add("lab".into(), "q".into(), "records".into(), "signals".into(), None, false).is_err());
        assert!(org_browse(None, "12D3KooWpeer".into(), None).is_err());
        assert!(org_sign_in("desk".into(), "https://example.org".into()).is_err());
    }

    #[test]
    fn overview_needs_unlock() {
        lock();
        assert!(org_overview().is_err());
    }
}
