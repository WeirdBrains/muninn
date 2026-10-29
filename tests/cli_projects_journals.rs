//! `mnnn search`, `mnnn project` and `mnnn journal` end to end.
mod common;
use common::*;

fn err(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

fn ingest(e: &Env, file: &str) {
    let f = fixture(file);
    e.ok(&["-y", "ingest", "csv", "--path", f.to_str().unwrap(), "--source", "binance"]);
}

#[test]
fn live_searches_follow_the_data_and_fork() {
    let e = Env::new();
    e.ok(&["author", "add", "--name", "Ada"]);
    ingest(&e, "BTCUSDT_5m.csv");
    e.ok(&["search", "save", "usdt", "--query", "usdt"]);
    e.ok(&["search", "save", "eth", "--query", "eth", "--source", "binance"]);
    let run = e.ok(&["search", "run", "usdt"]);
    assert_eq!(run.lines().count(), 1, "{run}");
    assert!(run.contains("BTC/USDT"), "{run}");
    assert_eq!(e.ok(&["search", "run", "eth"]).trim(), "");
    // New data shows up without touching the searches.
    ingest(&e, "ETHUSDT_5m.csv");
    assert_eq!(e.ok(&["search", "run", "usdt"]).lines().count(), 2);
    assert!(e.ok(&["search", "run", "eth"]).contains("ETH/USDT"));
    let list = e.ok(&["search", "list"]);
    assert!(list.lines().any(|l| l.contains("usdt\t2 matches")), "{list}");
    // Fork keeps the original; optimize changes in place.
    e.ok(&["search", "fork", "usdt", "--name", "usdt-btc", "--symbol", "btc-usdt"]);
    let v: serde_json::Value = serde_json::from_str(&e.ok(&["search", "run", "usdt-btc", "--json"])).unwrap();
    assert_eq!(v.as_array().unwrap().len(), 1);
    assert_eq!(v[0]["series"]["symbol"], "BTC/USDT");
    let show = e.ok(&["search", "show", "usdt-btc"]);
    assert!(show.contains("fork of: usdt") && show.contains("author:  Ada"), "{show}");
    assert_eq!(e.ok(&["search", "run", "usdt"]).lines().count(), 2, "original untouched");
    e.ok(&["search", "optimize", "usdt", "--year", "1999"]);
    assert_eq!(e.ok(&["search", "run", "usdt"]).trim(), "");
    e.ok(&["search", "edit", "usdt", "--clear", "--query", "usdt", "--name", "all usdt"]);
    assert_eq!(e.ok(&["search", "run", "all usdt"]).lines().count(), 2);
    // Ad hoc searches can be saved too.
    let found = e.ok(&["search", "find", "--query", "eth", "--save", "eth again"]);
    assert!(found.contains("ETH/USDT"), "{found}");
    assert!(e.ok(&["search", "list"]).contains("eth again"));
    let o = e.run(&["search", "save", "bad", "--price", "nope"]);
    assert!(!o.status.success() && err(&o).contains("--price"), "{}", err(&o));
    let o = e.run(&["search", "remove", "eth"]);
    assert!(!o.status.success(), "needs confirmation");
    e.ok(&["-y", "search", "remove", "eth"]);
    assert!(!e.ok(&["search", "list"]).lines().any(|l| l.split('\t').nth(1) == Some("eth")));
    let hist = e.ok(&["history", "list"]);
    for k in ["search.save", "search.fork", "search.edit", "search.remove"] {
        assert!(hist.contains(k), "{k} missing: {hist}");
    }
}

#[test]
fn projects_hold_searches_results_and_journals() {
    let e = Env::new();
    ingest(&e, "BTCUSDT_5m.csv");
    e.ok(&["search", "save", "btc", "--query", "btc"]);
    e.ok(&["project", "create", "Breakouts", "--description", "range breaks"]);
    e.ok(&["project", "add", "Breakouts", "--search", "btc"]);
    assert!(e.ok(&["project", "add", "Breakouts", "--search", "btc"]).contains("already in"));
    e.ok(&["project", "add", "Breakouts", "--result", "binance:btc-usdt:5m", "--from", "2017-08-17 04:00", "--to", "2017-08-17 04:10"]);
    let o = e.run(&["project", "add", "Breakouts", "--result", "binance:DOGE/USDT:5m"]);
    assert!(!o.status.success() && err(&o).contains("no series"), "{}", err(&o));
    let o = e.run(&["project", "add", "Breakouts", "--search", "btc", "--from", "2017-01-01"]);
    assert!(!o.status.success());
    e.ok(&["journal", "new", "Notes", "--project", "Breakouts"]);
    e.ok(&["journal", "new", "Loose"]);
    e.ok(&["project", "add", "Breakouts", "--journal", "Loose"]);
    let show = e.ok(&["project", "show", "Breakouts"]);
    assert!(show.contains("1 matches now"), "{show}");
    assert!(show.contains("binance BTC/USDT 5m") || show.contains("BTC/USDT"), "{show}");
    assert!(show.contains("bars now"), "{show}");
    assert!(show.contains("Notes\t0 entries · project journal") && show.contains("Loose\t0 entries · linked"), "{show}");
    let v: serde_json::Value = serde_json::from_str(&e.ok(&["project", "show", "Breakouts", "--json"])).unwrap();
    let items = v["items"].as_array().unwrap();
    assert_eq!(items.len(), 4, "{v}");
    let result = items.iter().find(|i| i["item"]["kind"] == "result").unwrap();
    assert!(result["now"].as_str().unwrap().starts_with("3 bars"), "{result}");
    // Data arriving later changes what the project shows.
    ingest(&e, "ETHUSDT_5m.csv");
    e.ok(&["search", "edit", "btc", "--query", "usdt"]);
    assert!(e.ok(&["project", "show", "Breakouts"]).contains("2 matches now"));
    let loose = items.iter().find(|i| i["name"] == "Loose").unwrap()["item"]["id"].as_i64().unwrap();
    e.ok(&["project", "drop", "Breakouts", &loose.to_string()]);
    assert!(e.ok(&["journal", "list"]).contains("Loose"), "the journal itself stays");
    e.ok(&["project", "edit", "Breakouts", "--name", "Breaks"]);
    e.ok(&["-y", "project", "remove", "Breaks"]);
    let jl = e.ok(&["journal", "list"]);
    assert!(jl.lines().any(|l| l.contains("Notes") && l.contains("project: -")), "{jl}");
    let hist = e.ok(&["history", "list"]);
    for k in ["project.create", "project.add", "project.drop", "project.edit", "project.remove", "journal.create"] {
        assert!(hist.contains(k), "{k} missing: {hist}");
    }
}

#[test]
fn journals_entries_notes_highlights_images_fork_and_copy() {
    let e = Env::new();
    e.ok(&["author", "add", "--name", "Ada"]);
    e.ok(&["journal", "new", "Desk"]);
    e.ok(&["journal", "write", "Desk", "--title", "March", "--text", "BTC broke out of the range.\\nThen it broke out again."]);
    let tex = e.dir.path().join("e.tex");
    std::fs::write(&tex, "Let $x = 1$.").unwrap();
    e.ok(&["journal", "write", "Desk", "--title", "Math", "--format", "latex", "--file", tex.to_str().unwrap()]);
    let v: serde_json::Value = serde_json::from_str(&e.ok(&["journal", "show", "Desk", "--json"])).unwrap();
    let entries = v["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[1]["entry"]["format"], "latex");
    assert_eq!(entries[0]["author"], "Ada");
    let march = entries[0]["entry"]["id"].as_str().unwrap()[..8].to_string();
    let math = entries[1]["entry"]["id"].as_str().unwrap()[..8].to_string();
    assert!(entries[0]["entry"]["body"].as_str().unwrap().contains('\n'));
    // Notes: global, per entry, on words; highlights on words.
    e.ok(&["journal", "note", "watch the weekly", "--journal", "Desk"]);
    e.ok(&["journal", "note", "good call", "--entry", &march]);
    e.ok(&["journal", "note", "the second one", "--entry", &march, "--on", "broke out", "--nth", "2"]);
    e.ok(&["journal", "highlight", &march, "--on", "the range", "--color", "yellow", "--comment", "4h range"]);
    let o = e.run(&["journal", "highlight", &march, "--on", "not there"]);
    assert!(!o.status.success());
    let o = e.run(&["journal", "note", "x", "--journal", "Desk", "--on", "BTC"]);
    assert!(!o.status.success() && err(&o).contains("--entry"), "{}", err(&o));
    let show = e.ok(&["journal", "show", "Desk"]);
    assert!(show.contains("note: watch the weekly · Ada"), "{show}");
    assert!(show.contains("note on \"broke out\": the second one"), "{show}");
    assert!(show.contains("highlight \"the range\" (yellow): 4h range"), "{show}");
    let v: serde_json::Value = serde_json::from_str(&e.ok(&["journal", "entry", &march, "--json"])).unwrap();
    let ann = v["annotations"].as_array().unwrap();
    let second = ann.iter().find(|a| a["body"] == "the second one").unwrap();
    assert_eq!(second["start"], 36, "{second}");
    // Editing keeps notes on their words.
    e.ok(&["journal", "edit-entry", &march, "--text", "Note: BTC broke out of the range.\\nThen it broke out again."]);
    let v: serde_json::Value = serde_json::from_str(&e.ok(&["journal", "entry", &march, "--json"])).unwrap();
    let hl = v["annotations"].as_array().unwrap().iter().find(|a| a["kind"] == "highlight").unwrap().clone();
    assert_eq!(hl["start"], 23, "{hl}");
    // Images.
    let png = e.dir.path().join("chart.png");
    let bytes = b"\x89PNG\r\n\x1a\nfake image data";
    std::fs::write(&png, bytes).unwrap();
    e.ok(&["journal", "attach", &march, png.to_str().unwrap()]);
    let v: serde_json::Value = serde_json::from_str(&e.ok(&["journal", "entry", &march, "--json"])).unwrap();
    let img = v["attachments"][0]["id"].as_str().unwrap().to_string();
    assert!(v["entry"]["body"].as_str().unwrap().contains(&format!("![chart.png](attachment:{img})")));
    let back = e.dir.path().join("back.png");
    e.ok(&["journal", "image", &img[..8], "--out", back.to_str().unwrap()]);
    assert_eq!(std::fs::read(&back).unwrap(), bytes);
    let o = e.run(&["journal", "attach", &march, tex.to_str().unwrap()]);
    assert!(!o.status.success(), "not an image");
    // Fork into a project journal, copy selected entries to a new journal.
    e.ok(&["project", "create", "P"]);
    e.ok(&["journal", "fork", "Desk", "--project", "P", "--name", "Desk (P)"]);
    let v: serde_json::Value = serde_json::from_str(&e.ok(&["journal", "show", "Desk (P)", "--json"])).unwrap();
    assert_eq!(v["project"], "P");
    assert_eq!(v["entries"].as_array().unwrap().len(), 2);
    assert_eq!(v["notes"].as_array().unwrap().len(), 1);
    assert_eq!(v["entries"][0]["annotations"].as_array().unwrap().len(), 3);
    assert_eq!(v["entries"][0]["attachments"].as_array().unwrap().len(), 1);
    e.ok(&["journal", "copy", &math, "--new", "Maths", "--project", "P"]);
    e.ok(&["journal", "copy", &march, &math, "--to", "Maths"]);
    let jl = e.ok(&["journal", "list", "--project", "P"]);
    assert!(jl.contains("Maths\t3 entries") && jl.contains("Desk (P)\t2 entries"), "{jl}");
    // Journal searches see entry text.
    let found = e.ok(&["search", "find", "--journals", "--query", "broke"]);
    assert_eq!(found.lines().count(), 3, "{found}");
    // Removing notes and entries.
    let note = ann.iter().find(|a| a["body"] == "good call").unwrap()["id"].as_str().unwrap()[..8].to_string();
    e.ok(&["journal", "edit-note", &note, "better call"]);
    assert!(e.ok(&["journal", "entry", &march]).contains("better call"));
    e.ok(&["journal", "remove-note", &note]);
    assert!(!e.ok(&["journal", "entry", &march]).contains("better call"));
    e.ok(&["-y", "journal", "remove-entry", &math]);
    e.ok(&["journal", "edit", "Desk", "--name", "Desk 2", "--format", "latex"]);
    e.ok(&["-y", "journal", "remove", "Desk 2"]);
    assert!(!e.ok(&["journal", "list"]).contains("Desk 2"));
    let hist = e.ok(&["history", "list"]);
    for k in [
        "journal.create",
        "entry.add",
        "entry.edit",
        "note.add",
        "highlight.add",
        "image.add",
        "journal.fork",
        "entry.copy",
        "note.edit",
        "note.remove",
        "entry.remove",
        "journal.edit",
        "journal.remove",
    ] {
        assert!(hist.contains(k), "{k} missing: {hist}");
    }
}
