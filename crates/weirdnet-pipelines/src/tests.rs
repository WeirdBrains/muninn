//! A made-up application: `in:NAME` sources, `x:N` steps that multiply a column, `out:NAME`
//! sinks and `keep:NAME` tees.
use crate::frame::{self, rows_frame};
use crate::run::{self, Host};
use crate::{Edge, Graph, Kind, Node, NodeRun, PipelineError, Problem, Role, execute};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum K {
    In { name: String },
    Times { by: f64 },
    Out { to: String },
}

#[derive(Debug, PartialEq)]
struct AppError(String);

impl From<PipelineError> for AppError {
    fn from(e: PipelineError) -> Self {
        AppError(e.to_string())
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl Kind for K {
    type Error = AppError;
    const EXAMPLE: &'static str = "a=in:numbers";
    fn parse(s: &str) -> Result<K, AppError> {
        match s.trim().split_once(':') {
            Some(("in", n)) if !n.is_empty() => Ok(K::In { name: n.into() }),
            Some(("x", n)) => n.parse().map(|by| K::Times { by }).map_err(|_| AppError(format!("x:{n}: not a number"))),
            Some(("out", n)) if !n.is_empty() => Ok(K::Out { to: n.into() }),
            _ => Err(AppError(format!("`{s}`: in:, x: or out:"))),
        }
    }
    fn text(&self) -> String {
        match self {
            K::In { name } => format!("in:{name}"),
            K::Times { by } => format!("x:{by}"),
            K::Out { to } => format!("out:{to}"),
        }
    }
    fn role(&self) -> Role {
        match self {
            K::In { .. } => Role::Source,
            K::Times { .. } => Role::Step,
            K::Out { .. } => Role::Sink,
        }
    }
    fn check_tee(t: &str) -> Result<(), AppError> {
        t.strip_prefix("keep:").filter(|n| !n.is_empty()).map(|_| ()).ok_or_else(|| AppError(format!("`{t}`: a tee is keep:NAME")))
    }
    fn target(&self) -> String {
        match self {
            K::Out { to } => to.clone(),
            o => o.text(),
        }
    }
}

fn g(nodes: &[&str], edges: &[&str]) -> Result<Graph<K>, AppError> {
    Graph::parse(&nodes.iter().map(|s| s.to_string()).collect::<Vec<_>>(), &edges.iter().map(|s| s.to_string()).collect::<Vec<_>>())
}

fn nums(name: &str, v: &[f64]) -> Value {
    rows_frame(name, &v.iter().map(|n| json!({ "n": n }).as_object().unwrap().clone()).collect::<Vec<_>>())
}

#[derive(Default)]
struct App {
    calls: Vec<String>,
    kept: Vec<(String, usize)>,
    landed: Vec<(String, Vec<f64>)>,
    finished: Vec<(usize, String, String, bool)>,
}

fn col(f: &Value) -> Vec<f64> {
    f["columns"]["n"].as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default()
}

impl Host<K> for App {
    type Error = AppError;
    fn output(&mut self, node: &Node<K>, inputs: &[(String, Value)]) -> Result<Value, AppError> {
        self.calls.push(format!("{}<{}", node.id, inputs.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(",")));
        let all = || frame::union(&node.id, &inputs.iter().map(|(_, f)| f).collect::<Vec<_>>());
        match &node.kind {
            K::In { name } if name == "broken" => Err(AppError("in:broken: no such data".into())),
            K::In { name } if name == "locked" => Err(AppError("in:locked: not allowed".into())),
            K::In { name } => Ok(nums(name, &[1.0, 2.0, 3.0])),
            K::Times { by } => {
                let f = all();
                let v: Vec<f64> = col(&f).iter().map(|n| n * by).collect();
                Ok(nums(&node.id, &v))
            }
            K::Out { to } => {
                let f = all();
                self.landed.push((to.clone(), col(&f)));
                Ok(f)
            }
        }
    }
    fn tee(&mut self, _node: &Node<K>, tee: &str, f: &Value) -> Result<String, AppError> {
        if tee == "keep:full" {
            return Err(AppError("full".into()));
        }
        self.kept.push((tee.into(), frame::rows(f)));
        Ok(format!("{} rows", frame::rows(f)))
    }
    fn failed(&mut self, _node: &Node<K>, e: &AppError) -> String {
        if e.0.contains("not allowed") { "waiting approval".into() } else { run::ERROR.into() }
    }
    fn finished(&mut self, i: usize, r: &NodeRun, f: Option<&Value>) {
        self.finished.push((i, r.id.clone(), r.status.clone(), f.is_some()));
    }
}

#[test]
fn nodes_and_edges_parse_print_and_serialize() {
    let x = g(&["a=in:numbers+keep:raw", "d=x:2", "o=out:doubled"], &["a>d>o:twice"]).unwrap();
    assert_eq!(x.nodes[0].tees, vec!["keep:raw"]);
    assert_eq!(
        x.edges,
        vec![Edge { from: "a".into(), to: "d".into(), name: None }, Edge { from: "d".into(), to: "o".into(), name: Some("twice".into()) }]
    );
    let text = serde_json::to_string(&x).unwrap();
    assert_eq!(
        text,
        r#"{"nodes":[{"id":"a","kind":"in","name":"numbers","tees":["keep:raw"]},{"id":"d","kind":"times","by":2.0},{"id":"o","kind":"out","to":"doubled"}],"edges":[{"from":"a","to":"d"},{"from":"d","to":"o","as":"twice"}]}"#
    );
    assert_eq!(serde_json::from_str::<Graph<K>>(&text).unwrap(), x);
    assert_eq!(serde_json::from_str::<Graph<K>>("{}").unwrap(), Graph::default());
    assert!(x.nodes.iter().all(|n| K::parse(&n.kind.text()).unwrap() == n.kind));
    // a node added again replaces it; an edge added again is kept as it was
    let mut y = x.clone();
    y.add_node("d=x:3").unwrap();
    y.add_edges("d>o:other").unwrap();
    assert_eq!((y.nodes.len(), y.nodes[1].kind.clone(), y.edges[1].name.clone()), (3, K::Times { by: 3.0 }, Some("twice".into())));
    assert!(y.remove_node("d") && !y.remove_node("d"));
    assert_eq!((y.nodes.len(), y.edges.len()), (2, 0));
}

#[test]
fn bad_specs_say_what_is_wrong() {
    let e = |n: &[&str], ed: &[&str]| g(n, ed).unwrap_err().0;
    assert_eq!(e(&["a"], &[]), "invalid pipeline: `a`: a node is ID=KIND (a=in:numbers)");
    assert_eq!(e(&["a b=in:x"], &[]), "invalid pipeline: `a b` is not a node id (letters, digits, - and _)");
    assert_eq!(e(&["=in:x"], &[]), "invalid pipeline: `` is not a node id (letters, digits, - and _)");
    assert_eq!(e(&["a=on:x"], &[]), "`on:x`: in:, x: or out:", "the application's own error comes back as is");
    assert_eq!(e(&["a=in:x+drop:y"], &[]), "`drop:y`: a tee is keep:NAME");
    for bad in ["a", "a>", ">b", "a>>b"] {
        assert_eq!(e(&[], &[bad]), format!("invalid pipeline: `{bad}`: an edge is FROM>TO[:NAME] (or A>B>C)"));
    }
}

#[test]
fn order_puts_upstreams_first_and_names_a_cycle() {
    let x = g(&["o=out:z", "b=x:2", "a=in:n", "c=x:3"], &["a>b>o", "a>c>o"]).unwrap();
    assert_eq!(x.order().unwrap(), vec!["a", "b", "c", "o"]);
    let c = g(&["a=in:n", "p=x:1", "q=x:1", "o=out:z"], &["a>p>q>p", "q>o"]).unwrap();
    assert_eq!(c.order().unwrap_err().to_string(), "invalid pipeline: a cycle through p, q, o");
}

#[test]
fn checks_find_every_problem_in_order() {
    let ok = g(&["a=in:n", "b=x:2", "c=out:z"], &["a>b>c"]).unwrap();
    assert!(ok.check(None).is_empty());
    let x = g(&["a=in:n", "b=x:2", "c=out:z", "e=out:y", "s=in:m", "l1=x:1", "l2=x:1"], &["a>b>c>e", "zz>b", "b>b", "s>a", "l1>l2>l1"]).unwrap();
    let mut x2 = x.clone();
    x2.nodes.push(x.nodes[1].clone());
    let got: Vec<String> = x2.check(None).iter().map(|p| p.to_string()).collect();
    assert_eq!(
        got,
        vec![
            "node `b` is there twice",
            "edge zz>b: no node `zz`",
            "node `b` feeds itself",
            "invalid pipeline: a cycle through b, c, e, l1, l2, b",
            "source `a` has inputs",
            "sink `c` must be a leaf (nothing after it)",
        ],
        "{:?}",
        x2.check(None)
    );
    assert_eq!(Graph::<K>::default().check(None), vec![Problem::NoSource, Problem::NoSink]);
    let lone = g(&["s=x:1"], &[]).unwrap();
    assert_eq!(lone.check(None), vec![Problem::NoSource, Problem::NoSink, Problem::NoInput("s".into()), Problem::LeafNotSink("s".into())]);
    // the application resolves names
    let known = |n: &Node<K>| match &n.kind {
        K::In { name } if name != "n" => Some(format!("no data `{name}`")),
        _ => None,
    };
    let y = g(&["a=in:n", "b=in:gone", "c=out:z"], &["a>c", "b>c"]).unwrap();
    assert_eq!(y.check(Some(&known)), vec![Problem::Unresolved { id: "b".into(), why: "no data `gone`".into() }]);
    assert_eq!(y.check(Some(&known))[0].to_string(), "`b`: no data `gone`");
}

#[test]
fn a_run_fans_out_fans_in_tees_and_skips_below_a_failure() {
    let x = g(
        &[
            "a=in:numbers+keep:raw",
            "d=x:2",
            "t=x:3",
            "both=out:both",
            "bad=in:broken",
            "after=x:10",
            "o2=out:never",
            "lock=in:locked",
            "o3=out:locked",
            "full=x:1+keep:full",
            "o4=out:full",
        ],
        &["a>d>both:doubled", "a>t>both:tripled", "bad>after>o2", "lock>o3", "a>full>o4"],
    )
    .unwrap();
    assert!(x.check(None).is_empty(), "{:?}", x.check(None));
    let mut app = App::default();
    let r = execute("demo", &x, &mut app).unwrap();
    let st: Vec<(&str, &str, Option<&str>)> = r.nodes.iter().map(|n| (n.id.as_str(), n.status.as_str(), n.error.as_deref())).collect();
    assert_eq!(
        st,
        vec![
            ("a", "ok", None),
            ("bad", "error", Some("in:broken: no such data")),
            ("lock", "waiting approval", Some("in:locked: not allowed")),
            ("d", "ok", None),
            ("t", "ok", None),
            ("full", "error", Some("save keep:full: full")),
            ("after", "skipped", Some("an upstream node did not finish")),
            ("o3", "skipped", Some("an upstream node did not finish")),
            ("both", "ok", None),
            ("o4", "skipped", Some("an upstream node did not finish")),
            ("o2", "skipped", Some("an upstream node did not finish")),
        ]
    );
    assert!(!r.ok());
    assert_eq!(app.calls.iter().filter(|c| c.starts_with("a<")).count(), 1, "the source runs once: {:?}", app.calls);
    assert!(app.calls.contains(&"both<doubled,tripled".to_string()), "inputs come named, in edge order: {:?}", app.calls);
    assert_eq!(app.landed, vec![("both".to_string(), vec![2.0, 4.0, 6.0, 3.0, 6.0, 9.0])], "fan-in is the union");
    assert_eq!(r.landed, vec![("both".to_string(), 6)]);
    assert_eq!(app.kept, vec![("keep:raw".to_string(), 3)]);
    assert_eq!(r.nodes[0].saved, vec!["keep:raw → 3 rows"]);
    let both = r.nodes.iter().find(|n| n.id == "both").unwrap();
    assert_eq!((both.rows_in, both.rows_out, both.node.as_str()), (6, 6, "out:both"));
    assert!(!r.frames.contains_key("full"), "a node whose tee failed passes nothing on");
    assert_eq!(app.finished.len(), 11);
    assert_eq!(app.finished[0], (0, "a".to_string(), "ok".to_string(), true));
    assert_eq!(app.finished[1], (1, "bad".to_string(), "error".to_string(), false));
    let json = serde_json::to_value(&r.nodes[6]).unwrap();
    assert_eq!(json.as_object().unwrap().keys().collect::<Vec<_>>(), vec!["id", "node", "status", "rows_in", "rows_out", "ms", "error"]);
    // a cycle is refused before anything runs
    let c = g(&["a=in:n", "p=x:1", "q=x:1", "o=out:z"], &["a>p>q>p", "q>o"]).unwrap();
    let mut quiet = App::default();
    assert!(execute("c", &c, &mut quiet).unwrap_err().to_string().contains("a cycle through p, q, o"));
    assert!(quiet.calls.is_empty());
}

#[test]
fn frames_union_select_filter_and_tail() {
    let a = rows_frame("a", &[json!({"x": 1, "y": "p"}).as_object().unwrap().clone(), json!({"x": 2, "z": true}).as_object().unwrap().clone()]);
    assert_eq!(a["fields"], json!([{"name": "x", "type": "real"}, {"name": "y", "type": "text"}, {"name": "z", "type": "text"}]));
    assert_eq!(a["columns"]["y"], json!(["p", null]));
    let b = rows_frame("b", &[json!({"y": "q", "x": 3.5}).as_object().unwrap().clone()]);
    let u = frame::union("u", &[&a, &b]);
    assert_eq!(
        (u["model"].clone(), u["columns"]["x"].clone(), u["columns"]["z"].clone()),
        (json!("u"), json!([1, 2, 3.5]), json!([null, true, null]))
    );
    assert_eq!(frame::union("u", &[&a]), a);
    assert_eq!(frame::rows(&frame::union("u", &[])), 0);
    let s = frame::select(&a, &["y".to_string()]);
    assert_eq!(s["columns"].as_object().unwrap().keys().collect::<Vec<_>>(), vec!["y"]);
    assert_eq!(frame::select(&a, &[]), a);
    let f = frame::filter_rows(&u, &|r| r["x"].as_f64().unwrap() > 1.0);
    assert_eq!((f["ids"].clone(), f["columns"]["x"].clone()), (json!(["1", "2"]), json!([2, 3.5])));
    let t = frame::tail(&u, 1);
    assert_eq!((t["ids"].clone(), t["columns"]["y"].clone()), (json!(["2"]), json!(["q"])));
    assert_eq!(frame::tail(&u, 9), u);
    assert_eq!(frame::tail(&json!({"columns": {"c": [1, 2, 3]}}), 2), json!({"columns": {"c": [2, 3]}, "groups": []}));
}

#[test]
fn text_reads_as_frames_and_prints_as_markdown() {
    assert_eq!(frame::csv_line("\"a,b\",c,\"say \"\"hi\"\"\",\r"), vec!["a,b", "c", "say \"hi\"", ""]);
    let c = frame::read_data("name,n\n\"Smith, J\",1\nLee,\nOx,x1\n", "csv").unwrap();
    assert_eq!((c["columns"]["name"].clone(), c["columns"]["n"].clone()), (json!(["Smith, J", "Lee", "Ox"]), json!([1.0, null, "x1"])));
    assert_eq!(frame::read_data("[{\"a\":1},{\"b\":2}]", "j").unwrap()["columns"]["b"], json!([null, 2]));
    assert_eq!(frame::read_data("{\"a\":1}\n\n{\"a\":2}", "nd").unwrap()["columns"]["a"], json!([1, 2]));
    let as_is = json!({"columns": {"k": [1]}, "ids": ["0"], "model": "m"});
    assert_eq!(frame::read_data(&as_is.to_string(), "x").unwrap(), as_is);
    assert_eq!(frame::read_data("[1]", "x").unwrap_err().to_string(), "invalid input: expected objects, got 1");
    assert!(frame::read_data("[nope", "x").unwrap_err().to_string().starts_with("invalid input: JSON: "));
    assert!(frame::read_data("{\"a\":1}\nnope", "x").unwrap_err().to_string().starts_with("invalid input: NDJSON: "));
    let md = frame::markdown(&json!({"columns": {"a": [1, "x|y", null], "_hidden": [1, 2, 3]}, "ids": ["0", "1", "2"]}), 2);
    assert_eq!(md, "| a |\n|---|\n| 1 |\n| x\\|y |\n\n… 1 more rows\n");
    assert_eq!(frame::markdown(&json!({}), 5), "(no columns)\n");
}
