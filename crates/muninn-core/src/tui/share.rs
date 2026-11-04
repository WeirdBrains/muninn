//! The Share action in the TUI: `S` (or the `[S share]` button) on a window with a selected
//! shareable item opens a picker of every place that takes it.
use super::App;
use super::modal::{Modal, PickKind};
use crate::share::{TargetKind, share, targets_for};
use crate::store::ShareKind;
use ratatui::crossterm::event::KeyCode;

/// The item the open window has selected: kind, id, name.
pub fn item(app: &App) -> Option<(ShareKind, String, String)> {
    let get = |l: Option<(&str, &str)>, k| l.map(|(i, n)| (k, i.to_string(), n.to_string()));
    match app.modal.as_ref()? {
        Modal::Models => match &app.models.model {
            Some(m) => get(Some((&m.id, &m.name)), ShareKind::Model),
            None => get(app.models.list.get(app.models.sel).map(|m| (m.id.as_str(), m.name.as_str())), ShareKind::Model),
        },
        Modal::Code => get(app.code.list.get(app.code.sel).map(|c| (c.id.as_str(), c.name.as_str())), ShareKind::Code),
        Modal::Journals => get(app.journals.list.get(app.journals.sel).map(|j| (j.id.as_str(), j.name.as_str())), ShareKind::Journal),
        Modal::Searches => get(app.searches.list.get(app.searches.sel).map(|(s, _)| (s.id.as_str(), s.name.as_str())), ShareKind::LiveSearch),
        Modal::Reports => get(app.reports.list.get(app.reports.sel).map(|r| (r.id.as_str(), r.name.as_str())), ShareKind::Report),
        Modal::Pipelines => {
            let d = &app.designer;
            let p = match d.screen {
                super::designer::Screen::Edit => d.open.as_ref().and_then(|id| d.list.iter().find(|p| &p.id == id)),
                super::designer::Screen::List => d.list.get(d.sel),
            };
            get(p.map(|p| (p.id.as_str(), p.name.as_str())), ShareKind::Pipeline)
        }
        Modal::Projects => get(app.projects.list.get(app.projects.sel).map(|p| (p.id.as_str(), p.name.as_str())), ShareKind::Project),
        _ => None,
    }
}

/// Put `S share` first in a window's key bar when its selected item can be shared.
pub fn add_key(app: &App, keys: &mut Vec<(&'static str, KeyCode)>) {
    if item(app).is_some() && !keys.iter().any(|(_, k)| *k == KeyCode::Char('S')) {
        keys.insert(0, ("S share", KeyCode::Char('S')));
    }
}

/// Picker rows `target · name` for everywhere `kind` can go (`net · all exchanges` for net).
pub fn choices(app: &App, kind: ShareKind, id: &str) -> Vec<String> {
    let mut v = vec![];
    for t in targets_for(kind) {
        let names: Vec<String> = match t {
            TargetKind::Project => app.store.projects().map(|l| l.into_iter().map(|p| p.name).collect()).unwrap_or_default(),
            TargetKind::Journal => app
                .store
                .journals()
                .map(|l| l.into_iter().filter(|j| !(kind == ShareKind::Journal && j.id == id)).map(|j| j.name).collect())
                .unwrap_or_default(),
            TargetKind::Code => app
                .store
                .code_spaces()
                .map(|l| l.into_iter().filter(|c| !(kind == ShareKind::Code && c.id == id)).map(|c| c.name).collect())
                .unwrap_or_default(),
            TargetKind::Net => vec!["all exchanges".into()],
        };
        v.extend(names.into_iter().map(|n| format!("{} · {n}", t.as_str())));
    }
    v
}

/// Open the share picker for the selected item; false when nothing here is shareable.
pub fn open(app: &mut App) -> bool {
    let Some((kind, id, name)) = item(app) else { return false };
    let items = choices(app, kind, &id);
    app.share_back = app.modal.take().map(Box::new);
    app.share_item = Some((kind, id));
    app.modal = Some(Modal::Pick { title: format!("Share {} {name} to", kind.label()), items, sel: 0, kind: PickKind::Share });
    true
}

/// A picker row was chosen.
pub fn picked(app: &mut App, row: &str) {
    let back = app.share_back.take().map(|b| *b);
    let Some((kind, id)) = app.share_item.take() else { return };
    app.modal = back;
    let Some((t, to)) = row.split_once(" · ") else { return };
    let Some(t) = TargetKind::parse(t) else { return };
    let to: Vec<String> = if t == TargetKind::Net { vec![] } else { vec![to.to_string()] };
    match share(&app.store, kind, &id, t, &to, "", "tui") {
        // data into a code space: open it and run it over the data (the space is not
        // changed; a new or changed source asks to be approved first)
        Ok(s) if t == TargetKind::Code => match crate::share::source_for(kind, &s.item, &s.name) {
            Ok(src) => {
                app.set_status(s.summary());
                super::code::open_space(app, &s.made);
                super::code::run_piped(app, src, &s.summary());
            }
            Err(e) => app.set_status(format!("share: {e}")),
        },
        Ok(s) => app.set_status(s.summary()),
        Err(e) => app.set_status(format!("share: {e}")),
    }
}
