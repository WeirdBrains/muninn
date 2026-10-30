//! TUI tests for the Code spaces window (C).
use super::action::Action;
use super::code::{CAsk, Confirm, Editor, Screen};
use super::modal::Modal;
use super::tests::{app, key, screen_text};
use super::*;
use crate::store::ItemKind;

fn typ(a: &mut App, s: &str) {
    for c in s.chars() {
        a.on_key(key(if c == '\n' { KeyCode::Enter } else { KeyCode::Char(c) }));
    }
}

fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

#[test]
fn the_editor_inserts_joins_and_moves() {
    let mut e = Editor::new("ab\ncd\n");
    assert_eq!(e.lines, vec!["ab", "cd"]);
    e.key(key(KeyCode::End), 10);
    e.key(key(KeyCode::Enter), 10);
    e.key(key(KeyCode::Char('é')), 10);
    assert_eq!(e.text(), "ab\né\ncd\n");
    e.key(key(KeyCode::Backspace), 10);
    e.key(key(KeyCode::Backspace), 10);
    assert_eq!(e.text(), "ab\ncd\n");
    e.key(key(KeyCode::Delete), 10);
    assert_eq!(e.text(), "abcd\n");
    // indentation carries over a new line
    let mut e = Editor::new("    x = 1");
    e.key(key(KeyCode::End), 10);
    e.key(key(KeyCode::Enter), 10);
    e.key(key(KeyCode::Char('y')), 10);
    assert_eq!(e.text(), "    x = 1\n    y\n");
    assert!(!e.key(key(KeyCode::F(2)), 10));
}

#[test]
fn code_window_creates_edits_and_approves_before_running() {
    let (_t, mut a) = app();
    a.on_key(key(KeyCode::Char('C')));
    assert!(matches!(a.modal, Some(Modal::Code)));
    assert_eq!(a.help_topic(), "code-spaces");
    assert!(screen_text(&mut a).contains("no code spaces yet"));
    assert!(menu::menus().iter().any(|m| m.items.iter().any(|i| i.label == "Code spaces…" && i.shortcut == "C")));

    // n asks for a name and a language; a bad language stays on that step
    a.on_key(key(KeyCode::Char('n')));
    typ(&mut a, "fast\n");
    assert_eq!(a.code.ask.as_ref().unwrap().value, "math", "math is offered");
    a.code.ask.as_mut().unwrap().value.clear();
    typ(&mut a, "cobol\n");
    assert!(a.code.msg.as_ref().unwrap().1);
    assert_eq!(a.code.ask.as_ref().unwrap().kind, CAsk::Lang);
    for _ in 0.."cobol".len() {
        a.on_key(key(KeyCode::Backspace));
    }
    typ(&mut a, "math\n");
    let c = a.store.find_code_space("fast").unwrap_or_else(|e| panic!("{e}: {:?} {:?}", a.code.msg, a.code.ask));
    assert_eq!(c.source, crate::code::template(crate::store::Lang::Math));
    assert_eq!(a.code.screen, Screen::Edit);
    assert!(a.code.editing, "a new space opens in the editor");
    assert!(a.typing(), "letters are typed, not commands");

    // type over the template: q and C are text here
    a.code.editor = Editor::new("");
    typ(&mut a, "@title Quick\nC = close * 2\n");
    a.on_key(ctrl('s'));
    let c = a.store.find_code_space("fast").unwrap();
    assert_eq!(c.source, "@title Quick\nC = close * 2\n");
    a.on_key(key(KeyCode::Esc));
    assert!(!a.code.editing);
    let text = screen_text(&mut a);
    assert!(text.contains("Code space fast") && text.contains("C = close * 2") && text.contains("not approved"), "{text}");

    // i sets the input (checked)
    a.on_key(key(KeyCode::Char('i')));
    typ(&mut a, "bars wher x\n");
    assert!(a.code.ask.is_some(), "a bad input stays open");
    a.code.ask.as_mut().unwrap().value = "bars where symbol = BTC/USDT; sort timestamp; limit 5".into();
    a.on_key(key(KeyCode::Enter));
    let c = a.store.find_code_space("fast").unwrap();
    assert_eq!(c.input.model, "bars");
    assert_eq!(c.input.limit, Some(5));

    // r asks to approve first; n leaves it unapproved and nothing runs
    a.on_key(key(KeyCode::Char('r')));
    assert_eq!(a.code.confirm, Some(Confirm::ApproveRun));
    assert!(screen_text(&mut a).contains("native code"));
    a.on_key(key(KeyCode::Char('n')));
    assert!(!crate::code::approved(&a.store.find_code_space("fast").unwrap()));

    // p adds it to a project and comes back
    let p = a.store.create_project("Lab", "").unwrap();
    a.on_key(key(KeyCode::Char('p')));
    assert!(matches!(a.modal, Some(Modal::PickProject)));
    a.on_key(key(KeyCode::Enter));
    assert!(matches!(a.modal, Some(Modal::Code)));
    let items = a.store.project_items(&p.id).unwrap();
    assert!(items.iter().any(|i| i.kind == ItemKind::Code && i.ref_id == c.id));

    // an unsaved change asks before leaving
    a.on_key(key(KeyCode::Char('e')));
    typ(&mut a, "x");
    a.on_key(key(KeyCode::Esc));
    a.on_key(key(KeyCode::Esc));
    assert_eq!(a.code.confirm, Some(Confirm::Discard));
    a.on_key(key(KeyCode::Char('y')));
    assert_eq!(a.code.screen, Screen::List);
    assert_eq!(a.store.find_code_space("fast").unwrap().source, "@title Quick\nC = close * 2\n");

    // the project opens the space in the editor
    a.modal = None;
    super::code::open_space(&mut a, &c.id);
    assert_eq!(a.code.screen, Screen::Edit);

    // d deletes after y
    a.on_key(key(KeyCode::Esc));
    a.on_key(key(KeyCode::Char('d')));
    a.on_key(key(KeyCode::Char('y')));
    assert!(a.store.code_spaces().unwrap().is_empty());
    let kinds: Vec<String> = a.store.actions().unwrap().into_iter().map(|r| r.kind).collect();
    for k in ["code.create", "code.edit", "code.remove", "project.add"] {
        assert!(kinds.contains(&k.to_string()), "{k}: {kinds:?}");
    }
    a.dispatch(Action::Code);
    assert!(matches!(a.modal, Some(Modal::Code)));
}

#[test]
fn code_window_builds_on_a_thread_and_shows_the_result() {
    if crate::code::cargo().is_err() {
        eprintln!("skipped: no cargo");
        return;
    }
    let (_t, mut a) = app();
    let c = crate::code::create(
        &a.store,
        "cli",
        "double",
        "",
        crate::store::Lang::Math,
        "@title Doubled\ndouble = close * 2\n",
        crate::code::parse_input("bars where symbol = BTC/USDT; sort timestamp; limit 3").unwrap(),
    )
    .unwrap();
    super::code::open_space(&mut a, &c.id);
    a.on_key(key(KeyCode::Char('r')));
    a.on_key(key(KeyCode::Char('y')));
    assert!(a.code.building.is_some(), "not built yet: builds first, then runs");
    let t = std::time::Instant::now();
    while a.code.building.is_some() && t.elapsed().as_secs() < 900 {
        std::thread::sleep(std::time::Duration::from_millis(200));
        a.tick();
    }
    let info = a.code.result.clone().expect("ran");
    assert_eq!(info.status, "ok", "{:?} {:?}", info, a.code.log);
    let text = screen_text(&mut a);
    assert!(text.contains("Doubled") && text.contains("double"), "{text}");
    let kinds: Vec<String> = a.store.actions().unwrap().into_iter().map(|r| r.kind).collect();
    for k in ["code.approve", "code.build", "code.run"] {
        assert!(kinds.contains(&k.to_string()), "{k}: {kinds:?}");
    }
    // t widens the result tile
    a.on_key(key(KeyCode::Char('t')));
    assert!(a.code.wide);
}
