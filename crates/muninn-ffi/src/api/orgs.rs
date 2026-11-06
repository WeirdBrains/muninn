//! Organizations (`mnnn org` / TUI Orgs): list and writes.
//! Create, edit, remove, roles, members, resources, join/apply/leave and hosting.
//! The store opens read-only for the overview and for writes via [`open_write`].
//! Invite tokens and host certificates stay in Rust; Dart sees public status only.
use super::data::{open, open_write};
use super::info::paths;
use muninn_core::net::{self, Node, client, identity, local};
use muninn_core::org::host::{self, Host, HostOptions};
use muninn_core::org::net as onet;
use muninn_core::org::{self, Actor, Leftovers};
use muninn_core::prefs::Preferences;
use muninn_core::store::net::short_peer;
use muninn_core::store::{Access, NewAction, Org, OrgChange, Published, ResourceState, ShareKind, Store, Visibility};
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
        .filter(|x| x.scope.get("kind").and_then(|v| v.as_str()) == Some("organization") && x.scope.get("as").and_then(|v| v.as_str()) == Some("invite"))
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
    edit_of(
        &open_write()?,
        &org,
        name,
        slug,
        description,
        visibility,
        access,
        hosted,
        member_auth,
        gated,
        template,
        apply_role,
        invite_role,
    )
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
    fn overview_needs_unlock() {
        lock();
        assert!(org_overview().is_err());
    }
}
