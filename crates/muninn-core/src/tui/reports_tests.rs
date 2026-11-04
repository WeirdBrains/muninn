//! TUI tests for reports, document export and printing.
use super::action::Action;
use super::modal::Modal;
use super::reports::RScreen;
use super::tests::{app, key, mkey, screen_text};
use super::*;
use crate::export::files::OutFormat;
use crate::store::{EntryFormat, SectionKind};
use ratatui::crossterm::event::KeyModifiers;

fn typ(a: &mut App, s: &str) {
    for c in s.chars() {
        a.on_key(key(if c == '\n' { KeyCode::Enter } else { KeyCode::Char(c) }));
    }
}

fn kinds(a: &App) -> Vec<String> {
    a.store.actions().unwrap().into_iter().map(|r| r.kind).collect()
}

/// Choose a format in the export window and write to `path`.
fn export_to(a: &mut App, f: OutFormat, path: &std::path::Path) {
    assert!(matches!(a.modal, Some(Modal::DocExport)));
    let i = a.doc_export.formats.iter().position(|x| *x == f).unwrap();
    a.doc_export.sel = i;
    a.on_key(key(KeyCode::Enter));
    let ask = a.doc_export.ask.as_mut().unwrap();
    assert!(ask.value.ends_with(&format!(".{}", f.ext())), "{}", ask.value);
    ask.value = path.display().to_string();
    a.on_key(key(KeyCode::Enter));
    assert!(path.exists(), "{:?}", a.doc_export.msg);
    assert!(!a.doc_export.msg.as_ref().unwrap().1, "{:?}", a.doc_export.msg);
}

#[test]
fn reports_are_built_from_sections() {
    let (t, mut a) = app();
    let ada = a.store.add_author("Ada", None, &[]).unwrap();
    a.store.set_current_author(Some(&ada.id)).unwrap();
    let j = a.store.create_journal("Desk", None, EntryFormat::Markdown).unwrap();
    a.store.add_entry(&j.id, "March", "BTC broke out.", None).unwrap();
    assert!(menu::all_actions().contains(&Action::Reports) && menu::all_actions().contains(&Action::NewReport));
    a.on_key(key(KeyCode::Char('O')));
    assert!(matches!(a.modal, Some(Modal::Reports)));
    assert_eq!(a.help_topic(), "reports");
    assert!(screen_text(&mut a).contains("no reports yet"));
    a.on_key(key(KeyCode::Char('n')));
    typ(&mut a, "Weekly\n");
    let r = a.store.reports().unwrap()[0].clone();
    assert_eq!(r.author_id.as_deref(), Some(ada.id.as_str()));
    assert_eq!(a.reports.screen, RScreen::Report(r.id.clone()));
    // a: text section via the editor
    a.on_key(key(KeyCode::Char('a')));
    assert!(screen_text(&mut a).contains("written text"));
    a.on_key(key(KeyCode::Enter));
    assert_eq!(a.reports.screen, RScreen::Edit);
    typ(&mut a, "Summary\n");
    typ(&mut a, "BTC held the range.");
    a.on_key(mkey(KeyCode::Char('s'), KeyModifiers::CONTROL));
    // a: journal section, chosen from the list
    a.on_key(key(KeyCode::Char('a')));
    a.reports.ksel = SECTION_IDX_JOURNAL;
    a.on_key(key(KeyCode::Enter));
    assert!(matches!(a.reports.screen, RScreen::Choose(_, SectionKind::Journal)));
    a.on_key(key(KeyCode::Enter));
    // a: authors; records need a sidebar series (one is selected)
    a.on_key(key(KeyCode::Char('a')));
    a.reports.ksel = 5;
    a.on_key(key(KeyCode::Enter));
    a.on_key(key(KeyCode::Char('a')));
    a.reports.ksel = 1;
    a.on_key(key(KeyCode::Enter));
    let secs = a.store.sections(&r.id).unwrap();
    let ks: Vec<_> = secs.iter().map(|s| s.kind).collect();
    assert_eq!(ks, [SectionKind::Text, SectionKind::Journal, SectionKind::Authors, SectionKind::Records], "{:?}", a.reports.msg);
    assert_eq!(secs[0].title, "Summary");
    let s = screen_text(&mut a);
    assert!(s.contains("journal Desk · every entry") && s.contains("authors of the report"), "{s}");
    // K moves the records section up; t retitles; Enter on records asks for rows
    assert_eq!(a.reports.ssel, 3);
    a.on_key(key(KeyCode::Char('K')));
    assert_eq!(a.store.sections(&r.id).unwrap()[2].kind, SectionKind::Records);
    a.on_key(key(KeyCode::Enter));
    a.reports.ask.as_mut().unwrap().value = "4".into();
    a.on_key(key(KeyCode::Enter));
    assert_eq!(a.store.sections(&r.id).unwrap()[2].params.rows, Some(4));
    a.on_key(key(KeyCode::Char('t')));
    typ(&mut a, "Bars\n");
    assert_eq!(a.store.sections(&r.id).unwrap()[2].title, "Bars");
    // d + y removes the authors section
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Char('d')));
    a.on_key(key(KeyCode::Char('y')));
    assert_eq!(a.store.sections(&r.id).unwrap().len(), 3);
    // v previews as text
    a.on_key(key(KeyCode::Char('v')));
    let s = screen_text(&mut a);
    assert!(s.contains("Preview Weekly") && s.contains("BTC held the range") && s.contains("March"), "{s}");
    a.on_key(key(KeyCode::Esc));
    assert_eq!(a.reports.screen, RScreen::Report(r.id.clone()));
    // E exports; Esc comes back to the report
    a.on_key(key(KeyCode::Char('E')));
    assert_eq!(a.help_topic(), "export-formats");
    assert!(!a.doc_export.formats.contains(&OutFormat::Archive));
    for f in [OutFormat::Pdf, OutFormat::Docx, OutFormat::Html, OutFormat::Csv] {
        export_to(&mut a, f, &t.path().join(format!("weekly.{}", f.ext())));
    }
    assert!(std::fs::read(t.path().join("weekly.pdf")).unwrap().starts_with(b"%PDF-"));
    a.on_key(key(KeyCode::Esc));
    assert!(matches!(a.modal, Some(Modal::Reports)));
    // p adds it to a project (new one from the picker)
    a.on_key(key(KeyCode::Char('p')));
    assert!(matches!(a.modal, Some(Modal::PickProject)));
    a.on_key(key(KeyCode::Char('n')));
    typ(&mut a, "Breakouts\n");
    assert!(matches!(a.modal, Some(Modal::Reports)));
    let p = a.store.projects().unwrap()[0].clone();
    assert!(a.store.project_items(&p.id).unwrap().iter().any(|i| i.ref_id == r.id));
    // back to the list; r renames, D deletes
    a.on_key(key(KeyCode::Esc));
    a.on_key(key(KeyCode::Char('r')));
    a.reports.ask.as_mut().unwrap().value = "Week 12".into();
    a.on_key(key(KeyCode::Enter));
    assert_eq!(a.store.report(&r.id).unwrap().unwrap().name, "Week 12");
    a.on_key(key(KeyCode::Char('d')));
    a.on_key(key(KeyCode::Char('n')));
    assert_eq!(a.store.reports().unwrap().len(), 1, "n cancels");
    a.on_key(key(KeyCode::Char('d')));
    a.on_key(key(KeyCode::Char('y')));
    assert!(a.store.reports().unwrap().is_empty());
    let k = kinds(&a);
    for want in [
        "report.create",
        "report.section.add",
        "report.section.move",
        "report.section.edit",
        "report.section.remove",
        "report.export",
        "report.edit",
        "report.remove",
        "project.add",
    ] {
        assert!(k.iter().any(|x| x == want), "{want} missing: {k:?}");
    }
    let hist = a.store.actions().unwrap();
    assert!(hist.iter().filter(|h| h.kind.starts_with("report.")).all(|h| h.author_id.as_deref() == Some(ada.id.as_str())));
}

const SECTION_IDX_JOURNAL: usize = 3;

#[test]
fn journals_export_and_print_and_projects_export() {
    let (t, mut a) = app();
    let j = a.store.create_journal("Desk", None, EntryFormat::Markdown).unwrap();
    let e1 = a.store.add_entry(&j.id, "March", "BTC broke out.", None).unwrap();
    a.store.add_entry(&j.id, "April", "It held.", None).unwrap();
    super::journals::open_journal(&mut a, &j.id);
    // E on the journal: the whole journal; no CSV for journals
    a.on_key(key(KeyCode::Char('E')));
    assert!(!a.doc_export.formats.contains(&OutFormat::Csv));
    export_to(&mut a, OutFormat::Text, &t.path().join("desk.txt"));
    let text = std::fs::read_to_string(t.path().join("desk.txt")).unwrap();
    assert!(text.contains("March") && text.contains("April"), "{text}");
    a.on_key(key(KeyCode::Esc));
    assert!(matches!(a.modal, Some(Modal::Journals)));
    // P prints (saved only in tests) into prints/
    a.on_key(key(KeyCode::Char('P')));
    let msg = a.journals.msg.clone().unwrap();
    assert!(!msg.1 && msg.0.contains("prints"), "{msg:?}");
    assert_eq!(std::fs::read_dir(a.paths.prints_dir()).unwrap().count(), 1);
    // E inside an entry: that entry only
    a.on_key(key(KeyCode::Enter));
    assert_eq!(a.journals.entry.as_ref().map(|e| e.id.clone()), Some(e1.id.clone()));
    a.on_key(key(KeyCode::Char('E')));
    assert!(a.doc_export.title.contains("March"), "{}", a.doc_export.title);
    export_to(&mut a, OutFormat::Text, &t.path().join("march.txt"));
    let text = std::fs::read_to_string(t.path().join("march.txt")).unwrap();
    assert!(text.contains("March") && !text.contains("April"), "{text}");
    a.on_key(key(KeyCode::Esc));
    a.on_key(key(KeyCode::Char('P')));
    assert_eq!(std::fs::read_dir(a.paths.prints_dir()).unwrap().count(), 2);
    let k = kinds(&a);
    for want in ["journal.export", "entry.export", "journal.print"] {
        assert!(k.iter().any(|x| x == want), "{want} missing: {k:?}");
    }
    // Projects: E offers the archive first and it imports back
    let p = a.store.create_project("Breakouts", "").unwrap();
    a.store.add_project_item(&p.id, crate::store::ItemKind::Journal, &j.id, serde_json::json!({})).unwrap();
    a.modal = None;
    a.on_key(key(KeyCode::Char('P')));
    a.on_key(key(KeyCode::Char('E')));
    assert_eq!(a.doc_export.formats[0], OutFormat::Archive);
    assert!(screen_text(&mut a).contains("Export project Breakouts"));
    let arc = t.path().join("b.mnnp");
    export_to(&mut a, OutFormat::Archive, &arc);
    a.on_key(key(KeyCode::Esc));
    assert!(matches!(a.modal, Some(Modal::Projects)));
    let r = crate::export::archive::import_project(&mut a.store, &arc, Some("Copy")).unwrap();
    assert_eq!(r.journals.len(), 1);
    assert_eq!(r.entries, 2);
    assert!(kinds(&a).iter().any(|x| x == "project.export"));
    // I on the list restores an archive next to the original.
    a.on_key(key(KeyCode::Char('I')));
    typ(&mut a, &format!("{}\n", arc.display()));
    let msg = a.projects.msg.clone().unwrap();
    assert!(!msg.1 && msg.0.contains("imported project Breakouts (imported)") && msg.0.contains("1 journal,"), "{msg:?}");
    assert!(a.projects.list.iter().any(|p| p.name == "Breakouts (imported)"));
    assert!(kinds(&a).iter().any(|x| x == "project.import"));
    a.on_key(key(KeyCode::Char('I')));
    typ(&mut a, "/nonexistent/x.mnnp\n");
    assert!(a.projects.msg.as_ref().unwrap().1);
}
