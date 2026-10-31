//! TUI tests on ratatui's TestBackend (no real terminal).
use super::action::Action;
use super::modal::Modal;
use super::*;
use crate::prefs::{HeaderMode, ThemeMode};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyEventState, KeyModifiers};

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
    a.save_prints_only = true;
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
    for m in ["File", "View", "Data", "Projects", "Net", "Orgs", "Keys", "Theme", "Help"] {
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
    assert_eq!(a.menu.open, Some(7));
    a.on_key(key(KeyCode::Esc));
    // mouse: click "Help" in the menu bar, then "About Muninn"
    screen_text(&mut a);
    let (r, _) = *a.hits.iter().find(|(_, h)| *h == Hit::MenuTop(8)).unwrap();
    a.on_mouse(click(r.x + 1, r.y));
    assert_eq!(a.menu.open, Some(8));
    screen_text(&mut a);
    let (r, _) = *a.hits.iter().find(|(_, h)| *h == Hit::MenuItem(0, 3)).unwrap();
    a.on_mouse(click(r.x + 2, r.y));
    assert_eq!(a.last_action, Some(Action::About));
    assert!(screen_text(&mut a).contains("Local, encrypted store"));
}

#[test]
fn every_category_fits_at_80_columns_and_dropdowns_stay_below_the_bar() {
    let (_t, mut a) = app();
    for w in [80u16, 100, 160] {
        let s = render(&mut a, w, 24);
        let bar = s.lines().find(|l| l.contains("File") && l.contains("Help")).unwrap_or_else(|| panic!("no menu bar at {w}: {s}"));
        let mut at = 0;
        for m in menu::menus() {
            let i = bar[at..].find(m.title).map(|i| i + at).unwrap_or_else(|| panic!("{} missing at {w}: {bar}", m.title));
            at = i + m.title.len();
        }
        assert_eq!(bar.contains("Alt+letter"), w >= 100, "{w}: {bar}");
    }
    // each dropdown opens right below its own category, also on a short screen
    for (i, m) in menu::menus().iter().enumerate() {
        a.menu.open_top(i);
        for h in [12u16, 24] {
            let s = render(&mut a, 80, h);
            let (top, _) = *a.hits.iter().find(|(_, x)| *x == Hit::MenuTop(i)).unwrap();
            let first =
                a.hits.iter().find(|(_, x)| *x == Hit::MenuItem(0, 0)).map(|(r, _)| *r).unwrap_or_else(|| panic!("{}: no items {s}", m.title));
            assert!(first.y > top.y, "{} covers the bar at height {h}", m.title);
            assert!(first.x <= top.x + 1 || first.x + first.width >= 79, "{} not under its category", m.title);
            assert!(s.lines().nth(top.y as usize).unwrap().contains(m.title), "{} bar hidden: {s}", m.title);
        }
        a.menu.close();
    }
    // every window and feature is reachable from a menu
    let all = menu::all_actions();
    for x in [
        Action::NetExchanges,
        Action::NetConnections,
        Action::NetInvites,
        Action::NetInvite,
        Action::NetEntrust,
        Action::NetShards,
        Action::NetSettings,
        Action::Orgs,
        Action::OrgSite,
        Action::Reviews,
        Action::Code,
        Action::Models,
        Action::Rules,
        Action::Authors,
        Action::Preferences,
        Action::KeysExport,
    ] {
        assert!(all.contains(&x), "{x:?}");
    }
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
    a.on_key(key(KeyCode::Char('{')));
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
            for f in ["binance_BTCUSDT_5m.csv", "binance_ETHUSDT_5m.csv"] {
                let opts = crate::ingest::IngestOptions { source: Some("binance".into()), ..Default::default() };
                let started = std::time::Instant::now();
                crate::ingest::ingest_csv(
                    &mut store,
                    crate::interact::Interact::batch(true),
                    opts,
                    &std::path::Path::new(&raw).join(f),
                    &Default::default(),
                )
                .unwrap();
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
        for f in ["binance_BTCUSDT_5m.csv", "binance_ETHUSDT_5m.csv"] {
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
    let pos = a.import.entries.iter().position(|e| e.name == "bitstamp_ETHUSD_5m.csv").unwrap();
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
        "import bitstamp_ETHUSD_5m.csv: {:?}, {} distinct progress values, monotonic {monotonic}, max jump {:.1}%, first move at {first_move:.2}s, at 1s {:.1}% 10s {:.1}% 20s {:.1}% 30s {:.1}%, slowest frame {slow_frame:?}",
        t.elapsed(),
        distinct.len(),
        max_jump * 100.0,
        pct(1.0),
        pct(10.0),
        pct(20.0),
        pct(30.0)
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
    for f in ["binance_BTCUSDT_5m.csv", "binance_ETHUSDT_5m.csv"] {
        let progress = std::env::var_os("MUNINN_TIMING_PROGRESS").map(|_| std::sync::Arc::new(crate::ingest::Progress::default()));
        let opts = crate::ingest::IngestOptions { source: Some("binance".into()), progress, ..Default::default() };
        let s = std::time::Instant::now();
        crate::ingest::ingest_csv(&mut store, crate::interact::Interact::batch(true), opts, &std::path::Path::new(&raw).join(f), &Default::default())
            .unwrap();
        println!("TIMING {f}: {:?}", s.elapsed());
    }
}

/// A synthetic 5m CSV with `n` valid rows.
fn big_csv(path: &std::path::Path, n: usize) {
    use std::io::Write;
    let mut w = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    writeln!(w, "timestamp,open,high,low,close,volume").unwrap();
    let t0 = 1_577_836_800_000i64;
    for i in 0..n as i64 {
        writeln!(w, "{},10,12,9,11,1", t0 + i * 300_000).unwrap();
    }
}

#[test]
fn cancel_mid_file_rolls_back_the_whole_file() {
    let (_t, mut a) = app();
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join("SOLUSDT_5m.csv");
    big_csv(&f, 300_000);
    let before = a.store.count_bars().unwrap();
    let series_before = a.store.series().unwrap();
    let p = std::sync::Arc::new(crate::ingest::Progress::default());
    let watcher = {
        let p = p.clone();
        std::thread::spawn(move || {
            while p.file_bars_done.load(std::sync::atomic::Ordering::Relaxed) < 20_000 {
                std::thread::yield_now();
            }
            p.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        })
    };
    let opts = crate::ingest::IngestOptions { source: Some("c".into()), progress: Some(p.clone()), ..Default::default() };
    let r = crate::ingest::ingest_csv(&mut a.store, crate::interact::Interact::batch(true), opts, &f, &Default::default());
    watcher.join().unwrap();
    assert!(matches!(r, Err(crate::Error::Aborted)), "{r:?}");
    assert!(p.file_bars_done.load(std::sync::atomic::Ordering::Relaxed) >= 20_000);
    assert_eq!(a.store.count_bars().unwrap(), before, "partial file must be rolled back");
    assert_eq!(a.store.series().unwrap(), series_before, "series_stats unchanged");
    // not recorded as ingested: the same file imports fully afterwards
    let opts = crate::ingest::IngestOptions { source: Some("c".into()), ..Default::default() };
    let r = crate::ingest::ingest_csv(&mut a.store, crate::interact::Interact::batch(true), opts, &f, &Default::default()).unwrap();
    assert!(!r[0].skipped && r[0].inserted == 300_000);
    assert_eq!(a.store.count_bars().unwrap(), before + 300_000);
}

#[test]
fn tui_cancel_keeps_finished_files_and_discards_current() {
    let (_t, mut a) = app();
    let d = tempfile::tempdir().unwrap();
    let fx = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    std::fs::copy(fx.join("ETHUSDT_5m.csv"), d.path().join("ADAUSDT_5m.csv")).unwrap();
    big_csv(&d.path().join("SOLUSDT_5m.csv"), 400_000);
    a.on_key(key(KeyCode::Char('i')));
    a.import.cd(d.path().to_path_buf());
    a.on_key(key(KeyCode::Char('a')));
    a.import.fields[0] = "cx".into();
    a.on_key(key(KeyCode::Char('i')));
    assert!(a.import.running());
    let t0 = std::time::Instant::now();
    // wait until the big file is being stored
    loop {
        screen_text(&mut a);
        if let import::Phase::Running(r) = &a.import.phase
            && r.progress.file_index.load(std::sync::atomic::Ordering::Relaxed) == 1
            && r.progress.file_bars_done.load(std::sync::atomic::Ordering::Relaxed) > 0
        {
            break;
        }
        assert!(t0.elapsed().as_secs() < 60);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(screen_text(&mut a).contains("[Cancel]"));
    a.on_key(key(KeyCode::Esc)); // asks first
    let s = screen_text(&mut a);
    assert!(s.contains("Cancel the import?") && s.contains("[Yes, cancel]"), "{s}");
    a.on_key(key(KeyCode::Char('n')));
    assert!(screen_text(&mut a).contains("[Cancel]"));
    a.on_key(key(KeyCode::Char('c')));
    a.on_key(key(KeyCode::Char('y')));
    assert!(screen_text(&mut a).contains("Cancelling"));
    while a.import.running() && t0.elapsed().as_secs() < 60 {
        screen_text(&mut a);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let s = screen_text(&mut a);
    assert!(s.contains("Import cancelled") && s.contains("kept 1 complete file(s), discarded 1"), "{s}");
    let src: Vec<String> = a.series.iter().filter(|x| x.source == "cx").map(|x| x.symbol.to_string()).collect();
    assert_eq!(src, ["ADA/USDT"], "only the finished file stays");
    a.on_key(key(KeyCode::Esc));
    assert!(a.modal.is_none());
}

#[test]
fn every_popup_has_a_clickable_close_button() {
    let (_t, mut a) = app();
    let opens: Vec<Action> = vec![
        Action::Help,
        Action::Export,
        Action::Import,
        Action::BackupCreate,
        Action::BackupRestore,
        Action::ThemeSelect,
        Action::ThemeEdit,
        Action::ThemeImport,
        Action::Preferences,
        Action::RotateKey,
        Action::KeysExport,
        Action::ReissueShards,
        Action::Sources,
        Action::Symbols,
        Action::Coverage,
        Action::Keybindings,
        Action::LayoutsSaved,
        Action::LayoutSave,
        Action::ChartLayers,
        Action::Filter(action::FilterKind::Source),
    ];
    for act in opens {
        a.dispatch(act);
        if a.modal.is_none() {
            continue; // not a popup in this state
        }
        let s = screen_text(&mut a);
        assert!(s.contains("[X]"), "{act:?}: {s}");
        let (r, _) = *a.modal_hits.iter().find(|(_, h)| *h == Hit::CloseModal).unwrap();
        a.on_mouse(click(r.x + 2, r.y));
        assert!(a.modal.is_none(), "{act:?} not closed by [X]");
    }
}

#[test]
fn x_during_import_asks_to_cancel() {
    let (_t, mut a) = app();
    let d = tempfile::tempdir().unwrap();
    big_csv(&d.path().join("SOLUSDT_5m.csv"), 300_000);
    a.on_key(key(KeyCode::Char('i')));
    a.import.cd(d.path().to_path_buf());
    a.import.fields[0] = "xx".into();
    a.on_key(key(KeyCode::Char('a')));
    a.on_key(key(KeyCode::Char('i')));
    screen_text(&mut a);
    let (r, _) = *a.modal_hits.iter().find(|(_, h)| *h == Hit::CloseModal).unwrap();
    a.on_mouse(click(r.x + 2, r.y));
    assert!(matches!(a.modal, Some(Modal::Import)));
    screen_text(&mut a);
    let (r, _) = *a.modal_hits.iter().find(|(_, h)| *h == Hit::ImportConfirm(true)).expect("confirm buttons");
    a.on_mouse(click(r.x + 1, r.y));
    let t0 = std::time::Instant::now();
    while a.import.running() && t0.elapsed().as_secs() < 60 {
        screen_text(&mut a);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(a.series.iter().all(|x| x.source != "xx"));
}

#[test]
fn places_are_visible_clickable_and_bookmarkable() {
    let (_t, mut a) = app();
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir(d.path().join("sub")).unwrap();
    a.on_key(key(KeyCode::Char('i')));
    let s = screen_text(&mut a);
    assert!(s.contains("Places (p)") && s.contains("Home") && s.contains("p places") && s.contains("b bookmarks this"), "{s}");
    a.import.cd(d.path().join("sub"));
    a.on_key(key(KeyCode::Char('b')));
    let sub = std::fs::canonicalize(d.path().join("sub")).unwrap().display().to_string();
    assert_eq!(a.prefs.tui.import_bookmarks, std::slice::from_ref(&sub));
    a.import.cd(d.path().to_path_buf());
    let s = screen_text(&mut a);
    assert!(s.contains("Bookmarks"), "{s}");
    let i = import::places(&a).iter().position(|p| p.kind == import::PlaceKind::Bookmark).unwrap();
    let (r, _) = *a.modal_hits.iter().find(|(_, h)| *h == Hit::ImportPlace(i)).unwrap();
    a.on_mouse(click(r.x + 1, r.y));
    assert_eq!(a.import.dir.display().to_string(), sub);
    // keyboard: p focuses places, x removes the bookmark
    a.import.pane = import::Pane::Files;
    a.on_key(key(KeyCode::Char('p')));
    assert_eq!(a.import.pane, import::Pane::Places);
    a.import.place_sel = i;
    a.on_key(key(KeyCode::Char('x')));
    assert!(a.prefs.tui.import_bookmarks.is_empty());
    a.on_key(key(KeyCode::Char('x'))); // on a built-in or recent: never removes built-ins
    assert!(import::places(&a).iter().any(|p| p.label == "Home") || dirs::home_dir().is_none());
}

/// Cancel half-way through the real BTCUSDT_5m.csv: nothing stays, cancel is quick.
#[test]
#[ignore]
fn cancel_fullsize() {
    use std::sync::atomic::Ordering::Relaxed;
    let Ok(raw) = std::env::var("MUNINN_RAW_DATA") else { return };
    let (_t, mut a) = app();
    let before = a.store.count_bars().unwrap();
    let p = std::sync::Arc::new(crate::ingest::Progress::default());
    let w = {
        let p = p.clone();
        std::thread::spawn(move || {
            while p.file_bars_done.load(Relaxed) < 480_000 {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            p.cancel.store(true, Relaxed);
            std::time::Instant::now()
        })
    };
    let opts = crate::ingest::IngestOptions { source: Some("big".into()), progress: Some(p.clone()), ..Default::default() };
    let r = crate::ingest::ingest_csv(
        &mut a.store,
        crate::interact::Interact::batch(true),
        opts,
        &std::path::Path::new(&raw).join("binance_BTCUSDT_5m.csv"),
        &Default::default(),
    );
    let asked = w.join().unwrap();
    println!("CANCEL stopped {:?} after the request, at {} bars stored", asked.elapsed(), p.file_bars_done.load(Relaxed));
    assert!(matches!(r, Err(crate::Error::Aborted)));
    assert_eq!(a.store.count_bars().unwrap(), before);
    assert!(a.store.series().unwrap().iter().all(|s| s.source != "big"));
}

#[test]
fn cancel_with_new_source_leaves_no_artifacts_and_is_recorded() {
    use std::sync::atomic::Ordering::Relaxed;
    let (_t, mut a) = app();
    let d = tempfile::tempdir().unwrap();
    big_csv(&d.path().join("SOLUSDT_5m.csv"), 300_000);
    let recent_before = a.prefs.tui.import_recent.clone();
    a.on_key(key(KeyCode::Char('i')));
    a.import.cd(d.path().to_path_buf());
    a.import.fields[0] = "fresh".into();
    a.on_key(key(KeyCode::Char('a')));
    a.on_key(key(KeyCode::Char('i')));
    let t0 = std::time::Instant::now();
    loop {
        screen_text(&mut a);
        if let import::Phase::Running(r) = &a.import.phase
            && r.progress.file_bars_done.load(Relaxed) > 0
        {
            break;
        }
        assert!(t0.elapsed().as_secs() < 60);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    a.on_key(key(KeyCode::Char('c')));
    a.on_key(key(KeyCode::Char('y')));
    while a.import.running() && t0.elapsed().as_secs() < 60 {
        screen_text(&mut a);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(a.store.source_by_name("fresh").unwrap().is_none(), "no empty source left");
    assert!(a.series.iter().all(|s| s.source != "fresh"));
    assert_eq!(a.prefs.tui.import_recent, recent_before, "recent entry undone");
    let run = a.store.last_import_run().unwrap().unwrap();
    assert_eq!(
        (run.origin.as_str(), run.status.as_str(), run.cancel_by.as_deref(), run.cancel_phase.as_deref()),
        ("tui", "cancelled", Some("tui"), Some("store"))
    );
    assert!(run.cancel_requested_at.is_some() && run.cancel_file.as_deref().unwrap().ends_with("SOLUSDT_5m.csv"));
    let arts = a.store.import_artifacts(run.id).unwrap();
    assert!(arts.iter().any(|x| x.kind == "source" && x.state == "rolled_back"), "{arts:?}");
    assert!(arts.iter().any(|x| x.kind == "series" && x.state == "rolled_back"), "{arts:?}");
    assert!(arts.iter().any(|x| x.kind == "recent" && x.state == "removed"), "{arts:?}");
    assert!(arts.iter().all(|x| x.state != "kept"));
    // re-import works and is recorded as completed with kept artifacts
    let opts = crate::ingest::IngestOptions { source: Some("fresh".into()), ..Default::default() };
    let f = d.path().join("SOLUSDT_5m.csv");
    let r = crate::ingest::tracked(&mut a.store, "cli", 1, opts, |st, o| {
        crate::ingest::ingest_csv(st, crate::interact::Interact::batch(true), o, &f, &Default::default())
    })
    .unwrap();
    assert_eq!(r[0].inserted, 300_000);
    let run = a.store.last_import_run().unwrap().unwrap();
    assert_eq!((run.status.as_str(), run.files_done), ("completed", 1));
    let kinds: Vec<String> = a.store.import_artifacts(run.id).unwrap().into_iter().filter(|x| x.state == "kept").map(|x| x.kind).collect();
    assert_eq!(kinds, ["source", "series", "bars", "ingest_record"]);
}

#[test]
fn cli_style_cancel_is_recorded_by_cli() {
    let (_t, mut a) = app();
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join("SOLUSDT_5m.csv");
    big_csv(&f, 100_000);
    let p = std::sync::Arc::new(crate::ingest::Progress::default());
    p.request_cancel("cli");
    let opts = crate::ingest::IngestOptions { source: Some("clicx".into()), progress: Some(p), ..Default::default() };
    let r = crate::ingest::tracked(&mut a.store, "cli", 1, opts, |st, o| {
        crate::ingest::ingest_csv(st, crate::interact::Interact::batch(true), o, &f, &Default::default())
    });
    assert!(r.is_err());
    let run = a.store.last_import_run().unwrap().unwrap();
    assert_eq!((run.status.as_str(), run.cancel_by.as_deref(), run.cancel_phase.as_deref()), ("cancelled", Some("cli"), Some("read")));
    assert!(a.store.source_by_name("clicx").unwrap().is_none());
}

#[test]
fn path_tab_list_cycles_and_click_picks() {
    let (_t, mut a) = app();
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir(d.path().join("alpha")).unwrap();
    std::fs::create_dir(d.path().join("apple")).unwrap();
    a.on_key(key(KeyCode::Char('i')));
    a.on_key(key(KeyCode::Tab)); // into the path input
    a.import.path = format!("{}/a", d.path().display());
    a.on_key(key(KeyCode::Tab)); // "a" -> "ap"? no: alpha/apple share only "a" -> list
    let s = screen_text(&mut a);
    assert!(s.contains("alpha/") && s.contains("apple/") && s.contains("Shift-Tab cycle"), "{s}");
    a.on_key(key(KeyCode::Tab));
    assert!(a.import.path.ends_with("/alpha/"));
    a.on_key(mkey(KeyCode::BackTab, KeyModifiers::SHIFT));
    assert!(a.import.path.ends_with("/apple/"));
    screen_text(&mut a);
    let (r, _) = *a.modal_hits.iter().find(|(_, h)| *h == Hit::ImportCandidate(0)).unwrap();
    a.on_mouse(click(r.x, r.y));
    assert!(a.import.path.ends_with("/alpha/"));
    a.on_key(key(KeyCode::Esc)); // revert to typed
    assert!(a.import.path.ends_with("/a"));
    a.on_key(key(KeyCode::Tab));
    a.on_key(key(KeyCode::Tab));
    a.on_key(key(KeyCode::Enter)); // accept selection
    a.on_key(key(KeyCode::Enter)); // go there
    assert!(a.import.dir.ends_with("alpha"));
}

fn wait_import(a: &mut App) -> String {
    let t0 = std::time::Instant::now();
    while a.import.running() && t0.elapsed().as_secs() < 60 {
        screen_text(a);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    screen_text(a)
}

#[test]
fn import_folder_recursively_from_tui() {
    let (_t, mut a) = app();
    let d = tempfile::tempdir().unwrap();
    let fx = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    std::fs::create_dir_all(d.path().join("sub/deeper")).unwrap();
    std::fs::copy(fx.join("ETHUSDT_5m.csv"), d.path().join("sub/SOLUSDT_5m.csv")).unwrap();
    std::fs::copy(fx.join("ETHUSDT_5m.csv"), d.path().join("sub/deeper/ADAUSDT_5m.csv")).unwrap();
    std::fs::copy(fx.join("ETHUSDT_5m.csv"), d.path().join("DOTUSDT_5m.csv")).unwrap();
    a.on_key(key(KeyCode::Char('i')));
    a.import.cd(d.path().to_path_buf());
    // highlight the folder "sub" and import it: both files below it, not the top one
    a.import.sel = a.import.entries.iter().position(|e| e.name == "sub").unwrap();
    a.on_key(key(KeyCode::Char('i')));
    let s = wait_import(&mut a);
    assert!(s.contains("Import finished") && s.contains("2 file(s)"), "{s}");
    assert!(a.series.iter().any(|x| x.symbol.to_string() == "ADA/USDT"));
    assert!(!a.series.iter().any(|x| x.symbol.to_string() == "DOT/USDT"));
    a.on_key(key(KeyCode::Enter));
    // A imports the whole current folder tree
    a.on_key(key(KeyCode::Char('i')));
    a.import.cd(d.path().to_path_buf());
    a.on_key(key(KeyCode::Char('A')));
    let s = wait_import(&mut a);
    assert!(s.contains("3 file(s)"), "{s}");
    assert!(a.series.iter().any(|x| x.symbol.to_string() == "DOT/USDT"));
    // Space selects a folder too
    a.on_key(key(KeyCode::Enter));
    a.on_key(key(KeyCode::Char('i')));
    a.import.cd(d.path().to_path_buf());
    a.import.sel = a.import.entries.iter().position(|e| e.name == "sub").unwrap();
    a.on_key(key(KeyCode::Char(' ')));
    assert!(a.import.chosen.contains(&a.import.dir.join("sub")));
    let empty = tempfile::tempdir().unwrap();
    crate::tui::import::start(&mut a, vec![empty.path().to_path_buf()]);
    assert!(a.import.message.as_deref().unwrap().contains("no data files"));
}

/// Click the first recorded hit matching `p` (popup hits first, then the screen).
fn click_on(a: &mut App, p: impl Fn(&Hit) -> bool) {
    screen_text(a);
    let r = a.modal_hits.iter().chain(a.hits.iter()).rev().find(|(_, h)| p(h)).map(|(r, _)| *r).expect("hit not drawn");
    a.on_mouse(click(r.x, r.y));
}

#[test]
fn import_picker_entries_inputs_and_keys_are_clickable() {
    let (_t, mut a) = app();
    let d = tempfile::tempdir().unwrap();
    let fx = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    std::fs::create_dir(d.path().join("sub")).unwrap();
    std::fs::copy(fx.join("ETHUSDT_5m.csv"), d.path().join("sub/SOLUSDT_5m.csv")).unwrap();
    a.on_key(key(KeyCode::Char('i')));
    a.import.cd(d.path().to_path_buf());
    let sub = a.import.entries.iter().position(|e| e.name == "sub").unwrap();
    // first click highlights, second opens the folder
    click_on(&mut a, |h| *h == Hit::ImportEntry(sub));
    assert_eq!(a.import.sel, sub);
    click_on(&mut a, |h| *h == Hit::ImportEntry(sub));
    assert!(a.import.dir.ends_with("sub"));
    // inputs focus on click
    click_on(&mut a, |h| *h == Hit::ImportInput(3));
    assert_eq!(a.import.pane, crate::tui::import::Pane::Field(1));
    click_on(&mut a, |h| *h == Hit::ImportInput(0));
    assert_eq!(a.import.pane, crate::tui::import::Pane::Path);
    // back to the list with a click, then the key buttons
    click_on(&mut a, |h| matches!(h, Hit::ImportEntry(_)));
    assert_eq!(a.import.pane, crate::tui::import::Pane::Files);
    click_on(&mut a, |h| *h == Hit::Press(KeyCode::Char('A')));
    let t0 = std::time::Instant::now();
    while a.import.running() && t0.elapsed().as_secs() < 30 {
        screen_text(&mut a);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(a.series.iter().any(|x| x.symbol.to_string() == "SOL/USDT"));
    click_on(&mut a, |h| *h == Hit::Press(KeyCode::Enter)); // [Close]
    assert!(a.modal.is_none());
}

#[test]
fn popup_rows_forms_and_list_rows_are_clickable() {
    let (_t, mut a) = app();
    // Preferences: click toggles the row
    a.dispatch(crate::tui::action::Action::Preferences);
    let before = a.prefs.ingest.drop_zero_volume;
    click_on(&mut a, |h| *h == Hit::Row(2));
    assert_ne!(a.prefs.ingest.drop_zero_volume, before);
    a.close_modal();
    // Export form: clicking a field starts editing it
    a.dispatch(crate::tui::action::Action::Export);
    click_on(&mut a, |h| *h == Hit::FormRow(1));
    assert!(a.export.form.editing && a.export.form.sel == 1);
    a.close_modal();
    // Themes: click selects, a second click applies
    a.themes = crate::tui::themes::ThemeState::new(&a.paths, &a.prefs.theme.name);
    a.modal = Some(Modal::Themes);
    let i = a.themes.names.iter().position(|n| *n != a.prefs.theme.name).unwrap();
    click_on(&mut a, |h| *h == Hit::Row(i));
    assert_eq!(a.themes.list.selected(), Some(i));
    click_on(&mut a, |h| *h == Hit::Row(i));
    assert_eq!(a.prefs.theme.name, a.themes.names[i]);
    click_on(&mut a, |h| *h == Hit::Press(KeyCode::Esc));
    assert!(a.modal.is_none());
    // Prompt: [Cancel] closes
    a.dispatch(crate::tui::action::Action::LayoutSave);
    click_on(&mut a, |h| *h == Hit::Press(KeyCode::Esc));
    assert!(a.modal.is_none());
    // list tile: clicking a bar row selects it
    screen_text(&mut a);
    let row = a.hits.iter().filter_map(|(_, h)| if let Hit::ListRow(1, r) = h { Some(*r) } else { None }).nth(3).unwrap();
    click_on(&mut a, |h| *h == Hit::ListRow(1, row));
    assert_eq!(a.caches[&1].list.sel, row);
}

#[test]
fn import_asks_for_a_symbol_it_cannot_guess() {
    let (_t, mut a) = app();
    let d = tempfile::tempdir().unwrap();
    let fx = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    std::fs::copy(fx.join("unnamed.csv"), d.path().join("mystery_5m.csv")).unwrap();
    std::fs::copy(fx.join("unnamed.csv"), d.path().join("other_5m.csv")).unwrap();
    std::fs::copy(fx.join("ETHUSDT_5m.csv"), d.path().join("SOLUSDT_5m.csv")).unwrap();
    a.on_key(key(KeyCode::Char('i')));
    a.import.cd(d.path().to_path_buf());
    a.on_key(key(KeyCode::Char('A')));
    assert!(!a.import.running());
    let s = screen_text(&mut a);
    assert!(s.contains("Symbol needed") && s.contains("mystery_5m.csv") && s.contains("1 more file"), "{s}");
    assert_eq!(a.import.ask.as_ref().unwrap().ask.len(), 2); // SOLUSDT is guessed
    // a bad answer stays in the popup with an error
    for c in "zzz".chars() {
        a.on_key(key(KeyCode::Char(c)));
    }
    a.on_key(key(KeyCode::Enter));
    assert!(screen_text(&mut a).contains("is not a symbol"));
    // [X] cancels without importing
    click_on(&mut a, |h| *h == Hit::ImportAsk(3));
    assert!(a.import.ask.is_none() && !a.import.running());
    // again: answer the first, skip the second (clicked)
    a.on_key(key(KeyCode::Char('A')));
    for c in "xmr/usdt".chars() {
        a.on_key(key(KeyCode::Char(c)));
    }
    a.on_key(key(KeyCode::Enter));
    assert!(screen_text(&mut a).contains("other_5m.csv"));
    click_on(&mut a, |h| *h == Hit::ImportAsk(2));
    let s = wait_import(&mut a);
    assert!(s.contains("2 file(s)"), "{s}");
    assert!(a.series.iter().any(|x| x.symbol.to_string() == "XMR/USDT"));
    assert!(a.series.iter().any(|x| x.symbol.to_string() == "SOL/USDT"));
    // the symbol field skips the question
    a.on_key(key(KeyCode::Enter));
    a.on_key(key(KeyCode::Char('i')));
    a.import.cd(d.path().to_path_buf());
    a.import.fields[1] = "LTC/USDT".into();
    a.import.sel = a.import.entries.iter().position(|e| e.name == "other_5m.csv").unwrap();
    a.on_key(key(KeyCode::Char('i')));
    assert!(a.import.ask.is_none() && a.import.running());
    wait_import(&mut a);
    assert!(a.series.iter().any(|x| x.symbol.to_string() == "LTC/USDT"));
}

#[test]
fn finished_import_loads_into_an_empty_view_only() {
    let (_t, mut a) = app();
    let d = tempfile::tempdir().unwrap();
    let fx = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    std::fs::copy(fx.join("ETHUSDT_5m.csv"), d.path().join("SOLUSDT_5m.csv")).unwrap();
    std::fs::copy(fx.join("ETHUSDT_5m.csv"), d.path().join("ADAUSDT_5m.csv")).unwrap();
    let ids: Vec<u32> = a.layout.tiles().iter().map(|t| t.id).collect();
    for id in ids {
        a.layout.set_focus(id);
        a.layout.focused_mut().series = None;
    }
    // nothing loaded: the import shows up
    a.on_key(key(KeyCode::Char('i')));
    a.import.cd(d.path().to_path_buf());
    a.import.sel = a.import.entries.iter().position(|e| e.name == "SOLUSDT_5m.csv").unwrap();
    a.on_key(key(KeyCode::Char('i')));
    wait_import(&mut a);
    assert_eq!(a.layout.focused().series.as_ref().map(|k| k.symbol.as_str()), Some("SOL/USDT"));
    assert!(a.status.contains("loaded the import"), "{}", a.status);
    let s = screen_text(&mut a);
    assert!(!s.contains("pick a series"), "{s}");
    // something loaded: a later import leaves the view alone
    a.on_key(key(KeyCode::Enter));
    a.on_key(key(KeyCode::Char('i')));
    a.import.cd(d.path().to_path_buf());
    a.import.sel = a.import.entries.iter().position(|e| e.name == "ADAUSDT_5m.csv").unwrap();
    a.on_key(key(KeyCode::Char('i')));
    wait_import(&mut a);
    assert_eq!(a.layout.focused().series.as_ref().map(|k| k.symbol.as_str()), Some("SOL/USDT"));
}

fn help(a: &mut App) -> &mut crate::help::HelpView {
    match a.modal.as_mut() {
        Some(Modal::Help(h)) => h,
        _ => panic!("help is not open"),
    }
}

#[test]
fn help_links_footer_and_search_are_clickable() {
    let (_t, mut a) = app();
    a.open_help("README");
    let s = screen_text(&mut a);
    for k in ["[Tab next link]", "[Enter follow]", "[Bksp back]", "[/ search]", "[Esc close]"] {
        assert!(s.contains(k), "footer misses {k}: {s}");
    }
    // click the second link
    let target = help(&mut a).internal_links()[1].1.clone();
    click_on(&mut a, |h| *h == Hit::HelpLink(1));
    assert_eq!(help(&mut a).current.id, target);
    // the footer's Back button returns
    click_on(&mut a, |h| *h == Hit::Press(KeyCode::Backspace));
    assert_eq!(help(&mut a).current.id, "README");
    // keyboard: Tab to the second link, Enter follows it
    a.on_key(key(KeyCode::Tab));
    a.on_key(key(KeyCode::Enter));
    assert_eq!(help(&mut a).current.id, target);
    // `/` starts a search from a topic other than the index; click a result
    a.on_key(key(KeyCode::Char('/')));
    assert!(help(&mut a).searching);
    for c in "redis".chars() {
        a.on_key(key(KeyCode::Char(c)));
    }
    assert!(screen_text(&mut a).contains("[Esc stop search]"));
    click_on(&mut a, |h| *h == Hit::Row(0));
    assert_eq!(help(&mut a).current.id, "services/redis");
    // the search box is clickable too, and [X] still closes
    click_on(&mut a, |h| *h == Hit::Press(KeyCode::Char('/')));
    assert!(help(&mut a).searching);
    click_on(&mut a, |h| *h == Hit::CloseModal);
    assert!(a.modal.is_none());
}

#[test]
fn help_link_hits_follow_wrapping_and_scroll() {
    let (_t, mut a) = app();
    a.open_help("README");
    let n = help(&mut a).internal_links().len();
    // walking to the last link scrolls it into view, where it can be clicked
    for _ in 1..n {
        a.on_key(key(KeyCode::Tab));
    }
    let target = help(&mut a).internal_links()[n - 1].1.clone();
    click_on(&mut a, |h| *h == Hit::HelpLink(n - 1));
    assert_eq!(help(&mut a).current.id, target);
}

#[test]
fn tiles_show_a_spinner_then_the_bars_while_loading_in_the_background() {
    let (_t, mut a) = app();
    a.data.sync = false;
    a.data.clear();
    a.caches.clear();
    let s = screen_text(&mut a);
    assert!(s.contains("loading series…"), "{s}");
    assert!(crate::tui::widgets::SPINNER.iter().any(|c| s.contains(*c)) || s.contains('▕'), "no spinner or bar: {s}");
    // frames keep coming while the worker loads (the UI does not block)
    let t0 = std::time::Instant::now();
    let mut frames = 0;
    while a.data.loading() > 0 && t0.elapsed().as_secs() < 30 {
        screen_text(&mut a);
        frames += 1;
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(frames > 0);
    let s = screen_text(&mut a);
    assert!(!s.contains("loading series…") && s.contains("bars"), "{s}");
}

#[test]
fn loading_widget_draws_a_bar_when_the_total_is_known() {
    use crate::tui::data::Pending;
    let t = a_theme();
    let draw = |p: Pending| {
        let mut term = Terminal::new(TestBackend::new(80, 3)).unwrap();
        term.draw(|f| crate::tui::widgets::loading(f, f.area(), &p, &t)).unwrap();
        let buf = term.backend().buffer().clone();
        (0..buf.area.height).map(|y| (0..buf.area.width).map(|x| buf[(x, y)].symbol().to_string()).collect::<String>() + "\n").collect::<String>()
    };
    let s = draw(Pending::Loading { done: 25_000, total: Some(100_000) });
    assert!(s.contains("25%") && s.contains("25,000 / 100,000 bars") && s.contains('█') && s.contains('░'), "{s}");
    let s = draw(Pending::Loading { done: 0, total: None });
    assert!(crate::tui::widgets::SPINNER.iter().any(|c| s.contains(*c)), "{s}");
    assert!(draw(Pending::Failed("boom".into())).contains("could not load the series: boom"));
    assert_eq!(crate::tui::widgets::progress_text(5, 10, 10), "▕█████░░░░░▏  50%  5 / 10 bars");
}

fn a_theme() -> crate::theme::Theme {
    crate::theme::builtin("tokyo-night").unwrap().resolved(crate::theme::Variant::Dark)
}

#[test]
fn symbol_popup_keeps_the_file_name_visible_with_long_paths() {
    let (_t, mut a) = app();
    let d = tempfile::tempdir().unwrap();
    let deep = d.path().join("a_very_long_directory_name_for_market_data/exchange_exports/2026/october/raw");
    std::fs::create_dir_all(&deep).unwrap();
    let fx = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    for n in ["mystery_5m.csv", "second_mystery_file_with_a_long_name_5m.csv", "third_5m.csv"] {
        std::fs::copy(fx.join("unnamed.csv"), deep.join(n)).unwrap();
    }
    a.on_key(key(KeyCode::Char('i')));
    a.import.cd(d.path().to_path_buf());
    a.on_key(key(KeyCode::Char('A')));
    let first = a.import.ask.as_ref().unwrap().ask[0].file_name().unwrap().to_string_lossy().into_owned();
    for w in [60u16, 80, 140] {
        let s = render(&mut a, w, 30);
        assert!(s.contains(&first), "file name cut at width {w}: {s}");
        assert!(s.contains("file 1 of 3"), "{s}");
        assert!(s.contains("…") || w == 140, "long dir not shortened at width {w}: {s}");
        assert!(s.contains("/raw"), "end of the directory missing at width {w}: {s}");
        assert!(a.modal_hits.iter().any(|(_, h)| *h == Hit::ImportAsk(3)), "[X] missing");
    }
    // the long name wraps instead of being cut, and the counter advances
    a.on_key(key(KeyCode::Tab));
    a.on_key(key(KeyCode::Tab));
    let s = render(&mut a, 60, 30);
    assert!(s.contains("file 3 of 3") || s.contains("file 2 of 3"), "{s}");
    let name = a.import.ask.as_ref().unwrap().ask[0].file_name().unwrap().to_string_lossy().into_owned();
    let flat: String = s.split('\n').map(|l| l.replace('│', "")).collect::<Vec<_>>().join("").replace(' ', "");
    assert!(flat.contains(&name.replace(' ', "")), "{name} not shown whole: {s}");
}

#[test]
fn ellipsize_left_keeps_the_end() {
    use super::import::ellipsize_left;
    assert_eq!(ellipsize_left("/a/b/c", 10), "/a/b/c");
    assert_eq!(ellipsize_left("/home/me/data/raw", 8), "…ata/raw");
    assert_eq!(ellipsize_left("abc", 0), "");
}

fn picker(a: &mut App) -> &mut crate::tui::picker::DatePicker {
    match a.modal.as_mut() {
        Some(Modal::DateTime(p)) => p,
        _ => panic!("picker not open"),
    }
}

#[test]
fn chart_period_picker_sets_date_and_time_by_keys_and_mouse() {
    use crate::ingest::time::parse_ts;
    let (_t, mut a) = app();
    chart_tile(&mut a);
    screen_text(&mut a);
    // keyboard: type a date and a time
    a.on_key(key(KeyCode::Char('[')));
    let s = screen_text(&mut a);
    assert!(s.contains("Period start (UTC)") && s.contains("▲") && s.contains("[Enter OK]"), "{s}");
    for c in "2017-08-17 06:30".chars() {
        a.on_key(key(KeyCode::Char(c)));
    }
    assert_eq!(picker(&mut a).text(), "2017-08-17 06:30");
    a.on_key(key(KeyCode::Enter));
    screen_text(&mut a);
    let c = &a.caches[&a.layout.focus].chart;
    assert!(c.start <= parse_ts("2017-08-17 06:30").unwrap(), "{}", c.start);
    // arrows: hour up, minute down
    a.on_key(key(KeyCode::Char(']')));
    let p = picker(&mut a).clone();
    a.on_key(key(KeyCode::Right));
    a.on_key(key(KeyCode::Right));
    a.on_key(key(KeyCode::Right));
    a.on_key(key(KeyCode::Up));
    assert_eq!(picker(&mut a).millis(), p.millis() + 3_600_000);
    a.on_key(key(KeyCode::Esc));
    assert!(a.modal.is_none());
    // mouse: click the end date, then the minute ▼, then OK
    click_on(&mut a, |h| matches!(h, Hit::ChartDate(_, 1)));
    let p0 = picker(&mut a).millis();
    click_on(&mut a, |h| *h == Hit::Picker(crate::tui::picker::PickHit::Down(4)));
    click_on(&mut a, |h| *h == Hit::Picker(crate::tui::picker::PickHit::Down(4)));
    assert_eq!(picker(&mut a).millis(), p0 - 120_000);
    assert_eq!(picker(&mut a).field, 4);
    click_on(&mut a, |h| *h == Hit::Picker(crate::tui::picker::PickHit::Ok));
    assert!(a.modal.is_none());
    assert_eq!(a.layout.focused().chart.end, Some(p0 - 120_000));
    // [X] closes
    a.on_key(key(KeyCode::Char('[')));
    click_on(&mut a, |h| *h == Hit::CloseModal);
    assert!(a.modal.is_none());
}

#[test]
fn picker_steps_keep_dates_valid() {
    use crate::tui::picker::{DatePicker, Target};
    let ms = crate::ingest::time::parse_ts("2024-01-31 23:59").unwrap();
    let mut p = DatePicker::new(Target::ChartStart, 1, ms);
    p.step(1, 1);
    assert_eq!(p.text(), "2024-02-29 23:59");
    p.step(4, 1);
    assert_eq!(p.text(), "2024-03-01 00:00");
    p.step(0, -1);
    assert_eq!(p.text(), "2023-03-01 00:00");
    p.field = 1;
    p.typed = "13".into();
    p.commit();
    assert_eq!(p.text(), "2023-03-01 00:00", "month 13 is ignored");
}

#[test]
fn list_range_filters_the_listed_bars() {
    use crate::ingest::time::parse_ts;
    let (_t, mut a) = app();
    let s = screen_text(&mut a);
    assert!(s.contains("from start to end"), "{s}");
    let id = a.layout.focus;
    let all = a.caches[&id].list.total;
    let first = a.caches[&id].list.selected().unwrap().ts;
    // keyboard: from = first bar + 1h
    a.on_key(key(KeyCode::Char('[')));
    let from = first + 3_600_000;
    *picker(&mut a) = crate::tui::picker::DatePicker::new(crate::tui::picker::Target::ListFrom, id, from);
    a.on_key(key(KeyCode::Enter));
    screen_text(&mut a);
    let n1 = a.caches[&id].list.total;
    assert!(n1 < all && n1 > 0, "{n1} of {all}");
    assert!(a.caches[&id].list.selected().unwrap().ts >= from);
    assert_eq!(a.layout.focused().list.from, Some(from));
    // mouse: click "to …" in the title, type a time 2h after the start, OK
    click_on(&mut a, |h| *h == Hit::ListRange(id, 1));
    let to_text = crate::ingest::time::fmt_ts(from + 2 * 3_600_000)[..16].to_string();
    for c in to_text.chars() {
        a.on_key(key(KeyCode::Char(c)));
    }
    click_on(&mut a, |h| *h == Hit::Picker(crate::tui::picker::PickHit::Ok));
    let s = screen_text(&mut a);
    let n2 = a.caches[&id].list.total;
    assert!(n2 < n1 && n2 > 0, "{n2} of {n1}");
    assert!(s.contains(&format!("to {to_text}")), "{s}");
    let to = a.layout.focused().list.to.unwrap();
    assert!(to >= parse_ts(&to_text).unwrap());
    // the last listed bar is inside the range
    a.on_key(key(KeyCode::End));
    assert!(a.caches[&id].list.selected().unwrap().ts <= to);
    // backslash clears the range
    a.on_key(key(KeyCode::Char('\\')));
    screen_text(&mut a);
    assert_eq!(a.caches[&id].list.total, all);
}

#[test]
fn tile_focus_next_previous_and_direct_jump() {
    let (_t, mut a) = app();
    a.dispatch(Action::Preset(action::Preset::TwoUp));
    a.dispatch(Action::SplitBelow);
    screen_text(&mut a);
    let ids: Vec<u32> = a.layout.tiles().iter().map(|t| t.id).collect();
    assert!(ids.len() >= 3, "{ids:?}");
    a.layout.set_focus(ids[0]);
    a.on_key(key(KeyCode::Tab));
    assert_eq!(a.layout.focus, ids[1]);
    a.on_key(mkey(KeyCode::BackTab, KeyModifiers::SHIFT));
    assert_eq!(a.layout.focus, ids[0]);
    a.on_key(mkey(KeyCode::BackTab, KeyModifiers::SHIFT));
    assert_eq!(a.layout.focus, *ids.last().unwrap(), "wraps around");
    // Alt+n jumps to tile [n]
    let target = ids[1];
    a.on_key(mkey(KeyCode::Char(char::from_digit(target, 10).unwrap()), KeyModifiers::ALT));
    assert_eq!(a.layout.focus, target);
    a.on_key(mkey(KeyCode::Char('9'), KeyModifiers::ALT));
    assert_eq!(a.layout.focus, target);
    assert!(a.status.contains("no tile [9]"), "{}", a.status);
    // Ctrl-w p / W: previous
    a.on_key(mkey(KeyCode::Char('w'), KeyModifiers::CONTROL));
    a.on_key(key(KeyCode::Char('p')));
    assert_eq!(a.layout.focus, ids[0]);
    // Alt+letter still opens menus, Alt+digit inside a popup does nothing
    a.on_key(key(KeyCode::Char('?')));
    a.on_key(mkey(KeyCode::Char(char::from_digit(target, 10).unwrap()), KeyModifiers::ALT));
    assert_eq!(a.layout.focus, ids[0]);
}

#[test]
fn import_source_defaults_to_the_exchange_in_the_name_then_local() {
    let (_t, mut a) = app();
    let d = tempfile::tempdir().unwrap();
    let fx = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    std::fs::copy(fx.join("ETHUSDT_5m.csv"), d.path().join("kraken_SOLUSDT_5m.csv")).unwrap();
    std::fs::copy(fx.join("ETHUSDT_5m.csv"), d.path().join("ADAUSDT_5m.csv")).unwrap();
    a.on_key(key(KeyCode::Char('i')));
    a.import.cd(d.path().to_path_buf());
    a.import.sel = a.import.entries.iter().position(|e| e.name == "kraken_SOLUSDT_5m.csv").unwrap();
    let s = screen_text(&mut a);
    assert!(s.contains("source kraken (from name)"), "{s}");
    assert!(s.contains("from file name, else local"), "{s}");
    a.on_key(key(KeyCode::Char('A')));
    wait_import(&mut a);
    let src = |a: &App, sym: &str| a.series.iter().find(|x| x.symbol.to_string() == sym).map(|x| x.source.clone());
    assert_eq!(src(&a, "SOL/USDT").as_deref(), Some("kraken"));
    assert_eq!(src(&a, "ADA/USDT").as_deref(), Some("local"));
    // a typed source wins (s focuses the field)
    a.on_key(key(KeyCode::Enter));
    a.on_key(key(KeyCode::Char('i')));
    a.import.cd(d.path().to_path_buf());
    a.import.fields[0] = "coinbase".into();
    a.import.sel = a.import.entries.iter().position(|e| e.name == "kraken_SOLUSDT_5m.csv").unwrap();
    assert!(screen_text(&mut a).contains("source coinbase"));
    a.on_key(key(KeyCode::Char('i')));
    wait_import(&mut a);
    assert!(a.series.iter().any(|x| x.source == "coinbase" && x.symbol.to_string() == "SOL/USDT"));
}

#[test]
fn symbols_are_guessed_searched_and_filtered_with_exchange_prefixed_names() {
    // names with the exchange in front (raw/data since v1.15.0)
    for (n, want) in [
        ("coinbase_BTC-USD_5m.csv", "BTC/USD"),
        ("binance_BTCUSDT_5m.csv", "BTC/USDT"),
        ("bitstamp_BTCUSD_5m_part1.csv", "BTC/USD"),
        ("Kraken-ETHUSDT_1h.csv", "ETH/USDT"),
    ] {
        assert_eq!(crate::ingest::symbol_from_name_static(n).map(|s| s.display()).as_deref(), Some(want), "{n}");
    }
    let (_t, mut a) = app();
    let d = tempfile::tempdir().unwrap();
    let fx = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    std::fs::copy(fx.join("BTCUSDT_5m.csv"), d.path().join("coinbase_BTC-USD_5m.csv")).unwrap();
    a.on_key(key(KeyCode::Char('i')));
    a.import.cd(d.path().to_path_buf());
    a.on_key(key(KeyCode::Char('A')));
    assert!(a.import.ask.is_none(), "symbol guessed, no popup");
    wait_import(&mut a);
    a.on_key(key(KeyCode::Enter));
    assert!(a.series.iter().any(|x| x.source == "coinbase" && x.symbol.display() == "BTC/USD"));
    // search ignores separators and word order
    let n = |a: &App, q: &str| crate::tui::sidebar::rows(&a.series, q, &a.prefs.tui.sidebar, &|_| None).len();
    assert_eq!(n(&a, "btc-usdt"), 1);
    assert_eq!(n(&a, "BTC_USDT"), 1);
    assert_eq!(n(&a, "binance btc"), 1);
    assert_eq!(n(&a, "btc binance"), 1);
    // symbol filter: key m in the sidebar, Space toggles
    a.on_key(key(KeyCode::Char('/')));
    a.on_key(key(KeyCode::Esc));
    a.sidebar.focused = true;
    a.sidebar.searching = false;
    a.on_key(key(KeyCode::Char('m')));
    let s = screen_text(&mut a);
    assert!(s.contains("[ ] BTC/USD") && s.contains("[ ] ETH/USDT"), "{s}");
    a.on_key(key(KeyCode::Char(' ')));
    assert!(a.prefs.tui.sidebar.filters.symbols.contains("BTC/USD"));
    assert_eq!(a.sidebar.rows.len(), 1);
    assert_eq!(a.series[a.sidebar.rows[0]].source, "coinbase");
    a.on_key(key(KeyCode::Esc));
    assert!(screen_text(&mut a).contains("sym:BTC/USD"));
    a.dispatch(Action::ClearFilters);
    assert_eq!(a.sidebar.rows.len(), 3);
}

#[test]
fn time_range_filters_every_view_from_key_menu_and_sidebar() {
    let (_t, mut a) = app();
    let bars = |a: &mut App| {
        let s = screen_text(a);
        s.lines().next().map(|_| s.clone()).unwrap()
    };
    let all = a.layout.focused().series.clone().unwrap();
    let d = a.data.get(&all, &a.store).unwrap();
    let (first, n) = (d.bars[0].ts, d.bars.len());
    // T opens the picker on any view (here a list), From then To
    a.on_key(key(KeyCode::Char('T')));
    assert!(matches!(a.modal, Some(Modal::DateTime(ref p)) if p.target == picker::Target::RangeFrom));
    assert!(screen_text(&mut a).contains("Time range from"));
    for _ in 0..2 {
        a.on_key(key(KeyCode::Right));
    }
    a.on_key(key(KeyCode::Right)); // hour
    a.on_key(key(KeyCode::Up));
    a.on_key(key(KeyCode::Enter));
    assert!(matches!(a.modal, Some(Modal::DateTime(ref p)) if p.target == picker::Target::RangeTo));
    a.on_key(key(KeyCode::Enter));
    let fl = a.prefs.tui.sidebar.filters.clone();
    assert_eq!(fl.from, Some(first + 3_600_000));
    let s = bars(&mut a);
    assert!(s.contains("from ") && !s.contains("2017-08-17 04:00:00"), "{s}");
    // the chart view is scoped the same way
    a.dispatch(Action::ChangeView);
    let s = screen_text(&mut a);
    assert!(!s.contains("04:00"), "{s}");
    a.dispatch(Action::ChangeView);
    // the list tile lists fewer bars
    let cnt = a.caches.get(&a.layout.focus).map(|c| c.list.total as usize).unwrap_or(0);
    assert!(cnt > 0 && cnt < n, "{cnt} of {n}");
    // a range outside a series hides it from the sidebar
    a.range_set(Some(i64::MAX - 10), true);
    assert!(a.sidebar.rows.is_empty());
    // the menu item and the sidebar row open the same picker; x clears
    a.modal = None;
    a.dispatch(Action::Filter(action::FilterKind::Time));
    a.on_key(key(KeyCode::Char('x')));
    a.on_key(key(KeyCode::Char('x')));
    assert!(a.modal.is_none());
    assert_eq!((a.prefs.tui.sidebar.filters.from, a.prefs.tui.sidebar.filters.to), (None, None));
    assert_eq!(a.sidebar.rows.len(), 2);
    click_on(&mut a, |h| *h == Hit::SidebarFilter(action::FilterKind::Time));
    assert!(matches!(a.modal, Some(Modal::DateTime(_))));
    assert_eq!(a.help_topic(), "sidebar");
    let s = screen_text(&mut a);
    assert!(s.contains("[X]"), "{s}");
}

fn type_text(a: &mut App, s: &str) {
    for c in s.chars() {
        a.on_key(key(KeyCode::Char(c)));
    }
}

#[test]
fn view_changes_are_recorded_and_stepping_restores_them() {
    let (_t, mut a) = app();
    assert!(a.store.actions().unwrap().is_empty(), "loading the fixtures records nothing");
    a.dispatch(Action::ClearFilters); // nothing to clear: no action
    assert!(a.store.actions().unwrap().is_empty());
    a.prefs.tui.sidebar.filters.sources.insert("binance".into());
    a.filters_changed();
    a.set_theme_mode(ThemeMode::Light);
    let acts = a.store.actions().unwrap();
    assert_eq!(acts.iter().map(|x| x.kind.as_str()).collect::<Vec<_>>(), ["filter", "theme.mode"]);
    assert!(acts.iter().all(|x| x.origin == "tui"));
    // u steps back: the mode, then the filter.
    a.on_key(key(KeyCode::Char('u')));
    assert_eq!(a.prefs.theme.mode, ThemeMode::System);
    assert!(a.prefs.tui.sidebar.filters.sources.contains("binance"));
    a.on_key(key(KeyCode::Char('u')));
    assert!(a.prefs.tui.sidebar.filters.is_empty());
    assert_eq!(a.store.history_head().unwrap(), None);
    a.on_key(key(KeyCode::Char('u')));
    assert!(a.status.contains("start"), "{}", a.status);
    // U steps forward again; replaying records nothing.
    a.on_key(key(KeyCode::Char('U')));
    assert!(a.prefs.tui.sidebar.filters.sources.contains("binance"));
    assert_eq!(a.store.actions().unwrap().len(), 2);
    // A different change now forks from #1; the old branch is kept.
    a.prefs.tui.sidebar.filters.years.insert(2017);
    a.filters_changed();
    assert_eq!(a.store.actions().unwrap().len(), 3);
    assert_eq!(a.store.history_forks().unwrap(), 1);
    // The preferences file has the restored state too.
    let p = crate::prefs::Preferences::load_or_create(&a.paths).unwrap();
    assert!(p.tui.sidebar.filters.years.contains(&2017));
}

#[test]
fn history_bar_shows_the_tree_and_goes_to_actions() {
    let (_t, mut a) = app();
    a.prefs.tui.sidebar.filters.sources.insert("binance".into());
    a.filters_changed();
    a.set_theme_mode(ThemeMode::Light);
    a.dispatch(Action::HistoryBack);
    a.dispatch(Action::ThemeMode(ThemeMode::Dark)); // fork
    assert!(!a.prefs.tui.history_open);
    assert!(!screen_text(&mut a).contains("History ·"));
    a.on_key(key(KeyCode::Char('Y')));
    assert!(a.prefs.tui.history_open && a.history.focused);
    assert_eq!(a.help_topic(), "history");
    let s = screen_text(&mut a);
    assert!(s.contains("History · 3"), "{s}");
    assert!(s.contains("start") && s.contains("#1 filters: src:binance"), "{s}");
    // Rows: start, #1, #2, then the fork #3 (indented under #1), which is the head.
    assert!(s.contains("○ #2 theme mode light") && s.contains("  ●⑂ #3 theme mode dark"), "{s}");
    assert_eq!(a.history.sel, 3);
    a.on_key(key(KeyCode::Char('k')));
    a.on_key(key(KeyCode::Enter));
    assert_eq!(a.store.history_head().unwrap(), Some(2));
    assert_eq!(a.prefs.theme.mode, ThemeMode::Light);
    // Click: first selects, second goes there.
    click_on(&mut a, |h| *h == Hit::HistoryRow(0));
    assert_eq!(a.history.sel, 0);
    assert_eq!(a.store.history_head().unwrap(), Some(2));
    click_on(&mut a, |h| *h == Hit::HistoryRow(0));
    assert_eq!(a.store.history_head().unwrap(), None);
    assert!(a.prefs.tui.sidebar.filters.is_empty());
    // Esc leaves the bar, Y focuses it again, the « button collapses it.
    a.on_key(key(KeyCode::Esc));
    assert!(!a.history.focused);
    a.on_key(key(KeyCode::Char('Y')));
    assert!(a.history.focused && a.prefs.tui.history_open);
    click_on(&mut a, |h| *h == Hit::HistoryCollapse);
    assert!(!a.prefs.tui.history_open);
    // Actions recorded by another connection (a background import) show up on the next draw.
    a.on_key(key(KeyCode::Char('Y')));
    screen_text(&mut a);
    let other = Store::open(&a.paths.db, Some(&a.dek.0)).unwrap();
    other.record_action(&crate::store::NewAction::new("import", "tui", "import x.csv")).unwrap();
    assert!(screen_text(&mut a).contains("#4 import x.csv"));
}

#[test]
fn authors_popup_adds_edits_deletes_and_sets_current() {
    let (_t, mut a) = app();
    a.on_key(key(KeyCode::Char('A')));
    assert!(matches!(a.modal, Some(Modal::Authors)));
    assert_eq!(a.help_topic(), "authors");
    assert!(screen_text(&mut a).contains("no authors yet"));
    a.on_key(key(KeyCode::Char('a')));
    assert!(a.typing());
    type_text(&mut a, "Ada"); // `q`, `u` and friends are typed, not run
    a.on_key(key(KeyCode::Tab));
    type_text(&mut a, "ada@example.org");
    a.on_key(key(KeyCode::Tab));
    type_text(&mut a, "org=Analytical; role=poet");
    a.on_key(key(KeyCode::Enter));
    let cur = a.store.current_author().unwrap().expect("first author becomes current");
    assert_eq!((cur.name.as_str(), cur.email.as_deref(), cur.details.len()), ("Ada", Some("ada@example.org"), 2));
    let s = screen_text(&mut a);
    assert!(s.contains("★ Ada <ada@example.org>"), "{s}");
    assert!(s.contains("author: Ada"), "header shows the current author: {s}");
    // An empty name is an error shown in the popup.
    a.on_key(key(KeyCode::Char('a')));
    a.on_key(key(KeyCode::Enter));
    assert!(a.authors.error.is_some() && a.authors.form.is_some());
    a.on_key(key(KeyCode::Esc));
    // Add a second one, make it current by clicking its row twice.
    a.on_key(key(KeyCode::Char('a')));
    type_text(&mut a, "Grace");
    a.on_key(key(KeyCode::Enter));
    let gi = a.authors.list.iter().position(|x| x.name == "Grace").unwrap();
    a.authors.sel = 0;
    click_on(&mut a, |h| *h == Hit::Row(gi));
    click_on(&mut a, |h| *h == Hit::Row(gi));
    assert_eq!(a.store.current_author().unwrap().unwrap().name, "Grace");
    // Edit: rename Grace.
    a.on_key(key(KeyCode::Char('e')));
    type_text(&mut a, " Hopper");
    a.on_key(key(KeyCode::Enter));
    assert_eq!(a.store.current_author().unwrap().unwrap().name, "Grace Hopper");
    // Delete needs y.
    a.on_key(key(KeyCode::Char('d')));
    a.on_key(key(KeyCode::Char('n')));
    assert_eq!(a.store.authors().unwrap().len(), 2);
    a.on_key(key(KeyCode::Char('d')));
    a.on_key(key(KeyCode::Char('y')));
    assert_eq!(a.store.authors().unwrap().len(), 1);
    assert!(a.store.current_author().unwrap().is_none());
    // Every change is in the history, attributed to whoever was current.
    let kinds: Vec<String> = a.store.actions().unwrap().into_iter().map(|x| x.kind).collect();
    assert_eq!(kinds, ["author.add", "author.add", "author.use", "author.edit", "author.remove"]);
    a.on_key(key(KeyCode::Esc));
    assert!(a.modal.is_none());
}

#[test]
fn menus_have_authors_and_history() {
    let all = crate::tui::menu::all_actions();
    for x in [Action::Authors, Action::ToggleHistory, Action::HistoryBack, Action::HistoryForward] {
        assert!(all.contains(&x), "{x:?}");
    }
    let (_t, mut a) = app();
    a.on_key(mkey(KeyCode::Char('v'), KeyModifiers::ALT));
    let s = screen_text(&mut a);
    assert!(s.contains("History"), "{s}");
}

/// Where `needle` is drawn (column of its first character, row).
fn find_on(s: &str, needle: &str) -> Option<(u16, u16)> {
    s.lines().enumerate().find_map(|(y, l)| l.find(needle).map(|b| (l[..b].chars().count() as u16, y as u16)))
}

#[test]
fn hint_text_names_clickable_keys() {
    use super::widgets::{hint_keys, parse_key};
    use KeyModifiers as M;
    assert_eq!(parse_key("Ctrl+S"), Some((KeyCode::Char('s'), M::CONTROL)));
    assert_eq!(parse_key("Ctrl-T"), Some((KeyCode::Char('t'), M::CONTROL)));
    assert_eq!(parse_key("S-Tab"), Some((KeyCode::BackTab, M::NONE)));
    assert_eq!(parse_key("F10"), Some((KeyCode::F(10), M::NONE)));
    assert_eq!(parse_key("j/k"), Some((KeyCode::Char('j'), M::NONE)));
    assert_eq!(parse_key("1-9"), Some((KeyCode::Char('1'), M::NONE)));
    assert_eq!(parse_key("Esc/p"), Some((KeyCode::Esc, M::NONE)));
    assert_eq!(parse_key("word"), None);
    let ks = |t: &str| hint_keys(t).into_iter().map(|(_, _, k, m)| (k, m)).collect::<Vec<_>>();
    assert_eq!(ks(" Enter ok · Esc cancel"), [(KeyCode::Enter, M::NONE), (KeyCode::Esc, M::NONE)]);
    assert_eq!(ks(" Ctrl+S save · Ctrl+R run · Esc for the window's keys")[1], (KeyCode::Char('r'), M::CONTROL));
    assert_eq!(ks("delete it? y confirms, any other key cancels"), [(KeyCode::Char('y'), M::NONE)]);
    assert_eq!(ks(" Format markdown (Ctrl-T switches)"), [(KeyCode::Char('t'), M::CONTROL)]);
    assert_eq!(ks("Code spaces · rust only (f: all)"), [(KeyCode::Char('f'), M::NONE)]);
    assert!(ks("a fine start, nothing to press here").is_empty(), "prose is not a hint");
    let (a, n, ..) = hint_keys("Prefs · Esc closes")[0];
    assert_eq!((a, n), (8, 3));
}

#[test]
fn clicking_a_key_named_in_hint_text_presses_it() {
    let (_t, mut a) = app();
    // the status bar: `? help`
    let s = render(&mut a, 140, 45);
    let (x, y) = find_on(&s, "? help").expect("status hint");
    a.on_mouse(click(x, y));
    assert!(matches!(a.modal, Some(Modal::Help(_))), "? opened help");
    // inside a popup: a title hint closes it
    a.modal = None;
    a.dispatch(Action::Code);
    a.on_key(key(KeyCode::Char('n')));
    let s = render(&mut a, 140, 45);
    let (x, y) = find_on(&s, "Esc cancel").or_else(|| find_on(&s, "Esc")).expect("an Esc hint");
    a.on_mouse(click(x, y));
    render(&mut a, 140, 45);
    assert!(a.code.ask.is_none(), "Esc in the hint cancelled the prompt");
}

#[test]
fn s_shares_the_selected_item_to_a_picked_target() {
    let (_t, mut a) = app();
    a.store.create_project("Alpha", "").unwrap();
    a.store.create_journal("Notes", None, crate::store::EntryFormat::Markdown).unwrap();
    a.dispatch(Action::Journals);
    let s = render(&mut a, 140, 45);
    assert!(s.contains("[S share]"), "a share button on the window\n{s}");
    // the button is a click on S
    let (x, y) = find_on(&s, "[S share]").unwrap();
    a.on_mouse(click(x + 1, y));
    let Some(Modal::Pick { items, .. }) = &a.modal else { panic!("share picker: {:?}", a.modal.is_some()) };
    assert!(items.contains(&"project · Alpha".to_string()) && items.contains(&"net · all exchanges".to_string()), "{items:?}");
    assert!(!items.iter().any(|i| i == "journal · Notes"), "not into itself");
    let i = items.iter().position(|i| i == "project · Alpha").unwrap();
    for _ in 0..i {
        a.on_key(key(KeyCode::Down));
    }
    a.on_key(key(KeyCode::Enter));
    assert!(matches!(a.modal, Some(Modal::Journals)), "back on the window");
    let p = a.store.find_project("Alpha").unwrap();
    assert_eq!(a.store.project_items(&p.id).unwrap().len(), 1);
    // Esc on the picker goes back too
    a.on_key(key(KeyCode::Char('S')));
    a.on_key(key(KeyCode::Esc));
    assert!(matches!(a.modal, Some(Modal::Journals)));
}
