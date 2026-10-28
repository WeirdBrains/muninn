//! `preferences.toml`.
use crate::error::Result;
use crate::paths::{write_private, Paths};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Light,
    Dark,
    #[default]
    System,
}

impl std::fmt::Display for ThemeMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ThemeMode::Light => "light",
            ThemeMode::Dark => "dark",
            ThemeMode::System => "system",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThemePrefs {
    pub name: String,
    pub mode: ThemeMode,
}

impl Default for ThemePrefs {
    fn default() -> Self {
        Self { name: "tokyo-night".into(), mode: ThemeMode::System }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct IngestPrefs {
    /// Drop zero-volume bars instead of keeping them flagged.
    #[serde(default)]
    pub drop_zero_volume: bool,
}

/// TUI header size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum HeaderMode {
    /// Large when the terminal has at least 30 rows.
    #[default]
    Auto,
    Compact,
    Large,
}

impl HeaderMode {
    pub fn next(self) -> Self {
        match self {
            HeaderMode::Auto => HeaderMode::Compact,
            HeaderMode::Compact => HeaderMode::Large,
            HeaderMode::Large => HeaderMode::Auto,
        }
    }
}

/// TUI state remembered between sessions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TuiPrefs {
    #[serde(default)]
    pub header: HeaderMode,
    #[serde(default)]
    pub sidebar_collapsed: bool,
    /// Current tiling layout (restored on start).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<crate::tui::layout::TileLayout>,
    /// Named layouts (View ▸ Layouts ▸ Save current…).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub layouts: std::collections::BTreeMap<String, crate::tui::layout::TileLayout>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Preferences {
    #[serde(default)]
    pub theme: ThemePrefs,
    #[serde(default)]
    pub ingest: IngestPrefs,
    #[serde(default)]
    pub tui: TuiPrefs,
}

impl Preferences {
    /// Load preferences, creating the file with defaults when missing.
    pub fn load_or_create(paths: &Paths) -> Result<Self> {
        let p = paths.preferences();
        if p.exists() {
            let s = std::fs::read_to_string(&p)?;
            Ok(toml::from_str(&s)?)
        } else {
            let prefs = Preferences::default();
            prefs.save(paths)?;
            Ok(prefs)
        }
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        let s = toml::to_string_pretty(self)?;
        write_private(&paths.preferences(), s.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_tokyo_night_system() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::resolve(Some(tmp.path().to_path_buf()), None).unwrap();
        paths.ensure().unwrap();
        let p = Preferences::load_or_create(&paths).unwrap();
        assert_eq!(p.theme.name, "tokyo-night");
        assert_eq!(p.theme.mode, ThemeMode::System);
        let text = std::fs::read_to_string(paths.preferences()).unwrap();
        assert!(text.contains("name = \"tokyo-night\""));
        assert!(text.contains("mode = \"system\""));
    }
}
