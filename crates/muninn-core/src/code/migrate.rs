//! `mnnn migrate`: code from before 2.23 into code spaces. Each code that is in no space
//! gets a space named as the code (taken: `NAME-migration`; that taken too, or too long:
//! skipped), with the code as its main and a kind inferred from how it is used; stored
//! pipelines' `code:NAME` steps become `space:…`. Offered, never automatic; idempotent.
use super::pipeline::{Graph, NodeKind};
use crate::error::Result;
use crate::store::{CodeSpace, NewAction, SpaceKind, Store, spaces::NAME_MAX};
use serde::Serialize;
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const SUFFIX: &str = "-migration";

/// The kind a migrated space would get, and why (`None`: unclear).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Inference {
    pub kind: Option<SpaceKind>,
    pub suggest: SpaceKind,
    pub why: String,
}

/// The pipelines that run a code as a step (`code:NAME` nodes).
pub fn steps_using(store: &Store, c: &CodeSpace) -> Result<Vec<String>> {
    Ok(store
        .pipelines()?
        .into_iter()
        .filter(|p| {
            p.graph.nodes.iter().any(|n| matches!(&n.kind, NodeKind::Code { space } if super::split_options(space).0.eq_ignore_ascii_case(&c.name)))
        })
        .map(|p| p.name)
        .collect())
}

pub fn infer(store: &Store, c: &CodeSpace) -> Result<Inference> {
    if let Some(b) = super::parsers::builtin(&c.name) {
        let k = if b.name == "to-bars" { SpaceKind::Transform } else { SpaceKind::Parser };
        return Ok(Inference { kind: Some(k), suggest: k, why: format!("the built-in {}", b.name) });
    }
    let steps = steps_using(store, c)?;
    let alone = !c.input.is_empty() || c.last_run.is_some();
    Ok(match (steps.is_empty(), alone) {
        (true, _) => Inference { kind: Some(SpaceKind::Analysis), suggest: SpaceKind::Analysis, why: "runs on its own".into() },
        (false, false) => Inference {
            kind: Some(SpaceKind::Transform),
            suggest: SpaceKind::Transform,
            why: format!("a pipeline step (pipeline {})", steps.join(", ")),
        },
        (false, true) => Inference {
            kind: None,
            suggest: SpaceKind::Transform,
            why: format!("a pipeline step (pipeline {}) that also runs on its own", steps.join(", ")),
        },
    })
}

#[derive(Debug, Clone)]
pub struct Options {
    /// Only these codes (names or ids); empty: every code in no space.
    pub names: Vec<String>,
    /// The kind for unclear ones.
    pub kind: Option<SpaceKind>,
    pub dry_run: bool,
    pub rewrite_pipelines: bool,
    pub backup: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { names: vec![], kind: None, dry_run: false, rewrite_pipelines: true, backup: true }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Created {
    pub code: String,
    pub space: String,
    pub kind: SpaceKind,
    /// Named `NAME-migration` (the name was taken).
    pub renamed: bool,
    pub approved: bool,
    pub why: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Skipped {
    pub code: String,
    pub why: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Report {
    pub created: Vec<Created>,
    pub skipped: Vec<Skipped>,
    /// Pipelines rewritten (`code:` → `space:`): name and the new version.
    pub pipelines: Vec<(String, i64)>,
    pub backup: Option<PathBuf>,
    pub dry_run: bool,
}

impl Report {
    pub fn renamed(&self) -> Vec<&Created> {
        self.created.iter().filter(|c| c.renamed).collect()
    }
    pub fn is_empty(&self) -> bool {
        self.created.is_empty() && self.skipped.is_empty() && self.pipelines.is_empty()
    }
}

/// How many codes are in no space (what migrate would look at).
pub fn pending(store: &Store) -> Result<usize> {
    let mut n = 0;
    for c in store.code_spaces()? {
        if !store.code_in_a_space(&c.id)? {
            n += 1;
        }
    }
    Ok(n)
}

/// Rewrite a graph's `code:OLD` steps and sources to `space:NEW` (options kept).
pub fn rewrite(g: &Graph, map: &HashMap<String, String>) -> Graph {
    let mut g = g.clone();
    for n in &mut g.nodes {
        let new = match &n.kind {
            NodeKind::Code { space } => {
                let (name, _) = super::split_options(space);
                map.get(&name.to_lowercase()).map(|to| {
                    let opts = space.split_once('?').map(|(_, q)| format!("?{q}")).unwrap_or_default();
                    NodeKind::Space { name: format!("{to}{opts}") }
                })
            }
            NodeKind::Source { source } => source
                .strip_prefix("code:")
                .and_then(|x| map.get(&x.trim().to_lowercase()))
                .map(|to| NodeKind::Source { source: format!("space:{to}") }),
            _ => None,
        };
        if let Some(k) = new {
            n.kind = k;
        }
    }
    g
}

pub fn migrate(
    store: &Store,
    home: &Path,
    origin: &str,
    opts: &Options,
    ask: &mut dyn FnMut(&CodeSpace, &Inference) -> Result<Option<SpaceKind>>,
) -> Result<Report> {
    let codes: Vec<CodeSpace> =
        if opts.names.is_empty() { store.code_spaces()? } else { opts.names.iter().map(|n| store.find_code_space(n)).collect::<Result<_>>()? };
    let mut rep = Report { dry_run: opts.dry_run, ..Default::default() };
    let mut taken: Vec<String> = store.spaces()?.into_iter().map(|s| s.name.to_lowercase()).collect();
    let mut plan: Vec<(CodeSpace, String, SpaceKind, bool, String)> = vec![];
    for c in codes {
        if store.code_in_a_space(&c.id)? {
            if !opts.names.is_empty() {
                rep.skipped.push(Skipped { code: c.name.clone(), why: "already in a code space".into() });
            }
            continue;
        }
        let base = c.name.trim().replace(':', "-");
        let (name, renamed) = if !taken.contains(&base.to_lowercase()) {
            (base.clone(), false)
        } else {
            let m = format!("{base}{SUFFIX}");
            if taken.contains(&m.to_lowercase()) {
                rep.skipped.push(Skipped { code: c.name.clone(), why: format!("`{base}` and `{m}` are both taken") });
                continue;
            }
            if m.chars().count() > NAME_MAX {
                rep.skipped.push(Skipped { code: c.name.clone(), why: format!("name too long for `{base}{SUFFIX}` (over {NAME_MAX} characters)") });
                continue;
            }
            (m, true)
        };
        let inf = infer(store, &c)?;
        let (kind, why) = match inf.kind {
            Some(k) => (k, inf.why.clone()),
            None => match opts.kind {
                Some(k) => (k, format!("{}; --kind {k}", inf.why)),
                None if opts.dry_run => {
                    rep.skipped.push(Skipped { code: c.name.clone(), why: format!("{}; would ask for its kind (likely {})", inf.why, inf.suggest) });
                    continue;
                }
                None => match ask(&c, &inf)? {
                    Some(k) => (k, format!("{}; chose {k}", inf.why)),
                    None => {
                        rep.skipped.push(Skipped { code: c.name.clone(), why: format!("{}; no kind given (use --kind)", inf.why) });
                        continue;
                    }
                },
            },
        };
        taken.push(name.to_lowercase());
        plan.push((c, name, kind, renamed, why));
    }
    let map: HashMap<String, String> = plan.iter().map(|(c, n, ..)| (c.name.to_lowercase(), n.clone())).collect();
    let pipes: Vec<_> = if opts.rewrite_pipelines {
        store
            .pipelines()?
            .into_iter()
            .filter_map(|p| {
                let g = rewrite(&p.graph, &map);
                (g != p.graph).then_some((p, g))
            })
            .collect()
    } else {
        vec![]
    };
    if opts.dry_run {
        for (c, name, kind, renamed, why) in plan {
            rep.created.push(Created { code: c.name.clone(), space: name, kind, renamed, approved: super::approved(&c), why });
        }
        rep.pipelines = pipes.iter().map(|(p, _)| (p.name.clone(), p.version + 1)).collect();
        return Ok(rep);
    }
    if plan.is_empty() && pipes.is_empty() {
        return Ok(rep);
    }
    if opts.backup {
        let dir = home.join("backups");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("migrate-{}.json", chrono::Utc::now().format("%Y%m%dT%H%M%S%3fZ")));
        let body = json!({
            "format": "muninn-migrate-1",
            "code": plan.iter().map(|p| &p.0).collect::<Vec<_>>(),
            "pipelines": pipes.iter().map(|(p, _)| json!({ "id": p.id, "name": p.name, "version": p.version, "graph": p.graph })).collect::<Vec<_>>(),
        });
        std::fs::write(&path, serde_json::to_vec_pretty(&body)?)?;
        rep.backup = Some(path);
    }
    for (c, name, kind, renamed, why) in plan {
        let s = super::spaces::create(store, origin, &name, &c.description, kind, &c)?;
        // nothing that ran before stops running: an approved code's space is approved
        let approved = super::approved(&c) && super::spaces::approve(store, origin, &s, false).is_ok();
        rep.created.push(Created { code: c.name.clone(), space: s.name, kind, renamed, approved, why });
    }
    for (p, g) in pipes {
        let np = store.update_pipeline(&p.id, None, None, Some(&g))?;
        store.note_action(
            &NewAction::new("pipeline.edit", origin, format!("pipeline {}: code: steps now space: (migrate)", p.name))
                .params(json!({ "id": p.id, "name": p.name, "version": np.version })),
        );
        rep.pipelines.push((p.name, np.version));
    }
    store.note_action(
        &NewAction::new(
            "migrate",
            origin,
            format!(
                "migrate: {} code space(s) created, {} skipped, {} pipeline(s) rewritten",
                rep.created.len(),
                rep.skipped.len(),
                rep.pipelines.len()
            ),
        )
        .params(json!({ "created": rep.created, "skipped": rep.skipped, "pipelines": rep.pipelines, "backup": rep.backup })),
    );
    Ok(rep)
}

/// Offered once: a marker in MUNINN_HOME says the notice was shown.
pub fn offer_once(store: &Store, home: &Path) -> Option<String> {
    let marker = home.join("migrate-offered");
    if marker.exists() {
        return None;
    }
    let n = pending(store).ok().filter(|n| *n > 0)?;
    let _ = std::fs::write(&marker, format!("{}\n", crate::store::now_ms()));
    Some(format!(
        "{n} code item(s) aren't in a code space yet; `mnnn migrate --dry-run` shows what migrating would do (nothing changes until you run `mnnn migrate`)"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{CodeInput, Lang};

    fn no_ask(_: &CodeSpace, _: &Inference) -> Result<Option<SpaceKind>> {
        Ok(None)
    }

    #[test]
    fn migrate_names_skips_idempotent_and_rewrites() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open_in_memory().unwrap();
        let a = s.create_code_space("alpha", "", Lang::Math, "x = close", CodeInput::default()).unwrap();
        let b = s.create_code_space("beta", "", Lang::Math, "x = close", CodeInput::default()).unwrap();
        let c = s.create_code_space("gamma", "", Lang::Math, "x = close", CodeInput::default()).unwrap();
        let other = s.create_code_space("other", "", Lang::Math, "y = 1", CodeInput::default()).unwrap();
        // beta's name is taken by a space; gamma's and gamma-migration both are
        s.create_space("beta", "", SpaceKind::Analysis, &other).unwrap();
        let o2 = s.create_code_space("o2", "", Lang::Math, "y = 1", CodeInput::default()).unwrap();
        let o3 = s.create_code_space("o3", "", Lang::Math, "y = 1", CodeInput::default()).unwrap();
        s.create_space("gamma", "", SpaceKind::Analysis, &o2).unwrap();
        s.create_space("Gamma-migration", "", SpaceKind::Analysis, &o3).unwrap();
        crate::code::approve(&s, "t", &a).unwrap();
        let g = Graph::parse(&["src=-".into(), "a=code:alpha?x=1".into(), "out=model:bars".into()], &["src>a".into(), "a>out".into()]).unwrap();
        s.create_pipeline("p", "", &g).unwrap();

        let dry = migrate(&s, dir.path(), "t", &Options { dry_run: true, ..Default::default() }, &mut no_ask).unwrap();
        assert_eq!(dry.created.len(), 2);
        assert_eq!(s.spaces().unwrap().len(), 3, "a dry run changes nothing");
        assert_eq!(dry.pipelines, vec![("p".to_string(), 2)]);

        let r = migrate(&s, dir.path(), "t", &Options::default(), &mut no_ask).unwrap();
        let names: Vec<_> = r.created.iter().map(|c| (c.space.as_str(), c.renamed)).collect();
        assert!(names.contains(&("alpha", false)) && names.contains(&("beta-migration", true)), "{names:?}");
        assert_eq!(r.skipped.len(), 1);
        assert_eq!(r.skipped[0].code, "gamma");
        assert!(r.backup.as_ref().unwrap().exists());
        let alpha = s.find_space("alpha").unwrap();
        assert_eq!(alpha.main_code_id, a.id);
        assert_eq!(alpha.kind, SpaceKind::Transform, "used only as a step");
        assert!(crate::code::spaces::approved(&s, &alpha), "approval carried over");
        assert!(!crate::code::spaces::approved(&s, &s.find_space("beta-migration").unwrap()));
        let p = s.find_pipeline("p").unwrap();
        assert_eq!(p.version, 2);
        assert!(p.graph.nodes.iter().any(|n| n.kind == NodeKind::Space { name: "alpha?x=1".into() }));
        let _ = (b, c);

        let again = migrate(&s, dir.path(), "t", &Options::default(), &mut no_ask).unwrap();
        assert!(again.created.is_empty() && again.pipelines.is_empty(), "idempotent");
        assert_eq!(again.skipped.len(), 1, "gamma is still skipped");
    }

    #[test]
    fn too_long_names_skip() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open_in_memory().unwrap();
        let long = "x".repeat(78);
        let a = s.create_code_space(&long, "", Lang::Math, "x = close", CodeInput::default()).unwrap();
        let o = s.create_code_space("o", "", Lang::Math, "x = close", CodeInput::default()).unwrap();
        s.create_space(&long, "", SpaceKind::Analysis, &o).unwrap();
        let r = migrate(&s, dir.path(), "t", &Options::default(), &mut no_ask).unwrap();
        assert!(r.skipped.iter().any(|k| k.code == a.name && k.why.contains("too long")));
    }
}
