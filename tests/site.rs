//! The website in `site/` (Jaspr, built in CI): its header art and colours stay the app's.
use std::path::Path;

fn read(p: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(p)).unwrap()
}

#[test]
fn the_hero_is_the_raven_and_a_screenshot_with_no_block_letter_title() {
    let app = read("site/lib/app.dart");
    assert!(!Path::new(env!("CARGO_MANIFEST_DIR")).join("site/lib/banner.dart").exists(), "the block-letter title is gone");
    assert!(!app.contains("banner"), "app.dart still draws the block-letter title");
    assert!(app.contains("h1(classes: 'sr-only', [Component.text('Muninn')])"), "the page keeps a (visually hidden) h1");
    let hero = app.split("header(classes: 'hero'").nth(1).unwrap();
    let (logo, shot) = (hero.find("hero-logo").unwrap(), hero.find("hero-shot").unwrap());
    assert!(logo < shot, "the raven sits above the hero screenshot");
    assert!(read("site/web/styles.css").contains(".sr-only {"));
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
        "const docs = '/docs/'",
        "const installDoc = '/docs/install/'",
        "id: 'install'",
        "apt install",
        "dnf install",
        "pacman -U",
        "brew install",
        "windows-setup.exe",
        "macos.dmg",
        "winget install WeirdBrains.muninn.tui",
    ] {
        assert!(app.contains(want), "site/lib/app.dart lacks {want}");
    }
    for (_, file) in app.lines().filter_map(|l| l.trim().strip_prefix("('")).filter_map(|l| l.split_once("', '")) {
        if let Some(f) = file.strip_suffix("'),").filter(|f| f.ends_with(".md")) {
            assert!(Path::new(env!("CARGO_MANIFEST_DIR")).join("docs").join(f).exists(), "site links docs/{f}, which is missing");
        }
    }
}

#[test]
fn every_doc_is_published_as_a_site_page_with_working_links() {
    // site/tool/docs.dart publishes docs/**/*.md except docs/historical at /docs/<slug>/;
    // links between docs must resolve to one of those pages (or leave docs/ on purpose).
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs");
    let generator = read("site/tool/docs.dart");
    assert!(generator.contains("startsWith('historical/')") && generator.contains("/docs/"), "the generator skips docs/historical and writes /docs/");
    assert!(read("docs/website.md").contains("dart run tool/docs.dart"), "the deploy steps run the docs generator");
    let mut docs = vec![];
    fn walk(d: &Path, root: &Path, out: &mut Vec<String>) {
        for e in std::fs::read_dir(d).unwrap().flatten() {
            let p = e.path();
            let rel = p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
            if p.is_dir() && rel != "historical" {
                walk(&p, root, out);
            } else if rel.ends_with(".md") {
                out.push(rel);
            }
        }
    }
    walk(&root, &root, &mut docs);
    assert!(docs.len() > 40 && docs.contains(&"install.md".to_string()) && docs.contains(&"services/redis.md".to_string()));
    for d in &docs {
        let text = std::fs::read_to_string(root.join(d)).unwrap();
        assert!(text.lines().any(|l| l.starts_with("# ")), "{d} has no title (# heading) for its page");
        let dir = Path::new(d).parent().unwrap();
        let mut in_code = false;
        for line in text.lines() {
            if line.trim_start().starts_with("```") {
                in_code = !in_code;
            }
            if in_code {
                continue;
            }
            for (_, href) in muninn::help::inline_links(line) {
                if href.contains("://") || href.starts_with('#') || href.starts_with("mailto:") {
                    continue;
                }
                let path = href.split('#').next().unwrap();
                if path.ends_with(".md") {
                    let target = dir.join(path);
                    assert!(root.join(&target).exists(), "{d} links {href}, which has no page");
                }
            }
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
fn the_site_shows_matched_app_and_tui_screenshot_pairs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let app = read("site/lib/app.dart");
    for want in [
        "import 'pairs.dart'",
        "'hero-shot'",
        "pairs.first",
        "id: 'screenshots'",
        "href: '#screenshots'",
        "Component.text('Screenshots')",
        "MediaLoading.lazy",
        "alt: s.$2",
        "figcaption",
        "'Flutter app'",
        "'Terminal UI'",
    ] {
        assert!(app.contains(want), "site/lib/app.dart lacks {want}");
    }
    assert!(!app.contains("shots.dart"), "the old screenshot gallery is still on the page");
    let pairs = read("site/lib/pairs.dart");
    let all = shot_refs(&pairs);
    assert!(all.len() >= 4 && all.len().is_multiple_of(2), "{} screenshots is not a set of pairs", all.len());
    for p in all.chunks(2) {
        assert!(p[0].contains("/app-") && p[1].contains("/tui-"), "{p:?} is not an app/TUI pair");
        assert_eq!(p[0].replace("/app-", "/"), p[1].replace("/tui-", "/"), "unmatched pair {p:?}");
    }
    for f in &all {
        assert!(root.join("site/web").join(f).exists(), "site shows {f}, which is missing");
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

#[test]
fn the_announcement_shows_existing_screenshots() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let post = read("docs/announcement-2.0.md");
    let refs: Vec<_> = post.match_indices("](../").map(|(i, _)| &post[i + 2..]).filter_map(|r| r.find(')').map(|e| r[..e].to_string())).collect();
    assert!(refs.iter().any(|r| r.ends_with("pipeline-live.png")), "the hero is missing");
    for r in refs {
        assert!(root.join("docs").join(&r).exists(), "the announcement shows {r}, which is missing");
    }
}

#[test]
fn the_raven_logo_is_inlined_verbatim_and_themed() {
    // site/lib/logo.dart holds byte-for-byte copies of the delivered artwork.
    let dart = read("site/lib/logo.dart");
    for (name, file) in [("ravenTile", "site/web/logo/muninn-raven.svg"), ("ravenMark", "site/web/logo/muninn-raven-mark.svg")] {
        let svg = read(file);
        let body = dart
            .split_once(&format!("const {name} ="))
            .and_then(|(_, r)| r.trim_start().strip_prefix("r\'\'\'"))
            .and_then(|r| r.split_once("\'\'\';"))
            .map(|(b, _)| b);
        assert_eq!(body, Some(svg.as_str()), "{name} in logo.dart differs from {file}");
    }
    assert_eq!(read("site/web/favicon.svg"), read("site/web/logo/muninn-raven-small.svg"));
    assert_eq!(read("assets/icons/muninn.svg"), read("site/web/logo/muninn-raven.svg"), "the app icon is the same tile");
    let css = read("site/web/styles.css");
    for want in ["--muninn-header: var(--header)", "--muninn-accent: var(--accent)", "--muninn-bg: var(--bg)"] {
        assert!(css.contains(want), "styles.css lacks {want}");
    }
    assert!(read("site/lib/app.dart").contains("RawText(ravenTile)"), "the hero inlines the tile");
    assert!(read("site/tool/docs.dart").contains("$ravenMark"), "the docs header inlines the mark");
    let head = read("site/lib/main.server.dart");
    for f in ["favicon.svg", "favicon-16.png", "favicon-32.png", "apple-touch-icon.png"] {
        assert!(head.contains(f) && Path::new(env!("CARGO_MANIFEST_DIR")).join("site/web").join(f).exists(), "favicon {f}");
    }
}

#[test]
fn the_site_about_shows_publisher_homepage_version_and_mark() {
    let app = read("site/lib/app.dart");
    let ver = env!("CARGO_PKG_VERSION");
    for want in [
        format!("const version = '{ver}';"),
        "const homepage = 'https://muninn.weirdbrains.com';".into(),
        "published by WeirdBrains".into(),
        "RawText(ravenMark)".into(),
        "BSD 3-Clause".into(),
    ] {
        assert!(app.contains(&want), "site/lib/app.dart lacks {want}");
    }
}
