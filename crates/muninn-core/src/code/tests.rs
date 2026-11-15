//! Building and loading real plugins (needs cargo; skipped without it). Polars is
//! `#[ignore]`d (it compiles the polars crate).
use super::*;
use crate::model::{Bar, Duration, Symbol};
use crate::store::{Design, Field, FieldType, RecordQuery};
use serde_json::{Value, json};

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

/// The code home is process-wide: tests that point it at a temp dir take turns.
struct HomeGuard {
    prev: Option<std::path::PathBuf>,
    _turn: std::sync::MutexGuard<'static, ()>,
}

impl HomeGuard {
    fn new(home: &std::path::Path) -> Self {
        static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
        HomeGuard { prev: replace_home(Some(home)), _turn: turn }
    }
}

impl Drop for HomeGuard {
    fn drop(&mut self) {
        replace_home(self.prev.as_deref());
    }
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

/// A one-shot HTTP server on localhost answering every request with `body` (and an
/// `x-next-since` header); gives its base URL.
fn serve(body: &'static str) -> String {
    use std::io::{Read, Write};
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    std::thread::spawn(move || {
        for s in l.incoming().take(4) {
            let mut s = s.unwrap();
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf);
            let _ = write!(
                s,
                "HTTP/1.1 200 OK\r\ncontent-type: text/csv\r\nx-next-since: 77\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    format!("http://{addr}/data.csv")
}

#[test]
fn the_connector_template_runs_in_a_dry_run() {
    if !have_cargo() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let env = CodeEnv::new(dir.path());
    let s = store();
    let c = create(&s, "cli", "feed", "", Lang::Rust, CONNECTOR_TEMPLATE, CodeInput::default()).unwrap();
    approve(&s, "cli", &c).unwrap();
    let c = s.code_space(&c.id).unwrap().unwrap();
    // outside a connector run the methods say where they work
    let info = run(&s, &env, "cli", &c, &mut |_| {}).unwrap();
    assert_eq!(info.status, "error");
    let url = serve("ts,price\n1,2\n");
    let mut opts = serde_json::Map::new();
    opts.insert("url".into(), json!(url));
    // the context's options are what `host.option` reads in a connector run
    let d = connector::DryRun::new("feed", None, Some("feed-pipe".into()), Some("rs1".into()), opts);
    let (info, d) = connector::dry_run(d, || run(&s, &env, "cli", &c, &mut |_| {}));
    let info = info.unwrap();
    assert_eq!(info.status, "ok", "{info:?}");
    let m: Vec<&str> = d.calls.iter().map(|c| c.method.as_str()).collect();
    assert_eq!(m, ["fetch", "store_raw", "pipe", "run_ruleset", "state_set"], "{:?}", d.calls);
    assert_eq!(d.calls[0].params["status"], 200);
    assert_eq!((d.stored, d.bytes), (1, 13));
    assert_eq!(d.calls[1].params["preview"], "ts,price\n1,2\n");
    assert_eq!(d.calls[3].params["ruleset_id"], "rs1");
    assert_eq!(d.state["since"].as_f64(), Some(77.0));
    let (_, rows) = run_view(&info).unwrap().leaves()[0].as_table();
    assert_eq!(rows[0], vec!["dry-1", "13", "0", "0", "true"]);
    // nothing was stored
    assert!(s.sources().unwrap().iter().all(|x| x.name != "feed"));
}

/// A connector main that stores two raw bar rows, then pipes and runs the rule set when
/// the context has them.
const BARS_CONNECTOR: &str = r#"
use muninn_plugin::prelude::*;

pub fn run(host: &Host) -> Result<View> {
    let ctx = host.context()?;
    let rows = "{\"ts\":1704067200000,\"o\":1,\"h\":2,\"l\":0.5,\"c\":1.5,\"v\":3}\n{\"ts\":1704067500000,\"o\":1.5,\"h\":2,\"l\":1,\"c\":1.8,\"v\":4}\n";
    host.store_raw("page-1", "ndjson", rows.as_bytes())?;
    let mut inserted = 0usize;
    let mut rules = 0usize;
    if ctx.pipeline.is_some() {
        inserted = host.pipe()?.inserted;
    }
    if ctx.ruleset_id.is_some() {
        rules = host.run_ruleset(None)?.rules_run;
    }
    Ok(View::table("Connector run", &["inserted", "rules run"]).row(vec![inserted.into(), rules.into()]))
}
"#;

const TOUCH: &str = r#"
use muninn_plugin::prelude::*;

pub fn run(_host: &Host) -> Result<View> {
    Ok(View::table("Touched", &["n"]).row(vec![1.0.into()]))
}
"#;

fn approved_space(s: &Store, name: &str, kind: crate::store::SpaceKind, src: &str) -> crate::store::Space {
    let c = create(s, "cli", name, "", Lang::Rust, src, CodeInput::default()).unwrap();
    let sp = spaces::create(s, "cli", name, "", kind, &c).unwrap();
    spaces::approve(s, "cli", &sp, true).unwrap();
    s.space(&sp.id).unwrap().unwrap()
}

fn builtin_space(s: &Store, name: &str) {
    let (sp, _) = parsers::install_space(s, "cli", parsers::builtin(name).unwrap()).unwrap();
    spaces::approve(s, "cli", &sp, true).unwrap();
}

#[test]
fn a_connector_space_dry_runs_through_the_space_runner() {
    if !have_cargo() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let env = CodeEnv::new(dir.path());
    let s = store();
    let c = create(&s, "cli", "feed", "", Lang::Rust, BARS_CONNECTOR, CodeInput::default()).unwrap();
    let sp = spaces::create(&s, "cli", "feed", "", crate::store::SpaceKind::IngestConnector, &c).unwrap();
    let dry = || connector::DryRun::new("feed", None, Some("p1".into()), Some("rs1".into()), serde_json::Map::new());
    // not approved yet: asks first
    let (r, _) = connector::dry_run_space(&s, &env, "cli", &sp, dry(), &mut |_| {});
    assert!(matches!(r, Err(Error::NeedsConfirmation(_))), "{r:?}");
    spaces::approve(&s, "cli", &sp, true).unwrap();
    let sp = s.space(&sp.id).unwrap().unwrap();
    let (info, d) = connector::dry_run_space(&s, &env, "cli", &sp, dry(), &mut |_| {});
    let info = info.unwrap();
    assert_eq!(info.status, "ok", "{info:?}");
    let m: Vec<&str> = d.calls.iter().map(|c| c.method.as_str()).collect();
    assert_eq!(m, ["store_raw", "pipe", "run_ruleset"], "{:?}", d.calls);
    assert_eq!((d.stored, d.bytes), (1, 110));
    assert!(!connector::active());
    // only connectors run this way
    let a = approved_space(&s, "plain", crate::store::SpaceKind::Analysis, TOUCH);
    let (r, _) = connector::dry_run_space(&s, &env, "cli", &a, dry(), &mut |_| {});
    assert!(r.unwrap_err().to_string().contains("not an ingest connector"));
}

/// A live connector run whose pipeline has code-space steps (parse-ndjson, to-bars) and
/// whose rule set has a `run-space` rule: both used to fail with "was started while
/// another code space runs".
#[test]
fn a_live_connector_run_pipes_and_runs_rules_through_code_spaces() {
    use crate::code::connector::Live;
    use crate::code::pipeline::Graph;
    use crate::store::{NewRule, RuleSpec, SpaceKind, Then};
    if !have_cargo() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let env = CodeEnv::new(dir.path());
    // `host.pipe()` builds its own env from the store's home
    let _home = HomeGuard::new(dir.path());
    let s = store();
    let tv = s.add_source("tv", "ndjson", None).unwrap();
    for n in ["parse-ndjson", "to-bars"] {
        builtin_space(&s, n);
    }
    let g = Graph::parse(
        &["src=-", "p=space:parse-ndjson", "b=space:to-bars?symbol=SOL/USDT&duration=5m&source=tv", "m=model:bars"].map(String::from),
        &["src>p>b>m".to_string()],
    )
    .unwrap();
    let pl = s.create_pipeline("tv-bars", "", &g).unwrap();
    let touch = approved_space(&s, "touch", SpaceKind::Analysis, TOUCH);
    let rule = s
        .create_rule(&NewRule {
            name: "touch".into(),
            spec: RuleSpec {
                on: vec!["connector.run".into()],
                then: vec![Then::RunSpace { space: touch.id.clone(), input: Some("action".into()) }],
                ..Default::default()
            },
            enabled: true,
            ..Default::default()
        })
        .unwrap();
    let set = s.create_ruleset("after", "", "standalone", None).unwrap();
    s.add_ruleset_rule(&set.id, &rule.id, None).unwrap();
    // rule-set rules honour their `on` and `if`: only the ones that fit the connector.run action run
    let cond = |c: &str| crate::rules::parse_condition(c).unwrap();
    let status = |text: &str| vec![Then::Status { text: text.into() }];
    for (name, on, when) in [
        ("has-data", vec!["connector.run"], vec![cond("inserted > 0")]),
        ("gap-alarm", vec!["connector.run"], vec![cond("inserted = 0")]),
        ("other-schedule", vec!["connector.run"], vec![cond("name = nope")]),
        ("implicit", vec!["schedule"], vec![]),
        ("wrong-kind", vec!["import"], vec![]),
        ("any-trigger", vec!["*"], vec![cond("stored = 1")]),
    ] {
        let r = s
            .create_rule(&NewRule {
                name: name.into(),
                spec: RuleSpec { on: on.into_iter().map(String::from).collect(), when, then: status(name), ..Default::default() },
                enabled: true,
                ..Default::default()
            })
            .unwrap();
        s.add_ruleset_rule(&set.id, &r.id, None).unwrap();
    }
    let feed = approved_space(&s, "feed", SpaceKind::IngestConnector, BARS_CONNECTOR);
    let mk_live = |run_id: i64| Live {
        schedule_id: "sched".into(),
        schedule_name: "tv".into(),
        run_id,
        plugin_id: feed.id.clone(),
        source_id: tv.id.clone(),
        source_name: tv.name.clone(),
        pipeline_id: Some(pl.id.clone()),
        ruleset_id: Some(set.id.clone()),
        options: serde_json::Map::new(),
        last_success_at: None,
        stored: 0,
        bytes: 0,
        inserted: 0,
        landed: 0,
        piped: 0,
        ruleset_ran: 0,
        warnings: vec![],
        state: serde_json::Map::new(),
    };
    let bars = s.find_model("bars").unwrap();
    let before = s.count_records(&bars).unwrap();
    let (info, live) = connector::live_run(mk_live(1), || spaces::run_connector(&s, &env, "scheduler", &feed, "test", &mut |_| {}));
    let info = info.unwrap();
    assert_eq!(info.status, "ok", "{info:?} {:?}", live.warnings);
    assert!(live.warnings.is_empty(), "{:?}", live.warnings);
    assert_eq!((live.stored, live.piped, live.inserted, live.landed), (1, 1, 2, 2));
    assert_eq!(s.count_records(&bars).unwrap() - before, 2);
    let (_, rows) = run_view(&info).unwrap().leaves()[0].as_table();
    assert_eq!(rows[0], vec!["2", "4"], "touch, has-data, implicit and any-trigger ran");
    let runs = |n: &str| s.find_rule(n).unwrap().runs;
    assert_eq!(["touch", "has-data", "implicit", "any-trigger"].map(runs), [1, 1, 1, 1]);
    assert_eq!(["gap-alarm", "other-schedule", "wrong-kind"].map(runs), [0, 0, 0], "skipped, not failed");
    // the connector.run action carries the fields rules and conditions use
    let a = s.actions_of_kind("connector.run").unwrap();
    assert_eq!((a.len(), &a[0].params["schedule"], &a[0].params["inserted"], &a[0].params["stored"]), (1, &json!("sched"), &json!(2), &json!(1)));
    // the same data again: `inserted` counts new records only (0), `landed` every row given (2)
    let (info, live2) = connector::live_run(mk_live(2), || spaces::run_connector(&s, &env, "scheduler", &feed, "test", &mut |_| {}));
    let info = info.unwrap();
    assert_eq!(info.status, "ok", "{:?}", live2.warnings);
    assert_eq!(run_view(&info).unwrap().leaves()[0].as_table().1[0][0], "0", "host.pipe() says 0 inserted");
    assert_eq!((live2.piped, live2.inserted, live2.landed), (1, 0, 2));
    assert_eq!(s.count_records(&bars).unwrap() - before, 2, "no bar was added twice");
    let a = s.actions_of_kind("connector.run").unwrap();
    assert_eq!((&a[1].params["inserted"], &a[1].params["landed"]), (&json!(0), &json!(2)));
    // so the gap alarm fires and "has-data" is skipped, this time
    assert_eq!(["has-data", "gap-alarm"].map(runs), [1, 1]);
    // the guard is gone: code still can't start code outside a space's main calling
    assert!(!spaces::nested_ok());
}

/// A connector that counts its runs in state and fails when the schedule says `fail`.
const COUNTER_CONNECTOR: &str = r#"
use muninn_plugin::prelude::*;

pub fn run(host: &Host) -> Result<View> {
    let n = host.state_get("n")?.and_then(|v| v.as_i64()).unwrap_or(0) + 1;
    host.state_set("n", n)?;
    host.state_set("last", "ok")?;
    if host.option("fail").is_some() {
        return Err(Error::new("boom"));
    }
    Ok(View::table("Counter", &["n"]).row(vec![(n as usize).into()]))
}
"#;

/// A connector schedule's state moves only when its run succeeds: a failing run, or one
/// that never called a pipe or rule set it was given, leaves the cursor where it was.
#[test]
fn connector_state_is_kept_only_when_the_run_succeeds() {
    if !have_cargo() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let _home = HomeGuard::new(dir.path());
    let s = store();
    let src = s.add_source("feed-src", "ndjson", None).unwrap();
    let feed = approved_space(&s, "feed", crate::store::SpaceKind::IngestConnector, COUNTER_CONNECTOR);
    let set = s.create_ruleset("never-run", "", "standalone", None).unwrap();
    let mut def = crate::store::ScheduleDef {
        name: "counter".into(),
        target_kind: "connector".into(),
        plugin_id: Some(feed.id.clone()),
        source_id: Some(src.id.clone()),
        overlap: "skip".into(),
        catch_up: "once".into(),
        max_attempts: 1,
        enabled: true,
        ..Default::default()
    };
    let now = crate::store::now_ms();
    crate::schedule::apply_timing(&mut def, &crate::schedule::Timing { run_every: Some("1h".into()), ..Default::default() }, now).unwrap();
    let mut def = s.save_schedule(&def).unwrap();
    let n = |s: &Store, def: &crate::store::ScheduleDef| connector::load_state(s, &def.id).unwrap().get("n").cloned();
    assert_eq!(crate::schedule::run_now(&s, &def, "t", now).unwrap().status, "success");
    assert_eq!(n(&s, &def), Some(json!(1)));
    // the connector fails: its state changes are dropped
    def.options.insert("fail".into(), json!("1"));
    let r = crate::schedule::run_now(&s, &def, "t", now).unwrap();
    assert_eq!((r.status.as_str(), n(&s, &def)), ("failed", Some(json!(1))), "{:?}", r.error);
    // it never calls the rule set it was given: failed, state untouched
    def.options.clear();
    def.ruleset_id = Some(set.id.clone());
    let r = crate::schedule::run_now(&s, &def, "t", now).unwrap();
    assert_eq!((r.status.as_str(), n(&s, &def)), ("failed", Some(json!(1))), "{:?}", r.error);
    assert!(r.error.unwrap().contains("did not call run_ruleset"));
    // without the rule set it succeeds and the cursor moves on from where it was
    def.ruleset_id = None;
    assert_eq!(crate::schedule::run_now(&s, &def, "t", now).unwrap().status, "success");
    assert_eq!(n(&s, &def), Some(json!(2)));
}

#[test]
fn connector_state_is_shown_set_and_cleared() {
    use crate::schedule::*;
    use crate::store::{ScheduleDef, ScheduleRun};
    const T0: i64 = 1_794_319_200_000;
    let st = store();
    let rule_set_schedule = ScheduleDef {
        name: "rs".into(),
        target_kind: "ruleset".into(),
        overlap: "skip".into(),
        catch_up: "once".into(),
        max_attempts: 1,
        enabled: true,
        ..Default::default()
    };
    let rs = st
        .save_schedule(&{
            let mut x = rule_set_schedule;
            x.ruleset_id = Some(st.create_ruleset("r", "", "standalone", None).unwrap().id);
            apply_timing(&mut x, &Timing { run_every: Some("1h".into()), ..Default::default() }, T0).unwrap();
            x
        })
        .unwrap();
    let src = st.add_source("feed", "ndjson", None).unwrap();
    let feed = approved_space(&st, "feed", crate::store::SpaceKind::IngestConnector, COUNTER_CONNECTOR);
    let mut c =
        ScheduleDef { name: "feed".into(), target_kind: "connector".into(), plugin_id: Some(feed.id.clone()), source_id: Some(src.id), ..rs.clone() };
    c.id = String::new();
    c.ruleset_id = None;
    apply_timing(&mut c, &Timing { run_every: Some("15m".into()), ..Default::default() }, T0).unwrap();
    let c = st.save_schedule(&c).unwrap();
    assert!(connector_state(&st, &c).unwrap().is_empty());
    let set = |k: &str, v: &str| parse_state_pair(&format!("{k}={v}")).unwrap();
    let state = edit_connector_state(&st, &c, None, &[set("BTC/USDT", "1700000000000"), set("note", "\"hi\""), set("cur", "{\"a\":[1,2]}")]).unwrap();
    assert_eq!(state.len(), 3);
    // a later read (and a run's load) sees it, under the schedule's id
    assert_eq!(connector::load_state(&st, &c.id).unwrap()["cur"]["a"][1], 2);
    // renaming keeps the id, so the state stays
    let mut renamed = c.clone();
    renamed.name = "feed2".into();
    let renamed = st.save_schedule(&renamed).unwrap();
    assert_eq!((renamed.id.as_str(), connector_state(&st, &renamed).unwrap().len()), (c.id.as_str(), 3));
    // null removes; one key; a missing key is an error; then all
    let state = edit_connector_state(&st, &c, None, &[set("note", "null")]).unwrap();
    assert!(!state.contains_key("note"));
    let state = edit_connector_state(&st, &c, Some(&StateClear::Key("cur".into())), &[]).unwrap();
    assert_eq!(state.keys().collect::<Vec<_>>(), ["BTC/USDT"]);
    assert!(edit_connector_state(&st, &c, Some(&StateClear::Key("nope".into())), &[]).unwrap_err().to_string().contains("no key `nope`"));
    // clear first, then set
    let state = edit_connector_state(&st, &c, Some(&StateClear::All), &[set("since", "5")]).unwrap();
    assert_eq!(serde_json::to_value(&state).unwrap(), json!({ "since": 5 }));
    // bad input
    assert!(parse_state_pair("novalue").is_err() && parse_state_pair("=1").is_err());
    assert!(parse_state_pair("k=text").unwrap_err().to_string().contains("not JSON"));
    // not while a run is going
    let id =
        st.insert_run(&ScheduleRun { schedule_id: c.id.clone(), status: "pending".into(), timestamp: T0, attempt: 1, ..Default::default() }).unwrap();
    st.claim_run(id, "h", T0).unwrap();
    assert!(edit_connector_state(&st, &c, Some(&StateClear::All), &[]).unwrap_err().to_string().contains("is running"));
    // rule set schedules have none; removing a schedule drops its state
    assert!(connector_state(&st, &rs).is_err());
    st.remove_schedule(&c.id).unwrap();
    assert!(st.setting(&connector::state_key(&c.id)).unwrap().is_none());
}

/// Rows (`ts` ms, `close`, `period`) as a frame; a `None` period is a null cell.
fn bar_rows(rows: &[(i64, Option<Value>)], with_period: bool) -> Value {
    let mut cols = json!({
        "timestamp": rows.iter().map(|r| json!(r.0)).collect::<Vec<_>>(),
        "close": rows.iter().map(|_| json!(10.0)).collect::<Vec<_>>(),
    });
    let mut fields = vec![json!({ "name": "timestamp", "type": "real" }), json!({ "name": "close", "type": "real" })];
    if with_period {
        cols["period"] = Value::Array(rows.iter().map(|r| r.1.clone().unwrap_or(Value::Null)).collect());
        fields.push(json!({ "name": "period", "type": "text" }));
    }
    json!({ "model": "t", "fields": fields, "ids": (0..rows.len()).map(|i| i.to_string()).collect::<Vec<_>>(), "columns": cols, "groups": [] })
}

/// to-bars over `frame`: the Bars rows (source, symbol, period, timestamp…) and the Rejected rows.
fn to_bars_rows(frame: Value, opts: &str) -> (Vec<Vec<String>>, Vec<Vec<String>>) {
    let dir = tempfile::tempdir().unwrap();
    let env = CodeEnv::new(dir.path());
    let s = store();
    builtin_space(&s, "to-bars");
    let step = pipe::Step::Space(format!("to-bars?symbol=SOL/USDT{opts}"));
    let p = pipe::run(&s, &env, "cli", &pipe::Source::Frame(frame), &[step], &[], &mut |_| {}).unwrap();
    let info = &p.runs[0].1;
    assert_eq!(info.status, "ok", "{info:?}");
    let v = run_view(info).unwrap();
    let leaves = v.leaves();
    let bars = leaves[0].as_table().1;
    let rejected = leaves.get(1).map(|l| l.as_table().1).unwrap_or_default();
    (bars, rejected)
}

#[test]
fn to_bars_takes_each_rows_own_period_column() {
    if !have_cargo() {
        return;
    }
    let t0 = 1_704_067_200_000i64; // 2024-01-01 00:00 UTC
    let n = |x: f64| Some(json!(x));
    let t = |x: &str| Some(json!(x));
    // 1m and 5m bars mixed: seconds as a number, `5m` and `300` as text
    let (bars, rejected) =
        to_bars_rows(bar_rows(&[(t0, n(60.0)), (t0 + 60_000, t("1m")), (t0 + 300_000, t("5m")), (t0 + 600_000, t("300"))], true), "");
    assert!(rejected.is_empty(), "{rejected:?}");
    let periods: Vec<&str> = bars.iter().map(|r| r[2].as_str()).collect();
    assert_eq!(periods, ["60", "60", "300", "300"], "{bars:?}");
    // a 5m row off the 5m grid is rejected by its own length (a 1m row there is fine)
    let (bars, rejected) = to_bars_rows(bar_rows(&[(t0 + 60_000, n(60.0)), (t0 + 60_000, t("5m"))], true), "");
    assert_eq!((bars.len(), rejected.len()), (1, 1), "{bars:?} {rejected:?}");
    assert!(rejected[0][1].contains("not aligned to 300"), "{rejected:?}");
    // a missing or unreadable period is rejected with a reason, whatever `check` says
    let (bars, rejected) =
        to_bars_rows(bar_rows(&[(t0, n(60.0)), (t0 + 60_000, None), (t0 + 120_000, t("soon")), (t0 + 180_000, n(-5.0))], true), "&check=false");
    assert_eq!(bars.len(), 1, "{bars:?}");
    let rows: Vec<&str> = rejected.iter().map(|r| r[0].as_str()).collect();
    assert_eq!(rows, ["2", "3", "4"]);
    assert!(rejected.iter().all(|r| r[1].contains("period is missing or not a duration")), "{rejected:?}");
    // the duration option still wins over a period column
    let (bars, _) = to_bars_rows(bar_rows(&[(t0, t("1h")), (t0 + 300_000, t("1h"))], true), "&duration=5m");
    assert_eq!(bars.iter().map(|r| r[2].as_str()).collect::<Vec<_>>(), ["300", "300"]);
}

#[test]
fn to_bars_without_a_period_column_keeps_the_most_common_step() {
    if !have_cargo() {
        return;
    }
    let t0 = 1_704_067_200_000i64;
    let rows: Vec<(i64, Option<Value>)> = (0..4).map(|i| (t0 + i * 300_000, None)).collect();
    let (bars, rejected) = to_bars_rows(bar_rows(&rows, false), "");
    assert!(rejected.is_empty(), "{rejected:?}");
    assert_eq!(bars.iter().map(|r| r[2].as_str()).collect::<Vec<_>>(), ["300"; 4]);
    // an explicit duration still checks alignment: only the row on the hour is kept
    let (bars, rejected) = to_bars_rows(bar_rows(&rows, false), "&duration=1h");
    assert_eq!((bars.len(), rejected.len()), (1, 3));
}
