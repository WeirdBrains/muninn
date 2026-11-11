//! Pipelines as graphs (DAGs) of nodes whose meaning belongs to the application: a [`Kind`]
//! says what a node spec means (a source, a step or a sink) and how it reads and prints.
//! This crate parses `ID=KIND+tee` nodes and `A>B[:NAME]` edges, checks a graph
//! ([`Graph::check`]), orders it, and runs it ([`run::execute`]): each node once, fan-out to
//! every node downstream, fan-in as named frames, tees as side saves, and the nodes below a
//! failure skipped. Everything a node does happens in the application's [`run::Host`].
//! [`frame`] has helpers for the JSON column frames the nodes pass along.
#![forbid(unsafe_code)]

mod error;
pub mod frame;
mod graph;
pub mod run;

pub use error::{PipelineError, Result};
pub use graph::{Edge, Graph, Kind, Node, Problem, Resolve, Role};
pub use run::{Host, NodeRun, PipelineRun, execute};

#[cfg(test)]
mod tests;

#[doc = include_str!("../README.md")]
#[cfg(doctest)]
pub struct ReadmeDoctests;
