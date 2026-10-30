//! `mnnn report`, document export, printing and project/journal packs end to end.
mod common;
use common::*;
use std::path::Path;

fn err(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

fn ingest(e: &Env, file: &str) {
    let f = fixture(file);
    e.ok(&["-y", "ingest", "csv", "--path", f.to_str().unwrap(), "--source", "binance"]);
}

fn png(path: &Path) {
    let f = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), 2, 2);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&[255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255]).unwrap();
}

fn kinds(e: &Env) -> Vec<(String, String)> {
    let v: serde_json::Value = serde_json::from_str(&e.ok(&["history", "list", "--all", "--json", "-n", "500"])).unwrap();
    v.as_array()
        .unwrap()
        .iter()
        .map(|a| (a["kind"].as_str().unwrap_or("").to_string(), a["author"].to_string() + &a["author_id"].to_string()))
        .collect()
}

/// A journal with notes, a highlight and an image; a project with a search, result and that journal.
fn desk(e: &Env) -> String {
    e.ok(&["author", "add", "--name", "Ada"]);
    ingest(e, "BTCUSDT_5m.csv");
    e.ok(&["search", "save", "btc", "--query", "btc"]);
    e.ok(&["project", "create", "Breakouts", "--description", "range breaks"]);
    e.ok(&["project", "add", "Breakouts", "--search", "btc"]);
    e.ok(&["project", "add", "Breakouts", "--result", "binance:btc-usdt:5m", "--from", "2017-08-17 04:00", "--to", "2017-08-17 04:30"]);
    e.ok(&["journal", "new", "Desk", "--project", "Breakouts"]);
    e.ok(&["journal", "write", "Desk", "--title", "March", "--text", "BTC broke out of the range.\\nThen it held."]);
    let v: serde_json::Value = serde_json::from_str(&e.ok(&["journal", "show", "Desk", "--json"])).unwrap();
    let id = v["entries"][0]["entry"]["id"].as_str().unwrap()[..8].to_string();
    e.ok(&["journal", "note", "watch the weekly", "--journal", "Desk"]);
    e.ok(&["journal", "note", "big candle", "--entry", &id, "--on", "broke out"]);
    e.ok(&["journal", "highlight", &id, "--on", "the range", "--color", "yellow"]);
    let img = e.dir.path().join("chart.png");
    png(&img);
    e.ok(&["journal", "attach", &id, img.to_str().unwrap()]);
    id
}

#[test]
fn reports_are_built_edited_and_exported_in_every_format() {
    let e = Env::new();
    let entry = desk(&e);
    e.ok(&["report", "create", "Weekly", "--description", "the week", "--project", "Breakouts"]);
    e.ok(&["report", "add", "Weekly", "--title", "Summary", "--text", "# Week\\nBTC **held** the range."]);
    e.ok(&["report", "add", "Weekly", "--records", "binance:btc-usdt:5m", "--rows", "5"]);
    e.ok(&["report", "add", "Weekly", "--series", "btc"]);
    e.ok(&["report", "add", "Weekly", "--series"]);
    e.ok(&["report", "add", "Weekly", "--journal", "Desk", "--entry", &entry]);
    e.ok(&["report", "add", "Weekly", "--project", "Breakouts"]);
    e.ok(&["code", "create", "momentum", "--lang", "math", "--model", "bars", "--source", "fast = sma(close, 3)"]);
    e.ok(&["report", "add", "Weekly", "--code", "momentum"]);
    e.ok(&["report", "add", "Weekly", "--authors", "--at", "1"]);
    let o = e.run(&["report", "add", "Weekly", "--journal", "Nope"]);
    assert!(!o.status.success());
    let o = e.run(&["report", "add", "Weekly", "--authors", "--format", "latex"]);
    assert!(!o.status.success() && err(&o).contains("--format"), "{}", err(&o));
    let show = e.ok(&["report", "show", "Weekly"]);
    let lines: Vec<&str> = show.lines().filter(|l| l.trim_start().starts_with(char::is_numeric)).collect();
    assert_eq!(lines.len(), 8, "{show}");
    assert!(lines[0].contains("Authors") && lines[1].contains("Summary"), "{show}");
    assert!(show.contains("records BTC/USDT binance 5m · latest 5 bars"), "{show}");
    assert!(show.contains("series matching live search btc") && show.contains("every series"), "{show}");
    assert!(show.contains("journal Desk · 1 entry") && show.contains("project Breakouts"), "{show}");
    assert!(show.contains("code space momentum") && show.contains("· Ada"), "{show}");
    e.ok(&["report", "move", "Weekly", "1", "--to", "8"]);
    e.ok(&["report", "edit-section", "Weekly", "1", "--title", "Overview", "--text", "BTC *held*."]);
    e.ok(&["report", "edit-section", "Weekly", "2", "--rows", "3"]);
    let o = e.run(&["report", "edit-section", "Weekly", "2", "--text", "nope"]);
    assert!(!o.status.success());
    e.ok(&["report", "remove-section", "Weekly", "4"]);
    e.ok(&["report", "edit", "Weekly", "--name", "Week 12"]);
    let v: serde_json::Value = serde_json::from_str(&e.ok(&["report", "show", "Week 12", "--json"])).unwrap();
    let secs = v["sections"].as_array().unwrap();
    assert_eq!(secs.len(), 7);
    assert_eq!(secs[0]["title"], "Overview");
    assert_eq!(secs[1]["params"]["rows"], 3);
    assert_eq!(secs[6]["kind"], "authors");
    assert!(e.ok(&["project", "show", "Breakouts"]).contains("Week 12"));

    let preview = e.ok(&["report", "preview", "Week 12"]);
    assert!(preview.contains("Week 12") && preview.contains("Overview") && preview.contains("BTC/USDT"), "{preview}");
    assert!(preview.contains("momentum") && preview.contains("Not run yet") && preview.contains("sma(close, 3)"), "{preview}");
    let dir = e.dir.path().join("out");
    std::fs::create_dir_all(&dir).unwrap();
    for f in ["pdf", "html", "docx", "latex", "json", "csv", "text"] {
        let out = dir.join(format!("week.{f}"));
        e.ok(&["report", "export", "Week 12", "--format", f, "--out", out.to_str().unwrap()]);
        let b = std::fs::read(&out).unwrap();
        assert!(b.len() > 100, "{f}: {} bytes", b.len());
        match f {
            "pdf" => assert!(b.starts_with(b"%PDF-")),
            "docx" => assert!(b.starts_with(b"PK")),
            "html" => assert!(String::from_utf8_lossy(&b).contains("<h1")),
            "latex" => assert!(String::from_utf8_lossy(&b).contains("\\documentclass")),
            "csv" => assert!(String::from_utf8_lossy(&b).lines().next().unwrap().starts_with("section")),
            "json" => {
                let v: serde_json::Value = serde_json::from_slice(&b).unwrap();
                assert_eq!(v["format"], "muninn-report");
            }
            _ => assert!(String::from_utf8_lossy(&b).contains("Overview")),
        }
    }
    // Default name in the working directory.
    let o = e.cmd().current_dir(&dir).args(["report", "export", "Week 12"]).output().unwrap();
    assert!(o.status.success(), "{}", err(&o));
    assert!(dir.join("week-12.pdf").exists(), "{:?}", std::fs::read_dir(&dir).unwrap().map(|d| d.unwrap().file_name()).collect::<Vec<_>>());
    let o = e.run(&["report", "export", "Week 12", "--format", "archive"]);
    assert!(!o.status.success());
    e.ok(&["report", "create", "Spare"]);
    let o = e.run(&["report", "remove", "Spare"]);
    assert!(!o.status.success(), "needs confirmation");
    e.ok(&["-y", "report", "remove", "Spare"]);
    assert!(!e.ok(&["report", "list"]).contains("Spare"));
    let k = kinds(&e);
    for want in [
        "report.create",
        "report.edit",
        "report.remove",
        "report.section.add",
        "report.section.edit",
        "report.section.remove",
        "report.section.move",
        "report.export",
    ] {
        let hit = k.iter().find(|(kind, _)| kind == want).unwrap_or_else(|| panic!("{want} missing: {k:?}"));
        assert!(hit.1.contains("Ada") || hit.1.len() > 8, "{want} has no author: {hit:?}");
    }
}

#[test]
fn journals_export_print_and_round_trip() {
    let e = Env::new();
    let entry = desk(&e);
    let dir = e.dir.path().join("out");
    std::fs::create_dir_all(&dir).unwrap();
    for f in ["pdf", "html", "docx", "latex", "text"] {
        let out = dir.join(format!("desk.{f}"));
        e.ok(&["journal", "export", "Desk", "--format", f, "--out", out.to_str().unwrap()]);
        assert!(std::fs::metadata(&out).unwrap().len() > 100, "{f}");
    }
    let text = std::fs::read_to_string(dir.join("desk.text")).unwrap();
    assert!(text.contains("March") && text.contains("big candle") && text.contains("watch the weekly"), "{text}");
    let html = std::fs::read_to_string(dir.join("desk.html")).unwrap();
    assert!(html.contains("<mark") && html.contains("data:image/png;base64"), "{html}");
    let o = e.run(&["journal", "export", "Desk", "--format", "csv", "--out", dir.join("x.csv").to_str().unwrap()]);
    assert!(!o.status.success());
    let one = dir.join("entry.pdf");
    e.ok(&["journal", "export", "Desk", "--entry", &entry, "--out", one.to_str().unwrap()]);
    assert!(std::fs::read(&one).unwrap().starts_with(b"%PDF-"));

    // JSON round trip, into a project and under a new name.
    let json = dir.join("desk.json");
    e.ok(&["journal", "export", "Desk", "--format", "json", "--out", json.to_str().unwrap()]);
    e.ok(&["project", "create", "Other"]);
    e.ok(&["journal", "import", json.to_str().unwrap(), "--project", "Other", "--name", "Desk copy"]);
    e.ok(&["journal", "import", json.to_str().unwrap()]);
    let list = e.ok(&["journal", "list"]);
    assert!(list.contains("Desk copy") && list.contains("Desk (imported)"), "{list}");
    let show = e.ok(&["journal", "show", "Desk copy"]);
    assert!(show.contains("big candle") && show.contains("watch the weekly") && show.contains("highlight \"the range\""), "{show}");
    let v: serde_json::Value = serde_json::from_str(&e.ok(&["journal", "show", "Desk copy", "--json"])).unwrap();
    let img = v["entries"][0]["attachments"][0]["id"].as_str().unwrap().to_string();
    let back = dir.join("back.png");
    e.ok(&["journal", "image", &img[..8], "--out", back.to_str().unwrap()]);
    let mut want = Vec::new();
    png(&dir.join("again.png"));
    want.extend(std::fs::read(dir.join("again.png")).unwrap());
    assert_eq!(std::fs::read(&back).unwrap(), want);
    assert!(e.ok(&["project", "show", "Other"]).contains("Desk copy"));

    // Printing without lp/lpr saves the PDF and says where.
    let o = e.cmd().env("PATH", "/nonexistent").args(["journal", "print", "Desk"]).output().unwrap();
    assert!(o.status.success(), "{}", err(&o));
    let msg = err(&o) + &String::from_utf8_lossy(&o.stdout);
    assert!(msg.contains("no print command") && msg.contains("prints"), "{msg}");
    let prints: Vec<_> = std::fs::read_dir(e.home().join("prints")).unwrap().collect();
    assert_eq!(prints.len(), 1);
    // --save-only never calls a printer, even when there is one.
    let kept = dir.join("kept.pdf");
    let o = e.run(&["journal", "print", "Desk", "--save-only", "--out", kept.to_str().unwrap()]);
    let msg = err(&o) + &String::from_utf8_lossy(&o.stdout);
    assert!(o.status.success() && msg.contains("not sent to a printer"), "{msg}");
    assert!(std::fs::read(&kept).unwrap().starts_with(b"%PDF-"));
    let k = kinds(&e);
    for want in ["journal.export", "entry.export", "journal.import", "journal.print"] {
        assert!(k.iter().any(|(kind, _)| kind == want), "{want} missing: {k:?}");
    }
}

#[cfg(unix)]
#[test]
fn printing_hands_the_pdf_to_lp() {
    use std::os::unix::fs::PermissionsExt;
    let e = Env::new();
    let entry = desk(&e);
    let bin = e.dir.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let log = e.dir.path().join("lp.log");
    let lp = bin.join("lp");
    std::fs::write(&lp, format!("#!/bin/sh\necho \"$@\" > '{}'\necho 'request id is fake-1'\n", log.display())).unwrap();
    std::fs::set_permissions(&lp, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default());
    let out = e.dir.path().join("p.pdf");
    let o = e
        .cmd()
        .env("PATH", &path)
        .args(["journal", "print", "Desk", "--entry", &entry, "--printer", "office", "--out", out.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", err(&o));
    let args = std::fs::read_to_string(&log).unwrap();
    assert!(args.contains("-d office") && args.contains(out.to_str().unwrap()), "{args}");
    assert!(std::fs::read(&out).unwrap().starts_with(b"%PDF-"));
    let msg = err(&o) + &String::from_utf8_lossy(&o.stdout);
    assert!(msg.contains("lp") && msg.contains("fake-1"), "{msg}");
    // A failing lp is an error that names the PDF.
    std::fs::write(&lp, "#!/bin/sh\necho 'no such printer' >&2\nexit 1\n").unwrap();
    let o = e.cmd().env("PATH", &path).args(["journal", "print", "Desk"]).output().unwrap();
    assert!(!o.status.success() && err(&o).contains(".pdf"), "{}", err(&o));
}

#[test]
fn projects_export_to_one_archive_and_import_back() {
    let e = Env::new();
    desk(&e);
    e.ok(&["report", "create", "Weekly", "--project", "Breakouts"]);
    e.ok(&["report", "add", "Weekly", "--text", "Overview"]);
    e.ok(&["report", "add", "Weekly", "--journal", "Desk"]);
    e.ok(&["report", "add", "Weekly", "--series", "btc"]);
    let dir = e.dir.path().join("out");
    std::fs::create_dir_all(&dir).unwrap();
    let arc = dir.join("b.tar.zst");
    e.ok(&["project", "export", "Breakouts", "--out", arc.to_str().unwrap()]);
    let json = dir.join("b.json");
    e.ok(&["project", "export", "Breakouts", "--format", "json", "--out", json.to_str().unwrap()]);
    for f in ["pdf", "html", "docx", "latex", "csv", "text"] {
        let out = dir.join(format!("b.{f}"));
        e.ok(&["project", "export", "Breakouts", "--format", f, "--out", out.to_str().unwrap()]);
        assert!(std::fs::metadata(&out).unwrap().len() > 100, "{f}");
    }
    let before: serde_json::Value = serde_json::from_str(&e.ok(&["project", "show", "Breakouts", "--json"])).unwrap();

    // Into a fresh home: everything comes back, data included.
    let f = Env::new();
    f.ok(&["project", "import", arc.to_str().unwrap()]);
    let show = f.ok(&["project", "show", "Breakouts"]);
    assert!(show.contains("btc") && show.contains("Desk") && show.contains("Weekly") && show.contains("BTC/USDT"), "{show}");
    let after: serde_json::Value = serde_json::from_str(&f.ok(&["project", "show", "Breakouts", "--json"])).unwrap();
    assert_eq!(before["items"].as_array().unwrap().len(), 4, "{before}");
    assert_eq!(before["items"].as_array().map(|v| v.len()), after["items"].as_array().map(|v| v.len()), "{before}\n{after}");
    let j = f.ok(&["journal", "show", "Desk"]);
    assert!(j.contains("big candle") && j.contains("watch the weekly") && j.contains("· Ada"), "{j}");
    assert!(f.ok(&["search", "run", "btc"]).contains("BTC/USDT"));
    let r = f.ok(&["report", "show", "Weekly"]);
    assert!(r.contains("journal Desk") && r.contains("series matching live search btc"), "{r}");
    let csv = f.dir.path().join("bars.csv");
    f.ok(&["export", "--format", "csv", "--out", csv.to_str().unwrap()]);
    assert!(std::fs::read_to_string(&csv).unwrap().lines().count() > 5);
    // JSON pack too, and a clash gets a new name.
    f.ok(&["project", "import", json.to_str().unwrap()]);
    assert!(f.ok(&["project", "list"]).contains("Breakouts (imported)"));
    // Its journal and report get new names too, so the originals stay unambiguous.
    assert!(f.ok(&["journal", "list"]).contains("Desk (imported)"));
    assert!(f.ok(&["report", "list"]).contains("Weekly (imported)"));
    f.ok(&["journal", "show", "Desk"]);
    f.ok(&["report", "show", "Weekly"]);
    f.ok(&["project", "import", arc.to_str().unwrap(), "--name", "Copy"]);
    assert!(f.ok(&["project", "list"]).contains("Copy"));
    // Without data the archive is smaller and still imports.
    let lean = dir.join("lean.tar.zst");
    e.ok(&["project", "export", "Breakouts", "--no-data", "--out", lean.to_str().unwrap()]);
    assert!(std::fs::metadata(&lean).unwrap().len() < std::fs::metadata(&arc).unwrap().len());
    let o = f.run(&["project", "import", dir.join("b.csv").to_str().unwrap()]);
    assert!(!o.status.success());
    for (env, want) in [(&e, "project.export"), (&f, "project.import")] {
        assert!(kinds(env).iter().any(|(k, _)| k == want), "{want}");
    }
}

#[test]
fn bars_export_as_documents() {
    let e = Env::new();
    ingest(&e, "BTCUSDT_5m.csv");
    let dir = e.dir.path();
    for f in ["pdf", "html", "docx", "latex"] {
        let out = dir.join(format!("bars.{f}"));
        e.ok(&["export", "--format", f, "--out", out.to_str().unwrap()]);
        assert!(std::fs::metadata(&out).unwrap().len() > 100, "{f}");
    }
    let text = e.ok(&["export", "--format", "text", "--out", "-"]);
    assert!(text.contains("BTC/USDT"), "{text}");
    let o = e.run(&["export", "--format", "pdf", "--out", "-"]);
    assert!(!o.status.success());
}

#[test]
fn report_authors_include_later_editors() {
    let e = Env::new();
    e.ok(&["report", "create", "Notes"]);
    e.ok(&["report", "add", "Notes", "--text", "Draft"]);
    e.ok(&["report", "add", "Notes", "--authors"]);
    let before = e.ok(&["report", "preview", "Notes"]);
    assert!(before.contains("No authors recorded"), "{before}");
    e.ok(&["author", "add", "--name", "Grace"]);
    e.ok(&["author", "use", "Grace"]);
    e.ok(&["report", "edit-section", "Notes", "1", "--text", "Final"]);
    let after = e.ok(&["report", "preview", "Notes"]);
    assert!(after.contains("Grace") && after.contains("edits"), "{after}");
}
