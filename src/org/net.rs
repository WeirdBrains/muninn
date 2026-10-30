//! Organizations across nodes. Joining is a key exchange with the org's node, scoped to the
//! org: an applicant offers (`as: apply`) and an owner or admin approves by answering, or
//! the org's node invites (`as: invite`) and the invitee answers. Members then reach the
//! org through sealed [`OrgReq`]s over that exchange, and act with their role.
use super::{Actor, Leftovers, OrgCard};
use crate::error::{Error, Result};
use crate::net::exchange::{self, Offer};
use crate::net::node::Node;
use crate::net::proto::{Item, Req, Resp, Wire, WireResp};
use crate::net::{client, service};
use crate::store::{
    Exchange, ExchangeStatus, Joined, Member, MemberStatus, NewAction, Org, OrgChange, Published, ResourceState, Role, ShareKind, Store, now_ms,
};
use libp2p::PeerId;
use libp2p::identity::Keypair;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// What a member asks the org's node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrgReq {
    /// The org, my membership, role and permissions.
    Status,
    Members,
    /// The org's resources I can see.
    Resources,
    Fetch {
        resource: String,
        format: Option<String>,
    },
    /// Give the org one of my things.
    Transfer {
        item: Item,
    },
    Call {
        endpoint: String,
        input: Value,
    },
    Manage {
        op: ManageOp,
    },
    /// The org's public site (for a mirror; needs `host`).
    Snapshot,
    Leave,
}

/// Management by a member with the permission for it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ManageOp {
    Approve { member: String, role: Option<String> },
    Reject { member: String },
    Kick { member: String, resources: Leftovers },
    Role { member: String, role: String },
    Defaults { apply: Option<String>, invite: Option<String> },
    Publish { resource: String, to: Published },
    State { resource: String, archived: bool },
}

fn record(s: &Store, origin: &str, kind: &str, summary: String, params: Value) {
    s.note_action(&NewAction::new(kind, origin, summary).params(params));
}

/// The scope of a membership exchange.
pub fn scope(o: &OrgCard, how: &str, note: &str) -> Value {
    json!({ "kind": "organization", "org": o.id, "name": o.name, "slug": o.slug, "as": how, "note": note })
}

fn scope_of(v: &Value) -> Option<(String, String)> {
    if v.get("kind")?.as_str()? != "organization" {
        return None;
    }
    Some((v.get("org")?.as_str()?.to_string(), v.get("as")?.as_str()?.to_string()))
}

fn sv(v: &Value, k: &str) -> String {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
}

// ---------- the org's node: unsealed ----------

/// Public orgs, for `Wire::Orgs`.
pub fn listed(s: &Store) -> Result<Vec<OrgCard>> {
    Ok(s.orgs()?.iter().filter(|o| o.visibility == crate::store::Visibility::Public).map(|o| super::card(s, o)).collect())
}

/// One org by its exact id or slug (no prefixes: private orgs are found only by name).
pub fn info(s: &Store, key: &str) -> Result<OrgCard> {
    let k = key.trim().to_ascii_lowercase();
    s.orgs()?
        .iter()
        .find(|o| o.id == k || o.slug == k)
        .map(|o| super::card(s, o))
        .ok_or_else(|| Error::invalid("organization", format!("no organization `{key}` on this node")))
}

/// Before an offer is stored: an application must be for an org of this node.
pub fn check_application(s: &Store, o: &Offer) -> Result<()> {
    let Some((org, how)) = scope_of(&o.scope) else { return Ok(()) };
    if how != "apply" {
        return Ok(());
    }
    let org = s.org(&org)?.ok_or_else(|| Error::invalid("organization", "no such organization on this node"))?;
    if let Some(m) = s.member_by_peer(&org.id, &o.peer)?
        && matches!(m.status, MemberStatus::Member | MemberStatus::Applied)
    {
        return Err(Error::invalid(
            "organization",
            format!("already {} of {}", if m.status == MemberStatus::Member { "a member" } else { "applying" }, org.name),
        ));
    }
    Ok(())
}

/// An application arrived: the applicant waits for an owner or admin.
pub fn applied(s: &Store, o: &Offer, x: &Exchange) -> Result<Option<String>> {
    let Some((org, how)) = scope_of(&o.scope) else { return Ok(None) };
    if how != "apply" {
        return Ok(None);
    }
    let Some(org) = s.org(&org)? else { return Ok(None) };
    let note: String = sv(&o.scope, "note").chars().take(500).collect();
    let m = s.upsert_member(&org.id, &o.peer, &o.label, None, MemberStatus::Applied, Some(&x.id), &note, None)?;
    record(
        s,
        "network",
        "org.member.apply",
        format!("{} applied to join {}", m.title(), org.name),
        json!({ "org": org.id, "member": m.id, "peer": m.peer_id, "exchange": x.id }),
    );
    Ok(Some(format!("application to {} received; an owner or admin decides", org.name)))
}

/// Has the applicant been decided on? (Answers are public, signed tokens.)
pub fn poll(s: &Store, from: &PeerId, exchange: &str) -> Result<(String, Option<String>)> {
    let x = s.exchange(exchange)?.ok_or_else(|| Error::invalid("exchange", "unknown application"))?;
    if x.peer_id.as_deref() != Some(&from.to_string()) {
        return Err(Error::invalid("exchange", "unknown application"));
    }
    let m = s.member_by_exchange(&x.id)?;
    Ok(match m.map(|m| m.status) {
        Some(MemberStatus::Member) if x.status.active() => ("member".into(), x.answer.clone()),
        Some(MemberStatus::Rejected) => ("rejected".into(), None),
        Some(MemberStatus::Kicked) => ("kicked".into(), None),
        Some(MemberStatus::Left) => ("left".into(), None),
        _ => ("applied".into(), None),
    })
}

/// An exchange completed (either side): settle the membership it is for.
pub fn answered(s: &Store, x: &Exchange) -> Result<()> {
    let Some((org_id, how)) = scope_of(&x.scope) else { return Ok(()) };
    let peer = x.peer_id.clone().unwrap_or_default();
    match (how.as_str(), x.role.as_str()) {
        // the applicant: approved
        ("apply", "offerer") => {
            if let Some(mut j) = s.joined_by_exchange(&x.id)? {
                j.status = MemberStatus::Member;
                j.updated_at = now_ms();
                s.save_joined(&j)?;
                record(s, "network", "org.joined", format!("joined {} (approved)", j.name), json!({ "org": j.org_id, "exchange": x.id }));
            }
        }
        // the org's node, answering an application: approve() settles the member
        ("apply", _) => {}
        // the org's node: an invitation was taken
        ("invite", "offerer") => {
            if let Some(m) = s.member_by_exchange(&x.id)?
                && m.status == MemberStatus::Invited
            {
                let m = s.set_member_peer(&m.id, &peer, &x.peer_label)?;
                let m = s.set_member(&m.id, Some(MemberStatus::Member), None, None)?;
                let org = s.org(&m.org_id)?.map(|o| o.name).unwrap_or_default();
                record(
                    s,
                    "network",
                    "org.member.join",
                    format!("{} accepted the invitation to {org}", m.title()),
                    json!({ "org": m.org_id, "member": m.id, "peer": peer }),
                );
            }
        }
        // the invitee
        _ => {
            let now = now_ms();
            let j = Joined {
                org_id: org_id.clone(),
                name: sv(&x.scope, "name"),
                slug: sv(&x.scope, "slug"),
                node_peer: peer,
                exchange_id: Some(x.id.clone()),
                status: MemberStatus::Member,
                role: String::new(),
                perms: vec![],
                info: Value::Null,
                created_at: now,
                updated_at: now,
            };
            s.save_joined(&j)?;
            record(s, "network", "org.joined", format!("joined {} (invited)", j.name), json!({ "org": j.org_id, "exchange": x.id }));
        }
    }
    Ok(())
}

/// The other side ended an exchange: a member left, or we were removed.
pub fn ended(s: &Store, x: &Exchange) -> Result<()> {
    if let Some(m) = s.member_by_exchange(&x.id)?
        && m.status == MemberStatus::Member
    {
        let m = s.set_member(&m.id, Some(MemberStatus::Left), None, None)?;
        record(s, "network", "org.member.leave", format!("{} left", m.title()), json!({ "org": m.org_id, "member": m.id }));
    }
    if let Some(mut j) = s.joined_by_exchange(&x.id)?
        && matches!(j.status, MemberStatus::Member | MemberStatus::Applied)
    {
        j.status = if j.status == MemberStatus::Applied { MemberStatus::Rejected } else { MemberStatus::Kicked };
        j.updated_at = now_ms();
        s.save_joined(&j)?;
        record(s, "network", "org.removed", format!("{} ended our membership", j.name), json!({ "org": j.org_id }));
    }
    Ok(())
}

// ---------- the org's node: deciding ----------

/// This node's label and addresses for answers it makes.
fn me(s: &Store) -> (String, Vec<String>) {
    (crate::net::local::my_label(s, &crate::prefs::NetPrefs::default()), client::my_addrs(s, None))
}

/// Approve an application: answer the applicant's offer (delivering it when a node runs;
/// the applicant can also pick it up with `mnnn org sync`) and give them a role.
#[allow(clippy::too_many_arguments)]
pub fn approve(
    s: &Store,
    node: Option<&Node>,
    kp: &Keypair,
    a: &Actor,
    org: &Org,
    m: &Member,
    role: Option<&Role>,
    origin: &str,
) -> Result<(Member, Option<std::result::Result<String, String>>)> {
    super::require(s, org, a, "members.approve")?;
    if m.status != MemberStatus::Applied {
        return Err(Error::invalid("member", format!("{} is {}, not applying", m.title(), m.status.as_str())));
    }
    let role = match role {
        Some(r) => r.clone(),
        None => super::guest_role(s, org).ok_or_else(|| Error::invalid("role", "the org has no role for new members; give --role"))?,
    };
    super::check_grant(org, a, &role)?;
    let xid = m.exchange_id.clone().ok_or_else(|| Error::invalid("member", "the application has no key exchange"))?;
    let (label, addrs) = me(s);
    let acc = client::accept(s, node, kp, &xid, "", &label, addrs, origin)?;
    let m = s.set_member(&m.id, Some(MemberStatus::Member), Some(&role.id), a.member_id().or(Some("owner")))?;
    record(
        s,
        origin,
        "org.member.approve",
        format!("{} approved as {} in {} by {}", m.title(), role.name, org.name, a.who()),
        json!({ "org": org.id, "member": m.id, "role": role.id, "exchange": xid, "by": a.who(), "delivered": acc.delivered.as_ref().map(|d| d.is_ok()) }),
    );
    Ok((m, acc.delivered))
}

pub fn reject(s: &Store, origin: &str, a: &Actor, org: &Org, m: &Member) -> Result<Member> {
    super::require(s, org, a, "members.approve")?;
    if m.status != MemberStatus::Applied {
        return Err(Error::invalid("member", format!("{} is {}, not applying", m.title(), m.status.as_str())));
    }
    let m = s.set_member(&m.id, Some(MemberStatus::Rejected), None, a.member_id().or(Some("owner")))?;
    if let Some(x) = &m.exchange_id {
        let _ = s.set_exchange_status(x, ExchangeStatus::Revoked, Some("me"));
    }
    record(
        s,
        origin,
        "org.member.reject",
        format!("{}'s application to {} rejected by {}", m.title(), org.name, a.who()),
        json!({ "org": org.id, "member": m.id, "by": a.who() }),
    );
    Ok(m)
}

/// Invite a node: an offer scoped to the org (delivered when `to` is given and a node runs).
#[allow(clippy::too_many_arguments)]
pub fn invite(
    s: &Store,
    node: Option<&Node>,
    kp: &Keypair,
    a: &Actor,
    org: &Org,
    role: Option<&Role>,
    to: Option<&str>,
    label: &str,
    relays: &[String],
    origin: &str,
) -> Result<(Member, client::Created)> {
    super::require(s, org, a, "members.invite")?;
    let role = match role {
        Some(r) => r.clone(),
        None => org.invite_role.as_deref().and_then(|r| s.role(r).ok().flatten()).ok_or_else(|| Error::invalid("role", "give --role"))?,
    };
    super::check_grant(org, a, &role)?;
    let (my_label, addrs) = me(s);
    let addrs = node.map(|n| client::my_addrs(s, Some(n))).unwrap_or(addrs);
    let c = client::create_scoped(
        s,
        node,
        kp,
        label,
        &my_label,
        addrs,
        to,
        client::DEFAULT_TTL_MS * 7,
        relays,
        scope(&super::card(s, org), "invite", ""),
        origin,
    )?;
    let peer = c.exchange.peer_id.clone().unwrap_or_else(|| format!("invite:{}", c.exchange.id));
    let m =
        s.upsert_member(&org.id, &peer, label, Some(&role.id), MemberStatus::Invited, Some(&c.exchange.id), "", a.member_id().or(Some("owner")))?;
    record(
        s,
        origin,
        "org.member.invite",
        format!("invitation to {} as {} made by {}", org.name, role.name, a.who()),
        json!({ "org": org.id, "member": m.id, "role": role.id, "exchange": c.exchange.id, "by": a.who() }),
    );
    Ok((m, c))
}

/// Remove a member: what they gave the org stays (kept, archived or published), the
/// exchange ends and their node is told when it can be reached.
#[allow(clippy::too_many_arguments)]
pub fn kick(
    s: &Store,
    node: Option<&Node>,
    a: &Actor,
    org: &Org,
    m: &Member,
    leftovers: Leftovers,
    relays: &[String],
    origin: &str,
) -> Result<Member> {
    let (m, _) = super::kick(s, origin, a, org, m, leftovers)?;
    if let Some(x) = &m.exchange_id
        && let Ok(x) = s.find_exchange(x)
    {
        let _ = client::revoke(s, node, &x.id, relays, origin);
    }
    Ok(m)
}

// ---------- the org's node: members' requests ----------

fn body(v: Value) -> Resp {
    Resp::Org { body: v }
}

/// A sealed request from a member.
pub fn serve(s: &mut Store, x: &Exchange, from: &PeerId, org_key: &str, req: OrgReq) -> Result<Resp> {
    let org = s.find_org(org_key)?;
    let m = s.member_by_exchange(&x.id)?.filter(|m| m.org_id == org.id && m.peer_id == from.to_string());
    let m = m.ok_or_else(|| Error::invalid("organization", format!("this key exchange is not a membership of {}", org.name)))?;
    let a = super::actor_for(s, &m).unwrap_or(Actor::Guest);
    let role = super::role_name(s, &m);
    if !matches!(req, OrgReq::Status) && m.status != MemberStatus::Member {
        return Err(Error::invalid("organization", format!("you are {} of {}", m.status.as_str(), org.name)));
    }
    let res = |s: &Store, key: &str| s.find_resource(&org.id, key);
    Ok(match req {
        OrgReq::Status => {
            let perms = match &a {
                Actor::Member { role, .. } => role.perms.clone(),
                _ => vec![],
            };
            body(json!({ "org": super::card(s, &org), "member": m.id, "status": m.status, "role": role, "perms": perms }))
        }
        OrgReq::Members => {
            super::require(s, &org, &a, "members.list")?;
            let v: Vec<Value> = s
                .members(&org.id)?
                .iter()
                .filter(|x| x.status == MemberStatus::Member || super::can(s, &org, &a, "members.approve"))
                .map(|x| json!({ "id": x.id, "label": x.title(), "peer": x.peer_id, "role": super::role_name(s, x), "status": x.status, "note": x.note }))
                .collect();
            body(json!({ "members": v }))
        }
        OrgReq::Resources => {
            let v: Vec<Value> = s.resources(&org.id)?.iter().filter(|r| super::visible(s, &org, &a, r)).map(|r| json!(r)).collect();
            body(json!({ "resources": v }))
        }
        OrgReq::Fetch { resource, format } => {
            let r = res(s, &resource)?;
            if !super::visible(s, &org, &a, &r) {
                return Err(Error::invalid("resource", "not yours to see"));
            }
            let it = service::item_of(s, r.kind, &r.ref_id, &r.name, format.as_deref())?;
            record(
                s,
                "network",
                "org.serve",
                format!("sent {} {} of {} to {}", r.kind.label(), r.name, org.name, m.title()),
                json!({ "org": org.id, "resource": r.id, "member": m.id, "bytes": it.data.len() }),
            );
            Resp::Item { item: it }
        }
        OrgReq::Transfer { item } => {
            super::require(s, &org, &a, "resources.transfer")?;
            let (id, name, made) = client::import_item_as(s, &item, &m.title(), "network")?;
            let (r, _) = s.add_resource(&org.id, item.kind, &id, &name, Some(&m.id))?;
            record(
                s,
                "network",
                "org.resource.add",
                format!("{} gave {made} to {}", m.title(), org.name),
                json!({ "org": org.id, "resource": r.id, "kind": item.kind, "item": id, "by": a.who() }),
            );
            body(json!({ "resource": r }))
        }
        OrgReq::Call { endpoint, input } => body(super::call(s, "network", &a, &org, &endpoint, &input)?),
        OrgReq::Snapshot => {
            super::require(s, &org, &a, "host")?;
            body(super::serve::snapshot(s, &org)?)
        }
        OrgReq::Leave => {
            let m = s.set_member(&m.id, Some(MemberStatus::Left), None, None)?;
            let _ = s.set_exchange_status(&x.id, ExchangeStatus::Revoked, Some("peer"));
            record(s, "network", "org.member.leave", format!("{} left {}", m.title(), org.name), json!({ "org": org.id, "member": m.id }));
            body(json!({ "left": org.name }))
        }
        OrgReq::Manage { op } => {
            let find = |k: &str| s.find_member(&org.id, k);
            let role_of = |k: &str| s.find_role(&org.id, k);
            match op {
                ManageOp::Approve { member, role } => {
                    let kp = crate::net::identity::load_or_create(s)?;
                    let r = role.as_deref().map(role_of).transpose()?;
                    let (m2, _) = approve(s, None, &kp, &a, &org, &find(&member)?, r.as_ref(), "network")?;
                    body(json!({ "member": m2.id, "status": m2.status }))
                }
                ManageOp::Reject { member } => body(json!({ "member": reject(s, "network", &a, &org, &find(&member)?)?.id })),
                ManageOp::Kick { member, resources } => {
                    let m2 = kick(s, None, &a, &org, &find(&member)?, resources, &[], "network")?;
                    body(json!({ "member": m2.id, "status": m2.status }))
                }
                ManageOp::Role { member, role } => {
                    let m2 = super::set_role(s, "network", &a, &org, &find(&member)?, &role_of(&role)?)?;
                    body(json!({ "member": m2.id, "role": role }))
                }
                ManageOp::Defaults { apply, invite } => {
                    let c = OrgChange {
                        apply_role: apply.as_deref().map(|r| role_of(r).map(|r| r.id)).transpose()?,
                        invite_role: invite.as_deref().map(|r| role_of(r).map(|r| r.id)).transpose()?,
                        ..Default::default()
                    };
                    super::require(s, &org, &a, "members.defaults")?;
                    let o = defaults(s, "network", &a, &org, &c)?;
                    body(json!({ "apply_role": o.apply_role, "invite_role": o.invite_role }))
                }
                ManageOp::Publish { resource, to } => {
                    body(json!({ "resource": super::set_published(s, "network", &a, &org, &res(s, &resource)?, to)? }))
                }
                ManageOp::State { resource, archived } => {
                    let st = if archived { ResourceState::Archived } else { ResourceState::Active };
                    body(json!({ "resource": super::set_state(s, "network", &a, &org, &res(s, &resource)?, st)? }))
                }
            }
        }
    })
}

/// Change only the default roles (`members.defaults`, without `org.edit`).
pub fn defaults(s: &Store, origin: &str, a: &Actor, org: &Org, c: &OrgChange) -> Result<Org> {
    super::require(s, org, a, "members.defaults")?;
    for r in [&c.apply_role, &c.invite_role].into_iter().flatten() {
        let role = s.role(r)?.filter(|x| x.org_id == org.id).ok_or_else(|| Error::invalid("role", "not a role of this org"))?;
        super::check_grant(org, a, &role)?;
    }
    let o = s.update_org(&org.id, &OrgChange { apply_role: c.apply_role.clone(), invite_role: c.invite_role.clone(), ..Default::default() })?;
    record(
        s,
        origin,
        "org.defaults",
        format!("default roles of {} changed by {}", o.name, a.who()),
        json!({ "org": o.id, "apply_role": o.apply_role, "invite_role": o.invite_role }),
    );
    Ok(o)
}

// ---------- a member's node ----------

/// Ask a node for its public orgs (or one by id or slug).
pub fn browse(s: &Store, node: &Node, target: &str, org: Option<&str>, relays: &[String]) -> Result<Vec<OrgCard>> {
    let (p, addrs) = client::resolve(s, target, relays)?;
    let req = match org {
        Some(o) => Wire::OrgInfo { org: o.to_string() },
        None => Wire::Orgs,
    };
    match node.request(p, &addrs, req)? {
        WireResp::Orgs { orgs } => Ok(orgs),
        WireResp::Error { msg } => Err(Error::Other(format!("peer: {msg}"))),
        _ => Err(Error::Other("peer: this node does not keep organizations (an older Muninn?)".into())),
    }
}

/// Apply to an org on another node: an org-scoped offer delivered to it.
#[allow(clippy::too_many_arguments)]
pub fn apply(
    s: &Store,
    node: &Node,
    kp: &Keypair,
    target: &str,
    org: &str,
    note: &str,
    my_label: &str,
    relays: &[String],
    origin: &str,
) -> Result<Joined> {
    let card = browse(s, node, target, Some(org), relays)?.into_iter().next().ok_or_else(|| Error::invalid("organization", "not found"))?;
    if let Some(j) = s.joined_org(&card.id)?
        && j.status == MemberStatus::Member
    {
        return Err(Error::invalid("organization", format!("already a member of {}", j.name)));
    }
    let (p, _) = client::resolve(s, target, relays)?;
    let c = client::create_scoped(
        s,
        Some(node),
        kp,
        &format!("org {}", card.name),
        my_label,
        client::my_addrs(s, Some(node)),
        Some(&p.to_string()),
        client::DEFAULT_TTL_MS * 7,
        relays,
        scope(&card, "apply", note),
        origin,
    )?;
    if let Some(Err(e)) = &c.delivered {
        let _ = s.set_exchange_status(&c.exchange.id, ExchangeStatus::Revoked, Some("me"));
        return Err(Error::Other(format!("{}: {e}", card.name)));
    }
    let now = now_ms();
    let j = Joined {
        org_id: card.id.clone(),
        name: card.name.clone(),
        slug: card.slug.clone(),
        node_peer: p.to_string(),
        exchange_id: Some(c.exchange.id.clone()),
        status: MemberStatus::Applied,
        role: String::new(),
        perms: vec![],
        info: json!({ "org": card }),
        created_at: now,
        updated_at: now,
    };
    s.save_joined(&j)?;
    record(
        s,
        origin,
        "org.apply",
        format!("applied to join {}", card.name),
        json!({ "org": card.id, "peer": j.node_peer, "exchange": c.exchange.id }),
    );
    Ok(j)
}

/// Bring a joined org up to date: pick up a decision on an application, then the role.
pub fn sync(s: &Store, node: &Node, j: &Joined, relays: &[String], origin: &str) -> Result<Joined> {
    let xid = j.exchange_id.clone().ok_or_else(|| Error::invalid("organization", "no key exchange"))?;
    if j.status == MemberStatus::Applied {
        let p: PeerId = j.node_peer.parse().map_err(|_| Error::invalid("peer", "bad peer id"))?;
        let addrs = client::peer_addrs(s, &j.node_peer, relays);
        match node.request(p, &addrs, Wire::Poll { exchange: xid.clone() })? {
            WireResp::Poll { status, answer } => match (status.as_str(), answer) {
                ("member", Some(tok)) => {
                    if s.exchange(&xid)?.is_some_and(|x| x.status == ExchangeStatus::Offered) {
                        let exchange::Token::Answer(a) = exchange::parse_token(&tok)? else {
                            return Err(Error::Other("peer: not an answer".into()));
                        };
                        let x = service::complete_offer(s, &a, &tok)?;
                        record(
                            s,
                            origin,
                            "net.exchange.complete",
                            format!("key exchange with {} complete", x.title()),
                            json!({ "exchange": x.id, "peer": x.peer_id }),
                        );
                    }
                }
                ("applied", _) => return s.joined_org(&j.org_id)?.ok_or_else(|| Error::Other("joined org vanished".into())),
                (st, _) => {
                    let mut j = j.clone();
                    j.status = MemberStatus::parse(st);
                    j.updated_at = now_ms();
                    s.save_joined(&j)?;
                    let _ = s.set_exchange_status(&xid, ExchangeStatus::Revoked, Some("peer"));
                    return Ok(j);
                }
            },
            WireResp::Error { msg } => return Err(Error::Other(format!("{}: {msg}", j.name))),
            _ => return Err(Error::Other("peer: unexpected answer".into())),
        }
    }
    let mut j = s.joined_org(&j.org_id)?.ok_or_else(|| Error::Other("joined org vanished".into()))?;
    if j.status != MemberStatus::Member {
        return Ok(j);
    }
    let v = request(s, node, &j, OrgReq::Status, relays)?;
    j.role = sv(&v, "role");
    j.perms = v.get("perms").and_then(|p| serde_json::from_value(p.clone()).ok()).unwrap_or_default();
    if let Some(o) = v.get("org") {
        j.name = sv(o, "name");
        j.slug = sv(o, "slug");
    }
    j.status = v.get("status").and_then(|x| x.as_str()).map(MemberStatus::parse).unwrap_or(j.status);
    j.info = v;
    j.updated_at = now_ms();
    s.save_joined(&j)?;
    Ok(j)
}

/// A sealed request to a joined org.
pub fn request(s: &Store, node: &Node, j: &Joined, req: OrgReq, relays: &[String]) -> Result<Value> {
    let xid = j.exchange_id.clone().ok_or_else(|| Error::invalid("organization", "no key exchange"))?;
    let x = s.find_exchange(&xid)?;
    match client::send(s, node, &x, Req::Org { org: j.org_id.clone(), req }, relays)? {
        Resp::Org { body } => Ok(body),
        Resp::Item { item } => Ok(json!({ "item": item })),
        _ => Err(Error::Other("peer: unexpected answer".into())),
    }
}

/// Fetch one of a joined org's resources.
pub fn fetch(s: &Store, node: &Node, j: &Joined, resource: &str, format: Option<&str>, relays: &[String]) -> Result<Item> {
    let xid = j.exchange_id.clone().ok_or_else(|| Error::invalid("organization", "no key exchange"))?;
    let x = s.find_exchange(&xid)?;
    match client::send(
        s,
        node,
        &x,
        Req::Org { org: j.org_id.clone(), req: OrgReq::Fetch { resource: resource.into(), format: format.map(str::to_string) } },
        relays,
    )? {
        Resp::Item { item } => Ok(item),
        _ => Err(Error::Other("peer: unexpected answer".into())),
    }
}

/// Give one of this node's things to a joined org.
pub fn transfer(s: &Store, node: &Node, j: &Joined, kind: ShareKind, key: &str, relays: &[String], origin: &str) -> Result<Value> {
    let (id, name) = client::find_item(s, kind, key)?;
    let format = match kind {
        ShareKind::Project => Some("mnnp"),
        _ => Some("json"),
    };
    let item = service::item_of(s, kind, &id, &name, format)?;
    let v = request(s, node, j, OrgReq::Transfer { item }, relays)?;
    record(s, origin, "org.transfer", format!("gave {} {name} to {}", kind.label(), j.name), json!({ "org": j.org_id, "kind": kind, "item": id }));
    Ok(v)
}

/// Leave a joined org (the org's node is told; the exchange ends here either way).
pub fn leave(s: &Store, node: Option<&Node>, j: &Joined, relays: &[String], origin: &str) -> Result<Joined> {
    let told = match node {
        Some(n) if j.status == MemberStatus::Member => request(s, n, j, OrgReq::Leave, relays).map(|_| ()).map_err(|e| e.to_string()),
        _ => Err("not told".into()),
    };
    if let Some(x) = &j.exchange_id {
        let _ = s.set_exchange_status(x, ExchangeStatus::Revoked, Some("me"));
    }
    let mut j = j.clone();
    j.status = MemberStatus::Left;
    j.updated_at = now_ms();
    s.save_joined(&j)?;
    record(s, origin, "org.leave", format!("left {}", j.name), json!({ "org": j.org_id, "told": told.is_ok() }));
    Ok(j)
}

/// Copy a joined org's public site to this node, to serve it with `mnnn org host start`
/// (needs the `host` permission). Run it again to refresh the copy.
pub fn mirror(s: &Store, node: &Node, j: &Joined, relays: &[String], origin: &str) -> Result<crate::store::Mirror> {
    let body = request(s, node, j, OrgReq::Snapshot, relays)?;
    let snap: super::serve::Snapshot = serde_json::from_value(body).map_err(|e| Error::Other(format!("peer: bad snapshot: {e}")))?;
    let m = crate::store::Mirror {
        org_id: j.org_id.clone(),
        slug: snap.org.slug.clone(),
        node_peer: j.node_peer.clone(),
        snapshot: serde_json::to_vec(&snap)?,
        fetched_at: now_ms(),
    };
    s.save_mirror(&m)?;
    record(
        s,
        origin,
        "org.mirror",
        format!("mirrored {} ({} artifacts)", j.name, snap.artifacts.len()),
        json!({ "org": j.org_id, "slug": m.slug, "artifacts": snap.artifacts.len() }),
    );
    Ok(m)
}
