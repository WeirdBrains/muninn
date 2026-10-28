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
    let a = App::new(paths, store, dek, prefs, Some(ThemeMode::Dark)).unwrap();
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
