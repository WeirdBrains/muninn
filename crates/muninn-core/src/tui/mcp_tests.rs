//! TUI tests for Net ▸ MCP….
use super::modal::Modal;
use super::tests::{app, key, screen_text};
use super::*;
use crate::store::McpRole;

#[test]
fn mcp_window_servers_tools_and_clients() {
    let (t, mut a) = app();
    a.dispatch(Action::Mcp);
    assert!(matches!(a.modal, Some(Modal::Mcp)));
    assert!(screen_text(&mut a).contains("mnnn mcp add NAME"));
    #[cfg(unix)]
    {
        let cmd = crate::mcp::client::tests::fake_server(t.path());
        let mut sec = crate::store::Secrets::default();
        sec.env.insert("FAKE_KEY".into(), "sk-hidden".into());
        a.store.add_mcp_server("fake", "stdio", &cmd.to_string_lossy(), &[], "", &sec).unwrap();
        a.dispatch(Action::Mcp);
        let s = screen_text(&mut a);
        assert!(s.contains("fake") && s.contains("1 credentials") && !s.contains("sk-hidden"), "{s}");
        a.on_key(key(KeyCode::Enter));
        assert!(screen_text(&mut a).contains("tools of fake"), "{}", screen_text(&mut a));
        a.on_key(key(KeyCode::Char('a')));
        let id = a.store.find_mcp_server("fake").unwrap().id;
        assert!(a.store.mcp_approved(&id, "price").unwrap());
        a.on_key(key(KeyCode::Esc));
        assert!(a.mcp.tools.is_none());
    }
    let (c, _) = a.store.add_mcp_client("bot", McpRole::Read).unwrap();
    a.on_key(key(KeyCode::Char('2')));
    assert!(screen_text(&mut a).contains("bot"));
    a.on_key(key(KeyCode::Char('o')));
    assert_eq!(a.store.find_mcp_client("bot").unwrap().role, McpRole::Write);
    a.on_key(key(KeyCode::Char('r')));
    assert!(a.store.find_mcp_client(&c.id).unwrap().revoked_at.is_some());
    assert!(screen_text(&mut a).contains("revoked"));
    a.on_key(key(KeyCode::Char('d')));
    assert!(a.store.mcp_clients().unwrap().is_empty());
    a.on_key(key(KeyCode::Esc));
    assert!(a.modal.is_none());
}
