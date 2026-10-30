//! TUI tests for the Network window (K) and menu.
use super::action::Action;
use super::modal::Modal;
use super::network::Tab;
use super::tests::{app, key, screen_text};
use super::*;
use crate::store::{EntryFormat, ExchangeStatus, SearchSpec};
use std::time::{Duration, Instant};

fn typ(a: &mut App, s: &str) {
    for c in s.chars() {
        a.on_key(key(if c == '\n' { KeyCode::Enter } else { KeyCode::Char(c) }));
    }
}

fn kinds(a: &App) -> Vec<String> {
    a.store.actions().unwrap().into_iter().map(|r| r.kind).collect()
}

/// Loopback only, no discovery: tests never reach the real network.
fn local_node(a: &mut App, label: &str) {
    a.prefs.net.listen = vec!["/ip4/127.0.0.1/tcp/0".into()];
    a.prefs.net.mdns = false;
    a.prefs.net.label = label.into();
}

/// Tick until the window's network task is done.
fn settle(a: &mut App) {
    let t = Instant::now();
    while a.network.job.is_some() {
        assert!(t.elapsed() < Duration::from_secs(30), "network task did not finish");
        std::thread::sleep(Duration::from_millis(20));
        a.tick();
    }
}

#[test]
fn network_window_is_off_and_empty_by_default_and_saves_settings() {
    let (_t, mut a) = app();
    assert!(menu::menus().iter().any(|m| m.title == "Network"));
    a.on_key(key(KeyCode::Char('K')));
    assert!(matches!(a.modal, Some(Modal::Network)));
    assert_eq!(a.help_topic(), "network");
    let text = screen_text(&mut a);
    assert!(text.contains("stopped") && text.contains("shared items 0"), "{text}");
    assert!(!a.network.node_running());

    // Tab walks the pages; digits jump
    a.on_key(key(KeyCode::Tab));
    assert_eq!(a.network.tab, Tab::Peers);
    assert!(screen_text(&mut a).contains("no peers yet"));
    a.on_key(key(KeyCode::Char('5')));
    assert_eq!(a.network.tab, Tab::Shared);
    assert!(screen_text(&mut a).contains("nothing is shared"));

    // settings: toggle discovery, edit the label; both saved and recorded
    a.on_key(key(KeyCode::Char('8')));
    assert_eq!(a.network.tab, Tab::Settings);
    assert!(a.prefs.net.mdns);
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Enter));
    assert!(!a.prefs.net.mdns);
    a.on_key(key(KeyCode::Up));
    a.on_key(key(KeyCode::Up));
    a.on_key(key(KeyCode::Up));
    a.on_key(key(KeyCode::Enter));
    typ(&mut a, "lab one\n");
    assert_eq!(a.prefs.net.label, "lab one");
    let saved = crate::prefs::Preferences::load_or_create(&a.paths).unwrap();
    assert!(!saved.net.mdns && saved.net.label == "lab one");
    assert!(kinds(&a).contains(&"net.config".to_string()));
    // a bad port is refused with the reason
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Enter));
    for _ in 0..4 {
        a.on_key(key(KeyCode::Backspace));
    }
    typ(&mut a, "lots\n");
    assert!(a.network.msg.as_ref().unwrap().1);
    assert_eq!(a.prefs.net.port, crate::net::node::DEFAULT_PORT);

    // dark shards never ask for keys here: the CLI does it
    a.on_key(key(KeyCode::Char('6')));
    a.on_key(key(KeyCode::Char('e')));
    assert!(screen_text(&mut a).contains("mnnn net shard entrust"));
    a.on_key(key(KeyCode::Esc));
    a.on_key(key(KeyCode::Esc));
    assert!(a.modal.is_none());
}

#[test]
fn offline_offer_and_sharing_from_the_window() {
    let (_t, mut a) = app();
    a.dispatch(Action::NetExchanges);
    assert_eq!(a.network.tab, Tab::Exchanges);
    a.on_key(key(KeyCode::Char('n')));
    typ(&mut a, "\n");
    let xs = a.store.exchanges().unwrap();
    assert_eq!(xs.len(), 1);
    assert_eq!(xs[0].status, ExchangeStatus::Offered);
    assert!(a.network.detail.iter().any(|l| l.starts_with("mnnn-x1-offer:")), "{:?}", a.network.detail);
    // an offer is not a channel yet: nothing to verify or share over it
    a.on_key(key(KeyCode::Char('v')));
    assert!(a.network.msg.as_ref().unwrap().1);

    a.store.create_journal("Lab book", None, EntryFormat::Markdown).unwrap();
    a.dispatch(Action::NetShared);
    a.on_key(key(KeyCode::Char('a')));
    typ(&mut a, "journal Lab book\n");
    let sh = a.store.shares().unwrap();
    assert_eq!((sh.len(), sh[0].name.as_str(), sh[0].exchange_id.is_none()), (1, "Lab book", true));
    assert!(screen_text(&mut a).contains("with every exchange"));
    a.on_key(key(KeyCode::Char('a')));
    typ(&mut a, "gadget x\n");
    assert!(a.network.msg.as_ref().unwrap().1, "unknown kind");
    a.on_key(key(KeyCode::Char('d')));
    assert!(a.store.shares().unwrap().is_empty());
    let k = kinds(&a);
    assert!(k.contains(&"net.exchange.create".into()) && k.contains(&"net.share".into()) && k.contains(&"net.unshare".into()), "{k:?}");
}

#[test]
fn two_windows_exchange_keys_share_import_search_and_revoke() {
    let (_ta, mut a) = app();
    let (_tb, mut b) = app();
    local_node(&mut a, "anna");
    local_node(&mut b, "ben");

    // A starts its node from the menu; the node file tells other processes
    a.dispatch(Action::NetStart);
    assert!(a.network.node_running(), "{:?}", a.network.msg);
    assert!(crate::net::local::running(&a.paths.home).is_some());
    assert!(screen_text(&mut a).contains("running here as anna"));

    // A makes an offer token; B (no node of its own) pastes it, the answer flows back
    a.dispatch(Action::NetExchanges);
    a.on_key(key(KeyCode::Char('n')));
    typ(&mut a, "\n");
    let token = a.network.detail.iter().find(|l| l.starts_with("mnnn-x1-offer:")).unwrap().clone();
    b.dispatch(Action::NetExchanges);
    b.on_key(key(KeyCode::Char('a')));
    typ(&mut b, &format!("{token}\n"));
    settle(&mut b);
    assert!(!b.network.msg.as_ref().unwrap().1, "{:?}", b.network.msg);
    let bx = b.store.exchanges().unwrap().remove(0);
    assert_eq!(bx.status, ExchangeStatus::Complete);
    let code = bx.short_code.clone().unwrap();
    let t = Instant::now();
    while a.store.exchanges().unwrap()[0].status != ExchangeStatus::Complete {
        assert!(t.elapsed() < Duration::from_secs(10), "answer not delivered");
        std::thread::sleep(Duration::from_millis(50));
        a.tick();
    }
    a.dispatch(Action::NetExchanges);
    a.on_key(key(KeyCode::Char('v')));
    typ(&mut a, &format!("{code}\n"));
    assert_eq!(a.store.exchanges().unwrap()[0].status, ExchangeStatus::Verified);

    // A shares a journal over that one exchange; B lists it and imports it
    a.store.create_journal("Lab book", None, EntryFormat::Markdown).unwrap();
    a.on_key(key(KeyCode::Char('s')));
    typ(&mut a, "journal Lab book\n");
    assert_eq!(a.store.shares().unwrap()[0].exchange_id.as_deref(), Some(bx.id.as_str()));
    b.dispatch(Action::NetExchanges);
    b.on_key(key(KeyCode::Char('L')));
    settle(&mut b);
    let rm = b.network.remote.as_ref().expect("remote list");
    assert_eq!(rm.items.len(), 1);
    assert!(screen_text(&mut b).contains("Lab book"));
    b.on_key(key(KeyCode::Enter));
    settle(&mut b);
    assert!(b.store.journals().unwrap().iter().any(|j| j.name.starts_with("Lab book")), "{:?}", b.network.msg);
    let k = kinds(&b);
    assert!(k.contains(&"net.fetch".into()) && k.contains(&"net.import".into()), "{k:?}");

    // a saved live search runs here and on A, merged by origin
    b.store.save_live_search("everything", &SearchSpec::default(), None).unwrap();
    b.dispatch(Action::NetSearch);
    b.on_key(key(KeyCode::Enter));
    settle(&mut b);
    let m = b.network.msg.clone().unwrap();
    assert!(!m.1 && m.0.contains("from 2 node(s)"), "{m:?} {:?}", b.network.detail);

    // B revokes; A is told
    b.dispatch(Action::NetExchanges);
    b.on_key(key(KeyCode::Char('r')));
    b.on_key(key(KeyCode::Char('y')));
    settle(&mut b);
    assert_eq!(b.store.exchanges().unwrap()[0].status, ExchangeStatus::Revoked);
    let t = Instant::now();
    while a.store.exchanges().unwrap()[0].status != ExchangeStatus::Revoked {
        assert!(t.elapsed() < Duration::from_secs(10), "revoke not delivered");
        std::thread::sleep(Duration::from_millis(50));
    }

    a.dispatch(Action::NetStop);
    assert!(!a.network.node_running());
    assert!(crate::net::local::running(&a.paths.home).is_none());
    let k = kinds(&a);
    assert!(k.contains(&"net.node.start".into()) && k.contains(&"net.node.stop".into()));
}
