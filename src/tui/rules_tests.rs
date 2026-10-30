//! TUI tests for the Rules window (R).
use super::action::Action;
use super::modal::Modal;
use super::tests::{app, key, screen_text};
use super::*;
use crate::store::{EntryFormat, ItemKind};

fn typ(a: &mut App, s: &str) {
    for c in s.chars() {
        a.on_key(key(if c == '\n' { KeyCode::Enter } else { KeyCode::Char(c) }));
    }
}

#[test]
fn rules_window_creates_tests_runs_and_removes() {
    let (_t, mut a) = app();
    a.on_key(key(KeyCode::Char('R')));
    assert!(matches!(a.modal, Some(Modal::Rules)));
    assert_eq!(a.help_topic(), "rules");
    assert!(screen_text(&mut a).contains("no rules yet"));
    // n walks through name, on, if, then, lives in; a bad step stays open with the reason
    a.on_key(key(KeyCode::Char('n')));
    typ(&mut a, "Greet\n");
    typ(&mut a, "jornal.create\n");
    assert!(a.rules.msg.as_ref().unwrap().1, "unknown kind is an error");
    assert_eq!(a.rules.ask.as_ref().unwrap().kind, rules::RAsk::On);
    for _ in 0.."jornal.create".len() {
        a.on_key(key(KeyCode::Backspace));
    }
    typ(&mut a, "journal.create\n");
    typ(&mut a, "name ~ lab\n");
    typ(&mut a, "status made {name}; note \"started {date}\"\n");
    assert_eq!(a.rules.ask.as_ref().unwrap().kind, rules::RAsk::Home);
    typ(&mut a, "\n");
    let all = a.store.rules().unwrap();
    assert_eq!(all.len(), 1, "{:?}", a.rules.msg);
    assert_eq!(all[0].spec.then.len(), 2);
    let text = screen_text(&mut a);
    assert!(text.contains("Greet") && text.contains("on journal.create") && text.contains("then status"), "{text}");

    // an action happens: the rule runs, and its message reaches the status line
    a.on_key(key(KeyCode::Esc));
    a.on_key(key(KeyCode::Esc));
    let j = a.store.create_journal("Lab book", None, EntryFormat::Markdown).unwrap();
    workspace::record(&mut a, "journal.create", "create journal Lab book".into(), serde_json::json!({ "journal": j.id, "name": j.name }));
    a.tick();
    assert!(a.status.contains("rule Greet: made Lab book"), "{}", a.status);
    assert!(a.status.contains("note failed"), "no journal to note in: {}", a.status);
    let kinds: Vec<String> = a.store.actions().unwrap().into_iter().map(|r| r.kind).collect();
    assert!(kinds.contains(&"rule.create".to_string()) && kinds.contains(&"rule.run".to_string()));

    // t tests against the newest action (the run itself never triggers rules), x dry-runs
    a.dispatch(Action::Rules);
    workspace::record(&mut a, "journal.edit", "x".into(), serde_json::json!({ "journal": j.id, "name": "Lab book" }));
    a.on_key(key(KeyCode::Char('t')));
    let text = screen_text(&mut a);
    assert!(text.contains("✗ on journal.create") && text.contains("no match"), "{text}");
    workspace::record(&mut a, "journal.create", "y".into(), serde_json::json!({ "journal": j.id, "name": "lab two" }));
    a.on_key(key(KeyCode::Char('x')));
    let text = screen_text(&mut a);
    assert!(text.contains("✓ name ~ lab") && text.contains("would show \"made lab two\""), "{text}");

    // e edits (prefilled), Space switches off, p adds to a project
    a.on_key(key(KeyCode::Char('e')));
    assert_eq!(a.rules.ask.as_ref().unwrap().value, "Greet");
    typ(&mut a, "!\n\n\n");
    a.on_key(key(KeyCode::Char('u')));
    let then = a.rules.ask.as_ref().unwrap().value.clone();
    assert!(then.starts_with("status") && then.ends_with('u'), "{then}");
    a.on_key(key(KeyCode::Backspace));
    typ(&mut a, "\njournal:Lab book\n");
    let r = a.store.find_rule("Greet!").unwrap();
    assert_eq!(r.owner_id.as_deref(), Some(j.id.as_str()));
    a.on_key(key(KeyCode::Char(' ')));
    assert!(!a.store.rule(&r.id).unwrap().unwrap().enabled);
    assert!(screen_text(&mut a).contains("○"));
    let p = a.store.create_project("Desk", "").unwrap();
    a.on_key(key(KeyCode::Char('p')));
    assert!(matches!(a.modal, Some(Modal::PickProject)));
    a.on_key(key(KeyCode::Enter));
    assert_eq!(a.store.project_rules(&p.id).unwrap().len(), 1);
    assert!(matches!(a.modal, Some(Modal::Rules)), "back to the rules");
    // d asks, y removes
    a.on_key(key(KeyCode::Char('d')));
    a.on_key(key(KeyCode::Char('y')));
    assert!(a.store.rules().unwrap().is_empty());
    assert!(a.store.project_items(&p.id).unwrap().is_empty());
    let kinds: Vec<String> = a.store.actions().unwrap().into_iter().map(|r| r.kind).collect();
    for k in ["rule.edit", "rule.disable", "project.add", "rule.remove"] {
        assert!(kinds.contains(&k.to_string()), "{k}");
    }
}

#[test]
fn rule_items_in_projects_open_the_rule() {
    let (_t, mut a) = app();
    let p = a.store.create_project("Desk", "").unwrap();
    let spec =
        crate::store::RuleSpec { on: vec!["import".into()], then: vec![crate::store::Then::Status { text: "hi".into() }], ..Default::default() };
    let r = a.store.create_rule(&crate::store::NewRule { name: "Hello".into(), spec, enabled: true, ..Default::default() }).unwrap();
    a.store.add_project_item(&p.id, ItemKind::RuleSet, &r.id, serde_json::json!({})).unwrap();
    a.dispatch(Action::Projects);
    a.on_key(key(KeyCode::Enter));
    assert!(screen_text(&mut a).contains("Hello"));
    a.on_key(key(KeyCode::Enter));
    assert!(matches!(a.modal, Some(Modal::Rules)));
    assert!(screen_text(&mut a).contains("then status hi"));
}

#[test]
fn rules_menu_entry_matches_the_key() {
    let (_t, mut a) = app();
    a.dispatch(Action::Rules);
    assert!(matches!(a.modal, Some(Modal::Rules)));
    assert!(super::menu::menus().iter().any(|m| m.items.iter().any(|i| i.label == "Rules…" && i.shortcut == "R")));
}
