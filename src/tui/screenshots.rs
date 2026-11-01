//! Screenshots of every TUI feature, some mid-action, with sample data, from a scratch
//! home (a temp dir: never `~/.muninn`). `cargo test --lib screenshots -- --ignored`
//! writes target/screenshots/NAME.html (and NAME.txt) plus target/screenshots/list.tsv
//! (name and caption); scripts/screenshots.sh renders them to PNG for the site and README.
use super::action::Action;
use super::tests::{app, key};
use super::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Color;

const W: u16 = 160;
const H: u16 = 48;

fn typ(a: &mut App, s: &str) {
    for c in s.chars() {
        a.on_key(key(if c == '\n' { KeyCode::Enter } else { KeyCode::Char(c) }));
    }
}

fn css(c: Color, fallback: &str) -> String {
    match c {
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        Color::Reset => fallback.into(),
        Color::Indexed(i) => format!("var(--c{i})"),
        other => format!("{other}").to_lowercase(),
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Render the app and save it as an HTML page of coloured cells.
fn shot(a: &mut App, name: &str, caption: &str) {
    let mut term = Terminal::new(TestBackend::new(W, H)).unwrap();
    term.draw(|f| a.draw(f)).unwrap();
    let buf = term.backend().buffer().clone();
    let (bg0, fg0) = (css(a.theme.bg, "#000"), css(a.theme.fg, "#ddd"));
    let mut html = format!(
        "<!doctype html><meta charset=utf-8><title>{caption}</title><style>body{{margin:0;background:{bg0}}}\
         pre{{margin:0;padding:12px;font:15px/1.2 'JetBrains Mono','DejaVu Sans Mono',monospace;color:{fg0}}}\
         span{{white-space:pre}}</style><pre>"
    );
    for y in 0..buf.area.height {
        let mut x = 0;
        while x < buf.area.width {
            let c = &buf[(x, y)];
            let (fg, bg, m) = (c.fg, c.bg, c.modifier);
            let mut run = String::new();
            while x < buf.area.width {
                let d = &buf[(x, y)];
                if (d.fg, d.bg, d.modifier) != (fg, bg, m) {
                    break;
                }
                run.push_str(d.symbol());
                x += 1;
            }
            let mut st = format!("color:{};background:{}", css(fg, &fg0), css(bg, &bg0));
            if m.contains(Modifier::BOLD) {
                st.push_str(";font-weight:bold");
            }
            if m.contains(Modifier::REVERSED) {
                st = format!("color:{};background:{}", css(bg, &bg0), css(fg, &fg0));
            }
            html.push_str(&format!("<span style=\"{st}\">{}</span>", esc(&run)));
        }
        html.push('\n');
    }
    html.push_str("</pre>");
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/screenshots");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{name}.html")), html).unwrap();
    let text: String =
        (0..buf.area.height).map(|y| (0..buf.area.width).map(|x| buf[(x, y)].symbol().to_string()).collect::<String>() + "\n").collect();
    std::fs::write(dir.join(format!("{name}.txt")), text).unwrap();
    let mut list = std::fs::read_to_string(dir.join("list.tsv")).unwrap_or_default();
    list.push_str(&format!("{name}\t{caption}\n"));
    std::fs::write(dir.join("list.tsv"), list).unwrap();
}

fn seed(a: &mut App) {
    for (p, d) in [("Q4 Crypto Research", "BTC and ETH momentum, Oct–Dec"), ("Macro Watch", "rates, DXY and risk-on days")] {
        a.store.create_project(p, d).unwrap();
    }
    let fnj = a
        .store
        .create_journal("Field Notes", Some(&a.store.find_project("Q4 Crypto Research").unwrap().id), crate::store::EntryFormat::Markdown)
        .unwrap();
    a.store.create_journal("Trade Log", None, crate::store::EntryFormat::Markdown).unwrap();
    for (t, b) in [
        (
            "ETH breakout above 2,600",
            "Volume doubled on the 5m bars after 14:00 UTC.\n\n- RSI 71\n- funding flat\n\nWatch the 2,640 retest. See https://muninn.example.org/reports/eth-q4",
        ),
        ("BTC range, day 4", "Still boxed in 41,800–42,600. Waiting for a close outside it."),
        ("Weekly review", "Three setups taken, two worked. Tighten stops on range days."),
    ] {
        a.store.add_entry(&fnj.id, t, b, None).unwrap();
    }
}

fn close(a: &mut App) {
    a.on_key(key(KeyCode::Esc));
    a.on_key(key(KeyCode::Esc));
    a.modal = None;
}

fn ticks(n: i64) -> String {
    let mut csv = String::from("timestamp,px,vol\n");
    for i in 0..n {
        let x = i as f64;
        csv.push_str(&format!(
            "{},{:.2},{:.1}\n",
            1_704_067_200_000 + i * 300_000,
            42_000.0 + 300.0 * (x / 9.0).sin() + 4.0 * x,
            20.0 + 15.0 * (x / 5.0).cos().abs()
        ));
    }
    csv
}

#[test]
#[ignore = "writes screenshots; run on demand"]
fn feature_screenshots() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/screenshots");
    let _ = std::fs::remove_dir_all(&dir);
    let (_t, mut a) = app();
    // display only: the home stays the scratch one
    a.home_shown = Some("~/.muninn".into());
    seed(&mut a);
    use crate::store::{Design, EntryFormat, Field, FieldType, NewRule, NewSection, RuleSpec, Schedule, SearchSpec, SectionKind};

    // home: the series list with a chart beside it
    a.tick();
    shot(&mut a, "home", "Home: every series in a list, the header with the node's status");
    a.dispatch(Action::NewTile(super::action::ViewKind::Chart));
    a.tick();
    shot(&mut a, "home-chart", "A chart tile beside the list: ETH/USDT 5m candles in the Muninn theme");
    a.dispatch(Action::ToggleHistory);
    shot(&mut a, "history", "The action history bar: every step, forks kept, step back and on");
    a.dispatch(Action::ToggleHistory);
    a.history.focused = false;

    // live searches
    for (n, q) in [("ETH momentum", "ETH"), ("BTC range days", "BTC"), ("All majors 5m", "")] {
        let spec = SearchSpec { query: q.into(), ..Default::default() };
        a.store.save_live_search(n, &spec, None).unwrap();
    }
    a.dispatch(Action::LiveSearches);
    shot(&mut a, "live-searches", "Live searches: saved questions that rerun on new data");
    close(&mut a);
    searches::open_save_then_share(&mut a);
    a.searches.ask.as_mut().unwrap().value = "BTC 5m tape".into();
    shot(&mut a, "share-view-name", "Share view, mid-action: saving the view as a live search");
    a.on_key(key(KeyCode::Enter));
    shot(&mut a, "share-view-picker", "Share: pick where it goes (a project, an org, a connection)");
    close(&mut a);

    // journals and share
    a.dispatch(Action::Journals);
    shot(&mut a, "journals", "Journals with their entries");
    a.on_key(key(KeyCode::Enter));
    shot(&mut a, "journal-entries", "A journal's entries in Markdown");
    a.on_key(key(KeyCode::Char('S')));
    shot(&mut a, "share-picker", "Sharing a journal entry: the picker open");
    close(&mut a);

    // reports
    let r = a.store.create_report("Q4 Crypto Review", "Momentum and range days, Oct–Dec").unwrap();
    let mut t = NewSection::new(SectionKind::Text);
    t.title = "Summary".into();
    t.body = "ETH broke out twice; BTC spent 11 days in a range. Three setups, two worked.".into();
    a.store.add_section(&r.id, &t).unwrap();
    let mut sr = NewSection::new(SectionKind::Series);
    sr.title = "Series in scope".into();
    a.store.add_section(&r.id, &sr).unwrap();
    let mut jr = NewSection::new(SectionKind::Journal);
    jr.title = "Field notes".into();
    jr.params.journal = Some(a.store.find_journal("Field Notes").unwrap().id);
    a.store.add_section(&r.id, &jr).unwrap();
    a.dispatch(Action::Reports);
    shot(&mut a, "reports", "Reports: sections of text, series and journals, exported to PDF, Docx or HTML");
    a.on_key(key(KeyCode::Enter));
    shot(&mut a, "report-sections", "A report's sections");
    close(&mut a);

    // rules, scheduled ones and the built-in cert watcher
    let mk = |a: &mut App, name: &str, spec: RuleSpec| {
        a.store.create_rule(&NewRule { name: name.into(), spec, enabled: true, ..Default::default() }).unwrap();
    };
    let s = &a.store;
    let weekly = RuleSpec {
        on: vec!["import".into()],
        when: vec![crate::rules::parse_condition("inserted > 0").unwrap()],
        then: vec![crate::rules::parse_then(s, "status Imported {inserted} bars").unwrap()],
        ..Default::default()
    };
    let nightly = RuleSpec {
        schedule: Some(Schedule::Cron("0 3 * * *".into())),
        then: vec![crate::rules::parse_then(s, "export-report \"Q4 Crypto Review\" to=~/reports/q4-{date}.pdf").unwrap()],
        ..Default::default()
    };
    let hourly = RuleSpec {
        schedule: Some(Schedule::Every(3_600_000)),
        then: vec![crate::rules::parse_then(s, "search \"ETH momentum\"").unwrap()],
        ..Default::default()
    };
    mk(&mut a, "After import", weekly);
    mk(&mut a, "Nightly report", nightly);
    mk(&mut a, "Hourly ETH check", hourly);
    mk(&mut a, crate::rules::CERT_WATCH, crate::rules::cert_watch_spec());
    a.dispatch(Action::Rules);
    shot(&mut a, "rules", "Rule sets: on actions or on a schedule (every 1h, cron), with the built-in cert watcher");
    a.on_key(key(KeyCode::Char('j')));
    a.on_key(key(KeyCode::Enter));
    shot(&mut a, "rule-scheduled", "A scheduled rule: its schedule, next run and then-actions");
    a.on_key(key(KeyCode::Esc));
    a.on_key(key(KeyCode::Char('n')));
    typ(&mut a, "Morning brief\n");
    a.rules.ask.as_mut().unwrap().value = "every 1d; import".into();
    shot(&mut a, "rule-new", "New rule, mid-action: \"on\" takes action kinds or a schedule");
    close(&mut a);

    // model designer and templates
    a.on_key(key(KeyCode::Char('D')));
    shot(&mut a, "models", "Model designer: models, the built-in bars model among them");
    a.on_key(key(KeyCode::Char('n')));
    typ(&mut a, "Signals\n");
    typ(&mut a, "symbol:text!; side:text default=long; price:real; confidence:real\n");
    shot(&mut a, "model-plan", "Designing a model: the migration plan before apply");
    a.on_key(key(KeyCode::Char('A')));
    a.on_key(key(KeyCode::Char('r')));
    for v in [
        "symbol=BTC/USDT side=long price=42150 confidence=0.72",
        "symbol=ETH/USDT side=long price=2611 confidence=0.81",
        "symbol=SOL/USDT side=short price=148.2 confidence=0.55",
    ] {
        a.on_key(key(KeyCode::Char('n')));
        a.models.ask.as_mut().expect("record prompt").value = v.to_string();
        a.on_key(key(KeyCode::Enter));
    }
    shot(&mut a, "model-records", "Records of the new model");
    close(&mut a);
    a.on_key(key(KeyCode::Char('D')));
    a.on_key(key(KeyCode::Char('i')));
    a.models.ask.as_mut().expect("import prompt").value = "~/templates/signals.mnnt".into();
    shot(&mut a, "model-template-import", "Model templates, mid-action: importing a design (planned first, rename or merge)");
    close(&mut a);

    // code spaces and their chart tile
    a.on_key(key(KeyCode::Char('C')));
    a.on_key(key(KeyCode::Char('n')));
    typ(&mut a, "ema-cross\n");
    shot(&mut a, "code-new", "New code space, mid-action: choosing a language (math, Polars, Rust, Python)");
    a.code.ask.as_mut().unwrap().value.clear();
    typ(&mut a, "math\n");
    a.code.editor = code::Editor::new("");
    typ(&mut a, "@title EMA cross\nfast = ema(close, 12)\nslow = ema(close, 26)\nC = fast - slow\n");
    shot(&mut a, "code-editor", "Editing a code space");
    a.on_key(key(KeyCode::Esc));
    a.code.screen = code::Screen::List;
    shot(&mut a, "code-list", "Code spaces and their scope");
    close(&mut a);
    a.layout = super::layout::TileLayout::single(None);
    a.tick();
    super::frametile::open(&mut a, super::frametile::FrameTile::new("code:ema-cross")).unwrap();
    let mut ema = String::from("timestamp,close,fast,slow\n");
    let (mut f, mut sl) = (42_000.0f64, 42_000.0f64);
    for i in 0..160i64 {
        let x = i as f64;
        let c = 42_000.0 + 380.0 * (x / 14.0).sin() + 120.0 * (x / 3.7).cos() + 3.0 * x;
        f += (c - f) * 2.0 / 13.0;
        sl += (c - sl) * 2.0 / 27.0;
        ema.push_str(&format!("{},{c:.2},{f:.2},{sl:.2}\n", 1_704_067_200_000 + i * 300_000));
    }
    crate::code::pipe::note_frame(&a.store, "cli", "code:ema-cross", &crate::code::pipe::read_data(&ema, "x").unwrap());
    a.tick();
    shot(&mut a, "code-chart", "A code space's result as a live chart tile beside the series");

    // pipes and pipelines
    for m in ["Prices", "Volumes", "Ticks"] {
        a.store
            .create_model(
                m,
                "",
                Design { fields: vec![Field::new("px", FieldType::Real), Field::new("vol", FieldType::Real)], ..Default::default() },
                true,
            )
            .unwrap();
    }
    let sv = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let g = crate::code::pipeline::Graph::parse(
        &sv(&["in=-", "price=view:chart:px", "volume=view:chart:vol", "tape=view:table", "p=model:Prices", "v=model:Volumes", "t=model:Ticks"]),
        &sv(&["in>price>p", "in>volume>v", "in>tape>t"]),
    )
    .unwrap();
    let p = a.store.create_pipeline("feed", "Ticks from stdin into three views and three models", &g).unwrap();
    a.layout = super::layout::TileLayout::single(None);
    a.tick();
    let env = crate::code::CodeEnv::new(&a.paths.home);
    crate::code::pipeline::run(&a.store, &env, "cli", &p, Some(crate::code::pipe::read_data(&ticks(120), "stdin").unwrap()), &mut |_| {}).unwrap();
    a.tick();
    shot(&mut a, "pipeline-live", "cat ticks.csv | mnnn pipeline run feed -: three views as three live columns");
    super::frametile::open(&mut a, super::frametile::FrameTile::new("monitor:feed")).unwrap();
    a.tick();
    shot(&mut a, "pipeline-monitor", "The pipeline monitor: each node's status, rows, rate and what landed");
    a.layout = super::layout::TileLayout::single(None);
    a.dispatch(Action::Pipes);
    a.tick();
    shot(&mut a, "pipes", "Pipes: data from stdin or any search through code spaces, followed live");
    close(&mut a);
    a.on_key(key(KeyCode::Char('E')));
    a.on_key(key(KeyCode::Enter));
    shot(&mut a, "pipeline-designer", "The pipeline designer: a graph of sources, code, views and model tables");
    a.on_key(key(KeyCode::Char('a')));
    a.designer.ask.as_mut().expect("node prompt").value = "ema=code:ema-cross".into();
    shot(&mut a, "pipeline-designer-add", "Pipeline designer, mid-action: adding a code node");
    close(&mut a);

    // organizations: TLS and domains
    a.on_key(key(KeyCode::Char('G')));
    a.on_key(key(KeyCode::Char('n')));
    typ(&mut a, "Comet Lab\n");
    let o = a.store.find_org("comet-lab").unwrap();
    crate::org::tls::add_domain(&a.store, &o, "lab.cometresearch.org").unwrap();
    crate::org::tls::add_domain(&a.store, &o, "www.cometresearch.org").unwrap();
    let c = crate::org::host::self_signed(&["lab.cometresearch.org".into(), "www.cometresearch.org".into()], 75).unwrap();
    crate::org::tls::set_user_cert(&a.store, &o, &c).unwrap();
    let o2 = crate::org::create(&a.store, "tui", "Nebula Desk", None, "").unwrap();
    crate::org::tls::add_domain(&a.store, &o2, "nebula.example.net").unwrap();
    crate::org::tls::set_acme(&a.store, &o2, &crate::org::tls::OrgAcme { enabled: true, email: "ops@nebula.example.net".into() }).unwrap();
    close(&mut a);
    a.on_key(key(KeyCode::Char('G')));
    shot(&mut a, "orgs", "Organizations: visibility, access, hosting, domains (w), certificate (c), ACME (L)");
    a.on_key(key(KeyCode::Char('w')));
    a.orgs.ask.as_mut().expect("domain prompt").value = "add docs.cometresearch.org".into();
    shot(&mut a, "org-domain", "Org TLS, mid-action: adding a domain");
    a.on_key(key(KeyCode::Esc));
    a.on_key(key(KeyCode::Char('6')));
    shot(&mut a, "org-site", "The org site: each org's domains and certificate (yours, Let's Encrypt or self-signed)");
    close(&mut a);

    // reviews
    a.on_key(key(KeyCode::Char('V')));
    a.on_key(key(KeyCode::Char('n')));
    typ(&mut a, "Field Notes\n");
    a.on_key(key(KeyCode::Enter));
    a.on_key(key(KeyCode::Char('c')));
    typ(&mut a, "Good call on the 2,640 retest; add the funding chart\n");
    shot(&mut a, "reviews", "Reviews: comments, suggestions, highlights and votes on shared work");
    close(&mut a);

    // network: this node and two sample nodes on localhost, connected
    let local = |x: &mut App, label: &str, user: &str| {
        let (me, _) = crate::net::user::load_or_create(&x.store).unwrap();
        x.store.label_user(&me.id, user).unwrap();
        x.prefs.net.listen = vec!["/ip4/127.0.0.1/tcp/0".into()];
        x.prefs.net.mdns = false;
        x.prefs.net.label = label.into();
        x.dispatch(Action::NetStart);
        assert!(x.network.node_running(), "{:?}", x.network.msg);
    };
    let settle = |x: &mut App| {
        let t = std::time::Instant::now();
        while x.network.job.is_some() {
            assert!(t.elapsed() < std::time::Duration::from_secs(30), "network task did not finish");
            std::thread::sleep(std::time::Duration::from_millis(20));
            x.tick();
        }
    };
    let (_tb, mut b) = app();
    let (_tc, mut c) = app();
    local(&mut a, "comet-lab", "Ada");
    local(&mut b, "ben-laptop", "Ben");
    local(&mut c, "cleo-desk", "Cleo");
    for x in [&mut b, &mut c] {
        let h = x.network.hosted.as_ref().expect("node running");
        let to = format!("{}/p2p/{}", h.node.status().listening[0], h.node.peer_id());
        a.dispatch(Action::NetExchanges);
        a.on_key(key(KeyCode::Char('c')));
        typ(&mut a, &format!("{to}\n"));
        settle(&mut a);
        close(&mut a);
        x.dispatch(Action::NetExchanges);
        x.on_key(key(KeyCode::Char('a')));
        settle(x);
    }
    let t = std::time::Instant::now();
    while a.store.connections().unwrap().len() < 2 || a.store.exchanges().unwrap().iter().filter(|x| x.status.active()).count() < 2 {
        assert!(t.elapsed() < std::time::Duration::from_secs(30), "sample nodes did not connect");
        std::thread::sleep(std::time::Duration::from_millis(50));
        a.tick();
        b.tick();
        c.tick();
    }
    for _ in 0..5 {
        a.tick();
    }
    a.set_status("connected: ben-laptop, cleo-desk");
    a.dispatch(Action::NetStatus);
    shot(&mut a, "network", "The network: this node, relays, connections and shares, end to end encrypted");
    close(&mut a);
    a.dispatch(Action::NetPeers);
    shot(&mut a, "peers", "Peers: nodes seen nearby and through relays");
    close(&mut a);
    a.dispatch(Action::NetConnections);
    shot(&mut a, "connections", "Connections: people you trust, made through invites");
    close(&mut a);
    for x in [&mut a, &mut b, &mut c] {
        x.dispatch(Action::NetStop);
    }
    let _ = EntryFormat::Markdown;
}
