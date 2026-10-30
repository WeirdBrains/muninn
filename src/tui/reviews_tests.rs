//! TUI tests for the Reviews window (V) and menu.
use super::action::Action;
use super::modal::Modal;
use super::reviews::{Tab, parse_item};
use super::tests::{app, key, screen_text};
use super::*;
use crate::store::{ReviewItemKind, ReviewItemState, ReviewStatus, ReviewerStatus, ReviewingStatus};
use std::time::{Duration, Instant};

fn typ(a: &mut App, s: &str) {
    for c in s.chars() {
        a.on_key(key(match c {
            '\n' => KeyCode::Enter,
            '\x08' => KeyCode::Backspace,
            c => KeyCode::Char(c),
        }));
    }
}

fn kinds(a: &App) -> Vec<String> {
    a.store.actions().unwrap().into_iter().map(|r| r.kind).collect()
}

fn local_node(a: &mut App, label: &str) {
    a.prefs.net.listen = vec!["/ip4/127.0.0.1/tcp/0".into()];
    a.prefs.net.mdns = false;
    a.prefs.net.label = label.into();
}

fn settle(a: &mut App) {
    let t = Instant::now();
    while a.reviews.job.is_some() {
        assert!(t.elapsed() < Duration::from_secs(30), "review task did not finish");
        std::thread::sleep(Duration::from_millis(20));
        a.tick();
    }
    assert!(!a.reviews.msg.as_ref().is_some_and(|m| m.1), "{:?}", a.reviews.msg);
}

#[test]
fn prompts_parse_into_thread_items() {
    let a = parse_item(ReviewItemKind::Suggestion, "entry=3f2a field=body @the cat :: the comet");
    assert_eq!((a.target.as_str(), a.field.as_str(), a.at.as_deref()), ("3f2a", "body", Some("the cat")));
    assert_eq!(a.value, Some(serde_json::json!("the comet")));
    let a = parse_item(ReviewItemKind::Comment, "nice work");
    assert_eq!((a.body.as_str(), a.at.as_deref()), ("nice work", None));
    let a = parse_item(ReviewItemKind::Highlight, "nth=2 @hill");
    assert_eq!((a.nth, a.at.as_deref()), (2, Some("hill")));
    // no @ needed: a highlight always names its text
    let a = parse_item(ReviewItemKind::Highlight, "hill :: lovely");
    assert_eq!((a.at.as_deref(), a.body.as_str()), (Some("hill"), "lovely"));
    let a = parse_item(ReviewItemKind::Vote, "up 1a2b");
    assert_eq!((a.value, a.reply_to.as_deref()), (Some(serde_json::json!("up")), Some("1a2b")));
}

#[test]
fn reviews_window_requests_comments_accepts_and_merges() {
    let (_t, mut a) = app();
    let j = a.store.create_journal("field notes", None, crate::store::EntryFormat::default()).unwrap();
    let e = a.store.add_entry(&j.id, "Comet", "the cat over the hill", None).unwrap();
    assert!(menu::all_actions().contains(&Action::Reviews));
    a.on_key(key(KeyCode::Char('V')));
    assert!(matches!(a.modal, Some(Modal::Reviews)));
    assert_eq!(a.help_topic(), "reviews");
    assert!(screen_text(&mut a).contains("no reviews yet"));

    // n asks for a review (the prompt starts with "journal ")
    a.on_key(key(KeyCode::Char('n')));
    typ(&mut a, "field notes\n");
    let r = a.store.find_review("field notes").unwrap();
    assert!(screen_text(&mut a).contains("journal field notes"));
    a.on_key(key(KeyCode::Char('n')));
    typ(&mut a, "\x08\x08\x08\x08\x08\x08\x08galaxy x\n");
    assert!(a.reviews.msg.as_ref().unwrap().1, "{:?}", a.reviews.msg);

    // a link (no node needed for just the token)
    a.on_key(key(KeyCode::Char('i')));
    typ(&mut a, "\n");
    settle(&mut a);
    assert!(a.reviews.detail.iter().any(|l| l.starts_with("mnnn-x1-offer:")), "{:?}", a.reviews.detail);
    assert_eq!(a.store.reviewers(&r.id).unwrap()[0].status, ReviewerStatus::Invited);
    a.on_key(key(KeyCode::Char('p')));
    assert!(screen_text(&mut a).contains("the cat over the hill"));
    a.on_key(key(KeyCode::Esc));

    // the thread: comment, suggest, highlight, vote
    a.on_key(key(KeyCode::Enter));
    assert_eq!(a.reviews.tab, Tab::Thread);
    a.on_key(key(KeyCode::Char('c')));
    typ(&mut a, "a fine start\n");
    a.on_key(key(KeyCode::Char('e')));
    typ(&mut a, &format!("entry={} @the cat :: the comet\n", e.short_id()));
    a.on_key(key(KeyCode::Char('h')));
    typ(&mut a, &format!("entry={} hill :: lovely\n", e.short_id()));
    assert!(!a.reviews.msg.as_ref().unwrap().1, "{:?}", a.reviews.msg);
    a.on_key(key(KeyCode::Char('v')));
    typ(&mut a, "approve\n");
    let items = a.store.review_items(&r.id).unwrap();
    assert_eq!(items.len(), 4, "{:?} {:?}", a.reviews.msg, items.iter().map(|i| i.kind).collect::<Vec<_>>());
    let s = screen_text(&mut a);
    assert!(s.contains("suggest") && s.contains("the comet") && s.contains("1 approve"), "{s}");

    // a on a comment is refused; A accepts every open suggestion; m merges
    a.reviews.sel = 0;
    a.on_key(key(KeyCode::Char('a')));
    assert!(a.reviews.msg.as_ref().unwrap().1);
    a.on_key(key(KeyCode::Char('A')));
    a.on_key(key(KeyCode::Char('m')));
    assert_eq!(a.store.find_entry(&e.id).unwrap().body, "the comet over the hill");
    assert!(a.store.review_items(&r.id).unwrap().iter().any(|i| i.state == ReviewItemState::Merged && i.revision == Some(1)));
    assert!(a.reviews.msg.as_ref().unwrap().0.contains("rev 1"), "{:?}", a.reviews.msg);

    // close, reopen, delete asks first
    a.on_key(key(KeyCode::Char('1')));
    a.on_key(key(KeyCode::Char('o')));
    settle(&mut a);
    assert_eq!(a.store.find_review("field notes").unwrap().status, ReviewStatus::Closed);
    a.on_key(key(KeyCode::Char('o')));
    assert_eq!(a.store.find_review("field notes").unwrap().status, ReviewStatus::Open);
    a.on_key(key(KeyCode::Char('d')));
    a.on_key(key(KeyCode::Char('n')));
    assert!(a.store.find_review("field notes").is_ok());
    a.on_key(key(KeyCode::Char('d')));
    a.on_key(key(KeyCode::Char('y')));
    assert!(a.store.find_review("field notes").is_err());
    let k = kinds(&a);
    for x in [
        "review.request",
        "review.invite",
        "review.comment",
        "review.suggest",
        "review.vote",
        "review.accept",
        "review.merge",
        "entry.edit",
        "review.close",
        "review.remove",
    ] {
        assert!(k.iter().any(|y| y == x), "{x}: {k:?}");
    }
}

#[test]
fn two_windows_review_through_a_link() {
    let (_ta, mut a) = app();
    let (_tb, mut b) = app();
    local_node(&mut a, "anna");
    local_node(&mut b, "ben");
    a.dispatch(Action::NetStart);
    assert!(a.network.node_running(), "{:?}", a.network.msg);
    let j = a.store.create_journal("field notes", None, crate::store::EntryFormat::default()).unwrap();
    let e = a.store.add_entry(&j.id, "Comet", "the cat over the hill", None).unwrap();
    a.dispatch(Action::ReviewNew);
    typ(&mut a, "field notes\n");
    a.on_key(key(KeyCode::Char('i')));
    typ(&mut a, "\n");
    settle(&mut a);
    let token = a.reviews.detail.iter().find(|l| l.starts_with("mnnn-x1-")).cloned().unwrap();

    // B joins on Reviewing, views, suggests
    b.dispatch(Action::ReviewsJoined);
    b.on_key(key(KeyCode::Char('j')));
    typ(&mut b, &format!("{token}\n"));
    settle(&mut b);
    assert_eq!(b.store.reviewing().unwrap()[0].status, ReviewingStatus::Joined);
    assert!(screen_text(&mut b).contains("field notes"));
    b.on_key(key(KeyCode::Enter));
    settle(&mut b);
    assert!(b.reviews.detail.iter().any(|l| l.contains("the cat over the hill")), "{:?}", b.reviews.detail);
    b.on_key(key(KeyCode::Char('e')));
    typ(&mut b, &format!("entry={} @cat :: comet\n", e.short_id()));
    settle(&mut b);
    b.on_key(key(KeyCode::Char('v')));
    typ(&mut b, "approve\n");
    settle(&mut b);

    // A sees them, accepts and merges
    let r = a.store.find_review("field notes").unwrap();
    assert_eq!(a.store.reviewers(&r.id).unwrap()[0].status, ReviewerStatus::Joined);
    a.dispatch(Action::ReviewThread);
    let s = screen_text(&mut a);
    assert!(s.contains("(joined)") && s.contains("comet") && s.contains("1 approve"), "{s}");
    a.on_key(key(KeyCode::Char('A')));
    a.on_key(key(KeyCode::Char('m')));
    assert_eq!(a.store.find_entry(&e.id).unwrap().body, "the comet over the hill");

    // B leaves (asks first)
    b.on_key(key(KeyCode::Char('l')));
    b.on_key(key(KeyCode::Char('y')));
    settle(&mut b);
    assert_eq!(b.store.reviewing().unwrap()[0].status, ReviewingStatus::Left);
    let t = Instant::now();
    while a.store.reviewers(&r.id).unwrap()[0].status != ReviewerStatus::Left {
        assert!(t.elapsed() < Duration::from_secs(10), "A did not hear B leave");
        std::thread::sleep(Duration::from_millis(50));
    }
    a.dispatch(Action::NetStop);
}
