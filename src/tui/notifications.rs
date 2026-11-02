//! Notifications (`!`, Projects ▸ Notifications…, or a click on the header bell): what
//! built-in events, rule sets and broadcasts told you. Enter opens what one is about.
use super::workspace::{View, draw_view, muted, row_style};
use super::{App, Hit};
use crate::ingest::time::fmt_ts;
use crate::notify;
use crate::store::Notification;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

#[derive(Debug, Default)]
pub struct NotifState {
    pub sel: usize,
    pub unread_only: bool,
    pub list: Vec<Notification>,
    pub msg: Option<(String, bool)>,
}

/// The header's bell.
pub fn bell(n: usize) -> String {
    format!("🔔 {n} unread (!)")
}

pub fn open(app: &mut App) {
    app.notifications.msg = None;
    reload(app);
    app.modal = Some(super::modal::Modal::Notifications);
}

fn reload(app: &mut App) {
    let st = &mut app.notifications;
    st.list = app.store.notifications(st.unread_only, 500).unwrap_or_default();
    st.sel = st.sel.min(st.list.len().saturating_sub(1));
}

/// Open what a notification links to (marking it read).
pub fn follow(app: &mut App, n: &Notification) {
    let _ = app.store.mark_notification(&n.id, true);
    let Some(link) = n.link.clone() else {
        let mut lines = vec![format!("{} · {} · {}", n.level, n.kind, fmt_ts(n.created_at))];
        lines.extend(n.body.lines().map(String::from));
        app.modal = Some(super::modal::Modal::info(&n.title, lines));
        return;
    };
    let (k, id) = link.split_once(':').unwrap_or((link.as_str(), ""));
    match k {
        "review" => {
            app.reviews.review = Some(id.to_string());
            super::reviews::open(app, super::reviews::Tab::Thread);
        }
        "org" => {
            app.orgs.org = Some(id.to_string());
            super::orgs::open(app, super::orgs::Tab::Members);
        }
        "peer" | "requests" => super::network::open(app, super::network::Tab::Invites),
        "sync" => super::network::open(app, super::network::Tab::Shared),
        "messages" => {
            app.peers.peer = Some(id.to_string());
            super::peers::open(app, super::peers::Tab::Messages);
        }
        "live" => super::peers::open(app, super::peers::Tab::Live),
        "code" => super::code::open(app),
        "journal" => super::journals::open_journal(app, id),
        "project" => super::projects::open_project(app, id),
        "rule" => super::rules::open_rule(app, id),
        "search" => super::searches::open(app),
        "action" => {
            if let Ok(i) = id.parse::<i64>() {
                app.prefs.tui.history_open = true;
                app.refresh_history(true);
                if let Some(p) = (0..app.history.len()).find(|r| app.history.id_at(*r) == Some(i)) {
                    app.history.sel = p;
                }
            }
            app.modal = None;
        }
        _ => {
            app.notifications.msg = Some((format!("nothing to open for {link}"), true));
        }
    }
}

pub fn on_key(app: &mut App, k: KeyEvent) {
    let n = app.notifications.list.len();
    let cur = app.notifications.list.get(app.notifications.sel).cloned();
    match k.code {
        KeyCode::Esc => app.modal = None,
        KeyCode::Up | KeyCode::Char('k') => app.notifications.sel = app.notifications.sel.saturating_sub(1),
        KeyCode::Down | KeyCode::Char('j') => app.notifications.sel = (app.notifications.sel + 1).min(n.saturating_sub(1)),
        KeyCode::Enter => {
            if let Some(c) = cur {
                follow(app, &c);
            }
            return;
        }
        KeyCode::Char('r') => {
            if let Some(c) = cur {
                let _ = app.store.mark_notification(&c.id, c.read_at.is_none());
            }
        }
        KeyCode::Char('R') => {
            let k = app.store.mark_all_notifications().unwrap_or(0);
            app.notifications.msg = Some((format!("marked {k} read"), false));
        }
        KeyCode::Char('u') => app.notifications.unread_only = !app.notifications.unread_only,
        KeyCode::Char('m') => {
            if let Some(c) = cur {
                let r = if notify::BUILTINS.iter().any(|b| b.kind == c.kind) {
                    let on = !notify::muted(&app.store).contains(&c.kind);
                    notify::set_muted(&app.store, &c.kind, on).map(|_| format!("{} {}", if on { "muted" } else { "unmuted" }, c.kind))
                } else {
                    Err(crate::Error::invalid("kind", format!("`{}` is not a built-in kind; change its rule set instead", c.kind)))
                };
                app.notifications.msg = Some(match r {
                    Ok(m) => (m, false),
                    Err(e) => (e.to_string(), true),
                });
            }
        }
        _ => return,
    }
    if matches!(app.modal, Some(super::modal::Modal::Notifications)) {
        reload(app);
    }
}

pub fn on_click(app: &mut App, i: usize) {
    if app.notifications.sel == i {
        on_key(app, KeyEvent::from(KeyCode::Enter));
    } else if i < app.notifications.list.len() {
        app.notifications.sel = i;
    }
}

pub fn draw(app: &mut App, f: &mut Frame, r: Rect, hits: &mut Vec<(Rect, Hit)>) {
    let t = app.theme.clone();
    let st = &app.notifications;
    let unread = app.store.unread_notifications().unwrap_or(0);
    let mut v = View::new(format!("Notifications · {unread} unread{}", if st.unread_only { " · showing unread" } else { "" }));
    if st.list.is_empty() {
        v.head.push(muted(&t, " nothing yet: built-in events, rule sets (then notify …) and broadcasts land here"));
    }
    let mt = Style::new().fg(t.muted);
    for (i, n) in st.list.iter().enumerate() {
        let lvl = match n.level.as_str() {
            "error" => Style::new().fg(t.error),
            "warn" => Style::new().fg(t.accent),
            _ => mt,
        };
        let about = n.link.as_deref().map(|l| format!(" → {}", notify::describe_link(&app.store, l))).unwrap_or_default();
        v.rows.push(Line::from(vec![
            Span::styled(if n.read_at.is_none() { " ● " } else { "   " }, lvl),
            Span::styled(format!("{:<40}", n.title), row_style(&t, i == st.sel)),
            Span::styled(format!(" {} · {}{about}", n.kind, fmt_ts(n.created_at)), mt),
        ]));
    }
    if let Some(c) = st.list.get(st.sel)
        && !c.body.is_empty()
    {
        v.head.push(Line::styled(format!(" {}", c.body.lines().next().unwrap_or_default()), Style::new().fg(t.fg)));
    }
    v.sel = (!v.rows.is_empty()).then_some(st.sel);
    v.msg = st.msg.clone();
    v.keys = vec![
        ("Enter open", KeyCode::Enter),
        ("r read/unread", KeyCode::Char('r')),
        ("R all read", KeyCode::Char('R')),
        ("u unread only", KeyCode::Char('u')),
        ("m mute kind", KeyCode::Char('m')),
        ("Esc", KeyCode::Esc),
    ];
    draw_view(f, r, v, &t, hits);
}
