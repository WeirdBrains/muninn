use super::*;
use crate::store::{Design, EntryFormat, Field, FieldType, ReviewerStatus};
use serde_json::json;

fn store() -> Store {
    Store::open_in_memory().unwrap()
}

fn kinds(s: &Store) -> Vec<String> {
    s.actions().unwrap().into_iter().map(|a| a.kind).collect()
}

fn ask(kind: ReviewItemKind) -> Ask {
    Ask { kind, ..Default::default() }
}

fn reviewer(s: &Store, r: &Review) -> Who {
    let v = s.add_reviewer(&r.id, "12D3peer", "ann", None, ReviewerStatus::Joined).unwrap();
    Who::Reviewer(Box::new(v))
}

#[test]
fn an_entry_is_reviewed_suggestions_accepted_and_merged_into_a_revision() {
    let s = store();
    let j = s.create_journal("Lab book", None, EntryFormat::Markdown).unwrap();
    let e = s.add_entry(&j.id, "Day one", "teh result was teh same", None).unwrap();
    let r = request(&s, "cli", ReviewKind::Entry, &e.id[..8], "Check day one", "", &[]).unwrap();
    assert_eq!(r.allow, PERMS.iter().map(|p| p.to_string()).collect::<Vec<_>>());
    let ann = reviewer(&s, &r);

    let c = add(&s, "network", &r, &ann, &Ask { body: "nice".into(), at: Some("result".into()), ..ask(ReviewItemKind::Comment) }).unwrap();
    assert_eq!((c.field.as_str(), c.start, c.end, c.who.as_str()), ("body", Some(4), Some(10), "ann"));
    let a = add(&s, "network", &r, &ann, &Ask { at: Some("teh".into()), value: Some(json!("the")), ..ask(ReviewItemKind::Suggestion) }).unwrap();
    let b =
        add(&s, "network", &r, &ann, &Ask { at: Some("teh".into()), nth: 2, value: Some(json!("the")), ..ask(ReviewItemKind::Suggestion) }).unwrap();
    let t = add(&s, "cli", &r, &Who::Author, &Ask { field: "title".into(), value: Some(json!("Day 1")), ..ask(ReviewItemKind::Suggestion) }).unwrap();
    assert_eq!(t.before, Some(json!("Day one")));
    add(&s, "network", &r, &ann, &Ask { at: Some("same".into()), value: Some(json!("yellow")), ..ask(ReviewItemKind::Highlight) }).unwrap();
    assert!(add(&s, "network", &r, &ann, &ask(ReviewItemKind::Highlight)).is_err(), "a highlight needs text");
    assert!(add(&s, "network", &r, &ann, &ask(ReviewItemKind::Comment)).is_err(), "empty comment");
    assert!(add(&s, "network", &r, &ann, &Ask { field: "nope".into(), body: "x".into(), ..ask(ReviewItemKind::Comment) }).is_err());

    // one vote each, changed in place; item votes are up or down
    let v1 = add(&s, "network", &r, &ann, &Ask { value: Some(json!("changes")), ..ask(ReviewItemKind::Vote) }).unwrap();
    let v2 = add(&s, "network", &r, &ann, &Ask { value: Some(json!("approve")), ..ask(ReviewItemKind::Vote) }).unwrap();
    assert_eq!(v1.id, v2.id);
    assert!(add(&s, "network", &r, &ann, &Ask { value: Some(json!("meh")), ..ask(ReviewItemKind::Vote) }).is_err());
    assert!(add(&s, "network", &r, &ann, &Ask { value: Some(json!("approve")), reply_to: Some(a.id.clone()), ..ask(ReviewItemKind::Vote) }).is_err());
    add(&s, "network", &r, &ann, &Ask { value: Some(json!("up")), reply_to: Some(a.id[..6].into()), ..ask(ReviewItemKind::Vote) }).unwrap();
    assert_eq!(tally(&s.review_items(&r.id).unwrap()), (1, 0, 0));

    // only suggestions are decided; reject one, then accept the rest
    assert!(decide(&s, "cli", &r, std::slice::from_ref(&c.id), true).is_err());
    decide(&s, "cli", &r, std::slice::from_ref(&t.id), false).unwrap();
    let acc = decide(&s, "cli", &r, &[], true).unwrap();
    assert_eq!(acc.len(), 2, "the open ones: both typo fixes");
    let m = merge(&s, "cli", &r, false).unwrap();
    assert!(m.conflicts.is_empty());
    assert_eq!(m.merged.len(), 2);
    let e2 = s.entry(&e.id).unwrap().unwrap();
    assert_eq!((e2.title.as_str(), e2.body.as_str()), ("Day one", "the result was the same"));
    let revs = revisions(&s, ReviewKind::Entry, &e.id).unwrap();
    assert_eq!(revs.len(), 1);
    assert_eq!(
        (revs[0].rev, revs[0].before["body"].as_str(), revs[0].after["body"].as_str()),
        (1, Some("teh result was teh same"), Some("the result was the same"))
    );
    assert_eq!(s.review_item(&b.id).unwrap().unwrap().state, ReviewItemState::Merged);
    assert_eq!(s.review_item(&b.id).unwrap().unwrap().revision, Some(1));
    assert_eq!(s.review_item(&t.id).unwrap().unwrap().state, ReviewItemState::Rejected);
    // nothing left to merge
    assert!(merge(&s, "cli", &r, false).unwrap().revisions.is_empty());

    let k = kinds(&s);
    for want in [
        "review.request",
        "review.comment",
        "review.suggest",
        "review.highlight",
        "review.vote",
        "review.accept",
        "review.reject",
        "review.merge",
        "entry.edit",
    ] {
        assert!(k.iter().any(|x| x == want), "{want} in {k:?}");
    }
    let r = close_local(&s, "cli", &r).unwrap();
    assert!(add(&s, "cli", &r, &Who::Author, &Ask { body: "late".into(), ..ask(ReviewItemKind::Comment) }).is_err());
    let r = reopen(&s, "cli", &r).unwrap();
    assert_eq!(r.status, ReviewStatus::Open);
}

#[test]
fn stale_suggestions_conflict_unless_forced() {
    let s = store();
    let j = s.create_journal("J", None, EntryFormat::Markdown).unwrap();
    let e = s.add_entry(&j.id, "T", "alpha beta", None).unwrap();
    let r = request(&s, "cli", ReviewKind::Entry, &e.id, "", "", &[]).unwrap();
    let whole = add(&s, "cli", &r, &Who::Author, &Ask { value: Some(json!("gamma")), ..ask(ReviewItemKind::Suggestion) }).unwrap();
    let quoted =
        add(&s, "cli", &r, &Who::Author, &Ask { at: Some("beta".into()), value: Some(json!("delta")), ..ask(ReviewItemKind::Suggestion) }).unwrap();
    s.update_entry(&e.id, None, Some("alpha omega"), None).unwrap();
    decide(&s, "cli", &r, &[], true).unwrap();
    let m = merge(&s, "cli", &r, false).unwrap();
    assert_eq!(m.conflicts.len(), 2, "{m:?}");
    assert!(m.revisions.is_empty());
    assert_eq!(s.review_item(&whole.id).unwrap().unwrap().state, ReviewItemState::Accepted);
    // forcing replaces the whole body; the quote is still gone
    let m = merge(&s, "cli", &r, true).unwrap();
    assert_eq!((m.merged, m.conflicts.len()), (vec![whole.id.clone()], 1));
    assert_eq!(s.entry(&e.id).unwrap().unwrap().body, "gamma");
    assert_eq!(s.review_item(&quoted.id).unwrap().unwrap().state, ReviewItemState::Accepted);
}

#[test]
fn records_journals_projects_and_searches_are_reviewable() {
    let s = store();
    let price = Field::new("price", FieldType::Real);
    let note = Field::new("note", FieldType::Text);
    let (m, _) = s.create_model("Signals", "", Design { fields: vec![price, note], ..Default::default() }, true).unwrap();
    let mut vals = serde_json::Map::new();
    vals.insert("price".into(), json!(3.0));
    vals.insert("note".into(), json!("buy"));
    let rec = s.create_record(&s.find_model("signals").unwrap(), &vals).unwrap();
    assert!(request(&s, "cli", ReviewKind::Record, "signals", "", "", &[]).is_err(), "MODEL/RECORD");
    let r = request(&s, "cli", ReviewKind::Record, &format!("signals/{}", &rec.id[..8]), "", "", &[]).unwrap();
    assert_eq!(r.model_id.as_deref(), Some(m.id.as_str()));
    assert!(add(&s, "cli", &r, &Who::Author, &Ask { value: Some(json!(4)), ..ask(ReviewItemKind::Suggestion) }).is_err(), "records name the field");
    add(&s, "cli", &r, &Who::Author, &Ask { field: "price".into(), value: Some(json!("4.5")), ..ask(ReviewItemKind::Suggestion) }).unwrap();
    decide(&s, "cli", &r, &[], true).unwrap();
    let mm = merge(&s, "cli", &r, false).unwrap();
    assert_eq!(mm.revisions[0].after["price"], json!(4.5));
    assert_eq!(s.find_record(&s.find_model("signals").unwrap(), &rec.id).unwrap().values["price"], json!(4.5));
    assert!(kinds(&s).iter().any(|k| k == "record.update"));
    assert_eq!(revisions(&s, ReviewKind::Record, &format!("signals/{}", rec.id)).unwrap().len(), 1);

    // a journal: its name, and its entries through --entry
    let j = s.create_journal("Lab", None, EntryFormat::Markdown).unwrap();
    let e = s.add_entry(&j.id, "One", "first", None).unwrap();
    let other = s.create_journal("Other", None, EntryFormat::Markdown).unwrap();
    let stray = s.add_entry(&other.id, "X", "y", None).unwrap();
    let r = request(&s, "cli", ReviewKind::Journal, "Lab", "", "", &["comment".into()]).unwrap();
    assert_eq!(r.allow, vec!["view".to_string(), "comment".to_string()]);
    assert!(view(&s, &r).unwrap()["entries"][0]["body"] == json!("first"));
    let ann = reviewer(&s, &r);
    assert!(add(&s, "network", &r, &ann, &Ask { value: Some(json!("Lab 2")), ..ask(ReviewItemKind::Suggestion) }).is_err(), "not allowed");
    assert!(add(&s, "cli", &r, &Who::Author, &Ask { target: stray.id.clone(), body: "x".into(), ..ask(ReviewItemKind::Comment) }).is_err());
    add(&s, "cli", &r, &Who::Author, &Ask { value: Some(json!("Lab 2")), ..ask(ReviewItemKind::Suggestion) }).unwrap();
    add(&s, "cli", &r, &Who::Author, &Ask { target: e.id[..8].into(), value: Some(json!("First!")), ..ask(ReviewItemKind::Suggestion) }).unwrap();
    decide(&s, "cli", &r, &[], true).unwrap();
    let mm = merge(&s, "cli", &r, false).unwrap();
    assert_eq!(mm.revisions.len(), 2);
    assert_eq!(s.find_journal(&j.id).unwrap().name, "Lab 2");
    assert_eq!(s.entry(&e.id).unwrap().unwrap().body, "First!");
    assert_eq!(s.revisions(ReviewKind::Entry, &e.id).unwrap().len(), 1);

    // a project and a live search
    let p = s.create_project("P", "old").unwrap();
    let r = request(&s, "cli", ReviewKind::Project, "P", "", "", &[]).unwrap();
    add(&s, "cli", &r, &Who::Author, &Ask { value: Some(json!("new")), ..ask(ReviewItemKind::Suggestion) }).unwrap();
    decide(&s, "cli", &r, &[], true).unwrap();
    merge(&s, "cli", &r, false).unwrap();
    assert_eq!(s.find_project(&p.id).unwrap().description, "new");
    let l = s.save_live_search("btc", &crate::store::SearchSpec { query: "btc".into(), ..Default::default() }, None).unwrap();
    let r = request(&s, "cli", ReviewKind::Search, "btc", "", "", &[]).unwrap();
    add(&s, "cli", &r, &Who::Author, &Ask { value: Some(json!("eth")), ..ask(ReviewItemKind::Suggestion) }).unwrap();
    add(&s, "cli", &r, &Who::Author, &Ask { field: "desc".into(), value: Some(json!(true)), ..ask(ReviewItemKind::Suggestion) }).unwrap();
    decide(&s, "cli", &r, &[], true).unwrap();
    merge(&s, "cli", &r, false).unwrap();
    let l = s.find_live_search(&l.id).unwrap();
    assert_eq!((l.spec.query.as_str(), l.spec.desc), ("eth", true));
}

#[test]
fn reviewers_withdraw_only_their_own_and_left_reviewers_are_refused() {
    let s = store();
    let p = s.create_project("P", "d").unwrap();
    let r = request(&s, "cli", ReviewKind::Project, &p.id, "", "", &[]).unwrap();
    let ann = reviewer(&s, &r);
    let mine = add(&s, "cli", &r, &Who::Author, &Ask { body: "hi".into(), ..ask(ReviewItemKind::Comment) }).unwrap();
    let hers = add(&s, "network", &r, &ann, &Ask { body: "yo".into(), reply_to: Some(mine.id.clone()), ..ask(ReviewItemKind::Comment) }).unwrap();
    assert_eq!(hers.reply_to.as_deref(), Some(mine.id.as_str()));
    assert!(withdraw(&s, "network", &r, &ann, &mine.id).is_err());
    assert_eq!(withdraw(&s, "network", &r, &ann, &hers.id).unwrap().state, ReviewItemState::Withdrawn);
    assert!(withdraw(&s, "network", &r, &ann, &hers.id).is_err());
    let Who::Reviewer(v) = &ann else { unreachable!() };
    let v = s.set_reviewer(&v.id, Some(ReviewerStatus::Left), None, None).unwrap();
    assert!(add(&s, "network", &r, &Who::Reviewer(Box::new(v)), &Ask { body: "x".into(), ..ask(ReviewItemKind::Comment) }).is_err());
    assert!(line(&hers).contains("comment: yo"));
    assert!(parse_allow(&["fly".into()]).is_err());
    remove(&s, "cli", &r).unwrap();
    assert!(s.reviews().unwrap().is_empty());
}

fn joined(review: &str, allow: &[&str]) -> crate::store::Reviewing {
    crate::store::Reviewing {
        review_id: review.into(),
        title: "Their signals".into(),
        kind: "record".into(),
        name: "signals".into(),
        node_peer: "12D3author".into(),
        exchange_id: None,
        status: crate::store::ReviewingStatus::Joined,
        allow: allow.iter().map(|a| a.to_string()).collect(),
        info: Value::Null,
        created_at: 1,
        updated_at: 1,
        copy: Value::Null,
        thread: json!([]),
        copied_at: 0,
    }
}

#[test]
fn a_review_copy_makes_and_extends_the_model_and_offline_work_is_kept() {
    // the author's node
    let a = store();
    let (m, _) = a
        .create_model(
            "Signals",
            "",
            Design { fields: vec![Field::new("price", FieldType::Real), Field::new("note", FieldType::Text)], ..Default::default() },
            true,
        )
        .unwrap();
    let rec = a.create_record(&a.find_model(&m.id).unwrap(), &serde_json::from_value(json!({ "price": 1.5, "note": "buy" })).unwrap()).unwrap();
    let r = request(&a, "cli", ReviewKind::Record, &format!("signals/{}", &rec.id[..8]), "", "", &[]).unwrap();
    let v = view(&a, &r).unwrap();
    assert_eq!(v["model"]["name"], "signals");

    // the reviewer's node: the model is made, and only extended later
    let b = store();
    let mut g = joined(&r.id, &["view", "comment"]);
    b.save_reviewing(&g).unwrap();
    let out = net::keep_copy(&b, &mut g, v.clone(), json!([])).unwrap();
    assert!(out.changed && out.model.is_some());
    let bm = b.find_model("signals").unwrap();
    assert_eq!(bm.design.stored().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["price", "note"]);
    assert_eq!(b.find_reviewing(&r.id).unwrap().copy["content"]["note"], "buy");
    let mut v2 = v.clone();
    v2["model"]["fields"].as_array_mut().unwrap().push(serde_json::to_value(Field::new("score", FieldType::Integer)).unwrap());
    let out = net::keep_copy(&b, &mut g, v2, json!([])).unwrap();
    assert!(!out.changed && out.model.unwrap().contains("score"));
    assert!(b.find_model("signals").unwrap().design.field("score").is_some());
    assert!(net::keep_copy(&b, &mut g, v, json!([])).unwrap().model.is_none(), "nothing removed or re-added");
    assert!(kinds(&b).iter().filter(|k| *k == "review.model").count() == 2);

    // no node: comments wait; what is not allowed is refused here
    let c = Ask { body: "why 1.5?".into(), ..ask(ReviewItemKind::Comment) };
    assert!(matches!(net::send_or_queue(&b, None, &g, &c, &[], "cli").unwrap(), net::Sent::Queued(_)));
    assert_eq!(b.review_work(&r.id).unwrap().len(), 1);
    assert!(net::send_or_queue(&b, None, &g, &ask(ReviewItemKind::Vote), &[], "cli").is_err(), "vote not allowed");

    // closed: the copy stays, read-only
    g.status = crate::store::ReviewingStatus::Closed;
    b.save_reviewing(&g).unwrap();
    assert!(net::send_or_queue(&b, None, &g, &c, &[], "cli").unwrap_err().to_string().contains("read-only"));
    assert_eq!(b.find_reviewing(&r.id).unwrap().copy["content"]["price"], 1.5);
    assert!(b.forget_reviewing(&r.id).unwrap());
    assert!(b.review_work(&r.id).unwrap().is_empty());
}
