//! Organizations (2.7): orgs this node runs, their roles, members, resources, projects,
//! templates, exposed endpoints and nodes; and the orgs this node has joined elsewhere.
//! The rules (who may do what) are `crate::org`; see `docs/organizations.md`.
use super::{Store, new_id, now_ms};
use crate::error::{Error, Result};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

/// 2.13: the code spaces this node runs for each org (code endpoints served so far stay served).
pub const MIGRATION_2_13: &str = r#"
CREATE TABLE org_served_code (
    org_id TEXT NOT NULL,
    code_id TEXT NOT NULL,
    added_at INTEGER NOT NULL,
    PRIMARY KEY (org_id, code_id)
);
INSERT OR IGNORE INTO org_served_code(org_id, code_id, added_at)
    SELECT org_id, ref_id, created_at FROM org_endpoints WHERE kind = 'code';
CREATE TABLE org_code_copies (
    org_id TEXT NOT NULL,
    resource_id TEXT NOT NULL,
    code_id TEXT NOT NULL,
    synced_at INTEGER NOT NULL,
    PRIMARY KEY (org_id, resource_id)
);
"#;

pub const MIGRATION: &str = r#"
CREATE TABLE orgs (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    visibility TEXT NOT NULL DEFAULT 'private',
    access TEXT NOT NULL DEFAULT 'members',
    owner_peer TEXT NOT NULL DEFAULT '',
    apply_role TEXT,
    invite_role TEXT,
    hosted INTEGER NOT NULL DEFAULT 0,
    member_auth INTEGER NOT NULL DEFAULT 1,
    template_id TEXT,
    author_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE org_roles (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    rank INTEGER NOT NULL,
    perms TEXT NOT NULL DEFAULT '[]',
    builtin INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    UNIQUE(org_id, name)
);
CREATE TABLE org_members (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    peer_id TEXT NOT NULL,
    label TEXT NOT NULL DEFAULT '',
    role_id TEXT,
    status TEXT NOT NULL,
    exchange_id TEXT,
    note TEXT NOT NULL DEFAULT '',
    decided_by TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE(org_id, peer_id)
);
CREATE INDEX org_members_exchange ON org_members(exchange_id);
CREATE TABLE org_resources (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    ref_id TEXT NOT NULL,
    name TEXT NOT NULL,
    transferred_by TEXT,
    state TEXT NOT NULL DEFAULT 'active',
    published TEXT NOT NULL DEFAULT 'no',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE(org_id, kind, ref_id)
);
CREATE TABLE org_projects (
    org_id TEXT NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL,
    added_at INTEGER NOT NULL,
    PRIMARY KEY (org_id, project_id)
);
CREATE TABLE org_project_members (
    org_id TEXT NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL,
    member_id TEXT NOT NULL REFERENCES org_members(id) ON DELETE CASCADE,
    role_id TEXT,
    added_at INTEGER NOT NULL,
    PRIMARY KEY (org_id, project_id, member_id)
);
CREATE TABLE org_templates (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    html TEXT NOT NULL,
    css TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE(org_id, name)
);
CREATE TABLE org_template_assets (
    template_id TEXT NOT NULL REFERENCES org_templates(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    mime TEXT NOT NULL,
    data BLOB NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (template_id, name)
);
CREATE TABLE org_endpoints (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    ref_id TEXT NOT NULL,
    perm TEXT NOT NULL DEFAULT 'call',
    anyone INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    UNIQUE(org_id, name)
);
CREATE TABLE org_rules (
    org_id TEXT NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    rule_id TEXT NOT NULL,
    added_at INTEGER NOT NULL,
    PRIMARY KEY (org_id, rule_id)
);
CREATE TABLE org_nodes (
    org_id TEXT NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    peer_id TEXT NOT NULL,
    label TEXT NOT NULL DEFAULT '',
    url TEXT NOT NULL DEFAULT '',
    added_at INTEGER NOT NULL,
    seen_at INTEGER,
    PRIMARY KEY (org_id, peer_id)
);
CREATE TABLE org_joined (
    org_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    slug TEXT NOT NULL DEFAULT '',
    node_peer TEXT NOT NULL,
    exchange_id TEXT,
    status TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT '',
    perms TEXT NOT NULL DEFAULT '[]',
    info TEXT NOT NULL DEFAULT '{}',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE org_mirrors (
    org_id TEXT PRIMARY KEY,
    slug TEXT NOT NULL,
    node_peer TEXT NOT NULL,
    snapshot BLOB NOT NULL,
    fetched_at INTEGER NOT NULL
);
"#;

/// Who can see an org's site and listings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Public,
    #[default]
    Private,
}

impl Visibility {
    pub fn as_str(self) -> &'static str {
        match self {
            Visibility::Public => "public",
            Visibility::Private => "private",
        }
    }
    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "public" => Ok(Visibility::Public),
            "private" => Ok(Visibility::Private),
            o => Err(Error::invalid("visibility", format!("`{o}`: public or private"))),
        }
    }
}

/// Whether an org has members (apply, approve) or is open to anyone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Access {
    #[default]
    Members,
    Open,
}

impl Access {
    pub fn as_str(self) -> &'static str {
        match self {
            Access::Members => "members",
            Access::Open => "open",
        }
    }
    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "members" | "member" | "closed" => Ok(Access::Members),
            "open" => Ok(Access::Open),
            o => Err(Error::invalid("access", format!("`{o}`: members or open"))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Org {
    pub id: String,
    pub name: String,
    /// The org's name in web addresses (`/o/SLUG/`).
    pub slug: String,
    pub description: String,
    pub visibility: Visibility,
    pub access: Access,
    /// The node that runs the org (the owner's).
    pub owner_peer: String,
    /// Role ids new members get: approved applicants, and invited nodes.
    pub apply_role: Option<String>,
    pub invite_role: Option<String>,
    /// Served by `mnnn org host start`.
    pub hosted: bool,
    /// Members may sign web requests with their exchange key.
    pub member_auth: bool,
    pub template_id: Option<String>,
    pub author_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Org {
    pub fn short_id(&self) -> &str {
        super::short(&self.id)
    }
}

/// What an org changes to (`None` keeps the value).
#[derive(Debug, Clone, Default)]
pub struct OrgChange {
    pub name: Option<String>,
    pub slug: Option<String>,
    pub description: Option<String>,
    pub visibility: Option<Visibility>,
    pub access: Option<Access>,
    pub apply_role: Option<String>,
    pub invite_role: Option<String>,
    pub hosted: Option<bool>,
    pub member_auth: Option<bool>,
    /// `Some(None)` goes back to the built-in template.
    pub template_id: Option<Option<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Role {
    pub id: String,
    pub org_id: String,
    pub name: String,
    /// Higher outranks lower: members act only on lower ranks and grant only lower roles.
    pub rank: i64,
    pub perms: Vec<String>,
    pub builtin: bool,
    pub created_at: i64,
}

/// Where a membership stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemberStatus {
    /// Applied (its key exchange waits on this node).
    Applied,
    /// Invited (an offer was made; it joins when the invitee answers).
    Invited,
    Member,
    Rejected,
    Kicked,
    Left,
}

impl MemberStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            MemberStatus::Applied => "applied",
            MemberStatus::Invited => "invited",
            MemberStatus::Member => "member",
            MemberStatus::Rejected => "rejected",
            MemberStatus::Kicked => "kicked",
            MemberStatus::Left => "left",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "applied" => MemberStatus::Applied,
            "invited" => MemberStatus::Invited,
            "member" => MemberStatus::Member,
            "rejected" => MemberStatus::Rejected,
            "kicked" => MemberStatus::Kicked,
            _ => MemberStatus::Left,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Member {
    pub id: String,
    pub org_id: String,
    /// The member's node (its key exchange is with this node).
    pub peer_id: String,
    pub label: String,
    pub role_id: Option<String>,
    pub status: MemberStatus,
    pub exchange_id: Option<String>,
    /// What the applicant wrote.
    pub note: String,
    /// `owner` (this node) or the member id that approved, rejected or kicked.
    pub decided_by: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Member {
    pub fn short_id(&self) -> &str {
        super::short(&self.id)
    }
    pub fn title(&self) -> String {
        if self.label.is_empty() { super::net::short_peer(&self.peer_id).to_string() } else { self.label.clone() }
    }
}

/// Is a resource in use or put away.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResourceState {
    Active,
    Archived,
}

impl ResourceState {
    pub fn as_str(self) -> &'static str {
        match self {
            ResourceState::Active => "active",
            ResourceState::Archived => "archived",
        }
    }
}

/// Who a resource is published to on the org's site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Published {
    /// Not on the site.
    No,
    /// Members only (signed requests).
    Members,
    /// Anyone who can see the org (everyone, for public orgs).
    Public,
}

impl Published {
    pub fn as_str(self) -> &'static str {
        match self {
            Published::No => "no",
            Published::Members => "members",
            Published::Public => "public",
        }
    }
    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "no" | "none" | "off" => Ok(Published::No),
            "members" | "member" => Ok(Published::Members),
            "public" | "yes" | "on" => Ok(Published::Public),
            o => Err(Error::invalid("published", format!("`{o}`: public, members or no"))),
        }
    }
}

/// Something the org owns: a model, live search, journal, project, report or code space.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Resource {
    pub id: String,
    pub org_id: String,
    pub kind: super::ShareKind,
    pub ref_id: String,
    pub name: String,
    /// The member who gave it to the org (None: added on this node).
    pub transferred_by: Option<String>,
    pub state: ResourceState,
    pub published: Published,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Resource {
    pub fn short_id(&self) -> &str {
        super::short(&self.id)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectMember {
    pub org_id: String,
    pub project_id: String,
    pub member_id: String,
    /// The member's role in this project (None: their org role).
    pub role_id: Option<String>,
    pub added_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Template {
    pub id: String,
    pub org_id: String,
    pub name: String,
    pub html: String,
    pub css: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateAsset {
    pub template_id: String,
    pub name: String,
    pub mime: String,
    pub size: usize,
    pub created_at: i64,
}

/// What an endpoint runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EndpointKind {
    /// A rule's then-actions, with the request as the action's fields.
    Rule,
    /// A code space (a service plugin on this node) over a model.
    Code,
    /// A model's records: query, and create/update with `records.write`.
    Records,
}

impl EndpointKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EndpointKind::Rule => "rule",
            EndpointKind::Code => "code",
            EndpointKind::Records => "records",
        }
    }
    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "rule" => Ok(EndpointKind::Rule),
            "code" => Ok(EndpointKind::Code),
            "records" | "model" => Ok(EndpointKind::Records),
            o => Err(Error::invalid("endpoint", format!("`{o}`: rule, code or records"))),
        }
    }
}

/// A rule, code space or model the org's hosting answers requests for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Endpoint {
    pub id: String,
    pub org_id: String,
    pub name: String,
    pub kind: EndpointKind,
    pub ref_id: String,
    /// The permission a caller needs.
    pub perm: String,
    /// Callable without signing in (public orgs only).
    pub anyone: bool,
    pub created_at: i64,
}

/// A node that serves an org.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrgNode {
    pub org_id: String,
    pub peer_id: String,
    pub label: String,
    pub url: String,
    pub added_at: i64,
    pub seen_at: Option<i64>,
}

/// An org on another node that this node applied to or belongs to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Joined {
    pub org_id: String,
    pub name: String,
    pub slug: String,
    pub node_peer: String,
    pub exchange_id: Option<String>,
    pub status: MemberStatus,
    pub role: String,
    pub perms: Vec<String>,
    pub info: serde_json::Value,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A public copy of another node's org, served here.
#[derive(Debug, Clone, PartialEq)]
pub struct Mirror {
    pub org_id: String,
    pub slug: String,
    pub node_peer: String,
    pub snapshot: Vec<u8>,
    pub fetched_at: i64,
}

/// `Research Lab` → `research-lab`.
pub fn slugify(s: &str) -> String {
    let mut out = String::new();
    for c in s.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() { "org".into() } else { out }
}

fn check_name(name: &str, what: &'static str) -> Result<String> {
    let n = name.trim();
    if n.is_empty() {
        return Err(Error::invalid(what, "must not be empty"));
    }
    Ok(n.to_string())
}

fn check_slug(s: &str) -> Result<String> {
    let v = s.trim().to_ascii_lowercase();
    if v.is_empty() || v.len() > 64 || !v.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') || v.starts_with('-') {
        return Err(Error::invalid("slug", format!("`{s}`: letters, digits and dashes (it is used in web addresses)")));
    }
    Ok(v)
}

const ORG_COLS: &str = "id, name, slug, description, visibility, access, owner_peer, apply_role, invite_role, hosted, member_auth, template_id, author_id, created_at, updated_at";
const MEMBER_COLS: &str = "id, org_id, peer_id, label, role_id, status, exchange_id, note, decided_by, created_at, updated_at";
const RES_COLS: &str = "id, org_id, kind, ref_id, name, transferred_by, state, published, created_at, updated_at";
const JOINED_COLS: &str = "org_id, name, slug, node_peer, exchange_id, status, role, perms, info, created_at, updated_at";

fn row_org(r: &rusqlite::Row) -> rusqlite::Result<Org> {
    let v: String = r.get(4)?;
    let a: String = r.get(5)?;
    Ok(Org {
        id: r.get(0)?,
        name: r.get(1)?,
        slug: r.get(2)?,
        description: r.get(3)?,
        visibility: Visibility::parse(&v).unwrap_or_default(),
        access: Access::parse(&a).unwrap_or_default(),
        owner_peer: r.get(6)?,
        apply_role: r.get(7)?,
        invite_role: r.get(8)?,
        hosted: r.get::<_, i64>(9)? != 0,
        member_auth: r.get::<_, i64>(10)? != 0,
        template_id: r.get(11)?,
        author_id: r.get(12)?,
        created_at: r.get(13)?,
        updated_at: r.get(14)?,
    })
}

fn row_role(r: &rusqlite::Row) -> rusqlite::Result<Role> {
    let p: String = r.get(4)?;
    Ok(Role {
        id: r.get(0)?,
        org_id: r.get(1)?,
        name: r.get(2)?,
        rank: r.get(3)?,
        perms: serde_json::from_str(&p).unwrap_or_default(),
        builtin: r.get::<_, i64>(5)? != 0,
        created_at: r.get(6)?,
    })
}

fn row_member(r: &rusqlite::Row) -> rusqlite::Result<Member> {
    let st: String = r.get(5)?;
    Ok(Member {
        id: r.get(0)?,
        org_id: r.get(1)?,
        peer_id: r.get(2)?,
        label: r.get(3)?,
        role_id: r.get(4)?,
        status: MemberStatus::parse(&st),
        exchange_id: r.get(6)?,
        note: r.get(7)?,
        decided_by: r.get(8)?,
        created_at: r.get(9)?,
        updated_at: r.get(10)?,
    })
}

fn row_resource(r: &rusqlite::Row) -> rusqlite::Result<Resource> {
    let k: String = r.get(2)?;
    let st: String = r.get(6)?;
    let p: String = r.get(7)?;
    Ok(Resource {
        id: r.get(0)?,
        org_id: r.get(1)?,
        kind: super::ShareKind::parse(&k).unwrap_or(super::ShareKind::Project),
        ref_id: r.get(3)?,
        name: r.get(4)?,
        transferred_by: r.get(5)?,
        state: if st == "archived" { ResourceState::Archived } else { ResourceState::Active },
        published: Published::parse(&p).unwrap_or(Published::No),
        created_at: r.get(8)?,
        updated_at: r.get(9)?,
    })
}

fn row_joined(r: &rusqlite::Row) -> rusqlite::Result<Joined> {
    let st: String = r.get(5)?;
    let p: String = r.get(7)?;
    let i: String = r.get(8)?;
    Ok(Joined {
        org_id: r.get(0)?,
        name: r.get(1)?,
        slug: r.get(2)?,
        node_peer: r.get(3)?,
        exchange_id: r.get(4)?,
        status: MemberStatus::parse(&st),
        role: r.get(6)?,
        perms: serde_json::from_str(&p).unwrap_or_default(),
        info: serde_json::from_str(&i).unwrap_or_default(),
        created_at: r.get(9)?,
        updated_at: r.get(10)?,
    })
}

/// A new role (for [`Store::create_role`]).
#[derive(Debug, Clone)]
pub struct NewRole {
    pub name: String,
    pub rank: i64,
    pub perms: Vec<String>,
    pub builtin: bool,
}

impl Store {
    // ---------- orgs ----------

    pub fn orgs(&self) -> Result<Vec<Org>> {
        let mut st = self.conn.prepare(&format!("SELECT {ORG_COLS} FROM orgs ORDER BY name COLLATE NOCASE, created_at"))?;
        let v = st.query_map([], row_org)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn org(&self, id: &str) -> Result<Option<Org>> {
        Ok(self.conn.query_row(&format!("SELECT {ORG_COLS} FROM orgs WHERE id = ?1"), [id], row_org).optional()?)
    }

    /// By id, id prefix, name or slug.
    pub fn find_org(&self, key: &str) -> Result<Org> {
        let all = self.orgs()?;
        if let Some(o) = all.iter().find(|o| o.slug == key.trim().to_ascii_lowercase()) {
            return Ok(o.clone());
        }
        super::pick(all, key, |o| &o.id, |o| &o.name, "organization", "mnnn org list")
    }

    /// Insert an org (roles are added by the caller, see `crate::org::create`).
    pub fn insert_org(&self, name: &str, slug: Option<&str>, description: &str, owner_peer: &str) -> Result<Org> {
        let name = check_name(name, "organization name")?;
        let slug = match slug {
            Some(s) => check_slug(s)?,
            None => {
                let base = slugify(&name);
                let taken: Vec<String> = self.orgs()?.into_iter().map(|o| o.slug).chain(self.mirrors()?.into_iter().map(|m| m.slug)).collect();
                if taken.contains(&base) { (2..).map(|i| format!("{base}-{i}")).find(|s| !taken.contains(s)).unwrap_or(base) } else { base }
            }
        };
        if self.orgs()?.iter().any(|o| o.slug == slug) {
            return Err(Error::invalid("slug", format!("`{slug}` is taken by another organization here")));
        }
        let now = now_ms();
        let id = new_id();
        self.conn.execute(
            "INSERT INTO orgs(id, name, slug, description, owner_peer, author_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
            params![id, name, slug, description.trim(), owner_peer, self.current_author_id()?, now],
        )?;
        self.org(&id)?.ok_or_else(|| Error::Other("organization vanished".into()))
    }

    pub fn update_org(&self, id: &str, c: &OrgChange) -> Result<Org> {
        let mut o = self.org(id)?.ok_or_else(|| Error::invalid("organization", format!("`{id}` not found")))?;
        if let Some(n) = &c.name {
            o.name = check_name(n, "organization name")?;
        }
        if let Some(s) = &c.slug {
            let s = check_slug(s)?;
            if self.orgs()?.iter().any(|x| x.slug == s && x.id != o.id) {
                return Err(Error::invalid("slug", format!("`{s}` is taken by another organization here")));
            }
            o.slug = s;
        }
        if let Some(d) = &c.description {
            o.description = d.trim().to_string();
        }
        if let Some(v) = c.visibility {
            o.visibility = v;
        }
        if let Some(a) = c.access {
            o.access = a;
        }
        if let Some(r) = &c.apply_role {
            o.apply_role = Some(r.clone());
        }
        if let Some(r) = &c.invite_role {
            o.invite_role = Some(r.clone());
        }
        if let Some(h) = c.hosted {
            o.hosted = h;
        }
        if let Some(m) = c.member_auth {
            o.member_auth = m;
        }
        if let Some(t) = &c.template_id {
            o.template_id = t.clone();
        }
        o.updated_at = now_ms();
        self.conn.execute(
            "UPDATE orgs SET name = ?2, slug = ?3, description = ?4, visibility = ?5, access = ?6, apply_role = ?7, invite_role = ?8,
             hosted = ?9, member_auth = ?10, template_id = ?11, updated_at = ?12 WHERE id = ?1",
            params![
                o.id,
                o.name,
                o.slug,
                o.description,
                o.visibility.as_str(),
                o.access.as_str(),
                o.apply_role,
                o.invite_role,
                o.hosted as i64,
                o.member_auth as i64,
                o.template_id,
                o.updated_at
            ],
        )?;
        Ok(o)
    }

    /// Delete an org and everything that belongs only to it (roles, memberships, links,
    /// templates, endpoints). The projects, journals and other things it held stay.
    pub fn delete_org(&self, id: &str) -> Result<Org> {
        let o = self.org(id)?.ok_or_else(|| Error::invalid("organization", format!("`{id}` not found")))?;
        self.conn.execute("DELETE FROM orgs WHERE id = ?1", [id])?;
        Ok(o)
    }

    // ---------- roles ----------

    pub fn roles(&self, org: &str) -> Result<Vec<Role>> {
        let mut st = self
            .conn
            .prepare("SELECT id, org_id, name, rank, perms, builtin, created_at FROM org_roles WHERE org_id = ?1 ORDER BY rank DESC, name")?;
        let v = st.query_map([org], row_role)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn role(&self, id: &str) -> Result<Option<Role>> {
        Ok(self
            .conn
            .query_row("SELECT id, org_id, name, rank, perms, builtin, created_at FROM org_roles WHERE id = ?1", [id], row_role)
            .optional()?)
    }

    pub fn find_role(&self, org: &str, key: &str) -> Result<Role> {
        super::pick(self.roles(org)?, key, |r| &r.id, |r| &r.name, "role", "mnnn org role list ORG")
    }

    pub fn create_role(&self, org: &str, r: &NewRole) -> Result<Role> {
        let name = check_name(&r.name, "role name")?.to_lowercase();
        if self.roles(org)?.iter().any(|x| x.name == name) {
            return Err(Error::invalid("role", format!("`{name}` already exists")));
        }
        let id = new_id();
        self.conn.execute(
            "INSERT INTO org_roles(id, org_id, name, rank, perms, builtin, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, org, name, r.rank, serde_json::to_string(&r.perms)?, r.builtin as i64, now_ms()],
        )?;
        self.role(&id)?.ok_or_else(|| Error::Other("role vanished".into()))
    }

    pub fn update_role(&self, id: &str, name: Option<&str>, rank: Option<i64>, perms: Option<&[String]>) -> Result<Role> {
        let mut r = self.role(id)?.ok_or_else(|| Error::invalid("role", format!("`{id}` not found")))?;
        if let Some(n) = name {
            let n = check_name(n, "role name")?.to_lowercase();
            if self.roles(&r.org_id)?.iter().any(|x| x.name == n && x.id != r.id) {
                return Err(Error::invalid("role", format!("`{n}` already exists")));
            }
            r.name = n;
        }
        if let Some(k) = rank {
            r.rank = k;
        }
        if let Some(p) = perms {
            r.perms = p.to_vec();
        }
        self.conn.execute(
            "UPDATE org_roles SET name = ?2, rank = ?3, perms = ?4 WHERE id = ?1",
            params![r.id, r.name, r.rank, serde_json::to_string(&r.perms)?],
        )?;
        Ok(r)
    }

    /// Delete a role; its members (and project roles) move to `fallback`.
    pub fn delete_role(&self, id: &str, fallback: &str) -> Result<Role> {
        let r = self.role(id)?.ok_or_else(|| Error::invalid("role", format!("`{id}` not found")))?;
        self.conn.execute("UPDATE org_members SET role_id = ?2 WHERE role_id = ?1", params![id, fallback])?;
        self.conn.execute("UPDATE org_project_members SET role_id = NULL WHERE role_id = ?1", [id])?;
        self.conn.execute("DELETE FROM org_roles WHERE id = ?1", [id])?;
        Ok(r)
    }

    // ---------- members ----------

    pub fn members(&self, org: &str) -> Result<Vec<Member>> {
        let mut st = self.conn.prepare(&format!("SELECT {MEMBER_COLS} FROM org_members WHERE org_id = ?1 ORDER BY created_at"))?;
        let v = st.query_map([org], row_member)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn member(&self, id: &str) -> Result<Option<Member>> {
        Ok(self.conn.query_row(&format!("SELECT {MEMBER_COLS} FROM org_members WHERE id = ?1"), [id], row_member).optional()?)
    }

    pub fn member_by_peer(&self, org: &str, peer: &str) -> Result<Option<Member>> {
        Ok(self
            .conn
            .query_row(&format!("SELECT {MEMBER_COLS} FROM org_members WHERE org_id = ?1 AND peer_id = ?2"), [org, peer], row_member)
            .optional()?)
    }

    /// The member a user is in an org (any of their nodes acts with it), when known.
    pub fn member_by_user(&self, org: &str, user: &str) -> Result<Option<Member>> {
        Ok(self
            .conn
            .query_row(
                &format!(
                    "SELECT {MEMBER_COLS} FROM org_members WHERE org_id = ?1 AND user_id = ?2 ORDER BY status = 'member' DESC, created_at LIMIT 1"
                ),
                [org, user],
                row_member,
            )
            .optional()?)
    }

    pub fn member_by_exchange(&self, exchange: &str) -> Result<Option<Member>> {
        Ok(self.conn.query_row(&format!("SELECT {MEMBER_COLS} FROM org_members WHERE exchange_id = ?1"), [exchange], row_member).optional()?)
    }

    /// By id, id prefix, label or peer id (or its last 8 characters).
    pub fn find_member(&self, org: &str, key: &str) -> Result<Member> {
        let all = self.members(org)?;
        let k = key.trim();
        if let Some(m) = all.iter().find(|m| m.peer_id == k || (k.len() >= 6 && m.peer_id.ends_with(k))) {
            return Ok(m.clone());
        }
        super::pick(all, k, |m| &m.id, |m| &m.label, "member", "mnnn org member list ORG")
    }

    /// Add or replace the membership of a node (a new application replaces a rejected
    /// or ended one; the id stays).
    #[allow(clippy::too_many_arguments)]
    pub fn upsert_member(
        &self,
        org: &str,
        peer: &str,
        label: &str,
        role: Option<&str>,
        status: MemberStatus,
        exchange: Option<&str>,
        note: &str,
        decided_by: Option<&str>,
    ) -> Result<Member> {
        let now = now_ms();
        match self.member_by_peer(org, peer)? {
            Some(m) => {
                self.conn.execute(
                    "UPDATE org_members SET label = ?2, role_id = ?3, status = ?4, exchange_id = ?5, note = ?6, decided_by = ?7, updated_at = ?8 WHERE id = ?1",
                    params![m.id, if label.is_empty() { &m.label } else { label }, role, status.as_str(), exchange, note, decided_by, now],
                )?;
                self.member(&m.id)?.ok_or_else(|| Error::Other("member vanished".into()))
            }
            None => {
                let id = new_id();
                self.conn.execute(
                    &format!("INSERT INTO org_members({MEMBER_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)"),
                    params![id, org, peer, label, role, status.as_str(), exchange, note, decided_by, now],
                )?;
                self.member(&id)?.ok_or_else(|| Error::Other("member vanished".into()))
            }
        }
    }

    pub fn set_member(&self, id: &str, status: Option<MemberStatus>, role: Option<&str>, decided_by: Option<&str>) -> Result<Member> {
        let m = self.member(id)?.ok_or_else(|| Error::invalid("member", format!("`{id}` not found")))?;
        self.conn.execute(
            "UPDATE org_members SET status = ?2, role_id = ?3, decided_by = COALESCE(?4, decided_by), updated_at = ?5 WHERE id = ?1",
            params![id, status.unwrap_or(m.status).as_str(), role.map(str::to_string).or(m.role_id), decided_by, now_ms()],
        )?;
        self.member(id)?.ok_or_else(|| Error::Other("member vanished".into()))
    }

    /// An invitation answered: the member's real node. A node that was a member before
    /// keeps its old membership row (the invitation's row goes).
    pub fn set_member_peer(&self, id: &str, peer: &str, label: &str) -> Result<Member> {
        let m = self.member(id)?.ok_or_else(|| Error::invalid("member", format!("`{id}` not found")))?;
        if let Some(old) = self.member_by_peer(&m.org_id, peer)?
            && old.id != m.id
        {
            self.conn.execute("DELETE FROM org_members WHERE id = ?1", [&m.id])?;
            return self.upsert_member(
                &m.org_id,
                peer,
                label,
                m.role_id.as_deref(),
                m.status,
                m.exchange_id.as_deref(),
                &m.note,
                m.decided_by.as_deref(),
            );
        }
        self.conn.execute(
            "UPDATE org_members SET peer_id = ?2, label = CASE WHEN ?3 = '' THEN label ELSE ?3 END, updated_at = ?4 WHERE id = ?1",
            params![id, peer, label, now_ms()],
        )?;
        self.member(id)?.ok_or_else(|| Error::Other("member vanished".into()))
    }

    pub fn delete_member(&self, id: &str) -> Result<bool> {
        Ok(self.conn.execute("DELETE FROM org_members WHERE id = ?1", [id])? > 0)
    }

    pub fn set_member_exchange(&self, id: &str, exchange: Option<&str>) -> Result<()> {
        self.conn.execute("UPDATE org_members SET exchange_id = ?2, updated_at = ?3 WHERE id = ?1", params![id, exchange, now_ms()])?;
        Ok(())
    }

    // ---------- resources ----------

    pub fn resources(&self, org: &str) -> Result<Vec<Resource>> {
        let mut st = self.conn.prepare(&format!("SELECT {RES_COLS} FROM org_resources WHERE org_id = ?1 ORDER BY kind, name COLLATE NOCASE"))?;
        let v = st.query_map([org], row_resource)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn resource(&self, id: &str) -> Result<Option<Resource>> {
        Ok(self.conn.query_row(&format!("SELECT {RES_COLS} FROM org_resources WHERE id = ?1"), [id], row_resource).optional()?)
    }

    pub fn find_resource(&self, org: &str, key: &str) -> Result<Resource> {
        let all = self.resources(org)?;
        let k = key.trim();
        if let Some(r) = all.iter().find(|r| r.ref_id == k) {
            return Ok(r.clone());
        }
        if let Some((kind, name)) = k.split_once('/')
            && let Some(kd) = super::ShareKind::parse(kind)
        {
            let m: Vec<&Resource> = all.iter().filter(|r| r.kind == kd && r.name.eq_ignore_ascii_case(name.trim())).collect();
            if m.len() == 1 {
                return Ok(m[0].clone());
            }
        }
        super::pick(all, k, |r| &r.id, |r| &r.name, "org resource", "mnnn org resources ORG")
    }

    /// Give a thing to an org (again: returns the existing one with `false`).
    pub fn add_resource(&self, org: &str, kind: super::ShareKind, ref_id: &str, name: &str, by: Option<&str>) -> Result<(Resource, bool)> {
        let now = now_ms();
        let n = self.conn.execute(
            "INSERT OR IGNORE INTO org_resources(id, org_id, kind, ref_id, name, transferred_by, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
            params![new_id(), org, kind.as_str(), ref_id, name, by, now],
        )?;
        let r = self.conn.query_row(
            &format!("SELECT {RES_COLS} FROM org_resources WHERE org_id = ?1 AND kind = ?2 AND ref_id = ?3"),
            params![org, kind.as_str(), ref_id],
            row_resource,
        )?;
        Ok((r, n == 1))
    }

    pub fn set_resource(&self, id: &str, state: Option<ResourceState>, published: Option<Published>) -> Result<Resource> {
        let r = self.resource(id)?.ok_or_else(|| Error::invalid("org resource", format!("`{id}` not found")))?;
        let st = state.unwrap_or(r.state);
        let p = published.unwrap_or(r.published);
        self.conn.execute(
            "UPDATE org_resources SET state = ?2, published = ?3, updated_at = ?4 WHERE id = ?1",
            params![id, if st == ResourceState::Archived { "archived" } else { "active" }, p.as_str(), now_ms()],
        )?;
        self.resource(id)?.ok_or_else(|| Error::Other("resource vanished".into()))
    }

    pub fn remove_resource(&self, id: &str) -> Result<Resource> {
        let r = self.resource(id)?.ok_or_else(|| Error::invalid("org resource", format!("`{id}` not found")))?;
        self.conn.execute("DELETE FROM org_resources WHERE id = ?1", [id])?;
        Ok(r)
    }

    // ---------- projects ----------

    pub fn org_projects(&self, org: &str) -> Result<Vec<super::Project>> {
        let ids: Vec<String> = {
            let mut st = self.conn.prepare("SELECT project_id FROM org_projects WHERE org_id = ?1 ORDER BY added_at")?;
            st.query_map([org], |r| r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let mut v = vec![];
        for id in ids {
            if let Some(p) = self.project(&id)? {
                v.push(p);
            }
        }
        Ok(v)
    }

    /// The orgs a project is in.
    pub fn orgs_of_project(&self, project: &str) -> Result<Vec<String>> {
        let mut st = self.conn.prepare("SELECT org_id FROM org_projects WHERE project_id = ?1")?;
        let v = st.query_map([project], |r| r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn add_org_project(&self, org: &str, project: &str) -> Result<bool> {
        Ok(self
            .conn
            .execute("INSERT OR IGNORE INTO org_projects(org_id, project_id, added_at) VALUES (?1, ?2, ?3)", params![org, project, now_ms()])?
            == 1)
    }

    pub fn remove_org_project(&self, org: &str, project: &str) -> Result<bool> {
        self.conn.execute("DELETE FROM org_project_members WHERE org_id = ?1 AND project_id = ?2", params![org, project])?;
        Ok(self.conn.execute("DELETE FROM org_projects WHERE org_id = ?1 AND project_id = ?2", params![org, project])? == 1)
    }

    pub fn project_members(&self, org: &str, project: &str) -> Result<Vec<ProjectMember>> {
        let mut st = self.conn.prepare(
            "SELECT org_id, project_id, member_id, role_id, added_at FROM org_project_members WHERE org_id = ?1 AND project_id = ?2 ORDER BY added_at",
        )?;
        let v = st
            .query_map([org, project], |r| {
                Ok(ProjectMember { org_id: r.get(0)?, project_id: r.get(1)?, member_id: r.get(2)?, role_id: r.get(3)?, added_at: r.get(4)? })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn set_project_member(&self, org: &str, project: &str, member: &str, role: Option<&str>) -> Result<()> {
        self.conn.execute(
            "INSERT INTO org_project_members(org_id, project_id, member_id, role_id, added_at) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(org_id, project_id, member_id) DO UPDATE SET role_id = ?4",
            params![org, project, member, role, now_ms()],
        )?;
        Ok(())
    }

    pub fn remove_project_member(&self, org: &str, project: &str, member: &str) -> Result<bool> {
        Ok(self
            .conn
            .execute("DELETE FROM org_project_members WHERE org_id = ?1 AND project_id = ?2 AND member_id = ?3", params![org, project, member])?
            == 1)
    }

    // ---------- templates ----------

    pub fn templates(&self, org: &str) -> Result<Vec<Template>> {
        let mut st = self.conn.prepare(
            "SELECT id, org_id, name, html, css, created_at, updated_at FROM org_templates WHERE org_id = ?1 ORDER BY name COLLATE NOCASE",
        )?;
        let v = st
            .query_map([org], |r| {
                Ok(Template {
                    id: r.get(0)?,
                    org_id: r.get(1)?,
                    name: r.get(2)?,
                    html: r.get(3)?,
                    css: r.get(4)?,
                    created_at: r.get(5)?,
                    updated_at: r.get(6)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn find_template(&self, org: &str, key: &str) -> Result<Template> {
        super::pick(self.templates(org)?, key, |t| &t.id, |t| &t.name, "template", "mnnn org template list ORG")
    }

    pub fn template(&self, id: &str) -> Result<Option<Template>> {
        let org: Option<String> = self.conn.query_row("SELECT org_id FROM org_templates WHERE id = ?1", [id], |r| r.get(0)).optional()?;
        Ok(match org {
            Some(o) => self.templates(&o)?.into_iter().find(|t| t.id == id),
            None => None,
        })
    }

    pub fn save_template(&self, org: &str, name: &str, html: &str, css: &str) -> Result<(Template, bool)> {
        let name = check_name(name, "template name")?;
        let now = now_ms();
        if let Ok(t) = self.find_template(org, &name)
            && t.name.eq_ignore_ascii_case(&name)
        {
            self.conn.execute("UPDATE org_templates SET html = ?2, css = ?3, updated_at = ?4 WHERE id = ?1", params![t.id, html, css, now])?;
            return Ok((self.template(&t.id)?.ok_or_else(|| Error::Other("template vanished".into()))?, false));
        }
        let id = new_id();
        self.conn.execute(
            "INSERT INTO org_templates(id, org_id, name, html, css, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            params![id, org, name, html, css, now],
        )?;
        Ok((self.template(&id)?.ok_or_else(|| Error::Other("template vanished".into()))?, true))
    }

    pub fn delete_template(&self, id: &str) -> Result<()> {
        self.conn.execute("UPDATE orgs SET template_id = NULL WHERE template_id = ?1", [id])?;
        self.conn.execute("DELETE FROM org_templates WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn template_assets(&self, template: &str) -> Result<Vec<TemplateAsset>> {
        let mut st = self
            .conn
            .prepare("SELECT template_id, name, mime, length(data), created_at FROM org_template_assets WHERE template_id = ?1 ORDER BY name")?;
        let v = st
            .query_map([template], |r| {
                Ok(TemplateAsset {
                    template_id: r.get(0)?,
                    name: r.get(1)?,
                    mime: r.get(2)?,
                    size: r.get::<_, i64>(3)? as usize,
                    created_at: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn template_asset(&self, template: &str, name: &str) -> Result<Option<(String, Vec<u8>)>> {
        Ok(self
            .conn
            .query_row("SELECT mime, data FROM org_template_assets WHERE template_id = ?1 AND name = ?2", [template, name], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()?)
    }

    pub fn put_template_asset(&self, template: &str, name: &str, mime: &str, data: &[u8]) -> Result<()> {
        self.conn.execute(
            "INSERT INTO org_template_assets(template_id, name, mime, data, created_at) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(template_id, name) DO UPDATE SET mime = ?3, data = ?4, created_at = ?5",
            params![template, name, mime, data, now_ms()],
        )?;
        Ok(())
    }

    pub fn delete_template_asset(&self, template: &str, name: &str) -> Result<bool> {
        Ok(self.conn.execute("DELETE FROM org_template_assets WHERE template_id = ?1 AND name = ?2", [template, name])? == 1)
    }

    // ---------- endpoints ----------

    pub fn endpoints(&self, org: &str) -> Result<Vec<Endpoint>> {
        let mut st = self
            .conn
            .prepare("SELECT id, org_id, name, kind, ref_id, perm, anyone, created_at FROM org_endpoints WHERE org_id = ?1 ORDER BY name")?;
        let v = st
            .query_map([org], |r| {
                let k: String = r.get(3)?;
                Ok(Endpoint {
                    id: r.get(0)?,
                    org_id: r.get(1)?,
                    name: r.get(2)?,
                    kind: EndpointKind::parse(&k).unwrap_or(EndpointKind::Records),
                    ref_id: r.get(4)?,
                    perm: r.get(5)?,
                    anyone: r.get::<_, i64>(6)? != 0,
                    created_at: r.get(7)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn find_endpoint(&self, org: &str, key: &str) -> Result<Endpoint> {
        super::pick(self.endpoints(org)?, key, |e| &e.id, |e| &e.name, "endpoint", "mnnn org endpoint list ORG")
    }

    pub fn save_endpoint(&self, org: &str, name: &str, kind: EndpointKind, ref_id: &str, perm: &str, anyone: bool) -> Result<Endpoint> {
        let name = check_slug(name).map_err(|_| Error::invalid("endpoint name", format!("`{name}`: letters, digits and dashes")))?;
        self.conn.execute(
            "INSERT INTO org_endpoints(id, org_id, name, kind, ref_id, perm, anyone, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(org_id, name) DO UPDATE SET kind = ?4, ref_id = ?5, perm = ?6, anyone = ?7",
            params![new_id(), org, name, kind.as_str(), ref_id, perm, anyone as i64, now_ms()],
        )?;
        self.find_endpoint(org, &name)
    }

    pub fn delete_endpoint(&self, id: &str) -> Result<bool> {
        Ok(self.conn.execute("DELETE FROM org_endpoints WHERE id = ?1", [id])? == 1)
    }

    // ---------- rules ----------

    pub fn org_rules(&self, org: &str) -> Result<Vec<String>> {
        let mut st = self.conn.prepare("SELECT rule_id FROM org_rules WHERE org_id = ?1 ORDER BY added_at")?;
        let v = st.query_map([org], |r| r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn add_org_rule(&self, org: &str, rule: &str) -> Result<bool> {
        Ok(self.conn.execute("INSERT OR IGNORE INTO org_rules(org_id, rule_id, added_at) VALUES (?1, ?2, ?3)", params![org, rule, now_ms()])? == 1)
    }

    pub fn remove_org_rule(&self, org: &str, rule: &str) -> Result<bool> {
        Ok(self.conn.execute("DELETE FROM org_rules WHERE org_id = ?1 AND rule_id = ?2", [org, rule])? == 1)
    }

    // ---------- nodes ----------

    pub fn org_nodes(&self, org: &str) -> Result<Vec<OrgNode>> {
        let mut st = self.conn.prepare("SELECT org_id, peer_id, label, url, added_at, seen_at FROM org_nodes WHERE org_id = ?1 ORDER BY added_at")?;
        let v = st
            .query_map([org], |r| {
                Ok(OrgNode { org_id: r.get(0)?, peer_id: r.get(1)?, label: r.get(2)?, url: r.get(3)?, added_at: r.get(4)?, seen_at: r.get(5)? })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn upsert_org_node(&self, org: &str, peer: &str, label: &str, url: Option<&str>) -> Result<()> {
        let now = now_ms();
        self.conn.execute(
            "INSERT INTO org_nodes(org_id, peer_id, label, url, added_at, seen_at) VALUES (?1, ?2, ?3, COALESCE(?4, ''), ?5, ?5)
             ON CONFLICT(org_id, peer_id) DO UPDATE SET label = CASE WHEN ?3 = '' THEN label ELSE ?3 END, url = COALESCE(?4, url), seen_at = ?5",
            params![org, peer, label, url, now],
        )?;
        Ok(())
    }

    pub fn remove_org_node(&self, org: &str, peer: &str) -> Result<bool> {
        Ok(self.conn.execute("DELETE FROM org_nodes WHERE org_id = ?1 AND peer_id = ?2", [org, peer])? == 1)
    }

    // ---------- code this node serves ----------

    /// The ids of the code spaces this node runs for an org.
    pub fn served_code(&self, org: &str) -> Result<Vec<String>> {
        let mut st = self.conn.prepare("SELECT code_id FROM org_served_code WHERE org_id = ?1 ORDER BY added_at")?;
        let v = st.query_map([org], |r| r.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?;
        Ok(v)
    }

    pub fn serves_code(&self, org: &str, code: &str) -> Result<bool> {
        Ok(self.conn.query_row("SELECT COUNT(*) FROM org_served_code WHERE org_id = ?1 AND code_id = ?2", [org, code], |r| r.get::<_, i64>(0))? > 0)
    }

    /// Serve a code space for an org (false when it already was).
    pub fn serve_code(&self, org: &str, code: &str) -> Result<bool> {
        Ok(self.conn.execute("INSERT OR IGNORE INTO org_served_code(org_id, code_id, added_at) VALUES (?1, ?2, ?3)", params![org, code, now_ms()])?
            == 1)
    }

    pub fn unserve_code(&self, org: &str, code: &str) -> Result<bool> {
        Ok(self.conn.execute("DELETE FROM org_served_code WHERE org_id = ?1 AND code_id = ?2", [org, code])? == 1)
    }

    /// The local copy of an org's code resource (on a node that syncs it).
    pub fn code_copy(&self, org: &str, resource: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT code_id FROM org_code_copies WHERE org_id = ?1 AND resource_id = ?2", [org, resource], |r| r.get(0))
            .optional()?)
    }

    pub fn save_code_copy(&self, org: &str, resource: &str, code: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO org_code_copies(org_id, resource_id, code_id, synced_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(org_id, resource_id) DO UPDATE SET code_id = ?3, synced_at = ?4",
            params![org, resource, code, now_ms()],
        )?;
        Ok(())
    }

    // ---------- joined (orgs on other nodes) ----------

    pub fn joined(&self) -> Result<Vec<Joined>> {
        let mut st = self.conn.prepare(&format!("SELECT {JOINED_COLS} FROM org_joined ORDER BY name COLLATE NOCASE"))?;
        let v = st.query_map([], row_joined)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn joined_org(&self, org: &str) -> Result<Option<Joined>> {
        Ok(self.conn.query_row(&format!("SELECT {JOINED_COLS} FROM org_joined WHERE org_id = ?1"), [org], row_joined).optional()?)
    }

    pub fn joined_by_exchange(&self, exchange: &str) -> Result<Option<Joined>> {
        Ok(self.conn.query_row(&format!("SELECT {JOINED_COLS} FROM org_joined WHERE exchange_id = ?1"), [exchange], row_joined).optional()?)
    }

    pub fn find_joined(&self, key: &str) -> Result<Joined> {
        let all = self.joined()?;
        if let Some(j) = all.iter().find(|j| !j.slug.is_empty() && j.slug == key.trim().to_ascii_lowercase()) {
            return Ok(j.clone());
        }
        super::pick(all, key, |j| &j.org_id, |j| &j.name, "joined organization", "mnnn org list")
    }

    pub fn save_joined(&self, j: &Joined) -> Result<()> {
        self.conn.execute(
            &format!(
                "INSERT INTO org_joined({JOINED_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                 ON CONFLICT(org_id) DO UPDATE SET name = ?2, slug = ?3, node_peer = ?4, exchange_id = ?5, status = ?6, role = ?7, perms = ?8,
                 info = ?9, updated_at = ?11"
            ),
            params![
                j.org_id,
                j.name,
                j.slug,
                j.node_peer,
                j.exchange_id,
                j.status.as_str(),
                j.role,
                serde_json::to_string(&j.perms)?,
                j.info.to_string(),
                j.created_at,
                j.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn forget_joined(&self, org: &str) -> Result<bool> {
        Ok(self.conn.execute("DELETE FROM org_joined WHERE org_id = ?1", [org])? == 1)
    }

    // ---------- mirrors ----------

    pub fn mirrors(&self) -> Result<Vec<Mirror>> {
        let mut st = self.conn.prepare("SELECT org_id, slug, node_peer, snapshot, fetched_at FROM org_mirrors ORDER BY slug")?;
        let v = st
            .query_map([], |r| Ok(Mirror { org_id: r.get(0)?, slug: r.get(1)?, node_peer: r.get(2)?, snapshot: r.get(3)?, fetched_at: r.get(4)? }))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn save_mirror(&self, m: &Mirror) -> Result<()> {
        self.conn.execute(
            "INSERT INTO org_mirrors(org_id, slug, node_peer, snapshot, fetched_at) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(org_id) DO UPDATE SET slug = ?2, node_peer = ?3, snapshot = ?4, fetched_at = ?5",
            params![m.org_id, m.slug, m.node_peer, m.snapshot, m.fetched_at],
        )?;
        Ok(())
    }

    pub fn drop_mirror(&self, org: &str) -> Result<bool> {
        Ok(self.conn.execute("DELETE FROM org_mirrors WHERE org_id = ?1", [org])? == 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::ShareKind;

    #[test]
    fn orgs_roles_members_and_resources_round_trip() {
        let s = Store::open_in_memory().unwrap();
        let o = s.insert_org("Research Lab", None, "we look", "12D3KooWowner").unwrap();
        assert_eq!((o.slug.as_str(), o.visibility, o.access, o.hosted), ("research-lab", Visibility::Private, Access::Members, false));
        let o2 = s.insert_org("Research  Lab!", None, "", "p").unwrap();
        assert_eq!(o2.slug, "research-lab-2");
        assert!(s.insert_org("x", Some("research-lab"), "", "p").is_err());
        assert!(s.insert_org("x", Some("bad slug"), "", "p").is_err());
        assert_eq!(s.find_org("research-lab").unwrap().id, o.id);
        let r = s.create_role(&o.id, &NewRole { name: "Analyst".into(), rank: 40, perms: vec!["read".into()], builtin: false }).unwrap();
        assert_eq!(r.name, "analyst");
        assert!(s.create_role(&o.id, &NewRole { name: "analyst".into(), rank: 1, perms: vec![], builtin: false }).is_err());
        let viewer = s.create_role(&o.id, &NewRole { name: "viewer".into(), rank: 10, perms: vec![], builtin: true }).unwrap();
        let m = s.upsert_member(&o.id, "12D3KooWbenbenben", "ben", Some(&r.id), MemberStatus::Applied, Some("x1"), "hi", None).unwrap();
        assert_eq!(s.find_member(&o.id, "benbenben").unwrap().id, m.id);
        assert_eq!(s.member_by_exchange("x1").unwrap().unwrap().id, m.id);
        let m = s.set_member(&m.id, Some(MemberStatus::Member), None, Some("owner")).unwrap();
        assert_eq!((m.status, m.role_id.as_deref()), (MemberStatus::Member, Some(r.id.as_str())));
        s.delete_role(&r.id, &viewer.id).unwrap();
        assert_eq!(s.member(&m.id).unwrap().unwrap().role_id.as_deref(), Some(viewer.id.as_str()));
        let (res, new) = s.add_resource(&o.id, ShareKind::Journal, "j1", "Notes", Some(&m.id)).unwrap();
        assert!(new && !s.add_resource(&o.id, ShareKind::Journal, "j1", "Notes", None).unwrap().1);
        assert_eq!(s.find_resource(&o.id, "journal/notes").unwrap().id, res.id);
        let res = s.set_resource(&res.id, Some(ResourceState::Archived), Some(Published::Public)).unwrap();
        assert_eq!((res.state, res.published), (ResourceState::Archived, Published::Public));
        let (t, made) = s.save_template(&o.id, "plain", "<html>{{content}}</html>", "body{}").unwrap();
        assert!(made && !s.save_template(&o.id, "plain", "<p>{{content}}</p>", "").unwrap().1);
        s.put_template_asset(&t.id, "logo.png", "image/png", &[1, 2, 3]).unwrap();
        assert_eq!(s.template_assets(&t.id).unwrap()[0].size, 3);
        assert_eq!(s.template_asset(&t.id, "logo.png").unwrap().unwrap().1, vec![1, 2, 3]);
        let e = s.save_endpoint(&o.id, "add-trade", EndpointKind::Records, "trades", "records.write", false).unwrap();
        assert_eq!(s.find_endpoint(&o.id, "add-trade").unwrap().id, e.id);
        s.upsert_org_node(&o.id, "12D3KooWowner", "anna", None).unwrap();
        s.upsert_org_node(&o.id, "12D3KooWowner", "", Some("https://x")).unwrap();
        let n = &s.org_nodes(&o.id).unwrap()[0];
        assert_eq!((n.label.as_str(), n.url.as_str()), ("anna", "https://x"));
        let o = s.update_org(&o.id, &OrgChange { visibility: Some(Visibility::Public), hosted: Some(true), ..Default::default() }).unwrap();
        assert!(o.hosted && o.visibility == Visibility::Public);
        s.delete_org(&o.id).unwrap();
        assert!(s.members(&o.id).unwrap().is_empty() && s.resources(&o.id).unwrap().is_empty() && s.templates(&o.id).unwrap().is_empty());
    }

    #[test]
    fn joined_orgs_and_mirrors() {
        let s = Store::open_in_memory().unwrap();
        let j = Joined {
            org_id: "o1".into(),
            name: "Lab".into(),
            slug: "lab".into(),
            node_peer: "p".into(),
            exchange_id: Some("x".into()),
            status: MemberStatus::Applied,
            role: String::new(),
            perms: vec![],
            info: serde_json::json!({}),
            created_at: 1,
            updated_at: 1,
        };
        s.save_joined(&j).unwrap();
        s.save_joined(&Joined { status: MemberStatus::Member, role: "user".into(), ..j.clone() }).unwrap();
        let got = s.find_joined("lab").unwrap();
        assert_eq!((got.status, got.role.as_str()), (MemberStatus::Member, "user"));
        assert_eq!(s.joined_by_exchange("x").unwrap().unwrap().org_id, "o1");
        s.save_mirror(&Mirror { org_id: "o1".into(), slug: "lab".into(), node_peer: "p".into(), snapshot: b"{}".to_vec(), fetched_at: 2 }).unwrap();
        assert_eq!(s.mirrors().unwrap().len(), 1);
        // a mirrored slug is not reused for a new org here
        assert_eq!(s.insert_org("Lab", None, "", "me").unwrap().slug, "lab-2");
        assert!(s.forget_joined("o1").unwrap() && s.drop_mirror("o1").unwrap());
    }
}
