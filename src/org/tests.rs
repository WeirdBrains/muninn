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

#[test]
fn the_site_serves_what_is_published_through_the_template() {
    use super::serve::route;
    use crate::store::{Design, FieldOp, Mirror};
    let s = Store::open_in_memory().unwrap();
    let o = create(&s, "cli", "Lab", None, "we <look>").unwrap();
    let j = s.create_journal("notes", None, crate::store::EntryFormat::Markdown).unwrap();
    s.add_entry(&j.id, "Comet", "a bright comet", None).unwrap();
    let (r, _) = transfer(&s, "cli", &Actor::Owner, &o, ShareKind::Journal, "notes", None).unwrap();
    let get = |p: &str| route(&s, "GET", p, b"", None);
    // nothing until hosted, public and published
    assert_eq!(get("/o/lab/").status, 404);
    let o = edit(&s, "cli", &Actor::Owner, &o, &OrgChange { hosted: Some(true), ..Default::default() }).unwrap();
    assert_eq!(get("/o/lab/").status, 404, "private orgs are invisible to guests");
    let o = edit(&s, "cli", &Actor::Owner, &o, &OrgChange { visibility: Some(Visibility::Public), ..Default::default() }).unwrap();
    let page = get("/o/lab/");
    assert_eq!(page.status, 200);
    let html = String::from_utf8(page.body).unwrap();
    assert!(html.contains("we &lt;look&gt;") && html.contains("Nothing is published"));
    assert_eq!(get(&format!("/o/lab/a/{}.html", r.id)).status, 404);
    let r = set_published(&s, "cli", &Actor::Owner, &o, &r, Published::Public).unwrap();
    assert!(String::from_utf8(get("/").body).unwrap().contains("/o/lab/"));
    assert!(String::from_utf8(get("/o/lab/").body).unwrap().contains(&format!("/o/lab/a/{}.json", r.id)));
    let doc = get(&format!("/o/lab/a/{}.html", r.id));
    assert!(doc.mime.starts_with("text/html") && String::from_utf8_lossy(&doc.body).contains("bright comet"));
    assert_eq!(get(&format!("/o/lab/a/{}.json", r.id)).mime, "application/json");
    assert_eq!(get(&format!("/o/lab/a/{}.mnnp", r.id)).status, 404);
    let api: serde_json::Value = serde_json::from_slice(&get("/o/lab/api/artifacts").body).unwrap();
    assert_eq!(api["artifacts"][0]["name"], "notes");
    // a template with an asset
    let t = save_template(
        &s,
        "cli",
        &Actor::Owner,
        &o,
        "plain",
        "<html><body><h1>{{org.name}}!</h1>{{content}}<img src=\"{{assets}}logo.svg\"></body></html>",
        "",
    )
    .unwrap();
    put_asset(&s, "cli", &Actor::Owner, &o, &t, "logo.svg", b"<svg/>").unwrap();
    assert!(put_asset(&s, "cli", &Actor::Owner, &o, &t, "../x", b"").is_err());
    let o = edit(&s, "cli", &Actor::Owner, &o, &OrgChange { template_id: Some(Some(t.id.clone())), ..Default::default() }).unwrap();
    let wrapped = String::from_utf8(get(&format!("/o/lab/a/{}", r.id)).body).unwrap();
    assert!(wrapped.contains("<h1>Lab!</h1>") && wrapped.contains("bright comet") && wrapped.contains("/o/lab/assets/logo.svg"));
    let logo = get("/o/lab/assets/logo.svg");
    assert_eq!((logo.status, logo.mime.as_str(), logo.body.as_slice()), (200, "image/svg+xml", b"<svg/>".as_slice()));
    // a records endpoint: members only unless open to anyone
    let (m, _) = s.create_model("signals", "", Design::default(), false).unwrap();
    s.edit_model(&m.id, &[FieldOp::Add { spec: "symbol:text".into() }]).unwrap();
    s.apply_model(&m.id, &Default::default()).unwrap();
    add_endpoint(&s, "cli", &Actor::Owner, &o, "signals", EndpointKind::Records, "signals", None, false).unwrap();
    let call = |b: &str| route(&s, "POST", "/o/lab/api/call/signals", b.as_bytes(), None);
    assert_eq!(call(r#"{"op":"query"}"#).status, 403);
    add_endpoint(&s, "cli", &Actor::Owner, &o, "signals", EndpointKind::Records, "signals", Some("records.write"), true).unwrap();
    assert_eq!(call(r#"{"op":"create","values":{"symbol":"BTC"}}"#).status, 200, "the owner opened it to anyone");
    let q: serde_json::Value = serde_json::from_slice(&call(r#"{"op":"query"}"#).body).unwrap();
    assert!(q["records"].as_array().unwrap().len() == 1 && q.to_string().contains("BTC"), "{q}");
    // served code is for callers with code.run only
    let c = s.create_code_space("sma", "", crate::store::Lang::Math, "x = close", crate::store::CodeInput::default()).unwrap();
    s.serve_code(&o.id, &c.id).unwrap();
    let ex: serde_json::Value = serde_json::from_slice(&get("/o/lab/api/executables").body).unwrap();
    assert!(ex["executables"].as_array().unwrap().iter().all(|e| e["language"] == "builtin"), "only built-ins for guests: {ex}");
    assert_eq!(get("/o/lab/proto/Sma.proto").status, 404);
    assert_eq!(get("/o/lab/proto/muninn/code/v1/executable.proto").status, 404);
    assert_eq!(route(&s, "POST", "/o/lab/api/run/sma", b"{}", None).status, 403);
    // a signed member sees members-only artifacts
    let (_, ben) = {
        let r = s.find_role(&o.id, "viewer").unwrap();
        let m = s.upsert_member(&o.id, "12D3KooWbenben", "ben", Some(&r.id), MemberStatus::Member, Some("x-ben"), "", None).unwrap();
        ((), m)
    };
    let r = set_published(&s, "cli", &Actor::Owner, &o, &r, Published::Members).unwrap();
    assert_eq!(get(&format!("/o/lab/a/{}.json", r.id)).status, 404);
    assert_eq!(route(&s, "GET", &format!("/o/lab/a/{}.json", r.id), b"", ben.exchange_id.as_deref()).status, 200);
    // a mirror serves a snapshot (public artifacts only)
    let r = set_published(&s, "cli", &Actor::Owner, &o, &r, Published::Public).unwrap();
    let snap = serde_json::to_vec(&super::serve::snapshot(&s, &o).unwrap()).unwrap();
    let m2 = Store::open_in_memory().unwrap();
    m2.save_mirror(&Mirror { org_id: o.id.clone(), slug: "lab".into(), node_peer: o.owner_peer.clone(), snapshot: snap, fetched_at: 1 }).unwrap();
    let mget = |p: &str| route(&m2, "GET", p, b"", None);
    assert!(String::from_utf8_lossy(&mget(&format!("/o/lab/a/{}", r.id)).body).contains("bright comet"));
    assert_eq!(mget("/o/lab/assets/logo.svg").body, b"<svg/>");
    assert_eq!(route(&m2, "POST", "/o/lab/api/call/signals", b"{}", None).status, 405);
    assert!(String::from_utf8(mget("/").body).unwrap().contains("(mirror)"));
}

#[test]
fn code_permissions_are_levels_and_old_roles_keep_their_reach() {
    let s = Store::open_in_memory().unwrap();
    let (o, ben) = org_with_member(&s, "user");
    let user = actor_for(&s, &ben).unwrap();
    assert!(can(&s, &o, &user, "code.view") && can(&s, &o, &user, "code.run"));
    assert!(!can(&s, &o, &user, "code.edit") && !can(&s, &o, &user, "code.manage"));
    let editor = create_role(&s, "cli", &Actor::Owner, &o, "editor", 40, &["code.edit".into()]).unwrap();
    assert!(role_has(&editor, "code.run") && role_has(&editor, "code.view") && !role_has(&editor, "code.manage"));
    let old = create_role(&s, "cli", &Actor::Owner, &o, "old", 30, &["read".into(), "call".into()]).unwrap();
    assert!(role_has(&old, "code.run") && !role_has(&old, "code.edit"), "roles from before code permissions still run code");
    let viewer = s.find_role(&o.id, "viewer").unwrap();
    assert!(role_has(&viewer, "code.view") && !role_has(&viewer, "code.run"));
    assert!(role_has(&s.find_role(&o.id, "admin").unwrap(), "code.manage"));
    let mut open = o.clone();
    open.access = Access::Open;
    assert!(!can(&s, &open, &Actor::Guest, "code.edit") && !can(&s, &open, &Actor::Guest, "code.manage"), "guests never edit code");
}

#[test]
fn each_node_chooses_the_code_it_serves() {
    let s = Store::open_in_memory().unwrap();
    let (o, ben) = org_with_member(&s, "user");
    let c = s.create_code_space("calc", "", crate::store::Lang::Math, "x = close", Default::default()).unwrap();
    add_endpoint(&s, "cli", &Actor::Owner, &o, "calc", EndpointKind::Code, "calc", None, false).unwrap();
    assert_eq!(served_code(&s, &o).unwrap().len(), 1, "exposing code serves it here");
    let user = actor_for(&s, &ben).unwrap();
    assert!(unserve_code(&s, "cli", &user, &o, "calc").is_err(), "serving needs code.manage");
    unserve_code(&s, "cli", &Actor::Owner, &o, "calc").unwrap();
    let err = call(&s, "cli", &Actor::Owner, &o, "calc", &serde_json::json!({})).unwrap_err().to_string();
    assert!(err.contains("does not serve"), "{err}");
    assert!(unserve_code(&s, "cli", &Actor::Owner, &o, "calc").is_err());
    serve_code(&s, "cli", &Actor::Owner, &o, &c.id).unwrap();
    assert_eq!(s.served_code(&o.id).unwrap(), vec![c.id]);
}

#[test]
fn a_gated_private_org_shows_guests_its_door_only() {
    let s = Store::open_in_memory().unwrap();
    let o = create(&s, "cli", "Lab", None, "we look").unwrap();
    let o = edit(&s, "cli", &Actor::Owner, &o, &OrgChange { hosted: Some(true), ..Default::default() }).unwrap();
    let get = |p: &str| serve::route(&s, "GET", p, b"", None);
    assert_eq!(get("/o/lab/").status, 404, "private and not gated: nothing");
    let o = edit(&s, "cli", &Actor::Owner, &o, &OrgChange { gated: Some(true), ..Default::default() }).unwrap();
    assert!(o.gated && s.org(&o.id).unwrap().unwrap().gated, "kept");
    let land = String::from_utf8(get("/o/lab/").body).unwrap();
    assert!(land.contains("we look") && land.contains("/o/lab/sign-in") && land.contains("/o/lab/join"), "{land}");
    assert!(String::from_utf8(get("/o/lab/join").body).unwrap().contains("join.js"));
    assert!(String::from_utf8(get("/o/x/assets-builtin/join.js").body).unwrap().contains("location.hash"));
    let ex: serde_json::Value = serde_json::from_slice(&get("/o/lab/api/executables").body).unwrap();
    assert_eq!(ex["executables"].as_array().unwrap().len(), builtin::BUILTINS.len(), "{ex}");
    let r = serve::route(&s, "POST", "/o/lab/api/run/membership-apply", b"{}", None);
    assert_eq!(r.status, 200);
    assert!(String::from_utf8_lossy(&r.body).contains("mnnn org apply"));
    assert_eq!(serve::route(&s, "POST", "/o/lab/api/run/other", b"{}", None).status, 404);
    assert_eq!(get("/o/lab/api/org").status, 404);
}

#[test]
fn previews_render_a_template_live_until_they_expire() {
    use super::serve::{preview_path, route};
    let s = Store::open_in_memory().unwrap();
    let o = create(&s, "cli", "Lab", None, "").unwrap();
    let j = s.create_journal("notes", None, crate::store::EntryFormat::Markdown).unwrap();
    s.add_entry(&j.id, "Comet", "x", None).unwrap();
    let (r, _) = transfer(&s, "cli", &Actor::Owner, &o, ShareKind::Journal, "notes", None).unwrap();
    let (r2, _) = {
        let j2 = s.create_journal("secret", None, crate::store::EntryFormat::Markdown).unwrap();
        transfer(&s, "cli", &Actor::Owner, &o, ShareKind::Journal, &j2.id, None).unwrap()
    };
    set_published(&s, "cli", &Actor::Owner, &o, &r, Published::Public).unwrap();
    set_published(&s, "cli", &Actor::Owner, &o, &r2, Published::Members).unwrap();
    let t = save_template(&s, "cli", &Actor::Owner, &o, "draft", "<i>{{content}}</i><img src=\"{{assets}}a.svg\">", "").unwrap();
    put_asset(&s, "cli", &Actor::Owner, &o, &t, "a.svg", b"<svg/>").unwrap();
    // a viewer may not make previews
    let (_, ben) = tests_member(&s, &o);
    assert!(preview(&s, "cli", &actor_for(&s, &ben).unwrap(), &o, &t).is_err());
    let tok = preview(&s, "cli", &Actor::Owner, &o, &t).unwrap();
    let get = |p: &str| route(&s, "GET", p, b"", None);
    let base = preview_path("lab", &tok);
    // works before the org is hosted or uses the template; public artifacts only
    let page = String::from_utf8(get(&base).body).unwrap();
    assert!(page.starts_with("<i>") && page.contains("notes") && !page.contains("secret"));
    assert!(page.contains(&format!("{base}assets/a.svg")));
    assert_eq!(get(&format!("{base}assets/a.svg")).body, b"<svg/>");
    // live: an edit shows on the next request
    save_template(&s, "cli", &Actor::Owner, &o, "draft", "<b>{{content}}</b>", "").unwrap();
    assert!(String::from_utf8(get(&base).body).unwrap().starts_with("<b>"));
    // wrong slug or token: nothing
    assert_eq!(get(&preview_path("other", &tok)).status, 404);
    assert_eq!(get(&preview_path("lab", "nope")).status, 404);
    let old = s.create_preview(&t.id, -1).unwrap();
    assert_eq!(get(&preview_path("lab", &old)).status, 404);
}

fn tests_member(s: &Store, o: &Org) -> (Org, Member) {
    let r = s.find_role(&o.id, "viewer").unwrap();
    let m = s.upsert_member(&o.id, "12D3KooWviewerviewer", "vic", Some(&r.id), MemberStatus::Member, None, "", Some("owner")).unwrap();
    (o.clone(), m)
}
