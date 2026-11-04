//! MCP (Net ▸ MCP…): external MCP servers (their tools and approvals) and the clients
//! allowed to call this node's MCP server. Tokens and credential values are never shown;
//! adding servers and clients is `mnnn mcp add` / `mnnn mcp client add`.
use super::workspace::{View, draw_view, muted, row_style};
use super::{App, Hit};
use crate::ingest::time::fmt_ts;
use crate::store::{McpClient, McpRole, McpServer};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Servers,
    Clients,
}

#[derive(Debug, Default)]
pub struct McpState {
    pub tab: Tab,
    pub sel: usize,
    pub servers: Vec<McpServer>,
    pub clients: Vec<McpClient>,
    /// The selected server's tools (name, description), once asked for.
    pub tools: Option<(String, Vec<(String, String)>)>,
    pub tool_sel: usize,
    pub msg: Option<(String, bool)>,
}

pub fn open(app: &mut App, tab: Tab) {
    app.mcp.tab = tab;
    app.mcp.tools = None;
    app.mcp.msg = None;
    reload(app);
    app.modal = Some(super::modal::Modal::Mcp);
}

fn reload(app: &mut App) {
    let st = &mut app.mcp;
    st.servers = app.store.mcp_servers().unwrap_or_default();
    st.clients = app.store.mcp_clients().unwrap_or_default();
    let n = if st.tab == Tab::Servers { st.servers.len() } else { st.clients.len() };
    st.sel = st.sel.min(n.saturating_sub(1));
}

fn say(app: &mut App, r: crate::Result<String>) {
    app.mcp.msg = Some(match r {
        Ok(m) => (m, false),
        Err(e) => (e.to_string(), true),
    });
}

fn load_tools(app: &mut App) {
    let Some(s) = app.mcp.servers.get(app.mcp.sel).cloned() else { return };
    match crate::mcp::client::Conn::open(&s).and_then(|mut c| c.tools()) {
        Ok(t) => {
            let v = t
                .iter()
                .map(|x| {
                    (
                        x["name"].as_str().unwrap_or_default().to_string(),
                        x["description"].as_str().unwrap_or_default().lines().next().unwrap_or_default().to_string(),
                    )
                })
                .collect();
            app.mcp.tools = Some((s.id.clone(), v));
            app.mcp.tool_sel = 0;
            app.mcp.msg = None;
        }
        Err(e) => app.mcp.msg = Some((e.to_string(), true)),
    }
}

pub fn on_key(app: &mut App, k: KeyEvent) {
    let st = &app.mcp;
    // a server's tools
    if let Some((sid, tools)) = st.tools.clone() {
        match k.code {
            KeyCode::Esc => app.mcp.tools = None,
            KeyCode::Up | KeyCode::Char('k') => app.mcp.tool_sel = app.mcp.tool_sel.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => app.mcp.tool_sel = (app.mcp.tool_sel + 1).min(tools.len().saturating_sub(1)),
            KeyCode::Char('a') | KeyCode::Enter => {
                if let (Some((name, _)), Ok(Some(s))) = (tools.get(app.mcp.tool_sel), app.store.mcp_server(&sid)) {
                    let on = !app.store.mcp_approved(&s.id, name).unwrap_or(false);
                    let r = crate::mcp::client::approve(&app.store, &s, name, "tui", on)
                        .map(|_| if on { format!("approved {}/{name}", s.name) } else { format!("{}/{name} needs approval again", s.name) });
                    say(app, r);
                }
            }
            _ => {}
        }
        return;
    }
    let n = if st.tab == Tab::Servers { st.servers.len() } else { st.clients.len() };
    match (st.tab, k.code) {
        (_, KeyCode::Esc) => {
            app.modal = None;
            return;
        }
        (_, KeyCode::Tab | KeyCode::BackTab | KeyCode::Left | KeyCode::Right) => {
            let t = if app.mcp.tab == Tab::Servers { Tab::Clients } else { Tab::Servers };
            return open(app, t);
        }
        (_, KeyCode::Char('1')) => return open(app, Tab::Servers),
        (_, KeyCode::Char('2')) => return open(app, Tab::Clients),
        (_, KeyCode::Up | KeyCode::Char('k')) => app.mcp.sel = app.mcp.sel.saturating_sub(1),
        (_, KeyCode::Down | KeyCode::Char('j')) => app.mcp.sel = (app.mcp.sel + 1).min(n.saturating_sub(1)),
        (Tab::Servers, KeyCode::Enter) => load_tools(app),
        (Tab::Servers, KeyCode::Char('d')) => {
            if let Some(s) = app.mcp.servers.get(app.mcp.sel).cloned() {
                let r = app.store.remove_mcp_server(&s.id).map(|_| {
                    app.store.note_action(
                        &crate::store::NewAction::new("mcp.server.remove", "tui", format!("remove MCP server {}", s.name))
                            .params(serde_json::json!({ "server": s.id, "name": s.name })),
                    );
                    format!("removed {}", s.name)
                });
                say(app, r);
            }
        }
        (Tab::Clients, KeyCode::Char('r')) => {
            if let Some(c) = app.mcp.clients.get(app.mcp.sel).cloned() {
                let revoke = c.revoked_at.is_none();
                let (kind, w) = if revoke { ("mcp.client.revoke", "revoked") } else { ("mcp.client.restore", "restored") };
                let r = app.store.revoke_mcp_client(&c.id, revoke).map(|_| {
                    app.store.note_action(
                        &crate::store::NewAction::new(kind, "tui", format!("{w} MCP client {}", c.name))
                            .params(serde_json::json!({ "client": c.name })),
                    );
                    format!("{w} {}", c.name)
                });
                say(app, r);
            }
        }
        (Tab::Clients, KeyCode::Char('o')) => {
            if let Some(c) = app.mcp.clients.get(app.mcp.sel).cloned() {
                let all = McpRole::ALL;
                let next = all[(all.iter().position(|r| *r == c.role).unwrap_or(0) + 1) % all.len()];
                let r = app.store.set_mcp_client_role(&c.id, next).map(|_| {
                    app.store.note_action(
                        &crate::store::NewAction::new("mcp.client.role", "tui", format!("MCP client {} is now {}", c.name, next.as_str()))
                            .params(serde_json::json!({ "client": c.name, "role": next.as_str() })),
                    );
                    format!("{} is now {}", c.name, next.as_str())
                });
                say(app, r);
            }
        }
        (Tab::Clients, KeyCode::Char('d')) => {
            if let Some(c) = app.mcp.clients.get(app.mcp.sel).cloned() {
                let r = app.store.remove_mcp_client(&c.id).map(|_| {
                    app.store.note_action(
                        &crate::store::NewAction::new("mcp.client.remove", "tui", format!("remove MCP client {}", c.name))
                            .params(serde_json::json!({ "client": c.name })),
                    );
                    format!("removed {}", c.name)
                });
                say(app, r);
            }
        }
        _ => return,
    }
    reload(app);
}

pub fn on_click(app: &mut App, i: usize) {
    if app.mcp.tools.is_some() {
        app.mcp.tool_sel = i;
        return;
    }
    if app.mcp.sel == i {
        on_key(app, KeyEvent::from(KeyCode::Enter));
    } else {
        app.mcp.sel = i;
    }
}

pub fn draw(app: &mut App, f: &mut Frame, r: Rect, hits: &mut Vec<(Rect, Hit)>) {
    let t = app.theme.clone();
    let st = &app.mcp;
    let mt = Style::new().fg(t.muted);
    let tabs = [(Tab::Servers, "Servers"), (Tab::Clients, "Clients")]
        .iter()
        .enumerate()
        .map(|(i, (x, n))| if *x == st.tab { format!("[{} {n}]", i + 1) } else { format!(" {} {n} ", i + 1) })
        .collect::<Vec<_>>()
        .join(" ");
    let mut v = View::new(format!("MCP · {}", if st.tab == Tab::Servers { "external servers" } else { "clients of this node" }));
    v.head.push(Line::styled(tabs, Style::new().fg(t.accent)));
    if let Some((sid, tools)) = &st.tools {
        let s = st.servers.iter().find(|s| &s.id == sid);
        let approved = app.store.mcp_approvals(sid).unwrap_or_default();
        v.head.push(muted(&t, format!(" tools of {}", s.map(|s| s.name.as_str()).unwrap_or("?"))));
        for (i, (n, d)) in tools.iter().enumerate() {
            let ok = approved.iter().any(|a| a == n);
            v.rows.push(Line::from(vec![
                Span::styled(if ok { " ✓ " } else { "   " }, mt),
                Span::styled(format!("{n:<28}"), row_style(&t, i == st.tool_sel)),
                Span::styled(format!(" {d}"), mt),
            ]));
        }
        if tools.is_empty() {
            v.head.push(muted(&t, " it offers no tools"));
        }
        v.sel = (!v.rows.is_empty()).then_some(st.tool_sel);
        v.keys = vec![("a approve/unapprove", KeyCode::Char('a')), ("Esc back", KeyCode::Esc)];
    } else if st.tab == Tab::Servers {
        if st.servers.is_empty() {
            v.head.push(muted(&t, " none yet: mnnn mcp add NAME --url URL (or -- COMMAND …)"));
        }
        for (i, s) in st.servers.iter().enumerate() {
            let reach = if s.transport == "http" { s.url.clone() } else { s.command.clone() };
            let n = app.store.mcp_approvals(&s.id).map(|a| a.len()).unwrap_or(0);
            v.rows.push(Line::from(vec![
                Span::styled(format!(" {:<20}", s.name), row_style(&t, i == st.sel)),
                Span::styled(format!(" {} · {reach} · {n} approved · {} credentials", s.transport, s.secrets.names().len()), mt),
            ]));
        }
        v.sel = (!v.rows.is_empty()).then_some(st.sel);
        v.keys = vec![("Enter tools", KeyCode::Enter), ("d remove", KeyCode::Char('d')), ("Tab clients", KeyCode::Tab), ("Esc", KeyCode::Esc)];
    } else {
        if st.clients.is_empty() {
            v.head.push(muted(&t, " none yet: mnnn mcp client add NAME [--role read|write|run|full]"));
        }
        for (i, c) in st.clients.iter().enumerate() {
            let state = if c.revoked_at.is_some() {
                "revoked".to_string()
            } else {
                c.last_seen_at.map(|x| format!("seen {}", fmt_ts(x))).unwrap_or_else(|| "never seen".into())
            };
            v.rows.push(Line::from(vec![
                Span::styled(format!(" {:<20}", c.name), row_style(&t, i == st.sel)),
                Span::styled(format!(" {} · {state}", c.role.as_str()), mt),
            ]));
        }
        v.sel = (!v.rows.is_empty()).then_some(st.sel);
        v.keys = vec![
            ("r revoke/restore", KeyCode::Char('r')),
            ("o role", KeyCode::Char('o')),
            ("d remove", KeyCode::Char('d')),
            ("Tab servers", KeyCode::Tab),
            ("Esc", KeyCode::Esc),
        ];
    }
    v.msg = st.msg.clone();
    draw_view(f, r, v, &t, hits);
}
