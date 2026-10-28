//! Themes: `themes/<name>/interface.toml` with `[meta]`, `[dark]` and `[light]` variants.
pub mod detect;

use crate::error::{Error, Result};
use crate::paths::{ensure_dir, write_private, Paths};
use crate::prefs::ThemeMode;
use ratatui::style::Color;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Semantic roles every variant must define.
pub const ROLES: [&str; 21] = [
    "bg", "fg", "muted", "border", "border_focus", "selection_bg", "selection_fg", "accent", "header", "up", "down",
    "warning", "error", "success", "info", "chart_1", "chart_2", "chart_3", "chart_4", "chart_5", "chart_6",
];

pub const DEFAULT_THEME: &str = "tokyo-night";

/// Built-in themes compiled into the binary.
pub const BUILTINS: &[(&str, &str)] = &[
    ("tokyo-night", include_str!("../../assets/themes/tokyo-night.toml")),
    ("catppuccin", include_str!("../../assets/themes/catppuccin.toml")),
    ("gruvbox", include_str!("../../assets/themes/gruvbox.toml")),
    ("dracula", include_str!("../../assets/themes/dracula.toml")),
    ("nord", include_str!("../../assets/themes/nord.toml")),
    ("one", include_str!("../../assets/themes/one.toml")),
    ("solarized", include_str!("../../assets/themes/solarized.toml")),
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
                Error::invalid("theme", format!("every theme needs both [dark] and [light] tables ({})", msg.trim()))
            } else {
                Error::invalid("theme", msg)
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
                    return Err(Error::invalid("theme", format!("[{vn}] is missing role `{r}`")));
                }
                if v.resolve(r).is_none() {
                    return Err(Error::invalid(
                        "theme",
                        format!("[{vn}] role `{r}` = `{}` is neither #rrggbb nor a palette name", v.roles[r]),
                    ));
                }
            }
            for k in v.roles.keys() {
                if !ROLES.contains(&k.as_str()) {
                    return Err(Error::invalid("theme", format!("[{vn}] has unknown role `{k}`")));
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

    pub fn resolved(&self, v: Variant) -> Theme {
        Theme::from_variant(self.variant(v))
    }
}

pub fn valid_name(n: &str) -> Result<()> {
    if n.is_empty() || !n.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(Error::invalid("theme name", format!("`{n}` (use letters, digits, - and _)")));
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

/// Terminal color depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorDepth {
    TrueColor,
    Ansi256,
    Ansi16,
}

impl ColorDepth {
    pub fn detect() -> Self {
        let ct = std::env::var("COLORTERM").unwrap_or_default().to_ascii_lowercase();
        if ct.contains("truecolor") || ct.contains("24bit") {
            return ColorDepth::TrueColor;
        }
        let term = std::env::var("TERM").unwrap_or_default();
        if term == "linux" || term.contains("16color") || term == "xterm" || term == "vt100" {
            ColorDepth::Ansi16
        } else if term.contains("256") {
            ColorDepth::Ansi256
        } else {
            ColorDepth::TrueColor
        }
    }
}

const ANSI16: [(u8, u8, u8); 16] = [
    (0, 0, 0), (128, 0, 0), (0, 128, 0), (128, 128, 0), (0, 0, 128), (128, 0, 128), (0, 128, 128), (192, 192, 192),
    (128, 128, 128), (255, 0, 0), (0, 255, 0), (255, 255, 0), (0, 0, 255), (255, 0, 255), (0, 255, 255), (255, 255, 255),
];

fn dist(a: (u8, u8, u8), b: (u8, u8, u8)) -> i32 {
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

pub fn to_color(c: (u8, u8, u8), depth: ColorDepth) -> Color {
    match depth {
        ColorDepth::TrueColor => Color::Rgb(c.0, c.1, c.2),
        ColorDepth::Ansi256 => Color::Indexed(nearest_256(c)),
        ColorDepth::Ansi16 => {
            let i = ANSI16.iter().enumerate().min_by_key(|(_, a)| dist(c, **a)).map(|(i, _)| i).unwrap_or(7);
            Color::Indexed(i as u8)
        }
    }
}

/// A resolved variant ready for rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub bg: Color,
    pub fg: Color,
    pub muted: Color,
    pub border: Color,
    pub border_focus: Color,
    pub selection_bg: Color,
    pub selection_fg: Color,
    pub accent: Color,
    pub header: Color,
    pub up: Color,
    pub down: Color,
    pub warning: Color,
    pub error: Color,
    pub success: Color,
    pub info: Color,
    pub charts: [Color; 6],
}

impl Theme {
    pub fn from_variant(v: &VariantDef) -> Self {
        Self::from_variant_depth(v, ColorDepth::detect())
    }

    pub fn from_variant_depth(v: &VariantDef, depth: ColorDepth) -> Self {
        let c = |r: &str| to_color(v.resolve(r).unwrap_or((128, 128, 128)), depth);
        Theme {
            bg: c("bg"),
            fg: c("fg"),
            muted: c("muted"),
            border: c("border"),
            border_focus: c("border_focus"),
            selection_bg: c("selection_bg"),
            selection_fg: c("selection_fg"),
            accent: c("accent"),
            header: c("header"),
            up: c("up"),
            down: c("down"),
            warning: c("warning"),
            error: c("error"),
            success: c("success"),
            info: c("info"),
            charts: [c("chart_1"), c("chart_2"), c("chart_3"), c("chart_4"), c("chart_5"), c("chart_6")],
        }
    }
}

pub fn builtin(name: &str) -> Option<ThemeFile> {
    BUILTINS.iter().find(|(n, _)| *n == name).and_then(|(_, t)| ThemeFile::parse(t).ok())
}

/// Write built-in themes to `themes/<name>/` when missing.
pub fn materialize(paths: &Paths) -> Result<()> {
    for (name, text) in BUILTINS {
        let dir = paths.theme_dir(name);
        if !dir.join("interface.toml").exists() {
            ensure_dir(&dir)?;
            ensure_dir(&dir.join("assets"))?;
            write_private(&dir.join("interface.toml"), text.as_bytes())?;
        }
    }
    Ok(())
}

/// Restore the shipped version of a built-in theme.
pub fn reset(paths: &Paths, name: &str) -> Result<()> {
    let (_, text) = BUILTINS
        .iter()
        .find(|(n, _)| *n == name)
        .ok_or_else(|| Error::invalid("theme", format!("`{name}` is not a built-in theme")))?;
    let dir = paths.theme_dir(name);
    ensure_dir(&dir)?;
    ensure_dir(&dir.join("assets"))?;
    write_private(&dir.join("interface.toml"), text.as_bytes())
}

pub fn load(paths: &Paths, name: &str) -> Result<ThemeFile> {
    let p = paths.theme_dir(name).join("interface.toml");
    if !p.exists() {
        return builtin(name).ok_or_else(|| Error::invalid("theme", format!("`{name}` not found in {}", paths.themes_dir().display())));
    }
    ThemeFile::parse(&std::fs::read_to_string(&p)?)
        .map_err(|e| Error::invalid("theme", format!("{}: {e}", p.display())))
}

pub fn save(paths: &Paths, t: &ThemeFile) -> Result<()> {
    t.validate()?;
    let dir = paths.theme_dir(&t.meta.name);
    ensure_dir(&dir)?;
    ensure_dir(&dir.join("assets"))?;
    write_private(&dir.join("interface.toml"), t.to_toml()?.as_bytes())
}

/// Theme folder names, built-ins first in their fixed order.
pub fn list(paths: &Paths) -> Result<Vec<String>> {
    let mut v: Vec<String> = BUILTINS.iter().map(|(n, _)| n.to_string()).collect();
    let mut extra = vec![];
    if let Ok(rd) = std::fs::read_dir(paths.themes_dir()) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if e.path().join("interface.toml").exists() && !v.contains(&n) {
                extra.push(n);
            }
        }
    }
    extra.sort();
    v.extend(extra);
    Ok(v)
}

/// Resolve the active variant from a mode.
pub fn resolve_variant(mode: ThemeMode) -> Variant {
    match mode {
        ThemeMode::Dark => Variant::Dark,
        ThemeMode::Light => Variant::Light,
        ThemeMode::System => detect::system_variant(),
    }
}

/// Load the active theme; falls back to the default if the configured one is broken.
pub fn active(paths: &Paths, name: &str) -> (ThemeFile, Option<String>) {
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
    fn materialize_and_list() {
        let tmp = tempfile::tempdir().unwrap();
        let p = Paths::resolve(Some(tmp.path().to_path_buf()), None).unwrap();
        p.ensure().unwrap();
        materialize(&p).unwrap();
        assert!(p.theme_dir("nord").join("assets").is_dir());
        let mut t = load(&p, "nord").unwrap();
        t.meta.name = "my-nord".into();
        save(&p, &t).unwrap();
        assert!(list(&p).unwrap().contains(&"my-nord".to_string()));
    }
}
