//! Notifications (`mnnn notify` / TUI Notifications, `!`).
use super::data::{open, open_write};
use muninn_core::notify;
use muninn_core::store::{NewNotification, Notification, Store};

const ORIGIN: &str = "app";

pub struct NotificationInfo {
    pub id: String,
    pub short_id: String,
    pub kind: String,
    pub level: String,
    pub title: String,
    pub body: String,
    pub link: String,
    pub link_label: String,
    pub action_id: i64,
    pub created_at: i64,
    pub read: bool,
    pub read_at: i64,
}

pub struct NotificationKindInfo {
    pub kind: String,
    pub about: String,
    pub muted: bool,
}

fn info(store: &Store, n: Notification) -> NotificationInfo {
    let link = n.link.clone().unwrap_or_default();
    let link_label = if link.is_empty() { String::new() } else { notify::describe_link(store, &link) };
    NotificationInfo {
        short_id: n.id[..n.id.len().min(8)].to_string(),
        action_id: n.action_id.unwrap_or(0),
        read: n.read_at.is_some(),
        read_at: n.read_at.unwrap_or(0),
        link_label,
        link,
        id: n.id,
        kind: n.kind,
        level: n.level,
        title: n.title,
        body: n.body,
        created_at: n.created_at,
    }
}

pub(crate) fn notifications_of(store: &Store, unread: bool, limit: usize) -> anyhow::Result<Vec<NotificationInfo>> {
    Ok(store.notifications(unread, limit)?.into_iter().map(|n| info(store, n)).collect())
}

/// `mnnn notify list` / TUI Notifications….
pub fn notifications(unread: bool, limit: i64) -> anyhow::Result<Vec<NotificationInfo>> {
    let limit = if limit <= 0 { 50 } else { limit as usize };
    notifications_of(&open()?, unread, limit)
}

pub fn notifications_unread_count() -> anyhow::Result<i64> {
    Ok(open()?.unread_notifications()? as i64)
}

/// `mnnn notify show`: mark read and return the notification.
pub fn notification_show(id: String) -> anyhow::Result<NotificationInfo> {
    let store = open_write()?;
    let n = store.find_notification(id.trim())?;
    store.mark_notification(&n.id, true)?;
    let n = store.find_notification(&n.id)?;
    Ok(info(&store, n))
}

/// `mnnn notify read` (one).
pub fn notification_read(id: String) -> anyhow::Result<String> {
    let store = open_write()?;
    let n = store.find_notification(id.trim())?;
    store.mark_notification(&n.id, true)?;
    Ok(format!("read: {}", n.title))
}

/// `mnnn notify unread`.
pub fn notification_unread(id: String) -> anyhow::Result<String> {
    let store = open_write()?;
    let n = store.find_notification(id.trim())?;
    store.mark_notification(&n.id, false)?;
    Ok(format!("unread: {}", n.title))
}

/// `mnnn notify read --all` / TUI `R`.
pub fn notification_read_all() -> anyhow::Result<String> {
    let k = open_write()?.mark_all_notifications()?;
    Ok(format!("marked {k} read"))
}

/// `mnnn notify send`.
pub fn notification_send(title: String, body: String, link: Option<String>, level: String) -> anyhow::Result<NotificationInfo> {
    let store = open_write()?;
    let level = if level.trim().is_empty() { "info".into() } else { level };
    if !["info", "warn", "error"].contains(&level.as_str()) {
        return Err(anyhow::anyhow!("level `{level}`: info, warn or error"));
    }
    let n = notify::add(
        &store,
        ORIGIN,
        &NewNotification {
            kind: "notify".into(),
            level,
            title,
            body,
            link: link.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
            action_id: None,
        },
    )?;
    Ok(info(&store, n))
}

/// `mnnn notify kinds`.
pub fn notification_kinds() -> anyhow::Result<Vec<NotificationKindInfo>> {
    let store = open()?;
    let m = notify::muted(&store);
    Ok(notify::BUILTINS
        .iter()
        .map(|b| NotificationKindInfo { kind: b.kind.into(), about: b.about.into(), muted: m.iter().any(|k| k == b.kind) })
        .collect())
}

/// `mnnn notify mute` / TUI `m`.
pub fn notification_mute(kind: String) -> anyhow::Result<String> {
    notify::set_muted(&open_write()?, kind.trim(), true)?;
    Ok(format!("muted {}", kind.trim()))
}

/// `mnnn notify unmute`.
pub fn notification_unmute(kind: String) -> anyhow::Result<String> {
    notify::set_muted(&open_write()?, kind.trim(), false)?;
    Ok(format!("unmuted {}", kind.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use muninn_core::store::{NewNotification, Store};

    #[test]
    fn notifications_send_list_read_without_net() {
        let s = Store::open_in_memory().unwrap();
        assert!(notifications_of(&s, false, 50).unwrap().is_empty());
        notify::add(
            &s,
            "test",
            &NewNotification {
                kind: "notify".into(),
                level: "info".into(),
                title: "hello".into(),
                body: "there".into(),
                link: None,
                action_id: None,
            },
        )
        .unwrap();
        let list = notifications_of(&s, false, 50).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].title, "hello");
        assert!(!list[0].read);
        s.mark_notification(&list[0].id, true).unwrap();
        let list = notifications_of(&s, false, 50).unwrap();
        assert!(list[0].read);
        assert!(notifications_of(&s, true, 50).unwrap().is_empty());
    }

    #[test]
    fn writes_need_unlock() {
        crate::api::data::lock();
        assert!(notifications(false, 50).is_err());
        assert!(notification_send("x".into(), "".into(), None, "info".into()).is_err());
    }
}
