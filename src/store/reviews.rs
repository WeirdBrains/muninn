//! Reviews (2.8): review requests on this node's journals, entries, projects, live searches
//! and model records, their reviewers, the thread (comments, suggestions, highlights and
//! votes), the revisions merges make, and the reviews this node takes part in elsewhere.
//! The rules (who may do what, merging) are `crate::review`; see `docs/reviews.md`.
use super::{Store, new_id, now_ms};
use crate::error::{Error, Result};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MIGRATION: &str = r#"
CREATE TABLE reviews (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    ref_id TEXT NOT NULL,
    model_id TEXT,
    name TEXT NOT NULL,
    title TEXT NOT NULL DEFAULT '',
    note TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'open',
    allow TEXT NOT NULL DEFAULT '[]',
    author_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX reviews_ref ON reviews(kind, ref_id);
CREATE TABLE review_reviewers (
    id TEXT PRIMARY KEY,
    review_id TEXT NOT NULL REFERENCES reviews(id) ON DELETE CASCADE,
    peer_id TEXT NOT NULL,
    label TEXT NOT NULL DEFAULT '',
    exchange_id TEXT,
    status TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX review_reviewers_exchange ON review_reviewers(exchange_id);
CREATE TABLE review_items (
    id TEXT PRIMARY KEY,
    review_id TEXT NOT NULL REFERENCES reviews(id) ON DELETE CASCADE,
    reviewer_id TEXT,
    who TEXT NOT NULL DEFAULT '',
    kind TEXT NOT NULL,
    target TEXT NOT NULL DEFAULT '',
    field TEXT NOT NULL DEFAULT '',
    span_start INTEGER,
    span_end INTEGER,
    quote TEXT NOT NULL DEFAULT '',
    body TEXT NOT NULL DEFAULT '',
    value TEXT,
    before TEXT,
    reply_to TEXT,
    state TEXT NOT NULL DEFAULT 'open',
    decided_by TEXT,
    revision INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX review_items_review ON review_items(review_id, created_at);
CREATE TABLE revisions (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    ref_id TEXT NOT NULL,
    rev INTEGER NOT NULL,
    review_id TEXT,
    before TEXT NOT NULL,
    after TEXT NOT NULL,
    items TEXT NOT NULL DEFAULT '[]',
    author_id TEXT,
    created_at INTEGER NOT NULL,
    UNIQUE(kind, ref_id, rev)
);
CREATE TABLE reviewing (
    review_id TEXT PRIMARY KEY,
    title TEXT NOT NULL DEFAULT '',
    kind TEXT NOT NULL DEFAULT '',
    name TEXT NOT NULL DEFAULT '',
    node_peer TEXT NOT NULL,
    exchange_id TEXT,
    status TEXT NOT NULL,
    allow TEXT NOT NULL DEFAULT '[]',
    info TEXT NOT NULL DEFAULT 'null',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
"#;

macro_rules! words {
    ($(#[$m:meta])* $name:ident { $($v:ident = $s:literal),+ $(,)? } default $d:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
        #[serde(rename_all = "lowercase")]
        pub enum $name { #[default] $d, $($v),+ }
        impl $name {
            pub fn as_str(self) -> &'static str {
                match self { $name::$d => words!(@s $d), $($name::$v => $s),+ }
            }
            pub fn parse(s: &str) -> Option<Self> {
                let s = s.trim().to_ascii_lowercase();
                if s == words!(@s $d) { return Some($name::$d); }
                match s.as_str() { $($s => Some($name::$v),)+ _ => None }
            }
            pub fn all() -> Vec<&'static str> { vec![words!(@s $d), $($s),+] }
        }
    };
    (@s Open) => { "open" };
    (@s Entry) => { "entry" };
    (@s Comment) => { "comment" };
    (@s Invited) => { "invited" };
    (@s Joined) => { "joined" };
}

words!(
    /// What a review is of.
    ReviewKind { Journal = "journal", Project = "project", Search = "search", Record = "record" } default Entry
);
words!(
    /// An open review takes comments; a closed one is over (its thread stays).
    ReviewStatus { Closed = "closed" } default Open
);
words!(
    /// What a thread item is.
    ReviewItemKind { Suggestion = "suggestion", Highlight = "highlight", Vote = "vote" } default Comment
);
words!(
    /// Where a thread item stands.
    ReviewItemState { Accepted = "accepted", Rejected = "rejected", Merged = "merged", Withdrawn = "withdrawn" } default Open
);
words!(
    /// A reviewer on this node's review.
    ReviewerStatus { Joined = "joined", Left = "left", Removed = "removed" } default Invited
);
words!(
    /// This node in someone else's review.
    ReviewingStatus { Left = "left", Closed = "closed" } default Joined
);

impl ReviewKind {
    pub fn label(self) -> &'static str {
        match self {
            ReviewKind::Entry => "entry",
            ReviewKind::Journal => "journal",
            ReviewKind::Project => "project",
            ReviewKind::Search => "live search",
            ReviewKind::Record => "record",
        }
    }
}

/// What reviewers may do (all of them unless the request narrows it).
pub const PERMS: &[&str] = &["view", "comment", "suggest", "highlight", "vote"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Review {
    pub id: String,
    pub kind: ReviewKind,
    /// The entry, journal, project, live search or record under review.
    pub ref_id: String,
    /// Records: the model.
    pub model_id: Option<String>,
    /// What it was called when the review was asked for.
    pub name: String,
    pub title: String,
    pub note: String,
    pub status: ReviewStatus,
    pub allow: Vec<String>,
    pub author_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Review {
    pub fn short_id(&self) -> &str {
        super::short(&self.id)
    }
    pub fn title(&self) -> String {
        if self.title.is_empty() { format!("{} {}", self.kind.label(), self.name) } else { self.title.clone() }
    }
    pub fn allows(&self, perm: &str) -> bool {
        self.allow.iter().any(|p| p == perm)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reviewer {
    pub id: String,
    pub review_id: String,
    /// The reviewer's peer id (`invite:EXCHANGE` until the link is taken).
    pub peer_id: String,
    pub label: String,
    pub exchange_id: Option<String>,
    pub status: ReviewerStatus,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Reviewer {
    pub fn short_id(&self) -> &str {
        super::short(&self.id)
    }
    pub fn title(&self) -> String {
        if !self.label.is_empty() {
            self.label.clone()
        } else if self.peer_id.starts_with("invite:") {
            format!("link {}", self.short_id())
        } else {
            super::net::short_peer(&self.peer_id).to_string()
        }
    }
}

/// One comment, suggestion, highlight or vote.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewItem {
    pub id: String,
    pub review_id: String,
    /// None: the author (this node).
    pub reviewer_id: Option<String>,
    /// Who wrote it, as shown.
    pub who: String,
    pub kind: ReviewItemKind,
    /// Journals: the entry it is about ("" for the journal itself).
    pub target: String,
    /// The field it is about ("" for the whole thing).
    pub field: String,
    /// Characters `start..end` of the field when it was written.
    pub start: Option<i64>,
    pub end: Option<i64>,
    /// The text it quotes (the text a suggestion replaces).
    pub quote: String,
    pub body: String,
    /// Suggestions: the new value (or replacement text); votes: the vote; highlights: a color.
    pub value: Option<Value>,
    /// Suggestions: the field's value when suggested.
    pub before: Option<Value>,
    /// A reply, or a vote on another item.
    pub reply_to: Option<String>,
    pub state: ReviewItemState,
    pub decided_by: Option<String>,
    /// Merged: the revision it went into.
    pub revision: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl ReviewItem {
    pub fn short_id(&self) -> &str {
        super::short(&self.id)
    }
}

/// A new thread item (the store fills in id, state and times).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NewReviewItem {
    pub kind: ReviewItemKind,
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub field: String,
    #[serde(default)]
    pub start: Option<i64>,
    #[serde(default)]
    pub end: Option<i64>,
    #[serde(default)]
    pub quote: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub value: Option<Value>,
    #[serde(default)]
    pub before: Option<Value>,
    #[serde(default)]
    pub reply_to: Option<String>,
}

/// One saved version of a reviewed thing: what it was and became, and the review items
/// merged into it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Revision {
    pub id: String,
    pub kind: ReviewKind,
    pub ref_id: String,
    pub rev: i64,
    pub review_id: Option<String>,
    pub before: Value,
    pub after: Value,
    pub items: Vec<String>,
    pub author_id: Option<String>,
    pub created_at: i64,
}

/// A review this node takes part in, on another node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reviewing {
    pub review_id: String,
    pub title: String,
    pub kind: String,
    pub name: String,
    pub node_peer: String,
    pub exchange_id: Option<String>,
    pub status: ReviewingStatus,
    pub allow: Vec<String>,
    /// The last status from the author's node.
    pub info: Value,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Reviewing {
    pub fn short_id(&self) -> &str {
        super::short(&self.review_id)
    }
    pub fn title(&self) -> String {
        if self.title.is_empty() { format!("{} {}", self.kind, self.name) } else { self.title.clone() }
    }
}

const REVIEW_COLS: &str = "id, kind, ref_id, model_id, name, title, note, status, allow, author_id, created_at, updated_at";
const REVIEWER_COLS: &str = "id, review_id, peer_id, label, exchange_id, status, created_at, updated_at";
const ITEM_COLS: &str = "id, review_id, reviewer_id, who, kind, target, field, span_start, span_end, quote, body, value, before, reply_to, state, \
                         decided_by, revision, created_at, updated_at";
const REVISION_COLS: &str = "id, kind, ref_id, rev, review_id, before, after, items, author_id, created_at";
const REVIEWING_COLS: &str = "review_id, title, kind, name, node_peer, exchange_id, status, allow, info, created_at, updated_at";

fn json_opt(s: Option<String>) -> Option<Value> {
    s.and_then(|s| serde_json::from_str(&s).ok())
}

fn row_review(r: &rusqlite::Row) -> rusqlite::Result<Review> {
    let k: String = r.get(1)?;
    let st: String = r.get(7)?;
    let a: String = r.get(8)?;
    Ok(Review {
        id: r.get(0)?,
        kind: ReviewKind::parse(&k).unwrap_or_default(),
        ref_id: r.get(2)?,
        model_id: r.get(3)?,
        name: r.get(4)?,
        title: r.get(5)?,
        note: r.get(6)?,
        status: ReviewStatus::parse(&st).unwrap_or_default(),
        allow: serde_json::from_str(&a).unwrap_or_default(),
        author_id: r.get(9)?,
        created_at: r.get(10)?,
        updated_at: r.get(11)?,
    })
}

fn row_reviewer(r: &rusqlite::Row) -> rusqlite::Result<Reviewer> {
    let st: String = r.get(5)?;
    Ok(Reviewer {
        id: r.get(0)?,
        review_id: r.get(1)?,
        peer_id: r.get(2)?,
        label: r.get(3)?,
        exchange_id: r.get(4)?,
        status: ReviewerStatus::parse(&st).unwrap_or_default(),
        created_at: r.get(6)?,
        updated_at: r.get(7)?,
    })
}

fn row_review_item(r: &rusqlite::Row) -> rusqlite::Result<ReviewItem> {
    let k: String = r.get(4)?;
    let st: String = r.get(14)?;
    Ok(ReviewItem {
        id: r.get(0)?,
        review_id: r.get(1)?,
        reviewer_id: r.get(2)?,
        who: r.get(3)?,
        kind: ReviewItemKind::parse(&k).unwrap_or_default(),
        target: r.get(5)?,
        field: r.get(6)?,
        start: r.get(7)?,
        end: r.get(8)?,
        quote: r.get(9)?,
        body: r.get(10)?,
        value: json_opt(r.get(11)?),
        before: json_opt(r.get(12)?),
        reply_to: r.get(13)?,
        state: ReviewItemState::parse(&st).unwrap_or_default(),
        decided_by: r.get(15)?,
        revision: r.get(16)?,
        created_at: r.get(17)?,
        updated_at: r.get(18)?,
    })
}

fn row_revision(r: &rusqlite::Row) -> rusqlite::Result<Revision> {
    let k: String = r.get(1)?;
    let b: String = r.get(5)?;
    let a: String = r.get(6)?;
    let i: String = r.get(7)?;
    Ok(Revision {
        id: r.get(0)?,
        kind: ReviewKind::parse(&k).unwrap_or_default(),
        ref_id: r.get(2)?,
        rev: r.get(3)?,
        review_id: r.get(4)?,
        before: serde_json::from_str(&b).unwrap_or_default(),
        after: serde_json::from_str(&a).unwrap_or_default(),
        items: serde_json::from_str(&i).unwrap_or_default(),
        author_id: r.get(8)?,
        created_at: r.get(9)?,
    })
}

fn row_reviewing(r: &rusqlite::Row) -> rusqlite::Result<Reviewing> {
    let st: String = r.get(6)?;
    let a: String = r.get(7)?;
    let i: String = r.get(8)?;
    Ok(Reviewing {
        review_id: r.get(0)?,
        title: r.get(1)?,
        kind: r.get(2)?,
        name: r.get(3)?,
        node_peer: r.get(4)?,
        exchange_id: r.get(5)?,
        status: ReviewingStatus::parse(&st).unwrap_or_default(),
        allow: serde_json::from_str(&a).unwrap_or_default(),
        info: serde_json::from_str(&i).unwrap_or_default(),
        created_at: r.get(9)?,
        updated_at: r.get(10)?,
    })
}

fn opt_json(v: &Option<Value>) -> Option<String> {
    v.as_ref().map(|v| v.to_string())
}

impl Store {
    // ---------- reviews ----------

    #[allow(clippy::too_many_arguments)]
    pub fn create_review(
        &self,
        kind: ReviewKind,
        ref_id: &str,
        model_id: Option<&str>,
        name: &str,
        title: &str,
        note: &str,
        allow: &[String],
    ) -> Result<Review> {
        for p in allow {
            if !PERMS.contains(&p.as_str()) {
                return Err(Error::invalid("--allow", format!("`{p}` (use {})", PERMS.join(", "))));
            }
        }
        let now = now_ms();
        let id = new_id();
        let author = self.current_author_id()?;
        self.conn.execute(
            &format!("INSERT INTO reviews({REVIEW_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'open', ?8, ?9, ?10, ?10)"),
            params![id, kind.as_str(), ref_id, model_id, name, title, note, serde_json::to_string(allow)?, author, now],
        )?;
        self.review(&id)?.ok_or_else(|| Error::Other("review vanished".into()))
    }

    pub fn review(&self, id: &str) -> Result<Option<Review>> {
        Ok(self.conn.query_row(&format!("SELECT {REVIEW_COLS} FROM reviews WHERE id = ?1"), [id], row_review).optional()?)
    }

    pub fn reviews(&self) -> Result<Vec<Review>> {
        let mut st = self.conn.prepare(&format!("SELECT {REVIEW_COLS} FROM reviews ORDER BY created_at DESC"))?;
        Ok(st.query_map([], row_review)?.collect::<rusqlite::Result<_>>()?)
    }

    /// Reviews of one thing.
    pub fn reviews_of(&self, kind: ReviewKind, ref_id: &str) -> Result<Vec<Review>> {
        let mut st = self.conn.prepare(&format!("SELECT {REVIEW_COLS} FROM reviews WHERE kind = ?1 AND ref_id = ?2 ORDER BY created_at DESC"))?;
        Ok(st.query_map(params![kind.as_str(), ref_id], row_review)?.collect::<rusqlite::Result<_>>()?)
    }

    /// By id, unique id prefix or exact title or name.
    pub fn find_review(&self, key: &str) -> Result<Review> {
        let all = self.reviews()?;
        let k = key.trim().to_lowercase();
        let titled: Vec<&Review> = all.iter().filter(|r| !r.title.is_empty() && r.title.to_lowercase() == k).collect();
        if titled.len() == 1 {
            return Ok(titled[0].clone());
        }
        super::pick(all, key, |r| &r.id, |r| &r.name, "review", "mnnn review list")
    }

    pub fn set_review(&self, id: &str, status: Option<ReviewStatus>, title: Option<&str>, allow: Option<&[String]>) -> Result<Review> {
        let r = self.review(id)?.ok_or_else(|| Error::invalid("review", "not found"))?;
        let st = status.unwrap_or(r.status);
        let t = title.unwrap_or(&r.title);
        let a = allow.map(|a| a.to_vec()).unwrap_or(r.allow.clone());
        self.conn.execute(
            "UPDATE reviews SET status = ?2, title = ?3, allow = ?4, updated_at = ?5 WHERE id = ?1",
            params![id, st.as_str(), t, serde_json::to_string(&a)?, now_ms()],
        )?;
        self.review(id)?.ok_or_else(|| Error::Other("review vanished".into()))
    }

    pub fn remove_review(&self, id: &str) -> Result<bool> {
        Ok(self.conn.execute("DELETE FROM reviews WHERE id = ?1", [id])? == 1)
    }

    // ---------- reviewers ----------

    pub fn add_reviewer(&self, review: &str, peer: &str, label: &str, exchange: Option<&str>, status: ReviewerStatus) -> Result<Reviewer> {
        let now = now_ms();
        let id = new_id();
        self.conn.execute(
            &format!("INSERT INTO review_reviewers({REVIEWER_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)"),
            params![id, review, peer, label, exchange, status.as_str(), now],
        )?;
        self.reviewer(&id)?.ok_or_else(|| Error::Other("reviewer vanished".into()))
    }

    pub fn reviewer(&self, id: &str) -> Result<Option<Reviewer>> {
        Ok(self.conn.query_row(&format!("SELECT {REVIEWER_COLS} FROM review_reviewers WHERE id = ?1"), [id], row_reviewer).optional()?)
    }

    pub fn reviewers(&self, review: &str) -> Result<Vec<Reviewer>> {
        let mut st = self.conn.prepare(&format!("SELECT {REVIEWER_COLS} FROM review_reviewers WHERE review_id = ?1 ORDER BY created_at"))?;
        Ok(st.query_map([review], row_reviewer)?.collect::<rusqlite::Result<_>>()?)
    }

    pub fn reviewer_by_exchange(&self, exchange: &str) -> Result<Option<Reviewer>> {
        Ok(self
            .conn
            .query_row(&format!("SELECT {REVIEWER_COLS} FROM review_reviewers WHERE exchange_id = ?1"), [exchange], row_reviewer)
            .optional()?)
    }

    pub fn find_reviewer(&self, review: &str, key: &str) -> Result<Reviewer> {
        let all = self.reviewers(review)?;
        if let Some(r) = all.iter().find(|r| r.peer_id == key.trim()) {
            return Ok(r.clone());
        }
        super::pick(all, key, |r| &r.id, |r| &r.label, "reviewer", "mnnn review show")
    }

    pub fn set_reviewer(&self, id: &str, status: Option<ReviewerStatus>, peer: Option<&str>, label: Option<&str>) -> Result<Reviewer> {
        let r = self.reviewer(id)?.ok_or_else(|| Error::invalid("reviewer", "not found"))?;
        self.conn.execute(
            "UPDATE review_reviewers SET status = ?2, peer_id = ?3, label = ?4, updated_at = ?5 WHERE id = ?1",
            params![id, status.unwrap_or(r.status).as_str(), peer.unwrap_or(&r.peer_id), label.unwrap_or(&r.label), now_ms()],
        )?;
        self.reviewer(id)?.ok_or_else(|| Error::Other("reviewer vanished".into()))
    }

    // ---------- the thread ----------

    pub fn add_review_item(&self, review: &str, reviewer: Option<&str>, who: &str, n: &NewReviewItem) -> Result<ReviewItem> {
        let now = now_ms();
        let id = new_id();
        self.conn.execute(
            &format!(
                "INSERT INTO review_items({ITEM_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, 'open', NULL, NULL, ?15, ?15)"
            ),
            params![
                id,
                review,
                reviewer,
                who,
                n.kind.as_str(),
                n.target,
                n.field,
                n.start,
                n.end,
                n.quote,
                n.body,
                opt_json(&n.value),
                opt_json(&n.before),
                n.reply_to,
                now
            ],
        )?;
        self.review_item(&id)?.ok_or_else(|| Error::Other("review item vanished".into()))
    }

    pub fn review_item(&self, id: &str) -> Result<Option<ReviewItem>> {
        Ok(self.conn.query_row(&format!("SELECT {ITEM_COLS} FROM review_items WHERE id = ?1"), [id], row_review_item).optional()?)
    }

    /// The whole thread, oldest first.
    pub fn review_items(&self, review: &str) -> Result<Vec<ReviewItem>> {
        let mut st = self.conn.prepare(&format!("SELECT {ITEM_COLS} FROM review_items WHERE review_id = ?1 ORDER BY created_at, rowid"))?;
        Ok(st.query_map([review], row_review_item)?.collect::<rusqlite::Result<_>>()?)
    }

    /// One item of a review by id or unique id prefix.
    pub fn find_review_item(&self, review: &str, key: &str) -> Result<ReviewItem> {
        super::pick(self.review_items(review)?, key, |n| &n.id, |_| "", "review item", "mnnn review show")
    }

    /// A reviewer's vote on the review (`reply_to` None) or on one item.
    pub fn review_vote(&self, review: &str, reviewer: Option<&str>, on: Option<&str>) -> Result<Option<ReviewItem>> {
        Ok(self.review_items(review)?.into_iter().find(|n| {
            n.kind == ReviewItemKind::Vote
                && n.reviewer_id.as_deref() == reviewer
                && n.reply_to.as_deref() == on
                && n.state != ReviewItemState::Withdrawn
        }))
    }

    pub fn set_review_item_state(&self, id: &str, state: ReviewItemState, by: Option<&str>, revision: Option<i64>) -> Result<ReviewItem> {
        self.conn.execute(
            "UPDATE review_items SET state = ?2, decided_by = COALESCE(?3, decided_by), revision = COALESCE(?4, revision), updated_at = ?5 WHERE id = ?1",
            params![id, state.as_str(), by, revision, now_ms()],
        )?;
        self.review_item(id)?.ok_or_else(|| Error::invalid("review item", "not found"))
    }

    /// Change a vote (or a comment's text).
    pub fn set_review_item_value(&self, id: &str, body: &str, value: Option<&Value>) -> Result<ReviewItem> {
        self.conn.execute(
            "UPDATE review_items SET body = ?2, value = ?3, updated_at = ?4 WHERE id = ?1",
            params![id, body, value.map(|v| v.to_string()), now_ms()],
        )?;
        self.review_item(id)?.ok_or_else(|| Error::invalid("review item", "not found"))
    }

    // ---------- revisions ----------

    /// Save the next revision of a thing (revision 1 is the first merge).
    pub fn add_revision(
        &self,
        kind: ReviewKind,
        ref_id: &str,
        review: Option<&str>,
        before: &Value,
        after: &Value,
        items: &[String],
    ) -> Result<Revision> {
        let rev: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(rev), 0) + 1 FROM revisions WHERE kind = ?1 AND ref_id = ?2",
            params![kind.as_str(), ref_id],
            |r| r.get(0),
        )?;
        let id = new_id();
        self.conn.execute(
            &format!("INSERT INTO revisions({REVISION_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"),
            params![
                id,
                kind.as_str(),
                ref_id,
                rev,
                review,
                before.to_string(),
                after.to_string(),
                serde_json::to_string(items)?,
                self.current_author_id()?,
                now_ms()
            ],
        )?;
        Ok(self.conn.query_row(&format!("SELECT {REVISION_COLS} FROM revisions WHERE id = ?1"), [id], row_revision)?)
    }

    /// Every revision of a thing, oldest first.
    pub fn revisions(&self, kind: ReviewKind, ref_id: &str) -> Result<Vec<Revision>> {
        let mut st = self.conn.prepare(&format!("SELECT {REVISION_COLS} FROM revisions WHERE kind = ?1 AND ref_id = ?2 ORDER BY rev"))?;
        Ok(st.query_map(params![kind.as_str(), ref_id], row_revision)?.collect::<rusqlite::Result<_>>()?)
    }

    // ---------- reviews elsewhere ----------

    pub fn save_reviewing(&self, r: &Reviewing) -> Result<()> {
        self.conn.execute(
            &format!(
                "INSERT INTO reviewing({REVIEWING_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                 ON CONFLICT(review_id) DO UPDATE SET title = ?2, kind = ?3, name = ?4, node_peer = ?5, exchange_id = ?6, status = ?7,
                 allow = ?8, info = ?9, updated_at = ?11"
            ),
            params![
                r.review_id,
                r.title,
                r.kind,
                r.name,
                r.node_peer,
                r.exchange_id,
                r.status.as_str(),
                serde_json::to_string(&r.allow)?,
                r.info.to_string(),
                r.created_at,
                r.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn reviewing(&self) -> Result<Vec<Reviewing>> {
        let mut st = self.conn.prepare(&format!("SELECT {REVIEWING_COLS} FROM reviewing ORDER BY created_at DESC"))?;
        Ok(st.query_map([], row_reviewing)?.collect::<rusqlite::Result<_>>()?)
    }

    pub fn reviewing_by_exchange(&self, exchange: &str) -> Result<Option<Reviewing>> {
        Ok(self.conn.query_row(&format!("SELECT {REVIEWING_COLS} FROM reviewing WHERE exchange_id = ?1"), [exchange], row_reviewing).optional()?)
    }

    /// By review id, unique prefix or exact title or name.
    pub fn find_reviewing(&self, key: &str) -> Result<Reviewing> {
        let all = self.reviewing()?;
        let k = key.trim().to_lowercase();
        let titled: Vec<&Reviewing> = all.iter().filter(|r| !r.title.is_empty() && r.title.to_lowercase() == k).collect();
        if titled.len() == 1 {
            return Ok(titled[0].clone());
        }
        super::pick(all, key, |r| &r.review_id, |r| &r.name, "review", "mnnn review list")
    }

    pub fn forget_reviewing(&self, review: &str) -> Result<bool> {
        Ok(self.conn.execute("DELETE FROM reviewing WHERE review_id = ?1", [review])? == 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn store() -> Store {
        Store::open_in_memory().unwrap()
    }

    #[test]
    fn reviews_reviewers_threads_and_revisions_round_trip() {
        let s = store();
        assert!(s.create_review(ReviewKind::Entry, "e1", None, "Notes", "", "", &["fly".into()]).is_err());
        let all: Vec<String> = PERMS.iter().map(|p| p.to_string()).collect();
        let r = s.create_review(ReviewKind::Entry, "e1", None, "Notes", "Check my notes", "please", &all).unwrap();
        assert_eq!(r.status, ReviewStatus::Open);
        assert!(r.allows("suggest"));
        assert_eq!(s.find_review("check my notes").unwrap().id, r.id);
        assert_eq!(s.find_review(&r.id[..6]).unwrap().id, r.id);
        assert_eq!(s.reviews_of(ReviewKind::Entry, "e1").unwrap().len(), 1);

        let v = s.add_reviewer(&r.id, "invite:x1", "ann", Some("x1"), ReviewerStatus::Invited).unwrap();
        assert_eq!(s.reviewer_by_exchange("x1").unwrap().unwrap().id, v.id);
        let v = s.set_reviewer(&v.id, Some(ReviewerStatus::Joined), Some("12D3peer"), None).unwrap();
        assert_eq!((v.status, v.peer_id.as_str(), v.label.as_str()), (ReviewerStatus::Joined, "12D3peer", "ann"));
        assert_eq!(s.find_reviewer(&r.id, "ann").unwrap().id, v.id);

        let n = s
            .add_review_item(
                &r.id,
                Some(&v.id),
                "ann",
                &NewReviewItem {
                    kind: ReviewItemKind::Suggestion,
                    field: "body".into(),
                    quote: "teh".into(),
                    value: Some(json!("the")),
                    before: Some(json!("teh cat")),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(n.value, Some(json!("the")));
        let vote = s
            .add_review_item(
                &r.id,
                Some(&v.id),
                "ann",
                &NewReviewItem { kind: ReviewItemKind::Vote, value: Some(json!("approve")), ..Default::default() },
            )
            .unwrap();
        assert_eq!(s.review_vote(&r.id, Some(&v.id), None).unwrap().unwrap().id, vote.id);
        assert!(s.review_vote(&r.id, None, None).unwrap().is_none());
        let n = s.set_review_item_state(&n.id, ReviewItemState::Accepted, Some("me"), None).unwrap();
        let n = s.set_review_item_state(&n.id, ReviewItemState::Merged, None, Some(1)).unwrap();
        assert_eq!((n.state, n.decided_by.as_deref(), n.revision), (ReviewItemState::Merged, Some("me"), Some(1)));
        assert_eq!(s.review_items(&r.id).unwrap().len(), 2);

        let a = s
            .add_revision(ReviewKind::Entry, "e1", Some(&r.id), &json!({"body": "teh cat"}), &json!({"body": "the cat"}), std::slice::from_ref(&n.id))
            .unwrap();
        let b = s.add_revision(ReviewKind::Entry, "e1", None, &json!({}), &json!({}), &[]).unwrap();
        assert_eq!((a.rev, b.rev), (1, 2));
        assert_eq!(s.revisions(ReviewKind::Entry, "e1").unwrap()[0].items, vec![n.id.clone()]);

        let r2 = s.set_review(&r.id, Some(ReviewStatus::Closed), None, None).unwrap();
        assert_eq!(r2.status, ReviewStatus::Closed);
        assert!(s.remove_review(&r.id).unwrap());
        assert!(s.review_items(&r.id).unwrap().is_empty());
        assert!(s.reviewers(&r.id).unwrap().is_empty());

        let now = now_ms();
        let g = Reviewing {
            review_id: "abc123".into(),
            title: "Their notes".into(),
            kind: "entry".into(),
            name: "x".into(),
            node_peer: "12D3".into(),
            exchange_id: Some("x9".into()),
            status: ReviewingStatus::Joined,
            allow: vec!["view".into()],
            info: Value::Null,
            created_at: now,
            updated_at: now,
        };
        s.save_reviewing(&g).unwrap();
        assert_eq!(s.find_reviewing("their notes").unwrap().review_id, "abc123");
        assert_eq!(s.reviewing_by_exchange("x9").unwrap().unwrap().allow, vec!["view".to_string()]);
        assert!(s.forget_reviewing("abc123").unwrap());
    }

    #[test]
    fn words_parse() {
        assert_eq!(ReviewKind::parse("Record"), Some(ReviewKind::Record));
        assert_eq!(ReviewKind::parse("entry"), Some(ReviewKind::Entry));
        assert_eq!(ReviewItemKind::parse("vote"), Some(ReviewItemKind::Vote));
        assert_eq!(ReviewItemState::parse("open"), Some(ReviewItemState::Open));
        assert!(ReviewKind::parse("report").is_none());
        assert_eq!(ReviewKind::all().len(), 5);
    }
}
