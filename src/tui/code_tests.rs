//! TUI tests for the Code spaces window (C).
use super::action::Action;
use super::code::{CAsk, Confirm, Editor, Screen};
use super::modal::Modal;
use super::tests::{app, click, find_on, key, render, screen_text};
use super::*;
use crate::store::ItemKind;

fn typ(a: &mut App, s: &str) {
    for c in s.chars() {
        a.on_key(key(if c == '\n' { KeyCode::Enter } else { KeyCode::Char(c) }));
    }
}

fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

#[test]
fn the_editor_inserts_joins_and_moves() {
    let mut e = Editor::new("ab\ncd\n");
    assert_eq!(e.lines, vec!["ab", "cd"]);
    e.key(key(KeyCode::End), 10);
    e.key(key(KeyCode::Enter), 10);
    e.key(key(KeyCode::Char('é')), 10);
    assert_eq!(e.text(), "ab\né\ncd\n");
    e.key(key(KeyCode::Backspace), 10);
    e.key(key(KeyCode::Backspace), 10);
    assert_eq!(e.text(), "ab\ncd\n");
    e.key(key(KeyCode::Delete), 10);
    assert_eq!(e.text(), "abcd\n");
    // indentation carries over a new line
    let mut e = Editor::new("    x = 1");
    e.key(key(KeyCode::End), 10);
    e.key(key(KeyCode::Enter), 10);
    e.key(key(KeyCode::Char('y')), 10);
    assert_eq!(e.text(), "    x = 1\n    y\n");
    assert!(!e.key(key(KeyCode::F(2)), 10));
}

#[test]
fn code_window_creates_edits_and_approves_before_running() {
    let (_t, mut a) = app();
    a.on_key(key(KeyCode::Char('C')));
    assert!(matches!(a.modal, Some(Modal::Code)));
    assert_eq!(a.help_topic(), "code-spaces");
    assert!(screen_text(&mut a).contains("no code spaces yet"));
    assert!(menu::menus().iter().any(|m| m.items.iter().any(|i| i.label == "Code spaces…" && i.shortcut == "C")));

    // n asks for a name and a language; a bad language stays on that step
    a.on_key(key(KeyCode::Char('n')));
    typ(&mut a, "fast\n");
    assert_eq!(a.code.ask.as_ref().unwrap().value, "math", "math is offered");
    a.code.ask.as_mut().unwrap().value.clear();
    typ(&mut a, "cobol\n");
    assert!(a.code.msg.as_ref().unwrap().1);
    assert_eq!(a.code.ask.as_ref().unwrap().kind, CAsk::Lang);
    for _ in 0.."cobol".len() {
        a.on_key(key(KeyCode::Backspace));
    }
    typ(&mut a, "math\n");
    let c = a.store.find_code_space("fast").unwrap_or_else(|e| panic!("{e}: {:?} {:?}", a.code.msg, a.code.ask));
    assert_eq!(c.source, crate::code::template(crate::store::Lang::Math));
    assert_eq!(a.code.screen, Screen::Edit);
    assert!(a.code.editing, "a new space opens in the editor");
    assert!(a.typing(), "letters are typed, not commands");

    // type over the template: q and C are text here
    a.code.editor = Editor::new("");
    typ(&mut a, "@title Quick\nC = close * 2\n");
    a.on_key(ctrl('s'));
    let c = a.store.find_code_space("fast").unwrap();
    assert_eq!(c.source, "@title Quick\nC = close * 2\n");
    a.on_key(key(KeyCode::Esc));
    assert!(!a.code.editing);
    let text = screen_text(&mut a);
    assert!(text.contains("Code space fast") && text.contains("C = close * 2") && text.contains("not approved"), "{text}");

    // i opens the input picker; "type the whole line" sets it as text (checked)
    a.on_key(key(KeyCode::Char('i')));
    let bars = a.code.picker.as_ref().unwrap().models.iter().position(|m| m == "bars").unwrap();
    a.code.picker.as_mut().unwrap().sel = bars;
    a.on_key(key(KeyCode::Enter));
    a.on_key(key(KeyCode::Up));
    a.on_key(key(KeyCode::Enter));
    assert_eq!(a.code.ask.as_ref().unwrap().kind, CAsk::Input);
    a.code.ask.as_mut().unwrap().value.clear();
    typ(&mut a, "bars wher x\n");
    assert!(a.code.ask.is_some(), "a bad input stays open");
    a.code.ask.as_mut().unwrap().value = "bars where symbol = BTC/USDT; sort timestamp; limit 5".into();
    a.on_key(key(KeyCode::Enter));
    let c = a.store.find_code_space("fast").unwrap();
    assert_eq!(c.input.model, "bars");
    assert_eq!(c.input.limit, Some(5));

    // r asks to approve first; n leaves it unapproved and nothing runs
    a.on_key(key(KeyCode::Char('r')));
    assert_eq!(a.code.confirm, Some(Confirm::ApproveRun));
    assert!(screen_text(&mut a).contains("native code"));
    a.on_key(key(KeyCode::Char('n')));
    assert!(!crate::code::approved(&a.store.find_code_space("fast").unwrap()));

    // p adds it to a project and comes back
    let p = a.store.create_project("Lab", "").unwrap();
    a.on_key(key(KeyCode::Char('p')));
    assert!(matches!(a.modal, Some(Modal::PickProject)));
    a.on_key(key(KeyCode::Enter));
    assert!(matches!(a.modal, Some(Modal::Code)));
    let items = a.store.project_items(&p.id).unwrap();
    assert!(items.iter().any(|i| i.kind == ItemKind::Code && i.ref_id == c.id));

    // an unsaved change asks before leaving
    a.on_key(key(KeyCode::Char('e')));
    typ(&mut a, "x");
    a.on_key(key(KeyCode::Esc));
    a.on_key(key(KeyCode::Esc));
    assert_eq!(a.code.confirm, Some(Confirm::Discard));
    a.on_key(key(KeyCode::Char('y')));
    assert_eq!(a.code.screen, Screen::List);
    assert_eq!(a.store.find_code_space("fast").unwrap().source, "@title Quick\nC = close * 2\n");

    // the project opens the space in the editor
    a.modal = None;
    super::code::open_space(&mut a, &c.id);
    assert_eq!(a.code.screen, Screen::Edit);

    // d deletes after y
    a.on_key(key(KeyCode::Esc));
    a.on_key(key(KeyCode::Char('d')));
    a.on_key(key(KeyCode::Char('y')));
    assert!(a.store.code_spaces().unwrap().is_empty());
    let kinds: Vec<String> = a.store.actions().unwrap().into_iter().map(|r| r.kind).collect();
    for k in ["code.create", "code.edit", "code.remove", "project.add"] {
        assert!(kinds.contains(&k.to_string()), "{k}: {kinds:?}");
    }
    a.dispatch(Action::Code);
    assert!(matches!(a.modal, Some(Modal::Code)));
}

#[test]
fn code_window_builds_on_a_thread_and_shows_the_result() {
    if crate::code::cargo().is_err() {
        eprintln!("skipped: no cargo");
        return;
    }
    let (_t, mut a) = app();
    let c = crate::code::create(
        &a.store,
        "cli",
        "double",
        "",
        crate::store::Lang::Math,
        "@title Doubled\ndouble = close * 2\n",
        crate::code::parse_input("bars where symbol = BTC/USDT; sort timestamp; limit 3").unwrap(),
    )
    .unwrap();
    super::code::open_space(&mut a, &c.id);
    a.on_key(key(KeyCode::Char('r')));
    a.on_key(key(KeyCode::Char('y')));
    assert!(a.code.building.is_some(), "not built yet: builds first, then runs");
    let t = std::time::Instant::now();
    while a.code.building.is_some() && t.elapsed().as_secs() < 900 {
        std::thread::sleep(std::time::Duration::from_millis(200));
        a.tick();
    }
    let info = a.code.result.clone().expect("ran");
    assert_eq!(info.status, "ok", "{:?} {:?}", info, a.code.log);
    let text = screen_text(&mut a);
    assert!(text.contains("Doubled") && text.contains("double"), "{text}");
    let kinds: Vec<String> = a.store.actions().unwrap().into_iter().map(|r| r.kind).collect();
    for k in ["code.approve", "code.build", "code.run"] {
        assert!(kinds.contains(&k.to_string()), "{k}: {kinds:?}");
    }
    // t widens the result tile
    a.on_key(key(KeyCode::Char('t')));
    assert!(a.code.wide);
}

#[test]
fn code_list_shows_scope_filters_it_and_gives_code_to_an_org() {
    let (_t, mut a) = app();
    crate::org::create(&a.store, "cli", "Lab", None, "").unwrap();
    for n in ["mine", "shared"] {
        crate::code::create(&a.store, "cli", n, "", crate::store::Lang::Math, "x = close", Default::default()).unwrap();
    }
    a.on_key(key(KeyCode::Char('C')));
    let txt = screen_text(&mut a);
    assert!(txt.contains("user") && txt.contains("g give") && txt.contains("f scope"), "{txt}");
    // give the selected one (mine sorts first or second: pick shared by name)
    a.code.sel = a.code.list.iter().position(|c| c.name == "shared").unwrap();
    a.on_key(key(KeyCode::Char('g')));
    assert_eq!(a.code.ask.as_ref().unwrap().kind, CAsk::Give);
    typ(&mut a, "nope\n");
    assert!(a.code.msg.as_ref().unwrap().1, "an unknown org is an error");
    a.code.ask.as_mut().unwrap().value.clear();
    typ(&mut a, "Lab\n");
    assert!(!a.code.msg.as_ref().unwrap().1, "{:?}", a.code.msg);
    assert!(screen_text(&mut a).contains("org:Lab"));
    // f cycles all → user → organization → all
    a.on_key(key(KeyCode::Char('f')));
    assert_eq!(a.code.list.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["mine"]);
    a.on_key(key(KeyCode::Char('f')));
    assert_eq!(a.code.list.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["shared"]);
    assert!(screen_text(&mut a).contains("organization only"));
    a.on_key(key(KeyCode::Char('f')));
    assert_eq!(a.code.list.len(), 2);
}

#[test]
fn the_input_picker_builds_the_line_from_a_model_and_its_parts() {
    let (_t, mut a) = app();
    let c = a.store.create_code_space("calc", "", crate::store::Lang::Math, "C = close * 2", Default::default()).unwrap();
    a.on_key(key(KeyCode::Char('C')));
    // the list's key bar has it
    assert!(render(&mut a, 140, 40).contains("[i input]"));
    super::code::open_space(&mut a, &c.id);
    // the [Input] button in the editor's head is the same as i
    let s = render(&mut a, 140, 40);
    let (x, y) = find_on(&s, "[Input]").unwrap_or_else(|| panic!("{s}"));
    a.on_mouse(click(x + 1, y));
    let p = a.code.picker.clone().expect("the picker opens");
    assert!(p.models_stage && p.models[0] == "none" && p.models.contains(&"bars".to_string()), "{p:?}");
    assert!(render(&mut a, 140, 40).contains("pick a model"));
    a.code.picker.as_mut().unwrap().sel = p.models.iter().position(|m| m == "bars").unwrap();
    a.on_key(key(KeyCode::Enter));
    // the parts: where, matching, group, sort, limit
    for (row, v) in [(1, "symbol = BTC/USDT"), (3, "symbol"), (4, "-timestamp"), (5, "50")] {
        a.code.picker.as_mut().unwrap().sel = row;
        a.on_key(key(KeyCode::Enter));
        a.code.ask.as_mut().unwrap().value = v.into();
        a.on_key(key(KeyCode::Enter));
    }
    let line = "bars where symbol = BTC/USDT group symbol sort -timestamp limit 50";
    let s = render(&mut a, 140, 40);
    assert!(s.contains(line), "the line is previewed\n{s}");
    // a bad limit stays open
    a.code.picker.as_mut().unwrap().sel = 5;
    a.on_key(key(KeyCode::Enter));
    a.code.ask.as_mut().unwrap().value = "lots".into();
    a.on_key(key(KeyCode::Enter));
    assert!(a.code.ask.is_some() && a.code.msg.as_ref().unwrap().1);
    a.on_key(key(KeyCode::Esc));
    assert!(a.code.picker.is_some(), "Esc on a part goes back to the picker");
    a.on_key(key(KeyCode::Char('u')));
    assert!(a.code.picker.is_none());
    assert_eq!(a.store.find_code_space("calc").unwrap().input.text(), line);
    // none clears it
    a.on_key(key(KeyCode::Char('i')));
    a.code.picker.as_mut().unwrap().sel = 0;
    a.on_key(key(KeyCode::Enter));
    assert!(a.store.find_code_space("calc").unwrap().input.is_empty());
}

#[test]
fn sharing_data_to_a_code_space_runs_it_over_the_data_without_rebinding_it() {
    let (_t, mut a) = app();
    a.store.create_code_space("calc", "", crate::store::Lang::Math, "C = close * 2", Default::default()).unwrap();
    a.share_item = Some((crate::store::ShareKind::Model, "bars".into()));
    super::share::picked(&mut a, "code · calc");
    assert!(matches!(a.modal, Some(Modal::Code)) && a.code.screen == Screen::Edit);
    // the data is piped in; the space keeps none of it
    assert!(a.store.find_code_space("calc").unwrap().input.is_empty());
    assert!(a.code.piped.as_ref().is_some_and(|p| p.text() == "bars"), "{:?}", a.code.piped);
    assert!(screen_text(&mut a).contains("piped in: bars"));
    // a new source asks first
    assert_eq!(a.code.confirm, Some(Confirm::ApproveRun));
    assert!(a.code.msg.as_ref().unwrap().0.contains("native code"));
    // a code space is not offered to itself
    assert!(!super::share::choices(&a, crate::store::ShareKind::Code, &a.code.space.as_ref().unwrap().id).contains(&"code · calc".to_string()));
}

/// search | code | code, saving on the way (tee) and at the end.
#[test]
fn a_search_pipes_through_two_code_spaces_into_journals() {
    if crate::code::cargo().is_err() {
        eprintln!("skipped: no cargo");
        return;
    }
    let (_t, mut a) = app();
    let j = a.store.create_journal("Notes", None, crate::store::EntryFormat::Markdown).unwrap();
    for (t, b) in [("one", "alpha beta"), ("two", "alpha beta gamma delta")] {
        a.store.add_entry(&j.id, t, b, None).unwrap();
    }
    let out = a.store.create_journal("Out", None, crate::store::EntryFormat::Markdown).unwrap();
    let spec = crate::store::SearchSpec { target: crate::store::SearchTarget::Journals, query: "alpha".into(), ..Default::default() };
    a.store.save_live_search("alphas", &spec, None).unwrap();
    for (n, src) in [("double", "@view table\nw2 = words * 2"), ("plus", "@view table\nw3 = w2 + 1")] {
        let c = a.store.create_code_space(n, "", crate::store::Lang::Math, src, Default::default()).unwrap();
        crate::code::approve(&a.store, "test", &c).unwrap();
    }
    // a journal search into the first space
    a.share_item = Some((crate::store::ShareKind::LiveSearch, "alphas".into()));
    super::share::picked(&mut a, "code · double");
    let r = a.code.result.clone().unwrap_or_else(|| panic!("{:?} {:?}", a.code.msg, a.code.log));
    assert_eq!(r.status, "ok", "{r:?} {:?}", a.code.log);
    // > : save it to Out (a tee) and keep going
    a.on_key(key(KeyCode::Char('>')));
    let Some(Modal::Pick { items, .. }) = &a.modal else { panic!("the pipe picker") };
    assert!(items.contains(&"code · plus".to_string()) && !items.contains(&"code · double".to_string()), "{items:?}");
    a.on_pick(super::modal::PickKind::Pipe, "save to journal · Out");
    assert_eq!(a.store.entries(&out.id).unwrap().len(), 1, "{:?}", a.code.msg);
    assert_eq!(a.code.space.as_ref().unwrap().name, "double", "a save does not end the pipe");
    // > : on into the second space, then save its result
    a.on_key(key(KeyCode::Char('>')));
    a.on_pick(super::modal::PickKind::Pipe, "code · plus");
    assert_eq!(a.code.space.as_ref().unwrap().name, "plus");
    let r = a.code.result.clone().expect("plus ran");
    assert_eq!(r.status, "ok", "{r:?} {:?}", a.code.log);
    a.on_key(key(KeyCode::Char('>')));
    a.on_pick(super::modal::PickKind::Pipe, "save to journal · Out");
    let es = a.store.entries(&out.id).unwrap();
    assert_eq!(es.len(), 2);
    let last = es.iter().find(|e| e.title.contains("plus")).unwrap_or_else(|| panic!("{es:?}"));
    // words 2 and 4 → w2 4 and 8 → w3 5 and 9
    assert!(last.body.contains("w3") && last.body.contains('5') && last.body.contains('9'), "{}", last.body);
    for c in ["double", "plus"] {
        assert!(a.store.find_code_space(c).unwrap().input.is_empty(), "{c} keeps no input");
    }
    let kinds: Vec<String> = a.store.actions().unwrap().into_iter().map(|r| r.kind).collect();
    assert!(kinds.iter().filter(|k| *k == "pipe.run").count() >= 2 && kinds.iter().filter(|k| *k == "code.run").count() >= 2, "{kinds:?}");
}

/// A pipe run elsewhere (the CLI, on the same database) shows up live in Pipes….
#[test]
fn the_pipes_window_follows_pipes_run_elsewhere() {
    let (_t, mut a) = app();
    a.store.create_journal("Feed", None, crate::store::EntryFormat::Markdown).unwrap();
    a.dispatch(Action::Pipes);
    assert!(matches!(a.modal, Some(Modal::Pipes)));
    assert!(screen_text(&mut a).contains("no pipes yet"));
    // as `cat x.csv | mnnn pipe - --to journal:Feed` would
    let env = crate::code::CodeEnv::for_store(&a.store).unwrap();
    let f = crate::code::pipe::read_data("a,b\n1,2\n3,4\n", "stdin").unwrap();
    crate::code::pipe::run(
        &a.store,
        &env,
        "cli",
        &crate::code::pipe::Source::Frame(f),
        &[],
        &[crate::code::pipe::Sink::Journal("Feed".into())],
        &mut |_| {},
    )
    .unwrap();
    a.tick();
    let p = a.pipes.list.first().cloned().expect("seen");
    assert_eq!((p.status.as_str(), p.origin.as_str()), ("done", "cli"));
    assert_eq!(p.hops.len(), 2);
    assert_eq!((p.hops[0].rows, p.hops[0].columns.clone()), (2, vec!["a".to_string(), "b".to_string()]));
    let s = render(&mut a, 140, 40);
    assert!(s.contains("data:stdin → journal:Feed") && s.contains("done"), "{s}");
    // a failing one shows its error
    crate::code::pipe::run(&a.store, &env, "cli", &crate::code::pipe::Source::Search("nope".into()), &[], &[], &mut |_| {}).unwrap_err();
    a.tick();
    assert_eq!(a.pipes.list[0].status, "error");
    assert!(render(&mut a, 140, 40).contains("error"));
}
