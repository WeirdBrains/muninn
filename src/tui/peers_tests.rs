//! TUI tests for the Peers window (`W`): messages, live sessions, cursors.
use super::modal::Modal;
use super::peers::{Tab, with_cursors};
use super::tests::{app, key, screen_text};
use super::*;
use crate::store::{EntryFormat, Message};

fn typ(a: &mut App, s: &str) {
    for c in s.chars() {
        a.on_key(key(if c == '\n' { KeyCode::Enter } else { KeyCode::Char(c) }));
    }
}

#[test]
fn cursors_are_marked_in_the_text() {
    let here = vec![("ana".to_string(), Some(4)), ("ben".to_string(), None), ("cy".to_string(), Some(99))];
    assert_eq!(with_cursors("buy low", &here), "buy ▏ana▕low▏cy▕");
}

#[test]
fn messages_and_a_hosted_session_edit_show_and_stop() {
    let (_t, mut a) = app();
    a.on_key(key(KeyCode::Char('W')));
    assert!(matches!(a.modal, Some(Modal::Peers)));
    assert!(screen_text(&mut a).contains("no peers yet"), "{}", screen_text(&mut a));
    // a message that arrived, and its notification's link
    let m = Message {
        id: "m1".into(),
        peer: "PEER1".into(),
        peer_label: "ana".into(),
        direction: "in".into(),
        about: Some("journal:J".into()),
        text: "the comet is back".into(),
        origin: "ME".into(),
        created_at: 1,
        delivered_at: Some(1),
        read_at: None,
        error: None,
    };
    a.store.put_message(&m).unwrap();
    a.on_key(key(KeyCode::Char('2')));
    let s = screen_text(&mut a);
    assert!(s.contains("the comet is back") && s.contains("1 unread"), "{s}");
    a.on_key(key(KeyCode::Char('r')));
    assert_eq!(a.store.unread_messages().unwrap(), 0);
    // a session this node hosts: show it with a guest's cursor, edit, stop
    let j = a.store.create_journal("Lab", None, EntryFormat::Markdown).unwrap();
    let e = a.store.add_entry(&j.id, "Plan", "buy low", None).unwrap();
    let me = crate::net::identity::load_or_create(&a.store).unwrap().public().to_peer_id().to_string();
    let ls = crate::collab::session::host(&a.store, "test", &me, "entry", &e.id, &["G".into()]).unwrap();
    a.store.set_presence(&ls.id, "G", "ben", Some(3)).unwrap();
    a.on_key(key(KeyCode::Char('3')));
    assert_eq!(a.peers.tab, Tab::Live);
    a.on_key(key(KeyCode::Enter));
    let s = screen_text(&mut a);
    assert!(s.contains("here: ben at 3") && s.contains("buy▏ben▕ low"), "{s}");
    a.on_key(key(KeyCode::Char('e')));
    assert!(a.peers.typing());
    typ(&mut a, ", sell high\n");
    assert_eq!(a.store.entry(&e.id).unwrap().unwrap().body, "buy low, sell high");
    a.on_key(key(KeyCode::Char('x')));
    assert_eq!(a.store.live_session(&ls.id).unwrap().unwrap().status, "closed");
    a.on_key(key(KeyCode::Esc));
    a.on_key(key(KeyCode::Esc));
    assert!(a.modal.is_none());
}
