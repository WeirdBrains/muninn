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
        Modal::Spaces => {
            let st = &app.spaces;
            let s = match st.screen {
                super::spaces::Screen::Members => st.open.as_ref(),
                super::spaces::Screen::List => st.list.get(st.sel),
                super::spaces::Screen::Migrate => None,
            };
            get(s.map(|s| (s.id.as_str(), s.name.as_str())), ShareKind::Space)
        }
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
/// Each creatable target starts with `+ New target…`.
pub fn choices(app: &App, kind: ShareKind, id: &str) -> Vec<String> {
    let mut v = vec![];
    for t in targets_for(kind) {
        v.push(format!("{} · + New {}…", t.as_str(), if t == TargetKind::Net { "exchange" } else { t.as_str() }));
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
            TargetKind::Space => app
                .store
                .spaces()
                .map(|l| l.into_iter().filter(|s| !(kind == ShareKind::Space && s.id == id)).map(|s| s.name).collect())
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
    let Some((t, rest)) = row.split_once(" · ") else { return };
    let Some(t) = TargetKind::parse(t) else { return };
    if rest.starts_with("+ New ") {
        // Ask for the new target's name; keep share_item / share_back for after.
        app.share_new_target = Some(t);
        app.modal = Some(super::modal::Modal::prompt(&format!("New {} name", t.as_str()), super::modal::PromptKind::ShareCreateTo));
        return;
    }
    let back = app.share_back.take().map(|b| *b);
    let Some((kind, id)) = app.share_item.take() else { return };
    app.modal = back;
    let to: Vec<String> = if t == TargetKind::Net { vec![] } else { vec![rest.to_string()] };
    finish_share(app, kind, &id, t, &to);
}

fn finish_share(app: &mut App, kind: ShareKind, id: &str, t: TargetKind, to: &[String]) {
    match share(&app.store, kind, id, t, to, "", "tui") {
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
        Ok(s) if t == TargetKind::Space => match crate::share::source_for(kind, &s.item, &s.name) {
            Ok(src) => {
                app.set_status(s.summary());
                super::spaces::run_piped(app, &s.made, src, &s.summary());
            }
            Err(e) => app.set_status(format!("share: {e}")),
        },
        Ok(s) => app.set_status(s.summary()),
        Err(e) => app.set_status(format!("share: {e}")),
    }
}

/// Prompt for a new share target finished (`PromptKind::ShareCreateTo`).
pub fn created_target(app: &mut App, name: &str) {
    let Some(t) = app.share_new_target.take() else { return };
    let back = app.share_back.take().map(|b| *b);
    let Some((kind, id)) = app.share_item.take() else {
        app.modal = back;
        return;
    };
    app.modal = back;
    let name = name.trim();
    if name.is_empty() {
        app.set_status("share: give a name");
        return;
    }
    let made = if t == TargetKind::Net {
        match crate::net::identity::load_or_create(&app.store) {
            Ok(kp) => {
                let relays = app.prefs.net.relays.clone();
                let addrs = crate::net::local::offer_addrs(&app.paths.home, &app.store, None);
                let label = app.prefs.net.label.clone();
                let net = crate::share::NetCreate { node: None, kp: &kp, my_label: &label, addrs, relays: &relays };
                crate::share::create_target(&app.store, t, name, &Default::default(), "tui", Some(&net))
            }
            Err(e) => Err(e),
        }
    } else {
        crate::share::create_target(&app.store, t, name, &Default::default(), "tui", None)
    };
    match made {
        Ok(c) => {
            app.set_status(format!("created {} {}", c.kind, c.name));
            let to = if t == TargetKind::Net { c.id } else { c.name };
            finish_share(app, kind, &id, t, &[to]);
        }
        Err(e) => app.set_status(format!("create: {e}")),
    }
}

/// Where to put a newly created item after the name prompt.
#[derive(Debug, Clone)]
pub enum CreateResume {
    /// Orgs ▸ Resources: add to the current org.
    OrgResource { org_id: String },
    /// Net ▸ Shared: share with every exchange (or these).
    NetShare { with: Vec<String> },
    /// Reviews ▸ Ask: open a review of the new item.
    ReviewAsk,
    /// Code `>` pipe: create target then pipe into it (`kind_label` is the picker prefix).
    PipeAfter { kind_label: String },
}

/// After naming a new item for org resource / net share / review ask.
pub fn created_item(app: &mut App, name: &str) {
    let Some(kind) = app.share_new_item.take() else { return };
    let resume = app.share_create_resume.take();
    let name = name.trim();
    if name.is_empty() {
        app.set_status("create: give a name");
        return;
    }
    match crate::share::create_item(&app.store, kind, name, &Default::default(), "tui") {
        Ok(c) => {
            app.set_status(format!("created {} {}", c.kind, c.name));
            match resume {
                Some(CreateResume::OrgResource { org_id }) => {
                    if let Ok(Some(o)) = app.store.org(&org_id) {
                        match crate::org::transfer(&app.store, "tui", &crate::org::Actor::Owner, &o, kind, &c.name, None) {
                            Ok((r, new)) => {
                                super::orgs::reload(app);
                                app.set_status(format!("{} {} {} {}", if new { "added" } else { "already has" }, kind.label(), r.name, o.name));
                            }
                            Err(e) => app.set_status(format!("resource: {e}")),
                        }
                    }
                }
                Some(CreateResume::NetShare { with }) => match crate::net::client::share(&app.store, kind, &c.name, &with, "tui") {
                    Ok(made) => {
                        let n = made.iter().filter(|(_, n)| *n).count();
                        super::network::reload(app);
                        app.set_status(if n > 0 {
                            format!("shared {} {} with {}", kind.label(), c.name, if with.is_empty() { "everyone".into() } else { with.join(", ") })
                        } else {
                            "already shared".into()
                        });
                    }
                    Err(e) => app.set_status(format!("share: {e}")),
                },
                Some(CreateResume::ReviewAsk) => match kind {
                    ShareKind::Journal | ShareKind::Project | ShareKind::LiveSearch => {
                        let rk = match kind {
                            ShareKind::Journal => crate::store::ReviewKind::Journal,
                            ShareKind::Project => crate::store::ReviewKind::Project,
                            _ => crate::store::ReviewKind::Search,
                        };
                        match crate::review::request(&app.store, "tui", rk, &c.name, "", "", &[]) {
                            Ok(r) => {
                                super::reviews::reload(app);
                                app.set_status(format!("review {} of {} {}", r.short_id(), kind.label(), c.name));
                            }
                            Err(e) => app.set_status(format!("review: {e}")),
                        }
                    }
                    other => app.set_status(format!("create-then-review supports journal, project or search (not {})", other.label())),
                },
                Some(CreateResume::PipeAfter { kind_label }) => {
                    let row = format!("{kind_label} · {}", c.name);
                    super::code::pipe_picked(app, &row);
                }
                None => {}
            }
        }
        Err(e) => app.set_status(format!("create: {e}")),
    }
}
