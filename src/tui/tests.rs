//! TUI tests on ratatui's TestBackend (no real terminal).
use super::action::Action;
use super::modal::Modal;
use super::*;
use crate::prefs::{HeaderMode, ThemeMode};
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyEventState, KeyModifiers};
use ratatui::Terminal;

pub fn key(c: KeyCode) -> KeyEvent {
    KeyEvent { code: c, modifiers: KeyModifiers::NONE, kind: KeyEventKind::Press, state: KeyEventState::NONE }
}
pub fn mkey(c: KeyCode, m: KeyModifiers) -> KeyEvent {
    KeyEvent { code: c, modifiers: m, kind: KeyEventKind::Press, state: KeyEventState::NONE }
}
pub fn click(x: u16, y: u16) -> MouseEvent {
    MouseEvent { kind: MouseEventKind::Down(MouseButton::Left), column: x, row: y, modifiers: KeyModifiers::NONE }
}

pub fn app() -> (tempfile::TempDir, App) {
    let t = tempfile::tempdir().unwrap();
    let paths = Paths::resolve(Some(t.path().join("h")), None).unwrap();
    paths.ensure().unwrap();
    let dek = crate::crypto::Key32::random().unwrap();
    let mut store = Store::open(&paths.db, Some(&dek.0)).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    for f in ["ETHUSDT_5m.csv", "BTCUSDT_5m.csv"] {
        let opts = crate::ingest::IngestOptions { source: Some("binance".into()), ..Default::default() };
        crate::ingest::ingest_csv(&mut store, crate::interact::Interact::batch(true), opts, &root.join(f), &Default::default()).unwrap();
    }
    let prefs = Preferences::load_or_create(&paths).unwrap();
    let mut a = App::new(paths, store, dek, prefs, Some(ThemeMode::Dark)).unwrap();
    a.data.sync = true;
    (t, a)
}

pub fn render(a: &mut App, w: u16, h: u16) -> String {
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| a.draw(f)).unwrap();
    let buf = term.backend().buffer().clone();
    let mut s = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            s.push_str(buf[(x, y)].symbol());
        }
        s.push('\n');
    }
    s
}

pub fn screen_text(a: &mut App) -> String {
    render(a, 140, 45)
}

#[test]
fn header_large_and_compact() {
    let (_t, mut a) = app();
    let s = render(&mut a, 140, 45);
    assert!(s.contains("███╗   ███╗"), "{s}");
    assert!(s.contains(&format!("v{}", env!("CARGO_PKG_VERSION"))));
    assert!(s.contains("unlocked"));
    let s = render(&mut a, 140, 24);
    assert!(!s.contains("███╗   ███╗"));
    assert!(s.lines().next().unwrap().contains("Muninn v"), "{s}");
    a.prefs.tui.header = HeaderMode::Large;
    assert!(render(&mut a, 140, 24).contains("███╗   ███╗"));
    a.prefs.tui.header = HeaderMode::Compact;
    assert!(!render(&mut a, 140, 45).contains("███╗   ███╗"));
}

#[test]
fn menu_opens_with_f10_alt_and_mouse() {
    let (_t, mut a) = app();
    let s = screen_text(&mut a);
    for m in ["File", "View", "Data", "Keys", "Theme", "Help"] {
        assert!(s.contains(m), "{m}");
    }
    a.on_key(key(KeyCode::F(10)));
    assert_eq!(a.menu.open, Some(0));
    let s = screen_text(&mut a);
    assert!(s.contains("Import…") && s.contains("Preferences…"), "{s}");
    a.on_key(key(KeyCode::Right));
    assert_eq!(a.menu.open, Some(1));
    a.on_key(key(KeyCode::Esc));
    assert!(!a.menu.is_open());
    a.on_key(mkey(KeyCode::Char('t'), KeyModifiers::ALT));
    assert_eq!(a.menu.open, Some(4));
    a.on_key(key(KeyCode::Esc));
    // mouse: click "Help" in the menu bar, then "About Muninn"
    screen_text(&mut a);
    let (r, _) = *a.hits.iter().find(|(_, h)| *h == Hit::MenuTop(5)).unwrap();
    a.on_mouse(click(r.x + 1, r.y));
    assert_eq!(a.menu.open, Some(5));
    screen_text(&mut a);
    let (r, _) = *a.hits.iter().find(|(_, h)| *h == Hit::MenuItem(0, 3)).unwrap();
    a.on_mouse(click(r.x + 2, r.y));
    assert_eq!(a.last_action, Some(Action::About));
    assert!(screen_text(&mut a).contains("Local, encrypted store"));
}

#[test]
fn submenus_and_mnemonics() {
    let (_t, mut a) = app();
    a.on_key(mkey(KeyCode::Char('t'), KeyModifiers::ALT));
    a.on_key(key(KeyCode::Char('m'))); // Mode ▸
    assert_eq!(a.menu.path.len(), 2);
    let s = screen_text(&mut a);
    assert!(s.contains("System") && s.contains("Light"), "{s}");
    a.on_key(key(KeyCode::Char('l')));
    assert_eq!(a.last_action, Some(Action::ThemeMode(ThemeMode::Light)));
    assert_eq!(a.prefs.theme.mode, ThemeMode::Light);
    assert_eq!(a.variant, Variant::Light);
}

#[test]
fn menu_and_key_dispatch_the_same_action() {
    let (_t, mut a) = app();
    a.on_key(key(KeyCode::Char('?')));
    let by_key = a.last_action;
    a.modal = None;
    a.last_action = None;
    a.on_key(mkey(KeyCode::Char('h'), KeyModifiers::ALT));
    a.on_key(key(KeyCode::Enter)); // Help topics
    assert_eq!(a.last_action, by_key);
    assert!(matches!(a.modal, Some(Modal::Help(_))));
    // Every menu action has a unique name.
    let names: Vec<String> = menu::all_actions().iter().map(|a| a.name()).collect();
    let mut d = names.clone();
    d.sort();
    d.dedup();
    assert_eq!(d.len(), names.len());
}

#[test]
fn disabled_items_do_not_dispatch() {
    let (_t, mut a) = app();
    assert!(!a.enabled(Action::KeysInit));
    a.on_key(mkey(KeyCode::Char('k'), KeyModifiers::ALT));
    a.on_key(key(KeyCode::Enter)); // Initialize… (disabled)
    assert!(a.menu.is_open());
    assert_eq!(a.last_action, None);
}

#[test]
fn export_from_menu() {
    let (t, mut a) = app();
    a.dispatch(Action::Export);
    let out = t.path().join("x.csv");
    a.export.form.fields[0].value = out.display().to_string();
    a.export.form.fields[1].value = "ETH/USDT".into();
    a.export.form.sel = a.export.form.fields.len() + 1;
    a.on_key(key(KeyCode::Enter));
    assert!(a.status.contains("wrote 300"), "{}", a.status);
    assert_eq!(std::fs::read_to_string(out).unwrap().lines().count(), 301);
    a.on_key(key(KeyCode::Esc));
    assert!(a.modal.is_none());
}

#[test]
fn theme_editor_new_from_current() {
    let (_t, mut a) = app();
    a.dispatch(Action::ThemeNew);
    assert!(a.themes.editor.is_some());
    a.on_key(key(KeyCode::Enter));
    assert!(a.status.contains("saved theme tokyo-night-custom"), "{}", a.status);
    a.on_key(key(KeyCode::Esc));
    assert!(a.themes.editor.is_none());
    a.on_key(key(KeyCode::Esc));
    assert!(a.modal.is_none());
}

#[test]
fn help_is_context_sensitive() {
    let (_t, mut a) = app();
    a.dispatch(Action::BackupCreate);
    a.on_key(key(KeyCode::F(1)));
    match &a.modal {
        Some(Modal::Help(h)) => assert_eq!(h.current.id, "keys"),
        _ => panic!("help not open"),
    }
    a.on_key(key(KeyCode::Esc));
    a.dispatch(Action::Keybindings);
    match &a.modal {
        Some(Modal::Help(h)) => assert_eq!(h.current.id, "keybindings"),
        _ => panic!(),
    }
}

#[test]
fn keys_dialog_hides_secrets_until_revealed() {
    let (_t, mut a) = app();
    a.dispatch(Action::ReissueShards);
    a.keys.secret_lines = vec!["muninn-shard-v1:secret".into()];
    let s = screen_text(&mut a);
    assert!(!s.contains("muninn-shard-v1:secret"));
    a.keys.form.sel = a.keys.form.fields.len() + 3;
    a.on_key(key(KeyCode::Enter));
    assert!(screen_text(&mut a).contains("muninn-shard-v1:secret"));
    a.on_key(key(KeyCode::Esc));
    assert!(a.keys.secret_lines.is_empty());
}

#[test]
fn data_dialogs_and_prefs() {
    let (_t, mut a) = app();
    a.dispatch(Action::Coverage);
    assert!(screen_text(&mut a).contains("ETH/USDT"));
    a.on_key(key(KeyCode::Esc));
    a.dispatch(Action::Preferences);
    a.on_key(key(KeyCode::Enter));
    assert_eq!(a.prefs.tui.header, HeaderMode::Compact);
    let saved = Preferences::load_or_create(&a.paths).unwrap();
    assert_eq!(saved.tui.header, HeaderMode::Compact);
}

fn ctrl_w(a: &mut App, c: KeyCode) {
    a.on_key(mkey(KeyCode::Char('w'), KeyModifiers::CONTROL));
    a.on_key(key(c));
}

#[test]
fn fresh_session_is_one_list_tile_with_bars() {
    let (_t, mut a) = app();
    assert_eq!(a.layout.tiles().len(), 1);
    let s = screen_text(&mut a);
    assert!(s.contains("[1] list"), "{s}");
    assert!(s.contains("chg%") && s.contains("time▲"), "{s}");
    a.on_key(key(KeyCode::Char('.')));
    let s = screen_text(&mut a);
    assert!(s.contains("ETH/USDT · binance · 5m") || s.contains("BTC/USDT · binance · 5m"));
}

#[test]
fn ctrl_w_keys_and_menu_split_the_same_way() {
    let (_t, mut a) = app();
    screen_text(&mut a);
    ctrl_w(&mut a, KeyCode::Char('v'));
    assert_eq!(a.last_action, Some(Action::SplitRight));
    assert_eq!(a.layout.tiles().len(), 2);
    // same action from the menu: View ▸ Split ▸ Below
    a.on_key(mkey(KeyCode::Char('v'), KeyModifiers::ALT));
    a.on_key(key(KeyCode::Char('s')));
    a.on_key(key(KeyCode::Char('b')));
    assert_eq!(a.last_action, Some(Action::SplitBelow));
    assert_eq!(a.layout.tiles().len(), 3);
    ctrl_w(&mut a, KeyCode::Char('t'));
    let s = screen_text(&mut a);
    assert!(s.contains("chart ·"), "{s}");
    ctrl_w(&mut a, KeyCode::Char('h'));
    assert_eq!(a.layout.focus, 1);
    ctrl_w(&mut a, KeyCode::Char('z'));
    assert!(a.layout.zoomed);
    ctrl_w(&mut a, KeyCode::Char('z'));
    ctrl_w(&mut a, KeyCode::Char('q'));
    assert_eq!(a.layout.tiles().len(), 2);
    // persisted
    let saved = Preferences::load_or_create(&a.paths).unwrap();
    assert_eq!(saved.tui.layout.unwrap().tiles().len(), 2);
}

#[test]
fn list_sorts_by_keys_and_header_click() {
    let (_t, mut a) = app();
    screen_text(&mut a);
    a.on_key(key(KeyCode::Char('s')));
    assert_eq!(a.layout.focused().list.sort, crate::store::BarSort::Open);
    a.on_key(key(KeyCode::Char('r')));
    assert!(a.layout.focused().list.desc);
    screen_text(&mut a);
    let (r, _) = *a.hits.iter().find(|(_, h)| *h == Hit::ListHeader(1, 7)).unwrap();
    a.on_mouse(click(r.x + 1, r.y));
    assert_eq!(a.layout.focused().list.sort, crate::store::BarSort::ChangePct);
    assert!(!a.layout.focused().list.desc);
    let s = screen_text(&mut a);
    assert!(s.contains("chg%▲"), "{s}");
    // selected row moves and pages lazily
    a.on_key(key(KeyCode::End));
    let c = &a.caches[&1].list;
    assert_eq!(c.sel, c.total - 1);
    screen_text(&mut a);
    assert!(a.caches[&1].list.selected().is_some());
}

#[test]
fn presets_and_saved_layouts() {
    let (_t, mut a) = app();
    a.dispatch(Action::Preset(action::Preset::Grid2x2));
    assert_eq!(a.layout.tiles().len(), 4);
    let s = screen_text(&mut a);
    assert!(s.contains("[4] list"), "{s}");
    a.dispatch(Action::LayoutSave);
    for c in "quad".chars() {
        a.on_key(key(KeyCode::Char(c)));
    }
    a.on_key(key(KeyCode::Enter));
    a.dispatch(Action::Preset(action::Preset::TwoUp));
    assert_eq!(a.layout.tiles().len(), 2);
    a.dispatch(Action::LayoutsSaved);
    a.on_key(key(KeyCode::Enter));
    assert_eq!(a.layout.tiles().len(), 4);
    a.dispatch(Action::Container(action::ContainerMode::Tabbed));
    let s = screen_text(&mut a);
    assert!(s.contains(" list BTC/USDT ") || s.contains(" list ETH/USDT "), "{s}");
}

fn chart_tile(a: &mut App) {
    screen_text(a);
    ctrl_w(a, KeyCode::Char('t'));
    assert_eq!(a.layout.focused().view, layout::View::Chart);
}

#[test]
fn chart_renders_layers_and_selectors() {
    let (_t, mut a) = app();
    chart_tile(&mut a);
    let s = screen_text(&mut a);
    assert!(s.contains("start ◂") && s.contains("end ◂"), "{s}");
    assert!(s.contains('█') || s.contains('│'), "candles: {s}");
    assert!(s.contains("live"));
    assert!(s.contains("o ") && s.contains(" v "), "readout: {s}");
    // toggle every layer on and render
    for (k, _) in chart::LAYERS {
        if !a.layout.focused().chart.layers.get(k) {
            a.on_key(key(KeyCode::Char(k)));
        }
    }
    let s = screen_text(&mut a);
    assert!(s.contains('•'), "line/MA: {s}");
    assert!(s.contains('┄'), "envelope");
    assert!(s.contains('░'), "area");
    // layers persist with the layout
    let saved = Preferences::load_or_create(&a.paths).unwrap().tui.layout.unwrap();
    assert!(saved.focused().chart.layers.envelope);
    // turn candles off -> OHLC bars
    a.on_key(key(KeyCode::Char('c')));
    let s = screen_text(&mut a);
    assert!(s.contains('┤') || s.contains('├') || s.contains('┼'), "{s}");
    // layer menu
    a.on_key(key(KeyCode::Char('L')));
    assert!(screen_text(&mut a).contains("Chart layers"));
    a.on_key(key(KeyCode::Char('c')));
    assert!(a.layout.focused().chart.layers.candles);
}

#[test]
fn chart_zoom_pan_jump_and_period() {
    let (_t, mut a) = app();
    chart_tile(&mut a);
    a.on_key(key(KeyCode::Char('-')));
    screen_text(&mut a);
    assert_eq!(a.layout.focused().chart.zoom, 2);
    a.on_key(key(KeyCode::Char('h')));
    screen_text(&mut a);
    assert!(a.layout.focused().chart.end.is_some());
    a.on_key(key(KeyCode::End));
    assert!(a.layout.focused().chart.end.is_none());
    a.on_key(key(KeyCode::Char('@')));
    for c in "2017-08-17 06:00".chars() {
        a.on_key(key(KeyCode::Char(c)));
    }
    a.on_key(key(KeyCode::Enter));
    let s = screen_text(&mut a);
    assert!(s.contains("2017-08-17 06:00:00  o"), "{s}");
    a.on_key(key(KeyCode::Char('[')));
    for c in "2017-08-17".chars() {
        a.on_key(key(KeyCode::Char(c)));
    }
    a.on_key(key(KeyCode::Enter));
    screen_text(&mut a);
    let c = &a.caches[&a.layout.focus].chart;
    assert!(c.start <= crate::ingest::time::parse_ts("2017-08-17").unwrap(), "{}", c.start);
    // clicking the end ▸ arrow moves the window later
    let before = c.end;
    let (r, _) = *a.hits.iter().find(|(_, h)| matches!(h, Hit::ChartNav(_, 3))).unwrap();
    a.on_mouse(click(r.x, r.y));
    screen_text(&mut a);
    assert!(a.caches[&a.layout.focus].chart.end > before);
}

#[test]
fn linked_list_centers_chart_and_chart_scrolls_list() {
    let (_t, mut a) = app();
    a.dispatch(Action::Preset(action::Preset::TwoUp));
    screen_text(&mut a);
    a.dispatch(Action::LinkTile); // tile 1 (list) -> tile 2 (chart)
    assert_eq!(a.layout.tile(1).unwrap().link, Some(2));
    for _ in 0..10 {
        a.on_key(key(KeyCode::Down));
    }
    screen_text(&mut a);
    let sel_ts = a.caches[&1].list.selected().unwrap().ts;
    assert_eq!(a.caches[&2].chart.cursor, Some(sel_ts));
    assert!(screen_text(&mut a).contains(&crate::ingest::time::fmt_ts(sel_ts)));
    // move the chart crosshair: the list follows
    a.on_key(key(KeyCode::Tab));
    a.on_key(key(KeyCode::Left));
    a.on_key(key(KeyCode::Left));
    screen_text(&mut a);
    let cur = a.caches[&2].chart.cursor.unwrap();
    assert_eq!(a.caches[&1].list.selected().unwrap().ts, cur);
}

/// Manual sanity: `cargo test --lib snapshots -- --ignored` writes renders to
/// `target/tui-snapshots/`. Set MUNINN_RAW_DATA to use the full raw CSVs.
#[test]
#[ignore]
fn snapshots() {
    let (_t, mut a) = match std::env::var("MUNINN_RAW_DATA") {
        Ok(raw) => {
            let t = tempfile::tempdir().unwrap();
            let paths = Paths::resolve(Some(t.path().join("h")), None).unwrap();
            paths.ensure().unwrap();
            let dek = crate::crypto::Key32::random().unwrap();
            let mut store = Store::open(&paths.db, Some(&dek.0)).unwrap();
            for f in ["BTCUSDT_5m.csv", "ETHUSDT_5m.csv"] {
                let opts = crate::ingest::IngestOptions { source: Some("binance".into()), ..Default::default() };
                let started = std::time::Instant::now();
                crate::ingest::ingest_csv(&mut store, crate::interact::Interact::batch(true), opts, &std::path::Path::new(&raw).join(f), &Default::default()).unwrap();
                println!("ingested {f} in {:?}", started.elapsed());
            }
            let prefs = Preferences::load_or_create(&paths).unwrap();
            (t, App::new(paths, store, dek, prefs, Some(ThemeMode::Dark)).unwrap())
        }
        Err(_) => app(),
    };
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/tui-snapshots");
    std::fs::create_dir_all(&dir).unwrap();
    let shot = |a: &mut App, name: &str, w: u16, h: u16| {
        let started = std::time::Instant::now();
        let s = render(a, w, h);
        println!("{name}: rendered in {:?}", started.elapsed());
        std::fs::write(dir.join(format!("{name}.txt")), s).unwrap();
    };
    shot(&mut a, "01-fresh", 160, 48);
    shot(&mut a, "02-compact", 120, 26);
    a.on_key(mkey(KeyCode::Char('v'), KeyModifiers::ALT));
    a.on_key(key(KeyCode::Char('l')));
    shot(&mut a, "03-menu-view-layouts", 160, 48);
    a.menu.close();
    a.dispatch(Action::Preset(action::Preset::MainStack));
    shot(&mut a, "04-main-stack", 160, 48);
    for k in ['m', 'e', 'w', 'z'] {
        a.on_key(key(KeyCode::Char(k)));
    }
    a.on_key(key(KeyCode::Char('-')));
    a.on_key(key(KeyCode::Char('-')));
    shot(&mut a, "05-chart-layers", 160, 48);
    a.dispatch(Action::Preset(action::Preset::Grid2x2));
    a.on_key(key(KeyCode::Char('s')));
    a.on_key(key(KeyCode::Char('s')));
    a.on_key(key(KeyCode::Char('s')));
    a.on_key(key(KeyCode::Char('s')));
    a.on_key(key(KeyCode::Char('s')));
    a.on_key(key(KeyCode::Char('s')));
    a.on_key(key(KeyCode::Char('s')));
    a.on_key(key(KeyCode::Char('r')));
    shot(&mut a, "06-grid-sorted-chgpct", 200, 50);
    for _ in 0..12 {
        a.on_key(key(KeyCode::Char('-')));
    }
    a.on_key(key(KeyCode::Tab));
    for _ in 0..14 {
        a.on_key(key(KeyCode::Char('-')));
    }
    shot(&mut a, "07-zoomed-out", 200, 50);
}

#[test]
fn sidebar_toggle_search_sort_filter() {
    let (_t, mut a) = app();
    let s = screen_text(&mut a);
    assert!(s.contains("ETH/USDT") && s.contains("BTC/USDT"), "{s}");
    a.on_key(mkey(KeyCode::Char('b'), KeyModifiers::CONTROL));
    assert!(a.prefs.tui.sidebar_collapsed);
    a.on_key(key(KeyCode::Char('/')));
    assert!(!a.prefs.tui.sidebar_collapsed && a.sidebar.searching);
    for c in "eth".chars() {
        a.on_key(key(KeyCode::Char(c)));
    }
    assert_eq!(a.sidebar.rows.len(), 1);
    assert_eq!(a.series[a.sidebar.rows[0]].symbol.to_string(), "ETH/USDT");
    a.on_key(key(KeyCode::Enter));
    a.on_key(key(KeyCode::Enter));
    assert_eq!(a.layout.focused().series.as_ref().unwrap().symbol.to_string(), "ETH/USDT");
    // clear search, sort by name desc
    a.on_key(key(KeyCode::Char('/')));
    for _ in 0..3 {
        a.on_key(key(KeyCode::Backspace));
    }
    a.on_key(key(KeyCode::Esc));
    a.dispatch(Action::SortName);
    let first = |a: &App| a.series[a.sidebar.rows[0]].symbol.to_string();
    assert_eq!(first(&a), "BTC/USDT");
    a.on_key(key(KeyCode::Char('o')));
    assert_eq!(first(&a), "ETH/USDT");
    // source filter with no match, then chip shows and clear all
    a.prefs.tui.sidebar.filters.sources.insert("kraken".into());
    a.filters_changed();
    assert!(a.sidebar.rows.is_empty());
    assert!(screen_text(&mut a).contains("kraken"));
    a.on_key(key(KeyCode::Esc));
    a.on_key(key(KeyCode::Char('X')));
    assert_eq!(a.sidebar.rows.len(), 2);
    // filter editor via modal
    a.dispatch(Action::Filter(action::FilterKind::Source));
    assert!(screen_text(&mut a).contains("[ ] binance"));
    a.on_key(key(KeyCode::Char(' ')));
    assert!(a.prefs.tui.sidebar.filters.sources.contains("binance"));
    // price editor: max far below any price excludes all
    a.on_key(key(KeyCode::Esc));
    a.dispatch(Action::Filter(action::FilterKind::Price));
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Enter));
    a.on_key(key(KeyCode::Char('1')));
    a.on_key(key(KeyCode::Enter));
    assert_eq!(a.prefs.tui.sidebar.filters.price.max, Some(1.0));
    assert!(a.sidebar.rows.is_empty());
    assert_eq!(a.help_topic(), "sidebar");
}

#[test]
fn import_popup_browses_and_imports_in_background() {
    let (_t, mut a) = app();
    let d = tempfile::tempdir().unwrap();
    let fx = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    std::fs::copy(fx.join("ETHUSDT_5m.csv"), d.path().join("SOLUSDT_5m.csv")).unwrap();
    std::fs::copy(fx.join("BTCUSDT_5m.csv"), d.path().join("ADAUSDT_5m.csv")).unwrap();
    std::fs::create_dir(d.path().join("sub")).unwrap();
    a.on_key(key(KeyCode::Char('i')));
    assert!(matches!(a.modal, Some(Modal::Import)));
    // type a path with Tab completion
    a.on_key(key(KeyCode::Tab));
    a.import.path.clear();
    for c in format!("{}/su", d.path().display()).chars() {
        a.on_key(key(KeyCode::Char(c)));
    }
    a.on_key(key(KeyCode::Tab));
    assert!(a.import.path.ends_with("/sub/"), "{}", a.import.path);
    // back to the folder and select it whole
    a.import.path = format!("{}/", d.path().display());
    a.on_key(key(KeyCode::Enter));
    assert_eq!(a.import.entries.iter().filter(|e| !e.dir).count(), 2);
    a.on_key(key(KeyCode::Char('a')));
    assert_eq!(a.import.chosen.len(), 2);
    a.on_key(key(KeyCode::End));
    let s = screen_text(&mut a);
    assert!(s.contains("SOL/USDT") && s.contains("Preview") && s.contains("● SOLUSDT_5m.csv"), "{s}");
    a.on_key(key(KeyCode::Char('s')));
    for c in "kraken".chars() {
        a.on_key(key(KeyCode::Char(c)));
    }
    a.on_key(key(KeyCode::Enter));
    a.on_key(key(KeyCode::Char('i')));
    assert!(a.import.running());
    let t0 = std::time::Instant::now();
    while a.import.running() && t0.elapsed().as_secs() < 30 {
        screen_text(&mut a);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let s = screen_text(&mut a);
    assert!(s.contains("Import finished") && s.contains("2 file(s)"), "{s}");
    assert!(a.series.iter().any(|x| x.source == "kraken" && x.symbol.to_string() == "SOL/USDT"));
    assert_eq!(a.prefs.tui.import_recent[0], std::fs::canonicalize(d.path()).unwrap().display().to_string());
    a.on_key(key(KeyCode::Enter));
    assert!(a.modal.is_none());
}

#[test]
fn ingest_progress_counts_and_cancel_aborts() {
    let (_t, mut a) = app();
    let fx = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ETHUSDT_5m.csv");
    let p = std::sync::Arc::new(crate::ingest::Progress::default());
    let opts = crate::ingest::IngestOptions { source: Some("p".into()), progress: Some(p.clone()), ..Default::default() };
    crate::ingest::ingest_csv(&mut a.store, crate::interact::Interact::batch(true), opts.clone(), &fx, &Default::default()).unwrap();
    let (d, t) = p.get();
    assert!(d > 0 && d == t);
    p.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    let o2 = crate::ingest::IngestOptions { force: true, ..opts };
    assert!(crate::ingest::ingest_csv(&mut a.store, crate::interact::Interact::batch(true), o2, &fx, &Default::default()).is_err());
}

/// Full-scale timings on the real raw data (read-only). Needs MUNINN_RAW_DATA.
/// MUNINN_PERF_HOME reuses an already ingested temp home between runs.
/// Run: MUNINN_RAW_DATA=... cargo test --release --lib fullscale -- --ignored --nocapture
#[test]
#[ignore]
fn fullscale() {
    use std::time::{Duration, Instant};
    let Ok(raw) = std::env::var("MUNINN_RAW_DATA") else { return };
    let raw = std::path::PathBuf::from(raw);
    let keep = std::env::var("MUNINN_PERF_HOME").ok().map(std::path::PathBuf::from);
    let tmp = tempfile::tempdir().unwrap();
    let home = keep.clone().unwrap_or_else(|| tmp.path().to_path_buf());
    let paths = Paths::resolve(Some(home.join("h")), None).unwrap();
    paths.ensure().unwrap();
    let kf = home.join("perf.key");
    let dek = match std::fs::read(&kf) {
        Ok(b) if b.len() == 32 => crate::crypto::Key32(b.try_into().unwrap()),
        _ => {
            let k = crate::crypto::Key32::random().unwrap();
            std::fs::write(&kf, k.0).unwrap();
            k
        }
    };
    let mut store = Store::open(&paths.db, Some(&dek.0)).unwrap();
    if store.series().unwrap().len() < 2 {
        for f in ["BTCUSDT_5m.csv", "ETHUSDT_5m.csv"] {
            let opts = crate::ingest::IngestOptions { source: Some("binance".into()), ..Default::default() };
            let t = Instant::now();
            let r = crate::ingest::ingest_csv(&mut store, crate::interact::Interact::batch(true), opts, &raw.join(f), &Default::default()).unwrap();
            println!("ingest {f}: {} rows in {:?}", r[0].rows, t.elapsed());
        }
    }
    let prefs = Preferences::load_or_create(&paths).unwrap();
    let t0 = Instant::now();
    let mut a = App::new(paths, store, dek, prefs, Some(ThemeMode::Dark)).unwrap();
    println!("{:<40} {:>10.1?}", "App::new (series listing)", t0.elapsed());
    a.prefs.tui = Default::default();
    a.layout = layout::TileLayout::single(Some(layout::SeriesKey::of(&a.series[0])));
    a.caches.clear();
    a.refresh_rows();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/tui-snapshots/fullscale");
    std::fs::create_dir_all(&dir).unwrap();
    let mut worst: Vec<(String, Duration)> = vec![];
    let mut step = |a: &mut App, name: &str, f: &mut dyn FnMut(&mut App)| {
        let t = Instant::now();
        f(a);
        let s = render(a, 160, 48);
        let d = t.elapsed();
        println!("{name:<40} {d:>10.1?}");
        std::fs::write(dir.join(format!("{}.txt", name.replace([' ', '/'], "_"))), s).unwrap();
        worst.push((name.to_string(), d));
    };
    let t0 = Instant::now();
    render(&mut a, 160, 48);
    let mut slow = Duration::ZERO;
    while a.caches.values().any(|c| c.list.loading) && t0.elapsed().as_secs() < 120 {
        std::thread::sleep(Duration::from_millis(20));
        let f0 = Instant::now();
        render(&mut a, 160, 48);
        slow = slow.max(f0.elapsed());
    }
    println!("{:<40} {:>10.1?} (background; slowest frame meanwhile {slow:?})", "initial series load", t0.elapsed());
    step(&mut a, "list first render", &mut |_| {});
    for i in 0..super::super::store::BarSort::ALL.len() {
        let label = format!("list sort {}", a.layout.focused().list.sort.label());
        let _ = i;
        step(&mut a, &format!("{label} (asc)"), &mut |_| {});
        step(&mut a, &format!("{label} desc"), &mut |a| a.on_key(key(KeyCode::Char('r'))));
        step(&mut a, &format!("{label} PgDn"), &mut |a| a.on_key(key(KeyCode::PageDown)));
        step(&mut a, &format!("{label} End"), &mut |a| a.on_key(key(KeyCode::End)));
        a.on_key(key(KeyCode::Char('r')));
        a.on_key(key(KeyCode::Char('s')));
    }
    step(&mut a, "preset main-stack (list+charts)", &mut |a| a.dispatch(Action::Preset(action::Preset::MainStack)));
    let t0 = Instant::now();
    while a.data.loading() > 0 && t0.elapsed().as_secs() < 120 {
        std::thread::sleep(Duration::from_millis(20));
        render(&mut a, 160, 48);
    }
    println!("{:<40} {:>10.1?}", "second series load (background)", t0.elapsed());
    let chart = a.layout.tiles().iter().find(|t| t.view == layout::View::Chart).map(|t| t.id).unwrap();
    a.layout.set_focus(chart);
    for k in ['m', 'e', 'w', 'z', 'b', 'n'] {
        step(&mut a, &format!("chart layer {k}"), &mut |a| a.on_key(key(KeyCode::Char(k))));
    }
    for i in 0..16 {
        step(&mut a, &format!("chart zoom out {i}"), &mut |a| a.on_key(key(KeyCode::Char('-'))));
    }
    step(&mut a, "chart pan PgUp", &mut |a| a.on_key(key(KeyCode::PageUp)));
    step(&mut a, "chart Home", &mut |a| a.on_key(key(KeyCode::Home)));
    step(&mut a, "chart End", &mut |a| a.on_key(key(KeyCode::End)));
    step(&mut a, "chart zoom in", &mut |a| a.on_key(key(KeyCode::Char('+'))));
    step(&mut a, "sidebar search", &mut |a| {
        a.on_key(key(KeyCode::Char('/')));
        a.on_key(key(KeyCode::Char('e')));
        a.on_key(key(KeyCode::Esc));
    });
    step(&mut a, "sidebar sort name", &mut |a| a.dispatch(Action::SortName));
    step(&mut a, "sidebar toggle order", &mut |a| a.on_key(key(KeyCode::Char('o'))));
    step(&mut a, "sidebar year filter 2021", &mut |a| {
        a.prefs.tui.sidebar.filters.years.insert(2021);
        a.filters_changed();
    });
    step(&mut a, "sidebar price filter max 3000", &mut |a| {
        a.prefs.tui.sidebar.filters.price.max = Some(3000.0);
        a.filters_changed();
    });
    step(&mut a, "sidebar price metric cycle", &mut |a| {
        a.prefs.tui.sidebar.filters.price.metric = a.prefs.tui.sidebar.filters.price.metric.next();
        a.filters_changed();
    });
    step(&mut a, "sidebar clear filters", &mut |a| a.on_key(key(KeyCode::Char('X'))));
    step(&mut a, "refresh (F5)", &mut |a| a.on_key(key(KeyCode::F(5))));
    // import via the TUI path
    a.sidebar.focused = false;
    step(&mut a, "import open", &mut |a| a.on_key(key(KeyCode::Char('i'))));
    step(&mut a, "import cd raw/data", &mut |a| a.import.cd(raw.clone()));
    let pos = a.import.entries.iter().position(|e| e.name == "ETHUSD_5m.csv").unwrap();
    step(&mut a, "import preview highlight", &mut |a| a.import.sel = pos);
    a.import.fields[0] = format!("perf{}", std::process::id());
    let t = Instant::now();
    a.on_key(key(KeyCode::Char('i')));
    let mut samples = vec![];
    let mut slow_frame = Duration::ZERO;
    while a.import.running() && t.elapsed().as_secs() < 300 {
        let f0 = Instant::now();
        render(&mut a, 160, 48);
        slow_frame = slow_frame.max(f0.elapsed());
        if let import::Phase::Running(r) = &a.import.phase {
            samples.push((t.elapsed().as_secs_f64(), r.progress.estimate().overall));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let s = render(&mut a, 160, 48);
    std::fs::write(dir.join("import_done.txt"), &s).unwrap();
    let distinct: std::collections::BTreeSet<u64> = samples.iter().map(|x| (x.1 * 1e6) as u64).collect();
    let monotonic = samples.windows(2).all(|w| w[1].1 >= w[0].1);
    let max_jump = samples.windows(2).map(|w| w[1].1 - w[0].1).fold(0.0, f64::max);
    let first_move = samples.iter().find(|x| x.1 > 0.0).map(|x| x.0).unwrap_or(f64::NAN);
    let pct = |sec: f64| samples.iter().take_while(|x| x.0 <= sec).last().map(|x| x.1 * 100.0).unwrap_or(0.0);
    println!(
        "import ETHUSD_5m.csv: {:?}, {} distinct progress values, monotonic {monotonic}, max jump {:.1}%, first move at {first_move:.2}s, at 1s {:.1}% 10s {:.1}% 20s {:.1}% 30s {:.1}%, slowest frame {slow_frame:?}",
        t.elapsed(), distinct.len(), max_jump * 100.0, pct(1.0), pct(10.0), pct(20.0), pct(30.0)
    );
    assert!(monotonic);
    assert!(s.contains("Import finished"), "{s}");
    worst.sort_by_key(|x| std::cmp::Reverse(x.1));
    println!("--- slowest");
    for (n, d) in worst.iter().take(8) {
        println!("{n:<40} {d:>10.1?}");
    }
}

/// Full-file ingest timing into a fresh temp home (raw data read-only).
#[test]
#[ignore]
fn ingest_timing() {
    let Ok(raw) = std::env::var("MUNINN_RAW_DATA") else { return };
    let t = tempfile::tempdir().unwrap();
    let paths = Paths::resolve(Some(t.path().join("h")), None).unwrap();
    paths.ensure().unwrap();
    let dek = crate::crypto::Key32::random().unwrap();
    let mut store = Store::open(&paths.db, Some(&dek.0)).unwrap();
    for f in ["BTCUSDT_5m.csv", "ETHUSDT_5m.csv"] {
        let progress = std::env::var_os("MUNINN_TIMING_PROGRESS").map(|_| std::sync::Arc::new(crate::ingest::Progress::default()));
        let opts = crate::ingest::IngestOptions { source: Some("binance".into()), progress, ..Default::default() };
        let s = std::time::Instant::now();
        crate::ingest::ingest_csv(&mut store, crate::interact::Interact::batch(true), opts, &std::path::Path::new(&raw).join(f), &Default::default()).unwrap();
        println!("TIMING {f}: {:?}", s.elapsed());
    }
}
