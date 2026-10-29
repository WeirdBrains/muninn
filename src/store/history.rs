//! Action history: every recorded action, its parent and the current position (head).
//!
//! Actions form a tree. A new action becomes a child of the head and the new head.
//! Stepping back moves the head to its parent without deleting anything; doing something
//! new from there adds a second child, a fork. All branches stay in the database.
//!
//! An action may carry the view state from before and after it (filters, time range,
//! theme). Moving the head returns the view state that belongs there so a front end can
//! restore it. Data actions (imports, exports) are only recorded, never undone.
use super::{Store, now_ms};
use crate::error::{Error, Result};
use rusqlite::{OptionalExtension, params};
use serde_json::Value;

pub const MIGRATION: &str = r#"
CREATE TABLE actions (
    id INTEGER PRIMARY KEY,
    parent_id INTEGER REFERENCES actions(id),
    kind TEXT NOT NULL,
    summary TEXT NOT NULL,
    params TEXT NOT NULL DEFAULT '{}',
    origin TEXT NOT NULL,
    author_id TEXT,
    created_at INTEGER NOT NULL,
    state_before TEXT,
    state_after TEXT
);
CREATE INDEX actions_parent ON actions(parent_id);
"#;

const HEAD: &str = "history_head";
const REDO: &str = "history_redo";

/// An action to record.
#[derive(Debug, Clone, Default)]
pub struct NewAction {
    /// `import`, `export`, `filter`, `theme`, `author`, `source`, ...
    pub kind: String,
    /// One line for listings, e.g. `export 120 bars to /tmp/x.csv (csv)`.
    pub summary: String,
    pub params: Value,
    /// `cli` or `tui`.
    pub origin: String,
    pub before: Option<Value>,
    pub after: Option<Value>,
}

impl NewAction {
    pub fn new(kind: &str, origin: &str, summary: impl Into<String>) -> Self {
        Self { kind: kind.into(), origin: origin.into(), summary: summary.into(), params: Value::Object(Default::default()), ..Default::default() }
    }
    pub fn params(mut self, v: Value) -> Self {
        self.params = v;
        self
    }
    pub fn states(mut self, before: Value, after: Value) -> Self {
        self.before = Some(before);
        self.after = Some(after);
        self
    }
}

/// A recorded action.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ActionRecord {
    pub id: i64,
    pub parent: Option<i64>,
    pub kind: String,
    pub summary: String,
    pub params: Value,
    pub origin: String,
    pub author_id: Option<String>,
    pub created_at: i64,
    pub before: Option<Value>,
    pub after: Option<Value>,
}

/// One row of [`Store::history_tree`].
#[derive(Debug, Clone, PartialEq)]
pub struct TreeRow {
    pub action: ActionRecord,
    /// Branch depth: 0 for the first line, +1 for each fork taken.
    pub depth: usize,
    /// First action of a fork (a later child of its parent, or a later root).
    pub fork: bool,
    pub head: bool,
    /// On the path from the first action to the head (what the current state is made of).
    pub current: bool,
}

/// Result of moving the head.
#[derive(Debug, Clone, PartialEq)]
pub struct Move {
    pub from: Option<i64>,
    pub to: Option<i64>,
    /// View state to restore, when the actions involved recorded one.
    pub state: Option<Value>,
}

fn row(r: &rusqlite::Row) -> rusqlite::Result<ActionRecord> {
    let p: String = r.get(4)?;
    let b: Option<String> = r.get(8)?;
    let a: Option<String> = r.get(9)?;
    let j = |s: Option<String>| s.and_then(|s| serde_json::from_str(&s).ok());
    Ok(ActionRecord {
        id: r.get(0)?,
        parent: r.get(1)?,
        kind: r.get(2)?,
        summary: r.get(3)?,
        params: serde_json::from_str(&p).unwrap_or(Value::Null),
        origin: r.get(5)?,
        author_id: r.get(6)?,
        created_at: r.get(7)?,
        before: j(b),
        after: j(a),
    })
}

const COLS: &str = "id, parent_id, kind, summary, params, origin, author_id, created_at, state_before, state_after";

impl Store {
    /// Record an action as a child of the head (attributed to the current author) and make it the head.
    pub fn record_action(&self, a: &NewAction) -> Result<i64> {
        let parent = self.history_head()?;
        let author = self.setting("current_author")?;
        let s = |v: &Option<Value>| v.as_ref().map(|v| v.to_string());
        // actions taken by a rule carry the chain of rules that led to them (provenance)
        let chain = self.rules_rt.chain.borrow().clone();
        let mut params = a.params.clone();
        if !chain.is_empty() {
            if !params.is_object() {
                params = serde_json::json!({});
            }
            params["rule_chain"] = serde_json::json!(chain);
        }
        self.conn.execute(
            "INSERT INTO actions(parent_id, kind, summary, params, origin, author_id, created_at, state_before, state_after)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![parent, a.kind, a.summary, params.to_string(), a.origin, author, now_ms(), s(&a.before), s(&a.after)],
        )?;
        let id = self.conn.last_insert_rowid();
        self.set_setting(HEAD, Some(&id.to_string()))?;
        crate::rules::after_action(self, id);
        Ok(id)
    }

    /// Rewrite an action's summary and params (a rule run fills in its results).
    pub(crate) fn update_action(&self, id: i64, summary: &str, params: &Value) -> Result<()> {
        self.conn.execute("UPDATE actions SET summary = ?2, params = ?3 WHERE id = ?1", params![id, summary, params.to_string()])?;
        Ok(())
    }

    /// Actions of one kind, oldest first.
    pub fn actions_of_kind(&self, kind: &str) -> Result<Vec<ActionRecord>> {
        let mut st = self.conn.prepare(&format!("SELECT {COLS} FROM actions WHERE kind = ?1 ORDER BY id"))?;
        let v = st.query_map([kind], row)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    /// Record, logging instead of failing (history must never break the action itself).
    pub fn note_action(&self, a: &NewAction) {
        if let Err(e) = self.record_action(a) {
            tracing::warn!("could not record action `{}` in history: {e}", a.kind);
        }
    }

    /// Cheap change marker (count, newest id, head) so views reload only when history moved.
    pub fn history_stamp(&self) -> Result<(i64, i64, Option<i64>)> {
        let (n, max) = self.conn.query_row("SELECT COUNT(*), COALESCE(MAX(id), 0) FROM actions", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok((n, max, self.history_head()?))
    }

    pub fn history_head(&self) -> Result<Option<i64>> {
        Ok(self.setting(HEAD)?.and_then(|s| s.parse().ok()))
    }

    pub fn action(&self, id: i64) -> Result<Option<ActionRecord>> {
        Ok(self.conn.query_row(&format!("SELECT {COLS} FROM actions WHERE id = ?1"), [id], row).optional()?)
    }

    /// Every action, oldest first.
    pub fn actions(&self) -> Result<Vec<ActionRecord>> {
        let mut st = self.conn.prepare(&format!("SELECT {COLS} FROM actions ORDER BY id"))?;
        let v = st.query_map([], row)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn action_children(&self, id: Option<i64>) -> Result<Vec<i64>> {
        let mut st = self.conn.prepare("SELECT id FROM actions WHERE parent_id IS ?1 ORDER BY id")?;
        let v = st.query_map([id], |r| r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    /// The head and its ancestors, head first.
    pub fn action_path(&self, id: Option<i64>) -> Result<Vec<ActionRecord>> {
        let mut out = vec![];
        let mut cur = id;
        while let Some(i) = cur {
            let Some(a) = self.action(i)? else { break };
            cur = a.parent;
            out.push(a);
        }
        Ok(out)
    }

    /// Number of forks (places where more than one branch continues).
    pub fn history_forks(&self) -> Result<i64> {
        Ok(self.conn.query_row("SELECT COUNT(*) FROM (SELECT parent_id FROM actions GROUP BY parent_id HAVING COUNT(*) > 1)", [], |r| r.get(0))?)
    }

    /// All actions as a tree in display order: each branch continues on its first child,
    /// later children start forks one level deeper.
    pub fn history_tree(&self) -> Result<Vec<TreeRow>> {
        let all = self.actions()?;
        let head = self.history_head()?;
        let current: std::collections::HashSet<i64> = self.action_path(head)?.iter().map(|a| a.id).collect();
        let mut kids: std::collections::HashMap<Option<i64>, Vec<usize>> = Default::default();
        for (i, a) in all.iter().enumerate() {
            kids.entry(a.parent).or_default().push(i);
        }
        let mut out = vec![];
        // Each line follows first children to its end; later children (forks) wait on
        // `forks` and are listed after it, newest-deferred first (closest to where it ended).
        let mut line: Option<(usize, usize, bool)> = None;
        let mut forks: Vec<(usize, usize, bool)> = vec![];
        if let Some(roots) = kids.get(&None) {
            for &r in roots.iter().skip(1).rev() {
                forks.push((r, 1, true));
            }
            line = Some((roots[0], 0, false));
        }
        while let Some((i, depth, fork)) = line.take().or_else(|| forks.pop()) {
            let a = &all[i];
            out.push(TreeRow { action: a.clone(), depth, fork, head: Some(a.id) == head, current: current.contains(&a.id) });
            if let Some(ch) = kids.get(&Some(a.id)) {
                for &c in ch.iter().skip(1).rev() {
                    forks.push((c, depth + 1, true));
                }
                line = Some((ch[0], depth, false));
            }
        }
        Ok(out)
    }

    /// Move the head to `to` (None: before the first action). Returns the view state to restore.
    pub fn history_goto(&self, to: Option<i64>) -> Result<Move> {
        if let Some(id) = to
            && self.action(id)?.is_none()
        {
            return Err(Error::invalid("action", format!("#{id} not found (see `mnnn history list`)")));
        }
        let from = self.history_head()?;
        let state = self.state_for(to, from)?;
        self.set_setting(HEAD, to.map(|i| i.to_string()).as_deref())?;
        Ok(Move { from, to, state })
    }

    /// Step back `n` actions (to the parent). Errors at the start of the history.
    pub fn history_back(&self, n: usize) -> Result<Move> {
        let head = self.history_head()?;
        let Some(h) = head else { return Err(Error::invalid("history", "already at the start")) };
        let path = self.action_path(Some(h))?;
        let to = path.get(n.max(1)).map(|a| a.id);
        // Remember where we came from so forward retraces it (unless already further down).
        let redo = self.setting(REDO)?.and_then(|s| s.parse::<i64>().ok());
        if !redo.is_some_and(|r| self.action_path(Some(r)).map(|p| p.iter().any(|a| a.id == h)).unwrap_or(false)) {
            self.set_setting(REDO, Some(&h.to_string()))?;
        }
        self.history_goto(to)
    }

    /// Step forward `n` actions: toward where you stepped back from, else the newest branch.
    pub fn history_forward(&self, n: usize) -> Result<Move> {
        let start = self.history_head()?;
        let redo = self.setting(REDO)?.and_then(|s| s.parse::<i64>().ok());
        let redo_path: Vec<i64> = match redo {
            Some(r) => self.action_path(Some(r))?.iter().map(|a| a.id).collect(),
            None => vec![],
        };
        let mut cur = start;
        for _ in 0..n.max(1) {
            let kids = self.action_children(cur)?;
            let next = kids.iter().copied().find(|k| redo_path.contains(k)).or_else(|| kids.last().copied());
            match next {
                Some(k) => cur = Some(k),
                None => break,
            }
        }
        if cur == start {
            return Err(Error::invalid("history", "already at the newest action"));
        }
        let m = self.state_for(cur, start)?;
        self.set_setting(HEAD, cur.map(|i| i.to_string()).as_deref())?;
        Ok(Move { from: start, to: cur, state: m })
    }

    /// View state that belongs at `to`: the newest "after" state on its path, else (nothing
    /// was changed before `to`) the oldest "before" state on the branch being left.
    fn state_for(&self, to: Option<i64>, from: Option<i64>) -> Result<Option<Value>> {
        let path = self.action_path(to)?;
        if let Some(v) = path.iter().find_map(|a| a.after.clone()) {
            return Ok(Some(v));
        }
        let on_path: std::collections::HashSet<i64> = path.iter().map(|a| a.id).collect();
        let left: Vec<ActionRecord> = self.action_path(from)?.into_iter().take_while(|a| !on_path.contains(&a.id)).collect();
        Ok(left.iter().rev().find_map(|a| a.before.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn act(s: &Store, n: &str) -> i64 {
        s.record_action(&NewAction::new("filter", "cli", n).states(json!(format!("before {n}")), json!(n))).unwrap()
    }

    #[test]
    fn record_back_fork_forward() {
        let s = Store::open_in_memory().unwrap();
        let au = s.add_author("Michael", None, &[]).unwrap();
        let a = act(&s, "a");
        let b = act(&s, "b");
        let c = act(&s, "c");
        assert_eq!(s.history_head().unwrap(), Some(c));
        assert_eq!(s.action(a).unwrap().unwrap().author_id.as_deref(), Some(au.id.as_str()));
        let m = s.history_back(1).unwrap();
        assert_eq!((m.from, m.to, m.state), (Some(c), Some(b), Some(json!("b"))));
        // something different from b: a fork, c is kept
        let d = act(&s, "d");
        assert_eq!(s.action(d).unwrap().unwrap().parent, Some(b));
        assert_eq!(s.action_children(Some(b)).unwrap(), vec![c, d]);
        assert_eq!(s.history_forks().unwrap(), 1);
        assert_eq!(s.actions().unwrap().len(), 4);
        // back to the very start restores the state from before the first action
        let m = s.history_back(3).unwrap();
        assert_eq!((m.to, m.state), (None, Some(json!("before a"))));
        assert!(s.history_back(1).is_err());
        // forward retraces the branch we stepped back from (d), not c
        let m = s.history_forward(3).unwrap();
        assert_eq!(m.to, Some(d));
        assert!(s.history_forward(1).is_err());
        // goto any action, on any branch
        let m = s.history_goto(Some(c)).unwrap();
        assert_eq!(m.state, Some(json!("c")));
        assert!(s.history_goto(Some(99)).is_err());
    }

    #[test]
    fn tree_lists_forks_after_their_line() {
        let s = Store::open_in_memory().unwrap();
        let a = act(&s, "a");
        let b = act(&s, "b");
        let c = act(&s, "c");
        s.history_goto(Some(a)).unwrap();
        let d = act(&s, "d");
        let e = act(&s, "e");
        let t = s.history_tree().unwrap();
        let ids: Vec<(i64, usize, bool)> = t.iter().map(|r| (r.action.id, r.depth, r.fork)).collect();
        assert_eq!(ids, vec![(a, 0, false), (b, 0, false), (c, 0, false), (d, 1, true), (e, 1, false)]);
        assert!(t.iter().find(|r| r.action.id == e).unwrap().head);
        assert!(t.iter().find(|r| r.action.id == a).unwrap().current);
        assert!(!t.iter().find(|r| r.action.id == b).unwrap().current);
    }

    #[test]
    fn data_actions_carry_no_state() {
        let s = Store::open_in_memory().unwrap();
        let i = s.record_action(&NewAction::new("import", "cli", "import 1 file").params(json!({"run": 1}))).unwrap();
        let r = s.action(i).unwrap().unwrap();
        assert_eq!((r.before, r.after, r.params["run"].as_i64()), (None, None, Some(1)));
        assert_eq!(s.history_back(1).unwrap().state, None);
    }
}
