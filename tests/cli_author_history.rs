//! `mnnn author` and `mnnn history` end to end.
mod common;
use common::*;

fn err(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

#[test]
fn author_crud_and_current() {
    let e = Env::new();
    assert_eq!(e.ok(&["-q", "author", "list"]).trim(), "");
    e.ok(&["author", "add", "--name", "Ada Lovelace", "--email", "ada@example.org", "--detail", "org=Analytical"]);
    e.ok(&["author", "add", "--name", "Grace", "--detail", "team=cobol"]);
    let list = e.ok(&["author", "list"]);
    assert!(list.lines().any(|l| l.starts_with('*') && l.contains("Ada Lovelace")), "{list}");
    assert!(list.lines().any(|l| l.starts_with(' ') && l.contains("Grace")), "{list}");
    let show = e.ok(&["author", "show"]);
    assert!(show.contains("ada@example.org") && show.contains("org: Analytical") && show.contains("(current)"), "{show}");
    e.ok(&["author", "use", "grace"]);
    e.ok(&["author", "edit", "Grace", "--name", "Grace Hopper", "--email", "grace@example.org", "--detail", "team="]);
    let v: serde_json::Value = serde_json::from_str(&e.ok(&["author", "show", "--json"])).unwrap();
    assert_eq!(v["current"], true);
    assert_eq!(v["author"]["name"], "Grace Hopper");
    assert!(v["author"]["details"].as_object().unwrap().is_empty());
    // Removing needs confirmation without a tty.
    let o = e.run(&["author", "remove", "Grace Hopper"]);
    assert!(!o.status.success(), "{}", err(&o));
    e.ok(&["-y", "author", "remove", "Grace Hopper"]);
    let o = e.run(&["author", "show"]);
    assert!(!o.status.success() && err(&o).contains("no current author"), "{}", err(&o));
    let o = e.run(&["author", "show", "nobody"]);
    assert!(!o.status.success());
    // Actions are attributed to the author current when they ran.
    let hist = e.ok(&["history", "list"]);
    assert!(hist.contains("author.add") && hist.contains("Ada Lovelace"), "{hist}");
    assert!(hist.lines().any(|l| l.contains("author.use") && l.contains("Grace")), "{hist}");
}

#[test]
fn imports_exports_and_view_changes_are_recorded_and_forks_kept() {
    let e = Env::new();
    e.ok(&["author", "add", "--name", "Ada"]);
    let f = fixture("BTCUSDT_5m.csv");
    e.ok(&["-y", "ingest", "csv", "--path", f.to_str().unwrap(), "--source", "binance"]);
    let out = e.dir.path().join("out.csv");
    e.ok(&["export", "--format", "csv", "--out", out.to_str().unwrap()]);
    e.ok(&["theme", "set", "nord"]);
    e.ok(&["theme", "mode", "dark"]);
    let list = e.ok(&["history", "list"]);
    let kinds: Vec<&str> = list.lines().map(|l| l.split('\t').nth(2).unwrap()).collect();
    assert_eq!(kinds, ["theme.mode", "theme", "export", "import", "author.add"], "{list}");
    assert!(list.lines().next().unwrap().starts_with('*'), "{list}");
    assert!(list.contains("401 bars") && list.contains("export 401 bars"), "{list}");
    assert!(list.lines().all(|l| l.contains("\tAda\t") || l.contains("author.add")), "{list}");

    // Back two steps restores the theme from before `theme set`.
    e.ok(&["history", "back", "2"]);
    let prefs = std::fs::read_to_string(e.home().join("preferences.toml")).unwrap();
    assert!(prefs.contains("tokyo-night") && !prefs.contains("nord"), "{prefs}");
    // Doing something else forks; both branches are kept.
    e.ok(&["theme", "set", "dracula"]);
    let tree = e.ok(&["history", "tree"]);
    assert!(tree.contains("⑂ from #3"), "{tree}");
    assert!(tree.contains("theme nord") && tree.contains("theme dracula"), "{tree}");
    assert_eq!(e.ok(&["history", "list", "--all", "--json"]).matches("\"kind\"").count(), 6);
    // Goto the old branch's tip brings back nord + dark.
    e.ok(&["history", "goto", "5"]);
    let prefs = std::fs::read_to_string(e.home().join("preferences.toml")).unwrap();
    assert!(prefs.contains("nord") && prefs.contains("dark"), "{prefs}");
    let show = e.ok(&["history", "show", "3"]);
    assert!(show.contains("next:    #4, #6  (forks)"), "{show}");
    let o = e.run(&["history", "forward"]);
    assert!(!o.status.success(), "already at the newest action");
    e.ok(&["history", "goto", "0"]);
    let o = e.run(&["history", "back"]);
    assert!(!o.status.success() && err(&o).contains("start"), "{}", err(&o));
    e.ok(&["history", "forward", "3"]);
    let v: serde_json::Value = serde_json::from_str(&e.ok(&["history", "show", "--json"])).unwrap();
    assert_eq!(v["id"], 3);
}

#[test]
fn source_and_symbol_changes_are_recorded() {
    let e = Env::new();
    e.ok(&["sources", "add", "kraken"]);
    e.ok(&["sources", "rename", "kraken", "krakenx"]);
    e.ok(&["symbols", "add", "BTC/USD", "--alias", "XBT"]);
    e.ok(&["-y", "sources", "remove", "krakenx"]);
    let list = e.ok(&["history", "list"]);
    for k in ["source.add", "source.rename", "symbol.add", "source.remove"] {
        assert!(list.contains(k), "{k} missing: {list}");
    }
    assert!(list.contains("\t-\t"), "no author -> `-`: {list}");
}
