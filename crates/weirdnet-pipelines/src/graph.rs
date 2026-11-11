//! Graph types, the node and edge syntax, ordering and checks.
use crate::error::{PipelineError, invalid};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;

/// Where a node sits: sources have no inputs, sinks are the leaves, steps are in between.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Source,
    Step,
    Sink,
}

/// What a node does, defined by the application: how its spec reads and prints, and its role.
/// The crate gives no source, step or sink any meaning of its own.
pub trait Kind: Sized {
    /// The application's error; graph errors convert into it.
    type Error: From<PipelineError>;
    /// A node spec shown when one has no `=` (`src=source:NAME`).
    const EXAMPLE: &'static str = "src=source:NAME";
    /// Read the part of a node spec after `ID=` and before any `+tee`.
    fn parse(spec: &str) -> Result<Self, Self::Error>;
    /// The spec again (`parse(text())` gives the same kind).
    fn text(&self) -> String;
    fn role(&self) -> Role;
    /// Check a tee (a side save: `+SPEC` after the kind) when the node is read.
    fn check_tee(_tee: &str) -> Result<(), Self::Error> {
        Ok(())
    }
    /// What a sink lands in, for [`crate::PipelineRun::landed`] (its text by default).
    fn target(&self) -> String {
        self.text()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node<K> {
    pub id: String,
    #[serde(flatten)]
    pub kind: K,
    /// Side saves along the way: the data is saved there and flows on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tees: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    /// The name the downstream node gets this input under (default: the upstream's id).
    #[serde(default, rename = "as", skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Graph<K> {
    #[serde(default = "Vec::new")]
    pub nodes: Vec<Node<K>>,
    #[serde(default)]
    pub edges: Vec<Edge>,
}

impl<K> Default for Graph<K> {
    fn default() -> Self {
        Graph { nodes: vec![], edges: vec![] }
    }
}

/// Says why a node's names do not resolve in the application (`None`: they do).
pub type Resolve<'a, K> = &'a dyn Fn(&Node<K>) -> Option<String>;

/// Something wrong with a graph ([`Graph::check`]). The texts are general; an application
/// can word them its own way by matching on the variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// Two nodes have this id.
    Twice(String),
    /// An edge names a node that is not there (`end`).
    NoNode {
        from: String,
        to: String,
        end: String,
    },
    FeedsItself(String),
    /// The nodes that could not be ordered.
    Cycle(Vec<String>),
    NoSource,
    NoSink,
    SourceHasInputs(String),
    /// A step or sink with nothing feeding it.
    NoInput(String),
    SinkNotLeaf(String),
    /// A leaf that is not a sink.
    LeafNotSink(String),
    /// The application could not resolve what the node names.
    Unresolved {
        id: String,
        why: String,
    },
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Problem::Twice(id) => write!(f, "node `{id}` is there twice"),
            Problem::NoNode { from, to, end } => write!(f, "edge {from}>{to}: no node `{end}`"),
            Problem::FeedsItself(id) => write!(f, "node `{id}` feeds itself"),
            Problem::Cycle(ids) => write!(f, "{}", cycle(ids)),
            Problem::NoSource => write!(f, "no source node"),
            Problem::NoSink => write!(f, "no sink node"),
            Problem::SourceHasInputs(id) => write!(f, "source `{id}` has inputs"),
            Problem::NoInput(id) => write!(f, "`{id}` gets no data (connect a source to it)"),
            Problem::SinkNotLeaf(id) => write!(f, "sink `{id}` must be a leaf (nothing after it)"),
            Problem::LeafNotSink(id) => write!(f, "`{id}` is a leaf: every leaf is a sink"),
            Problem::Unresolved { id, why } => write!(f, "`{id}`: {why}"),
        }
    }
}

fn cycle(ids: &[String]) -> PipelineError {
    invalid(format!("a cycle through {}", ids.join(", ")))
}

impl<K: Kind> Graph<K> {
    /// Read `ID=KIND` node specs (with `+tee` saves: `raw=step:fast+save:raw`) and
    /// `A>B[:NAME]` edges; `A>B>C` chains.
    pub fn parse(nodes: &[String], edges: &[String]) -> Result<Self, K::Error> {
        let mut g: Graph<K> = Graph::default();
        for n in nodes {
            g.add_node(n)?;
        }
        for e in edges {
            g.add_edges(e)?;
        }
        Ok(g)
    }

    /// Add a node, or replace the one with its id.
    pub fn add_node(&mut self, spec: &str) -> Result<(), K::Error> {
        let (id, rest) = spec.split_once('=').ok_or_else(|| invalid(format!("`{spec}`: a node is ID=KIND ({})", K::EXAMPLE)))?;
        let id = id.trim().to_string();
        if id.is_empty() || id.contains(['>', ':', ' ']) {
            return Err(invalid(format!("`{id}` is not a node id (letters, digits, - and _)")).into());
        }
        let mut parts = rest.split('+');
        let kind = K::parse(parts.next().unwrap_or_default())?;
        let tees: Vec<String> = parts.map(|t| t.trim().to_string()).collect();
        for t in &tees {
            K::check_tee(t)?;
        }
        match self.nodes.iter_mut().find(|n| n.id == id) {
            Some(n) => *n = Node { id, kind, tees },
            None => self.nodes.push(Node { id, kind, tees }),
        }
        Ok(())
    }

    /// What is wrong with the graph (empty: valid). `resolve`, when given, says why a node's
    /// names do not resolve in the application (`None`: they do).
    pub fn check(&self, resolve: Option<Resolve<'_, K>>) -> Vec<Problem> {
        let mut p = vec![];
        let mut seen = HashSet::new();
        for n in &self.nodes {
            if !seen.insert(n.id.as_str()) {
                p.push(Problem::Twice(n.id.clone()));
            }
        }
        for e in &self.edges {
            for end in [&e.from, &e.to] {
                if self.node(end).is_none() {
                    p.push(Problem::NoNode { from: e.from.clone(), to: e.to.clone(), end: end.clone() });
                }
            }
            if e.from == e.to {
                p.push(Problem::FeedsItself(e.from.clone()));
            }
        }
        if let Err(stuck) = self.ordered() {
            p.push(Problem::Cycle(stuck));
        }
        if !self.nodes.iter().any(|n| n.kind.role() == Role::Source) {
            p.push(Problem::NoSource);
        }
        if !self.nodes.iter().any(|n| n.kind.role() == Role::Sink) {
            p.push(Problem::NoSink);
        }
        for n in &self.nodes {
            let (ups, downs) = (self.ups(&n.id).len(), self.downs(&n.id).len());
            let role = n.kind.role();
            match role {
                Role::Source if ups > 0 => p.push(Problem::SourceHasInputs(n.id.clone())),
                Role::Source => {}
                _ if ups == 0 => p.push(Problem::NoInput(n.id.clone())),
                _ => {}
            }
            match role {
                Role::Sink if downs > 0 => p.push(Problem::SinkNotLeaf(n.id.clone())),
                Role::Sink => {}
                _ if downs == 0 => p.push(Problem::LeafNotSink(n.id.clone())),
                _ => {}
            }
            if let Some(why) = resolve.and_then(|r| r(n)) {
                p.push(Problem::Unresolved { id: n.id.clone(), why });
            }
        }
        p
    }
}

impl<K> Graph<K> {
    /// Add `A>B[:NAME]` edges (`A>B>C` chains); an edge already there is kept as it is.
    pub fn add_edges(&mut self, spec: &str) -> Result<(), PipelineError> {
        let hops: Vec<&str> = spec.split('>').map(str::trim).collect();
        if hops.len() < 2 || hops.iter().any(|h| h.is_empty()) {
            return Err(invalid(format!("`{spec}`: an edge is FROM>TO[:NAME] (or A>B>C)")));
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

    /// Remove a node and its edges; whether it was there.
    pub fn remove_node(&mut self, id: &str) -> bool {
        let n = self.nodes.len();
        self.nodes.retain(|x| x.id != id);
        self.edges.retain(|e| e.from != id && e.to != id);
        n != self.nodes.len()
    }

    pub fn node(&self, id: &str) -> Option<&Node<K>> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Edges into a node.
    pub fn ups(&self, id: &str) -> Vec<&Edge> {
        self.edges.iter().filter(|e| e.to == id).collect()
    }

    /// Edges out of a node.
    pub fn downs(&self, id: &str) -> Vec<&Edge> {
        self.edges.iter().filter(|e| e.from == id).collect()
    }

    /// Node ids in an order where every node comes after its upstreams (ties in the order
    /// the nodes were added); `Err` names a cycle.
    pub fn order(&self) -> Result<Vec<String>, PipelineError> {
        self.ordered().map_err(|stuck| cycle(&stuck))
    }

    fn ordered(&self) -> Result<Vec<String>, Vec<String>> {
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
            return Err(self.nodes.iter().map(|n| n.id.clone()).filter(|id| !out.contains(id)).collect());
        }
        Ok(out)
    }
}
