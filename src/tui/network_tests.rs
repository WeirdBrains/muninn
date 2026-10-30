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
    assert!(menu::menus().iter().any(|m| m.title == "Net"));
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

    // accept-anyone and the web server with the node are settings too
    a.network.sel = 6;
    a.on_key(key(KeyCode::Enter));
    assert!(a.prefs.hosting.autostart);
    a.network.sel = 7;
    a.on_key(key(KeyCode::Enter));
    assert!(a.prefs.net.accept_anyone());
    let saved = crate::prefs::Preferences::load_or_create(&a.paths).unwrap();
    assert!(saved.hosting.autostart && saved.net.accept_anyone());
    assert!(screen_text(&mut a).contains("web server with the node"));

    // dark shards: the entrust form opens even with nobody to entrust to
    a.on_key(key(KeyCode::Char('6')));
    a.on_key(key(KeyCode::Char('e')));
    assert!(a.network.entrust.is_some());
    assert!(screen_text(&mut a).contains("no connected nodes"));
    a.on_key(key(KeyCode::Esc));
    assert!(a.network.entrust.is_none());
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
    assert!(a.network.detail.iter().any(|l| l.starts_with("muninn://join/")), "{:?}", a.network.detail);
    assert!(screen_text(&mut a).contains("pending"));
    // an offer is not a channel yet: nothing to verify or share over it
    a.on_key(key(KeyCode::Char('v')));
    assert!(a.network.msg.as_ref().unwrap().1);

    a.store.create_journal("Lab book", None, EntryFormat::Markdown).unwrap();
    a.dispatch(Action::NetShared);
    a.on_key(key(KeyCode::Char('a')));
    typ(&mut a, "journal Lab book\n");
    let sh = a.store.shares().unwrap();
    assert_eq!((sh.len(), sh[0].name.as_str(), sh[0].exchange_id.is_none()), (1, "Lab book", true));
    assert!(screen_text(&mut a).contains("with every connection"));
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
    let token = a.network.detail.iter().find(|l| l.starts_with("muninn://join/")).unwrap().clone();
    b.dispatch(Action::NetExchanges);
    b.on_key(key(KeyCode::Char('p')));
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

/// An app whose home has keys (master key, words), so entrusting can check the key file.
fn keyed_app() -> (tempfile::TempDir, App, std::path::PathBuf) {
    let t = tempfile::tempdir().unwrap();
    let paths = crate::paths::Paths::resolve(Some(t.path().join("h")), None).unwrap();
    let pw = secrecy::SecretString::from("unlock");
    let out = crate::crypto::keyring::init(&paths, &pw).unwrap();
    let words = t.path().join("words.txt");
    std::fs::write(&words, out.words.as_str()).unwrap();
    let dek = crate::crypto::keyring::unlock_with_password(&paths, &pw).unwrap();
    let store = crate::store::Store::open(&paths.db, Some(&dek.0)).unwrap();
    let prefs = crate::prefs::Preferences::load_or_create(&paths).unwrap();
    let a = App::new(paths, store, dek, prefs, Some(crate::prefs::ThemeMode::Dark)).unwrap();
    (t, a, words)
}

fn addr(a: &App) -> String {
    let h = a.network.hosted.as_ref().expect("node running");
    let st = h.node.status();
    format!("{}/p2p/{}", st.listening[0], h.node.peer_id())
}

#[test]
fn connections_are_automatic_and_shards_go_to_picked_nodes() {
    let (_ta, mut a, words) = keyed_app();
    let (_tb, mut b) = app();
    let (_tc, mut c) = app();
    for (x, l) in [(&mut a, "anna"), (&mut b, "ben"), (&mut c, "cleo")] {
        local_node(x, l);
        x.dispatch(Action::NetStart);
        assert!(x.network.node_running(), "{:?}", x.network.msg);
    }
    // A connects; B and C see a request and accept it: no tokens or codes anywhere
    for x in [&mut b, &mut c] {
        let to = addr(x);
        a.dispatch(Action::NetExchanges);
        a.on_key(key(KeyCode::Char('c')));
        typ(&mut a, &format!("{to}\n"));
        settle(&mut a);
        let m = a.network.msg.clone().unwrap();
        assert!(!m.1 && m.0.starts_with("pending"), "{m:?}");
        x.dispatch(Action::NetExchanges);
        let text = screen_text(x);
        assert!(text.contains("anna") && text.contains("requested"), "{text}");
        assert!(!text.contains("mnnn-x1-"));
        x.on_key(key(KeyCode::Char('a')));
        settle(x);
        assert!(x.network.msg.as_ref().unwrap().0.starts_with("connected"), "{:?}", x.network.msg);
    }
    let t = Instant::now();
    while a.store.exchanges().unwrap().iter().filter(|x| x.status.active()).count() < 2 {
        assert!(t.elapsed() < Duration::from_secs(15), "answers not delivered");
        std::thread::sleep(Duration::from_millis(50));
        a.tick();
    }
    a.dispatch(Action::NetExchanges);
    assert_eq!(screen_text(&mut a).matches("connected").count(), 2);

    // they are untrusted connections: none is offered for shards until trusted
    let t = Instant::now();
    while a.store.connections().unwrap().len() < 2 {
        assert!(t.elapsed() < Duration::from_secs(20), "users not introduced");
        std::thread::sleep(Duration::from_millis(50));
        a.tick();
    }
    a.dispatch(Action::NetEntrust);
    assert!(a.network.entrust.as_ref().unwrap().peers.is_empty());
    assert!(a.network.msg.as_ref().unwrap().0.contains("untrusted"), "{:?}", a.network.msg);
    a.on_key(key(KeyCode::Esc));
    a.dispatch(Action::NetConnections);
    let text = screen_text(&mut a);
    assert!(text.contains("untrusted") && text.contains("you:"), "{text}");
    a.on_key(key(KeyCode::Char('t')));
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Char('t')));
    assert!(a.store.connections().unwrap().iter().all(|c| c.trusted));
    assert!(kinds(&a).contains(&"connection.trust".to_string()));

    // entrust: pick both nodes, the threshold is suggested, passwords are masked
    a.dispatch(Action::NetEntrust);
    assert_eq!(a.network.entrust.as_ref().unwrap().peers.len(), 2);
    a.on_key(key(KeyCode::Char(' ')));
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Char(' ')));
    assert_eq!(a.network.entrust.as_ref().unwrap().form.get("threshold").as_deref(), Some("2"));
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Enter));
    typ(&mut a, &format!("{}\n", words.display()));
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Enter));
    typ(&mut a, "s3cret-shard-pw\n");
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Enter));
    typ(&mut a, "s3cret-shard-pw\n");
    let text = screen_text(&mut a);
    assert!(!text.contains("s3cret") && !text.contains(&std::fs::read_to_string(&words).unwrap()[..12]), "{text}");
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Enter));
    settle(&mut a);
    let st = a.network.entrust.as_ref().unwrap().status.clone();
    assert!(st[0].starts_with("entrusted: any 2 of 2"), "{st:?} {:?}", a.network.msg);
    assert_eq!(st.iter().filter(|l| l.starts_with("✓")).count(), 2);
    assert_eq!((b.store.held_shards().unwrap().len(), c.store.held_shards().unwrap().len()), (1, 1));
    // the passwords were cleared and never reach history
    assert!(a.network.entrust.as_ref().unwrap().form.get("shard pass").is_none());
    for r in a.store.actions().unwrap() {
        let all = format!("{} {}", r.summary, r.params);
        assert!(!all.contains("s3cret"), "{all}");
    }
    // check restore: the shards rebuild this database's key
    for _ in 0..3 {
        a.on_key(key(KeyCode::Up));
    }
    a.on_key(key(KeyCode::Enter));
    typ(&mut a, "s3cret-shard-pw\n");
    for _ in 0..4 {
        a.on_key(key(KeyCode::Down));
    }
    a.on_key(key(KeyCode::Enter));
    settle(&mut a);
    let st = a.network.entrust.as_ref().unwrap().status.clone();
    assert!(st[0].starts_with("✓ your shards restore"), "{st:?} {:?}", a.network.msg);
    for x in [&mut a, &mut b, &mut c] {
        x.dispatch(Action::NetStop);
    }
}

#[test]
fn invites_page_lists_and_withdraws_invites() {
    let (_t, mut a) = app();
    a.dispatch(Action::NetInvites);
    assert_eq!(a.network.tab, Tab::Invites);
    assert!(screen_text(&mut a).contains("no open invites"));
    a.on_key(key(KeyCode::Char('n')));
    typ(&mut a, "\n");
    a.dispatch(Action::NetInvites);
    let text = screen_text(&mut a);
    assert!(text.contains("invite") && text.contains("anyone with the link"), "{text}");
    a.on_key(key(KeyCode::Enter));
    assert!(a.network.detail.iter().any(|l| l.starts_with("muninn://join/")), "{:?}", a.network.detail);
    a.on_key(key(KeyCode::Char('d')));
    assert!(a.network.msg.as_ref().unwrap().1);
    a.on_key(key(KeyCode::Char('r')));
    a.on_key(key(KeyCode::Char('y')));
    assert_eq!(a.store.exchanges().unwrap()[0].status, ExchangeStatus::Revoked);
    assert!(screen_text(&mut a).contains("no open invites"));
    // digits reach the new pages: 9 Connections, 0 Invites
    a.on_key(key(KeyCode::Char('9')));
    assert_eq!(a.network.tab, Tab::Users);
    assert!(screen_text(&mut a).contains("no connections yet"));
    a.on_key(key(KeyCode::Char('0')));
    assert_eq!(a.network.tab, Tab::Invites);
}
