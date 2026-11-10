//! The app's saved state in `preferences.toml`, shared with the TUI: the theme (`[theme]`,
//! like `mnnn theme set/mode`), the time range (`[tui.sidebar.filters]` from/to, like `T`)
//! and the open dataset (`[app]`). Reading never creates the file; each setter loads,
//! changes one thing and saves, like the TUI.
use super::info::paths;
use muninn_core::model::PriceMetric;
use muninn_core::paths::Paths;
use muninn_core::prefs::{HeaderMode, Preferences, ThemeMode};
use muninn_core::theme;
use muninn_core::tui::sidebar::{PriceFilter, SortBy};

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

/// Sidebar filters other than the time range (Data ▸ Filters / Sort), shared with the TUI.
pub struct SidebarState {
    pub sources: Vec<String>,
    pub symbols: Vec<String>,
    pub durations: Vec<String>,
    pub years: Vec<i32>,
    /// `open`, `high`, `low`, `close`, `typical`, `median`, `ohlc avg` or `vwap`.
    pub price_metric: String,
    pub price_min: Option<f64>,
    pub price_max: Option<f64>,
    /// `year` or `name`.
    pub sort: String,
    pub desc: bool,
}

fn metric_of(s: &str) -> PriceMetric {
    PriceMetric::ALL.into_iter().find(|m| m.label() == s).unwrap_or(PriceMetric::Close)
}

/// Read `[tui.sidebar]` except `from`/`to` (those stay on [`saved_state`] / [`save_time_range`]).
pub fn sidebar_state(home: Option<String>) -> anyhow::Result<SidebarState> {
    let p = read(&paths(home)?);
    let s = &p.tui.sidebar;
    let f = &s.filters;
    Ok(SidebarState {
        sources: f.sources.iter().cloned().collect(),
        symbols: f.symbols.iter().cloned().collect(),
        durations: f.durations.iter().cloned().collect(),
        years: f.years.iter().copied().collect(),
        price_metric: f.price.metric.label().into(),
        price_min: f.price.min,
        price_max: f.price.max,
        sort: match s.sort {
            SortBy::Name => "name",
            SortBy::Year => "year",
        }
        .into(),
        desc: s.desc,
    })
}

/// Write `[tui.sidebar]` filters and sort. The time range is left as it is.
pub fn save_sidebar_state(home: Option<String>, state: SidebarState) -> anyhow::Result<()> {
    update(home, |p| {
        p.tui.sidebar.sort = if state.sort == "name" { SortBy::Name } else { SortBy::Year };
        p.tui.sidebar.desc = state.desc;
        let f = &mut p.tui.sidebar.filters;
        f.sources = state.sources.into_iter().collect();
        f.symbols = state.symbols.into_iter().collect();
        f.durations = state.durations.into_iter().collect();
        f.years = state.years.into_iter().collect();
        f.price = PriceFilter { metric: metric_of(&state.price_metric), min: state.price_min, max: state.price_max };
    })
}

/// TUI header size: `auto`, `compact` or `large` (Preferences).
pub fn header_mode(home: Option<String>) -> anyhow::Result<String> {
    let p = read(&paths(home)?);
    Ok(match p.tui.header {
        HeaderMode::Compact => "compact",
        HeaderMode::Large => "large",
        HeaderMode::Auto => "auto",
    }
    .into())
}

pub fn save_header_mode(home: Option<String>, mode: String) -> anyhow::Result<()> {
    let m = match mode.as_str() {
        "compact" => HeaderMode::Compact,
        "large" => HeaderMode::Large,
        "auto" => HeaderMode::Auto,
        _ => anyhow::bail!("header must be auto, compact or large"),
    };
    update(home, |p| p.tui.header = m)
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

        let side = sidebar_state(home.clone()).unwrap();
        assert!(side.sources.is_empty() && side.sort == "year" && !side.desc);
        save_sidebar_state(
            home.clone(),
            SidebarState {
                sources: vec!["coinbase".into()],
                symbols: vec!["BTC/USD".into()],
                durations: vec!["5m".into()],
                years: vec![2024],
                price_metric: "high".into(),
                price_min: Some(1.0),
                price_max: None,
                sort: "name".into(),
                desc: true,
            },
        )
        .unwrap();
        let side = sidebar_state(home.clone()).unwrap();
        assert_eq!(side.sources, vec!["coinbase"]);
        assert_eq!((side.sort.as_str(), side.desc, side.price_metric.as_str(), side.years), ("name", true, "high", vec![2024]));
        assert_eq!(saved_state(home.clone()).unwrap().from, Some(1_704_067_200_000), "sidebar save keeps the time range");
        assert_eq!(header_mode(home.clone()).unwrap(), "auto");
        save_header_mode(home.clone(), "compact".into()).unwrap();
        assert_eq!(header_mode(home.clone()).unwrap(), "compact");
        assert!(save_header_mode(home.clone(), "huge".into()).is_err());

        assert!(save_theme(home.clone(), "no-such-theme".into()).is_err());
        assert!(save_theme_mode(home.clone(), "sepia".into()).is_err());
        assert_eq!(saved_state(home).unwrap().theme, "nord", "a bad value changes nothing");
    }
}
