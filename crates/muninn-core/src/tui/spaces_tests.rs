//! TUI tests for the Code spaces window (Code ▸ Code spaces…).
use super::modal::Modal;
use super::tests::{app, key, screen_text};
use super::*;
use crate::store::{CodeInput, ItemKind, Lang, SpaceKind};

#[test]
fn p_adds_a_space_to_a_project_from_the_list_and_from_its_members() {
    let (_t, mut a) = app();
    let c = a.store.create_code_space("dbl", "", Lang::Math, "y = 2", CodeInput::default()).unwrap();
    let s = crate::code::spaces::create(&a.store, "test", "pair", "", SpaceKind::Transform, &c).unwrap();
    let lab = a.store.create_project("Lab", "").unwrap();
    let other = a.store.create_project("Other", "").unwrap();
    spaces::open(&mut a);
    assert!(screen_text(&mut a).contains("p add to project"));
    a.on_key(key(KeyCode::Char('p')));
    assert!(matches!(a.modal, Some(Modal::PickProject)));
    a.on_key(key(KeyCode::Enter));
    assert!(matches!(a.modal, Some(Modal::Spaces)), "back to the spaces window");
    let pick = |a: &App, p: &str| a.store.project_items(p).unwrap().iter().any(|i| i.kind == ItemKind::Space && i.ref_id == s.id);
    assert!(pick(&a, &lab.id) || pick(&a, &other.id));
    // from its members, into the other project
    let first = if pick(&a, &lab.id) { other.id.clone() } else { lab.id.clone() };
    a.on_key(key(KeyCode::Enter));
    assert_eq!(a.spaces.screen, spaces::Screen::Members);
    a.on_key(key(KeyCode::Char('p')));
    assert!(matches!(a.modal, Some(Modal::PickProject)));
    for _ in 0..3 {
        if a.pick_project.list.get(a.pick_project.sel).map(|p| p.id.as_str()) == Some(first.as_str()) {
            break;
        }
        a.on_key(key(KeyCode::Down));
    }
    a.on_key(key(KeyCode::Enter));
    assert!(matches!(a.modal, Some(Modal::Spaces)));
    assert!(pick(&a, &lab.id) && pick(&a, &other.id));
    // the project opens it again
    let it = a.store.project_items(&lab.id).unwrap().into_iter().find(|i| i.kind == ItemKind::Space).unwrap();
    spaces::open_space(&mut a, &it.ref_id);
    assert_eq!(a.spaces.screen, spaces::Screen::Members);
    assert_eq!(a.spaces.open.as_ref().map(|x| x.id.clone()), Some(s.id));
}

#[test]
fn s_shares_a_space_from_its_window_to_every_place_code_goes() {
    let (_t, mut a) = app();
    let c = a.store.create_code_space("dbl", "", Lang::Math, "y = 2", CodeInput::default()).unwrap();
    crate::code::spaces::create(&a.store, "test", "pair", "", SpaceKind::Transform, &c).unwrap();
    let c2 = a.store.create_code_space("ten", "", Lang::Math, "z = 10", CodeInput::default()).unwrap();
    crate::code::spaces::create(&a.store, "test", "tenx", "", SpaceKind::Transform, &c2).unwrap();
    a.store.create_project("Lab", "").unwrap();
    spaces::open(&mut a);
    let text = screen_text(&mut a);
    assert!(text.contains("S share") && text.contains("S Share"), "the key and the button: {text}");
    a.on_key(key(KeyCode::Char('S')));
    let Some(Modal::Pick { items, .. }) = &a.modal else { panic!("the share picker") };
    assert!(items.iter().any(|i| i == "project · Lab"), "{items:?}");
    assert!(items.iter().any(|i| i.starts_with("journal")) || a.store.journals().unwrap().is_empty());
    assert!(items.iter().any(|i| i == "net · all exchanges"), "{items:?}");
    let me = a.store.find_space("pair").unwrap();
    let other = if a.spaces.list.first().map(|s| s.id.clone()) == Some(me.id.clone()) { "tenx" } else { "pair" };
    assert!(items.iter().any(|i| *i == format!("space · {other}")), "a space takes another space's output: {items:?}");
    let sel = a.spaces.list.get(a.spaces.sel).map(|s| s.name.clone()).unwrap();
    assert!(!items.iter().any(|i| *i == format!("space · {sel}")), "not into itself: {items:?}");
}

#[test]
fn g_gives_a_space_to_an_org() {
    let (_t, mut a) = app();
    let c = a.store.create_code_space("dbl", "", Lang::Math, "y = 2", CodeInput::default()).unwrap();
    let s = crate::code::spaces::create(&a.store, "test", "pair", "", SpaceKind::Transform, &c).unwrap();
    spaces::open(&mut a);
    a.on_key(key(KeyCode::Char('g')));
    assert!(a.spaces.msg.as_ref().is_some_and(|(m, e)| *e && m.contains("no organizations")));
    let o = crate::org::create(&a.store, "test", "Lab", None, "").unwrap();
    a.on_key(key(KeyCode::Char('g')));
    assert!(matches!(a.modal, Some(Modal::Pick { .. })));
    a.on_key(key(KeyCode::Enter));
    assert!(matches!(a.modal, Some(Modal::Spaces)), "back to the window");
    let rs = a.store.resources(&o.id).unwrap();
    assert!(rs.iter().any(|r| r.kind == crate::store::ShareKind::Space && r.ref_id == s.id), "{rs:?}");
}

#[test]
fn the_plugins_menu_lists_spaces_by_kind() {
    let (_t, mut a) = app();
    let c = a.store.create_code_space("p1", "", Lang::Math, "y = 2", CodeInput::default()).unwrap();
    let d = a.store.create_code_space("t1", "", Lang::Math, "y = 3", CodeInput::default()).unwrap();
    crate::code::spaces::create(&a.store, "test", "csvish", "", SpaceKind::Parser, &c).unwrap();
    crate::code::spaces::create(&a.store, "test", "shaper", "", SpaceKind::Transform, &d).unwrap();
    // the bar has Plug, with an entry per kind and All plugins
    let m = super::menu::menus();
    let plug = m.iter().find(|m| m.title == "Plug").expect("a Plug menu");
    assert_eq!(plug.mnemonic, 'l');
    let labels: Vec<&str> = plug.items.iter().map(|i| i.label).filter(|l| !l.is_empty()).collect();
    assert_eq!(labels, ["Parsers…", "Transforms…", "Analysis…", "Ingest connectors…", "All plugins…"]);
    a.dispatch(super::action::Action::Plugins(Some(SpaceKind::Parser)));
    assert!(matches!(a.modal, Some(Modal::Spaces)));
    let text = screen_text(&mut a);
    assert!(text.contains("Plugins ▸ Parsers") && text.contains("csvish") && !text.contains("shaper"), "{text}");
    a.dispatch(super::action::Action::Plugins(Some(SpaceKind::IngestConnector)));
    assert!(screen_text(&mut a).contains("plugins install PATH|CODE --kind ingest-connector"));
    a.dispatch(super::action::Action::Plugins(None));
    let text = screen_text(&mut a);
    assert!(text.contains("All plugins") && text.contains("csvish") && text.contains("shaper"), "{text}");
    // Space disables from here too
    a.on_key(key(KeyCode::Char(' ')));
    assert!(a.store.spaces().unwrap().iter().any(|s| !s.enabled));
    // the Code menu's window is unfiltered again
    spaces::open(&mut a);
    assert!(!screen_text(&mut a).contains("Plugins ▸"));
}
