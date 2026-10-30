//! Building and loading real plugins (needs cargo; skipped without it). Polars is
//! `#[ignore]`d (it compiles the polars crate).
use super::*;
use crate::model::{Bar, Duration, Symbol};
use crate::store::{Design, Field, FieldType, RecordQuery};
use serde_json::json;

fn store() -> Store {
    let mut s = Store::open_in_memory().unwrap();
    s.add_author("Ada", None, &[]).unwrap();
    let src = s.add_source("bn", "csv", None).unwrap();
    let bars: Vec<Bar> = (0..40)
        .map(|i| Bar {
            ts: 1_704_067_200_000 + i * 300_000,
            open: 1.0,
            high: 2.0 + i as f64,
            low: 0.5,
            close: 1.0 + i as f64,
            volume: 10.0,
            flags: 0,
        })
        .collect();
    s.upsert_bars(&src, &Symbol::new("BTC", "USDT"), Duration(300), &bars).unwrap();
    s.upsert_bars(&src, &Symbol::new("ETH", "USDT"), Duration(300), &bars[..10]).unwrap();
    s
}

fn have_cargo() -> bool {
    let ok = cargo().is_ok();
    if !ok {
        eprintln!("skipped: no cargo");
    }
    ok
}

fn bars_input() -> CodeInput {
    CodeInput { model: "bars".into(), sort: vec!["timestamp".into()], ..Default::default() }
}

const RUST: &str = r#"
use muninn_plugin::prelude::*;

pub fn run(host: &Host) -> Result<View> {
    let bars = host.input()?;
    host.log(format!("{} bars", bars.len()));
    let first = bars.ids[0].clone();
    host.update("bars", &first, Json::obj().with("close", 99.5))?;
    host.create_model("signals", "name:text; score:real", "made by a plugin")?;
    let id = host.create("signals", Json::obj().with("name", "a").with("score", 1.0))?;
    host.create_many("signals", vec![Json::obj().with("name", "b").with("score", 2.0), Json::obj().with("name", "c").with("score", 3.0)])?;
    let n = host.update_where("signals", &["score >= 2"], Json::obj().with("score", 5.0))?;
    let eth = host.load(&Load::model("bars").filter("symbol = ETH/USDT").sort("-timestamp").limit(3))?;
    let close = bars.f64("close")?;
    let t = View::table("Summary", &["bars", "updated", "eth"]).row(vec![(bars.len() as f64).into(), (n as f64).into(), (eth.len() as f64).into()]);
    let c = View::chart("Close").x("timestamp", bars.f64("timestamp")?, true).line("close", close);
    if id.is_empty() { return Err(Error::new("no id")); }
    Ok(View::tiles("Rust", false, vec![t, c]))
}
"#;

#[test]
fn a_rust_plugin_builds_loads_runs_and_writes_history() {
    if !have_cargo() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let env = CodeEnv::new(dir.path());
    let s = store();
    let c = create(&s, "cli", "probe", "", Lang::Rust, RUST, bars_input()).unwrap();
    let mut log = vec![];
    // new code asks first
    assert!(matches!(run(&s, &env, "cli", &c, &mut |l| log.push(l.to_string())), Err(Error::NeedsConfirmation(_))));
    approve(&s, "cli", &c).unwrap();
    let c = s.code_space(&c.id).unwrap().unwrap();
    let info = run(&s, &env, "cli", &c, &mut |l| log.push(l.to_string())).unwrap();
    assert_eq!(info.status, "ok", "{info:?}\n{}", log.join("\n"));
    assert_eq!(info.logs, vec!["50 bars".to_string()]);
    let v = run_view(&info).unwrap();
    assert_eq!(v.leaves().len(), 2);
    let (_, rows) = v.leaves()[0].as_table();
    assert_eq!(rows[0], vec!["50", "2", "3"]);
    // a built-in bars field was changed, with the value before in history
    let bars = s.find_model("bars").unwrap();
    let all = s.records(&bars, &RecordQuery { sort: vec![("timestamp".into(), false), ("symbol".into(), false)], ..Default::default() }).unwrap();
    assert!(all.iter().any(|r| r.values["close"] == json!(99.5)));
    let acts = s.actions().unwrap();
    let up = acts.iter().find(|a| a.kind == "record.update" && a.origin == "code" && a.params["name"] == "bars").unwrap();
    assert_eq!(up.params["before"][0]["close"], json!(1.0));
    let sig = s.find_model("signals").unwrap();
    assert_eq!(s.count_records(&sig).unwrap(), 3);
    let kinds: Vec<&str> = acts.iter().map(|a| a.kind.as_str()).collect();
    for k in ["code.create", "code.approve", "code.build", "code.run", "model.create", "record.create"] {
        assert!(kinds.contains(&k), "{k} in {kinds:?}");
    }
    let r = acts.iter().find(|a| a.kind == "code.run").unwrap();
    assert_eq!(r.params["status"], "ok");
    assert!(r.summary.contains("write"), "{}", r.summary);
    assert!(r.author_id.is_some());
    assert_eq!(s.code_space(&c.id).unwrap().unwrap().last_run.unwrap().status, "ok");
    // cached: a second run builds nothing
    run(&s, &env, "cli", &c, &mut |_| {}).unwrap();
    assert_eq!(s.actions_of_kind("code.build").unwrap().len(), 1);
    // a changed source needs approving again
    let c = edit(&s, "cli", &c.id, &CodeChange { source: Some(RUST.replace("Summary", "Sum")), ..Default::default() }).unwrap();
    assert!(!approved(&c));

    // the built library loads as a prebuilt plugin too, and changing the file asks again
    let lib = env.plugin_path(&build_key(Lang::Rust, RUST));
    let copy = dir.path().join(format!("probe.{}", std::env::consts::DLL_EXTENSION));
    std::fs::copy(&lib, &copy).unwrap();
    let p = create(&s, "cli", "prebuilt", "", Lang::Plugin, &copy.display().to_string(), bars_input()).unwrap();
    assert!(check(&env, Lang::Plugin, &p.source).unwrap()[0].contains("\"lang\":\"rust\""));
    approve(&s, "cli", &p).unwrap();
    let p = s.code_space(&p.id).unwrap().unwrap();
    // signals exist now: the plugin's create_model is a no-op, the rest runs again
    assert_eq!(run(&s, &env, "cli", &p, &mut |_| {}).unwrap().status, "ok");
    std::fs::write(&copy, b"not a library").unwrap();
    assert!(!approved(&p));
    assert!(check(&env, Lang::Plugin, &p.source).is_err());
}

#[test]
fn plugin_errors_and_panics_are_runs_with_status_error() {
    if !have_cargo() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let env = CodeEnv::new(dir.path());
    let s = store();
    let src = "use muninn_plugin::prelude::*;\npub fn run(host: &Host) -> Result<View> {\n    let f = host.input()?;\n    if f.len() > 1 { panic!(\"boom {}\", f.len()); }\n    host.update(\"bars\", \"nope\", Json::obj().with(\"close\", 1.0))?;\n    Ok(View::text(\"x\", \"y\"))\n}\n";
    let c = create(&s, "cli", "bad", "", Lang::Rust, src, bars_input()).unwrap();
    approve(&s, "cli", &c).unwrap();
    let c = s.code_space(&c.id).unwrap().unwrap();
    let info = run(&s, &env, "cli", &c, &mut |_| {}).unwrap();
    assert_eq!(info.status, "error");
    assert_eq!(info.error.as_deref(), Some("panicked: boom 50"));
    let c = edit(&s, "cli", &c.id, &CodeChange { input: Some(CodeInput { limit: Some(1), ..bars_input() }), ..Default::default() }).unwrap();
    let info = run(&s, &env, "cli", &c, &mut |_| {}).unwrap();
    assert!(info.error.unwrap().contains("no bar"), "host errors reach the plugin");
    // a compile error is a build error with the compiler's message
    let c = edit(&s, "cli", &c.id, &CodeChange { source: Some("pub fn run() { let x: u8 = \"s\"; }".into()), ..Default::default() }).unwrap();
    approve(&s, "cli", &c).unwrap();
    let c = s.code_space(&c.id).unwrap().unwrap();
    let e = run(&s, &env, "cli", &c, &mut |_| {}).unwrap_err().to_string();
    assert!(e.contains("build failed") && e.contains("mismatched types"), "{e}");
}

#[test]
fn a_math_plugin_computes_per_group_and_writes_fields() {
    if !have_cargo() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let env = CodeEnv::new(dir.path());
    let s = store();
    let mut f = Field::new("fast", FieldType::Real);
    f.default = None;
    s.set_model_draft("bars", Some(Design { fields: vec![f], ..Default::default() })).unwrap();
    s.apply_model("bars", &Default::default()).unwrap();
    let src = "@title SMA\n@view value\nfast = sma(close, 5)\nup = if(close > fast, 1, 0)\n@update fast = fast\n";
    let input = CodeInput { group: vec!["symbol".into()], ..bars_input() };
    let c = create(&s, "tui", "sma", "", Lang::Math, src, input).unwrap();
    approve(&s, "tui", &c).unwrap();
    let c = s.code_space(&c.id).unwrap().unwrap();
    let info = run(&s, &env, "tui", &c, &mut |_| {}).unwrap();
    assert_eq!(info.status, "ok", "{info:?}");
    assert_eq!(info.writes, 50);
    let v = run_view(&info).unwrap();
    let (head, rows) = v.as_table();
    assert!(head.contains(&"fast".to_string()), "{head:?}");
    assert_eq!(rows.len(), 2, "one row per symbol: {rows:?}");
    let bars = s.find_model("bars").unwrap();
    let last = s
        .records(
            &bars,
            &RecordQuery {
                filter: vec![crate::rules::parse_condition("symbol = BTC/USDT").unwrap()],
                sort: vec![("timestamp".into(), true)],
                limit: Some(1),
                ..Default::default()
            },
        )
        .unwrap();
    // close 36..40 → mean 38
    assert_eq!(last[0].values["fast"], json!(38.0));
}

#[test]
fn bars_built_in_fields_and_keys_change_without_replacing_bars() {
    let s = store();
    let m = s.find_model("bars").unwrap();
    let all = s
        .records(
            &m,
            &RecordQuery {
                sort: vec![("timestamp".into(), false)],
                filter: vec![crate::rules::parse_condition("symbol = ETH/USDT").unwrap()],
                ..Default::default()
            },
        )
        .unwrap();
    let (a, b) = (&all[0], &all[1]);
    let v = |j: Value| j.as_object().unwrap().clone();
    let (id, before) = bars::update_record(&s, &m, &a.id, &v(json!({"open": 7, "flags": 1}))).unwrap();
    assert_eq!(id, a.id);
    assert_eq!(before["open"], json!(1.0));
    // moving a bar onto another one's key is refused
    let e = bars::update_record(&s, &m, &a.id, &v(json!({"timestamp": b.values["timestamp"].clone()}))).unwrap_err();
    assert!(e.to_string().contains("would replace"), "{e}");
    // re-keying moves it (new id, stats follow)
    let (id, _) = bars::update_record(&s, &m, &a.id, &v(json!({"symbol": "SOL/USDT", "source": "mine"}))).unwrap();
    assert_ne!(id, a.id);
    let r = s.find_record(&m, &id).unwrap();
    assert_eq!((r.values["symbol"].clone(), r.values["source"].clone(), r.values["open"].clone()), (json!("SOL/USDT"), json!("mine"), json!(7.0)));
    assert!(s.series().unwrap().iter().any(|x| x.symbol.base == "SOL" && x.count == 1));
    // and bars can be created
    let n = bars::create_record(
        &s,
        &m,
        &v(json!({"source": "mine", "symbol": "SOL/USDT", "period": "5m", "timestamp": 5, "open": 1, "high": 2, "low": 0.5, "close": 1.5})),
    )
    .unwrap();
    assert_eq!(s.find_record(&m, &n).unwrap().values["volume"], json!(0.0));
    assert!(
        bars::create_record(
            &s,
            &m,
            &v(json!({"source": "mine", "symbol": "SOL/USDT", "period": "5m", "timestamp": 5, "open": 1, "high": 2, "low": 0.5, "close": 1.5}))
        )
        .is_err()
    );
    assert!(bars::create_record(&s, &m, &v(json!({"source": "mine"}))).unwrap_err().to_string().contains("needs"));
}

#[test]
fn approval_follows_the_source_and_no_toolchain_is_explained() {
    let s = store();
    let c = s.create_code_space("x", "", Lang::Math, "a = close", CodeInput::default()).unwrap();
    assert!(!approved(&c));
    approve(&s, "cli", &c).unwrap();
    assert!(approved(&s.code_space(&c.id).unwrap().unwrap()));
    assert_ne!(build_key(Lang::Math, "a = close"), build_key(Lang::Rust, "a = close"));
    assert_eq!(trust_hash_of(Lang::Math, "a").unwrap(), trust_hash_of(Lang::Math, "a").unwrap());
    // run checks approval first
    let c2 = s.create_code_space("y", "", Lang::Math, "a = close", CodeInput::default()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    assert!(run(&s, &CodeEnv::new(dir.path()), "cli", &c2, &mut |_| {}).unwrap_err().to_string().contains("approve"));
    for l in Lang::ALL {
        if l != Lang::Plugin {
            check(&CodeEnv::new(dir.path()), l, template(l)).unwrap();
        }
    }
    assert!(load(&dir.path().join("missing.so")).is_err());
}

#[test]
fn python_compiles_in_process() {
    if !have_cargo() {
        eprintln!("skipped: no cargo");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let env = CodeEnv::new(dir.path());
    let s = store();
    let src = "# @title Range\n# @view value\ndef _half(x: float) -> float:\n    return x / 2.0\n\ndef spread(high: float, low: float) -> float:\n    return _half(high - low) * 2.0\n\ndef big(close: float, symbol: str) -> bool:\n    return close > 30.0 and symbol == \"BTC/USDT\"\n";
    let c = create(&s, "cli", "py", "", Lang::Python, src, bars_input()).unwrap();
    approve(&s, "cli", &c).unwrap();
    let c = s.code_space(&c.id).unwrap().unwrap();
    let mut log = vec![];
    let info = run(&s, &env, "cli", &c, &mut |l| log.push(l.to_string())).unwrap_or_else(|e| panic!("{e}\n{}", log.join("\n")));
    assert_eq!(info.status, "ok", "{info:?}");
    let (head, rows) = run_view(&info).unwrap().as_table();
    let i = head.iter().position(|h| h == "spread").unwrap();
    let j = head.iter().position(|h| h == "big").unwrap();
    assert_eq!((rows[0][i].as_str(), rows[0][j].as_str()), ("40.5", "1"), "{head:?} {rows:?}");
}

#[test]
#[ignore = "compiles the polars crate (minutes); run with --ignored"]
fn polars_compiles_and_runs() {
    let dir = tempfile::tempdir().unwrap();
    let env = CodeEnv::new(dir.path());
    let s = store();
    let src = "@view table\nwhere col(\"symbol\").eq(lit(\"BTC/USDT\"))\nret = col(\"close\").pct_change(lit(1)) * lit(100)\nhl = col(\"high\") - col(\"low\")\n";
    let c = create(&s, "cli", "pl", "", Lang::Polars, src, bars_input()).unwrap();
    approve(&s, "cli", &c).unwrap();
    let c = s.code_space(&c.id).unwrap().unwrap();
    let mut log = vec![];
    let info = run(&s, &env, "cli", &c, &mut |l| log.push(l.to_string())).unwrap_or_else(|e| panic!("{e}\n{}", log.join("\n")));
    assert_eq!(info.status, "ok", "{info:?}");
    let (head, rows) = run_view(&info).unwrap().as_table();
    assert_eq!(rows.len(), 40, "{head:?}");
}

#[test]
fn inputs_read_back_what_they_print() {
    let i = parse_input("bars where symbol = BTC/USDT; close > 3 matching \"a b\" group symbol sort -timestamp,close limit 7").unwrap();
    assert_eq!(i.model, "bars");
    assert_eq!(i.filter, vec!["symbol = BTC/USDT", "close > 3"]);
    assert_eq!(i.text, "a b");
    assert_eq!(i.group, vec!["symbol"]);
    assert_eq!(i.sort, vec!["-timestamp", "close"]);
    assert_eq!(i.limit, Some(7));
    assert_eq!(parse_input(&i.text()).unwrap(), i);
    // `;` before a keyword reads too
    assert_eq!(parse_input("bars where symbol = BTC/USDT; sort timestamp; limit 3").unwrap().sort, vec!["timestamp"]);
    assert_eq!(parse_input("none").unwrap(), CodeInput::default());
    assert_eq!(parse_input("  ").unwrap().text(), "none");
    for bad in ["bars limit x", "bars sort a sort b", "bars where", "two words", "bars where close"] {
        assert!(parse_input(bad).is_err(), "{bad}");
    }
}
