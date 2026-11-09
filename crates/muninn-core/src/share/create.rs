//! Create-then-share: make an item or a share target in the same place a share is added.
//!
//! Used by `mnnn share --create` / `--create-to`, org resource/project/give `--create`,
//! `project add --create`, `net share --create`, review request `--create`, and the TUI
//! "+ New KIND…" rows. Create is recorded in history before the share; cancel never creates.
use crate::code::{self, spaces};
use crate::error::{Error, Result};
use crate::store::{
    CodeInput, Design, EntryFormat, ItemKind, Lang, NewAction, SearchSpec, ShareKind, SpaceKind, Store,
};
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
                &NewAction::new("journal.create", origin, format!("create journal {}", j.name))
                    .params(json!({ "journal": j.id, "name": j.name })),
            );
            Ok(Created::item(kind, j.id, j.name))
        }
        ShareKind::Project => {
            let p = s.create_project(name, &opts.description)?;
            s.note_action(
                &NewAction::new("project.create", origin, format!("create project {}", p.name))
                    .params(json!({ "project": p.id, "name": p.name })),
            );
            Ok(Created::item(kind, p.id, p.name))
        }
        ShareKind::Report => {
            let r = s.create_report(name, &opts.description)?;
            s.note_action(
                &NewAction::new("report.create", origin, format!("create report {}", r.name))
                    .params(json!({ "report": r.id, "name": r.name })),
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
                &NewAction::new("pipeline.create", origin, format!("create pipeline {}", p.name))
                    .params(json!({ "pipeline": p.id, "name": p.name })),
            );
            Ok(Created::item(kind, p.id, p.name))
        }
    }
}

/// Create a share target (project, journal, code, or code space). Net exchanges are not
/// created here — open a key exchange first (`mnnn net exchange`).
pub fn create_target(s: &Store, target: TargetKind, name: &str, opts: &CreateOpts, origin: &str) -> Result<Created> {
    match target {
        TargetKind::Project => create_item(s, ShareKind::Project, name, opts, origin).map(|c| Created::target(target, c.id, c.name)),
        TargetKind::Journal => create_item(s, ShareKind::Journal, name, opts, origin).map(|c| Created::target(target, c.id, c.name)),
        TargetKind::Code => create_item(s, ShareKind::Code, name, opts, origin).map(|c| Created::target(target, c.id, c.name)),
        TargetKind::Space => create_item(s, ShareKind::Space, name, opts, origin).map(|c| Created::target(target, c.id, c.name)),
        TargetKind::Net => Err(Error::invalid(
            "create",
            "a net target is an exchange; create one with `mnnn net exchange` (or share with every exchange, leaving TO empty)",
        )),
    }
}

/// Create a project item of `kind` (for `mnnn project add --create`). Series results are not
/// created this way. Rule → a rule set named `name` with no rules yet.
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
                &NewAction::new("ruleset.create", origin, format!("create rule set {}", r.name))
                    .params(json!({ "ruleset": r.id, "name": r.name })),
            );
            Ok(Created { kind: "ruleset".into(), id: r.id, name: r.name })
        }
        ItemKind::Result => Err(Error::invalid("create", "a result is a series reference; use --result SERIES, not --create")),
    }
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
            // New space asks for its main: create a main code when none was named.
            let main_name = format!("{name}-main");
            create_code(s, &main_name, opts, origin)?
        }
    };
    spaces::create(s, origin, name, &opts.description, kind, &main)
}

/// Share after optionally creating the item and/or the target. Create actions are recorded
/// first; then the share (same `origin`).
#[allow(clippy::too_many_arguments)] // thin wrapper used by CLI share
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
        let c = create_target(s, target, name, opts, origin)?;
        let key = c.name.clone();
        made_to = Some(c);
        vec![key]
    } else {
        to.to_vec()
    };
    let shared = share(s, kind, &item_key, target, &to_keys, note, origin)?;
    Ok((made_item, made_to, shared))
}
