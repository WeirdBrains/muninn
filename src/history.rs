//! Action history over the store: the view state that actions save and restore, and
//! helpers front ends use to record what they do. See `docs/history.md`.
use crate::prefs::{Preferences, ThemeMode};
use crate::store::{Move, NewAction, Store};
use crate::tui::sidebar::Filters;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What stepping through the history restores: filters (including the time range), theme and mode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ViewState {
    pub theme: String,
    pub mode: ThemeMode,
    #[serde(default)]
    pub filters: Filters,
}

impl ViewState {
    pub fn capture(p: &Preferences) -> Self {
        Self { theme: p.theme.name.clone(), mode: p.theme.mode, filters: p.tui.sidebar.filters.clone() }
    }
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
    pub fn from_value(v: &Value) -> Option<Self> {
        serde_json::from_value(v.clone()).ok()
    }
    /// Put this state into preferences. Returns true if anything changed.
    pub fn apply(&self, p: &mut Preferences) -> bool {
        let before = Self::capture(p);
        p.theme.name = self.theme.clone();
        p.theme.mode = self.mode;
        p.tui.sidebar.filters = self.filters.clone();
        before != *self
    }
    /// Short description of the filters, e.g. `src:binance, from 2024-01-01 00:00` or `no filters`.
    pub fn filters_label(&self) -> String {
        let c: Vec<String> = self.filters.chips().into_iter().map(|(_, l)| l).collect();
        if c.is_empty() { "no filters".into() } else { c.join(", ") }
    }
}

/// A view change (filters, time range, theme, mode) with its before and after state.
pub fn view_action(kind: &str, origin: &str, summary: impl Into<String>, before: &ViewState, after: &ViewState) -> NewAction {
    NewAction::new(kind, origin, summary).states(before.to_value(), after.to_value())
}

/// Apply the state a head move returned to preferences (and save them). Returns the state applied.
pub fn apply_move(m: &Move, prefs: &mut Preferences, paths: &crate::paths::Paths) -> crate::Result<Option<ViewState>> {
    let Some(v) = m.state.as_ref().and_then(ViewState::from_value) else { return Ok(None) };
    if v.apply(prefs) {
        prefs.save(paths)?;
    }
    Ok(Some(v))
}

/// Author name for an action (`-` when none, `(removed)` when the author was deleted).
pub fn author_label(store: &Store, id: Option<&str>) -> String {
    match id {
        None => "-".into(),
        Some(i) => store.author(i).ok().flatten().map(|a| a.name).unwrap_or_else(|| "(removed author)".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_apply_roundtrip() {
        let mut p = Preferences::default();
        let a = ViewState::capture(&p);
        p.theme.name = "nord".into();
        p.tui.sidebar.filters.sources.insert("binance".into());
        let b = ViewState::capture(&p);
        assert_eq!(b.filters_label(), "src:binance");
        assert_eq!(ViewState::from_value(&b.to_value()), Some(b.clone()));
        assert!(a.apply(&mut p));
        assert_eq!(p.theme.name, "tokyo-night");
        assert!(p.tui.sidebar.filters.is_empty());
        assert!(!a.apply(&mut p));
    }
}
