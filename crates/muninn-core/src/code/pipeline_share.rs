//! Pipeline files: export a pipeline with its code (by trust hash) and its sink models
//! (by name, optionally with their templates embedded), and import one with a plan first.
use super::{Graph, NodeKind};
use crate::code::{trust_hash, trust_hash_of};
use crate::error::{Error, Result};
use crate::store::templates::{ImportOpts, Template};
use crate::store::{CodeSpace, Lang, SpaceKind, Store, pipelines::Pipeline};
use serde::{Deserialize, Serialize};

pub const FORMAT: &str = "muninn-pipeline";
pub const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodeEntry {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub lang: Lang,
    pub source: String,
    /// The trust hash of lang + source: what an importer approves.
    pub hash: String,
}

/// A code space the pipeline runs (or one main calls): its line-up, by code name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpaceEntry {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub kind: SpaceKind,
    /// The main's code name (in `code`).
    pub main: String,
    #[serde(default)]
    pub members: Vec<MemberEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemberEntry {
    pub alias: String,
    /// The code name (in `code`).
    pub code: String,
    /// Pinned (to the code as it is in this file).
    #[serde(default)]
    pub pinned: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelEntry {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<Template>,
}

/// A pipeline file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Export {
    pub format: String,
    pub version: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub graph: Graph,
    #[serde(default)]
    pub code: Vec<CodeEntry>,
    /// Code spaces (2.23+), their members' code in `code`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spaces: Vec<SpaceEntry>,
    #[serde(default)]
    pub models: Vec<ModelEntry>,
}

impl Export {
    pub fn parse(bytes: &[u8]) -> Result<Export> {
        let e: Export = serde_json::from_slice(bytes).map_err(|e| Error::invalid("pipeline", format!("not a pipeline file ({e})")))?;
        if e.format != FORMAT {
            return Err(Error::invalid("pipeline", format!("format `{}`: expected {FORMAT}", e.format)));
        }
        if e.version > VERSION {
            return Err(Error::invalid("pipeline", format!("version {} is newer than this muninn reads ({VERSION})", e.version)));
        }
        for c in &e.code {
            if trust_hash_of(c.lang, &c.source)? != c.hash {
                return Err(Error::invalid("pipeline", format!("code space {}: its source does not match its hash", c.name)));
            }
        }
        for s in &e.spaces {
            for c in std::iter::once(&s.main).chain(s.members.iter().map(|m| &m.code)) {
                if !e.code.iter().any(|x| &x.name == c) {
                    return Err(Error::invalid("pipeline", format!("code space {}: its code {c} is not in the file", s.name)));
                }
            }
        }
        Ok(e)
    }
}

pub(crate) fn code_entry(c: &CodeSpace) -> Result<CodeEntry> {
    Ok(CodeEntry { hash: trust_hash(c)?, name: c.name.clone(), description: c.description.clone(), lang: c.lang, source: c.source.clone() })
}

/// The spaces a graph runs and the spaces their mains import, with every member's code.
pub(crate) fn bundle_spaces(store: &Store, roots: Vec<String>, code: &mut Vec<CodeEntry>) -> Result<Vec<SpaceEntry>> {
    let mut todo = roots;
    let mut out: Vec<SpaceEntry> = vec![];
    while let Some(name) = todo.pop() {
        if out.iter().any(|s| s.name.eq_ignore_ascii_case(&name)) {
            continue;
        }
        let s = store.find_space(&name)?;
        let mut members = vec![];
        let mut main = String::new();
        for m in store.space_members(&s.id)? {
            let c = store.code_space(&m.code_id)?.ok_or_else(|| Error::invalid("space", format!("{}: a member's code is missing", s.name)))?;
            if !code.iter().any(|x| x.name == c.name) {
                code.push(code_entry(&c)?);
            }
            if c.id == s.main_code_id {
                main = c.name.clone();
                todo.extend(crate::code::spaces::imports(&c.source).spaces);
            }
            members.push(MemberEntry { alias: m.alias, code: c.name, pinned: m.pin_hash.is_some() });
        }
        out.push(SpaceEntry { name: s.name, description: s.description, kind: s.kind, main, members });
    }
    out.reverse();
    Ok(out)
}

/// Export a pipeline; `templates` embeds the sink models' designs.
pub fn export(store: &Store, key: &str, templates: bool) -> Result<Export> {
    let p = store.find_pipeline(key)?;
    let mut code = vec![];
    for n in p.graph.code_spaces() {
        code.push(code_entry(&store.find_code_space(&n)?)?);
    }
    let spaces = bundle_spaces(store, p.graph.spaces(), &mut code)?;
    let mut models = vec![];
    for m in p.graph.sinks() {
        let template = if templates { Some(store.export_template(&m, false)?) } else { None };
        models.push(ModelEntry { name: m, template });
    }
    Ok(Export { format: FORMAT.into(), version: VERSION, name: p.name, description: p.description, graph: p.graph, code, spaces, models })
}

/// What importing would do.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ImportPlan {
    pub name: String,
    /// Replacing the graph of the pipeline of that name (a new version).
    pub merge: bool,
    /// (space in the file, space it lands as, what happens)
    pub code: Vec<(String, String, String)>,
    /// (space in the file, space it lands as, what happens)
    #[serde(default)]
    pub spaces: Vec<(String, String, String)>,
    /// (model, what happens)
    pub models: Vec<(String, String)>,
    /// What will still be wrong (missing searches, models without templates…).
    pub problems: Vec<String>,
    /// Land as a draft (from a peer): the pipeline and the models it makes wait for approval.
    pub draft: bool,
}

impl ImportPlan {
    pub fn lines(&self) -> Vec<String> {
        let mut v = vec![format!("pipeline {}: {}", self.name, if self.merge { "a new version of the existing one" } else { "new" })];
        v.extend(self.code.iter().map(|(a, b, w)| if a == b { format!("  code {a}: {w}") } else { format!("  code {a} → {b}: {w}") }));
        v.extend(self.spaces.iter().map(|(a, b, w)| if a == b { format!("  space {a}: {w}") } else { format!("  space {a} → {b}: {w}") }));
        v.extend(self.models.iter().map(|(m, w)| format!("  model {m}: {w}")));
        v.extend(self.problems.iter().map(|p| format!("  ! {p}")));
        v
    }
}

fn free_space_name(store: &Store, base: &str) -> Result<String> {
    let mut name = format!("{base}-imported");
    let mut i = 2;
    while store.space_name_taken(&name)? {
        name = format!("{base}-imported-{i}");
        i += 1;
    }
    Ok(name)
}

fn free_name(store: &Store, base: &str) -> String {
    let mut name = format!("{base} (imported)");
    let mut i = 2;
    while store.find_code_space(&name).is_ok() {
        name = format!("{base} (imported {i})");
        i += 1;
    }
    name
}

/// The plan; `name` renames, `merge` replaces the graph of a pipeline of the same name.
pub fn plan_import(store: &Store, e: &Export, name: Option<&str>, merge: bool) -> Result<ImportPlan> {
    let name = name.unwrap_or(&e.name).trim().to_string();
    let exists = store.find_pipeline(&name).is_ok();
    if exists && !merge {
        return Err(Error::invalid(
            "pipeline",
            format!("a pipeline `{name}` exists already: import it --name OTHER, or --merge to make this its new version"),
        ));
    }
    let mut plan = ImportPlan { name, merge: exists, ..Default::default() };
    plan.code = plan_code(store, &e.code)?;
    plan.spaces = plan_spaces(store, &e.spaces, &plan.code)?;
    for m in &e.models {
        let what = if store.find_model(&m.name).is_ok() {
            "exists: rows land in it".to_string()
        } else if m.template.is_some() {
            "missing: created from the embedded template".to_string()
        } else {
            plan.problems.push(format!("model {} is not here and the file has no template for it", m.name));
            "missing".to_string()
        };
        plan.models.push((m.name.clone(), what));
    }
    for n in &e.graph.nodes {
        if let NodeKind::Source { source } = &n.kind
            && source != "-"
        {
            let missing = match crate::code::pipe::Source::parse(source) {
                Ok(crate::code::pipe::Source::Search(x)) => store.find_live_search(&x).is_err(),
                Ok(crate::code::pipe::Source::Journal(x)) => store.find_journal(&x).is_err(),
                _ => false,
            };
            if missing {
                plan.problems.push(format!("source `{}` ({source}) is not here: create it before running", n.id));
            }
        }
    }
    Ok(plan)
}

/// Import: code spaces created unapproved, missing models from their templates, then the pipeline.
pub fn import(store: &Store, e: &Export, plan: &ImportPlan) -> Result<Pipeline> {
    let mut g = e.graph.clone();
    land_code(store, &e.code, &plan.code)?;
    for (from, to, _) in &plan.code {
        if from != to {
            for n in &mut g.nodes {
                if let NodeKind::Code { space } = &mut n.kind
                    && super::super::split_options(space).0 == *from
                {
                    *space = match space.split_once('?') {
                        Some((_, q)) => format!("{to}?{q}"),
                        None => to.clone(),
                    };
                }
            }
        }
    }
    land_spaces(store, &e.spaces, &plan.code, &plan.spaces)?;
    for (from, to, _) in &plan.spaces {
        if from != to {
            for n in &mut g.nodes {
                match &mut n.kind {
                    NodeKind::Space { name } if super::super::split_options(name).0 == *from => {
                        *name = match name.split_once('?') {
                            Some((_, q)) => format!("{to}?{q}"),
                            None => to.clone(),
                        };
                    }
                    NodeKind::Source { source } if source.strip_prefix("space:").is_some_and(|x| super::super::split_options(x).0 == *from) => {
                        *source = match source.split_once('?') {
                            Some((_, q)) => format!("space:{to}?{q}"),
                            None => format!("space:{to}"),
                        };
                    }
                    _ => {}
                }
            }
        }
    }
    for m in &e.models {
        if store.find_model(&m.name).is_err()
            && let Some(t) = &m.template
        {
            store.import_template(t, &ImportOpts { name: Some(m.name.clone()), draft: plan.draft, ..Default::default() }, false)?;
        }
    }
    let mut p = match store.find_pipeline(&plan.name) {
        Ok(p) => store.update_pipeline(&p.id, None, Some(&e.description), Some(&g))?,
        Err(_) => store.create_pipeline(&plan.name, &e.description, &g)?,
    };
    if plan.draft {
        store.set_pipeline_draft(&p.id, true)?;
        p.draft = true;
    }
    Ok(p)
}

/// What landing code does: (name in the file, name it lands as, what happens).
pub(crate) fn plan_code(store: &Store, entries: &[CodeEntry]) -> Result<Vec<(String, String, String)>> {
    let mut out = vec![];
    for c in entries {
        let (to, what) = match store.find_code_space(&c.name) {
            Ok(have) if trust_hash(&have)? == c.hash => (c.name.clone(), "the same code is here already: used as is".to_string()),
            Ok(_) => (free_name(store, &c.name), "different code of that name is here: created alongside, needs approval".to_string()),
            Err(_) => (c.name.clone(), "created; needs approval before it runs".to_string()),
        };
        out.push((c.name.clone(), to, what));
    }
    Ok(out)
}

/// What landing spaces does: the same line-up is reused, a clash lands as `NAME-imported`.
pub(crate) fn plan_spaces(store: &Store, spaces: &[SpaceEntry], code: &[(String, String, String)]) -> Result<Vec<(String, String, String)>> {
    let mut out = vec![];
    for s in spaces {
        let landed = |c: &str| code.iter().find(|(a, _, _)| a == c).map(|(_, b, _)| b.clone()).unwrap_or_else(|| c.to_string());
        let same = match store.find_space(&s.name) {
            Ok(have) => {
                let want: Vec<(String, String)> = s.members.iter().map(|m| (m.alias.to_lowercase(), landed(&m.code))).collect();
                let mut got = vec![];
                for m in store.space_members(&have.id)? {
                    if let Some(c) = store.code_space(&m.code_id)? {
                        got.push((m.alias.to_lowercase(), c.name));
                    }
                }
                let main = store.code_space(&have.main_code_id)?.map(|c| c.name);
                let (mut a, mut b) = (want, got);
                a.sort();
                b.sort();
                Some(a == b && main.as_deref() == Some(landed(&s.main).as_str()) && have.kind == s.kind)
            }
            Err(_) => None,
        };
        let (to, what) = match same {
            Some(true) => (s.name.clone(), "the same line-up is here already: used as is".to_string()),
            Some(false) => {
                (free_space_name(store, &s.name)?, "a different space of that name is here: created alongside, needs approval".to_string())
            }
            None => (s.name.clone(), "created; needs approval before it runs".to_string()),
        };
        out.push((s.name.clone(), to, what));
    }
    Ok(out)
}

/// Create the code that isn't here (unapproved).
pub(crate) fn land_code(store: &Store, entries: &[CodeEntry], plan: &[(String, String, String)]) -> Result<()> {
    for (c, (_, to, _)) in entries.iter().zip(plan) {
        if store.find_code_space(to).is_err() {
            store.create_code_space(to, &c.description, c.lang, &c.source, Default::default())?;
        }
    }
    Ok(())
}

/// Create the spaces that aren't here: unapproved, members by their landed code, pins kept.
pub(crate) fn land_spaces(store: &Store, spaces: &[SpaceEntry], code: &[(String, String, String)], plan: &[(String, String, String)]) -> Result<()> {
    for (s, (_, to, _)) in spaces.iter().zip(plan) {
        if !store.space_name_taken(to)? {
            let landed = |c: &str| -> Result<CodeSpace> {
                let n = code.iter().find(|(a, _, _)| a == c).map(|(_, b, _)| b.as_str()).unwrap_or(c);
                store.find_code_space(n)
            };
            let main = landed(&s.main)?;
            let sp = store.create_space(to, &s.description, s.kind, &main)?;
            for m in &s.members {
                let c = landed(&m.code)?;
                let pin = if m.pinned { Some(trust_hash(&c)?) } else { None };
                if c.id == main.id {
                    if let Some(h) = pin {
                        store.set_member_pin(&sp.id, &c.id, Some(&h))?;
                    }
                } else {
                    store.add_space_member(&sp.id, &c, Some(&m.alias), pin.as_deref())?;
                }
            }
        }
    }
    Ok(())
}
