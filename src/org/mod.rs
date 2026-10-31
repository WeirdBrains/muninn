//! Organizations (2.7): a publishing house, a gatekeeper and a source of membership.
//!
//! An org lives on the node that made it (its owner). It has roles (owner, admin, user and
//! viewer to start with, plus any you add), members who joined through an authenticated key
//! exchange with the org's node, resources it owns, projects with their own members,
//! templates for its web site and endpoints that run rules, code spaces or record requests.
//!
//! Everything here checks permissions through an [`Actor`]: the person at this node is the
//! owner; a member reaching the node over the network or the web acts with their role; a
//! guest (no identity) only reaches what is public, or what an open org lets anyone do.
//! Every change is recorded in the action history. See `docs/organizations.md`.
pub mod exec;
pub mod host;
pub mod net;
pub mod reflect;
pub mod serve;

use crate::error::{Error, Result};
use crate::store::{
    Access, Endpoint, EndpointKind, Member, MemberStatus, NewAction, NewRole, Org, OrgChange, Published, Resource, ResourceState, Role, ShareKind,
    Store, Template, Visibility,
};
use serde::Serialize;

/// Every permission a role can hold, with what it allows.
pub const PERMS: &[(&str, &str)] = &[
    ("org.edit", "change the org's name, description, visibility, access and hosting"),
    ("roles.manage", "create, edit and remove roles ranked below your own"),
    ("members.list", "see who the members are"),
    ("members.approve", "approve or reject applications"),
    ("members.invite", "invite nodes to join"),
    ("members.kick", "remove members ranked below you"),
    ("members.role", "change the roles of members ranked below you"),
    ("members.defaults", "choose the roles new members get"),
    ("publish", "publish and unpublish resources; manage templates"),
    ("host", "serve the org's site and mirror it on your node"),
    ("projects.manage", "add the org's projects and manage their members"),
    ("resources.transfer", "give your own resources to the org"),
    ("resources.manage", "archive, publish or remove the org's resources"),
    ("read", "see and fetch the org's resources"),
    ("records.read", "query records through endpoints"),
    ("records.write", "create and change records through endpoints"),
    ("call", "call the org's endpoints (rules and code spaces)"),
    ("code.view", "see the source of the org's code spaces"),
    ("code.run", "run the org's code spaces (through endpoints and hosting)"),
    ("code.edit", "change the source of the org's code spaces"),
    ("code.manage", "approve, serve and remove the org's code spaces"),
];

/// A permission and the weaker ones it brings: each code level includes the ones below it,
/// and roles made before code permissions existed keep what they could do (`read` sees
/// sources, `call` runs them, `resources.manage` manages them).
const IMPLIES: &[(&str, &[&str])] = &[
    ("code.manage", &["code.edit", "code.run", "code.view"]),
    ("code.edit", &["code.run", "code.view"]),
    ("code.run", &["code.view"]),
    ("resources.manage", &["code.manage", "code.edit", "code.run", "code.view"]),
    ("call", &["code.run", "code.view"]),
    ("read", &["code.view"]),
];

/// Permissions guests never get, even in open orgs.
const MANAGING: &[&str] = &[
    "org.edit",
    "roles.manage",
    "members.approve",
    "members.invite",
    "members.kick",
    "members.role",
    "members.defaults",
    "publish",
    "host",
    "projects.manage",
    "resources.manage",
    "code.edit",
    "code.manage",
];

pub const OWNER: &str = "owner";
pub const ADMIN: &str = "admin";
pub const USER: &str = "user";
pub const VIEWER: &str = "viewer";

/// The built-in roles: (name, rank, permissions).
pub fn default_roles() -> Vec<(&'static str, i64, Vec<String>)> {
    let all: Vec<String> = PERMS.iter().map(|(p, _)| p.to_string()).collect();
    let s = |v: &[&str]| v.iter().map(|p| p.to_string()).collect::<Vec<_>>();
    vec![
        (OWNER, 100, vec!["*".into()]),
        (ADMIN, 80, all),
        (USER, 50, s(&["members.list", "read", "records.read", "records.write", "call", "resources.transfer", "code.view", "code.run"])),
        (VIEWER, 10, s(&["members.list", "read", "records.read", "code.view"])),
    ]
}

/// Check a permission name (`*` is everything).
pub fn check_perm(p: &str) -> Result<String> {
    let p = p.trim().to_ascii_lowercase();
    if p == "*" || PERMS.iter().any(|(n, _)| *n == p) {
        Ok(p)
    } else {
        Err(Error::invalid("permission", format!("`{p}`: one of {}", PERMS.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", "))))
    }
}

pub fn role_has(r: &Role, perm: &str) -> bool {
    r.perms.iter().any(|p| p == "*" || p == perm || IMPLIES.iter().any(|(k, v)| k == p && v.contains(&perm)))
}

/// Who is acting.
#[derive(Debug, Clone)]
pub enum Actor {
    /// The person at the org's own node (its owner).
    Owner,
    /// A member, with the role they act under.
    Member { member: Box<Member>, role: Box<Role> },
    /// No identity: a web visitor or an unsigned request.
    Guest,
}

impl Actor {
    /// Who, for the history.
    pub fn who(&self) -> String {
        match self {
            Actor::Owner => "owner".into(),
            Actor::Member { member, role } => format!("{} ({})", member.title(), role.name),
            Actor::Guest => "guest".into(),
        }
    }
    pub fn member_id(&self) -> Option<&str> {
        match self {
            Actor::Member { member, .. } => Some(&member.id),
            _ => None,
        }
    }
    pub fn rank(&self) -> i64 {
        match self {
            Actor::Owner => i64::MAX,
            Actor::Member { role, .. } => role.rank,
            Actor::Guest => i64::MIN,
        }
    }
    pub fn is_member(&self) -> bool {
        !matches!(self, Actor::Guest)
    }
}

/// What an actor may do in an org.
pub fn can(s: &Store, org: &Org, a: &Actor, perm: &str) -> bool {
    match a {
        Actor::Owner => true,
        Actor::Member { member, role } => member.status == MemberStatus::Member && role_has(role, perm),
        Actor::Guest => {
            if org.access != Access::Open || MANAGING.contains(&perm) {
                return false;
            }
            guest_role(s, org).is_some_and(|r| role_has(&r, perm))
        }
    }
}

/// The role guests act under in an open org: the role approved applicants get.
pub fn guest_role(s: &Store, org: &Org) -> Option<Role> {
    org.apply_role.as_deref().and_then(|id| s.role(id).ok().flatten())
}

pub fn require(s: &Store, org: &Org, a: &Actor, perm: &str) -> Result<()> {
    if can(s, org, a, perm) { Ok(()) } else { Err(Error::invalid("permission", format!("{} may not {perm} in {}", a.who(), org.name))) }
}

/// The actor for a member of an org (by membership).
pub fn actor_for(s: &Store, m: &Member) -> Result<Actor> {
    let role = m.role_id.as_deref().and_then(|r| s.role(r).ok().flatten()).ok_or_else(|| Error::invalid("member", "has no role"))?;
    Ok(Actor::Member { member: Box::new(m.clone()), role: Box::new(role) })
}

fn record(s: &Store, origin: &str, kind: &str, summary: String, params: serde_json::Value) {
    s.note_action(&NewAction::new(kind, origin, summary).params(params));
}

/// This node's peer id (made on first use).
pub fn my_peer(s: &Store) -> Result<String> {
    Ok(crate::net::identity::load_or_create(s)?.public().to_peer_id().to_string())
}

// ---------- orgs ----------

/// Create an org owned by this node, with the built-in roles and the owner's membership.
pub fn create(s: &Store, origin: &str, name: &str, slug: Option<&str>, description: &str) -> Result<Org> {
    let me = my_peer(s)?;
    let o = s.insert_org(name, slug, description, &me)?;
    let mut user = None;
    let mut owner = None;
    for (n, rank, perms) in default_roles() {
        let r = s.create_role(&o.id, &NewRole { name: n.into(), rank, perms, builtin: true })?;
        match n {
            USER => user = Some(r.id),
            OWNER => owner = Some(r.id),
            _ => {}
        }
    }
    let o = s.update_org(&o.id, &OrgChange { apply_role: user.clone(), invite_role: user, ..Default::default() })?;
    let label = crate::net::local::my_label(s, &crate::prefs::NetPrefs::default());
    s.upsert_member(&o.id, &me, &label, owner.as_deref(), MemberStatus::Member, None, "", Some("owner"))?;
    s.upsert_org_node(&o.id, &me, &label, None)?;
    record(s, origin, "org.create", format!("organization {} created", o.name), serde_json::json!({ "org": o.id, "name": o.name, "slug": o.slug }));
    Ok(o)
}

pub fn edit(s: &Store, origin: &str, a: &Actor, org: &Org, c: &OrgChange) -> Result<Org> {
    require(s, org, a, "org.edit")?;
    if c.apply_role.is_some() || c.invite_role.is_some() {
        require(s, org, a, "members.defaults")?;
        for r in [&c.apply_role, &c.invite_role].into_iter().flatten() {
            let role = s.role(r)?.filter(|x| x.org_id == org.id).ok_or_else(|| Error::invalid("role", "not a role of this org"))?;
            check_grant(org, a, &role)?;
        }
    }
    let o = s.update_org(&org.id, c)?;
    record(s, origin, "org.edit", format!("organization {} changed by {}", o.name, a.who()), serde_json::json!({ "org": o.id, "by": a.who() }));
    Ok(o)
}

/// Delete an org (owner only). Its projects and resources stay on this node.
pub fn remove(s: &Store, origin: &str, a: &Actor, org: &Org) -> Result<Org> {
    if !matches!(a, Actor::Owner) {
        return Err(Error::invalid("permission", "only the owner deletes an organization"));
    }
    let o = s.delete_org(&org.id)?;
    record(s, origin, "org.remove", format!("organization {} deleted", o.name), serde_json::json!({ "org": o.id, "name": o.name }));
    Ok(o)
}

// ---------- roles ----------

pub(crate) fn check_grant(org: &Org, a: &Actor, role: &Role) -> Result<()> {
    if role.name == OWNER && role.builtin && !matches!(a, Actor::Owner) {
        return Err(Error::invalid("role", format!("only the owner of {} gives the owner role", org.name)));
    }
    if !matches!(a, Actor::Owner) && role.rank >= a.rank() {
        return Err(Error::invalid("role", format!("{} ranks as high as or above {}", role.name, a.who())));
    }
    Ok(())
}

pub fn create_role(s: &Store, origin: &str, a: &Actor, org: &Org, name: &str, rank: i64, perms: &[String]) -> Result<Role> {
    require(s, org, a, "roles.manage")?;
    let perms = perms.iter().map(|p| check_perm(p)).collect::<Result<Vec<_>>>()?;
    if !matches!(a, Actor::Owner) && rank >= a.rank() {
        return Err(Error::invalid("role", "a new role must rank below your own"));
    }
    let r = s.create_role(&org.id, &NewRole { name: name.into(), rank, perms, builtin: false })?;
    record(
        s,
        origin,
        "org.role.create",
        format!("role {} added to {}", r.name, org.name),
        serde_json::json!({ "org": org.id, "role": r.id, "name": r.name, "rank": r.rank, "perms": r.perms }),
    );
    Ok(r)
}

#[allow(clippy::too_many_arguments)]
pub fn edit_role(
    s: &Store,
    origin: &str,
    a: &Actor,
    org: &Org,
    role: &Role,
    name: Option<&str>,
    rank: Option<i64>,
    perms: Option<&[String]>,
) -> Result<Role> {
    require(s, org, a, "roles.manage")?;
    check_grant(org, a, role)?;
    if role.builtin && role.name == OWNER && (perms.is_some() || rank.is_some() || name.is_some()) {
        return Err(Error::invalid("role", "the owner role always holds everything"));
    }
    if let Some(k) = rank
        && !matches!(a, Actor::Owner)
        && k >= a.rank()
    {
        return Err(Error::invalid("role", "a role must rank below your own"));
    }
    let perms = perms.map(|v| v.iter().map(|p| check_perm(p)).collect::<Result<Vec<_>>>()).transpose()?;
    let r = s.update_role(&role.id, name, rank, perms.as_deref())?;
    record(
        s,
        origin,
        "org.role.edit",
        format!("role {} of {} changed", r.name, org.name),
        serde_json::json!({ "org": org.id, "role": r.id, "rank": r.rank, "perms": r.perms }),
    );
    Ok(r)
}

/// Remove a role; its members move to the lowest remaining role.
pub fn remove_role(s: &Store, origin: &str, a: &Actor, org: &Org, role: &Role) -> Result<Role> {
    require(s, org, a, "roles.manage")?;
    check_grant(org, a, role)?;
    if role.builtin && role.name == OWNER {
        return Err(Error::invalid("role", "the owner role stays"));
    }
    if org.apply_role.as_deref() == Some(&role.id) || org.invite_role.as_deref() == Some(&role.id) {
        return Err(Error::invalid(
            "role",
            format!("{} is what new members get; choose another default first (mnnn org edit --apply-role/--invite-role)", role.name),
        ));
    }
    let rest: Vec<Role> = s.roles(&org.id)?.into_iter().filter(|r| r.id != role.id).collect();
    let fallback = rest.iter().min_by_key(|r| r.rank).ok_or_else(|| Error::invalid("role", "an org needs at least one role"))?;
    let r = s.delete_role(&role.id, &fallback.id)?;
    record(
        s,
        origin,
        "org.role.remove",
        format!("role {} removed from {}; its members are now {}", r.name, org.name, fallback.name),
        serde_json::json!({ "org": org.id, "role": r.id, "name": r.name, "moved_to": fallback.id }),
    );
    Ok(r)
}

// ---------- members ----------

/// Change a member's role.
pub fn set_role(s: &Store, origin: &str, a: &Actor, org: &Org, m: &Member, role: &Role) -> Result<Member> {
    require(s, org, a, "members.role")?;
    over(s, a, m)?;
    check_grant(org, a, role)?;
    let m = s.set_member(&m.id, None, Some(&role.id), a.member_id().or(Some("owner")))?;
    record(
        s,
        origin,
        "org.member.role",
        format!("{} is now {} in {}", m.title(), role.name, org.name),
        serde_json::json!({ "org": org.id, "member": m.id, "role": role.id, "by": a.who() }),
    );
    Ok(m)
}

/// May `a` act on member `m` (rank above it)?
fn over(s: &Store, a: &Actor, m: &Member) -> Result<()> {
    if matches!(a, Actor::Owner) {
        if m.role_id.as_deref().and_then(|r| s.role(r).ok().flatten()).is_some_and(|r| r.builtin && r.name == OWNER) {
            return Err(Error::invalid("member", "that is the owner"));
        }
        return Ok(());
    }
    let their = m.role_id.as_deref().and_then(|r| s.role(r).ok().flatten()).map(|r| r.rank).unwrap_or(i64::MIN);
    if their >= a.rank() {
        return Err(Error::invalid("member", format!("{} ranks as high as or above {}", m.title(), a.who())));
    }
    Ok(())
}

/// What happens to the resources a kicked member gave the org.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Leftovers {
    /// Leave them as they are (the org owns them).
    #[default]
    Keep,
    Archive,
    Publish,
}

impl Leftovers {
    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "keep" | "leave" => Ok(Leftovers::Keep),
            "archive" => Ok(Leftovers::Archive),
            "publish" => Ok(Leftovers::Publish),
            o => Err(Error::invalid("resources", format!("`{o}`: keep, archive or publish"))),
        }
    }
}

/// The member's resources after a kick.
pub fn settle_resources(s: &Store, org: &Org, member: &str, what: Leftovers) -> Result<Vec<Resource>> {
    let mut v = vec![];
    for r in s.resources(&org.id)?.into_iter().filter(|r| r.transferred_by.as_deref() == Some(member)) {
        v.push(match what {
            Leftovers::Keep => r,
            Leftovers::Archive => s.set_resource(&r.id, Some(ResourceState::Archived), None)?,
            Leftovers::Publish => s.set_resource(&r.id, None, Some(Published::Public))?,
        });
    }
    Ok(v)
}

/// Mark a member kicked ([`net::kick`] also ends the key exchange and tells their node).
/// What they gave the org stays the org's: kept, archived or published.
pub fn kick(s: &Store, origin: &str, a: &Actor, org: &Org, m: &Member, leftovers: Leftovers) -> Result<(Member, Vec<Resource>)> {
    require(s, org, a, "members.kick")?;
    over(s, a, m)?;
    let m = s.set_member(&m.id, Some(MemberStatus::Kicked), None, a.member_id().or(Some("owner")))?;
    for pm in s.org_projects(&org.id)? {
        s.remove_project_member(&org.id, &pm.id, &m.id)?;
    }
    let res = settle_resources(s, org, &m.id, leftovers)?;
    record(
        s,
        origin,
        "org.member.kick",
        format!(
            "{} removed from {} ({} resource(s) {})",
            m.title(),
            org.name,
            res.len(),
            match leftovers {
                Leftovers::Keep => "kept",
                Leftovers::Archive => "archived",
                Leftovers::Publish => "published",
            }
        ),
        serde_json::json!({ "org": org.id, "member": m.id, "peer": m.peer_id, "by": a.who(), "resources": leftovers, "count": res.len() }),
    );
    Ok((m, res))
}

// ---------- resources ----------

/// The name of an item of this node, by kind and key (id, prefix or name).
pub fn find_item(s: &Store, kind: ShareKind, key: &str) -> Result<(String, String)> {
    crate::net::client::find_item(s, kind, key)
}

/// Give one of this node's things to the org (`by`: the member who gave it, else this node).
pub fn transfer(s: &Store, origin: &str, a: &Actor, org: &Org, kind: ShareKind, key: &str, by: Option<&str>) -> Result<(Resource, bool)> {
    require(s, org, a, "resources.transfer")?;
    let (id, name) = find_item(s, kind, key)?;
    let (r, new) = s.add_resource(&org.id, kind, &id, &name, by.or(a.member_id()))?;
    if new {
        record(
            s,
            origin,
            "org.resource.add",
            format!("{} {name} given to {}", kind.label(), org.name),
            serde_json::json!({ "org": org.id, "resource": r.id, "kind": kind, "item": id, "by": a.who() }),
        );
    }
    Ok((r, new))
}

pub fn set_published(s: &Store, origin: &str, a: &Actor, org: &Org, r: &Resource, p: Published) -> Result<Resource> {
    require(s, org, a, "publish")?;
    let r = s.set_resource(&r.id, None, Some(p))?;
    record(
        s,
        origin,
        if p == Published::No { "org.unpublish" } else { "org.publish" },
        format!(
            "{} {} {} in {}",
            r.kind.label(),
            r.name,
            if p == Published::No { "unpublished".to_string() } else { format!("published to {}", p.as_str()) },
            org.name
        ),
        serde_json::json!({ "org": org.id, "resource": r.id, "published": p, "by": a.who() }),
    );
    Ok(r)
}

pub fn set_state(s: &Store, origin: &str, a: &Actor, org: &Org, r: &Resource, st: ResourceState) -> Result<Resource> {
    require(s, org, a, "resources.manage")?;
    let r = s.set_resource(&r.id, Some(st), None)?;
    let word = if st == ResourceState::Archived { "archived" } else { "restored" };
    record(
        s,
        origin,
        "org.resource.state",
        format!("{} {} {word} in {}", r.kind.label(), r.name, org.name),
        serde_json::json!({ "org": org.id, "resource": r.id, "state": word }),
    );
    Ok(r)
}

/// Take a resource out of the org (the thing itself stays on this node).
pub fn release(s: &Store, origin: &str, a: &Actor, org: &Org, r: &Resource) -> Result<Resource> {
    require(s, org, a, "resources.manage")?;
    let r = s.remove_resource(&r.id)?;
    record(
        s,
        origin,
        "org.resource.remove",
        format!("{} {} taken out of {}", r.kind.label(), r.name, org.name),
        serde_json::json!({ "org": org.id, "resource": r.id }),
    );
    Ok(r)
}

/// Who may see a resource.
pub fn visible(s: &Store, org: &Org, a: &Actor, r: &Resource) -> bool {
    if r.state == ResourceState::Archived && !can(s, org, a, "resources.manage") {
        return false;
    }
    match a {
        Actor::Guest => match r.published {
            Published::No => false,
            Published::Public => org.visibility == Visibility::Public || org.access == Access::Open,
            Published::Members => org.access == Access::Open,
        },
        _ if r.kind == ShareKind::Code => can(s, org, a, "read") && can(s, org, a, "code.view"),
        _ => can(s, org, a, "read"),
    }
}

// ---------- projects ----------

pub fn add_project(s: &Store, origin: &str, a: &Actor, org: &Org, project: &str) -> Result<crate::store::Project> {
    require(s, org, a, "projects.manage")?;
    let p = s.find_project(project)?;
    if s.add_org_project(&org.id, &p.id)? {
        s.add_resource(&org.id, ShareKind::Project, &p.id, &p.name, a.member_id())?;
        record(
            s,
            origin,
            "org.project.add",
            format!("project {} added to {}", p.name, org.name),
            serde_json::json!({ "org": org.id, "project": p.id }),
        );
    }
    Ok(p)
}

pub fn remove_project(s: &Store, origin: &str, a: &Actor, org: &Org, project: &str) -> Result<crate::store::Project> {
    require(s, org, a, "projects.manage")?;
    let p = s.find_project(project)?;
    if s.remove_org_project(&org.id, &p.id)? {
        record(
            s,
            origin,
            "org.project.remove",
            format!("project {} taken out of {}", p.name, org.name),
            serde_json::json!({ "org": org.id, "project": p.id }),
        );
    }
    Ok(p)
}

/// Add a member to an org project (with a project role, else their org role).
pub fn add_project_member(s: &Store, origin: &str, a: &Actor, org: &Org, project: &str, m: &Member, role: Option<&Role>) -> Result<()> {
    require(s, org, a, "projects.manage")?;
    let p = s.org_projects(&org.id)?.into_iter().find(|p| p.id == project || p.name.eq_ignore_ascii_case(project) || p.id.starts_with(project));
    let p = p.ok_or_else(|| Error::invalid("project", format!("`{project}` is not a project of {}", org.name)))?;
    if m.status != MemberStatus::Member {
        return Err(Error::invalid("member", format!("{} is {}, not a member", m.title(), m.status.as_str())));
    }
    if let Some(r) = role {
        check_grant(org, a, r)?;
    }
    s.set_project_member(&org.id, &p.id, &m.id, role.map(|r| r.id.as_str()))?;
    record(
        s,
        origin,
        "org.project.member",
        format!("{} joined project {} of {}{}", m.title(), p.name, org.name, role.map(|r| format!(" as {}", r.name)).unwrap_or_default()),
        serde_json::json!({ "org": org.id, "project": p.id, "member": m.id, "role": role.map(|r| r.id.clone()) }),
    );
    Ok(())
}

/// The role a member acts under in a project: the project role, else the org role. Org
/// roles rank above project roles: a project role never outranks the member's org role.
pub fn project_role(s: &Store, org: &Org, project: &str, m: &Member) -> Result<Option<Role>> {
    let org_role = m.role_id.as_deref().and_then(|r| s.role(r).ok().flatten());
    let Some(pm) = s.project_members(&org.id, project)?.into_iter().find(|x| x.member_id == m.id) else { return Ok(None) };
    let pr = pm.role_id.as_deref().and_then(|r| s.role(r).ok().flatten());
    Ok(match (pr, org_role) {
        (Some(p), Some(o)) if p.rank > o.rank => Some(o),
        (Some(p), _) => Some(p),
        (None, o) => o,
    })
}

// ---------- endpoints ----------

/// Expose a rule, code space or model's records as an endpoint.
#[allow(clippy::too_many_arguments)]
pub fn add_endpoint(
    s: &Store,
    origin: &str,
    a: &Actor,
    org: &Org,
    name: &str,
    kind: EndpointKind,
    target: &str,
    perm: Option<&str>,
    anyone: bool,
) -> Result<Endpoint> {
    require(s, org, a, "publish")?;
    let name = name.trim();
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(Error::invalid("endpoint", "a name of letters, digits, - and _ (it goes in URLs)"));
    }
    let ref_id = match kind {
        EndpointKind::Rule => s.find_rule(target)?.id,
        EndpointKind::Code => s.find_code_space(target)?.id,
        EndpointKind::Records => s.find_model(target)?.id,
    };
    let perm = check_perm(perm.unwrap_or(match kind {
        EndpointKind::Records => "records.read",
        _ => "call",
    }))?;
    let e = s.save_endpoint(&org.id, name, kind, &ref_id, &perm, anyone)?;
    if kind == EndpointKind::Code {
        // exposing code here means this node runs it
        s.serve_code(&org.id, &ref_id)?;
    }
    record(
        s,
        origin,
        "org.endpoint.add",
        format!("endpoint {} ({}) in {}", e.name, kind.as_str(), org.name),
        serde_json::json!({ "org": org.id, "endpoint": e.id, "kind": kind.as_str(), "ref": ref_id, "perm": perm, "anyone": anyone }),
    );
    Ok(e)
}

pub fn remove_endpoint(s: &Store, origin: &str, a: &Actor, org: &Org, name: &str) -> Result<Endpoint> {
    require(s, org, a, "publish")?;
    let e = s.find_endpoint(&org.id, name)?;
    s.delete_endpoint(&e.id)?;
    record(
        s,
        origin,
        "org.endpoint.remove",
        format!("endpoint {} removed from {}", e.name, org.name),
        serde_json::json!({ "org": org.id, "endpoint": e.id }),
    );
    Ok(e)
}

fn may_call(s: &Store, org: &Org, a: &Actor, e: &Endpoint, perm: &str) -> Result<()> {
    if matches!(a, Actor::Guest) && e.anyone && (org.visibility == Visibility::Public || org.access == Access::Open) {
        return Ok(());
    }
    require(s, org, a, &e.perm)?;
    if perm != e.perm { require(s, org, a, perm) } else { Ok(()) }
}

/// Call an endpoint. Records endpoints take `{"op": "query", "where": [...], "text", "sort",
/// "limit", "offset"}`, `{"op": "get", "id"}`, `{"op": "create", "values": {...}}`,
/// `{"op": "update", "id", "values"}` or `{"op": "delete", "id"}`; rule endpoints run the
/// rule's then-actions on an `org.call` action carrying the input; code endpoints run the
/// (approved) code space.
pub fn call(s: &Store, origin: &str, a: &Actor, org: &Org, endpoint: &str, input: &serde_json::Value) -> Result<serde_json::Value> {
    let e = s.find_endpoint(&org.id, endpoint)?;
    let op = input.get("op").and_then(|v| v.as_str()).unwrap_or("query").to_ascii_lowercase();
    let out = match e.kind {
        EndpointKind::Records => {
            let m = s.model(&e.ref_id)?.ok_or_else(|| Error::invalid("endpoint", "its model is gone"))?;
            let values = || -> Result<serde_json::Map<String, serde_json::Value>> {
                input.get("values").and_then(|v| v.as_object()).cloned().ok_or_else(|| Error::invalid("input", "give `values` as an object"))
            };
            let id = || input.get("id").and_then(|v| v.as_str()).map(str::to_string).ok_or_else(|| Error::invalid("input", "give the record `id`"));
            match op.as_str() {
                "query" | "list" => {
                    may_call(s, org, a, &e, "records.read")?;
                    let mut q: crate::store::RecordQuery =
                        serde_json::from_value(input.clone()).map_err(|x| Error::invalid("query", x.to_string()))?;
                    q.limit = Some(q.limit.unwrap_or(500).min(5000));
                    serde_json::json!({ "records": s.records(&m, &q)? })
                }
                "get" => {
                    may_call(s, org, a, &e, "records.read")?;
                    serde_json::json!({ "record": s.find_record(&m, &id()?)? })
                }
                "create" => {
                    may_call(s, org, a, &e, "records.write")?;
                    let r = s.create_record(&m, &values()?)?;
                    record(
                        s,
                        origin,
                        "record.create",
                        format!("record {} created in {} through {} by {}", r.short_id(), m.name, org.name, a.who()),
                        serde_json::json!({ "model": m.id, "record": r.id, "org": org.id, "endpoint": e.id, "by": a.who() }),
                    );
                    serde_json::json!({ "record": r })
                }
                "update" => {
                    may_call(s, org, a, &e, "records.write")?;
                    let r = s.find_record(&m, &id()?)?;
                    let r = s.update_record(&m, &r.id, &values()?)?;
                    record(
                        s,
                        origin,
                        "record.update",
                        format!("record {} changed in {} through {} by {}", r.short_id(), m.name, org.name, a.who()),
                        serde_json::json!({ "model": m.id, "record": r.id, "org": org.id, "endpoint": e.id, "by": a.who() }),
                    );
                    serde_json::json!({ "record": r })
                }
                "delete" => {
                    may_call(s, org, a, &e, "records.write")?;
                    let r = s.find_record(&m, &id()?)?;
                    let r = s.delete_record(&m, &r.id)?;
                    record(
                        s,
                        origin,
                        "record.delete",
                        format!("record {} deleted from {} through {} by {}", r.short_id(), m.name, org.name, a.who()),
                        serde_json::json!({ "model": m.id, "record": r.id, "org": org.id, "endpoint": e.id, "by": a.who() }),
                    );
                    serde_json::json!({ "deleted": r.id })
                }
                o => return Err(Error::invalid("input", format!("op `{o}`: query, get, create, update or delete"))),
            }
        }
        EndpointKind::Rule => {
            may_call(s, org, a, &e, "call")?;
            let rule = s.rule(&e.ref_id)?.ok_or_else(|| Error::invalid("endpoint", "its rule is gone"))?;
            let id = s.record_action(
                &NewAction::new("org.call", origin, format!("{} called {} in {}", a.who(), e.name, org.name))
                    .params(serde_json::json!({ "org": org.id, "endpoint": e.id, "name": e.name, "by": a.who(), "input": input })),
            )?;
            let act = s.action(id)?.ok_or_else(|| Error::Other("the call's action vanished".into()))?;
            let r = crate::rules::run(s, &rule, &act);
            serde_json::json!({ "status": r.status, "messages": r.messages, "action": r.action })
        }
        EndpointKind::Code => {
            may_call(s, org, a, &e, "code.run")?;
            let space = s.code_space(&e.ref_id)?.ok_or_else(|| Error::invalid("endpoint", "its code space is gone"))?;
            if !s.serves_code(&org.id, &space.id)? {
                return Err(Error::invalid("endpoint", format!("this node does not serve code space {} for {}", space.name, org.name)));
            }
            if !crate::code::approved(&space) {
                return Err(Error::invalid(
                    "endpoint",
                    format!("code space {} changed since it was approved; approve it on this node first", space.name),
                ));
            }
            let env = crate::code::CodeEnv::for_store(s)?;
            let info = crate::code::run(s, &env, origin, &space, &mut |_| {})?;
            record(
                s,
                origin,
                "org.call",
                format!("{} ran {} in {}", a.who(), e.name, org.name),
                serde_json::json!({ "org": org.id, "endpoint": e.id, "by": a.who() }),
            );
            crate::code::run_json(&info)
        }
    };
    Ok(out)
}

// ---------- code this node serves ----------

/// The code spaces this node runs for an org (its own per-node choice).
pub fn served_code(s: &Store, org: &Org) -> Result<Vec<crate::store::CodeSpace>> {
    Ok(s.served_code(&org.id)?.into_iter().filter_map(|id| s.code_space(&id).ok().flatten()).collect())
}

/// May this node, a member of a joined org, do something there (by the role it was given)?
pub fn joined_can(j: &crate::store::Joined, perm: &str) -> bool {
    j.status == MemberStatus::Member && j.perms.iter().any(|p| p == "*" || p == perm || IMPLIES.iter().any(|(k, v)| k == p && v.contains(&perm)))
}

/// Serve (or stop serving) a code space for a joined org on this node (needs `code.manage`
/// in the role this node holds there).
pub fn serve_joined_code(s: &Store, origin: &str, j: &crate::store::Joined, code: &str, on: bool) -> Result<crate::store::CodeSpace> {
    if !joined_can(j, "code.manage") {
        return Err(Error::invalid("permission", format!("your role in {} may not code.manage", j.name)));
    }
    let c = s.find_code_space(code)?;
    let changed = if on { s.serve_code(&j.org_id, &c.id)? } else { s.unserve_code(&j.org_id, &c.id)? };
    if !on && !changed {
        return Err(Error::invalid("code", format!("{} is not served for {}", c.name, j.name)));
    }
    let verb = if on { "serves" } else { "stopped serving" };
    record(
        s,
        origin,
        if on { "org.code.serve" } else { "org.code.unserve" },
        format!("this node {verb} code space {} for {}", c.name, j.name),
        serde_json::json!({ "org": j.org_id, "code": c.id }),
    );
    Ok(c)
}

/// Run a code space for an org on this node (needs `code.manage`).
pub fn serve_code(s: &Store, origin: &str, a: &Actor, org: &Org, code: &str) -> Result<crate::store::CodeSpace> {
    require(s, org, a, "code.manage")?;
    let c = s.find_code_space(code)?;
    if s.serve_code(&org.id, &c.id)? {
        record(
            s,
            origin,
            "org.code.serve",
            format!("{} serves code space {} for {}", a.who(), c.name, org.name),
            serde_json::json!({ "org": org.id, "code": c.id }),
        );
    }
    Ok(c)
}

/// Stop running a code space for an org on this node (its endpoints then refuse calls).
pub fn unserve_code(s: &Store, origin: &str, a: &Actor, org: &Org, code: &str) -> Result<crate::store::CodeSpace> {
    require(s, org, a, "code.manage")?;
    let c = s.find_code_space(code)?;
    if !s.unserve_code(&org.id, &c.id)? {
        return Err(Error::invalid("code", format!("{} is not served for {}", c.name, org.name)));
    }
    record(
        s,
        origin,
        "org.code.unserve",
        format!("{} stopped serving code space {} for {}", a.who(), c.name, org.name),
        serde_json::json!({ "org": org.id, "code": c.id }),
    );
    Ok(c)
}

// ---------- syncing code to org nodes ----------

/// One of an org's code spaces, as sent to its nodes (approval and runs stripped).
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct OrgCode {
    pub resource: String,
    pub space: crate::store::CodeSpace,
}

/// What syncing did with one code space.
#[derive(Debug, Clone, Serialize)]
pub struct Synced {
    pub name: String,
    pub code_id: String,
    /// new, changed or same.
    pub state: &'static str,
}

/// The org's active code resources.
pub fn code_of(s: &Store, org: &Org) -> Result<Vec<OrgCode>> {
    let mut v = vec![];
    for r in s.resources(&org.id)?.into_iter().filter(|r| r.kind == ShareKind::Code && r.state == ResourceState::Active) {
        if let Some(mut c) = s.code_space(&r.ref_id)? {
            c.approved_hash = None;
            c.last_run = None;
            v.push(OrgCode { resource: r.id, space: c });
        }
    }
    Ok(v)
}

/// Apply an org's code list to this node's copies.
pub fn apply_code(s: &Store, org_id: &str, org_name: &str, list: &[OrgCode]) -> Result<Vec<Synced>> {
    let mut out = vec![];
    for oc in list {
        let c = &oc.space;
        let existing = s.code_copy(org_id, &oc.resource)?.and_then(|id| s.code_space(&id).ok().flatten());
        let (space, state) = match existing {
            Some(mine) if mine.source == c.source && mine.lang == c.lang && mine.input == c.input => (mine, "same"),
            Some(mine) => {
                let ch = crate::store::CodeChange {
                    lang: Some(c.lang),
                    source: Some(c.source.clone()),
                    input: Some(c.input.clone()),
                    description: Some(c.description.clone()),
                    ..Default::default()
                };
                (s.update_code_space(&mine.id, &ch)?, "changed")
            }
            None => {
                let taken: Vec<String> = s.code_spaces()?.into_iter().map(|x| x.name.to_lowercase()).collect();
                let mut name = c.name.clone();
                if taken.contains(&name.to_lowercase()) {
                    name = format!("{} ({org_name})", c.name);
                }
                let mut n = 2;
                while taken.contains(&name.to_lowercase()) {
                    name = format!("{} ({org_name} {n})", c.name);
                    n += 1;
                }
                (s.create_code_space(&name, &c.description, c.lang, &c.source, c.input.clone())?, "new")
            }
        };
        s.save_code_copy(org_id, &oc.resource, &space.id)?;
        out.push(Synced { name: space.name, code_id: space.id, state });
    }
    Ok(out)
}

// ---------- templates ----------

/// Add or replace a template (the look of the org's site).
pub fn save_template(s: &Store, origin: &str, a: &Actor, org: &Org, name: &str, html: &str, css: &str) -> Result<Template> {
    require(s, org, a, "publish")?;
    let (t, new) = s.save_template(&org.id, name, html, css)?;
    record(
        s,
        origin,
        "org.template.save",
        format!("template {} {} in {}", t.name, if new { "added" } else { "changed" }, org.name),
        serde_json::json!({ "org": org.id, "template": t.id }),
    );
    Ok(t)
}

/// Store a file with a template (served at `/o/{slug}/assets/{template}/{name}`).
pub fn put_asset(s: &Store, origin: &str, a: &Actor, org: &Org, t: &Template, name: &str, data: &[u8]) -> Result<String> {
    require(s, org, a, "publish")?;
    let name = name.trim().trim_start_matches('/');
    if name.is_empty() || name.contains("..") || name.contains('\\') {
        return Err(Error::invalid("asset", "a plain file name (folders with /, no ..)"));
    }
    let mime = mime_of(name).to_string();
    s.put_template_asset(&t.id, name, &mime, data)?;
    record(
        s,
        origin,
        "org.template.asset",
        format!("asset {name} stored with template {} of {}", t.name, org.name),
        serde_json::json!({ "org": org.id, "template": t.id, "asset": name, "bytes": data.len() }),
    );
    Ok(mime)
}

pub fn mime_of(name: &str) -> &'static str {
    match name.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str() {
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "html" | "htm" => "text/html; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "txt" | "md" => "text/plain; charset=utf-8",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

// ---------- summaries ----------

/// One org as the CLI, TUI and peers see it.
#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq)]
pub struct OrgCard {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub visibility: Visibility,
    pub access: Access,
    pub owner_peer: String,
    #[serde(default)]
    pub members: usize,
}

pub fn card(s: &Store, o: &Org) -> OrgCard {
    let members = s.members(&o.id).map(|v| v.iter().filter(|m| m.status == MemberStatus::Member).count()).unwrap_or(0);
    OrgCard {
        id: o.id.clone(),
        name: o.name.clone(),
        slug: o.slug.clone(),
        description: o.description.clone(),
        visibility: o.visibility,
        access: o.access,
        owner_peer: o.owner_peer.clone(),
        members,
    }
}

/// The role name of a member (or `-`).
pub fn role_name(s: &Store, m: &Member) -> String {
    m.role_id.as_deref().and_then(|r| s.role(r).ok().flatten()).map(|r| r.name).unwrap_or_else(|| "-".into())
}

#[cfg(test)]
mod tests;
