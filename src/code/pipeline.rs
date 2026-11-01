//! Pipelines as graphs (DAGs): sources, code spaces, views and filters, ending in model
//! tables. A node's output is computed once and goes to every node downstream (fan-out);
//! a node with several upstreams waits for all of them and gets them as named frames
//! (fan-in: `host.input()` is the first, `host.input_named(NAME)` any). Saves (tees) hang
//! off any node and the data flows on. See `docs/pipelines.md`.
use super::CodeEnv;
use super::pipe::{self, Sink, Source};
use crate::error::{Error, Result};
use crate::store::{NewAction, Store, pipelines::Pipeline};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

/// What a node does.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NodeKind {
    /// Data in: `search:NAME`, `journal:NAME`, `code:SPACE`, a model query, or `-` (stdin).
    Source { source: String },
    /// A code space run over its inputs.
    Code { space: String },
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    #[serde(flatten)]
    pub kind: NodeKind,
    /// Saves along the way (`journal:NAME`, `model:NAME`): side branches.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tees: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    /// The name the downstream node gets this input under (default: the upstream's id).
    #[serde(default, rename = "as", skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Graph {
    #[serde(default)]
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub edges: Vec<Edge>,
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
            "source" => {
                Source::parse(&rest)?;
                NodeKind::Source { source: need("source")? }
            }
            "code" => NodeKind::Code { space: need("code")? },
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
            _ => return Err(Error::invalid("pipeline", format!("`{s}`: a node is search:/journal:/source:…/- , code:, view:, filter: or model:"))),
        })
    }

    pub fn text(&self) -> String {
        match self {
            NodeKind::Source { source } if source.starts_with("search:") || source.starts_with("journal:") || source == "-" => source.clone(),
            NodeKind::Source { source } => format!("source:{source}"),
            NodeKind::Code { space } => format!("code:{space}"),
            NodeKind::View { view, columns } if columns.is_empty() => format!("view:{view}"),
            NodeKind::View { view, columns } => format!("view:{view}:{}", columns.join(",")),
            NodeKind::Filter { search } => format!("filter:{search}"),
            NodeKind::Sink { model } => format!("model:{model}"),
        }
    }
}

impl Graph {
    /// Read `ID=KIND` node specs (with `+tee` saves: `raw=code:fast+journal:Raw`) and
    /// `A>B[:NAME]` edges; `A>B>C` chains.
    pub fn parse(nodes: &[String], edges: &[String]) -> Result<Graph> {
        let mut g = Graph::default();
        for n in nodes {
            g.add_node(n)?;
        }
        for e in edges {
            g.add_edges(e)?;
        }
        Ok(g)
    }

    pub fn add_node(&mut self, spec: &str) -> Result<()> {
        let (id, rest) = spec.split_once('=').ok_or_else(|| Error::invalid("pipeline", format!("`{spec}`: a node is ID=KIND (src=search:usdt)")))?;
        let id = id.trim().to_string();
        if id.is_empty() || id.contains(['>', ':', ' ']) {
            return Err(Error::invalid("pipeline", format!("`{id}` is not a node id (letters, digits, - and _)")));
        }
        let mut parts = rest.split('+');
        let kind = NodeKind::parse(parts.next().unwrap_or_default())?;
        let tees: Vec<String> = parts.map(|t| t.trim().to_string()).collect();
        for t in &tees {
            Sink::parse(t)?;
        }
        match self.nodes.iter_mut().find(|n| n.id == id) {
            Some(n) => *n = Node { id, kind, tees },
            None => self.nodes.push(Node { id, kind, tees }),
        }
        Ok(())
    }

    pub fn add_edges(&mut self, spec: &str) -> Result<()> {
        let hops: Vec<&str> = spec.split('>').map(str::trim).collect();
        if hops.len() < 2 || hops.iter().any(|h| h.is_empty()) {
            return Err(Error::invalid("pipeline", format!("`{spec}`: an edge is FROM>TO[:NAME] (or A>B>C)")));
        }
        for w in hops.windows(2) {
            let (to, name) = w[1].split_once(':').map(|(a, b)| (a.trim(), Some(b.trim().to_string()))).unwrap_or((w[1], None));
            let from = w[0].split_once(':').map(|x| x.0.trim()).unwrap_or(w[0]);
            let e = Edge { from: from.into(), to: to.into(), name };
            if !self.edges.iter().any(|x| x.from == e.from && x.to == e.to) {
                self.edges.push(e);
            }
        }
        Ok(())
    }

    pub fn remove_node(&mut self, id: &str) -> bool {
        let n = self.nodes.len();
        self.nodes.retain(|x| x.id != id);
        self.edges.retain(|e| e.from != id && e.to != id);
        n != self.nodes.len()
    }

    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn ups(&self, id: &str) -> Vec<&Edge> {
        self.edges.iter().filter(|e| e.to == id).collect()
    }

    pub fn downs(&self, id: &str) -> Vec<&Edge> {
        self.edges.iter().filter(|e| e.from == id).collect()
    }

    /// Node ids in an order where every node comes after its upstreams; `Err` names a cycle.
    pub fn order(&self) -> Result<Vec<String>> {
        let mut deg: HashMap<&str, usize> = self.nodes.iter().map(|n| (n.id.as_str(), 0)).collect();
        for e in &self.edges {
            if let Some(d) = deg.get_mut(e.to.as_str()) {
                *d += 1;
            }
        }
        let mut ready: Vec<&str> = self.nodes.iter().map(|n| n.id.as_str()).filter(|id| deg[id] == 0).collect();
        let mut out = vec![];
        while let Some(id) = (!ready.is_empty()).then(|| ready.remove(0)) {
            out.push(id.to_string());
            for e in self.edges.iter().filter(|e| e.from == id) {
                if let Some(d) = deg.get_mut(e.to.as_str()) {
                    *d -= 1;
                    if *d == 0 {
                        ready.push(e.to.as_str());
                    }
                }
            }
        }
        if out.len() < self.nodes.len() {
            let stuck: Vec<&str> = self.nodes.iter().map(|n| n.id.as_str()).filter(|id| !out.iter().any(|o| o == id)).collect();
            return Err(Error::invalid("pipeline", format!("a cycle through {}", stuck.join(", "))));
        }
        Ok(out)
    }

    /// What is wrong with the graph (empty: valid). With a store, the names are checked too.
    pub fn problems(&self, store: Option<&Store>) -> Vec<String> {
        let mut p = vec![];
        let mut seen = std::collections::HashSet::new();
        for n in &self.nodes {
            if !seen.insert(n.id.as_str()) {
                p.push(format!("node `{}` is there twice", n.id));
            }
        }
        for e in &self.edges {
            for end in [&e.from, &e.to] {
                if self.node(end).is_none() {
                    p.push(format!("edge {}>{}: no node `{end}`", e.from, e.to));
                }
            }
            if e.from == e.to {
                p.push(format!("node `{}` feeds itself", e.from));
            }
        }
        if let Err(e) = self.order() {
            p.push(e.to_string());
        }
        if !self.nodes.iter().any(|n| matches!(n.kind, NodeKind::Source { .. })) {
            p.push("no source node (search:, journal:, source:… or -)".into());
        }
        if !self.nodes.iter().any(|n| matches!(n.kind, NodeKind::Sink { .. })) {
            p.push("no model table to end in (a model:NAME node)".into());
        }
        for n in &self.nodes {
            let (ups, downs) = (self.ups(&n.id).len(), self.downs(&n.id).len());
            match &n.kind {
                NodeKind::Source { .. } if ups > 0 => p.push(format!("source `{}` has inputs", n.id)),
                NodeKind::Source { .. } => {}
                _ if ups == 0 => p.push(format!("`{}` gets no data (connect a source to it)", n.id)),
                _ => {}
            }
            match &n.kind {
                NodeKind::Sink { .. } if downs > 0 => p.push(format!("model table `{}` must be a leaf (nothing after it)", n.id)),
                NodeKind::Sink { .. } => {}
                _ if downs == 0 => p.push(format!("`{}` is a leaf: every leaf is a model table (add model:NAME after it)", n.id)),
                _ => {}
            }
            if let Some(s) = store {
                let missing = match &n.kind {
                    NodeKind::Code { space } => s.find_code_space(space).err(),
                    NodeKind::Filter { search } => s.find_live_search(search).err(),
                    NodeKind::Sink { model } => s.find_model(model).err(),
                    NodeKind::Source { source } if source != "-" => match Source::parse(source) {
                        Ok(Source::Search(x)) => s.find_live_search(&x).err(),
                        Ok(Source::Journal(x)) => s.find_journal(&x).err(),
                        Ok(Source::Code(x)) => s.find_code_space(&x).err(),
                        Ok(_) => None,
                        Err(e) => Some(e),
                    },
                    _ => None,
                };
                if let Some(e) = missing {
                    p.push(format!("`{}`: {e}", n.id));
                }
            }
        }
        p
    }

    /// The code spaces it runs.
    pub fn code_spaces(&self) -> Vec<String> {
        let mut v = vec![];
        for n in &self.nodes {
            let name = match &n.kind {
                NodeKind::Code { space } => Some(space.clone()),
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

    /// The model tables it ends in.
    pub fn sinks(&self) -> Vec<String> {
        self.nodes.iter().filter_map(|n| if let NodeKind::Sink { model } = &n.kind { Some(model.clone()) } else { None }).collect()
    }
}

/// One node of a run.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct NodeRun {
    pub id: String,
    pub node: String,
    /// `ok`, `error`, `waiting approval` or `skipped` (an upstream failed).
    pub status: String,
    pub rows_in: usize,
    pub rows_out: usize,
    pub ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub saved: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct PipelineRun {
    pub pipeline: String,
    pub nodes: Vec<NodeRun>,
    /// Rows landed per model table.
    pub landed: Vec<(String, usize)>,
    /// Each node's latest output (for views and previews).
    #[serde(skip)]
    pub frames: HashMap<String, Value>,
}

impl PipelineRun {
    pub fn ok(&self) -> bool {
        self.nodes.iter().all(|n| n.status == "ok")
    }
}

fn rows(f: &Value) -> usize {
    f.get("ids").and_then(Value::as_array).map(Vec::len).unwrap_or(0)
}

/// Frames one after the other (columns by name; groups dropped).
pub fn union(name: &str, frames: &[&Value]) -> Value {
    if frames.len() == 1 {
        return frames[0].clone();
    }
    let mut out: Vec<Map<String, Value>> = vec![];
    for f in frames {
        let cols = f.get("columns").and_then(Value::as_object).cloned().unwrap_or_default();
        for i in 0..rows(f) {
            out.push(cols.iter().map(|(k, v)| (k.clone(), v.get(i).cloned().unwrap_or(Value::Null))).collect());
        }
    }
    pipe::rows_frame(name, &out)
}

/// Run a pipeline: nodes in order, each once; `stdin` is the frame for `-` sources. A node
/// that fails stops the nodes downstream of it (`skipped`); the others still run.
pub fn run(store: &Store, env: &CodeEnv, origin: &str, p: &Pipeline, stdin: Option<Value>, log: &mut dyn FnMut(&str)) -> Result<PipelineRun> {
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
    let mut out = PipelineRun { pipeline: p.name.clone(), ..Default::default() };
    for (i, id) in order.iter().enumerate() {
        let n = g.node(id).expect("ordered nodes exist");
        let t = std::time::Instant::now();
        let mut nr = NodeRun { id: id.clone(), node: n.kind.text(), status: "ok".into(), ..Default::default() };
        let ups: Vec<(String, Value)> =
            g.ups(id).iter().filter_map(|e| out.frames.get(&e.from).map(|f| (e.name.clone().unwrap_or_else(|| e.from.clone()), f.clone()))).collect();
        if g.ups(id).len() > ups.len() {
            nr.status = "skipped".into();
            nr.error = Some("an upstream node did not finish".into());
        } else {
            nr.rows_in = ups.iter().map(|(_, f)| rows(f)).sum();
            let res = node_out(store, env, origin, &p.name, n, &ups, stdin.as_ref(), log);
            match res {
                Ok(f) => {
                    nr.rows_out = rows(&f);
                    for tee in &n.tees {
                        match Sink::parse(tee).and_then(|s| pipe::save(store, origin, &s, &f, None, &format!("pipeline {} · {id}", p.name))) {
                            Ok(m) => nr.saved.push(format!("{tee} → {m}")),
                            Err(e) => {
                                nr.status = "error".into();
                                nr.error = Some(format!("save {tee}: {e}"));
                            }
                        }
                    }
                    if let NodeKind::Sink { model } = &n.kind {
                        out.landed.push((model.clone(), nr.rows_out));
                    }
                    match &n.kind {
                        NodeKind::Code { space } => pipe::note_frame(store, origin, &format!("code:{space}"), &f),
                        NodeKind::View { .. } => pipe::note_frame(store, origin, &format!("view:{}/{id}", p.name), &f),
                        _ => {}
                    }
                    if nr.status == "ok" {
                        out.frames.insert(id.clone(), f);
                    }
                }
                Err(e) => {
                    let m = e.to_string();
                    let unapproved = matches!(&n.kind, NodeKind::Code { space } if store.find_code_space(space).is_ok_and(|c| !super::approved(&c)));
                    nr.status = if unapproved { "waiting approval".into() } else { "error".into() };
                    nr.error = Some(m);
                }
            }
        }
        nr.ms = t.elapsed().as_millis() as u64;
        let cols: Vec<String> =
            out.frames.get(id).and_then(|f| f.get("columns")).and_then(Value::as_object).map(|c| c.keys().cloned().collect()).unwrap_or_default();
        store.note_action(
            &NewAction::new(
                "pipe.hop",
                origin,
                format!("pipeline {} · {id}: {}", p.name, nr.error.clone().unwrap_or_else(|| format!("{} rows", nr.rows_out))),
            )
            .params(json!({ "token": token, "i": i, "hop": plan[i], "rows": nr.rows_out, "rows_in": nr.rows_in, "columns": cols, "ms": nr.ms,
                    "status": if nr.status == "ok" { "ok" } else { "error" }, "state": nr.status, "error": nr.error, "saved": nr.saved })),
        );
        out.nodes.push(nr);
    }
    let landed: Vec<String> = out.landed.iter().map(|(m, n)| format!("model:{m} → {n} records")).collect();
    store.note_action(
        &NewAction::new("pipe.run", origin, format!("pipeline {} {}", p.name, if out.ok() { "ran" } else { "ran with errors" }))
            .params(json!({ "token": token, "pipeline": p.name, "hops": plan, "saved": landed, "nodes": out.nodes })),
    );
    Ok(out)
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
        NodeKind::Code { space } => {
            let c = store.find_code_space(space)?;
            let from = ups.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>().join(", ");
            let info = super::run_named(store, env, origin, &c, ups.to_vec(), &format!("pipeline {pipeline}: {from}"), log)?;
            pipe::output(&c, &info)
        }
        NodeKind::View { columns, .. } => {
            let f = all();
            if columns.is_empty() {
                return Ok(f);
            }
            let mut v = f.clone();
            if let Some(cols) = f.get("columns").and_then(Value::as_object) {
                v["columns"] = Value::Object(cols.iter().filter(|(k, _)| columns.contains(k)).map(|(k, x)| (k.clone(), x.clone())).collect());
            }
            Ok(v)
        }
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
