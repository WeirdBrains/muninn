use super::*;
use crate::store::{CodeInput, Design, EntryFormat, Lang};

fn v(s: &str) -> Vec<String> {
    vec![s.to_string()]
}

#[test]
fn targets_take_only_what_they_can_hold() {
    assert_eq!(targets_for(ShareKind::Model), TargetKind::ALL.to_vec());
    assert_eq!(targets_for(ShareKind::Project), [TargetKind::Journal, TargetKind::Net]);
    assert_eq!(targets_for(ShareKind::Report), [TargetKind::Project, TargetKind::Journal, TargetKind::Net]);
    assert_eq!(targets_for(ShareKind::LiveSearch), TargetKind::ALL.to_vec());
    for t in TargetKind::ALL {
        assert_eq!(TargetKind::parse(t.as_str()), Some(t));
    }
}

#[test]
fn anything_shares_to_projects_journals_and_code_spaces() {
    let s = Store::open_in_memory().unwrap();
    let p = s.create_project("Alpha", "").unwrap();
    let j = s.create_journal("Notes", None, EntryFormat::Markdown).unwrap();
    let (m, _) = s.create_model("Signals", "", Design::default(), true).unwrap();
    let c = s.create_code_space("calc", "", Lang::Math, "1+1", CodeInput::default()).unwrap();

    let a = share(&s, ShareKind::Model, "signals", TargetKind::Project, &v("alpha"), "", "cli").unwrap();
    assert!(a.new, "{a:?}");
    assert_eq!(s.project_items(&p.id).unwrap().len(), 1);
    let again = share(&s, ShareKind::Model, &m.id, TargetKind::Project, &v("Alpha"), "", "cli").unwrap();
    assert!(!again.new, "sharing twice keeps one item");
    assert!(again.summary().contains("already"));

    let e = share(&s, ShareKind::Code, "calc", TargetKind::Journal, &v("Notes"), "worth a look", "cli").unwrap();
    let es = s.entries(&j.id).unwrap();
    assert_eq!(es.len(), 1);
    assert_eq!(es[0].id, e.made);
    assert!(es[0].title.contains("calc") && es[0].body.contains("worth a look") && es[0].body.contains(&c.id));

    // a code space takes data to run over; it keeps none of it
    let out = share(&s, ShareKind::Model, "signals", TargetKind::Code, &v("calc"), "", "cli").unwrap();
    assert_eq!(out.made, c.id);
    assert!(s.find_code_space("calc").unwrap().input.is_empty());
    let _ = &m;

    // refusals
    share(&s, ShareKind::Project, "alpha", TargetKind::Project, &v("alpha"), "", "cli").unwrap_err();
    share(&s, ShareKind::Report, "notes", TargetKind::Code, &v("calc"), "", "cli").unwrap_err();
    share(&s, ShareKind::Code, "calc", TargetKind::Code, &v("calc"), "", "cli").unwrap_err();
    share(&s, ShareKind::Journal, "notes", TargetKind::Journal, &v("notes"), "", "cli").unwrap_err();
    share(&s, ShareKind::Model, "signals", TargetKind::Project, &[], "", "cli").unwrap_err();
    share(&s, ShareKind::Model, "nope", TargetKind::Project, &v("alpha"), "", "cli").unwrap_err();
    assert!(s.actions().unwrap().iter().filter(|x| x.kind == "share").count() >= 3);
}

#[test]
fn any_data_is_a_pipe_source_for_a_code_space() {
    use crate::code::pipe::Source;
    for k in [ShareKind::Model, ShareKind::LiveSearch, ShareKind::Journal, ShareKind::Code] {
        assert!(TargetKind::Code.accepts(k), "{k:?}");
    }
    assert!(!TargetKind::Code.accepts(ShareKind::Report) && !TargetKind::Code.accepts(ShareKind::Project));
    assert_eq!(source_for(ShareKind::LiveSearch, "id1", "big").unwrap(), Source::Search("id1".into()));
    assert_eq!(source_for(ShareKind::Journal, "j", "Notes").unwrap(), Source::Journal("j".into()));
    assert_eq!(source_for(ShareKind::Code, "c", "calc").unwrap(), Source::Code("c".into()));
    assert!(matches!(source_for(ShareKind::Model, "m", "signals").unwrap(), Source::Model(i) if i.model == "signals"));
}

#[test]
fn pipelines_share_to_projects_journals_and_net_but_not_code() {
    let s = Store::open_in_memory().unwrap();
    s.add_author("Ada", None, &[]).unwrap();
    s.create_project("Alpha", "").unwrap();
    s.create_journal("Notes", None, crate::store::EntryFormat::Markdown).unwrap();
    let st = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let g = crate::code::pipeline::Graph::parse(&st(&["src=journal:Notes", "v=view:table"]), &st(&["src>v"])).unwrap();
    s.create_pipeline("flow", "", &g).unwrap();
    assert_eq!(targets_for(ShareKind::Pipeline), vec![TargetKind::Project, TargetKind::Journal, TargetKind::Net]);
    assert!(share(&s, ShareKind::Pipeline, "flow", TargetKind::Space, &["x".into()], "", "test").is_err());
    let r = share(&s, ShareKind::Pipeline, "flow", TargetKind::Project, &["Alpha".into()], "", "test").unwrap();
    assert!(r.summary().contains("added pipeline flow to project Alpha"), "{}", r.summary());
    let p = s.find_project("Alpha").unwrap();
    assert!(s.project_items(&p.id).unwrap().iter().any(|i| i.kind == crate::store::ItemKind::Pipeline));
    share(&s, ShareKind::Pipeline, "flow", TargetKind::Journal, &["Notes".into()], "see", "test").unwrap();
    assert!(share(&s, ShareKind::Pipeline, "flow", TargetKind::Code, &["x".into()], "", "test").is_err());
}

#[test]
fn code_spaces_share_everywhere_code_does_and_take_data_like_code() {
    use crate::store::SpaceKind;
    let s = Store::open_in_memory().unwrap();
    s.create_project("Alpha", "").unwrap();
    s.create_journal("Notes", None, EntryFormat::Markdown).unwrap();
    let c = s.create_code_space("calc", "", Lang::Math, "x = 1", CodeInput::default()).unwrap();
    crate::code::spaces::create(&s, "test", "pair", "", SpaceKind::Analysis, &c).unwrap();
    // from: a space goes wherever code goes
    assert_eq!(targets_for(ShareKind::Space), targets_for(ShareKind::Code));
    let r = share(&s, ShareKind::Space, "pair", TargetKind::Project, &v("Alpha"), "", "test").unwrap();
    assert!(r.summary().contains("added code space pair to project Alpha"), "{}", r.summary());
    let p = s.find_project("Alpha").unwrap();
    assert!(s.project_items(&p.id).unwrap().iter().any(|i| i.kind == ItemKind::Space));
    let e = share(&s, ShareKind::Space, "pair", TargetKind::Journal, &v("Notes"), "worth a look", "test").unwrap();
    assert!(e.summary().contains("code space pair"), "{}", e.summary());
    // into: everything that can go into code can go into a space
    for k in ShareKind::ALL {
        assert_eq!(TargetKind::Space.accepts(k), TargetKind::Code.accepts(k), "{k:?}");
    }
    let r = share(&s, ShareKind::Journal, "Notes", TargetKind::Space, &v("pair"), "", "test").unwrap();
    assert_eq!(r.to, "pair");
    assert!(r.summary().contains("into code space pair"), "{}", r.summary());
    assert!(share(&s, ShareKind::Space, "pair", TargetKind::Space, &v("pair"), "", "test").is_err(), "not into itself");
    assert_eq!(source_for(ShareKind::Space, "id", "pair").unwrap(), Source::Space("pair".into()));
    assert_eq!(TargetKind::parse("space"), Some(TargetKind::Space));
    assert_eq!(ShareKind::parse("code-space"), Some(ShareKind::Space));
}

#[test]
fn create_then_share_records_both_and_builds_target() {
    let s = Store::open_in_memory().unwrap();
    let opts = CreateOpts::default();
    let (item, to, shared) =
        create_then_share(&s, ShareKind::Journal, "Lab notes", TargetKind::Project, &[], "", "test", true, Some("Alpha"), &opts, None).unwrap();
    assert_eq!(item.unwrap().name, "Lab notes");
    assert_eq!(to.unwrap().name, "Alpha");
    assert!(shared.new);
    assert_eq!(s.journals().unwrap().len(), 1);
    assert_eq!(s.projects().unwrap().len(), 1);
    assert_eq!(s.project_items(&s.find_project("Alpha").unwrap().id).unwrap().len(), 1);
    let kinds: Vec<_> = s.actions().unwrap().into_iter().map(|a| a.kind).collect();
    assert!(kinds.iter().any(|k| k == "journal.create"), "{kinds:?}");
    assert!(kinds.iter().any(|k| k == "project.create"), "{kinds:?}");
    assert!(kinds.iter().any(|k| k == "share"), "{kinds:?}");
    // create before share
    let ji = kinds.iter().position(|k| k == "journal.create").unwrap();
    let pi = kinds.iter().position(|k| k == "project.create").unwrap();
    let si = kinds.iter().position(|k| k == "share").unwrap();
    assert!(ji < si && pi < si, "{kinds:?}");
}

#[test]
fn create_space_makes_main_when_none_named() {
    let s = Store::open_in_memory().unwrap();
    let c = create_item(
        &s,
        ShareKind::Space,
        "Ingest",
        &CreateOpts { space_kind: Some(crate::store::SpaceKind::Analysis), ..Default::default() },
        "test",
    )
    .unwrap();
    let sp = s.find_space(&c.name).unwrap();
    let main = s.code_space(&sp.main_code_id).unwrap().unwrap();
    assert_eq!(main.name, "Ingest-main");
}

#[test]
fn create_net_target_needs_identity() {
    let s = Store::open_in_memory().unwrap();
    let e = create_target(&s, TargetKind::Net, "x", &CreateOpts::default(), "test", None).unwrap_err();
    assert!(e.to_string().contains("identity"), "{e}");
}

#[test]
fn create_net_exchange_then_share() {
    let s = Store::open_in_memory().unwrap();
    let kp = crate::net::identity::load_or_create(&s).unwrap();
    let relays: Vec<String> = vec![];
    let net = NetCreate { node: None, kp: &kp, my_label: "me", addrs: vec![], relays: &relays };
    s.create_project("Alpha", "").unwrap();
    let (item, to, shared) =
        create_then_share(&s, ShareKind::Project, "Alpha", TargetKind::Net, &[], "", "test", false, Some("lab"), &CreateOpts::default(), Some(&net))
            .unwrap();
    assert!(item.is_none());
    assert_eq!(to.as_ref().unwrap().kind, "net");
    assert!(shared.new || !shared.made.is_empty(), "{shared:?}");
    assert_eq!(s.exchanges().unwrap().len(), 1);
}

#[test]
fn create_review_entry_and_record() {
    let s = Store::open_in_memory().unwrap();
    let e = create_review_subject(&s, crate::store::ReviewKind::Entry, "Monday", &CreateOpts::default(), "test").unwrap();
    assert_eq!(e.kind, "entry");
    assert!(s.find_entry(&e.id).is_ok());
    let r = create_review_subject(&s, crate::store::ReviewKind::Record, "Signals", &CreateOpts::default(), "test").unwrap();
    assert_eq!(r.kind, "record");
    assert!(r.name.to_ascii_lowercase().starts_with("signals/"), "{}", r.name);
}

#[test]
fn create_review_all_kinds() {
    let s = Store::open_in_memory().unwrap();
    for (k, name) in [(crate::store::ReviewKind::Journal, "J1"), (crate::store::ReviewKind::Project, "P1"), (crate::store::ReviewKind::Search, "S1")]
    {
        let c = create_review_subject(&s, k, name, &CreateOpts::default(), "test").unwrap();
        assert_eq!(c.kind, k.as_str(), "{k:?}");
        assert_eq!(c.name, name);
    }
}

#[test]
fn create_result_requires_existing_series() {
    let s = Store::open_in_memory().unwrap();
    let e = create_project_item(&s, crate::store::ItemKind::Result, "binance:BTC/USDT:5m", &CreateOpts::default(), "test").unwrap_err();
    assert!(e.to_string().contains("import"), "{e}");
}

#[test]
fn cancel_create_makes_nothing() {
    // Cancelling is TUI-only; create_item is simply not called. Sanity: empty store stays empty.
    let s = Store::open_in_memory().unwrap();
    assert!(s.journals().unwrap().is_empty());
    assert!(s.projects().unwrap().is_empty());
}
