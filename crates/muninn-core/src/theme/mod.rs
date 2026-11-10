//! Themes: the `weirdnet-themes` crate (format, roles, built-ins, contrast checks, ratatui
//! colours), re-exported, plus Muninn's adapters: its folder layout (`impl ThemeDirs for Paths`
//! in `crate::paths`), `prefs::ThemeMode` converting into [`Mode`], and `crate::Error` results.
pub use weirdnet_themes::{
    BUILTINS, ColorDepth, DEFAULT_THEME, MIN_CONTRAST, Meta, Mode, PlainDir, ROLES, THEME_FILE, Theme, ThemeDirs, ThemeError, ThemeFile, Variant,
    VariantDef, active, builtin, contrast, contrast_warnings, detect, luminance, nearest_256, parse_hex, resolve_variant, to_color, to_hex,
};

use crate::error::Result;

/// Write built-in themes to `themes/<name>/` when missing.
pub fn materialize(paths: &(impl ThemeDirs + ?Sized)) -> Result<()> {
    Ok(weirdnet_themes::materialize(paths)?)
}

/// Restore the shipped version of a built-in theme.
pub fn reset(paths: &(impl ThemeDirs + ?Sized), name: &str) -> Result<()> {
    Ok(weirdnet_themes::reset(paths, name)?)
}

pub fn load(paths: &(impl ThemeDirs + ?Sized), name: &str) -> Result<ThemeFile> {
    Ok(weirdnet_themes::load(paths, name)?)
}

pub fn save(paths: &(impl ThemeDirs + ?Sized), t: &ThemeFile) -> Result<()> {
    Ok(weirdnet_themes::save(paths, t)?)
}

/// Theme folder names, built-ins first in their fixed order.
pub fn list(paths: &(impl ThemeDirs + ?Sized)) -> Result<Vec<String>> {
    Ok(weirdnet_themes::list(paths)?)
}

pub fn valid_name(n: &str) -> Result<()> {
    Ok(weirdnet_themes::valid_name(n)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Paths;

    /// Muninn's own layout (`<home>/themes/<name>/{assets,interface.toml}`) through the crate.
    #[test]
    fn materialize_and_list_in_the_muninn_home() {
        let tmp = tempfile::tempdir().unwrap();
        let p = Paths::resolve(Some(tmp.path().to_path_buf()), None).unwrap();
        p.ensure().unwrap();
        materialize(&p).unwrap();
        assert!(p.theme_dir("nord").join("assets").is_dir());
        assert!(tmp.path().join("themes/nord/interface.toml").is_file());
        let mut t = load(&p, "nord").unwrap();
        t.meta.name = "my-nord".into();
        save(&p, &t).unwrap();
        assert!(list(&p).unwrap().contains(&"my-nord".to_string()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&p.theme_dir("my-nord").join(THEME_FILE)), 0o600);
            assert_eq!(mode(&p.theme_dir("my-nord")), 0o700);
        }
    }

    /// Crate errors keep Muninn's messages (`crate::Error::Invalid`).
    #[test]
    fn errors_convert_unchanged() {
        let e = valid_name("a b").unwrap_err();
        assert!(matches!(e, crate::Error::Invalid { what: "theme name", .. }));
        assert_eq!(e.to_string(), "invalid theme name: `a b` (use letters, digits, - and _)");
    }

    #[test]
    fn preference_mode_converts() {
        use crate::prefs::ThemeMode;
        assert_eq!(resolve_variant(ThemeMode::Dark), Variant::Dark);
        assert_eq!(resolve_variant(ThemeMode::Light), Variant::Light);
        assert_eq!(Mode::from(ThemeMode::System), Mode::System);
    }
}
