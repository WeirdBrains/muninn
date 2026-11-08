//! About Muninn: the single source for the TUI About box, `mnnn --version` neighbours,
//! the Flutter app's About dialog (over FFI) and the website.

pub const NAME: &str = "Muninn";
pub const BINARY: &str = "mnnn";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const TAGLINE: &str = "Data goes in, your magic transforms it.";
pub const MOTTO: &str = "Odin's raven of memory: it remembers every bar.";
pub const PUBLISHER: &str = "WeirdBrains";
pub const HOMEPAGE: &str = "https://muninn.weirdbrains.com";
pub const REPOSITORY: &str = "https://github.com/WeirdBrains/muninn";
pub const LICENSE: &str = "BSD-3-Clause";
pub const AUTHOR: &str = "Michael Christenson II";
/// As in LICENSE (BSD-3-Clause).
pub const COPYRIGHT: &str = "Copyright (c) 2026 Weird Brains";
/// Third-party credits shown in About (bundled fonts and theme palettes).
pub const CREDITS: &[&str] = &[
    "Liberation Mono and Liberation Serif fonts for PDF exports: Copyright (c) 2012 Red Hat, Inc., digitized data (c) 2010 Google Corporation; SIL Open Font License 1.1",
    "Theme palettes (MIT): Catppuccin (Catppuccin), Dracula (Zeno Rocha), Gruvbox (morhetz), Nord (Arctic Ice Studio), One (Atom), Solarized (Ethan Schoonover), Tokyo Night (enkia, folke)",
];

/// The About text as lines (TUI About box; the Flutter dialog lays out the same fields).
pub fn lines() -> Vec<String> {
    vec![
        format!("{NAME} v{VERSION}"),
        String::new(),
        TAGLINE.into(),
        MOTTO.into(),
        String::new(),
        "Help: ? or F1 · Menus: F10 or Alt+letter · Quit: q".into(),
    ]
}

#[cfg(test)]
mod tests {
    #[test]
    fn about_starts_with_name_and_version() {
        assert_eq!(super::lines()[0], format!("Muninn v{}", env!("CARGO_PKG_VERSION")));
        assert!(super::HOMEPAGE.starts_with("https://"));
    }

    #[test]
    fn about_matches_the_license_and_the_bundled_assets() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../");
        let read = |p: &str| std::fs::read_to_string(format!("{root}{p}")).unwrap();
        assert!(read("LICENSE").contains("Copyright (c) 2026, Weird Brains"));
        assert_eq!((super::LICENSE, super::PUBLISHER, super::HOMEPAGE), ("BSD-3-Clause", "WeirdBrains", "https://muninn.weirdbrains.com"));
        assert!(super::COPYRIGHT.contains("2026 Weird Brains"));
        let ofl = read("assets/fonts/LICENSE-OFL.txt");
        assert!(ofl.contains("SIL OPEN FONT LICENSE Version 1.1") && ofl.contains("Red Hat, Inc.") && ofl.contains("Google Corporation"));
        assert!(super::CREDITS[0].contains("Open Font License 1.1") && super::CREDITS[0].contains("Red Hat"));
        for t in std::fs::read_dir(format!("{root}assets/themes")).unwrap() {
            let t: toml::Value = toml::from_str(&std::fs::read_to_string(t.unwrap().path()).unwrap()).unwrap();
            let name = t["meta"]["display_name"].as_str().unwrap();
            assert!(super::CREDITS[1].contains(name), "About credits lack the {name} palette");
        }
    }
}
