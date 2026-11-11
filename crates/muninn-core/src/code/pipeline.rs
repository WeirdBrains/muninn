//! Pipelines as graphs (DAGs): sources, code spaces, views and filters, ending in model
//! tables. A node's output is computed once and goes to every node downstream (fan-out);
//! a node with several upstreams waits for all of them and gets them as named frames
//! (fan-in: `host.input()` is the first, `host.input_named(NAME)` any). Saves (tees) hang
//! off any node and the data flows on. See `docs/pipelines.md`.
//!
//! The graph, its syntax, checks, order and runner are `weirdnet-pipelines`; this module
//! is Muninn's side: the node kinds and what they mean, name checks against the store,
//! and the host a run goes through (sources, code spaces, saves, approvals, the actions it
//! records).
use super::CodeEnv;
use super::pipe::{self, Sink, Source};
use crate::error::{Error, Result};
use crate::store::{NewAction, Store, pipelines::Pipeline};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use weirdnet_pipelines as wp;

pub use wp::frame::union;
pub use wp::{Edge, NodeRun, PipelineRun, Problem};

/// A pipeline graph over Muninn's node kinds, as stored (`pipelines.graph`) and exported.
pub type Graph = wp::Graph<NodeKind>;
pub type Node = wp::Node<NodeKind>;

/// What a node does.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NodeKind {
    /// Data in: `search:NAME`, `journal:NAME`, `code:SPACE`, a model query, or `-` (stdin).
    Source { source: String },
    /// One code run over its inputs (deprecated in 2.23; use `space:`).
    Code { space: String },
    /// A code space (its main) run over its inputs.
    Space { name: String },
    /// Shows what passes (a table or a chart of these columns) and passes it on.
    View {
        view: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        columns: Vec<String>,
    },
    /// Keeps the rows a live search matches.
    Filter { search: String },
    /// A leaf: the rows land in this model table.
    Sink { model: String },
}

impl NodeKind {
    /// `search:NAME` · `journal:NAME` · `-` · `source:ANY SOURCE` (sources), `code:SPACE`,
    /// `view:table|chart[:col,col]`, `filter:SEARCH`, `model:NAME` (a sink).
    pub fn parse(s: &str) -> Result<NodeKind> {
        let s = s.trim();
        if s == "-" || s.eq_ignore_ascii_case("stdin") {
            return Ok(NodeKind::Source { source: "-".into() });
        }
        let (k, rest) = s.split_once(':').map(|(k, r)| (k.trim().to_ascii_lowercase(), r.trim().to_string())).unwrap_or_default();
        let need = |w: &str| if rest.is_empty() { Err(Error::invalid("pipeline", format!("`{w}:` needs a value"))) } else { Ok(rest.clone()) };
        Ok(match k.as_str() {
            "search" | "journal" => {
                Source::parse(s)?;
                NodeKind::Source { source: s.to_string() }
            }
            "source" => match Source::parse(s)? {
                // `source:NAME`: a source's stored data, as it arrived
                Source::Data(n) => NodeKind::Source { source: format!("source:{n}") },
                _ => NodeKind::Source { source: need("source")? },
            },
            "code" => NodeKind::Code { space: need("code")? },
            "space" => NodeKind::Space { name: need("space")? },
            "view" => {
                let v = need("view")?;
                let (view, cols) = v.split_once(':').map(|(a, b)| (a.to_string(), b.to_string())).unwrap_or((v, String::new()));
                if !["table", "chart"].contains(&view.as_str()) {
                    return Err(Error::invalid("pipeline", format!("view `{view}`: table or chart")));
                }
                NodeKind::View { view, columns: cols.split(',').map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).collect() }
            }
            "filter" => NodeKind::Filter { search: need("filter")? },
            "model" | "sink" => NodeKind::Sink { model: need("model")? },
            _ => {
                return Err(Error::invalid(
                    "pipeline",
                    format!("`{s}`: a node is search:/journal:/source:…/- , space:, code:, view:, filter: or model:"),
                ));
            }
        })
    }

    pub fn text(&self) -> String {
        match self {
            NodeKind::Source { source }
                if source.starts_with("search:") || source.starts_with("journal:") || source.starts_with("source:") || source == "-" =>
            {
                source.clone()
            }
            NodeKind::Source { source } => format!("source:{source}"),
            NodeKind::Code { space } => format!("code:{space}"),
            NodeKind::Space { name } => format!("space:{name}"),
            NodeKind::View { view, columns } if columns.is_empty() => format!("view:{view}"),
            NodeKind::View { view, columns } => format!("view:{view}:{}", columns.join(",")),
            NodeKind::Filter { search } => format!("filter:{search}"),
            NodeKind::Sink { model } => format!("model:{model}"),
        }
    }
}

impl wp::Kind for NodeKind {
    type Error = Error;
    const EXAMPLE: &'static str = "src=search:usdt";
    fn parse(s: &str) -> Result<NodeKind> {
        NodeKind::parse(s)
    }
    fn text(&self) -> String {
        NodeKind::text(self)
    }
    fn role(&self) -> wp::Role {
        match self {
            NodeKind::Source { .. } => wp::Role::Source,
            NodeKind::Sink { .. } => wp::Role::Sink,
            _ => wp::Role::Step,
        }
    }
    /// Tees are saves: `journal:NAME`, `model:NAME`, `project:…`, `org:…`, `net…`.
    fn check_tee(t: &str) -> Result<()> {
        Sink::parse(t).map(|_| ())
    }
    /// A sink lands in its model table.
    fn target(&self) -> String {
        match self {
            NodeKind::Sink { model } => model.clone(),
            o => NodeKind::text(o),
        }
    }
}

/// Muninn's checks and lists over a graph.
pub trait GraphExt {
    /// What is wrong with the graph (empty: valid). With a store, the names are checked too.
    fn problems(&self, store: Option<&Store>) -> Vec<String>;
    /// The code spaces (`space:NAME` nodes and sources) the graph runs, in order.
    fn spaces(&self) -> Vec<String>;
    /// The single codes (`code:NAME` nodes and sources) it runs (deprecated in 2.23).
    fn code_spaces(&self) -> Vec<String>;
    /// The model tables it ends in.
    fn sinks(&self) -> Vec<String>;
}

impl GraphExt for Graph {
    fn problems(&self, store: Option<&Store>) -> Vec<String> {
        let resolve = |n: &Node| store.and_then(|s| missing(s, n)).map(|e| e.to_string());
        let resolve: Option<wp::Resolve<'_, NodeKind>> = if store.is_some() { Some(&resolve) } else { None };
        self.check(resolve)
            .into_iter()
            .map(|p| match p {
                Problem::NoSource => "no source node (search:, journal:, source:… or -)".into(),
                Problem::NoSink => "no model table to end in (a model:NAME node)".into(),
                Problem::SinkNotLeaf(id) => format!("model table `{id}` must be a leaf (nothing after it)"),
                Problem::LeafNotSink(id) => format!("`{id}` is a leaf: every leaf is a model table (add model:NAME after it)"),
                o => o.to_string(),
            })
            .collect()
    }

    fn spaces(&self) -> Vec<String> {
        let mut v = vec![];
        for n in &self.nodes {
            let name = match &n.kind {
                NodeKind::Space { name } => Some(super::split_options(name).0),
                NodeKind::Source { source } => source.strip_prefix("space:").map(|s| super::split_options(s).0),
                _ => None,
            };
            if let Some(x) = name
                && !v.contains(&x)
            {
                v.push(x);
            }
        }
        v
    }

    fn code_spaces(&self) -> Vec<String> {
        let mut v = vec![];
        for n in &self.nodes {
            let name = match &n.kind {
                NodeKind::Code { space } => Some(super::split_options(space).0),
                NodeKind::Source { source } => source.strip_prefix("code:").map(str::to_string),
                _ => None,
            };
            if let Some(x) = name
                && !v.contains(&x)
            {
                v.push(x);
            }
        }
        v
    }

    fn sinks(&self) -> Vec<String> {
        self.nodes.iter().filter_map(|n| if let NodeKind::Sink { model } = &n.kind { Some(model.clone()) } else { None }).collect()
    }
}

/// Why a node's names are not in the store (`None`: they are).
fn missing(s: &Store, n: &Node) -> Option<Error> {
    match &n.kind {
        NodeKind::Code { space } => s.find_code_space(&super::split_options(space).0).err(),
        NodeKind::Space { name } => s.find_space(&super::split_options(name).0).err(),
        NodeKind::Filter { search } => s.find_live_search(search).err(),
        NodeKind::Sink { model } => s.find_model(model).err(),
        NodeKind::Source { source } if source != "-" => match Source::parse(source) {
            Ok(Source::Search(x)) => s.find_live_search(&x).err(),
            Ok(Source::Journal(x)) => s.find_journal(&x).err(),
            Ok(Source::Code(x)) => s.find_code_space(&x).err(),
            Ok(Source::Space(x)) => s.find_space(&x).err(),
            Ok(Source::Data(x)) if s.source_by_name(&x).ok().flatten().is_none() && s.find_model(&x).is_err() => {
                Some(Error::invalid("source", format!("no source `{x}`")))
            }
            Ok(_) => None,
            Err(e) => Some(e),
        },
        _ => None,
    }
}

/// Run a pipeline: nodes in order, each once; `stdin` is the frame for `-` sources. A node
/// that fails stops the nodes downstream of it (`skipped`); the others still run.
pub fn run(store: &Store, env: &CodeEnv, origin: &str, p: &Pipeline, stdin: Option<Value>, log: &mut dyn FnMut(&str)) -> Result<PipelineRun> {
    if p.draft {
        return Err(Error::invalid(
            "pipeline",
            format!("{} is a draft from a peer: review it, then approve it (`mnnn pipeline approve {}`)", p.name, p.name),
        ));
    }
    let g = &p.graph;
    let problems = g.problems(Some(store));
    if !problems.is_empty() {
        return Err(Error::invalid("pipeline", format!("{} is not valid: {}", p.name, problems.join("; "))));
    }
    let order = g.order()?;
    let token = crate::store::new_id();
    let plan: Vec<String> = order.iter().map(|id| format!("{id} ({})", g.node(id).map(|n| n.kind.text()).unwrap_or_default())).collect();
    store.note_action(
        &NewAction::new("pipe.start", origin, format!("pipeline {}", p.name)).params(json!({ "token": token, "plan": plan, "pipeline": p.name, "views": g.nodes.iter().filter(|n| matches!(n.kind, NodeKind::View { .. })).map(|n| format!("view:{}/{}", p.name, n.id)).collect::<Vec<_>>() })),
    );
    let mut host = RunHost { store, env, origin, pipeline: &p.name, stdin: stdin.as_ref(), log, token: &token, plan: &plan };
    let out = wp::execute(&p.name, g, &mut host)?;
    let landed: Vec<String> = out.landed.iter().map(|(m, n)| format!("model:{m} → {n} records")).collect();
    store.note_action(
        &NewAction::new("pipe.run", origin, format!("pipeline {} {}", p.name, if out.ok() { "ran" } else { "ran with errors" }))
            .params(json!({ "token": token, "pipeline": p.name, "hops": plan, "saved": landed, "nodes": out.nodes })),
    );
    Ok(out)
}

/// What a pipeline's nodes do in Muninn: read sources, run code spaces (approved ones only:
/// an unapproved one is `waiting approval`), filter, save, and record each hop.
struct RunHost<'a> {
    store: &'a Store,
    env: &'a CodeEnv,
    origin: &'a str,
    pipeline: &'a str,
    stdin: Option<&'a Value>,
    log: &'a mut dyn FnMut(&str),
    token: &'a str,
    plan: &'a [String],
}

impl wp::Host<NodeKind> for RunHost<'_> {
    type Error = Error;

    fn output(&mut self, n: &Node, inputs: &[(String, Value)]) -> Result<Value> {
        node_out(self.store, self.env, self.origin, self.pipeline, n, inputs, self.stdin, &mut *self.log)
    }

    fn tee(&mut self, n: &Node, tee: &str, f: &Value) -> Result<String> {
        Sink::parse(tee).and_then(|s| pipe::save(self.store, self.origin, &s, f, None, &format!("pipeline {} · {}", self.pipeline, n.id)))
    }

    fn produced(&mut self, n: &Node, f: &Value) {
        match &n.kind {
            NodeKind::Code { space } => pipe::note_frame(self.store, self.origin, &format!("code:{}", super::split_options(space).0), f),
            NodeKind::Space { name } => pipe::note_frame(self.store, self.origin, &format!("space:{}", super::split_options(name).0), f),
            NodeKind::View { .. } => pipe::note_frame(self.store, self.origin, &format!("view:{}/{}", self.pipeline, n.id), f),
            _ => {}
        }
    }

    fn failed(&mut self, n: &Node, _e: &Error) -> String {
        let store = self.store;
        let unapproved = match &n.kind {
            NodeKind::Code { space } => store.find_code_space(&super::split_options(space).0).is_ok_and(|c| !super::approved(&c)),
            NodeKind::Space { name } => store.find_space(&super::split_options(name).0).is_ok_and(|s| !super::spaces::approved(store, &s)),
            _ => false,
        };
        if unapproved { "waiting approval".into() } else { wp::run::ERROR.into() }
    }

    fn finished(&mut self, i: usize, nr: &NodeRun, f: Option<&Value>) {
        let id = &nr.id;
        let cols: Vec<String> = f.and_then(|f| f.get("columns")).and_then(Value::as_object).map(|c| c.keys().cloned().collect()).unwrap_or_default();
        self.store.note_action(
            &NewAction::new(
                "pipe.hop",
                self.origin,
                format!("pipeline {} · {id}: {}", self.pipeline, nr.error.clone().unwrap_or_else(|| format!("{} rows", nr.rows_out))),
            )
            .params(
                json!({ "token": self.token, "i": i, "hop": self.plan[i], "rows": nr.rows_out, "rows_in": nr.rows_in, "columns": cols, "ms": nr.ms,
                    "status": if nr.status == "ok" { "ok" } else { "error" }, "state": nr.status, "error": nr.error, "saved": nr.saved }),
            ),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn node_out(
    store: &Store,
    env: &CodeEnv,
    origin: &str,
    pipeline: &str,
    n: &Node,
    ups: &[(String, Value)],
    stdin: Option<&Value>,
    log: &mut dyn FnMut(&str),
) -> Result<Value> {
    let all = || union(&format!("pipeline:{pipeline}:{}", n.id), &ups.iter().map(|(_, f)| f).collect::<Vec<_>>());
    match &n.kind {
        NodeKind::Source { source } if source == "-" => stdin
            .cloned()
            .ok_or_else(|| Error::invalid("pipeline", format!("`{}` reads standard input: pipe data in (`… | mnnn pipeline run NAME -`)", n.id))),
        NodeKind::Source { source } => pipe::frame(store, env, origin, &Source::parse(source)?, log)?
            .ok_or_else(|| Error::invalid("pipeline", format!("`{}` gives no data", n.id))),
        NodeKind::Space { name } => {
            let (name, options) = super::split_options(name);
            let s = store.find_space(&name)?;
            let main = store.code_space(&s.main_code_id)?.ok_or_else(|| Error::invalid("space", format!("{}: no main", s.name)))?;
            let from = ups.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>().join(", ");
            let first = ups.first().map(|x| x.1.clone());
            let info = super::with_options(options, || {
                super::with_named(ups.to_vec(), || super::spaces::run(store, env, origin, &s, first, &format!("pipeline {pipeline}: {from}"), log))
            })?;
            pipe::output(&main, &info)
        }
        NodeKind::Code { space } => {
            let (name, options) = super::split_options(space);
            log(&format!("note: code:{name} is deprecated (it runs one code); use space:NAME (`mnnn migrate` makes spaces)"));
            let c = store.find_code_space(&name)?;
            let from = ups.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>().join(", ");
            let info = super::with_options(options, || {
                super::run_named(store, env, origin, &c, ups.to_vec(), &format!("pipeline {pipeline}: {from}"), log)
            })?;
            pipe::output(&c, &info)
        }
        NodeKind::View { columns, .. } => Ok(wp::frame::select(&all(), columns)),
        NodeKind::Filter { search } => Ok(pipe::search_filter(&all(), &store.find_live_search(search)?.spec)),
        NodeKind::Sink { model } => {
            let f = all();
            let made = pipe::save(store, origin, &Sink::Model(model.clone()), &f, None, &format!("pipeline {pipeline}"))?;
            log(&format!("{}: {made}", n.id));
            Ok(f)
        }
    }
}

#[path = "pipeline_share.rs"]
pub mod share;
pub use share::{Export, ImportPlan, export, import, plan_import};

#[cfg(test)]
#[path = "pipeline_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "pipeline_golden_tests.rs"]
mod golden;
