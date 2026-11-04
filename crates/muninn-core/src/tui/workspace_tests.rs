//! TUI tests for live searches, projects and journals.
use super::action::Action;
use super::journals::JScreen;
use super::modal::Modal;
use super::projects::PScreen;
use super::tests::{app, key, mkey, screen_text};
use super::*;
use crate::store::{AnnotationKind, ItemKind};
use ratatui::crossterm::event::KeyModifiers;

fn typ(a: &mut App, s: &str) {
    for c in s.chars() {
        a.on_key(key(if c == '\n' { KeyCode::Enter } else { KeyCode::Char(c) }));
    }
}

fn kinds(a: &App) -> Vec<String> {
    a.store.actions().unwrap().into_iter().map(|r| r.kind).collect()
}

fn search_eth(a: &mut App) {
    a.on_key(key(KeyCode::Char('/')));
    typ(a, "eth");
    a.on_key(key(KeyCode::Enter));
}

#[test]
fn live_searches_save_apply_fork_refine_and_delete() {
    let (_t, mut a) = app();
    search_eth(&mut a);
    assert_eq!(a.sidebar.rows.len(), 1);
    // B saves the current search
    a.on_key(key(KeyCode::Esc));
    a.on_key(key(KeyCode::Char('B')));
    assert!(matches!(a.modal, Some(Modal::Searches)));
    assert_eq!(a.searches.ask.as_ref().unwrap().value, "eth");
    typ(&mut a, " only\n");
    let s = a.store.live_searches().unwrap();
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].name, "eth only");
    assert_eq!(s[0].spec.query, "eth");
    assert!(screen_text(&mut a).contains("eth only"));
    assert_eq!(a.help_topic(), "live-searches");
    // clear the search, then apply the live search: the sidebar follows it
    a.modal = None;
    a.sidebar.query.clear();
    a.filters_changed();
    assert_eq!(a.sidebar.rows.len(), 2);
    a.on_key(key(KeyCode::Char('F')));
    assert!(matches!(a.modal, Some(Modal::Searches)));
    a.on_key(key(KeyCode::Enter));
    assert!(a.modal.is_none());
    assert_eq!(a.sidebar.rows.len(), 1);
    assert_eq!(a.series[a.sidebar.rows[0]].symbol.to_string(), "ETH/USDT");
    // fork it, then refine the fork with the current view (w)
    a.dispatch(Action::LiveSearches);
    a.on_key(key(KeyCode::Char('f')));
    a.on_key(key(KeyCode::Enter));
    let all = a.store.live_searches().unwrap();
    let fork = all.iter().find(|s| s.name == "eth only (fork)").unwrap().clone();
    assert_eq!(fork.forked_from.as_deref(), Some(s[0].id.as_str()));
    a.modal = None;
    a.sidebar.query = "btc".into();
    a.filters_changed();
    a.dispatch(Action::LiveSearches);
    let i = a.searches.list.iter().position(|(x, _)| x.id == fork.id).unwrap();
    a.searches.sel = i;
    a.on_key(key(KeyCode::Char('w')));
    assert_eq!(a.store.live_search(&fork.id).unwrap().unwrap().spec.query, "btc");
    assert_eq!(a.store.live_search(&s[0].id).unwrap().unwrap().spec.query, "eth");
    // rename, then delete (y confirms)
    a.on_key(key(KeyCode::Char('r')));
    for _ in 0.."eth only (fork)".len() {
        a.on_key(key(KeyCode::Backspace));
    }
    typ(&mut a, "btc\n");
    assert_eq!(a.store.live_search(&fork.id).unwrap().unwrap().name, "btc");
    a.on_key(key(KeyCode::Char('d')));
    a.on_key(key(KeyCode::Char('n')));
    assert_eq!(a.store.live_searches().unwrap().len(), 2);
    a.on_key(key(KeyCode::Char('d')));
    a.on_key(key(KeyCode::Char('y')));
    assert_eq!(a.store.live_searches().unwrap().len(), 1);
    let k = kinds(&a);
    for want in ["search.save", "search.fork", "search.edit", "search.remove"] {
        assert!(k.iter().any(|x| x == want), "{want} in {k:?}");
    }
}

#[test]
fn projects_create_add_open_and_drop() {
    let (_t, mut a) = app();
    // the Projects menu reaches every workspace window
    let titles: Vec<_> = menu::menus().iter().map(|m| m.title).collect();
    assert!(titles.contains(&"Projects"));
    a.on_key(key(KeyCode::Char('P')));
    assert!(matches!(a.modal, Some(Modal::Projects)));
    assert!(screen_text(&mut a).contains("Projects"));
    a.on_key(key(KeyCode::Char('n')));
    typ(&mut a, "Alpha\n");
    let p = a.store.projects().unwrap();
    assert_eq!(p[0].name, "Alpha");
    a.on_key(key(KeyCode::Enter));
    assert_eq!(a.projects.screen, PScreen::Project(p[0].id.clone()));
    // a: add the sidebar series as a result
    a.sidebar.sel = 0;
    a.on_key(key(KeyCode::Char('a')));
    a.on_key(key(KeyCode::Char('a')));
    let items = a.store.project_items(&p[0].id).unwrap();
    assert_eq!(items.len(), 1, "a second add is a no-op");
    assert_eq!(items[0].kind, ItemKind::Result);
    // s: save the current search into the project
    a.sidebar.query = "btc".into();
    a.filters_changed();
    a.on_key(key(KeyCode::Char('s')));
    assert!(matches!(a.modal, Some(Modal::Searches)));
    typ(&mut a, " search\n");
    let items = a.store.project_items(&p[0].id).unwrap();
    assert!(items.iter().any(|i| i.kind == ItemKind::LiveSearch));
    // n: a new journal in the project
    a.dispatch(Action::Projects);
    a.on_key(key(KeyCode::Enter));
    a.on_key(key(KeyCode::Char('n')));
    typ(&mut a, "Notes\n");
    let j = a.store.project_journals(&p[0].id).unwrap();
    assert_eq!(j.len(), 1);
    assert!(screen_text(&mut a).contains("Notes"));
    // Enter on the result opens its series in the focused tile
    let ri = a.projects.items.iter().position(|x| x.0.kind == ItemKind::Result).unwrap();
    a.projects.isel = ri;
    let want = a.projects.items[ri].0.series().unwrap();
    a.on_key(key(KeyCode::Enter));
    assert!(a.modal.is_none());
    assert_eq!(a.layout.focused().series.as_ref().unwrap().symbol.to_string(), want.symbol.display());
    // Enter on the journal opens it
    a.dispatch(Action::Projects);
    a.on_key(key(KeyCode::Enter));
    a.projects.isel = a.projects.items.iter().position(|x| x.0.kind == ItemKind::Journal).unwrap();
    a.on_key(key(KeyCode::Enter));
    assert!(matches!(a.modal, Some(Modal::Journals)));
    assert_eq!(a.journals.screen, JScreen::Journal(j[0].id.clone()));
    // drop the result
    a.dispatch(Action::Projects);
    a.on_key(key(KeyCode::Enter));
    a.projects.isel = a.projects.items.iter().position(|x| x.0.kind == ItemKind::Result).unwrap();
    a.on_key(key(KeyCode::Char('d')));
    assert_eq!(a.store.project_items(&p[0].id).unwrap().len(), 2);
    let k = kinds(&a);
    for want in ["project.create", "project.add", "journal.create", "project.drop"] {
        assert!(k.iter().any(|x| x == want), "{want} in {k:?}");
    }
}

#[test]
fn add_to_project_picker_from_sidebar_and_new_project() {
    let (_t, mut a) = app();
    a.sidebar.focused = true;
    a.sidebar.sel = 0;
    a.on_key(key(KeyCode::Char('a')));
    assert!(matches!(a.modal, Some(Modal::PickProject)));
    assert!(screen_text(&mut a).contains("New project"));
    // the only row is "+ New project…"
    a.on_key(key(KeyCode::Enter));
    typ(&mut a, "Beta\n");
    assert!(a.modal.is_none());
    let p = a.store.find_project("Beta").unwrap();
    let items = a.store.project_items(&p.id).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].kind, ItemKind::Result);
    // via the menu action, now choosing the existing project
    a.dispatch(Action::AddToProject);
    a.on_key(key(KeyCode::Enter));
    assert!(a.status.contains("already"), "{}", a.status);
}

#[test]
fn journals_write_edit_highlight_note_attach_copy_and_fork() {
    let (t, mut a) = app();
    let me = a.store.add_author("Ada", None, &[]).unwrap();
    a.store.set_current_author(Some(&me.id)).unwrap();
    a.on_key(key(KeyCode::Char('J')));
    assert!(matches!(a.modal, Some(Modal::Journals)));
    assert_eq!(a.help_topic(), "journals");
    a.on_key(key(KeyCode::Char('n')));
    typ(&mut a, "Trades\n");
    let j = a.store.find_journal("Trades").unwrap();
    assert_eq!(a.journals.screen, JScreen::Journal(j.id.clone()));
    // w writes an entry: title, Enter to the body, Ctrl-S saves
    a.on_key(key(KeyCode::Char('w')));
    assert!(a.typing());
    typ(&mut a, "Day one\nbought eth at the low");
    a.on_key(mkey(KeyCode::Char('s'), KeyModifiers::CONTROL));
    let e = a.store.entries(&j.id).unwrap();
    assert_eq!(e.len(), 1);
    assert_eq!((e[0].title.as_str(), e[0].body.as_str()), ("Day one", "bought eth at the low"));
    // the key-bar button for saving works too
    a.on_key(key(KeyCode::Char('w')));
    typ(&mut a, "Day two\n$x^2$");
    a.on_key(mkey(KeyCode::Char('t'), KeyModifiers::CONTROL));
    a.on_key(key(KeyCode::F(30)));
    let e = a.store.entries(&j.id).unwrap();
    assert_eq!(e.len(), 2);
    assert_eq!(e[1].format, crate::store::EntryFormat::Latex);
    // read the first entry; select "eth at" and highlight it
    a.journals.esel = 0;
    a.on_key(key(KeyCode::Enter));
    assert!(matches!(a.journals.screen, JScreen::Entry(_)));
    a.on_key(key(KeyCode::Right));
    a.on_key(key(KeyCode::Char('v')));
    a.on_key(key(KeyCode::Right));
    a.on_key(key(KeyCode::Char('m')));
    let anns = a.store.entry_annotations(&e[0].id).unwrap();
    assert_eq!(anns.len(), 1);
    assert_eq!(anns[0].kind, AnnotationKind::Highlight);
    assert_eq!(anns[0].quote.as_deref(), Some("eth at"));
    // a note on one word, and on the entry
    a.on_key(key(KeyCode::End));
    a.on_key(key(KeyCode::Char('v')));
    a.on_key(key(KeyCode::Char('n')));
    typ(&mut a, "really?\n");
    a.on_key(key(KeyCode::Char('n')));
    typ(&mut a, "good day\n");
    let anns = a.store.entry_annotations(&e[0].id).unwrap();
    assert!(anns.iter().any(|x| x.quote.as_deref() == Some("low") && x.body == "really?"));
    assert!(anns.iter().any(|x| x.quote.is_none() && x.body == "good day"));
    // N: a note on the journal
    a.on_key(key(KeyCode::Char('N')));
    typ(&mut a, "summer plan\n");
    assert!(a.store.annotations(&j.id).unwrap().iter().any(|x| x.entry_id.is_none() && x.body == "summer plan"));
    // Enter edits the selected note
    a.journals.asel = a.journals.anns.iter().position(|x| x.body == "good day").unwrap();
    a.on_key(key(KeyCode::Enter));
    typ(&mut a, "!\n");
    assert!(a.store.entry_annotations(&e[0].id).unwrap().iter().any(|x| x.body == "good day!"));
    // i attaches an image
    let img = t.path().join("chart.png");
    std::fs::write(&img, b"\x89PNG fake").unwrap();
    a.on_key(key(KeyCode::Char('i')));
    typ(&mut a, &format!("{}\n", img.display()));
    assert_eq!(a.store.attachments(&e[0].id).unwrap().len(), 1);
    assert!(screen_text(&mut a).contains("chart.png"));
    // x then y deletes a note
    let before = a.store.entry_annotations(&e[0].id).unwrap().len();
    a.journals.asel = 0;
    a.on_key(key(KeyCode::Char('x')));
    a.on_key(key(KeyCode::Char('y')));
    assert_eq!(a.store.entry_annotations(&e[0].id).unwrap().len(), before - 1);
    // back to the journal; mark both entries and copy them into a new journal
    a.on_key(key(KeyCode::Esc));
    assert_eq!(a.journals.screen, JScreen::Journal(j.id.clone()));
    a.journals.esel = 0;
    a.on_key(key(KeyCode::Char(' ')));
    a.on_key(key(KeyCode::Char(' ')));
    a.on_key(key(KeyCode::Char('c')));
    assert!(screen_text(&mut a).contains("New journal"));
    a.on_key(key(KeyCode::Enter));
    typ(&mut a, "Best of\n");
    let best = a.store.find_journal("Best of").unwrap();
    assert_eq!(a.store.entries(&best.id).unwrap().len(), 2);
    // f forks the journal into a new project (picker)
    a.on_key(key(KeyCode::Char('f')));
    assert!(matches!(a.modal, Some(Modal::PickProject)));
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Enter));
    typ(&mut a, "Gamma\n");
    assert!(matches!(a.modal, Some(Modal::Journals)));
    let g = a.store.find_project("Gamma").unwrap();
    let pj = a.store.project_journals(&g.id).unwrap();
    assert_eq!(pj.len(), 1);
    assert_eq!(a.store.entries(&pj[0].id).unwrap().len(), 2);
    let k = kinds(&a);
    for want in ["journal.create", "entry.add", "highlight.add", "note.add", "note.edit", "image.add", "note.remove", "entry.copy", "journal.fork"] {
        assert!(k.iter().any(|x| x == want), "{want} in {k:?}");
    }
    // everything is attributed to the current author
    assert!(a.store.entry_annotations(&e[0].id).unwrap().iter().all(|x| x.author_id.as_deref() == Some(me.id.as_str())));
    assert_eq!(a.store.entries(&best.id).unwrap()[0].author_id.as_deref(), Some(me.id.as_str()));
    assert!(a.store.actions().unwrap().iter().filter(|r| r.kind == "entry.add").all(|r| r.author_id.as_deref() == Some(me.id.as_str())));
}
