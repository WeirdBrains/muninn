//! Pipeline graphs: parsing, order, validation, fan-out and fan-in (the runs need cargo).
use super::*;
use crate::store::{Design, EntryFormat, Field, FieldType, Lang, RecordQuery};

fn g(nodes: &[&str], edges: &[&str]) -> Result<Graph> {
    Graph::parse(&nodes.iter().map(|s| s.to_string()).collect::<Vec<_>>(), &edges.iter().map(|s| s.to_string()).collect::<Vec<_>>())
}

fn store() -> crate::store::Store {
    let s = crate::store::Store::open_in_memory().unwrap();
    s.add_author("Ada", None, &[]).unwrap();
    s
}

#[test]
fn nodes_and_edges_parse_and_order() {
    let x = g(&["src=journal:Notes+journal:Raw", "a=code:double", "out=model:Out"], &["src>a>out"]).unwrap();
    assert_eq!(x.nodes.len(), 3);
    assert_eq!(x.nodes[0].tees, vec!["journal:Raw".to_string()]);
    assert_eq!(x.edges.len(), 2);
    assert_eq!(x.order().unwrap(), vec!["src", "a", "out"]);
    assert!(x.problems(None).is_empty(), "{:?}", x.problems(None));
    let back: Graph = serde_json::from_value(serde_json::to_value(&x).unwrap()).unwrap();
    assert_eq!(back, x);
    assert!(g(&["x=nothing:1"], &[]).is_err());
    assert!(g(&[], &["a"]).is_err());
}

#[test]
fn cycles_and_open_leaves_are_refused() {
    let c = g(&["s=journal:N", "a=code:x", "b=code:y", "m=model:M"], &["s>a>b>a", "b>m"]).unwrap();
    assert!(c.order().is_err());
    assert!(c.problems(None).iter().any(|p| p.contains("cycle")), "{:?}", c.problems(None));
    let open = g(&["s=journal:N", "a=code:x", "b=code:y", "m=model:M"], &["s>a>m", "s>b"]).unwrap();
    assert!(open.problems(None).iter().any(|p| p.contains("`b` is a leaf")), "{:?}", open.problems(None));
    let after = g(&["s=journal:N", "m=model:M", "a=code:x", "n=model:N"], &["s>m>a>n"]).unwrap();
    assert!(after.problems(None).iter().any(|p| p.contains("must be a leaf")));
    let s = store();
    let names = g(&["s=journal:Nope", "m=model:Nope"], &["s>m"]).unwrap();
    assert_eq!(names.problems(Some(&s)).len(), 2, "{:?}", names.problems(Some(&s)));
}

#[test]
fn pipelines_are_stored_and_versioned() {
    let s = store();
    let x = g(&["s=journal:N", "m=model:M"], &["s>m"]).unwrap();
    let p = s.create_pipeline("flow", "demo", &x).unwrap();
    assert_eq!(s.find_pipeline("flow").unwrap().graph, x);
    let mut y = x.clone();
    y.add_node("v=view:table").unwrap();
    s.update_pipeline(&p.id, None, None, Some(&y)).unwrap();
    assert_eq!(s.find_pipeline("flow").unwrap().graph, y);
    assert_eq!(s.pipeline_versions(&p.id).unwrap().len(), 2);
    assert!(s.create_pipeline("flow", "", &x).is_err(), "names are unique");
    s.remove_pipeline(&p.id).unwrap();
    assert!(s.pipelines().unwrap().is_empty());
}

fn fixture(s: &crate::store::Store) {
    let j = s.create_journal("Notes", None, EntryFormat::Markdown).unwrap();
    for (t, b) in [("one", "alpha beta"), ("two", "alpha beta gamma delta")] {
        s.add_entry(&j.id, t, b, None).unwrap();
    }
    let k = s.create_journal("More", None, EntryFormat::Markdown).unwrap();
    s.add_entry(&k.id, "three", "a b c", None).unwrap();
    for (n, src) in [("double", "@view table\nw2 = words * 2"), ("triple", "@view table\nw3 = words * 3")] {
        let c = s.create_code_space(n, "", Lang::Math, src, Default::default()).unwrap();
        crate::code::approve(s, "test", &c).unwrap();
    }
    for (m, f) in [("Doubled", "w2"), ("Tripled", "w3"), ("All", "words")] {
        s.create_model(m, "", Design { fields: vec![Field::new(f, FieldType::Real)], ..Default::default() }, true).unwrap();
    }
}

fn col(s: &crate::store::Store, model: &str, f: &str) -> Vec<f64> {
    let m = s.find_model(model).unwrap();
    let mut v: Vec<f64> = s.records(&m, &RecordQuery::default()).unwrap().iter().filter_map(|r| r.values.get(f).and_then(Value::as_f64)).collect();
    v.sort_by(f64::total_cmp);
    v
}

/// One source, computed once, into two code spaces, each into its own table.
#[test]
fn fan_out_one_source_two_spaces_two_tables() {
    if crate::code::cargo().is_err() {
        eprintln!("skipped: no cargo");
        return;
    }
    let (s, dir) = (store(), tempfile::tempdir().unwrap());
    fixture(&s);
    let x = g(&["src=journal:Notes", "a=code:double", "b=code:triple", "da=model:Doubled", "tb=model:Tripled"], &["src>a>da", "src>b>tb"]).unwrap();
    let p = s.create_pipeline("fan", "", &x).unwrap();
    let r = run(&s, &CodeEnv::new(dir.path()), "test", &p, None, &mut |_| {}).unwrap();
    assert!(r.ok(), "{:?}", r.nodes);
    assert_eq!(r.nodes.iter().filter(|n| n.id == "src").count(), 1, "the source runs once");
    assert_eq!(col(&s, "Doubled", "w2"), vec![4.0, 8.0]);
    assert_eq!(col(&s, "Tripled", "w3"), vec![6.0, 12.0]);
    assert_eq!(r.landed.len(), 2);
    let kinds: Vec<String> = s.actions().unwrap().into_iter().map(|a| a.kind).collect();
    assert_eq!(kinds.iter().filter(|k| *k == "pipe.hop").count(), 5, "{kinds:?}");
}

/// Two sources wait for each other: a table gets both (by column), a space gets them named.
#[test]
fn fan_in_waits_for_all_inputs() {
    if crate::code::cargo().is_err() {
        eprintln!("skipped: no cargo");
        return;
    }
    let (s, dir) = (store(), tempfile::tempdir().unwrap());
    fixture(&s);
    let x = g(
        &["n=journal:Notes", "m=journal:More", "all=model:All", "d=code:double", "out=model:Doubled"],
        &["n>all", "m>all", "n>d:notes", "m>d:more", "d>out"],
    )
    .unwrap();
    let p = s.create_pipeline("in", "", &x).unwrap();
    let r = run(&s, &CodeEnv::new(dir.path()), "test", &p, None, &mut |_| {}).unwrap();
    assert!(r.ok(), "{:?}", r.nodes);
    let ord: Vec<&str> = r.nodes.iter().map(|n| n.id.as_str()).collect();
    let at = |id: &str| ord.iter().position(|x| *x == id).unwrap();
    assert!(at("all") > at("n") && at("all") > at("m") && at("d") > at("m"), "{ord:?}");
    assert_eq!(col(&s, "All", "words"), vec![2.0, 3.0, 4.0]);
    let d = r.nodes.iter().find(|n| n.id == "d").unwrap();
    assert_eq!(d.rows_in, 3);
    // host.input() is the first named input (notes)
    assert_eq!(col(&s, "Doubled", "w2"), vec![4.0, 8.0]);
}

#[test]
fn a_failing_node_skips_what_is_downstream() {
    let (s, dir) = (store(), tempfile::tempdir().unwrap());
    fixture(&s);
    s.create_code_space("raw", "", Lang::Math, "w2 = words * 2", Default::default()).unwrap();
    let x = g(&["src=journal:Notes", "a=code:raw", "da=model:Doubled", "all=model:All"], &["src>a>da", "src>all"]).unwrap();
    let p = s.create_pipeline("wait", "", &x).unwrap();
    let r = run(&s, &CodeEnv::new(dir.path()), "test", &p, None, &mut |_| {}).unwrap();
    let st = |id: &str| r.nodes.iter().find(|n| n.id == id).unwrap().status.clone();
    assert_eq!(st("a"), "waiting approval", "{:?}", r.nodes);
    assert_eq!(st("da"), "skipped");
    assert_eq!(st("all"), "ok");
    assert_eq!(col(&s, "All", "words"), vec![2.0, 4.0]);
}

/// Export with templates; import into another store: a plan first, code unapproved,
/// missing models created, a clash renamed or merged, a changed space kept alongside.
#[test]
fn pipelines_export_and_import() {
    let a = store();
    fixture(&a);
    let x = g(&["src=journal:Notes", "a=code:double", "da=model:Doubled"], &["src>a>da"]).unwrap();
    a.create_pipeline("flow", "words", &x).unwrap();
    let e = export(&a, "flow", true).unwrap();
    assert_eq!(e.code.len(), 1);
    assert!(e.models[0].template.is_some());
    let bytes = serde_json::to_vec(&e).unwrap();
    let back = Export::parse(&bytes).unwrap();
    let mut bad = back.clone();
    bad.code[0].source.push_str("\nw9 = 1");
    assert!(Export::parse(&serde_json::to_vec(&bad).unwrap()).is_err(), "a tampered source is refused");

    let b = store();
    b.create_journal("Notes", None, EntryFormat::Markdown).unwrap();
    b.create_code_space("double", "", Lang::Math, "w2 = words", Default::default()).unwrap();
    let plan = plan_import(&b, &back, None, false).unwrap();
    assert!(plan.problems.is_empty(), "{:?}", plan.lines());
    assert_eq!(plan.code[0].1, "double (imported)", "{:?}", plan.lines());
    let p = import(&b, &back, &plan).unwrap();
    assert!(p.graph.problems(Some(&b)).is_empty(), "{:?}", p.graph.problems(Some(&b)));
    let c = b.find_code_space("double (imported)").unwrap();
    assert!(!crate::code::approved(&c), "imported code needs approval");
    assert!(b.find_model("Doubled").is_ok(), "the model came from its template");
    // again: a clash
    assert!(plan_import(&b, &back, None, false).is_err());
    assert!(plan_import(&b, &back, Some("flow 2"), false).is_ok());
    let again = plan_import(&b, &back, None, true).unwrap();
    assert!(again.merge);
    assert_eq!(import(&b, &back, &again).unwrap().version, 2);
}
