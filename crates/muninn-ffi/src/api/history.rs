//! Action history (`View ▸ History`, TUI `Y` / `u` / `U`). See `docs/history.md`.
use super::data::{open, open_write};
use muninn_core::history::ViewState;
use muninn_core::store::{Move, Store};

#[derive(Debug)]
pub struct HistoryRow {
    pub id: i64,
    pub kind: String,
    pub summary: String,
    pub depth: i64,
    pub head: bool,
    pub current: bool,
    pub fork: bool,
    pub created_at: i64,
}

#[derive(Debug)]
pub struct HistoryMove {
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub theme: String,
    pub mode: String,
    pub range_from: Option<i64>,
    pub range_to: Option<i64>,
}

fn row(r: &muninn_core::store::TreeRow) -> HistoryRow {
    HistoryRow {
        id: r.action.id,
        kind: r.action.kind.clone(),
        summary: r.action.summary.clone(),
        depth: r.depth as i64,
        head: r.head,
        current: r.current,
        fork: r.fork,
        created_at: r.action.created_at,
    }
}

fn from_move(m: Move) -> HistoryMove {
    let v = m.state.as_ref().and_then(ViewState::from_value);
    HistoryMove {
        from: m.from,
        to: m.to,
        theme: v.as_ref().map(|x| x.theme.clone()).unwrap_or_default(),
        mode: v.as_ref().map(|x| x.mode.to_string()).unwrap_or_default(),
        range_from: v.as_ref().and_then(|x| x.filters.from),
        range_to: v.as_ref().and_then(|x| x.filters.to),
    }
}

pub(crate) fn history_tree_of(store: &Store) -> anyhow::Result<Vec<HistoryRow>> {
    Ok(store.history_tree()?.iter().map(row).collect())
}

/// The action tree (forks indented), oldest first. Row 0 in the UI is the start, before these.
pub fn history_tree() -> anyhow::Result<Vec<HistoryRow>> {
    history_tree_of(&open()?)
}

pub(crate) fn history_back_of(store: &Store) -> anyhow::Result<HistoryMove> {
    Ok(from_move(store.history_back(1)?))
}

/// Step back (`u` / View ▸ History ▸ Step back).
pub fn history_back() -> anyhow::Result<HistoryMove> {
    history_back_of(&open_write()?)
}

pub(crate) fn history_forward_of(store: &Store) -> anyhow::Result<HistoryMove> {
    Ok(from_move(store.history_forward(1)?))
}

/// Step forward (`U` / View ▸ History ▸ Step forward).
pub fn history_forward() -> anyhow::Result<HistoryMove> {
    history_forward_of(&open_write()?)
}

pub(crate) fn history_goto_of(store: &Store, id: Option<i64>) -> anyhow::Result<HistoryMove> {
    Ok(from_move(store.history_goto(id)?))
}

/// Jump to an action (or `None` for the start of the history).
pub fn history_goto(id: Option<i64>) -> anyhow::Result<HistoryMove> {
    history_goto_of(&open_write()?, id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use muninn_core::store::{NewAction, Store};

    #[test]
    fn tree_lists_actions_and_back_reaches_the_start() {
        let s = Store::open_in_memory().unwrap();
        assert!(history_tree_of(&s).unwrap().is_empty());
        s.note_action(&NewAction::new("view.filter", "app", "filter BTC"));
        s.note_action(&NewAction::new("view.theme", "app", "theme nord"));
        let tree = history_tree_of(&s).unwrap();
        assert_eq!(tree.len(), 2);
        assert!(tree.iter().any(|r| r.summary.contains("nord")));
        assert!(tree.iter().any(|r| r.head));
        let m = history_back_of(&s).unwrap();
        assert!(m.to.is_some());
        let m = history_back_of(&s).unwrap();
        assert!(m.to.is_none(), "start of the history");
        let again = history_back_of(&s).unwrap_err().to_string();
        assert!(again.contains("start"), "{again}");
        let fwd = history_forward_of(&s).unwrap();
        assert!(fwd.to.is_some());
    }
}
