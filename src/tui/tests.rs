//! TUI tests on ratatui's TestBackend (no real terminal).
use super::*;
use crate::prefs::ThemeMode;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyEventState, KeyModifiers};
use ratatui::Terminal;

fn key(c: KeyCode) -> KeyEvent {
    KeyEvent { code: c, modifiers: KeyModifiers::NONE, kind: KeyEventKind::Press, state: KeyEventState::NONE }
}

fn app() -> (tempfile::TempDir, App) {
    let t = tempfile::tempdir().unwrap();
    let paths = Paths::resolve(Some(t.path().join("h")), None).unwrap();
    paths.ensure().unwrap();
    let dek = crate::crypto::Key32::random().unwrap();
    let mut store = Store::open(&paths.db, Some(&dek.0)).unwrap();
    let csv = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ETHUSDT_5m.csv");
    let opts = crate::ingest::IngestOptions { source: Some("binance".into()), ..Default::default() };
    crate::ingest::ingest_csv(&mut store, crate::interact::Interact::batch(true), opts, &csv, &Default::default()).unwrap();
    let prefs = Preferences::load_or_create(&paths).unwrap();
    let a = App::new(paths, store, dek, prefs, Some(ThemeMode::Dark)).unwrap();
    (t, a)
}

fn screen_text(a: &mut App) -> String {
    let mut term = Terminal::new(TestBackend::new(140, 45)).unwrap();
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

#[test]
fn navigator_shows_series_and_pages() {
    let (_t, mut a) = app();
    let s = screen_text(&mut a);
    assert!(s.contains("ETH/USDT"), "{s}");
    assert!(s.contains("300"));
    assert!(s.contains("2017-08-17 04:00:00"));
    a.on_key(key(KeyCode::Enter));
    a.on_key(key(KeyCode::PageDown));
    assert_eq!(a.nav.page, 1);
    assert_eq!(a.nav.bars.len(), 100);
}

#[test]
fn every_screen_renders_and_tab_cycles() {
    let (_t, mut a) = app();
    for s in Screen::ALL {
        assert_eq!(a.screen, s);
        let txt = screen_text(&mut a);
        assert!(!txt.trim().is_empty());
        a.on_key(key(KeyCode::Tab));
    }
    assert_eq!(a.screen, Screen::Navigator);
}

#[test]
fn ingest_form_prompts_inline_for_missing_values() {
    let (t, mut a) = app();
    a.on_key(key(KeyCode::Char('2')));
    let p = t.path().join("unnamed.csv");
    std::fs::copy(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/unnamed.csv"), &p).unwrap();
    a.ingest.form.fields[0].value = p.display().to_string();
    a.ingest.form.fields[1].value = "local".into();
    forms::run_ingest(&mut a);
    assert!(a.status.contains("missing symbol"), "{}", a.status);
    assert!(a.ingest.form.editing);
    for c in "BTC/USDT".chars() {
        a.on_key(key(KeyCode::Char(c)));
    }
    a.on_key(key(KeyCode::Enter));
    forms::run_ingest(&mut a);
    assert!(a.status.contains("ingested 1"), "{}", a.status);
    assert!(a.nav.series.iter().any(|s| s.source == "local"));
}

#[test]
fn export_from_tui() {
    let (t, mut a) = app();
    a.on_key(key(KeyCode::Char('3')));
    let out = t.path().join("x.csv");
    a.export.form.fields[0].value = out.display().to_string();
    a.export.form.sel = a.export.form.fields.len() + 1;
    a.on_key(key(KeyCode::Enter));
    assert!(a.status.contains("wrote 300"), "{}", a.status);
    assert_eq!(std::fs::read_to_string(out).unwrap().lines().count(), 301);
}

#[test]
fn theme_selector_mode_and_editor() {
    let (_t, mut a) = app();
    a.on_key(key(KeyCode::Char('5')));
    let s = screen_text(&mut a);
    assert!(s.contains("Tokyo Night · dark"), "{s}");
    assert!(s.contains("Tokyo Night · light"));
    a.on_key(key(KeyCode::Down)); // catppuccin
    a.on_key(key(KeyCode::Enter));
    assert_eq!(a.prefs.theme.name, "catppuccin");
    a.on_key(key(KeyCode::Char('m')));
    assert_eq!(a.prefs.theme.mode, ThemeMode::Dark);
    a.on_key(key(KeyCode::Char('m')));
    assert_eq!(a.prefs.theme.mode, ThemeMode::Light);
    assert_eq!(a.variant, Variant::Light);
    let saved = Preferences::load_or_create(&a.paths).unwrap();
    assert_eq!(saved.theme.name, "catppuccin");
    // editor: set fg to a low-contrast hex, see a warning, save as new theme
    a.on_key(key(KeyCode::Char('e')));
    a.on_key(key(KeyCode::Down)); // fg
    a.on_key(key(KeyCode::Enter));
    for c in "eff1f5".chars() {
        a.on_key(key(KeyCode::Char(c)));
    }
    a.on_key(key(KeyCode::Enter));
    let s = screen_text(&mut a);
    assert!(s.contains("contrast"), "{s}");
    a.on_key(key(KeyCode::Char('s')));
    a.on_key(key(KeyCode::Enter));
    assert!(a.status.contains("saved theme catppuccin-custom"), "{}", a.status);
    assert!(a.paths.theme_dir("catppuccin-custom").join("interface.toml").exists());
    a.on_key(key(KeyCode::Esc));
    assert!(a.themes.editor.is_none());
}

#[test]
fn help_is_context_sensitive() {
    let (_t, mut a) = app();
    a.on_key(key(KeyCode::Char('4')));
    a.on_key(key(KeyCode::Char('?')));
    assert_eq!(a.help.as_ref().unwrap().current.id, "keys");
    let s = screen_text(&mut a);
    assert!(s.contains("Help: Keys"), "{s}");
    a.on_key(key(KeyCode::Esc));
    assert!(a.help.is_none());
    a.on_key(key(KeyCode::Char('5')));
    a.on_key(key(KeyCode::F(1)));
    assert_eq!(a.help.as_ref().unwrap().current.id, "themes");
    a.on_key(key(KeyCode::Esc));
    a.on_key(key(KeyCode::Char('6')));
    assert_eq!(a.help.as_ref().unwrap().current.id, "help");
}

#[test]
fn keys_screen_hides_secrets_until_revealed() {
    let (_t, mut a) = app();
    a.on_key(key(KeyCode::Char('4')));
    a.keys.secret_lines = vec!["muninn-shard-v1:secret".into()];
    let s = screen_text(&mut a);
    assert!(!s.contains("muninn-shard-v1:secret"));
    a.keys.form.sel = a.keys.form.fields.len() + 3;
    a.on_key(key(KeyCode::Enter));
    assert!(screen_text(&mut a).contains("muninn-shard-v1:secret"));
    a.on_key(key(KeyCode::Enter));
    assert!(a.keys.secret_lines.is_empty());
}
