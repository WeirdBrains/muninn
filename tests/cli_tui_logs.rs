//! The TUI keeps log lines off the terminal.
mod common;
use common::*;

#[test]
fn tui_logs_go_to_a_file_not_the_terminal() {
    let e = Env::new();
    // stdout is not a terminal here, so the TUI refuses to start, but logging
    // is already routed by then.
    let o = e.cmd().env("MUNINN_LOG", "trace").arg("tui").output().unwrap();
    assert!(!o.status.success());
    let err = String::from_utf8_lossy(&o.stderr);
    for lvl in [" TRACE ", " DEBUG ", " INFO ", " WARN "] {
        assert!(!err.contains(lvl), "log line on the terminal: {err}");
    }
    assert!(e.home().join("logs/tui.log").is_file());
}
