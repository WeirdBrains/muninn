use super::*;
use crate::store::{Joined, Store};

pub(crate) fn org_with_member(s: &Store, role: &str) -> (Org, Member) {
    let o = create(s, "cli", "Lab", None, "").unwrap();
    let r = s.find_role(&o.id, role).unwrap();
    let m = s.upsert_member(&o.id, "12D3KooWmembermember", "ben", Some(&r.id), MemberStatus::Member, None, "", Some("owner")).unwrap();
    (o, m)
}

#[test]
fn a_new_org_has_the_built_in_roles_and_its_owner() {
    let s = Store::open_in_memory().unwrap();
    let o = create(&s, "cli", "Research Lab", None, "we look").unwrap();
    let roles: Vec<String> = s.roles(&o.id).unwrap().into_iter().map(|r| r.name).collect();
    assert_eq!(roles, vec!["owner", "admin", "user", "viewer"]);
    assert_eq!((o.visibility, o.access), (Visibility::Private, Access::Members), "private with members by default");
    let user = s.find_role(&o.id, "user").unwrap();
    assert_eq!((o.apply_role.as_deref(), o.invite_role.as_deref()), (Some(user.id.as_str()), Some(user.id.as_str())));
    let ms = s.members(&o.id).unwrap();
    assert_eq!((ms.len(), role_name(&s, &ms[0]).as_str(), ms[0].peer_id.as_str()), (1, "owner", o.owner_peer.as_str()));
    assert_eq!(s.org_nodes(&o.id).unwrap().len(), 1);
    assert!(s.actions_of_kind("org.create").unwrap().len() == 1);
}

#[test]
fn roles_rank_and_permissions_decide() {
    let s = Store::open_in_memory().unwrap();
    let (o, ben) = org_with_member(&s, "admin");
    let admin = actor_for(&s, &ben).unwrap();
    assert!(can(&s, &o, &admin, "members.approve") && can(&s, &o, &admin, "members.defaults"));
    // an admin adds roles below itself, never at or above
    let analyst = create_role(&s, "cli", &admin, &o, "analyst", 40, &["read".into(), "call".into()]).unwrap();
    assert!(create_role(&s, "cli", &admin, &o, "boss", 90, &[]).is_err());
    assert!(create_role(&s, "cli", &Actor::Owner, &o, "x", 1, &["fly".into()]).is_err());
    // an admin cannot touch the owner or another admin, or hand out admin
    let owner_m = s.member_by_peer(&o.id, &o.owner_peer).unwrap().unwrap();
    assert!(set_role(&s, "cli", &admin, &o, &owner_m, &analyst).is_err());
    let cid = s.upsert_member(&o.id, "12D3KooWcarlcarl", "carl", Some(&analyst.id), MemberStatus::Member, None, "", None).unwrap();
    let adminr = s.find_role(&o.id, "admin").unwrap();
    assert!(set_role(&s, "cli", &admin, &o, &cid, &adminr).is_err());
    let viewer = s.find_role(&o.id, "viewer").unwrap();
    let carl = set_role(&s, "cli", &admin, &o, &cid, &viewer).unwrap();
    let carl_a = actor_for(&s, &carl).unwrap();
    assert!(can(&s, &o, &carl_a, "read") && !can(&s, &o, &carl_a, "call") && !can(&s, &o, &carl_a, "members.approve"));
    // the owner role cannot be edited or removed; defaults cannot be removed
    let owner_r = s.find_role(&o.id, "owner").unwrap();
    assert!(edit_role(&s, "cli", &Actor::Owner, &o, &owner_r, None, None, Some(&[])).is_err());
    assert!(remove_role(&s, "cli", &Actor::Owner, &o, &owner_r).is_err());
    let user = s.find_role(&o.id, "user").unwrap();
    assert!(remove_role(&s, "cli", &Actor::Owner, &o, &user).unwrap_err().to_string().contains("new members get"));
    // built-in roles can be changed (nothing is locked down but the owner)
    let viewer = edit_role(&s, "cli", &Actor::Owner, &o, &viewer, None, None, Some(&["read".into(), "call".into()])).unwrap();
    assert!(role_has(&viewer, "call"));
    // removing a role moves its members to the lowest one left
    remove_role(&s, "cli", &Actor::Owner, &o, &viewer).unwrap();
    assert_eq!(role_name(&s, &s.member(&carl.id).unwrap().unwrap()), "analyst");
    // members who are not (yet) members can do nothing
    let applied = s.upsert_member(&o.id, "12D3KooWdavedave", "dave", Some(&adminr.id), MemberStatus::Applied, None, "", None).unwrap();
    assert!(!can(&s, &o, &actor_for(&s, &applied).unwrap(), "read"));
}

#[test]
fn guests_see_what_is_public_and_open_orgs_let_them_act() {
    let s = Store::open_in_memory().unwrap();
    let o = create(&s, "cli", "Lab", None, "").unwrap();
    let p = s.create_project("Halving", "").unwrap();
    add_project(&s, "cli", &Actor::Owner, &o, "Halving").unwrap();
    let r = s.find_resource(&o.id, &p.id).unwrap();
    assert!(!visible(&s, &o, &Actor::Guest, &r) && visible(&s, &o, &Actor::Owner, &r));
    let r = set_published(&s, "cli", &Actor::Owner, &o, &r, Published::Public).unwrap();
    assert!(!visible(&s, &o, &Actor::Guest, &r), "a private org shows nothing to guests");
    let o = edit(&s, "cli", &Actor::Owner, &o, &OrgChange { visibility: Some(Visibility::Public), ..Default::default() }).unwrap();
    assert!(visible(&s, &o, &Actor::Guest, &r));
    let r = set_published(&s, "cli", &Actor::Owner, &o, &r, Published::Members).unwrap();
    assert!(!visible(&s, &o, &Actor::Guest, &r));
    assert!(!can(&s, &o, &Actor::Guest, "call"));
    let o = edit(&s, "cli", &Actor::Owner, &o, &OrgChange { access: Some(Access::Open), ..Default::default() }).unwrap();
    assert!(visible(&s, &o, &Actor::Guest, &r));
    assert!(can(&s, &o, &Actor::Guest, "call") && can(&s, &o, &Actor::Guest, "records.write"), "open orgs: guests act as the applicant role");
    assert!(!can(&s, &o, &Actor::Guest, "members.approve") && !can(&s, &o, &Actor::Guest, "publish"), "but never manage");
    let r = set_state(&s, "cli", &Actor::Owner, &o, &r, ResourceState::Archived).unwrap();
    assert!(!visible(&s, &o, &Actor::Guest, &r) && visible(&s, &o, &Actor::Owner, &r));
}

#[test]
fn kicked_members_leave_their_resources_with_the_org() {
    let s = Store::open_in_memory().unwrap();
    let (o, ben) = org_with_member(&s, "user");
    let ben_a = actor_for(&s, &ben).unwrap();
    s.create_journal("Ben's notes", None, crate::store::EntryFormat::Markdown).unwrap();
    s.create_project("Ben's project", "").unwrap();
    let (r1, new) = transfer(&s, "cli", &ben_a, &o, ShareKind::Journal, "Ben's notes", None).unwrap();
    assert!(new && r1.transferred_by.as_deref() == Some(ben.id.as_str()));
    transfer(&s, "cli", &ben_a, &o, ShareKind::Project, "Ben's project", None).unwrap();
    assert!(kick(&s, "cli", &ben_a, &o, &ben, Leftovers::Keep).is_err(), "users cannot kick");
    let (m, res) = kick(&s, "cli", &Actor::Owner, &o, &ben, Leftovers::Archive).unwrap();
    assert_eq!((m.status, res.len()), (MemberStatus::Kicked, 2));
    assert!(s.resources(&o.id).unwrap().iter().all(|r| r.state == ResourceState::Archived && r.transferred_by.as_deref() == Some(ben.id.as_str())));
    assert!(!can(&s, &o, &actor_for(&s, &m).unwrap(), "read"));
    assert!(s.find_journal("Ben's notes").is_ok(), "the org keeps the journal");
    let owner_m = s.member_by_peer(&o.id, &o.owner_peer).unwrap().unwrap();
    assert!(kick(&s, "cli", &Actor::Owner, &o, &owner_m, Leftovers::Keep).is_err());
    // publish instead
    let (o2, carl) = org_with_member(&s, "user");
    let carl_a = actor_for(&s, &carl).unwrap();
    transfer(&s, "cli", &carl_a, &o2, ShareKind::Journal, "Ben's notes", None).unwrap();
    let (_, res) = kick(&s, "cli", &Actor::Owner, &o2, &carl, Leftovers::Publish).unwrap();
    assert_eq!(res[0].published, Published::Public);
    let _ = Joined::clone;
}

#[test]
fn project_members_act_under_project_roles_capped_by_their_org_role() {
    let s = Store::open_in_memory().unwrap();
    let (o, ben) = org_with_member(&s, "user");
    let p = s.create_project("Halving", "").unwrap();
    assert!(add_project_member(&s, "cli", &Actor::Owner, &o, "Halving", &ben, None).is_err(), "only org projects");
    add_project(&s, "cli", &Actor::Owner, &o, "Halving").unwrap();
    add_project_member(&s, "cli", &Actor::Owner, &o, "Halving", &ben, None).unwrap();
    assert_eq!(project_role(&s, &o, &p.id, &ben).unwrap().unwrap().name, "user");
    let viewer = s.find_role(&o.id, "viewer").unwrap();
    add_project_member(&s, "cli", &Actor::Owner, &o, "Halving", &ben, Some(&viewer)).unwrap();
    assert_eq!(project_role(&s, &o, &p.id, &ben).unwrap().unwrap().name, "viewer");
    let admin = s.find_role(&o.id, "admin").unwrap();
    add_project_member(&s, "cli", &Actor::Owner, &o, "Halving", &ben, Some(&admin)).unwrap();
    assert_eq!(project_role(&s, &o, &p.id, &ben).unwrap().unwrap().name, "user", "never above the org role");
    remove_project(&s, "cli", &Actor::Owner, &o, "Halving").unwrap();
    assert!(s.project_members(&o.id, &p.id).unwrap().is_empty());
    assert!(remove(&s, "cli", &actor_for(&s, &ben).unwrap(), &o).is_err());
    remove(&s, "cli", &Actor::Owner, &o).unwrap();
    assert!(s.find_project("Halving").is_ok(), "deleting an org keeps its projects");
}
