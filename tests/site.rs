//! The website in `site/` (Jaspr, built in CI): its header art and colours stay the app's.
use std::path::Path;

fn read(p: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(p)).unwrap()
}

#[test]
fn the_site_uses_the_header_art() {
    let dart = read("site/lib/banner.dart");
    for row in muninn::tui::header::banner() {
        assert!(dart.contains(&format!("'{row}'")), "site/lib/banner.dart lacks the header row {row}");
    }
}

#[test]
fn the_site_uses_the_default_theme_colours() {
    let css = read("site/web/styles.css");
    let theme: toml::Value = toml::from_str(&read("assets/themes/tokyo-night.toml")).unwrap();
    for (mode, block) in [("dark", css.split("@media").next().unwrap()), ("light", css.split("@media").nth(1).unwrap())] {
        let pal = &theme[mode]["palette"];
        for (var, name) in
            [("bg", "bg"), ("fg", "fg"), ("muted", "comment"), ("border", "fg_gutter"), ("header", "blue"), ("accent", "magenta"), ("info", "cyan")]
        {
            let hex = pal[name].as_str().unwrap();
            assert!(block.contains(&format!("--{var}: {hex};")), "{mode} --{var} should be {hex} ({name})");
        }
    }
}

#[test]
fn the_site_links_releases_docs_and_install_steps() {
    let app = read("site/lib/app.dart");
    for want in [
        "/releases",
        "/tree/main/docs",
        "docs/install.md",
        "id: 'install'",
        "apt install",
        "dnf install",
        "makepkg",
        "brew install",
        "winget install",
    ] {
        assert!(app.contains(want), "site/lib/app.dart lacks {want}");
    }
    for (_, file) in app.lines().filter_map(|l| l.trim().strip_prefix("('")).filter_map(|l| l.split_once("', '")) {
        if let Some(f) = file.strip_suffix("'),").filter(|f| f.ends_with(".md")) {
            assert!(Path::new(env!("CARGO_MANIFEST_DIR")).join("docs").join(f).exists(), "site links docs/{f}, which is missing");
        }
    }
}

/// Every `screenshots/NAME.png` mentioned in `text`.
fn shot_refs(text: &str) -> Vec<String> {
    text.match_indices("screenshots/")
        .filter_map(|(i, _)| {
            let rest = &text[i..];
            rest.find(".png").map(|end| rest[..end + 4].to_string())
        })
        .filter(|s| !s.contains(char::is_whitespace) && !s.contains('\''))
        .collect()
}

#[test]
fn the_site_shows_a_hero_and_a_screenshot_per_feature() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let app = read("site/lib/app.dart");
    for want in [
        "import 'shots.dart'",
        "'hero-shot'",
        "heroShot.$1",
        "id: 'screenshots'",
        "href: '#screenshots'",
        "Component.text('Screenshots')",
        "MediaLoading.lazy",
        "alt: caption",
        "figcaption",
    ] {
        assert!(app.contains(want), "site/lib/app.dart lacks {want}");
    }
    let shots = read("site/lib/shots.dart");
    let hero = shots.split("heroShot").nth(1).and_then(|h| shot_refs(h).into_iter().next()).expect("shots.dart has no heroShot");
    let all = shot_refs(&shots);
    assert!(all.len() >= 20, "only {} screenshots", all.len());
    for f in all.iter().chain([&hero]) {
        assert!(root.join("site/web").join(f).exists(), "site shows {f}, which is missing");
    }
    for feature in [
        "home",
        "live-searches",
        "journals",
        "reports",
        "rule-scheduled",
        "model-template-import",
        "code-chart",
        "pipeline-designer",
        "pipeline-live",
        "pipes",
        "org-domain",
        "reviews",
        "peers",
        "share-picker",
    ] {
        assert!(all.iter().any(|f| f == &format!("screenshots/{feature}.png")), "no {feature} screenshot");
    }
}

#[test]
fn the_readme_shows_existing_screenshots() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let readme = read("README.md");
    let refs: Vec<_> =
        readme.match_indices("site/web/").map(|(i, _)| &readme[i..]).filter_map(|r| r.find(".png").map(|e| r[..e + 4].to_string())).collect();
    assert!(!refs.is_empty(), "README shows no screenshots");
    for f in refs {
        assert!(root.join(&f).exists(), "README shows {f}, which is missing");
    }
}
