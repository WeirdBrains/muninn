//! Site announcements (2.17): when an org's site opens (or closes), this node tells the org's
//! nodes and the user's linked nodes, and, for public orgs, its connections, where the site
//! is and what it serves. Received announcements are kept per peer and org and shown by
//! `mnnn org sites` and the TUI's Site page. See `docs/org-hosting.md`.
use crate::error::{Error, Result};
use crate::net::exchange::scope_kind;
use crate::store::{Exchange, Org, Published, ResourceState, Store, Visibility, now_ms};
use serde::{Deserialize, Serialize};

const MAX_URLS: usize = 16;
const MAX_SERVES: usize = 64;
const MAX_TEXT: usize = 512;

/// What a node says about one of its sites.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SiteCard {
    pub org_id: String,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub visibility: Visibility,
    pub gated: bool,
    /// Where to open it (`https://…/o/slug/`, or a domain's root).
    pub urls: Vec<String>,
    /// What anyone may open there: public artifacts as `kind: name`.
    pub serves: Vec<String>,
    pub open: bool,
    pub at: i64,
}

/// Where this node's copy of an org's site is reached from elsewhere. Loopback and
/// unspecified bind addresses only count through the names and external addresses given.
pub fn urls(s: &Store, org: &Org, bind: &str, port: u16, names: &[String], external: &[String]) -> Result<Vec<String>> {
    let mut out = vec![];
    let port_part = if port == 443 { String::new() } else { format!(":{port}") };
    for d in crate::org::tls::domains(s, org)? {
        out.push(format!("https://{d}{port_part}/"));
    }
    let mut hosts: Vec<String> = vec![];
    for e in external {
        if let Some(h) = external_host(e) {
            hosts.push(h);
        }
    }
    hosts.extend(names.iter().map(|n| n.trim().to_string()).filter(|n| !n.is_empty() && n != "localhost"));
    if let Ok(ip) = bind.parse::<std::net::IpAddr>()
        && !ip.is_loopback()
        && !ip.is_unspecified()
    {
        hosts.push(if ip.is_ipv6() { format!("[{ip}]") } else { ip.to_string() });
    }
    for h in hosts {
        let u = format!("https://{h}{port_part}/o/{}/", org.slug);
        if !out.contains(&u) {
            out.push(u);
        }
    }
    out.truncate(MAX_URLS);
    Ok(out)
}

/// The host of an external address (`host`, `host:port` or a multiaddr).
fn external_host(e: &str) -> Option<String> {
    let e = e.trim();
    if e.is_empty() {
        return None;
    }
    if e.starts_with('/') {
        let parts: Vec<&str> = e.split('/').collect();
        return parts
            .windows(2)
            .find(|w| matches!(w[0], "ip4" | "dns" | "dns4" | "dns6"))
            .map(|w| w[1].to_string())
            .or_else(|| parts.windows(2).find(|w| w[0] == "ip6").map(|w| format!("[{}]", w[1])));
    }
    if e.starts_with('[') {
        return e.split(']').next().map(|h| format!("{h}]"));
    }
    Some(e.rsplit_once(':').filter(|(_, p)| p.parse::<u16>().is_ok()).map(|(h, _)| h).unwrap_or(e).to_string())
}

/// This node's card for an org's site.
pub fn card(s: &Store, org: &Org, urls: Vec<String>, open: bool) -> Result<SiteCard> {
    let serves = s
        .resources(&org.id)?
        .into_iter()
        .filter(|r| r.state == ResourceState::Active && r.published == Published::Public)
        .map(|r| format!("{}: {}", r.kind.label(), r.name))
        .take(MAX_SERVES)
        .collect();
    Ok(SiteCard {
        org_id: org.id.clone(),
        slug: org.slug.clone(),
        name: org.name.clone(),
        description: org.description.clone(),
        visibility: org.visibility,
        gated: org.gated,
        urls,
        serves,
        open,
        at: now_ms(),
    })
}

/// Whom an org's site is announced to: active exchanges that are memberships of this org,
/// node links, and (public orgs only) connections.
pub fn targets(s: &Store, org: &Org) -> Result<Vec<Exchange>> {
    Ok(s.exchanges()?.into_iter().filter(|x| x.status.active() && x.peer_id.is_some() && may_hear(x, &org.id, org.visibility)).collect())
}

/// May the other side of this exchange hear about (or tell us of) a site of this org?
pub fn may_hear(x: &Exchange, org_id: &str, vis: Visibility) -> bool {
    match scope_kind(&x.scope) {
        "organization" => x.scope.get("org").and_then(|v| v.as_str()) == Some(org_id),
        "link" => true,
        "node" => vis == Visibility::Public,
        _ => false,
    }
}

/// How an announcement went.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Report {
    pub sent: Vec<String>,
    pub failed: Vec<(String, String)>,
    /// Why nothing was sent, when nothing was.
    pub skipped: Option<String>,
}

/// Announce a site through `send` (one call per target exchange; the network or a test).
pub fn announce_with(s: &Store, org: &Org, c: &SiteCard, mut send: impl FnMut(&Exchange, SiteCard) -> Result<()>) -> Result<Report> {
    let mut r = Report::default();
    if !org.hosted {
        r.skipped = Some(format!("{} is not hosted (mnnn org edit {} --hosted on)", org.name, org.slug));
        return Ok(r);
    }
    if c.open && c.urls.is_empty() {
        r.skipped = Some(
            "the site is only reachable from this machine; set [hosting] bind to a reachable address, add hosting names, an external address (mnnn net external) or an org domain"
                .into(),
        );
        return Ok(r);
    }
    let to = targets(s, org)?;
    if to.is_empty() {
        r.skipped = Some("no org nodes, linked nodes or connections to tell".into());
        return Ok(r);
    }
    for x in to {
        match send(&x, c.clone()) {
            Ok(()) => r.sent.push(x.title()),
            Err(e) => r.failed.push((x.title(), e.to_string())),
        }
    }
    Ok(r)
}

/// Announce over the network (best effort; failures are reported, not raised).
pub fn announce(s: &Store, node: &crate::net::node::Node, org: &Org, c: &SiteCard, relays: &[String]) -> Result<Report> {
    let r = announce_with(s, org, c, |x, site| match crate::net::client::send(s, node, x, crate::net::proto::Req::SiteAnnounce { site }, relays)? {
        crate::net::proto::Resp::Ok => Ok(()),
        crate::net::proto::Resp::Error { msg } => Err(Error::Other(msg)),
        _ => Err(Error::Other("unexpected answer".into())),
    })?;
    if !r.sent.is_empty() {
        s.note_action(
            &crate::store::NewAction::new(
                "org.site.announce",
                "network",
                format!("announced {} as {} to {} node(s)", org.name, if c.open { "open" } else { "closed" }, r.sent.len()),
            )
            .params(serde_json::json!({ "org": org.id, "open": c.open, "sent": r.sent.len() })),
        );
    }
    Ok(r)
}

fn clip(s: &mut String) {
    if s.len() > MAX_TEXT {
        let mut end = MAX_TEXT;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        s.truncate(end);
    }
}

/// An announcement arrived over exchange `x`: check it may come this way and keep it.
pub fn receive(s: &Store, x: &Exchange, from: &str, mut c: SiteCard) -> Result<()> {
    if c.org_id.is_empty() || c.org_id.len() > 64 || !may_hear(x, &c.org_id, c.visibility) {
        return Err(Error::invalid("site", "this exchange may not announce that site"));
    }
    for t in [&mut c.slug, &mut c.name, &mut c.description] {
        clip(t);
    }
    c.urls.retain(|u| u.starts_with("https://") && u.len() <= MAX_TEXT);
    c.urls.truncate(MAX_URLS);
    c.serves.truncate(MAX_SERVES);
    c.serves.iter_mut().for_each(clip);
    s.put_site_announcement(from, &c.org_id, &x.id, &serde_json::to_string(&c)?, c.open)
}

/// The announced sites, newest first, with who announced each.
pub fn received(s: &Store) -> Result<Vec<(crate::store::SiteAnnouncement, SiteCard)>> {
    Ok(s.site_announcements()?.into_iter().filter_map(|a| serde_json::from_str(&a.card).ok().map(|c| (a, c))).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn x(scope: serde_json::Value) -> Exchange {
        serde_json::from_value(json!({
            "id": "x1", "label": "", "role": "answerer", "status": "complete", "peer_id": "peerA", "peer_label": "A",
            "scope": scope, "offer": "", "answer": null, "short_code": null, "fingerprint": null, "expires_at": null,
            "revoked_by": null, "author_id": null, "created_at": 0, "updated_at": 0,
            "verified_at": null, "revoked_at": null
        }))
        .unwrap()
    }

    #[test]
    fn external_hosts_parse() {
        assert_eq!(external_host("/ip4/1.2.3.4/tcp/4741").as_deref(), Some("1.2.3.4"));
        assert_eq!(external_host("/dns4/a.example/udp/1").as_deref(), Some("a.example"));
        assert_eq!(external_host("/ip6/::1/tcp/1").as_deref(), Some("[::1]"));
        assert_eq!(external_host("alien.duckdns.org:4741").as_deref(), Some("alien.duckdns.org"));
        assert_eq!(external_host("host").as_deref(), Some("host"));
        assert_eq!(external_host("[fe80::1]:9").as_deref(), Some("[fe80::1]"));
        assert_eq!(external_host(" "), None);
    }

    #[test]
    fn urls_skip_loopback_and_use_names() {
        let s = Store::open_in_memory().unwrap();
        let o = crate::org::create(&s, "test", "Lab", None, "").unwrap();
        assert!(urls(&s, &o, "127.0.0.1", 8443, &[], &[]).unwrap().is_empty());
        assert!(urls(&s, &o, "0.0.0.0", 8443, &["localhost".into()], &[]).unwrap().is_empty());
        let u = urls(&s, &o, "10.0.0.5", 8443, &["lab.example".into()], &["/ip4/1.2.3.4/tcp/4741".into()]).unwrap();
        assert_eq!(u, vec!["https://1.2.3.4:8443/o/lab/", "https://lab.example:8443/o/lab/", "https://10.0.0.5:8443/o/lab/"]);
        assert_eq!(urls(&s, &o, "10.0.0.5", 443, &[], &[]).unwrap(), vec!["https://10.0.0.5/o/lab/"]);
    }

    #[test]
    fn who_hears_what() {
        let org = json!({"kind": "organization", "org": "o1", "as": "apply"});
        assert!(may_hear(&x(org.clone()), "o1", Visibility::Private));
        assert!(!may_hear(&x(org), "o2", Visibility::Public));
        assert!(may_hear(&x(json!({"kind": "link"})), "o1", Visibility::Private));
        assert!(may_hear(&x(json!({"kind": "node"})), "o1", Visibility::Public));
        assert!(!may_hear(&x(json!({"kind": "node"})), "o1", Visibility::Private));
    }

    #[test]
    fn receive_checks_and_keeps() {
        let s = Store::open_in_memory().unwrap();
        let o = crate::org::create(&s, "test", "Lab", None, "").unwrap();
        let mut c = card(&s, &o, vec!["https://a/o/lab/".into(), "http://insecure/".into()], true).unwrap();
        // private org over a plain connection: refused
        assert!(receive(&s, &x(json!({"kind": "node"})), "peerA", c.clone()).is_err());
        c.name = "x".repeat(2000);
        receive(&s, &x(json!({"kind": "link"})), "peerA", c.clone()).unwrap();
        let got = received(&s).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].1.urls, vec!["https://a/o/lab/"]);
        assert_eq!(got[0].1.name.len(), MAX_TEXT);
        c.open = false;
        receive(&s, &x(json!({"kind": "link"})), "peerA", c).unwrap();
        assert!(!received(&s).unwrap()[0].1.open);
    }

    #[test]
    fn announce_reports_and_skips() {
        let s = Store::open_in_memory().unwrap();
        let o = crate::org::create(&s, "test", "Lab", None, "").unwrap();
        let c = card(&s, &o, vec!["https://a/o/lab/".into()], true).unwrap();
        let r = announce_with(&s, &o, &c, |_, _| Ok(())).unwrap();
        assert!(r.skipped.unwrap().contains("not hosted"));
        let o = s.update_org(&o.id, &crate::store::OrgChange { hosted: Some(true), ..Default::default() }).unwrap();
        let empty = card(&s, &o, vec![], true).unwrap();
        assert!(announce_with(&s, &o, &empty, |_, _| Ok(())).unwrap().skipped.unwrap().contains("only reachable"));
        assert!(announce_with(&s, &o, &c, |_, _| Ok(())).unwrap().skipped.unwrap().contains("no org nodes"));
    }
}
