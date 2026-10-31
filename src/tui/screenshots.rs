//! Screenshots of key TUI features, mid-action, with sample data.
//! `cargo test --lib screenshots -- --ignored` writes target/screenshots/*.html;
//! scripts/screenshots.sh turns them into PNGs.
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

#[test]
#[ignore = "writes screenshots; run on demand"]
fn feature_screenshots() {
    let (_t, mut a) = app();
    seed(&mut a);
    shot(&mut a, "01-home", "Home: charts of the ingested bars");

    // journals, then the share picker open on an entry
    a.dispatch(Action::Journals);
    shot(&mut a, "02-journals", "Journals window with entries");
    a.on_key(key(KeyCode::Enter));
    shot(&mut a, "03-journal-entries", "A journal's entries");
    a.on_key(key(KeyCode::Char('S')));
    shot(&mut a, "04-share-picker", "Share: pick where the item goes");
    a.close_modal();
    a.modal = None;

    // model designer: a new model, its migration plan, then records
    a.on_key(key(KeyCode::Char('D')));
    shot(&mut a, "05-models", "Model designer: the model list");
    a.on_key(key(KeyCode::Char('n')));
    typ(&mut a, "Signals\n");
    typ(&mut a, "symbol:text!; side:text default=long; price:real; confidence:real\n");
    shot(&mut a, "06-model-plan", "Designing a model: the migration plan before apply");
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
    shot(&mut a, "07-model-records", "Records of the new model");
    a.modal = None;

    // code spaces: create one, edit it, and the list
    a.on_key(key(KeyCode::Char('C')));
    a.on_key(key(KeyCode::Char('n')));
    typ(&mut a, "ema-cross\n");
    shot(&mut a, "08-code-new", "New code space: choosing a language");
    a.code.ask.as_mut().unwrap().value.clear();
    typ(&mut a, "math\n");
    a.code.editor = code::Editor::new("");
    typ(&mut a, "@title EMA cross\nfast = ema(close, 12)\nslow = ema(close, 26)\nC = fast - slow\n");
    shot(&mut a, "09-code-editor", "Editing a code space");
    a.on_key(key(KeyCode::Esc));
    a.code.screen = code::Screen::List;
    shot(&mut a, "10-code-list", "Code spaces list with scope");
}
