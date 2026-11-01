//! Pipeline files: export a pipeline with its code (by trust hash) and its sink models
//! (by name, optionally with their templates embedded), and import one with a plan first.
use super::{Graph, NodeKind};
use crate::code::{trust_hash, trust_hash_of};
use crate::error::{Error, Result};
use crate::store::templates::{ImportOpts, Template};
use crate::store::{Lang, Store, pipelines::Pipeline};
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
        Ok(e)
    }
}

/// Export a pipeline; `templates` embeds the sink models' designs.
pub fn export(store: &Store, key: &str, templates: bool) -> Result<Export> {
    let p = store.find_pipeline(key)?;
    let mut code = vec![];
    for n in p.graph.code_spaces() {
        let c = store.find_code_space(&n)?;
        code.push(CodeEntry { hash: trust_hash(&c)?, name: c.name, description: c.description, lang: c.lang, source: c.source });
    }
    let mut models = vec![];
    for m in p.graph.sinks() {
        let template = if templates { Some(store.export_template(&m, false)?) } else { None };
        models.push(ModelEntry { name: m, template });
    }
    Ok(Export { format: FORMAT.into(), version: VERSION, name: p.name, description: p.description, graph: p.graph, code, models })
}

/// What importing would do.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ImportPlan {
    pub name: String,
    /// Replacing the graph of the pipeline of that name (a new version).
    pub merge: bool,
    /// (space in the file, space it lands as, what happens)
    pub code: Vec<(String, String, String)>,
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
        v.extend(self.models.iter().map(|(m, w)| format!("  model {m}: {w}")));
        v.extend(self.problems.iter().map(|p| format!("  ! {p}")));
        v
    }
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
    for c in &e.code {
        let (to, what) = match store.find_code_space(&c.name) {
            Ok(have) if trust_hash(&have)? == c.hash => (c.name.clone(), "the same code is here already: used as is".to_string()),
            Ok(_) => (free_name(store, &c.name), "different code of that name is here: created alongside, needs approval".to_string()),
            Err(_) => (c.name.clone(), "created; needs approval before it runs".to_string()),
        };
        plan.code.push((c.name.clone(), to, what));
    }
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
    for (c, (from, to, _)) in e.code.iter().zip(&plan.code) {
        if store.find_code_space(to).is_err() {
            store.create_code_space(to, &c.description, c.lang, &c.source, Default::default())?;
        }
        if from != to {
            for n in &mut g.nodes {
                if let NodeKind::Code { space } = &mut n.kind
                    && space == from
                {
                    *space = to.clone();
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
