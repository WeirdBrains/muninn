//! Data tiles: code results, pipeline views (the 3-column live case) and the monitor.
use super::frametile::{FrameTile, open};
use super::layout::TileLayout;
use super::tests::{app, key, render, screen_text};
use super::*;
use crate::code::pipe::{note_frame, read_data};
use crate::code::pipeline::{Graph, run};
use crate::store::{Design, Field, FieldType, Lang};

fn csv(rows: &[(i64, f64, f64)]) -> serde_json::Value {
    let mut s = String::from("timestamp,px,vol\n");
    for (t, p, v) in rows {
        s.push_str(&format!("{t},{p},{v}\n"));
    }
    read_data(&s, "stdin").unwrap()
}

fn typ(a: &mut App, s: &str) {
    for c in s.chars() {
        a.on_key(key(if c == '\n' { KeyCode::Enter } else { KeyCode::Char(c) }));
    }
}

/// A code space's result in a tile: a chart by default, t for the table, c for columns;
/// it follows runs made anywhere on the same home.
#[test]
fn a_code_result_tile_charts_tables_and_follows_runs() {
    let (_t, mut a) = app();
    a.tick();
    let id = open(&mut a, FrameTile::new("code:calc")).unwrap();
    assert!(a.layout.tiles().len() >= 2, "the series tile stays; the data tile splits off");
    assert!(screen_text(&mut a).contains("no result yet"));
    // a run elsewhere (`mnnn pipe … code:calc`) records its output
    note_frame(&a.store, "cli", "code:calc", &csv(&[(1_704_067_200_000, 1.0, 10.0), (1_704_067_500_000, 2.0, 30.0)]));
    a.tick();
    let s = screen_text(&mut a);
    assert!(s.contains("chart · code:calc · 2 rows"), "{s}");
    assert!(s.contains("px") && s.contains("vol"), "the legend names the series: {s}");
    a.layout.set_focus(id);
    a.on_key(key(KeyCode::Char('t')));
    let s = screen_text(&mut a);
    assert!(s.contains("table · code:calc") && s.contains("timestamp") && s.contains("30"), "{s}");
    a.on_key(key(KeyCode::Char('c')));
    assert!(matches!(a.modal, Some(super::modal::Modal::Prompt(_))));
    typ(&mut a, "px\n");
    assert_eq!(a.layout.tile(id).unwrap().frame.as_ref().unwrap().columns, vec!["px".to_string()]);
    let s = screen_text(&mut a);
    assert!(s.contains("· px") && !s.contains("vol "), "{s}");
    // the next run updates it
    note_frame(&a.store, "cli", "code:calc", &csv(&[(1, 5.0, 1.0), (2, 6.0, 1.0), (3, 7.0, 1.0)]));
    a.tick();
    assert!(screen_text(&mut a).contains("3 rows"));
    // it is kept with the layout
    let saved: TileLayout = serde_json::from_value(serde_json::to_value(&a.layout).unwrap()).unwrap();
    assert_eq!(saved.tile(id).unwrap().frame.as_ref().unwrap().key, "code:calc");
}

/// A run in the TUI updates the tile too, and a piped run that needs a build builds on the
/// thread (the UI is not held up).
#[test]
fn a_tui_run_updates_the_tile_and_piped_builds_are_async() {
    if crate::code::cargo().is_err() {
        eprintln!("skipped: no cargo");
        return;
    }
    let (_t, mut a) = app();
    let j = a.store.create_journal("Notes", None, crate::store::EntryFormat::Markdown).unwrap();
    a.store.add_entry(&j.id, "one", "alpha beta", None).unwrap();
    let c = a
        .store
        .create_code_space("dbl", "", Lang::Math, &format!("@view table\nw2 = words * 2\n# {}", crate::store::new_id()), Default::default())
        .unwrap();
    crate::code::approve(&a.store, "test", &c).unwrap();
    super::code::open_space(&mut a, &c.id);
    super::code::run_piped(&mut a, crate::code::pipe::Source::Journal("Notes".into()), "Notes");
    assert!(a.code.building.is_some(), "the build runs on a thread: {:?}", a.code.msg);
    for _ in 0..600 {
        a.tick();
        if a.code.building.is_none() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    let r = a.code.result.clone().unwrap_or_else(|| panic!("{:?} {:?}", a.code.msg, a.code.log));
    assert_eq!(r.status, "ok");
    let f = a.frames.data.get("code:dbl").expect("the result frame");
    assert_eq!(f["columns"]["w2"][0].as_f64(), Some(4.0), "{f}");
    a.on_key(key(KeyCode::Char('T')));
    assert!(a.modal.is_none());
    assert!(screen_text(&mut a).contains("code:dbl · 1 rows"));
}

fn feed(a: &App) -> crate::store::pipelines::Pipeline {
    for m in ["Prices", "Volumes", "Ticks"] {
        a.store
            .create_model(
                m,
                "",
                Design { fields: vec![Field::new("px", FieldType::Real), Field::new("vol", FieldType::Real)], ..Default::default() },
                true,
            )
            .unwrap();
    }
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let g = Graph::parse(
        &s(&["in=-", "price=view:chart:px", "volume=view:chart:vol", "tape=view:table", "p=model:Prices", "v=model:Volumes", "t=model:Ticks"]),
        &s(&["in>price>p", "in>volume>v", "in>tape>t"]),
    )
    .unwrap();
    a.store.create_pipeline("feed", "", &g).unwrap()
}

/// Michael's case: data piped in on the command line, a pipeline with three views, and an
/// open TUI shows them as three columns, live.
#[test]
fn a_pipeline_run_anywhere_opens_its_views_as_three_live_columns() {
    let (_t, mut a) = app();
    a.layout = TileLayout::single(None);
    let p = feed(&a);
    a.tick();
    render(&mut a, 180, 45);
    // as `cat ticks.csv | mnnn pipeline run feed - --chunk 2` would, from another process
    let env = crate::code::CodeEnv::new(&a.paths.home);
    let r = run(&a.store, &env, "cli", &p, Some(csv(&[(1_704_067_200_000, 100.0, 5.0), (1_704_067_500_000, 101.0, 7.0)])), &mut |_| {}).unwrap();
    assert!(r.ok(), "{:?}", r.nodes);
    a.tick();
    let keys: Vec<String> = a.layout.tiles().iter().filter_map(|t| t.frame.as_ref().map(|f| f.key.clone())).collect();
    assert_eq!(keys, vec!["view:feed/price", "view:feed/volume", "view:feed/tape"]);
    assert_eq!(a.layout.tiles().len(), 3, "three views: three columns");
    let rects = a.layout.rects(Rect::new(0, 0, 180, 40));
    let ws: Vec<u16> = rects.iter().map(|(_, r)| r.width).collect();
    assert!(rects.iter().all(|(_, r)| r.y == 0 && r.height == 40) && ws.iter().max().unwrap() - ws.iter().min().unwrap() <= 1, "{rects:?}");
    let s = render(&mut a, 180, 45);
    assert!(s.contains("view:feed/price · 2 rows") && s.contains("view:feed/tape"), "{s}");
    // the table view shows the rows
    assert!(s.contains("table · view:feed/tape") && s.contains("101"), "{s}");
    // the next chunk updates them in place: no new tiles
    run(
        &a.store,
        &env,
        "cli",
        &p,
        Some(csv(&[(1_704_067_800_000, 103.0, 9.0), (1_704_068_100_000, 99.0, 2.0), (1_704_068_400_000, 98.0, 1.0)])),
        &mut |_| {},
    )
    .unwrap();
    a.tick();
    assert_eq!(a.layout.tiles().len(), 3);
    assert!(render(&mut a, 180, 45).contains("view:feed/volume · 3 rows"));
}

/// The monitor: each node's status, rows, rate and errors; Enter opens a node.
#[test]
fn the_pipeline_monitor_follows_runs_and_opens_nodes() {
    let (_t, mut a) = app();
    a.layout = TileLayout::single(None);
    let p = feed(&a);
    a.tick();
    let mid = open(&mut a, FrameTile::new("monitor:feed")).unwrap();
    let s = render(&mut a, 180, 45);
    assert!(s.contains("monitor · feed") && s.contains("not run yet") && s.contains("idle"), "{s}");
    let env = crate::code::CodeEnv::new(&a.paths.home);
    run(&a.store, &env, "cli", &p, Some(csv(&[(1, 1.0, 1.0), (2, 2.0, 2.0)])), &mut |_| {}).unwrap();
    a.tick();
    let (nodes, landed, state) = super::frametile::monitor(&a, "feed");
    assert_eq!(state, "done");
    assert!(nodes.iter().all(|n| n.status == "ok"), "{nodes:?}");
    assert_eq!(nodes.iter().find(|n| n.id == "in").unwrap().rows_out, 2);
    assert_eq!(nodes.iter().find(|n| n.id == "p").unwrap().rows_in, 2);
    assert_eq!(landed.len(), 3, "{landed:?}");
    a.layout.set_focus(mid);
    let s = render(&mut a, 180, 45);
    assert!(s.contains("landed model:Prices"), "{s}");
    // Enter on the second node (a view) focuses its tile
    a.on_key(key(KeyCode::Char('j')));
    a.on_key(key(KeyCode::Enter));
    let f = a.layout.focused().frame.clone().unwrap();
    assert!(f.key.starts_with("view:feed/"), "{f:?}");
    // an unapproved code space waits; what is after it is skipped
    a.store.create_code_space("raw", "", Lang::Math, "x = px", Default::default()).unwrap();
    let mut g = p.graph.clone();
    g.add_node("c=code:raw").unwrap();
    g.add_node("cm=model:Ticks").unwrap();
    g.add_edges("in>c>cm").unwrap();
    let q = a.store.update_pipeline(&p.id, None, None, Some(&g)).unwrap();
    run(&a.store, &env, "cli", &q, Some(csv(&[(1, 1.0, 1.0)])), &mut |_| {}).unwrap();
    a.tick();
    let (nodes, _, state) = super::frametile::monitor(&a, "feed");
    let st = |id: &str| nodes.iter().find(|n| n.id == id).unwrap().status.clone();
    assert_eq!((st("c").as_str(), st("cm").as_str(), state.as_str()), ("waiting approval", "skipped", "error"), "{nodes:?}");
    // r runs it from the tile: refused until the space is approved
    a.layout.set_focus(mid);
    a.on_key(key(KeyCode::Char('r')));
    assert!(a.status.contains("approve code space raw"), "{}", a.status);
}
