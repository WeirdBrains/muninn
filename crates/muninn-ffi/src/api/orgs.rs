//! Read-only organizations (`mnnn org list` / `mnnn org show`).
//! The store is opened read-only; no keys or mirror snapshots are returned.
use super::data::open;
use muninn_core::org;
use muninn_core::store::net::short_peer;
use muninn_core::store::{Org, Store};

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

fn role_named(roles: &[muninn_core::store::Role], id: &Option<String>) -> String {
    id.as_deref().and_then(|r| roles.iter().find(|x| x.id == r)).map(|r| r.name.clone()).unwrap_or_else(|| "-".into())
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
        roles: roles.iter().map(|r| OrgRoleInfo { name: r.name.clone(), rank: r.rank, perms: r.perms.clone() }).collect(),
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

pub(crate) fn org_overview_of(s: &Store) -> anyhow::Result<OrgOverview> {
    let mut mine = Vec::new();
    for o in s.orgs()? {
        mine.push(mine_of(s, &o)?);
    }
    let joined = s
        .joined()?
        .into_iter()
        .map(|j| OrgJoined {
            name: j.name,
            slug: j.slug,
            status: j.status.as_str().into(),
            role: j.role,
            node: short_peer(&j.node_peer).to_string(),
            perms: j.perms,
        })
        .collect();
    let mirrors = s.mirrors()?.into_iter().map(|m| OrgMirror { slug: m.slug, fetched_at: m.fetched_at }).collect();
    Ok(OrgOverview { mine, joined, mirrors })
}

/// `mnnn org list` plus the fields `mnnn org show` prints for orgs this node owns.
pub fn org_overview() -> anyhow::Result<OrgOverview> {
    org_overview_of(&open()?)
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
    fn overview_needs_unlock() {
        lock();
        assert!(org_overview().is_err());
    }
}
