# weirdnet-pipelines

Pipelines as graphs (DAGs) of nodes whose meaning belongs to the application.

- `Kind`: the application's node type. It reads and prints a node spec, says whether the
  node is a source, a step or a sink, checks tees, and names what a sink lands in. The
  crate gives no source, step or sink a meaning of its own; their names are the
  application's strings.
- `Graph<K>`: nodes (`ID=KIND`, with `+TEE` side saves) and edges (`A>B[:NAME]`, chains
  `A>B>C`), serialized as `{nodes: [{id, kind…, tees}], edges: [{from, to, as}]}`.
- `Graph::order`: every node after its upstreams; a cycle is an error naming its nodes.
- `Graph::check`: duplicate ids, edges to missing nodes, self-loops, cycles, no source or
  sink, sources with inputs, steps or sinks with none, leaves that are not sinks, sinks
  with something after them, and (with a resolver) names the application can't find.
- `execute`: each node once, in order. Fan-out: a node's frame goes to every node after
  it. Fan-in: a node waits for all its inputs and gets them as `(name, frame)` pairs. Tees
  save a node's frame and the data flows on. A node that fails stops the nodes below it
  (`skipped`); the others still run.
- `frame`: JSON column frames (`{model, fields, ids, columns, groups}`): build them from
  rows, union, select columns, filter rows, keep the last rows, read CSV, JSON or NDJSON,
  and print a Markdown table.

The runner reaches the application only through `Host`: it asks for a node's frame given
its inputs, saves tees, and is told when a node produced a frame or finished. Lookups,
permissions, approvals, storage and every other effect stay in the application.

```rust
use serde_json::{Value, json};
use weirdnet_pipelines::{Graph, Host, Kind, Node, PipelineError, Role, execute, frame};

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind")]
enum Step {
    Numbers,
    Double,
    Print,
}

impl Kind for Step {
    type Error = PipelineError;
    fn parse(s: &str) -> Result<Step, PipelineError> {
        match s {
            "numbers" => Ok(Step::Numbers),
            "double" => Ok(Step::Double),
            "print" => Ok(Step::Print),
            _ => Err(PipelineError::invalid("node", format!("`{s}`: numbers, double or print"))),
        }
    }
    fn text(&self) -> String {
        format!("{self:?}").to_lowercase()
    }
    fn role(&self) -> Role {
        match self {
            Step::Numbers => Role::Source,
            Step::Double => Role::Step,
            Step::Print => Role::Sink,
        }
    }
}

struct App(Vec<String>);

impl Host<Step> for App {
    type Error = String;
    fn output(&mut self, node: &Node<Step>, inputs: &[(String, Value)]) -> Result<Value, String> {
        let all = frame::union(&node.id, &inputs.iter().map(|(_, f)| f).collect::<Vec<_>>());
        let n = |v: i64| json!({ "n": v }).as_object().unwrap().clone();
        Ok(match node.kind {
            Step::Numbers => frame::rows_frame("numbers", &[n(1), n(2)]),
            Step::Double => {
                let rows: Vec<_> = all["columns"]["n"].as_array().unwrap().iter().map(|v| n(v.as_i64().unwrap() * 2)).collect();
                frame::rows_frame("doubled", &rows)
            }
            Step::Print => {
                self.0.push(frame::markdown(&all, 10));
                all
            }
        })
    }
    fn tee(&mut self, _: &Node<Step>, tee: &str, _: &Value) -> Result<String, String> {
        Err(format!("no tee {tee}"))
    }
}

let g: Graph<Step> = Graph::parse(&["a=numbers".into(), "b=double".into(), "c=print".into()], &["a>b>c".into()]).unwrap();
assert!(g.check(None).is_empty());
let mut app = App(vec![]);
let run = execute("demo", &g, &mut app).unwrap();
assert!(run.ok());
assert_eq!(app.0, vec!["| n |\n|---|\n| 2 |\n| 4 |\n"]);
```
