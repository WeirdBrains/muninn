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
