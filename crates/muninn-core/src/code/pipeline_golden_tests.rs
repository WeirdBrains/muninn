//! Golden test for pipelines, captured from the code before the graph, its checks, its
//! runner and the frame helpers moved to weirdnet-pipelines: the stored graph JSON, node
//! texts, order, every problem and parse message, a run's nodes and the actions it records,
//! and what the frame helpers return. `pipeline_golden.json` was written by `capture` on
//! 8fc22a82 (set `MUNINN_PIPELINE_GOLDEN_OUT` to a path to write it again).
use super::*;
use crate::code::pipe;
use crate::store::{Design, EntryFormat, Field, FieldType, Lang};
use serde_json::Map;

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

const NODES: &[&str] = &[
    "src=search:usdt+journal:Raw+model:Keep",
    "in=-",
    "std=stdin",
    "d=source:raw",
    "q=source:bars where close > 1 limit 5",
    "j=journal:Notes",
    "c=code:fast?x=1",
    "sp=space:smooth",
    "v=view:chart:close, open",
    "t=view:table",
    "f=filter:big",
    "out=model:Out",
    "o2=sink:Out2",
];
const EDGES: &[&str] = &["src>c>v:prices", "in>sp", "d>f", "q>t", "v>out", "sp>out:two", "f>o2", "t>o2", "j:x>o2", "std>out", "src>c"];

fn err(nodes: &[&str], edges: &[&str]) -> String {
    Graph::parse(&s(nodes), &s(edges)).map(|_| "ok".to_string()).unwrap_or_else(|e| e.to_string())
}

fn problems(nodes: &[&str], edges: &[&str], store: Option<&crate::store::Store>) -> Value {
    let mut g = Graph::default();
    for n in nodes {
        g.add_node(n).unwrap();
    }
    for e in edges {
        g.add_edges(e).unwrap();
    }
    json!(g.problems(store))
}

fn capture() -> Value {
    let g = Graph::parse(&s(NODES), &s(EDGES)).unwrap();
    let text = serde_json::to_string(&g).unwrap();
    let back: Graph = serde_json::from_str(&text).unwrap();
    assert_eq!(back, g);
    let mut removed = g.clone();
    let gone = (removed.remove_node("c"), removed.remove_node("nope"));
    let empty = crate::store::Store::open_in_memory().unwrap();
    let parse_errors: Vec<Value> = [
        (vec!["x"], vec![]),
        (vec!["a b=search:x"], vec![]),
        (vec!["a>b=search:x"], vec![]),
        (vec!["=search:x"], vec![]),
        (vec!["x=nothing:1"], vec![]),
        (vec!["x=nothing"], vec![]),
        (vec!["x=view:pie"], vec![]),
        (vec!["x=code:"], vec![]),
        (vec!["x=view:"], vec![]),
        (vec!["x=search:"], vec![]),
        (vec!["x=source:"], vec![]),
        (vec!["x=search:a+bogus:1"], vec![]),
        (vec!["x=search:a+model:"], vec![]),
        (vec![], vec!["a"]),
        (vec![], vec!["a>"]),
        (vec![], vec![">b"]),
        (vec![], vec!["a>>b"]),
    ]
    .iter()
    .map(|(n, e)| json!([n, e, err(n, e)]))
    .collect();
    let checks = json!({
        "valid": problems(&["a=-", "b=view:table", "c=model:X"], &["a>b>c"], None),
        "twice": problems(&["a=-", "b=model:X"], &["a>b"], None).as_array().unwrap().len(),
        "broken": problems(
            &["a=-", "b=code:x", "c=model:X", "e=model:Y", "s=search:s", "loop1=view:table", "loop2=view:table", "lone=view:table"],
            &["a>b", "b>c", "c>e", "zz>b", "b>yy", "b>b", "s>a", "loop1>loop2", "loop2>loop1"],
            None,
        ),
        "nothing": problems(&[], &[], None),
        "steps_only": problems(&["a=view:table"], &[], None),
        "names": problems(
            &["a=search:nope", "b=journal:nope", "c=code:nope", "d=space:nope", "e=source:nope", "f=-", "x=code:nope2?y=1", "y=space:nope3", "z=filter:nope", "m=model:nope", "w=source:bars where close > 1"],
            &["a>x", "b>x", "c>x", "d>x", "e>x", "f>y", "w>z", "x>m", "y>m", "z>m"],
            Some(&empty),
        ),
    });
    json!({
        "graph": serde_json::from_str::<Value>(&text).unwrap(),
        "graph_text": text,
        "texts": g.nodes.iter().map(|n| json!([n.id, n.kind.text(), n.tees])).collect::<Vec<_>>(),
        "reparsed": g.nodes.iter().map(|n| NodeKind::parse(&n.kind.text()).unwrap() == n.kind).collect::<Vec<_>>(),
        "order": g.order().unwrap(),
        "ups_downs": g.nodes.iter().map(|n| json!([n.id, g.ups(&n.id).len(), g.downs(&n.id).len()])).collect::<Vec<_>>(),
        "spaces": g.spaces(),
        "code_spaces": g.code_spaces(),
        "sinks": g.sinks(),
        "removed": [gone.0, gone.1, removed.nodes.len(), removed.edges.len()],
        "parse_errors": parse_errors,
        "checks": checks,
        "run": run_capture(),
        "frames": frames_capture(),
    })
}

/// A run over standard input: fan-out to two views, fan-in at a table, a tee, a code space
/// that is not approved (waiting approval) and the node after it (skipped).
fn run_capture() -> Value {
    let st = crate::store::Store::open_in_memory().unwrap();
    st.add_author("Ada", None, &[]).unwrap();
    let raw = st.create_journal("Raw", None, EntryFormat::Markdown).unwrap();
    st.create_model(
        "Out",
        "",
        Design { fields: vec![Field::new("x", FieldType::Real), Field::new("y", FieldType::Text)], ..Default::default() },
        true,
    )
    .unwrap();
    st.create_model("Side", "", Design { fields: vec![Field::new("x", FieldType::Real)], ..Default::default() }, true).unwrap();
    let c = st.create_code_space("raw", "", Lang::Math, "w2 = x * 2", Default::default()).unwrap();
    let g = Graph::parse(
        &s(&["in=-+journal:Raw", "v=view:table:x", "u=view:chart", "out=model:Out", "a=code:raw", "side=model:Side"]),
        &s(&["in>v>out:first", "in>u>out:second", "in>a>side"]),
    )
    .unwrap();
    let p = st.create_pipeline("gold", "", &g).unwrap();
    let rows: Vec<Map<String, Value>> = [json!({"x": 1, "y": "a"}), json!({"x": 2, "y": "b"}), json!({"x": 3, "y": "bb"})]
        .into_iter()
        .map(|v| v.as_object().unwrap().clone())
        .collect();
    let input = pipe::rows_frame("stdin", &rows);
    let dir = tempfile::tempdir().unwrap();
    let mut log = vec![];
    let r = run(&st, &CodeEnv::new(dir.path()), "test", &p, Some(input), &mut |l| log.push(l.to_string())).unwrap();
    let mut no_stdin = vec![];
    let missing = run(&st, &CodeEnv::new(dir.path()), "test", &p, None, &mut |l| no_stdin.push(l.to_string())).unwrap();
    let mut draft = p.clone();
    draft.draft = true;
    let refused = run(&st, &CodeEnv::new(dir.path()), "test", &draft, None, &mut |_| {}).unwrap_err().to_string();
    let bad = st.create_pipeline("bad", "", &Graph::parse(&s(&["a=-"]), &[]).unwrap()).unwrap();
    let invalid = run(&st, &CodeEnv::new(dir.path()), "test", &bad, None, &mut |_| {}).unwrap_err().to_string();
    let mut frames: Vec<(String, Value)> = r.frames.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    frames.sort_by(|a, b| a.0.cmp(&b.0));
    let actions: Vec<Value> = st
        .actions()
        .unwrap()
        .into_iter()
        .filter(|a| a.kind.starts_with("pipe.") || a.kind == "record.create" || a.kind == "journal.entry.create")
        .map(|a| {
            let mut params = a.params.clone();
            if let Some(m) = params.as_object_mut() {
                m.remove("ms");
                // timings: `pipe.run` carries each node's run, ms included
                for n in m.get_mut("nodes").and_then(Value::as_array_mut).into_iter().flatten() {
                    n["ms"] = json!(0);
                }
            }
            json!([a.kind, a.origin, a.summary, params])
        })
        .collect();
    let mut text = serde_json::to_string(&json!({
        "nodes": r.nodes.iter().map(|n| { let mut v = serde_json::to_value(n).unwrap(); v["ms"] = json!(0); v }).collect::<Vec<_>>(),
        "landed": r.landed,
        "ok": r.ok(),
        "frames": frames,
        "log": log,
        "no_stdin": missing.nodes.iter().map(|n| json!([n.id, n.status, n.error])).collect::<Vec<_>>(),
        "no_stdin_log": no_stdin,
        "draft": refused,
        "invalid": invalid,
        "actions": actions,
    }))
    .unwrap();
    let mut ids: Vec<(String, String)> = vec![(raw.id.clone(), "JOURNAL".into()), (c.id.clone(), "CODE".into()), (p.id.clone(), "PIPELINE".into())];
    for e in st.entries(&raw.id).unwrap() {
        ids.push((e.id.clone(), "ENTRY".into()));
    }
    for m in ["Out", "Side"] {
        ids.push((st.find_model(m).unwrap().id, format!("MODEL-{m}")));
    }
    for a in st.actions().unwrap() {
        if let Some(t) = a.params.get("token").and_then(Value::as_str) {
            ids.push((t.to_string(), "TOKEN".into()));
        }
    }
    for (id, name) in ids {
        text = text.replace(&id, &name);
    }
    serde_json::from_str(&text).unwrap()
}

fn frames_capture() -> Value {
    let a = pipe::rows_frame("a", &[json!({"x": 1, "y": "p"}).as_object().unwrap().clone(), json!({"x": 2, "z": true}).as_object().unwrap().clone()]);
    let b = pipe::rows_frame("b", &[json!({"y": "q", "x": 3.5}).as_object().unwrap().clone()]);
    let csv = "name,n,note\n\"Smith, J\",1,\"say \"\"hi\"\"\"\nLee,2.5,\nOx,x1,|pipe|\n";
    json!({
        "rows": a,
        "union1": union("u", &[&a]),
        "union2": union("u", &[&a, &b]),
        "union0": union("u", &[]),
        "filter": pipe::filter_rows(&a, &|r| r.get("x").and_then(Value::as_f64).unwrap_or(0.0) > 1.0),
        "tail": [pipe::tail(&a, 1), pipe::tail(&a, 5), pipe::tail(&json!({"columns": {"c": [1, 2, 3]}}), 2)],
        "csv_line": (["a,b", "\"a,b\",c", "x,\"he said \"\"no\"\"\",", "", "a\r"].map(pipe::csv_line)),
        "read": [
            pipe::read_data(csv, "csv").unwrap(),
            pipe::read_data("[{\"a\":1},{\"b\":\"x\"}]", "json").unwrap(),
            pipe::read_data("{\"a\":1}\n\n{\"a\":2,\"c\":null}\n", "nd").unwrap(),
            pipe::read_data("{\"columns\":{\"k\":[1]},\"ids\":[\"0\"],\"model\":\"as is\"}", "frame").unwrap(),
        ],
        "read_errors": [
            pipe::read_data("[1, 2]", "x").unwrap_err().to_string(),
            pipe::read_data("[nope", "x").unwrap_err().to_string(),
            pipe::read_data("{\"a\":1}\nnope", "x").unwrap_err().to_string(),
        ],
        "markdown": [pipe::markdown(&pipe::read_data(csv, "csv").unwrap(), 2), pipe::markdown(&a, 10), pipe::markdown(&json!({}), 10)],
    })
}

#[test]
fn pipelines_serialize_check_run_and_record_as_before() {
    let got = capture();
    if let Ok(path) = std::env::var("MUNINN_PIPELINE_GOLDEN_OUT") {
        std::fs::write(path, serde_json::to_string_pretty(&got).unwrap() + "\n").unwrap();
    }
    let want: Value = serde_json::from_str(include_str!("pipeline_golden.json")).unwrap();
    for k in want.as_object().unwrap().keys() {
        assert_eq!(got[k], want[k], "golden `{k}` changed");
    }
    assert_eq!(got, want);
}
