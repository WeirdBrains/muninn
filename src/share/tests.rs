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

    share(&s, ShareKind::Model, "signals", TargetKind::Code, &v("calc"), "", "cli").unwrap();
    assert_eq!(s.find_code_space("calc").unwrap().input.model, m.name);

    // refusals
    share(&s, ShareKind::Project, "alpha", TargetKind::Project, &v("alpha"), "", "cli").unwrap_err();
    share(&s, ShareKind::Journal, "notes", TargetKind::Code, &v("calc"), "", "cli").unwrap_err();
    share(&s, ShareKind::Journal, "notes", TargetKind::Journal, &v("notes"), "", "cli").unwrap_err();
    share(&s, ShareKind::Model, "signals", TargetKind::Project, &[], "", "cli").unwrap_err();
    share(&s, ShareKind::Model, "nope", TargetKind::Project, &v("alpha"), "", "cli").unwrap_err();
    assert!(s.actions().unwrap().iter().filter(|x| x.kind == "share").count() >= 3);
}
