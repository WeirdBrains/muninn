//! Running a graph: each node once, in order, through the application's [`Host`].
use crate::error::PipelineError;
use crate::frame::rows;
use crate::graph::{Graph, Kind, Node, Role};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fmt::Display;

/// A node's status when it gave a frame (and every tee saved it).
pub const OK: &str = "ok";
/// The default status of a node that failed ([`Host::failed`] can name another).
pub const ERROR: &str = "error";
/// A node not run because an upstream node did not finish.
pub const SKIPPED: &str = "skipped";

/// One node of a run.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct NodeRun {
    pub id: String,
    /// The node's kind as text.
    pub node: String,
    /// [`OK`], [`ERROR`], [`SKIPPED`], or a status the host gave a failure.
    pub status: String,
    pub rows_in: usize,
    pub rows_out: usize,
    pub ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// What each tee made (`TEE → what`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub saved: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct PipelineRun {
    pub pipeline: String,
    pub nodes: Vec<NodeRun>,
    /// Rows landed per sink ([`Kind::target`]), in run order.
    pub landed: Vec<(String, usize)>,
    /// Each node's frame (the nodes that finished).
    #[serde(skip)]
    pub frames: HashMap<String, Value>,
}

impl PipelineRun {
    /// Every node finished.
    pub fn ok(&self) -> bool {
        self.nodes.iter().all(|n| n.status == OK)
    }
}

/// What the nodes do: everything with an effect, every name lookup and every permission
/// check is the application's.
pub trait Host<K> {
    type Error: Display;
    /// The frame a node gives for its inputs: `(name, frame)` per edge into it, in edge order
    /// (none for a source).
    fn output(&mut self, node: &Node<K>, inputs: &[(String, Value)]) -> Result<Value, Self::Error>;
    /// Save a node's frame at one of its tees; what it made.
    fn tee(&mut self, node: &Node<K>, tee: &str, frame: &Value) -> Result<String, Self::Error>;
    /// A node gave a frame (after its tees).
    fn produced(&mut self, _node: &Node<K>, _frame: &Value) {}
    /// The status of a node whose output failed ([`ERROR`] by default).
    fn failed(&mut self, _node: &Node<K>, _error: &Self::Error) -> String {
        ERROR.into()
    }
    /// A node is done (`index` in run order); `frame` is its frame when it finished.
    fn finished(&mut self, _index: usize, _run: &NodeRun, _frame: Option<&Value>) {}
}

/// Run a graph: nodes in [`Graph::order`], each once. A node waits for all its inputs; one
/// that fails (or whose tee fails) stops the nodes downstream of it ([`SKIPPED`]), and the
/// others still run. Check the graph first ([`Graph::check`]): this only refuses a cycle.
pub fn execute<K: Kind, H: Host<K>>(pipeline: &str, g: &Graph<K>, host: &mut H) -> Result<PipelineRun, PipelineError> {
    let order = g.order()?;
    let mut out = PipelineRun { pipeline: pipeline.to_string(), ..Default::default() };
    for (i, id) in order.iter().enumerate() {
        let Some(n) = g.node(id) else { continue };
        let t = std::time::Instant::now();
        let mut nr = NodeRun { id: id.clone(), node: n.kind.text(), status: OK.into(), ..Default::default() };
        let ups: Vec<(String, Value)> =
            g.ups(id).iter().filter_map(|e| out.frames.get(&e.from).map(|f| (e.name.clone().unwrap_or_else(|| e.from.clone()), f.clone()))).collect();
        if g.ups(id).len() > ups.len() {
            nr.status = SKIPPED.into();
            nr.error = Some("an upstream node did not finish".into());
        } else {
            nr.rows_in = ups.iter().map(|(_, f)| rows(f)).sum();
            match host.output(n, &ups) {
                Ok(f) => {
                    nr.rows_out = rows(&f);
                    for tee in &n.tees {
                        match host.tee(n, tee, &f) {
                            Ok(m) => nr.saved.push(format!("{tee} → {m}")),
                            Err(e) => {
                                nr.status = ERROR.into();
                                nr.error = Some(format!("save {tee}: {e}"));
                            }
                        }
                    }
                    if n.kind.role() == Role::Sink {
                        out.landed.push((n.kind.target(), nr.rows_out));
                    }
                    host.produced(n, &f);
                    if nr.status == OK {
                        out.frames.insert(id.clone(), f);
                    }
                }
                Err(e) => {
                    nr.error = Some(e.to_string());
                    nr.status = host.failed(n, &e);
                }
            }
        }
        nr.ms = t.elapsed().as_millis() as u64;
        host.finished(i, &nr, out.frames.get(id));
        out.nodes.push(nr);
    }
    Ok(out)
}
