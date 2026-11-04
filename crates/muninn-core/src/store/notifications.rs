//! Notifications (2.18): what deserves your attention, from built-in events, rule sets
//! (`notify …`) and other nodes' broadcasts. Kept additively and synced to your linked
//! nodes (read marks too). The rules for making them are `crate::notify`; see
//! `docs/notifications.md`.
use super::{Store, new_id, now_ms};
use crate::error::Result;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

pub const MIGRATION: &str = r#"
CREATE TABLE notifications (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    level TEXT NOT NULL DEFAULT 'info',
    title TEXT NOT NULL,
    body TEXT NOT NULL DEFAULT '',
    link TEXT,
    action_id INTEGER,
    created_at INTEGER NOT NULL,
    read_at INTEGER
);
CREATE INDEX notifications_unread ON notifications(read_at, created_at);
"#;

/// One notification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Notification {
    pub id: String,
    /// What made it: a built-in event's kind (`review.comment`), `rule` or `broadcast`.
    pub kind: String,
    /// `info`, `warn` or `error`.
    pub level: String,
    pub title: String,
    pub body: String,
    /// Where it points (`review:ID`, `org:ID`, `journal:ID`, …; see `crate::notify::Link`).
    pub link: Option<String>,
    /// The history action that caused it, on the node that made it.
    pub action_id: Option<i64>,
    pub created_at: i64,
    pub read_at: Option<i64>,
}

/// A new notification.
#[derive(Debug, Clone, Default)]
pub struct NewNotification {
    pub kind: String,
    pub level: String,
    pub title: String,
    pub body: String,
    pub link: Option<String>,
    pub action_id: Option<i64>,
}

const COLS: &str = "id, kind, level, title, body, link, action_id, created_at, read_at";

fn row(r: &rusqlite::Row) -> rusqlite::Result<Notification> {
    Ok(Notification {
        id: r.get(0)?,
        kind: r.get(1)?,
        level: r.get(2)?,
        title: r.get(3)?,
        body: r.get(4)?,
        link: r.get(5)?,
        action_id: r.get(6)?,
        created_at: r.get(7)?,
        read_at: r.get(8)?,
    })
}

impl Store {
    pub fn add_notification(&self, n: &NewNotification) -> Result<Notification> {
        let id = new_id();
        let level = if n.level.is_empty() { "info" } else { n.level.as_str() };
        self.conn.execute(
            &format!("INSERT INTO notifications({COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL)"),
            params![id, n.kind, level, n.title, n.body, n.link, n.action_id, now_ms()],
        )?;
        Ok(self.notification(&id)?.expect("just added"))
    }

    pub fn notification(&self, id: &str) -> Result<Option<Notification>> {
        Ok(self.conn.query_row(&format!("SELECT {COLS} FROM notifications WHERE id = ?1"), [id], row).optional()?)
    }

    /// Newest first; `unread` keeps only unread ones; at most `limit`.
    pub fn notifications(&self, unread: bool, limit: usize) -> Result<Vec<Notification>> {
        let w = if unread { "WHERE read_at IS NULL" } else { "" };
        let mut st = self.conn.prepare(&format!("SELECT {COLS} FROM notifications {w} ORDER BY created_at DESC, id LIMIT ?1"))?;
        let v = st.query_map([limit as i64], row)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    /// By id or unique id prefix.
    pub fn find_notification(&self, key: &str) -> Result<Notification> {
        let k = key.trim();
        let all = self.notifications(false, usize::MAX >> 1)?;
        super::pick(all, k, |n| &n.id, |n| &n.id, "notification", "mnnn notify list")
    }

    pub fn unread_notifications(&self) -> Result<usize> {
        let n: i64 = self.conn.query_row("SELECT COUNT(*) FROM notifications WHERE read_at IS NULL", [], |r| r.get(0))?;
        Ok(n as usize)
    }

    /// Mark one read (or unread again); `true` when it changed.
    pub fn mark_notification(&self, id: &str, read: bool) -> Result<bool> {
        let n = if read {
            self.conn.execute("UPDATE notifications SET read_at = ?2 WHERE id = ?1 AND read_at IS NULL", params![id, now_ms()])?
        } else {
            self.conn.execute("UPDATE notifications SET read_at = NULL WHERE id = ?1 AND read_at IS NOT NULL", [id])?
        };
        Ok(n == 1)
    }

    /// Mark every unread one read; how many.
    pub fn mark_all_notifications(&self) -> Result<usize> {
        Ok(self.conn.execute("UPDATE notifications SET read_at = ?1 WHERE read_at IS NULL", [now_ms()])?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notifications_add_list_and_read() {
        let s = Store::open_in_memory().unwrap();
        assert_eq!(s.unread_notifications().unwrap(), 0);
        let a = s.add_notification(&NewNotification { kind: "rule".into(), title: "a".into(), ..Default::default() }).unwrap();
        assert_eq!(a.level, "info");
        let b = s
            .add_notification(&NewNotification {
                kind: "notify".into(),
                level: "warn".into(),
                title: "b".into(),
                link: Some("org:o1".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(s.unread_notifications().unwrap(), 2);
        assert_eq!(s.find_notification(&b.id[..8]).unwrap().link.as_deref(), Some("org:o1"));
        assert!(s.mark_notification(&a.id, true).unwrap() && !s.mark_notification(&a.id, true).unwrap());
        assert_eq!(s.notifications(true, 10).unwrap().len(), 1);
        assert_eq!(s.notifications(false, 10).unwrap().len(), 2);
        assert!(s.mark_notification(&a.id, false).unwrap());
        assert_eq!(s.mark_all_notifications().unwrap(), 2);
        assert_eq!(s.unread_notifications().unwrap(), 0);
    }
}
