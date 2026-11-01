//! What an org's web site serves, apart from the transport: pages through the org's
//! template, published artifacts in their formats, template assets and the JSON API.
//! The HTTPS and gRPC server in [`super::host`] calls [`route`]; mirrors serve a
//! [`Snapshot`] of another node's public site the same way.
use super::{Actor, OrgCard};
use crate::error::{Error, Result};
use crate::net::service;
use crate::store::{Access, MemberStatus, Org, Published, Resource, ResourceState, ShareKind, Store, Visibility, now_ms};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The look of an org's site when it has no template of its own.
pub const DEFAULT_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{{title}}</title>
<style>{{css}}</style>
</head>
<body>
<header><h1><a href="{{base}}">{{org.name}}</a></h1><p>{{org.description}}</p></header>
<main>{{content}}</main>
<footer>Published with Muninn</footer>
</body>
</html>
"#;

pub const DEFAULT_CSS: &str = "body{font-family:system-ui,sans-serif;max-width:60rem;margin:2rem auto;padding:0 1rem;line-height:1.5;color:#1d1d1f}\
header{border-bottom:1px solid #ddd;margin-bottom:1.5rem}a{color:#2b59c3}table{border-collapse:collapse}td,th{border:1px solid #ddd;padding:.25rem .5rem}\
footer{margin-top:3rem;color:#888;font-size:.85rem}ul.artifacts li{margin:.4rem 0}.formats a{margin-left:.5rem;font-size:.85rem}";

/// An answer, whatever carries it.
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub status: u16,
    pub mime: String,
    pub body: Vec<u8>,
}

impl Page {
    pub fn new(status: u16, mime: &str, body: impl Into<Vec<u8>>) -> Page {
        Page { status, mime: mime.into(), body: body.into() }
    }
    pub fn json(v: &Value) -> Page {
        Page::new(200, "application/json", serde_json::to_vec_pretty(v).unwrap_or_default())
    }
    pub fn error(status: u16, msg: &str) -> Page {
        Page::new(status, "application/json", serde_json::to_vec(&json!({ "error": msg })).unwrap_or_default())
    }
    pub fn not_found() -> Page {
        Page::error(404, "not found")
    }
}

/// The formats an artifact of a kind comes in (the first is the default).
pub fn formats(kind: ShareKind) -> &'static [&'static str] {
    match kind {
        ShareKind::Journal | ShareKind::Report => &["html", "json", "pdf", "docx", "latex", "text"],
        ShareKind::Project => &["html", "json", "pdf", "docx", "latex", "text", "mnnp"],
        ShareKind::Model | ShareKind::LiveSearch | ShareKind::Code | ShareKind::Pipeline => &["json"],
    }
}

fn ext_of(f: &str) -> &str {
    match f {
        "latex" => "tex",
        "text" => "txt",
        f => f,
    }
}

fn format_of(ext: &str) -> &str {
    match ext {
        "tex" => "latex",
        "txt" => "text",
        e => e,
    }
}

pub fn mime(format: &str) -> &'static str {
    match format {
        "html" => "text/html; charset=utf-8",
        "json" => "application/json",
        "pdf" => "application/pdf",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "latex" => "application/x-tex",
        "text" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

pub fn escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&#39;"),
            c => o.push(c),
        }
    }
    o
}

/// The inside of a rendered document's `<body>` (for wrapping it in the template).
fn body_of(html: &str) -> &str {
    let lower = html.to_ascii_lowercase();
    let Some(b) = lower.find("<body") else { return html };
    let Some(open) = lower[b..].find('>') else { return html };
    let start = b + open + 1;
    let end = lower.rfind("</body>").filter(|e| *e >= start).unwrap_or(html.len());
    &html[start..end]
}

/// One site's look: template html, css and its assets.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Look {
    pub name: String,
    pub html: String,
    pub css: String,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Asset {
    pub name: String,
    pub mime: String,
    /// base64
    pub data: String,
}

/// Fill a template: `{{title}}`, `{{org.name}}`, `{{org.description}}`, `{{org.slug}}`,
/// `{{base}}` (the site's address), `{{assets}}` (where its assets are), `{{css}}` and
/// `{{content}}` (the page, already HTML). Everything but the content is escaped.
pub fn fill(look: &Look, org: &OrgCard, title: &str, content: &str) -> String {
    let html = if look.html.trim().is_empty() { DEFAULT_HTML } else { &look.html };
    let css = if look.css.trim().is_empty() && look.html.trim().is_empty() { DEFAULT_CSS } else { &look.css };
    let base = format!("/o/{}/", org.slug);
    html.replace("{{title}}", &escape(title))
        .replace("{{org.name}}", &escape(&org.name))
        .replace("{{org.description}}", &escape(&org.description))
        .replace("{{org.slug}}", &escape(&org.slug))
        .replace("{{base}}", &base)
        .replace("{{assets}}", &format!("{base}assets/"))
        .replace("{{css}}", &css.replace("</", "<\\/"))
        .replace("{{content}}", content)
}

/// A published artifact with what it renders to (for mirrors: html and json).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Artifact {
    pub id: String,
    pub kind: ShareKind,
    pub name: String,
    pub published: Published,
    /// format → base64 (snapshots only)
    #[serde(default)]
    pub data: std::collections::BTreeMap<String, String>,
}

/// An org's public site, as a mirror keeps it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Snapshot {
    pub org: OrgCard,
    pub look: Look,
    pub artifacts: Vec<Artifact>,
    pub taken_at: i64,
}

fn b64() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

/// The org's look (its chosen template, else the default).
pub fn look(s: &Store, org: &Org) -> Result<Look> {
    let t = match &org.template_id {
        Some(id) => s.template(id)?,
        None => None,
    };
    let Some(t) = t else { return Ok(Look::default()) };
    let mut assets = vec![];
    for a in s.template_assets(&t.id)? {
        if let Some((mime, data)) = s.template_asset(&t.id, &a.name)? {
            assets.push(Asset { name: a.name, mime, data: b64().encode(data) });
        }
    }
    Ok(Look { name: t.name, html: t.html, css: t.css, assets })
}

/// What anyone may see of an org: its public artifacts, in html and json.
pub fn snapshot(s: &Store, org: &Org) -> Result<Value> {
    let mut artifacts = vec![];
    for r in s.resources(&org.id)?.into_iter().filter(|r| r.state == ResourceState::Active && r.published == Published::Public) {
        let mut data = std::collections::BTreeMap::new();
        for f in formats(r.kind).iter().filter(|f| matches!(**f, "html" | "json")) {
            if let Ok(it) = service::item_of(s, r.kind, &r.ref_id, &r.name, Some(f)) {
                data.insert(f.to_string(), b64().encode(&it.data));
            }
        }
        artifacts.push(Artifact { id: r.id, kind: r.kind, name: r.name, published: r.published, data });
    }
    let mut card = super::card(s, org);
    card.members = 0;
    Ok(serde_json::to_value(Snapshot { org: card, look: look(s, org)?, artifacts, taken_at: now_ms() })?)
}

// ---------- routing ----------

/// A site this node serves: an org it keeps, or a mirror of another node's.
#[allow(clippy::large_enum_variant)]
pub enum Site {
    Live(Org),
    Mirror(Box<Snapshot>),
}

/// The site at a slug.
pub fn site(s: &Store, slug: &str) -> Result<Option<Site>> {
    if let Some(o) = s.orgs()?.into_iter().find(|o| o.slug == slug) {
        return Ok(Some(Site::Live(o)));
    }
    for m in s.mirrors()? {
        if m.slug == slug
            && let Ok(snap) = serde_json::from_slice::<Snapshot>(&m.snapshot)
        {
            return Ok(Some(Site::Mirror(Box::new(snap))));
        }
    }
    Ok(None)
}

/// The actor for a signed request (an exchange that is a membership of the org).
pub fn actor(s: &Store, org: &Org, exchange: Option<&str>) -> Actor {
    let Some(x) = exchange else { return Actor::Guest };
    match s.member_by_exchange(x) {
        Ok(Some(m)) if m.org_id == org.id && m.status == MemberStatus::Member && org.member_auth => super::actor_for(s, &m).unwrap_or(Actor::Guest),
        _ => Actor::Guest,
    }
}

/// May this actor see the org at all? (Private orgs are invisible to guests.)
/// The org a gRPC executable call is for and who is calling, gated like the site.
pub fn exec_caller(s: &Store, org: &Org, exchange: Option<&str>) -> Option<Actor> {
    let a = actor(s, org, exchange);
    (org.hosted && shown(org, &a)).then_some(a)
}

/// What a guest sees of a gated private org: a landing page, how members sign in, and the
/// page an invite link opens. The invite itself rides in the link's `#fragment`, which
/// browsers never send, so it never reaches this node's logs.
fn gate(s: &Store, org: &Org, page: &str) -> Result<Page> {
    let card = super::card(s, org);
    let name = escape(&org.name);
    let slug = escape(&org.slug);
    let (title, content) = match page {
        "sign-in" => (
            format!("Sign in to {}", org.name),
            format!(
                "<h1>Sign in to {name}</h1><p>Members sign in from their own Muninn node: run</p>\
                 <pre>mnnn org sign-in {slug} --site https://THIS-ADDRESS</pre>\
                 <p>and open the link it prints here within five minutes. It works once and keeps this browser signed in for 12 hours \
                 (<a href=\"/o/{slug}/sign-out\">sign out</a>).</p>"
            ),
        ),
        "join" => (
            format!("Join {}", org.name),
            format!(
                "<h1>Join {name}</h1><p id=\"how\">Open this page from the invite link you were given.</p>\
                 <p><a id=\"open\" hidden>Open in Muninn</a></p><pre id=\"cmd\" hidden></pre>\
                 <script src=\"/o/{slug}/assets-builtin/join.js\"></script>\
                 <noscript><p>Copy the part of the link after <code>#</code> and run <code>mnnn org join</code> with it.</p></noscript>"
            ),
        ),
        _ => (
            org.name.clone(),
            format!(
                "<h1>{name}</h1><p>{}</p><p>This organization is private.</p>\
                 <ul><li><a href=\"/o/{slug}/sign-in\">Members: sign in</a></li><li><a href=\"/o/{slug}/join\">Have an invite? Join</a></li></ul>",
                escape(&org.description)
            ),
        ),
    };
    Ok(Page::new(200, "text/html; charset=utf-8", fill(&look(s, org)?, &card, &title, &content)))
}

/// The join page's script: it reads the invite from the fragment, in the browser.
pub const JOIN_JS: &str = r#"(function () {
  var t = decodeURIComponent((location.hash || "").slice(1));
  if (!t) return;
  history.replaceState(null, "", location.pathname);
  var how = document.getElementById("how"), open = document.getElementById("open"), cmd = document.getElementById("cmd");
  how.textContent = "Accept it from your Muninn node (this starts the key exchange with the organization):";
  if (t.indexOf("muninn://") === 0) { open.href = t; open.hidden = false; }
  cmd.textContent = "mnnn org join '" + t.replace(/'/g, "") + "'";
  cmd.hidden = false;
})();
"#;

fn gate_asset(s: &Store, org: &Org, rest: &[&str]) -> Result<Page> {
    let name = rest[1..].join("/");
    let Some(t) = org.template_id.as_deref() else { return Ok(Page::not_found()) };
    match s.template_asset(t, &name)? {
        Some((mime, data)) => Ok(Page::new(200, &mime, data)),
        None => Ok(Page::not_found()),
    }
}

fn builtin_run(s: &Store, org: &Org, a: &Actor, name: &str, body: &[u8]) -> Result<Page> {
    let input: Value = if body.is_empty() { json!({}) } else { serde_json::from_slice(body).map_err(|e| Error::invalid("input", e.to_string()))? };
    match super::builtin::run(s, "web", a, org, name, &input) {
        Ok(v) => Ok(Page::json(&v)),
        Err(e) if e.to_string().contains("may not") => Ok(Page::error(403, &e.to_string())),
        Err(e) => Ok(Page::error(400, &e.to_string())),
    }
}

fn shown(org: &Org, a: &Actor) -> bool {
    a.is_member() || org.visibility == Visibility::Public || org.access == Access::Open
}

/// A request to the site: method, path (with an optional `?format=`), body and the
/// exchange id of a member who signed it.
pub fn route(s: &Store, method: &str, path: &str, body: &[u8], exchange: Option<&str>) -> Page {
    match route_inner(s, method, path, body, exchange) {
        Ok(p) => p,
        Err(e) => Page::error(400, &e.to_string()),
    }
}

fn route_inner(s: &Store, method: &str, path: &str, body: &[u8], exchange: Option<&str>) -> Result<Page> {
    let path = path.split('?').next().unwrap_or("");
    let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    match parts.as_slice() {
        [""] => index(s),
        ["o", slug] | ["o", slug, ""] => {
            let Some(site) = site(s, slug)? else { return Ok(Page::not_found()) };
            org_page(s, &site, exchange)
        }
        ["o", _, "assets-builtin", "join.js"] => Ok(Page::new(200, "text/javascript; charset=utf-8", JOIN_JS.as_bytes().to_vec())),
        ["o", slug, rest @ ..] => {
            let Some(site) = site(s, slug)? else { return Ok(Page::not_found()) };
            match site {
                Site::Live(org) => live(s, &org, method, rest, body, exchange),
                Site::Mirror(snap) => mirror(&snap, method, rest),
            }
        }
        _ => Ok(Page::not_found()),
    }
}

/// `/`: the public orgs and mirrors this node serves.
fn index(s: &Store) -> Result<Page> {
    let mut items = String::new();
    for o in s.orgs()?.into_iter().filter(|o| o.hosted && o.visibility == Visibility::Public) {
        items.push_str(&format!("<li><a href=\"/o/{0}/\">{1}</a> {2}</li>", escape(&o.slug), escape(&o.name), escape(&o.description)));
    }
    for m in s.mirrors()? {
        if let Ok(snap) = serde_json::from_slice::<Snapshot>(&m.snapshot) {
            items.push_str(&format!(
                "<li><a href=\"/o/{0}/\">{1}</a> (mirror) {2}</li>",
                escape(&snap.org.slug),
                escape(&snap.org.name),
                escape(&snap.org.description)
            ));
        }
    }
    let card = OrgCard {
        id: String::new(),
        name: "Organizations".into(),
        slug: String::new(),
        description: String::new(),
        visibility: Visibility::Public,
        access: Access::Members,
        owner_peer: String::new(),
        members: 0,
    };
    let content = if items.is_empty() { "<p>Nothing is published here.</p>".to_string() } else { format!("<ul>{items}</ul>") };
    Ok(Page::new(200, "text/html; charset=utf-8", fill(&Look::default(), &card, "Organizations", &content).replace("href=\"/o//\"", "href=\"/\"")))
}

fn artifact_list(org: &OrgCard, arts: &[(String, ShareKind, String)]) -> String {
    if arts.is_empty() {
        return "<p>Nothing is published yet.</p>".into();
    }
    let mut out = String::from("<ul class=\"artifacts\">");
    for (id, kind, name) in arts {
        let links: Vec<String> =
            formats(*kind).iter().map(|f| format!("<a href=\"/o/{}/a/{}.{}\">{}</a>", escape(&org.slug), escape(id), ext_of(f), f)).collect();
        out.push_str(&format!(
            "<li><a href=\"/o/{0}/a/{1}\">{2}</a> <small>{3}</small><span class=\"formats\">{4}</span></li>",
            escape(&org.slug),
            escape(id),
            escape(name),
            kind.label(),
            links.join("")
        ));
    }
    out.push_str("</ul>");
    out
}

fn visible_resources(s: &Store, org: &Org, a: &Actor) -> Result<Vec<Resource>> {
    Ok(s.resources(&org.id)?.into_iter().filter(|r| r.published != Published::No && super::visible(s, org, a, r)).collect())
}

fn org_page(s: &Store, site: &Site, exchange: Option<&str>) -> Result<Page> {
    match site {
        Site::Live(org) => {
            let a = actor(s, org, exchange);
            if org.hosted && org.gated && !shown(org, &a) {
                return gate(s, org, "");
            }
            if !org.hosted || !shown(org, &a) {
                return Ok(Page::not_found());
            }
            let card = super::card(s, org);
            let arts: Vec<_> = visible_resources(s, org, &a)?.into_iter().map(|r| (r.id, r.kind, r.name)).collect();
            Ok(Page::new(200, "text/html; charset=utf-8", fill(&look(s, org)?, &card, &org.name, &artifact_list(&card, &arts))))
        }
        Site::Mirror(snap) => {
            let arts: Vec<_> = snap.artifacts.iter().map(|r| (r.id.clone(), r.kind, r.name.clone())).collect();
            Ok(Page::new(200, "text/html; charset=utf-8", fill(&snap.look, &snap.org, &snap.org.name, &artifact_list(&snap.org, &arts))))
        }
    }
}

fn split_ext(name: &str) -> (&str, Option<&str>) {
    match name.rsplit_once('.') {
        Some((id, ext)) => (id, Some(ext)),
        None => (name, None),
    }
}

fn live(s: &Store, org: &Org, method: &str, rest: &[&str], body: &[u8], exchange: Option<&str>) -> Result<Page> {
    let a = actor(s, org, exchange);
    if org.hosted && org.gated && !shown(org, &a) {
        return match (method, rest) {
            ("GET", [page @ ("sign-in" | "join")]) => gate(s, org, page),
            ("GET", ["assets", ..]) if org.template_id.is_some() => gate_asset(s, org, rest),
            ("POST", ["api", "run", name]) if super::builtin::find(name).is_some() => builtin_run(s, org, &a, name, body),
            ("GET", ["api", "executables"]) => {
                let v: Vec<Value> = super::builtin::executables(org).iter().map(|e| json!(e)).collect();
                Ok(Page::json(&json!({ "executables": v })))
            }
            _ => Ok(Page::not_found()),
        };
    }
    if !org.hosted || !shown(org, &a) {
        return Ok(Page::not_found());
    }
    let card = super::card(s, org);
    match (method, rest) {
        ("GET", ["a", name]) => {
            let (id, ext) = split_ext(name);
            let Ok(r) = s.find_resource(&org.id, id) else { return Ok(Page::not_found()) };
            if r.published == Published::No || !super::visible(s, org, &a, &r) {
                return Ok(Page::not_found());
            }
            let fmt = ext.map(format_of);
            if let Some(f) = fmt
                && !formats(r.kind).contains(&f)
            {
                return Ok(Page::error(404, &format!("{} comes as {}", r.kind.label(), formats(r.kind).join(", "))));
            }
            let it = service::item_of(s, r.kind, &r.ref_id, &r.name, Some(fmt.unwrap_or(formats(r.kind)[0])))?;
            note_serve(s, org, &a, &r, &it.format);
            if fmt.is_none() && it.format == "html" {
                let html = String::from_utf8_lossy(&it.data);
                return Ok(Page::new(200, "text/html; charset=utf-8", fill(&look(s, org)?, &card, &r.name, body_of(&html))));
            }
            Ok(Page::new(200, mime(&it.format), it.data))
        }
        ("GET", ["assets", name @ ..]) => {
            let name = name.join("/");
            let Some(t) = org.template_id.as_deref() else { return Ok(Page::not_found()) };
            match s.template_asset(t, &name)? {
                Some((mime, data)) => Ok(Page::new(200, &mime, data)),
                None => Ok(Page::not_found()),
            }
        }
        ("GET", ["api", "org"]) => Ok(Page::json(&json!(card))),
        ("GET", ["api", "artifacts"]) => {
            let v: Vec<Value> = visible_resources(s, org, &a)?
                .into_iter()
                .map(|r| json!({ "id": r.id, "kind": r.kind, "name": r.name, "published": r.published, "formats": formats(r.kind) }))
                .collect();
            Ok(Page::json(&json!({ "artifacts": v })))
        }
        ("GET", ["api", "endpoints"]) => {
            let v: Vec<Value> = s
                .endpoints(&org.id)?
                .into_iter()
                .filter(|e| (e.anyone && matches!(a, Actor::Guest)) || super::can(s, org, &a, &e.perm))
                .map(|e| json!({ "name": e.name, "kind": e.kind.as_str(), "perm": e.perm, "anyone": e.anyone }))
                .collect();
            Ok(Page::json(&json!({ "endpoints": v })))
        }
        ("POST", ["api", "call", endpoint]) => {
            if s.find_endpoint(&org.id, endpoint).is_err() {
                return Ok(Page::not_found());
            }
            let input: Value =
                if body.is_empty() { json!({}) } else { serde_json::from_slice(body).map_err(|e| Error::invalid("input", e.to_string()))? };
            match super::call(s, "web", &a, org, endpoint, &input) {
                Ok(v) => Ok(Page::json(&v)),
                Err(e) if e.to_string().contains("may not") => Ok(Page::error(403, &e.to_string())),
                Err(e) => Ok(Page::error(400, &e.to_string())),
            }
        }
        ("GET", ["api", "executables"]) => {
            let mut v: Vec<Value> = super::exec::executables(s, org, &a)?.iter().map(|e| json!(e)).collect();
            v.extend(
                super::builtin::BUILTINS.iter().filter(|b| !super::builtin::overridden(s, org, b)).map(|b| json!(super::builtin::describe(org, b))),
            );
            Ok(Page::json(&json!({ "executables": v })))
        }
        ("POST", ["api", "run", name]) if super::builtin::find(name).is_some() => builtin_run(s, org, &a, name, body),
        ("POST", ["api", "run", name]) => {
            let input: Value =
                if body.is_empty() { json!({}) } else { serde_json::from_slice(body).map_err(|e| Error::invalid("input", e.to_string()))? };
            let input = match super::exec::Input::from_json(&input) {
                Ok(i) => i,
                Err(e) => return Ok(Page::error(400, &e.to_string())),
            };
            match super::exec::run(s, "web", &a, org, name, &input) {
                Ok(v) => Ok(Page::json(&v)),
                Err(e) if e.to_string().contains("may not") => Ok(Page::error(403, &e.to_string())),
                Err(e) if e.to_string().contains("is not served") => Ok(Page::not_found()),
                Err(e) => Ok(Page::error(400, &e.to_string())),
            }
        }
        ("GET", ["proto", rest @ ..]) => {
            let path = rest.join("/");
            if !super::can(s, org, &a, "code.run") {
                return Ok(Page::not_found());
            }
            if path == super::exec::COMMON_PROTO {
                return Ok(Page::new(200, "text/plain; charset=utf-8", super::exec::common_proto().into_bytes()));
            }
            let Some(svc) = path.strip_suffix(".proto") else { return Ok(Page::not_found()) };
            match super::exec::executables(s, org, &a)?.into_iter().find(|e| e.service.rsplit('.').next() == Some(svc)) {
                Some(e) => Ok(Page::new(200, "text/plain; charset=utf-8", super::exec::space_proto(org, &e).into_bytes())),
                None => Ok(Page::not_found()),
            }
        }
        _ => Ok(Page::not_found()),
    }
}

fn note_serve(s: &Store, org: &Org, a: &Actor, r: &Resource, format: &str) {
    if a.is_member() {
        s.note_action(
            &crate::store::NewAction::new("org.serve", "web", format!("served {} {} of {} to {}", r.kind.label(), r.name, org.name, a.who()))
                .params(json!({ "org": org.id, "resource": r.id, "format": format, "by": a.who() })),
        );
    }
}

fn mirror(snap: &Snapshot, method: &str, rest: &[&str]) -> Result<Page> {
    if method != "GET" {
        return Ok(Page::error(405, "a mirror only serves what is published"));
    }
    match rest {
        ["a", name] => {
            let (id, ext) = split_ext(name);
            let Some(r) = snap.artifacts.iter().find(|r| r.id == id || (id.len() >= 4 && r.id.starts_with(id))) else { return Ok(Page::not_found()) };
            let f = ext.map(format_of).unwrap_or(if r.data.contains_key("html") { "html" } else { "json" });
            let Some(data) = r.data.get(f).and_then(|d| b64().decode(d).ok()) else {
                return Ok(Page::error(404, "a mirror keeps html and json; ask the org's own node for other formats"));
            };
            if ext.is_none() && f == "html" {
                let html = String::from_utf8_lossy(&data);
                return Ok(Page::new(200, "text/html; charset=utf-8", fill(&snap.look, &snap.org, &r.name, body_of(&html))));
            }
            Ok(Page::new(200, mime(f), data))
        }
        ["assets", name @ ..] => {
            let name = name.join("/");
            match snap.look.assets.iter().find(|a| a.name == name) {
                Some(a) => Ok(Page::new(200, &a.mime, b64().decode(&a.data).unwrap_or_default())),
                None => Ok(Page::not_found()),
            }
        }
        ["api", "org"] => Ok(Page::json(&json!(snap.org))),
        ["api", "artifacts"] => {
            let v: Vec<Value> = snap
                .artifacts
                .iter()
                .map(
                    |r| json!({ "id": r.id, "kind": r.kind, "name": r.name, "published": r.published, "formats": r.data.keys().collect::<Vec<_>>() }),
                )
                .collect();
            Ok(Page::json(&json!({ "artifacts": v })))
        }
        _ => Ok(Page::not_found()),
    }
}
