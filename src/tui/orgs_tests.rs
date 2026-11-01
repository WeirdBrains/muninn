//! TUI tests for the Organizations window (G) and menu.
use super::action::Action;
use super::modal::Modal;
use super::orgs::Tab;
use super::tests::{app, key, screen_text};
use super::*;
use crate::store::{MemberStatus, Published, ResourceState, Visibility};
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

fn settle(a: &mut App) {
    let t = Instant::now();
    while a.orgs.job.is_some() {
        assert!(t.elapsed() < Duration::from_secs(30), "org task did not finish");
        std::thread::sleep(Duration::from_millis(20));
        a.tick();
    }
    assert!(!a.orgs.msg.as_ref().is_some_and(|m| m.1), "{:?}", a.orgs.msg);
}

#[test]
fn organizations_window_makes_and_runs_an_org_and_serves_its_site() {
    let (_t, mut a) = app();
    local_node(&mut a, "anna");
    assert!(menu::menus().iter().any(|m| m.title == "Orgs"));
    a.on_key(key(KeyCode::Char('G')));
    assert!(matches!(a.modal, Some(Modal::Orgs)));
    assert_eq!(a.help_topic(), "organizations");
    assert!(screen_text(&mut a).contains("no organizations yet"));

    // n makes one: private and members only
    a.on_key(key(KeyCode::Char('n')));
    typ(&mut a, "Comet Lab\n");
    let o = a.store.find_org("comet-lab").unwrap();
    assert_eq!(o.visibility, Visibility::Private);
    assert!(screen_text(&mut a).contains("Comet Lab"));
    a.on_key(key(KeyCode::Char('v')));
    a.on_key(key(KeyCode::Char('h')));
    let o = a.store.find_org("comet-lab").unwrap();
    assert!(o.visibility == Visibility::Public && o.hosted);

    // roles: the defaults, a custom one, default for applicants
    a.on_key(key(KeyCode::Char('3')));
    assert_eq!(a.orgs.tab, Tab::Roles);
    let s = screen_text(&mut a);
    assert!(s.contains("owner") && s.contains("viewer"), "{s}");
    a.on_key(key(KeyCode::Char('n')));
    typ(&mut a, "observer 20 read call\n");
    let r = a.store.find_role(&o.id, "observer").unwrap();
    assert_eq!((r.rank, r.perms.clone()), (20, vec!["read".to_string(), "call".to_string()]));
    a.orgs.sel = a.orgs.roles.iter().position(|x| x.name == "observer").unwrap();
    a.on_key(key(KeyCode::Char('a')));
    assert_eq!(a.store.find_org("comet-lab").unwrap().apply_role.as_deref(), Some(r.id.as_str()));
    a.on_key(key(KeyCode::Char('n')));
    typ(&mut a, "bad fly\n");
    assert!(a.orgs.msg.as_ref().unwrap().1, "unknown permissions are refused");

    // resources: give a journal, publish to members then everyone, archive
    a.store.create_journal("field notes", None, crate::store::EntryFormat::default()).unwrap();
    a.on_key(key(KeyCode::Char('4')));
    a.on_key(key(KeyCode::Char('a')));
    typ(&mut a, "journal field notes\n");
    assert!(screen_text(&mut a).contains("field notes"));
    a.on_key(key(KeyCode::Char('p')));
    a.on_key(key(KeyCode::Char('p')));
    a.on_key(key(KeyCode::Char('z')));
    let res = a.store.resources(&o.id).unwrap().remove(0);
    assert_eq!((res.published, res.state), (Published::Public, ResourceState::Archived));

    // members: an invitation token without a node
    a.on_key(key(KeyCode::Char('2')));
    a.on_key(key(KeyCode::Char('i')));
    typ(&mut a, "\n");
    settle(&mut a);
    assert!(a.orgs.detail.iter().any(|l| l.starts_with("mnnn-x1-offer:")), "{:?}", a.orgs.detail);
    assert!(a.store.members(&o.id).unwrap().iter().any(|m| m.status == MemberStatus::Invited));

    // the site: served on localhost from the menu, then stopped
    a.prefs.hosting.port = 0;
    a.dispatch(Action::OrgHostStart);
    assert!(a.orgs.host.is_some(), "{:?}", a.orgs.msg);
    assert_eq!(a.orgs.tab, Tab::Site);
    let s = screen_text(&mut a);
    assert!(s.contains("serving here at https://127.0.0.1:") && s.contains("self-signed"), "{s}");
    assert!(crate::org::host::running(&a.paths.home).is_some());
    a.on_key(key(KeyCode::Char('S')));
    assert!(a.orgs.host.is_none());
    assert!(crate::org::host::running(&a.paths.home).is_none());

    // ACME needs a domain and an email
    a.on_key(key(KeyCode::Char('e')));
    typ(&mut a, "lab.example.org\n");
    assert!(a.orgs.msg.as_ref().unwrap().1);
    a.on_key(key(KeyCode::Char('e')));
    typ(&mut a, "lab.example.org me@example.org\n");
    assert!(a.prefs.hosting.acme.enabled);
    a.on_key(key(KeyCode::Char('e')));
    for _ in 0..40 {
        a.on_key(key(KeyCode::Backspace));
    }
    typ(&mut a, "off\n");
    assert!(!a.prefs.hosting.acme.enabled);

    // delete asks first; the journal stays
    a.on_key(key(KeyCode::Char('1')));
    a.on_key(key(KeyCode::Char('d')));
    a.on_key(key(KeyCode::Char('n')));
    assert!(a.store.find_org("comet-lab").is_ok());
    a.on_key(key(KeyCode::Char('d')));
    a.on_key(key(KeyCode::Char('y')));
    assert!(a.store.find_org("comet-lab").is_err());
    assert!(a.store.find_journal("field notes").is_ok());
    let k = kinds(&a);
    for x in ["org.create", "org.role.create", "org.host.start", "org.host.stop", "org.host.acme", "org.remove"] {
        assert!(k.iter().any(|y| y == x), "{x}: {k:?}");
    }
}

#[test]
fn two_windows_browse_apply_approve_and_sync() {
    let (_ta, mut a) = app();
    let (_tb, mut b) = app();
    local_node(&mut a, "anna");
    local_node(&mut b, "ben");
    a.dispatch(Action::NetStart);
    assert!(a.network.node_running(), "{:?}", a.network.msg);
    let addr = {
        let h = a.network.hosted.as_ref().unwrap();
        format!("{}/p2p/{}", h.node.reachable_addrs()[0], h.node.peer_id())
    };
    a.dispatch(Action::OrgNew);
    typ(&mut a, "Comet Lab\n");
    a.on_key(key(KeyCode::Char('v')));

    // B browses A's node and applies
    b.dispatch(Action::OrgJoined);
    b.on_key(key(KeyCode::Char('b')));
    typ(&mut b, &format!("{addr}\n"));
    settle(&mut b);
    assert!(b.orgs.detail.iter().any(|l| l.contains("Comet Lab")), "{:?}", b.orgs.detail);
    b.on_key(key(KeyCode::Char('A')));
    typ(&mut b, &format!("{addr} comet-lab hello there\n"));
    settle(&mut b);
    assert!(screen_text(&mut b).contains("applied"));

    // A approves on the Members page
    let o = a.store.find_org("comet-lab").unwrap();
    let t = Instant::now();
    while !a.store.members(&o.id).unwrap().iter().any(|m| m.status == MemberStatus::Applied) {
        assert!(t.elapsed() < Duration::from_secs(10), "no application");
        std::thread::sleep(Duration::from_millis(50));
    }
    a.dispatch(Action::OrgMembers);
    a.orgs.sel = a.orgs.members.iter().position(|m| m.status == MemberStatus::Applied).unwrap();
    assert!(screen_text(&mut a).contains("hello there"));
    a.on_key(key(KeyCode::Char('a')));
    settle(&mut a);
    assert!(a.store.members(&o.id).unwrap().iter().filter(|m| m.status == MemberStatus::Member).count() == 2);

    // B syncs and sees it
    b.dispatch(Action::OrgJoined);
    b.on_key(key(KeyCode::Char('s')));
    settle(&mut b);
    let j = b.store.find_joined("comet-lab").unwrap();
    assert_eq!((j.status, j.role.as_str()), (MemberStatus::Member, "user"));
    assert!(screen_text(&mut b).contains("member"));
    a.dispatch(Action::NetStop);
}

#[test]
fn org_settings_add_domains_and_set_or_replace_the_certificate() {
    use crate::org::tls;
    let (t, mut a) = app();
    a.on_key(key(KeyCode::Char('G')));
    a.on_key(key(KeyCode::Char('n')));
    typ(&mut a, "Comet Lab\n");
    let o = a.store.find_org("comet-lab").unwrap();
    // w: a domain (refused when it is not a host name)
    a.on_key(key(KeyCode::Char('w')));
    typ(&mut a, "not a domain\n");
    assert!(a.orgs.msg.as_ref().is_some_and(|m| m.1) && a.orgs.ask.is_none());
    a.on_key(key(KeyCode::Char('w')));
    typ(&mut a, "lab.example.org\n");
    assert_eq!(tls::domains(&a.store, &o).unwrap(), vec!["lab.example.org".to_string()]);
    assert!(a.store.find_rule(crate::rules::CERT_WATCH).is_ok(), "the built-in watcher comes with the first domain");
    // c: your certificate from files, then a replacement
    let write = |name: &str, days: u32| {
        let c = crate::org::host::self_signed(&["lab.example.org".into()], days).unwrap();
        let (cp, kp) = (t.path().join(format!("{name}.pem")), t.path().join(format!("{name}.key")));
        std::fs::write(&cp, &c.chain).unwrap();
        std::fs::write(&kp, &*c.key).unwrap();
        format!("{} {}\n", cp.display(), kp.display())
    };
    a.on_key(key(KeyCode::Char('c')));
    typ(&mut a, &write("one", 20));
    assert_eq!(tls::status(&a.store, &o).unwrap().source, tls::USER);
    let first = tls::status(&a.store, &o).unwrap().cert.unwrap().not_after;
    a.on_key(key(KeyCode::Char('c')));
    typ(&mut a, &write("two", 60));
    assert!(tls::status(&a.store, &o).unwrap().cert.unwrap().not_after > first);
    // L: Let's Encrypt for the org; the Site page lists where each certificate comes from
    a.on_key(key(KeyCode::Char('L')));
    typ(&mut a, "on ops@example.org\n");
    assert!(tls::acme(&a.store, &o).unwrap().enabled);
    a.on_key(key(KeyCode::Char('6')));
    let text = screen_text(&mut a);
    assert!(text.contains("lab.example.org") && text.contains("user"), "{text}");
    // c clear: back to ACME
    a.on_key(key(KeyCode::Char('1')));
    a.on_key(key(KeyCode::Char('c')));
    typ(&mut a, "clear\n");
    assert_eq!(tls::status(&a.store, &o).unwrap().source, tls::SELF_SIGNED, "ACME has no certificate yet");
    assert!(kinds(&a).iter().any(|k| k == "org.cert.clear"));
}
