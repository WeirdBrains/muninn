//! Themes: `<themes dir>/<name>/interface.toml` with `[meta]`, `[dark]` and `[light]` variants.
//!
//! Brand- and app-free: the caller supplies where themes live ([`ThemeDirs`]) and its own
//! preference type converts into [`Mode`]. See the README for the features.
#![forbid(unsafe_code)]

pub mod detect;
mod error;
mod term;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub use error::{Result, ThemeError};
pub use term::ColorDepth;
#[cfg(feature = "ratatui")]
pub use term::{Theme, to_color};

/// The file holding one theme inside its folder.
pub const THEME_FILE: &str = "interface.toml";

/// Where themes live and how they are written. The app implements it, so this crate owns no
/// home-directory layout or file-permission policy.
pub trait ThemeDirs {
    /// The folder holding one `<name>/interface.toml` per theme.
    fn themes_dir(&self) -> PathBuf;
    fn theme_dir(&self, name: &str) -> PathBuf {
        self.themes_dir().join(name)
    }
    /// Create a folder (private where the platform supports it).
    fn ensure_dir(&self, dir: &Path) -> std::io::Result<()>;
    /// Write a file (private where the platform supports it).
    fn write_private(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()>;
}

/// A plain folder: `std::fs` with the platform's default permissions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlainDir(pub PathBuf);

impl ThemeDirs for PlainDir {
    fn themes_dir(&self) -> PathBuf {
        self.0.clone()
    }
    fn ensure_dir(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)
    }
    fn write_private(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        std::fs::write(path, bytes)
    }
}

/// Light, dark, or follow the system. The app's preference type converts into it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Mode {
    Light,
    Dark,
    #[default]
    System,
}

/// Semantic roles every variant must define.
pub const ROLES: [&str; 21] = [
    "bg",
    "fg",
    "muted",
    "border",
    "border_focus",
    "selection_bg",
    "selection_fg",
    "accent",
    "header",
    "up",
    "down",
    "warning",
    "error",
    "success",
    "info",
    "chart_1",
    "chart_2",
    "chart_3",
    "chart_4",
    "chart_5",
    "chart_6",
];

pub const DEFAULT_THEME: &str = "tokyo-night";

/// Built-in themes compiled into the binary.
pub const BUILTINS: &[(&str, &str)] = &[
    ("tokyo-night", include_str!("../themes/tokyo-night.toml")),
    ("catppuccin", include_str!("../themes/catppuccin.toml")),
    ("gruvbox", include_str!("../themes/gruvbox.toml")),
    ("dracula", include_str!("../themes/dracula.toml")),
    ("nord", include_str!("../themes/nord.toml")),
    ("one", include_str!("../themes/one.toml")),
    ("solarized", include_str!("../themes/solarized.toml")),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Dark,
    Light,
}

impl Variant {
    pub fn toggle(self) -> Self {
        match self {
            Variant::Dark => Variant::Light,
            Variant::Light => Variant::Dark,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Variant::Dark => "dark",
            Variant::Light => "light",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    pub name: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub base: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct VariantDef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant_name: Option<String>,
    /// role -> palette name or `#rrggbb`
    #[serde(flatten)]
    pub roles: BTreeMap<String, String>,
    #[serde(default)]
    pub palette: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThemeFile {
    pub meta: Meta,
    pub dark: VariantDef,
    pub light: VariantDef,
}

pub fn parse_hex(s: &str) -> Option<(u8, u8, u8)> {
    let h = s.trim().strip_prefix('#')?;
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some(((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

pub fn to_hex(c: (u8, u8, u8)) -> String {
    format!("#{:02x}{:02x}{:02x}", c.0, c.1, c.2)
}

impl VariantDef {
    /// Resolve a role (palette reference or literal hex).
    pub fn resolve(&self, role: &str) -> Option<(u8, u8, u8)> {
        let v = self.roles.get(role)?;
        parse_hex(v).or_else(|| self.palette.get(v).and_then(|h| parse_hex(h)))
    }

    /// Distinct colors offered by the editor: the palette plus any literal role values.
    pub fn swatches(&self) -> Vec<(String, String)> {
        let mut v: Vec<(String, String)> = self.palette.iter().map(|(k, h)| (k.clone(), h.clone())).collect();
        for val in self.roles.values() {
            if parse_hex(val).is_some() && !v.iter().any(|(_, h)| h.eq_ignore_ascii_case(val)) {
                v.push((val.clone(), val.clone()));
            }
        }
        v
    }
}

impl ThemeFile {
    pub fn parse(text: &str) -> Result<Self> {
        let t: ThemeFile = toml::from_str(text).map_err(|e| {
            let msg = e.to_string();
            if msg.contains("missing field `light`") || msg.contains("missing field `dark`") {
                ThemeError::invalid("theme", format!("every theme needs both [dark] and [light] tables ({})", msg.trim()))
            } else {
                ThemeError::invalid("theme", msg)
            }
        })?;
        t.validate()?;
        Ok(t)
    }

    pub fn validate(&self) -> Result<()> {
        valid_name(&self.meta.name)?;
        for (vn, v) in [("dark", &self.dark), ("light", &self.light)] {
            for r in ROLES {
                if !v.roles.contains_key(r) {
                    return Err(ThemeError::invalid("theme", format!("[{vn}] is missing role `{r}`")));
                }
                if v.resolve(r).is_none() {
                    return Err(ThemeError::invalid("theme", format!("[{vn}] role `{r}` = `{}` is neither #rrggbb nor a palette name", v.roles[r])));
                }
            }
            for k in v.roles.keys() {
                if !ROLES.contains(&k.as_str()) {
                    return Err(ThemeError::invalid("theme", format!("[{vn}] has unknown role `{k}`")));
                }
            }
        }
        Ok(())
    }

    pub fn to_toml(&self) -> Result<String> {
        Ok(toml::to_string_pretty(self)?)
    }

    pub fn variant(&self, v: Variant) -> &VariantDef {
        match v {
            Variant::Dark => &self.dark,
            Variant::Light => &self.light,
        }
    }

    pub fn variant_mut(&mut self, v: Variant) -> &mut VariantDef {
        match v {
            Variant::Dark => &mut self.dark,
            Variant::Light => &mut self.light,
        }
    }

    pub fn display_name(&self) -> String {
        self.meta.display_name.clone().unwrap_or_else(|| self.meta.name.clone())
    }

    /// Generate a starting variant from the other one by inverting lightness of every color.
    pub fn generate_variant(&mut self, target: Variant) {
        let src = self.variant(target.toggle()).clone();
        let mut out = VariantDef { variant_name: Some(format!("{} (generated)", target.name())), ..Default::default() };
        for r in ROLES {
            if let Some(c) = src.resolve(r) {
                out.roles.insert(r.to_string(), to_hex(invert_lightness(c)));
            }
        }
        *self.variant_mut(target) = out;
    }

    #[cfg(feature = "ratatui")]
    pub fn resolved(&self, v: Variant) -> Theme {
        Theme::from_variant(self.variant(v))
    }
}

pub fn valid_name(n: &str) -> Result<()> {
    if n.is_empty() || !n.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(ThemeError::invalid("theme name", format!("`{n}` (use letters, digits, - and _)")));
    }
    Ok(())
}

fn invert_lightness((r, g, b): (u8, u8, u8)) -> (u8, u8, u8) {
    // Keep hue roughly, flip lightness: c' = 255 - c shifted toward the original hue.
    let l = (r as u16 + g as u16 + b as u16) / 3;
    let shift = 255 - 2 * l as i32;
    let f = |c: u8| (c as i32 + shift).clamp(0, 255) as u8;
    (f(r), f(g), f(b))
}

/// WCAG relative luminance.
pub fn luminance((r, g, b): (u8, u8, u8)) -> f64 {
    let ch = |c: u8| {
        let c = c as f64 / 255.0;
        if c <= 0.03928 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * ch(r) + 0.7152 * ch(g) + 0.0722 * ch(b)
}

/// WCAG contrast ratio (1..21).
pub fn contrast(a: (u8, u8, u8), b: (u8, u8, u8)) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

pub const MIN_CONTRAST: f64 = 4.5;

/// Contrast warnings for a variant (fg/bg pairs).
pub fn contrast_warnings(v: &VariantDef) -> Vec<String> {
    let pairs = [("fg", "bg"), ("selection_fg", "selection_bg"), ("header", "bg")];
    let mut out = vec![];
    for (f, b) in pairs {
        if let (Some(fc), Some(bc)) = (v.resolve(f), v.resolve(b)) {
            let c = contrast(fc, bc);
            if c < MIN_CONTRAST {
                out.push(format!("{f}/{b} contrast {c:.2}:1 is below {MIN_CONTRAST}:1"));
            }
        }
    }
    out
}

#[cfg_attr(not(feature = "ratatui"), allow(dead_code))]
pub(crate) fn dist(a: (u8, u8, u8), b: (u8, u8, u8)) -> i32 {
    let d = |x: u8, y: u8| (x as i32 - y as i32).pow(2);
    d(a.0, b.0) + d(a.1, b.1) + d(a.2, b.2)
}

/// Nearest xterm-256 index.
pub fn nearest_256(c: (u8, u8, u8)) -> u8 {
    let levels = [0u8, 95, 135, 175, 215, 255];
    let idx = |v: u8| levels.iter().enumerate().min_by_key(|(_, l)| (v as i32 - **l as i32).abs()).map(|(i, _)| i).unwrap_or(0);
    let (ri, gi, bi) = (idx(c.0), idx(c.1), idx(c.2));
    let cube = (levels[ri], levels[gi], levels[bi]);
    let cube_i = 16 + 36 * ri + 6 * gi + bi;
    let avg = (c.0 as u32 + c.1 as u32 + c.2 as u32) / 3;
    let gi2 = (((avg as i32 - 8).max(0)) / 10).min(23) as u8;
    let gv = 8 + 10 * gi2;
    if dist(c, (gv, gv, gv)) < dist(c, cube) { 232 + gi2 } else { cube_i as u8 }
}

pub fn builtin(name: &str) -> Option<ThemeFile> {
    BUILTINS.iter().find(|(n, _)| *n == name).and_then(|(_, t)| ThemeFile::parse(t).ok())
}

/// Write built-in themes to `themes/<name>/` when missing.
pub fn materialize(paths: &(impl ThemeDirs + ?Sized)) -> Result<()> {
    for (name, text) in BUILTINS {
        let dir = paths.theme_dir(name);
        if !dir.join(THEME_FILE).exists() {
            paths.ensure_dir(&dir)?;
            paths.ensure_dir(&dir.join("assets"))?;
            paths.write_private(&dir.join(THEME_FILE), text.as_bytes())?;
        }
    }
    Ok(())
}

/// Restore the shipped version of a built-in theme.
pub fn reset(paths: &(impl ThemeDirs + ?Sized), name: &str) -> Result<()> {
    let (_, text) =
        BUILTINS.iter().find(|(n, _)| *n == name).ok_or_else(|| ThemeError::invalid("theme", format!("`{name}` is not a built-in theme")))?;
    let dir = paths.theme_dir(name);
    paths.ensure_dir(&dir)?;
    paths.ensure_dir(&dir.join("assets"))?;
    paths.write_private(&dir.join(THEME_FILE), text.as_bytes())?;
    Ok(())
}

pub fn load(paths: &(impl ThemeDirs + ?Sized), name: &str) -> Result<ThemeFile> {
    let p = paths.theme_dir(name).join(THEME_FILE);
    if !p.exists() {
        return builtin(name).ok_or_else(|| ThemeError::invalid("theme", format!("`{name}` not found in {}", paths.themes_dir().display())));
    }
    ThemeFile::parse(&std::fs::read_to_string(&p)?).map_err(|e| ThemeError::invalid("theme", format!("{}: {e}", p.display())))
}

pub fn save(paths: &(impl ThemeDirs + ?Sized), t: &ThemeFile) -> Result<()> {
    t.validate()?;
    let dir = paths.theme_dir(&t.meta.name);
    paths.ensure_dir(&dir)?;
    paths.ensure_dir(&dir.join("assets"))?;
    paths.write_private(&dir.join(THEME_FILE), t.to_toml()?.as_bytes())?;
    Ok(())
}

/// Theme folder names, built-ins first in their fixed order.
pub fn list(paths: &(impl ThemeDirs + ?Sized)) -> Result<Vec<String>> {
    let mut v: Vec<String> = BUILTINS.iter().map(|(n, _)| n.to_string()).collect();
    let mut extra = vec![];
    if let Ok(rd) = std::fs::read_dir(paths.themes_dir()) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if e.path().join(THEME_FILE).exists() && !v.contains(&n) {
                extra.push(n);
            }
        }
    }
    extra.sort();
    v.extend(extra);
    Ok(v)
}

/// Resolve the active variant from a mode.
pub fn resolve_variant(mode: impl Into<Mode>) -> Variant {
    match mode.into() {
        Mode::Dark => Variant::Dark,
        Mode::Light => Variant::Light,
        Mode::System => detect::system_variant(),
    }
}

/// Load the active theme; falls back to the default if the configured one is broken.
pub fn active(paths: &(impl ThemeDirs + ?Sized), name: &str) -> (ThemeFile, Option<String>) {
    match load(paths, name) {
        Ok(t) => (t, None),
        Err(e) => (builtin(DEFAULT_THEME).expect("default theme is valid"), Some(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_are_valid_and_complete() {
        assert_eq!(BUILTINS.len(), 7);
        for (n, t) in BUILTINS {
            let th = ThemeFile::parse(t).unwrap_or_else(|e| panic!("{n}: {e}"));
            assert_eq!(&th.meta.name, n);
            for v in [Variant::Dark, Variant::Light] {
                let var = th.variant(v);
                let fg = var.resolve("fg").unwrap();
                let bg = var.resolve("bg").unwrap();
                // Solarized Light's published body text (base00 on base3) is 4.13:1; we keep the
                // real palette and let the contrast check warn about it.
                let min = if *n == "solarized" && v == Variant::Light { 4.0 } else { MIN_CONTRAST };
                assert!(contrast(fg, bg) >= min, "{n} {:?} fg/bg contrast {}", v, contrast(fg, bg));
                // dark variants are dark, light are light
                assert_eq!(luminance(bg) < 0.2, v == Variant::Dark, "{n} {v:?}");
            }
        }
    }

    #[test]
    fn known_palette_values() {
        let t = builtin("tokyo-night").unwrap();
        assert_eq!(t.dark.resolve("bg"), parse_hex("#1a1b26"));
        assert_eq!(t.light.resolve("bg"), parse_hex("#e1e2e7"));
        let d = builtin("dracula").unwrap();
        assert_eq!(d.dark.resolve("accent"), parse_hex("#ff79c6"));
        assert_eq!(builtin("solarized").unwrap().light.resolve("bg"), parse_hex("#fdf6e3"));
    }

    #[test]
    fn missing_variant_fails() {
        let t = BUILTINS[0].1;
        let only_dark = &t[..t.find("[light]").unwrap()];
        let e = ThemeFile::parse(only_dark).unwrap_err().to_string();
        assert!(e.contains("[dark] and [light]"), "{e}");
        let broken = t.replacen("bg = \"bg\"", "bg = \"nope\"", 1);
        assert!(ThemeFile::parse(&broken).is_err());
    }

    #[test]
    fn contrast_math() {
        assert!((contrast((0, 0, 0), (255, 255, 255)) - 21.0).abs() < 0.01);
        assert!((contrast((10, 10, 10), (10, 10, 10)) - 1.0).abs() < 1e-9);
        assert_eq!(nearest_256((255, 0, 0)), 196);
        assert_eq!(nearest_256((0, 0, 0)), 16);
    }

    #[test]
    fn generate_and_round_trip() {
        let mut t = builtin("gruvbox").unwrap();
        t.generate_variant(Variant::Light);
        t.validate().unwrap();
        let s = t.to_toml().unwrap();
        assert_eq!(ThemeFile::parse(&s).unwrap(), t);
    }

    #[test]
    fn plain_dir_materialize_list_load_save() {
        let tmp = tempfile::tempdir().unwrap();
        let d = PlainDir(tmp.path().join("t"));
        materialize(&d).unwrap();
        assert!(d.theme_dir("nord").join("assets").is_dir());
        assert_eq!(list(&d).unwrap(), BUILTINS.iter().map(|(n, _)| n.to_string()).collect::<Vec<_>>());
        // a materialized built-in is byte-for-byte the shipped file
        assert_eq!(std::fs::read_to_string(d.theme_dir("nord").join(THEME_FILE)).unwrap(), BUILTINS.iter().find(|(n, _)| *n == "nord").unwrap().1);
        let mut t = load(&d, "nord").unwrap();
        t.meta.name = "my-nord".into();
        save(&d, &t).unwrap();
        assert!(list(&d).unwrap().contains(&"my-nord".to_string()));
        assert_eq!(load(&d, "my-nord").unwrap(), t);
        assert_eq!(resolve_variant(Mode::Light), Variant::Light);
        assert_eq!(resolve_variant(Mode::Dark), Variant::Dark);
    }

    #[test]
    fn external_theme_errors_and_fallback() {
        let tmp = tempfile::tempdir().unwrap();
        let d = PlainDir(tmp.path().join("t"));
        let e = load(&d, "nope").unwrap_err().to_string();
        assert!(e.starts_with("invalid theme: `nope` not found in "), "{e}");
        d.ensure_dir(&d.theme_dir("bad")).unwrap();
        std::fs::write(d.theme_dir("bad").join(THEME_FILE), "[meta]\nname = \"bad\"\n").unwrap();
        let e = load(&d, "bad").unwrap_err().to_string();
        assert!(e.contains("every theme needs both [dark] and [light] tables"), "{e}");
        let (t, warn) = active(&d, "bad");
        assert_eq!(t.meta.name, DEFAULT_THEME);
        assert!(warn.unwrap().contains("bad"));
        assert_eq!(reset(&d, "my-own").unwrap_err().to_string(), "invalid theme: `my-own` is not a built-in theme");
        assert_eq!(valid_name("a b").unwrap_err().to_string(), "invalid theme name: `a b` (use letters, digits, - and _)");
        let mut t = builtin("one").unwrap();
        t.dark.roles.remove("fg");
        assert_eq!(save(&d, &t).unwrap_err().to_string(), "invalid theme: [dark] is missing role `fg`");
    }

    #[cfg(feature = "ratatui")]
    #[test]
    fn ratatui_colours_by_depth() {
        use ratatui::style::Color;
        let v = &builtin("tokyo-night").unwrap().dark;
        assert_eq!(Theme::from_variant_depth(v, ColorDepth::TrueColor).bg, Color::Rgb(0x1a, 0x1b, 0x26));
        assert!(matches!(Theme::from_variant_depth(v, ColorDepth::Ansi256).bg, Color::Indexed(_)));
        assert_eq!(to_color((255, 0, 0), ColorDepth::Ansi16), Color::Indexed(9));
    }
}
