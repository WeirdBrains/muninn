//! Create-then-share: make an item or a share target in the same place a share is added.
//!
//! Used by `mnnn share --create` / `--create-to`, org resource/project/give `--create`,
//! `project add --create`, `net share --create`, review request `--create`, FFI, and the TUI
//! "+ New KIND…" rows. Create is recorded in history before the share; cancel never creates.
use crate::code::{self, spaces};
use crate::error::{Error, Result};
use crate::model::{Duration, Symbol};
use crate::net::{Node, client};
use crate::store::ReviewKind;
use crate::store::{CodeInput, Design, EntryFormat, ItemKind, Lang, NewAction, SearchSpec, ShareKind, SpaceKind, Store};
use libp2p::identity::Keypair;
use serde::Serialize;
use serde_json::json;

use super::{Shared, TargetKind, share};

/// Options for creating an item or a target inline (minimal fields; same as the usual create).
#[derive(Debug, Clone, Default)]
pub struct CreateOpts {
    pub description: String,
    /// Code language (`math` default when creating code without `--lang`).
    pub lang: Option<Lang>,
    /// Code: copy from an existing code or a built-in (`template:…` / parser name).
    pub from: Option<String>,
    /// Code space kind (default `analysis`).
    pub space_kind: Option<SpaceKind>,
    /// Code space main (existing code name/id). When empty and creating a space, a main
    /// code named `{name}-main` is created first.
    pub main: Option<String>,
    /// Journal format (default Markdown).
    pub format: Option<EntryFormat>,
    /// Live search query (default empty series search).
    pub query: Option<String>,
    /// Review entry: journal to write into (created if missing when empty → `{name}-journal`).
    pub journal: Option<String>,
    /// Review entry body / record field assignments (`k=v` lines or JSON object).
    pub body: Option<String>,
    /// Review record: model to create the record in (or `MODEL` from `MODEL/…` key).
    pub model: Option<String>,
    /// Net exchange: send the offer to this peer (`…/p2p/ID`, peer id or known name).
    pub peer: Option<String>,
    /// Net exchange: hours the offer stays valid (default 168).
    pub hours: Option<u64>,
}

/// What was created inline before a share.
#[derive(Debug, Clone, Serialize)]
pub struct Created {
    pub kind: String,
    pub id: String,
    pub name: String,
}

impl Created {
    fn item(kind: ShareKind, id: String, name: String) -> Self {
        Self { kind: kind.as_str().into(), id, name }
    }
    fn target(t: TargetKind, id: String, name: String) -> Self {
        Self { kind: t.as_str().into(), id, name }
    }
    fn review(kind: ReviewKind, id: String, name: String) -> Self {
        Self { kind: kind.as_str().into(), id, name }
    }
}

/// Identity needed to offer a key exchange as a net share destination.
pub struct NetCreate<'a> {
    pub node: Option<&'a Node>,
    pub kp: &'a Keypair,
    pub my_label: &'a str,
    pub addrs: Vec<String>,
    pub relays: &'a [String],
}

/// Create a shareable item. Records the create in history with `origin`.
pub fn create_item(s: &Store, kind: ShareKind, name: &str, opts: &CreateOpts, origin: &str) -> Result<Created> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::invalid("create", "give a name"));
    }
    match kind {
        ShareKind::Model => {
            let (m, _) = s.create_model(name, &opts.description, Design::default(), false)?;
            s.note_action(
                &NewAction::new("model.create", origin, format!("create model {} as a draft", m.name))
                    .params(json!({ "model": m.id, "name": m.name })),
            );
            Ok(Created::item(kind, m.id, m.name))
        }
        ShareKind::Journal => {
            let fmt = opts.format.unwrap_or_default();
            let j = s.create_journal(name, None, fmt)?;
            s.note_action(
                &NewAction::new("journal.create", origin, format!("create journal {}", j.name)).params(json!({ "journal": j.id, "name": j.name })),
            );
            Ok(Created::item(kind, j.id, j.name))
        }
        ShareKind::Project => {
            let p = s.create_project(name, &opts.description)?;
            s.note_action(
                &NewAction::new("project.create", origin, format!("create project {}", p.name)).params(json!({ "project": p.id, "name": p.name })),
            );
            Ok(Created::item(kind, p.id, p.name))
        }
        ShareKind::Report => {
            let r = s.create_report(name, &opts.description)?;
            s.note_action(
                &NewAction::new("report.create", origin, format!("create report {}", r.name)).params(json!({ "report": r.id, "name": r.name })),
            );
            Ok(Created::item(kind, r.id, r.name))
        }
        ShareKind::LiveSearch => {
            let spec = SearchSpec { query: opts.query.clone().unwrap_or_default(), ..Default::default() };
            let ls = s.save_live_search(name, &spec, None)?;
            s.note_action(
                &NewAction::new("search.create", origin, format!("create live search {}", ls.name))
                    .params(json!({ "search": ls.id, "name": ls.name })),
            );
            Ok(Created::item(kind, ls.id, ls.name))
        }
        ShareKind::Code => {
            let c = create_code(s, name, opts, origin)?;
            Ok(Created::item(kind, c.id, c.name))
        }
        ShareKind::Space => {
            let sp = create_space(s, name, opts, origin)?;
            Ok(Created::item(kind, sp.id, sp.name))
        }
        ShareKind::Pipeline => {
            let g = crate::code::pipeline::Graph::default();
            let p = s.create_pipeline(name, &opts.description, &g)?;
            s.note_action(
                &NewAction::new("pipeline.create", origin, format!("create pipeline {}", p.name)).params(json!({ "pipeline": p.id, "name": p.name })),
            );
            Ok(Created::item(kind, p.id, p.name))
        }
    }
}

/// Create a share target (project, journal, code, space, or an offered net exchange).
/// For `net`, pass [`NetCreate`] (identity); without it this refuses.
pub fn create_target(s: &Store, target: TargetKind, name: &str, opts: &CreateOpts, origin: &str, net: Option<&NetCreate<'_>>) -> Result<Created> {
    match target {
        TargetKind::Project => create_item(s, ShareKind::Project, name, opts, origin).map(|c| Created::target(target, c.id, c.name)),
        TargetKind::Journal => create_item(s, ShareKind::Journal, name, opts, origin).map(|c| Created::target(target, c.id, c.name)),
        TargetKind::Code => create_item(s, ShareKind::Code, name, opts, origin).map(|c| Created::target(target, c.id, c.name)),
        TargetKind::Space => create_item(s, ShareKind::Space, name, opts, origin).map(|c| Created::target(target, c.id, c.name)),
        TargetKind::Net => {
            let net = net.ok_or_else(|| Error::invalid("create", "creating a net destination needs the node identity (CLI/TUI/FFI supply it)"))?;
            create_exchange_target(s, net, name, opts, origin)
        }
    }
}

/// Offer a key exchange labeled `name` and return it as a share destination.
pub fn create_exchange_target(s: &Store, net: &NetCreate<'_>, name: &str, opts: &CreateOpts, origin: &str) -> Result<Created> {
    let label = name.trim();
    if label.is_empty() {
        return Err(Error::invalid("create", "give an exchange label"));
    }
    let hours = opts.hours.unwrap_or(168);
    let peer = opts.peer.as_deref();
    let c = client::create(s, net.node, net.kp, label, net.my_label, net.addrs.clone(), peer, (hours as i64) * 3_600_000, net.relays, origin)?;
    let name = if c.exchange.label.is_empty() { c.exchange.short_id().to_string() } else { c.exchange.label.clone() };
    Ok(Created::target(TargetKind::Net, c.exchange.id, name))
}

/// Create the subject of a review for every [`ReviewKind`] the reviews table supports.
///
/// - journal / project / search: same as [`create_item`]
/// - entry: writes an entry (creates the journal when `--journal` is omitted: `{name}-journal`)
/// - record: creates a record in `--model` (or `MODEL` from `MODEL/…`); values from `--body`
pub fn create_review_subject(s: &Store, kind: ReviewKind, key: &str, opts: &CreateOpts, origin: &str) -> Result<Created> {
    let key = key.trim();
    if key.is_empty() {
        return Err(Error::invalid("create", "give a name"));
    }
    match kind {
        ReviewKind::Journal => create_item(s, ShareKind::Journal, key, opts, origin).map(|c| Created::review(kind, c.id, c.name)),
        ReviewKind::Project => create_item(s, ShareKind::Project, key, opts, origin).map(|c| Created::review(kind, c.id, c.name)),
        ReviewKind::Search => create_item(s, ShareKind::LiveSearch, key, opts, origin).map(|c| Created::review(kind, c.id, c.name)),
        ReviewKind::Entry => {
            let jname = opts.journal.as_deref().map(str::trim).filter(|s| !s.is_empty()).unwrap_or("").to_string();
            let j = if jname.is_empty() {
                let n = format!("{key}-journal");
                create_item(s, ShareKind::Journal, &n, opts, origin)?;
                s.find_journal(&n)?
            } else if let Ok(j) = s.find_journal(&jname) {
                j
            } else {
                create_item(s, ShareKind::Journal, &jname, opts, origin)?;
                s.find_journal(&jname)?
            };
            let body = opts.body.clone().unwrap_or_default();
            let e = s.add_entry(&j.id, key, &body, opts.format)?;
            let heading = e.heading().to_string();
            s.note_action(
                &NewAction::new("entry.add", origin, format!("write entry {} in {}", heading, j.name))
                    .params(json!({ "journal": j.id, "entry": e.id })),
            );
            Ok(Created::review(kind, e.id, heading))
        }
        ReviewKind::Record => {
            let (model_key, _) = key.split_once(['/', ':']).map(|(m, r)| (m.trim(), r.trim())).unwrap_or((key, ""));
            let model_key = opts.model.as_deref().map(str::trim).filter(|s| !s.is_empty()).unwrap_or(model_key);
            if model_key.is_empty() {
                return Err(Error::invalid("create", "a record needs a model (`--model NAME` or MODEL/… )"));
            }
            let m = if let Ok(m) = s.find_model(model_key) {
                m
            } else {
                // Records need an applied model (not a draft).
                let (m, _) = s.create_model(model_key, &opts.description, Design::default(), true)?;
                s.note_action(
                    &NewAction::new("model.create", origin, format!("create model {} v1", m.name)).params(json!({ "model": m.id, "name": m.name })),
                );
                m
            };
            let values = parse_record_body(opts.body.as_deref().unwrap_or(""))?;
            let r = s.create_record(&m, &values)?;
            s.note_action(
                &NewAction::new("record.create", origin, format!("create {} record {}", m.name, r.short_id()))
                    .params(json!({ "model": m.id, "record": r.id })),
            );
            Ok(Created::review(kind, r.id.clone(), format!("{}/{}", m.name, r.short_id())))
        }
    }
}

fn parse_record_body(body: &str) -> Result<serde_json::Map<String, serde_json::Value>> {
    let body = body.trim();
    if body.is_empty() {
        return Ok(serde_json::Map::new());
    }
    if body.starts_with('{') {
        let v: serde_json::Value = serde_json::from_str(body).map_err(|e| Error::invalid("body", e.to_string()))?;
        return v.as_object().cloned().ok_or_else(|| Error::invalid("body", "JSON object of field values"));
    }
    // k=v lines (same spirit as `mnnn model record` assignments)
    let mut map = serde_json::Map::new();
    for line in body.split(['\n', ';']) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (k, v) = line.split_once('=').ok_or_else(|| Error::invalid("body", format!("`{line}` (use field=value or JSON)")))?;
        let v = v.trim();
        let val = if let Ok(n) = v.parse::<f64>() {
            json!(n)
        } else if v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("false") {
            json!(v.eq_ignore_ascii_case("true"))
        } else {
            json!(v)
        };
        map.insert(k.trim().to_string(), val);
    }
    Ok(map)
}

/// Create a project item of `kind` (for `mnnn project add --create`).
///
/// For `result`, `name` is `SOURCE:SYMBOL:PERIOD` (same as `--result`). The series must
/// already exist in the store (bars come from import — Muninn does not invent them).
pub fn create_project_item(s: &Store, kind: ItemKind, name: &str, opts: &CreateOpts, origin: &str) -> Result<Created> {
    match kind {
        ItemKind::Model => create_item(s, ShareKind::Model, name, opts, origin),
        ItemKind::LiveSearch => create_item(s, ShareKind::LiveSearch, name, opts, origin),
        ItemKind::Journal => create_item(s, ShareKind::Journal, name, opts, origin),
        ItemKind::Report => create_item(s, ShareKind::Report, name, opts, origin),
        ItemKind::Code => create_item(s, ShareKind::Code, name, opts, origin),
        ItemKind::Space => create_item(s, ShareKind::Space, name, opts, origin),
        ItemKind::Pipeline => create_item(s, ShareKind::Pipeline, name, opts, origin),
        ItemKind::RuleSet => {
            let r = s.create_ruleset(name, &opts.description, "standalone", None)?;
            s.note_action(
                &NewAction::new("ruleset.create", origin, format!("create rule set {}", r.name)).params(json!({ "ruleset": r.id, "name": r.name })),
            );
            Ok(Created { kind: "ruleset".into(), id: r.id, name: r.name })
        }
        ItemKind::Result => {
            let r = parse_series_ref(s, name)?;
            // No separate row until added to a project; the "created" handle is the series label.
            s.note_action(
                &NewAction::new("result.ready", origin, format!("series {} ready to add", r.label()))
                    .params(json!({ "source": r.source, "symbol": r.symbol.to_string(), "duration": r.duration.to_string() })),
            );
            Ok(Created { kind: "result".into(), id: r.label(), name: r.label() })
        }
    }
}

/// Parse `SOURCE:SYMBOL:PERIOD` and require the series exists (import first).
pub fn parse_series_ref(s: &Store, series: &str) -> Result<crate::store::SeriesRef> {
    let parts: Vec<&str> = series.split(':').collect();
    let [src, sym, per] = parts.as_slice() else {
        return Err(Error::invalid("result", format!("`{series}` (use SOURCE:SYMBOL:PERIOD, e.g. binance:BTC/USDT:5m)")));
    };
    let r = crate::store::SeriesRef {
        source: src.trim().to_string(),
        symbol: s.canonical_symbol(&Symbol::parse(sym)?)?,
        duration: Duration::parse(per)?,
        from: None,
        to: None,
    };
    if !s.series()?.iter().any(|x| x.source == r.source && x.symbol == r.symbol && x.duration == r.duration) {
        return Err(Error::invalid("result", format!("no series {} in the store (import bars first; Muninn does not invent series)", r.label())));
    }
    Ok(r)
}

fn create_code(s: &Store, name: &str, opts: &CreateOpts, origin: &str) -> Result<crate::store::CodeSpace> {
    let from = opts.from.as_deref();
    let base = match from {
        Some("template:ingest-connector") => Some((Lang::Rust, code::CONNECTOR_TEMPLATE.to_string())),
        Some(f) => Some(match s.find_code_space(f) {
            Ok(c) => (c.lang, c.source),
            Err(_) => {
                let b = code::parsers::builtin(f).ok_or_else(|| Error::invalid("create", format!("no code or built-in `{f}`")))?;
                (Lang::Rust, b.source.to_string())
            }
        }),
        None => None,
    };
    let lang = opts.lang.or(base.as_ref().map(|b| b.0)).unwrap_or(Lang::Math);
    let src = base.map(|b| b.1).unwrap_or_else(|| code::template(lang).to_string());
    code::create(s, origin, name, &opts.description, lang, &src, CodeInput::default())
}

fn create_space(s: &Store, name: &str, opts: &CreateOpts, origin: &str) -> Result<crate::store::Space> {
    let kind = opts.space_kind.unwrap_or(SpaceKind::Analysis);
    let main = match opts.main.as_deref().map(str::trim).filter(|m| !m.is_empty()) {
        Some(m) => s.find_code_space(m)?,
        None => {
            let main_name = format!("{name}-main");
            create_code(s, &main_name, opts, origin)?
        }
    };
    spaces::create(s, origin, name, &opts.description, kind, &main)
}

/// Share after optionally creating the item and/or the target. Create actions are recorded
/// first; then the share (same `origin`). For a net `--create-to`, pass [`NetCreate`].
#[allow(clippy::too_many_arguments)] // thin wrapper used by CLI / FFI
pub fn create_then_share(
    s: &Store,
    kind: ShareKind,
    item: &str,
    target: TargetKind,
    to: &[String],
    note: &str,
    origin: &str,
    create_item_flag: bool,
    create_to: Option<&str>,
    opts: &CreateOpts,
    net: Option<&NetCreate<'_>>,
) -> Result<(Option<Created>, Option<Created>, Shared)> {
    let mut made_item = None;
    let item_key = if create_item_flag {
        let c = create_item(s, kind, item, opts, origin)?;
        let key = c.name.clone();
        made_item = Some(c);
        key
    } else {
        item.to_string()
    };
    let mut made_to = None;
    let to_keys: Vec<String> = if let Some(name) = create_to {
        let c = create_target(s, target, name, opts, origin, net)?;
        // Prefer label/name for subsequent share lookup; net share accepts id or label.
        let key = if target == TargetKind::Net { c.id.clone() } else { c.name.clone() };
        made_to = Some(c);
        vec![key]
    } else {
        to.to_vec()
    };
    let shared = share(s, kind, &item_key, target, &to_keys, note, origin)?;
    Ok((made_item, made_to, shared))
}
