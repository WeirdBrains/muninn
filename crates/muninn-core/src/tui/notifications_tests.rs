//! TUI tests for the header bell and the Notifications window (`!`).
use super::modal::Modal;
use super::tests::{app, key, screen_text};
use super::*;
use crate::store::{EntryFormat, NewNotification};

#[test]
fn bell_window_open_read_and_follow() {
    let (_t, mut a) = app();
    assert!(!screen_text(&mut a).contains("🔔"));
    let j = a.store.create_journal("Field", None, EntryFormat::Markdown).unwrap();
    let add = |a: &App, title: &str, link: Option<String>| {
        crate::notify::add(&a.store, "test", &NewNotification { kind: "notify".into(), title: title.into(), link, ..Default::default() }).unwrap()
    };
    add(&a, "Plain one", None);
    add(&a, "About the journal", Some(format!("journal:{}", j.id)));
    assert!(screen_text(&mut a).contains("2 unread"), "{}", screen_text(&mut a));
    a.on_key(key(KeyCode::Char('!')));
    assert!(matches!(a.modal, Some(Modal::Notifications)));
    let s = screen_text(&mut a);
    assert!(s.contains("Notifications · 2 unread") && s.contains("journal Field"), "{s}");
    // Enter on the journal one opens the journal and marks it read
    a.notifications.sel = a.notifications.list.iter().position(|n| n.title == "About the journal").unwrap();
    a.on_key(key(KeyCode::Enter));
    assert!(matches!(a.modal, Some(Modal::Journals)), "{:?}", a.modal.as_ref().map(|m| m.help_topic()));
    assert_eq!(a.store.unread_notifications().unwrap(), 1);
    a.on_key(key(KeyCode::Esc));
    a.modal = None;
    a.on_key(key(KeyCode::Char('!')));
    a.on_key(key(KeyCode::Char('u')));
    assert_eq!(a.notifications.list.len(), 1);
    a.on_key(key(KeyCode::Char('R')));
    assert_eq!(a.store.unread_notifications().unwrap(), 0);
    a.on_key(key(KeyCode::Esc));
    assert!(a.modal.is_none());
    assert!(!screen_text(&mut a).contains("🔔"));
}
