//! The shared themes (`assets/themes/*.toml`, plus any in `<home>/themes/`), resolved to hex roles.
use super::info::paths;
use muninn_core::prefs::{Preferences, ThemeMode};
use muninn_core::theme::{self, Variant};

/// One resolved colour role, e.g. `header` = `#7aa2f7`.
pub struct ThemeRole {
    pub role: String,
    pub hex: String,
}

/// A theme variant with every role resolved to `#rrggbb`.
pub struct ThemeData {
    pub name: String,
    pub display_name: String,
    pub variant: String,
    pub roles: Vec<ThemeRole>,
}

/// The configured theme and mode (`light`, `dark`, `system`) from preferences, read-only.
pub struct ThemeChoice {
    pub name: String,
    pub mode: String,
}

/// Theme names: built-ins first, then the user's own from `<home>/themes/`.
pub fn list_themes(home: Option<String>) -> anyhow::Result<Vec<String>> {
    Ok(theme::list(&paths(home)?)?)
}

/// Load one variant (`dark` or `light`) of a theme by name.
pub fn load_theme(home: Option<String>, name: String, variant: String) -> anyhow::Result<ThemeData> {
    let t = theme::load(&paths(home)?, &name)?;
    Ok(resolve(&t, &variant))
}

/// A built-in theme without touching any data dir (tests, first run).
#[flutter_rust_bridge::frb(sync)]
pub fn builtin_theme(name: String, variant: String) -> Option<ThemeData> {
    theme::builtin(&name).map(|t| resolve(&t, &variant))
}

/// The theme `mnnn` would use, from `preferences.toml` (never created or rewritten here).
pub fn current_theme(home: Option<String>) -> anyhow::Result<ThemeChoice> {
    let p = paths(home)?.preferences();
    let prefs: Preferences = match std::fs::read_to_string(&p) {
        Ok(s) => toml::from_str(&s).unwrap_or_default(),
        Err(_) => Preferences::default(),
    };
    let mode = match prefs.theme.mode {
        ThemeMode::Light => "light",
        ThemeMode::Dark => "dark",
        ThemeMode::System => "system",
    };
    Ok(ThemeChoice { name: prefs.theme.name, mode: mode.into() })
}

/// Both variants, every role resolved to `#rrggbb`, for Theme ▸ Edit / New.
pub struct ThemeDraft {
    pub name: String,
    pub display_name: String,
    /// Theme this one was copied from, or empty.
    pub base: String,
    pub dark: Vec<ThemeRole>,
    pub light: Vec<ThemeRole>,
}

fn draft_of(t: &theme::ThemeFile) -> ThemeDraft {
    ThemeDraft {
        name: t.meta.name.clone(),
        display_name: t.display_name(),
        base: t.meta.base.clone().unwrap_or_default(),
        dark: resolved_roles(t, "dark"),
        light: resolved_roles(t, "light"),
    }
}

fn resolved_roles(t: &theme::ThemeFile, variant: &str) -> Vec<ThemeRole> {
    let v = if variant == "light" { Variant::Light } else { Variant::Dark };
    let def = t.variant(v);
    theme::ROLES.iter().filter_map(|r| def.resolve(r).map(|c| ThemeRole { role: (*r).to_string(), hex: theme::to_hex(c) })).collect()
}

/// The theme as the editor shows it (built-in or `<home>/themes/`).
pub fn theme_draft(home: Option<String>, name: String) -> anyhow::Result<ThemeDraft> {
    Ok(draft_of(&theme::load(&paths(home)?, &name)?))
}

fn variant_of(roles: &[ThemeRole]) -> anyhow::Result<theme::VariantDef> {
    let mut v = theme::VariantDef::default();
    for r in roles {
        let hex = r.hex.trim();
        if theme::parse_hex(hex).is_none() {
            anyhow::bail!("role {} must be #rrggbb", r.role);
        }
        v.roles.insert(r.role.clone(), hex.to_ascii_lowercase());
    }
    Ok(v)
}

/// Write a theme under `<home>/themes/<name>/`. Both variants must list every role as `#rrggbb`.
pub fn theme_save_draft(
    home: Option<String>,
    name: String,
    display_name: String,
    base: String,
    dark: Vec<ThemeRole>,
    light: Vec<ThemeRole>,
) -> anyhow::Result<()> {
    let file = theme::ThemeFile {
        meta: theme::Meta {
            name,
            display_name: if display_name.is_empty() { None } else { Some(display_name) },
            author: None,
            base: if base.is_empty() { None } else { Some(base) },
        },
        dark: variant_of(&dark)?,
        light: variant_of(&light)?,
    };
    theme::save(&paths(home)?, &file)?;
    Ok(())
}

/// Theme ▸ Import…: parse a theme file and save it. Returns the theme name.
pub fn theme_import_toml(home: Option<String>, text: String) -> anyhow::Result<String> {
    let t = theme::ThemeFile::parse(&text)?;
    let name = t.meta.name.clone();
    theme::save(&paths(home)?, &t)?;
    Ok(name)
}

/// Theme ▸ Export…: the theme file as TOML. Bytes are written by the app.
pub fn theme_export_toml(home: Option<String>, name: String) -> anyhow::Result<String> {
    Ok(theme::load(&paths(home)?, &name)?.to_toml()?)
}

pub(crate) fn resolve(t: &theme::ThemeFile, variant: &str) -> ThemeData {
    let v = if variant == "light" { Variant::Light } else { Variant::Dark };
    let def = t.variant(v);
    let roles = def
        .roles
        .keys()
        .filter(|k| k.as_str() != "variant_name")
        .filter_map(|k| def.resolve(k).map(|c| ThemeRole { role: k.clone(), hex: theme::to_hex(c) }))
        .collect();
    ThemeData { name: t.meta.name.clone(), display_name: t.display_name(), variant: v.name().into(), roles }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_resolves_both_variants() {
        let home = tempfile::tempdir().unwrap();
        let names = list_themes(Some(home.path().display().to_string())).unwrap();
        assert!(names.len() >= 7);
        for n in names {
            for v in ["dark", "light"] {
                let t = builtin_theme(n.clone(), v.into()).unwrap();
                for r in ["bg", "fg", "header", "accent", "border", "selection_bg"] {
                    assert!(t.roles.iter().any(|x| x.role == r && x.hex.len() == 7), "{n} {v} {r}");
                }
            }
        }
        assert!(!home.path().join("themes").exists(), "listing must not write");
    }

    #[test]
    fn tokyo_night_dark_header() {
        let t = builtin_theme("tokyo-night".into(), "dark".into()).unwrap();
        assert_eq!(t.roles.iter().find(|r| r.role == "header").unwrap().hex, "#7aa2f7");
        assert_eq!(t.roles.iter().find(|r| r.role == "bg").unwrap().hex, "#1a1b26");
    }

    #[test]
    fn current_theme_defaults_without_prefs() {
        let home = tempfile::tempdir().unwrap();
        let c = current_theme(Some(home.path().display().to_string())).unwrap();
        assert_eq!(c.name, "tokyo-night");
        assert!(!home.path().join("preferences.toml").exists());
    }

    #[test]
    fn draft_round_trip_saves_a_copy() {
        let home = tempfile::tempdir().unwrap();
        let h = Some(home.path().display().to_string());
        let d = theme_draft(h.clone(), "nord".into()).unwrap();
        let bg = d.dark.iter().find(|r| r.role == "bg").unwrap().hex.clone();
        assert!(bg.starts_with('#'));
        theme_save_draft(h.clone(), "nord-mine".into(), "Nord mine".into(), "nord".into(), d.dark, d.light).unwrap();
        let again = theme_draft(h.clone(), "nord-mine".into()).unwrap();
        assert_eq!(again.base, "nord");
        assert_eq!(again.dark.iter().find(|r| r.role == "bg").unwrap().hex, bg);
        let toml = theme_export_toml(h.clone(), "nord-mine".into()).unwrap();
        let imported = theme_import_toml(h, toml.replace("nord-mine", "nord-copy")).unwrap();
        assert_eq!(imported, "nord-copy");
    }
}
