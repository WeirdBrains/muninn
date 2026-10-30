//! theme and help commands.
mod common;
use common::*;

#[test]
fn theme_commands() {
    let e = Env::bare();
    let out = e.ok(&["theme", "list"]);
    for n in ["tokyo-night", "catppuccin", "gruvbox", "dracula", "nord", "one", "solarized"] {
        assert!(out.contains(n), "{out}");
    }
    assert!(out.contains("* tokyo-night"));
    assert!(e.home().join("themes/dracula/interface.toml").exists());
    assert!(e.home().join("themes/dracula/assets").is_dir());
    let show = e.ok(&["--theme-mode", "light", "theme", "show"]);
    assert!(show.contains("mode     light -> light"), "{show}");
    e.ok(&["theme", "set", "nord"]);
    e.ok(&["theme", "mode", "dark"]);
    let prefs = std::fs::read_to_string(e.home().join("preferences.toml")).unwrap();
    assert!(prefs.contains("name = \"nord\"") && prefs.contains("mode = \"dark\""), "{prefs}");
    assert!(!e.run(&["theme", "set", "nope"]).status.success());
    // export -> rename -> import
    let f = e.dir.path().join("t.toml");
    e.ok(&["theme", "export", "gruvbox", "--out", f.to_str().unwrap()]);
    let s = std::fs::read_to_string(&f).unwrap().replace("name = \"gruvbox\"", "name = \"gruv2\"");
    std::fs::write(&f, s).unwrap();
    e.ok(&["theme", "import", f.to_str().unwrap()]);
    assert!(e.ok(&["theme", "list"]).contains("gruv2"));
    assert!(!e.run(&["theme", "import", f.to_str().unwrap()]).status.success()); // needs -y
    // a theme without [light] is rejected
    let s = std::fs::read_to_string(&f).unwrap();
    std::fs::write(&f, &s[..s.find("[light]").unwrap()]).unwrap();
    let o = e.run(&["-y", "theme", "import", f.to_str().unwrap()]);
    assert!(String::from_utf8_lossy(&o.stderr).contains("[dark] and [light]"));
}

#[test]
fn help_command() {
    let e = Env::bare();
    let out = e.ok(&["help"]);
    assert!(out.contains("services/redis"));
    let out = e.ok(&["help", "keys"]);
    assert!(out.starts_with("# Keys"));
    let out = e.ok(&["help", "--search", "shard"]);
    assert!(out.lines().next().unwrap().starts_with("dark-shards"));
    assert!(out.lines().take(2).any(|l| l.starts_with("keys")));
    assert!(!e.run(&["help", "nope"]).status.success());
    assert!(e.ok(&["help", "install"]).starts_with("# Install"));
    assert!(e.ok(&["--help"]).contains("mnnn help install"));
    // archived snapshots are not help topics
    assert!(!e.run(&["help", "historical/1.0/PLAN"]).status.success());
    // help never creates a home
    assert!(!e.home().exists());
}
