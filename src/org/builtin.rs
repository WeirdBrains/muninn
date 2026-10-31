//! Built-in global executables (2.13): shipped with Muninn and versioned with it, served by
//! every hosted org, and callable by anyone (they are how a guest gets in). Each has a
//! readable contract (`mnnn code builtin show NAME`). An org overrides one by serving a code
//! space of the same name (`mnnn code builtin override NAME`), which then runs instead as
//! ordinary code: it needs `code.run` and approval like any served space.
use super::Actor;
use super::exec::{self, Executable};
use crate::error::{Error, Result};
use crate::store::{CodeScope, MemberStatus, Org, Store, now_ms};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize)]
pub struct Builtin {
    pub name: &'static str,
    pub summary: &'static str,
    /// The contract shown to people (markdown).
    pub contract: &'static str,
}

pub const BUILTINS: &[Builtin] = &[
    Builtin { name: "invite-redeem", summary: "check an invite link for this org", contract: include_str!("../../builtin/invite-redeem.md") },
    Builtin { name: "membership-apply", summary: "how to apply to this org", contract: include_str!("../../builtin/membership-apply.md") },
];

pub fn find(name: &str) -> Option<&'static Builtin> {
    let k = name.trim();
    BUILTINS.iter().find(|b| b.name.eq_ignore_ascii_case(k) || exec::pascal(b.name) == k)
}

/// Its version: Muninn's version and a hash of its contract.
pub fn version_hash(b: &Builtin) -> String {
    hex::encode(Sha256::digest(format!("{}\n{}", env!("CARGO_PKG_VERSION"), b.contract)))
}

/// Is a built-in overridden in this org (a served space of the same name)?
pub fn overridden(s: &Store, org: &Org, b: &Builtin) -> bool {
    exec::find(s, org, b.name).is_ok_and(|(c, _)| c.name.eq_ignore_ascii_case(b.name))
}

pub fn describe(org: &Org, b: &Builtin) -> Executable {
    let svc = exec::pascal(b.name);
    Executable {
        id: format!("builtin:{}", b.name),
        name: b.name.into(),
        service: format!("{}.{svc}", exec::package(org)),
        language: "builtin".into(),
        version_hash: version_hash(b),
        scope: CodeScope::Global,
        inputs: vec![],
        output: "json".into(),
        proto: format!("/o/{}/proto/{svc}.proto", org.slug),
    }
}

/// The built-ins as executables of an org.
pub fn executables(org: &Org) -> Vec<Executable> {
    BUILTINS.iter().map(|b| describe(org, b)).collect()
}

/// Run a built-in (or the org's override of it) for anyone.
pub fn run(s: &Store, origin: &str, a: &Actor, org: &Org, name: &str, input: &Value) -> Result<Value> {
    let b = find(name).ok_or_else(|| Error::invalid("executable", format!("no built-in `{name}`")))?;
    if overridden(s, org, b) {
        return exec::run(s, origin, a, org, b.name, &exec::Input::from_json(input)?);
    }
    let out = match b.name {
        "invite-redeem" => invite_redeem(s, org, input)?,
        _ => membership_apply(s, org)?,
    };
    s.note_action(
        &crate::store::NewAction::new("org.run", origin, format!("{} ran built-in {} in {}", a.who(), b.name, org.name))
            .params(json!({ "org": org.id, "code": format!("builtin:{}", b.name), "by": a.who() })),
    );
    Ok(json!({ "status": "ok", "builtin": b.name, "version": version_hash(b), "result": out }))
}

fn invite_redeem(s: &Store, org: &Org, input: &Value) -> Result<Value> {
    let link = input.get("link").and_then(|v| v.as_str()).ok_or_else(|| Error::invalid("input", "give `link`: the invite link"))?;
    let no = |why: &str| json!({ "valid": false, "org": org.name, "why": why });
    let Ok(crate::net::exchange::Token::Offer(o)) = crate::net::exchange::parse_token(link) else { return Ok(no("not an invite link")) };
    let for_org =
        o.scope.get("org").and_then(|v| v.as_str()) == Some(org.id.as_str()) && o.scope.get("as").and_then(|v| v.as_str()) == Some("invite");
    if !for_org {
        return Ok(no("this link is for another organization"));
    }
    if o.expires < now_ms() {
        return Ok(no("this invitation expired; ask for a new one"));
    }
    match s.member_by_exchange(&o.id)? {
        Some(m) if m.org_id == org.id && m.status == MemberStatus::Invited => Ok(json!({
            "valid": true,
            "org": org.name,
            "role": super::role_name(s, &m),
            "expires": o.expires,
            "next": "mnnn org join LINK",
        })),
        Some(m) if m.org_id == org.id && m.status == MemberStatus::Member => Ok(no("this invitation was already accepted")),
        _ => Ok(no("this invitation was withdrawn or is unknown here")),
    }
}

fn membership_apply(s: &Store, org: &Org) -> Result<Value> {
    let addrs = crate::net::client::my_addrs(s, None);
    Ok(json!({
        "org": org.name,
        "slug": org.slug,
        "access": org.access.as_str(),
        "node": org.owner_peer,
        "addrs": addrs,
        "next": format!("mnnn org apply {} {} --note \"…\"", if org.owner_peer.is_empty() { "NODE" } else { &org.owner_peer }, org.slug),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_are_global_versioned_and_answer_guests() {
        let s = Store::open_in_memory().unwrap();
        let (o, _) = super::super::tests::org_with_member(&s, "viewer");
        let es = executables(&o);
        assert_eq!(es.len(), BUILTINS.len());
        assert!(es.iter().all(|e| e.scope == CodeScope::Global && e.version_hash.len() == 64));
        assert_eq!(find("InviteRedeem").unwrap().name, "invite-redeem");
        let r = run(&s, "web", &Actor::Guest, &o, "membership-apply", &json!({})).unwrap();
        assert_eq!(r["result"]["slug"], o.slug.as_str());
        let r = run(&s, "web", &Actor::Guest, &o, "invite-redeem", &json!({ "link": "nonsense" })).unwrap();
        assert_eq!(r["result"]["valid"], false);
        assert!(run(&s, "web", &Actor::Guest, &o, "invite-redeem", &json!({})).is_err());
        // a served space of the same name overrides it, and then needs code.run
        let c = s.create_code_space("membership-apply", "", crate::store::Lang::Math, "x = close", Default::default()).unwrap();
        s.serve_code(&o.id, &c.id).unwrap();
        assert!(overridden(&s, &o, find("membership-apply").unwrap()));
        assert!(run(&s, "web", &Actor::Guest, &o, "membership-apply", &json!({})).is_err());
    }
}
