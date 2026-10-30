//! TUI tests for the Model designer (D).
use super::action::Action;
use super::modal::Modal;
use super::models::Screen;
use super::tests::{app, key, mkey, screen_text};
use super::*;

/// Answer the open prompt with `s` (replacing what is prefilled).
fn fill(a: &mut App, s: &str) {
    let ask = a.models.ask.as_mut().expect("a prompt is open");
    ask.value = s.to_string();
    a.on_key(key(KeyCode::Enter));
}

fn kinds(a: &App) -> Vec<String> {
    a.store.actions().unwrap().into_iter().map(|r| r.kind).collect()
}

#[test]
fn designer_opens_from_key_and_menu() {
    let (_t, mut a) = app();
    a.on_key(key(KeyCode::Char('D')));
    assert!(matches!(a.modal, Some(Modal::Models)));
    assert_eq!(a.help_topic(), "models");
    let text = screen_text(&mut a);
    assert!(text.contains("bars") && text.contains("built in"), "{text}");
    a.on_key(key(KeyCode::Esc));
    assert!(a.modal.is_none());
    a.on_key(mkey(KeyCode::Char('d'), KeyModifiers::ALT));
    assert!(screen_text(&mut a).contains("Model designer"));
    a.on_key(key(KeyCode::Char('m')));
    assert_eq!(a.last_action, Some(Action::Models));
    assert!(matches!(a.modal, Some(Modal::Models)));
}

#[test]
fn design_apply_roll_back_records_and_rules() {
    let (_t, mut a) = app();
    a.on_key(key(KeyCode::Char('D')));
    // n: name, then fields; a bad spec keeps the prompt open with the reason
    a.on_key(key(KeyCode::Char('n')));
    fill(&mut a, "Signals");
    fill(&mut a, "symbol:text!; price");
    assert!(a.models.msg.as_ref().unwrap().1, "price needs a type");
    assert!(a.store.find_model("signals").is_err(), "nothing half made");
    fill(&mut a, "symbol:text!; price:real");
    assert_eq!(a.models.screen, Screen::Design);
    let text = screen_text(&mut a);
    assert!(text.contains("+ symbol") && text.contains("Migration plan"), "{text}");
    // A applies the draft as v1
    a.on_key(key(KeyCode::Char('A')));
    let m = a.store.find_model("signals").unwrap();
    assert_eq!((m.version, m.design.fields.len()), (1, 2));

    // records: n new, w where, U update all matching
    a.on_key(key(KeyCode::Char('r')));
    assert_eq!(a.models.screen, Screen::Records);
    for v in ["symbol=BTC/USDT price=42000", "symbol=ETH/USDT price=2500"] {
        a.on_key(key(KeyCode::Char('n')));
        fill(&mut a, v);
    }
    a.on_key(key(KeyCode::Char('n')));
    fill(&mut a, "price=1");
    assert!(a.models.msg.as_ref().unwrap().1, "symbol is required");
    a.on_key(key(KeyCode::Esc));
    assert_eq!(a.store.count_records(&m).unwrap(), 2);
    let text = screen_text(&mut a);
    assert!(text.contains("BTC/USDT") && text.contains("ETH/USDT"), "{text}");
    a.on_key(key(KeyCode::Char('w')));
    fill(&mut a, "price > 10000");
    let text = screen_text(&mut a);
    assert!(text.contains("BTC/USDT") && !text.contains("ETH/USDT"), "{text}");

    // design: add a field, rename one (e replaces the spec, keeps the values), plan shows it
    a.on_key(key(KeyCode::Char('m')));
    a.on_key(key(KeyCode::Char('a')));
    fill(&mut a, "note:text default=new");
    a.models.fsel = 1;
    a.on_key(key(KeyCode::Char('e')));
    fill(&mut a, "cost:real");
    let text = screen_text(&mut a);
    assert!(text.contains("+ note") && text.contains("was price"), "{text}");
    a.on_key(key(KeyCode::Char('A')));
    let m = a.store.find_model("signals").unwrap();
    assert_eq!(m.version, 2);
    let r = a.store.records(&m, &Default::default()).unwrap();
    assert!(r.iter().any(|r| r.values["cost"] == serde_json::json!(42000.0)) && r.iter().all(|r| r.values["note"] == "new"));

    // dropping asks first: y applies
    a.models.fsel = 2;
    a.on_key(key(KeyCode::Char('x')));
    assert!(screen_text(&mut a).contains("- note"));
    a.on_key(key(KeyCode::Char('A')));
    assert!(a.models.confirm.is_some());
    a.on_key(key(KeyCode::Char('n')));
    assert_eq!(a.store.find_model("signals").unwrap().version, 2, "cancelled");
    a.on_key(key(KeyCode::Char('A')));
    a.on_key(key(KeyCode::Char('y')));
    assert_eq!(a.store.find_model("signals").unwrap().version, 3);

    // history: roll back to v2 (always asks)
    a.on_key(key(KeyCode::Char('h')));
    assert_eq!(a.models.screen, Screen::History);
    assert!(screen_text(&mut a).contains("v3"));
    a.models.vsel = 1;
    a.on_key(key(KeyCode::Char('b')));
    a.on_key(key(KeyCode::Char('y')));
    let m = a.store.find_model("signals").unwrap();
    assert!(m.version == 4 && m.design.field("note").is_some(), "{m:?}");

    // records again: U sets a value on all matching, D deletes them
    a.on_key(key(KeyCode::Char('r')));
    a.on_key(key(KeyCode::Char('c')));
    a.on_key(key(KeyCode::Char('w')));
    fill(&mut a, "symbol ~ eth");
    a.on_key(key(KeyCode::Char('U')));
    fill(&mut a, "note=checked");
    a.on_key(key(KeyCode::Char('y')));
    let eth = a.store.records(&m, &crate::store::RecordQuery { text: "ETH".into(), ..Default::default() }).unwrap();
    assert_eq!(eth[0].values["note"], "checked");
    a.on_key(key(KeyCode::Char('D')));
    a.on_key(key(KeyCode::Char('y')));
    assert_eq!(a.store.count_records(&m).unwrap(), 1);

    // rules: n walks name → on → if → then (prefilled to copy the trigger into a record)
    a.on_key(key(KeyCode::Char('f')));
    assert_eq!(a.models.screen, Screen::Rules);
    a.on_key(key(KeyCode::Char('n')));
    fill(&mut a, "Feed");
    fill(&mut a, "journal.create");
    fill(&mut a, "");
    assert!(a.models.ask.as_ref().unwrap().value.starts_with("record-create"));
    fill(&mut a, "record-create symbol={name} cost=0");
    let rs = a.store.owned_rules(crate::store::OwnerKind::Model, &m.id).unwrap();
    assert_eq!(rs.len(), 1, "{:?}", a.models.msg);
    let j = a.store.create_journal("SOL/USDT", None, crate::store::EntryFormat::Markdown).unwrap();
    workspace::record(&mut a, "journal.create", "create journal".into(), serde_json::json!({ "journal": j.id, "name": j.name }));
    a.tick();
    assert_eq!(a.store.count_records(&m).unwrap(), 2, "the rule fed the model");
    // x dry-runs on the newest action, Space switches off
    a.on_key(key(KeyCode::Char('x')));
    a.on_key(key(KeyCode::Char(' ')));
    assert!(!a.store.find_rule("Feed").unwrap().enabled);

    // d on the list removes the model (asks)
    a.on_key(key(KeyCode::Esc));
    a.on_key(key(KeyCode::Esc));
    a.models.sel = a.models.list.iter().position(|x| x.name == "signals").unwrap();
    a.on_key(key(KeyCode::Char('d')));
    a.on_key(key(KeyCode::Char('y')));
    assert!(a.store.find_model("signals").is_err());
    let ks = kinds(&a);
    for k in [
        "model.create",
        "model.edit",
        "model.apply",
        "model.rollback",
        "model.remove",
        "record.create",
        "record.update",
        "record.delete",
        "rule.create",
    ] {
        assert!(ks.contains(&k.to_string()), "{k}");
    }
}

#[test]
fn bars_are_protected() {
    let (_t, mut a) = app();
    a.dispatch(Action::Models);
    a.models.sel = a.models.list.iter().position(|x| x.builtin).unwrap();
    a.on_key(key(KeyCode::Char('d')));
    assert!(a.models.msg.as_ref().unwrap().1, "built in");
    a.on_key(key(KeyCode::Enter));
    assert_eq!(a.models.screen, Screen::Design);
    let text = screen_text(&mut a);
    assert!(text.contains("built in (fixed)") && text.contains("open"), "{text}");
    a.models.fsel = 0;
    a.on_key(key(KeyCode::Char('x')));
    assert!(a.models.msg.as_ref().map(|m| m.1).unwrap_or(false), "its own fields stay");
    a.on_key(key(KeyCode::Char('a')));
    fill(&mut a, "flag:text");
    a.on_key(key(KeyCode::Char('A')));
    assert!(a.store.find_model("bars").unwrap().design.field("flag").is_some());
    a.on_key(key(KeyCode::Char('r')));
    a.on_key(key(KeyCode::Char('n')));
    assert!(a.models.msg.as_ref().unwrap().1, "bars come from imports");
}
