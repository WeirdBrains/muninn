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

#[test]
fn log_file_flag_takes_logs_off_stderr() {
    let e = Env::new();
    let log = e.home().join("custom/out.log");
    let o = e.cmd().env("MUNINN_LOG", "trace").arg("--log-file").arg(&log).args(["keys", "init", "--json"]).output().unwrap();
    let err = String::from_utf8_lossy(&o.stderr);
    for lvl in [" TRACE ", " DEBUG ", " INFO "] {
        assert!(!err.contains(lvl), "log line on stderr: {err}");
    }
    let text = std::fs::read_to_string(&log).expect("log file written");
    assert!(!text.trim().is_empty(), "log file empty");
    assert!(!text.contains('\u{1b}'), "colour codes in log file");
}

#[test]
fn log_file_env_var_works() {
    let e = Env::new();
    let log = e.home().join("env.log");
    let o = e.cmd().env("MUNINN_LOG", "trace").env("MUNINN_LOG_FILE", &log).args(["keys", "init", "--json"]).output().unwrap();
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(!err.contains(" TRACE ") && !err.contains(" DEBUG "), "log line on stderr: {err}");
    assert!(!std::fs::read_to_string(&log).expect("log file written").trim().is_empty());
}
