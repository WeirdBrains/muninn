//! The app's saved state in `preferences.toml`, shared with the TUI: the theme (`[theme]`,
//! like `mnnn theme set/mode`), the time range (`[tui.sidebar.filters]` from/to, like `T`)
//! and the open dataset (`[app]`). Reading never creates the file; each setter loads,
//! changes one thing and saves, like the TUI.
use super::info::paths;
use muninn_core::paths::Paths;
use muninn_core::prefs::{Preferences, ThemeMode};
use muninn_core::theme;

/// What the app restores on launch.
pub struct SavedState {
    pub theme: String,
    /// `light`, `dark` or `system`.
    pub mode: String,
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub dataset: Option<String>,
}

fn read(p: &Paths) -> Preferences {
    std::fs::read_to_string(p.preferences()).ok().and_then(|s| toml::from_str(&s).ok()).unwrap_or_default()
}

fn update(home: Option<String>, f: impl FnOnce(&mut Preferences)) -> anyhow::Result<()> {
    let p = paths(home)?;
    let mut prefs = Preferences::load_or_create(&p)?;
    f(&mut prefs);
    prefs.save(&p)?;
    Ok(())
}

fn mode_name(m: ThemeMode) -> &'static str {
    match m {
        ThemeMode::Light => "light",
        ThemeMode::Dark => "dark",
        ThemeMode::System => "system",
    }
}

/// The saved theme, mode, time range and dataset (read-only).
pub fn saved_state(home: Option<String>) -> anyhow::Result<SavedState> {
    let p = read(&paths(home)?);
    let f = &p.tui.sidebar.filters;
    Ok(SavedState { theme: p.theme.name, mode: mode_name(p.theme.mode).into(), from: f.from, to: f.to, dataset: p.app.dataset })
}

/// `mnnn theme set NAME`: the theme must exist (built-in or in `<home>/themes/`).
pub fn save_theme(home: Option<String>, name: String) -> anyhow::Result<()> {
    theme::load(&paths(home.clone())?, &name)?;
    update(home, |p| p.theme.name = name)
}

/// `mnnn theme mode light|dark|system`.
pub fn save_theme_mode(home: Option<String>, mode: String) -> anyhow::Result<()> {
    let m = match mode.as_str() {
        "light" => ThemeMode::Light,
        "dark" => ThemeMode::Dark,
        "system" => ThemeMode::System,
        _ => anyhow::bail!("mode must be light, dark or system"),
    };
    update(home, |p| p.theme.mode = m)
}

/// The time range every view uses (the TUI's `T`), UTC ms; `None` leaves an end open.
pub fn save_time_range(home: Option<String>, from: Option<i64>, to: Option<i64>) -> anyhow::Result<()> {
    update(home, |p| {
        p.tui.sidebar.filters.from = from;
        p.tui.sidebar.filters.to = to;
    })
}

/// The dataset open in the Data view (`source:BASE/QUOTE:duration_s`), or `None`.
pub fn save_dataset(home: Option<String>, key: Option<String>) -> anyhow::Result<()> {
    update(home, |p| p.app.dataset = key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_round_trip_and_keep_other_preferences() {
        let dir = tempfile::tempdir().unwrap();
        let home = Some(dir.path().display().to_string());
        let s = saved_state(home.clone()).unwrap();
        assert_eq!((s.theme.as_str(), s.mode.as_str(), s.from, s.dataset.clone()), ("tokyo-night", "system", None, None));
        assert!(!dir.path().join("preferences.toml").exists(), "reading must not create preferences");

        save_theme(home.clone(), "nord".into()).unwrap();
        save_theme_mode(home.clone(), "light".into()).unwrap();
        save_time_range(home.clone(), Some(1_704_067_200_000), None).unwrap();
        save_dataset(home.clone(), Some("coinbase:BTC/USD:300".into())).unwrap();
        let s = saved_state(home.clone()).unwrap();
        assert_eq!((s.theme.as_str(), s.mode.as_str()), ("nord", "light"));
        assert_eq!((s.from, s.to), (Some(1_704_067_200_000), None));
        assert_eq!(s.dataset.as_deref(), Some("coinbase:BTC/USD:300"));

        // the TUI sees the same values
        let p = Preferences::load_or_create(&paths(home.clone()).unwrap()).unwrap();
        assert_eq!(p.theme.name, "nord");
        assert_eq!(p.tui.sidebar.filters.from, Some(1_704_067_200_000));

        assert!(save_theme(home.clone(), "no-such-theme".into()).is_err());
        assert!(save_theme_mode(home.clone(), "sepia".into()).is_err());
        assert_eq!(saved_state(home).unwrap().theme, "nord", "a bad value changes nothing");
    }
}
