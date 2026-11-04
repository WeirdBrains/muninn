//! About Muninn: the single source for the TUI About box, `mnnn --version` neighbours,
//! the Flutter app's About dialog (over FFI) and the website.

pub const NAME: &str = "Muninn";
pub const BINARY: &str = "mnnn";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const TAGLINE: &str = "Local, encrypted store for OHLCV market bars.";
pub const MOTTO: &str = "Odin's raven of memory: it remembers every bar.";
pub const PUBLISHER: &str = "WeirdBrains";
pub const HOMEPAGE: &str = "https://muninn.weirdbrains.com";
pub const REPOSITORY: &str = "https://github.com/WeirdBrains/muninn";
pub const LICENSE: &str = "BSD-3-Clause";
pub const AUTHOR: &str = "Michael Christenson II";
pub const COPYRIGHT: &str = "© 2026 Michael Christenson II";
/// Third-party credits shown in About (bundled fonts and theme palettes).
pub const CREDITS: &[&str] =
    &["Liberation fonts (SIL Open Font License) for PDF exports", "Theme palettes: Catppuccin, Dracula, Gruvbox, Nord, One, Solarized, Tokyo Night"];

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
}
