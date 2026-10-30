//! Reviews: ask peers to look at a journal, entry, project, live search or model record.
//! A review request makes links (offers scoped to the review); a reviewer who takes one
//! has a key exchange with this node and, through it, views the thing and adds comments,
//! suggestions, highlights and votes to the review's thread ([`net`]). The author accepts
//! or rejects suggestions and merges the accepted ones, which saves a revision.
pub mod net;
#[cfg(test)]
mod tests;

use crate::error::{Error, Result};
use crate::store::reviews::PERMS;
use crate::store::{
    NewAction, NewReviewItem, Review, ReviewItem, ReviewItemKind, ReviewItemState, ReviewKind, ReviewStatus, Reviewer, ReviewerStatus, Revision,
    SearchSpec, Store,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

/// Votes on a review.
pub const VOTES: &[&str] = &["approve", "changes", "reject"];
/// Votes on one item of the thread.
pub const ITEM_VOTES: &[&str] = &["up", "down"];

pub(crate) fn record(s: &Store, origin: &str, kind: &str, summary: String, params: Value) {
    s.note_action(&NewAction::new(kind, origin, summary).params(params));
}

/// Who is acting on a review.
#[derive(Debug, Clone)]
pub enum Who {
    /// The person at the author's node.
    Author,
    Reviewer(Box<Reviewer>),
}

impl Who {
    fn reviewer_id(&self) -> Option<&str> {
        match self {
            Who::Author => None,
            Who::Reviewer(r) => Some(&r.id),
        }
    }
    fn name(&self, s: &Store) -> String {
        match self {
            Who::Author => s.current_author().ok().flatten().map(|a| a.name).unwrap_or_else(|| "author".into()),
            Who::Reviewer(r) => r.title(),
        }
    }
}

/// `--allow view,comment` (empty: everything).
pub fn parse_allow(v: &[String]) -> Result<Vec<String>> {
    let mut out: Vec<String> = vec![];
    for p in v.iter().flat_map(|s| s.split([',', ' '])).map(|s| s.trim().to_ascii_lowercase()).filter(|s| !s.is_empty()) {
        if !PERMS.contains(&p.as_str()) {
            return Err(Error::invalid("--allow", format!("`{p}` (use {})", PERMS.join(", "))));
        }
        if !out.contains(&p) {
            out.push(p);
        }
    }
    if out.is_empty() {
        return Ok(PERMS.iter().map(|p| p.to_string()).collect());
    }
    if !out.iter().any(|p| p == "view") {
        out.insert(0, "view".into());
    }
    Ok(out)
}

/// The thing a key names: (its id, the model for records, a name to show).
pub fn resolve(s: &Store, kind: ReviewKind, key: &str) -> Result<(String, Option<String>, String)> {
    Ok(match kind {
        ReviewKind::Entry => {
            let e = s.find_entry(key)?;
            (e.id.clone(), None, e.heading())
        }
        ReviewKind::Journal => {
            let j = s.find_journal(key)?;
            (j.id, None, j.name)
        }
        ReviewKind::Project => {
            let p = s.find_project(key)?;
            (p.id, None, p.name)
        }
        ReviewKind::Search => {
            let l = s.find_live_search(key)?;
            (l.id, None, l.name)
        }
        ReviewKind::Record => {
            let (m, r) = key.split_once(['/', ':']).ok_or_else(|| Error::invalid("record", format!("`{key}` (use MODEL/RECORD)")))?;
            let m = s.find_model(m)?;
            let r = s.find_record(&m, r)?;
            (r.id.clone(), Some(m.id.clone()), format!("{}/{}", m.name, r.short_id()))
        }
    })
}

/// The model of a record review.
fn model_of(s: &Store, r: &Review) -> Result<crate::store::Model> {
    s.find_model(r.model_id.as_deref().ok_or_else(|| Error::invalid("review", "a record review without its model"))?)
}

/// A journal review's entry (`target`), checked to be in the journal.
fn entry_in(s: &Store, r: &Review, target: &str) -> Result<crate::store::Entry> {
    let e = s.find_entry(target)?;
    if e.journal_id != r.ref_id {
        return Err(Error::invalid("--entry", format!("{} is not in {}", e.short_id(), r.name)));
    }
    Ok(e)
}

/// The fields of the thing under review (or a journal review's entry) and their values.
pub fn content(s: &Store, r: &Review, target: &str) -> Result<Map<String, Value>> {
    let obj = |v: Value| v.as_object().cloned().unwrap_or_default();
    Ok(match (r.kind, target.is_empty()) {
        (ReviewKind::Entry, true) => {
            let e = s.find_entry(&r.ref_id)?;
            obj(json!({ "title": e.title, "body": e.body }))
        }
        (ReviewKind::Journal, true) => obj(json!({ "name": s.find_journal(&r.ref_id)?.name })),
        (ReviewKind::Journal, false) => {
            let e = entry_in(s, r, target)?;
            obj(json!({ "title": e.title, "body": e.body }))
        }
        (ReviewKind::Project, true) => {
            let p = s.find_project(&r.ref_id)?;
            obj(json!({ "name": p.name, "description": p.description }))
        }
        (ReviewKind::Search, true) => {
            let l = s.find_live_search(&r.ref_id)?;
            let mut m = obj(json!({ "name": l.name }));
            m.extend(obj(serde_json::to_value(&l.spec)?));
            m
        }
        (ReviewKind::Record, true) => {
            let m = model_of(s, r)?;
            let rec = s.find_record(&m, &r.ref_id)?;
            m.design.fields.iter().map(|f| (f.name.clone(), rec.values.get(&f.name).cloned().unwrap_or(Value::Null))).collect()
        }
        (_, false) => return Err(Error::invalid("--entry", "only journal reviews have entries")),
    })
}

/// The field a suggestion or quote is about when none is given.
fn default_field(r: &Review, target: &str) -> Option<&'static str> {
    match (r.kind, target.is_empty()) {
        (ReviewKind::Entry, _) | (ReviewKind::Journal, false) => Some("body"),
        (ReviewKind::Journal, true) => Some("name"),
        (ReviewKind::Project, _) => Some("description"),
        (ReviewKind::Search, _) => Some("query"),
        (ReviewKind::Record, _) => None,
    }
}

/// The review as others see it (no ids of this node's people).
pub fn card(s: &Store, r: &Review) -> Value {
    let author = r.author_id.as_deref().and_then(|a| s.author(a).ok().flatten()).map(|a| a.name);
    json!({
        "id": r.id, "title": r.title(), "kind": r.kind, "name": r.name, "note": r.note, "status": r.status,
        "allow": r.allow, "author": author, "created_at": r.created_at,
    })
}

/// What a reviewer sees: the review, the thing's fields and (journals) its entries.
pub fn view(s: &Store, r: &Review) -> Result<Value> {
    let mut v = json!({ "review": card(s, r), "content": content(s, r, "")? });
    if r.kind == ReviewKind::Journal {
        let es: Vec<Value> = s.entries(&r.ref_id)?.iter().map(|e| json!({ "id": e.id, "title": e.title, "body": e.body })).collect();
        v["entries"] = json!(es);
    }
    Ok(v)
}

/// The thread, as sent to reviewers.
pub fn thread(s: &Store, r: &Review) -> Result<Vec<ReviewItem>> {
    s.review_items(&r.id)
}

/// Ask for a review of one of this node's things.
pub fn request(s: &Store, origin: &str, kind: ReviewKind, key: &str, title: &str, note: &str, allow: &[String]) -> Result<Review> {
    let (id, model, name) = resolve(s, kind, key)?;
    let allow = parse_allow(allow)?;
    let r = s.create_review(kind, &id, model.as_deref(), &name, title.trim(), note.trim(), &allow)?;
    record(
        s,
        origin,
        "review.request",
        format!("review of {} {} requested", kind.label(), name),
        json!({ "review": r.id, "kind": kind, "ref": id, "name": name, "allow": allow }),
    );
    Ok(r)
}

/// What one of the thread's items asks for (from the CLI, TUI or a reviewer's node).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Ask {
    pub kind: ReviewItemKind,
    /// Journal reviews: the entry.
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub field: String,
    /// Anchor on this text of the field.
    #[serde(default)]
    pub at: Option<String>,
    /// Which occurrence of `at` (1-based; 0 or 1: the first).
    #[serde(default)]
    pub nth: usize,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub value: Option<Value>,
    #[serde(default)]
    pub reply_to: Option<String>,
}

fn perm_of(k: ReviewItemKind) -> &'static str {
    match k {
        ReviewItemKind::Comment => "comment",
        ReviewItemKind::Suggestion => "suggest",
        ReviewItemKind::Highlight => "highlight",
        ReviewItemKind::Vote => "vote",
    }
}

fn text_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        v => v.to_string(),
    }
}

/// Add a comment, suggestion, highlight or vote (a vote replaces the same person's vote).
pub fn add(s: &Store, origin: &str, r: &Review, who: &Who, ask: &Ask) -> Result<ReviewItem> {
    if r.status != ReviewStatus::Open {
        return Err(Error::invalid("review", format!("{} is closed", r.title())));
    }
    if let Who::Reviewer(v) = who {
        if v.status != ReviewerStatus::Joined {
            return Err(Error::invalid("review", format!("you are {} in {}", v.status.as_str(), r.title())));
        }
        if !r.allows(perm_of(ask.kind)) {
            return Err(Error::invalid("review", format!("reviewers may not {} in {}", perm_of(ask.kind), r.title())));
        }
    }
    let target = if ask.target.trim().is_empty() {
        String::new()
    } else if r.kind == ReviewKind::Journal {
        entry_in(s, r, ask.target.trim())?.id
    } else {
        return Err(Error::invalid("--entry", "only journal reviews have entries"));
    };
    let reply_to = match ask.reply_to.as_deref().map(str::trim).filter(|k| !k.is_empty()) {
        Some(k) => Some(s.find_review_item(&r.id, k)?.id),
        None => None,
    };
    let fields = content(s, r, &target)?;
    let anchored = ask.at.as_deref().is_some_and(|a| !a.is_empty());
    let field = match ask.field.trim() {
        "" if ask.kind == ReviewItemKind::Suggestion || anchored => default_field(r, &target)
            .map(str::to_string)
            .ok_or_else(|| Error::invalid("--field", format!("which field? ({})", fields.keys().cloned().collect::<Vec<_>>().join(", "))))?,
        f => f.to_string(),
    };
    if !field.is_empty() && !fields.contains_key(&field) {
        return Err(Error::invalid("--field", format!("`{field}` (use {})", fields.keys().cloned().collect::<Vec<_>>().join(", "))));
    }
    let current = fields.get(&field).cloned().unwrap_or(Value::Null);
    let mut n = NewReviewItem { kind: ask.kind, target, field: field.clone(), reply_to, body: ask.body.trim().to_string(), ..Default::default() };
    if let Some(at) = ask.at.as_deref().filter(|a| !a.is_empty()) {
        let sp = crate::store::journals::select(&text_of(&current), at, ask.nth)?;
        n.start = Some(sp.start as i64);
        n.end = Some(sp.end as i64);
        n.quote = at.to_string();
    }
    let who_name = who.name(s);
    let (kind, summary) = match ask.kind {
        ReviewItemKind::Comment => {
            if n.body.is_empty() {
                return Err(Error::invalid("comment", "is empty"));
            }
            ("review.comment", format!("{who_name} commented on {}", r.title()))
        }
        ReviewItemKind::Highlight => {
            if n.quote.is_empty() {
                return Err(Error::invalid("highlight", "give the text to highlight (--at)"));
            }
            n.value = ask.value.clone().filter(|v| !v.is_null());
            ("review.highlight", format!("{who_name} highlighted \"{}\" in {}", n.quote, r.title()))
        }
        ReviewItemKind::Suggestion => {
            let v = ask.value.clone().ok_or_else(|| Error::invalid("suggestion", "give the new text or value (--to)"))?;
            if !n.quote.is_empty() && !v.is_string() {
                return Err(Error::invalid("suggestion", "replacing quoted text takes text"));
            }
            n.value = Some(v);
            n.before = Some(current);
            ("review.suggest", format!("{who_name} suggested a change to {} of {}", field, r.title()))
        }
        ReviewItemKind::Vote => {
            let v = ask.value.as_ref().and_then(|v| v.as_str()).map(|v| v.trim().to_ascii_lowercase()).unwrap_or_default();
            let ok = if n.reply_to.is_some() { ITEM_VOTES } else { VOTES };
            if !ok.contains(&v.as_str()) {
                return Err(Error::invalid("vote", format!("`{v}` (use {})", ok.join(", "))));
            }
            n.value = Some(json!(v));
            if let Some(old) = s.review_vote(&r.id, who.reviewer_id(), n.reply_to.as_deref())? {
                let it = s.set_review_item_value(&old.id, &n.body, n.value.as_ref())?;
                record(
                    s,
                    origin,
                    "review.vote",
                    format!("{who_name} changed their vote on {} to {v}", r.title()),
                    json!({ "review": r.id, "item": it.id, "vote": v, "on": it.reply_to, "by": who_name }),
                );
                return Ok(it);
            }
            ("review.vote", format!("{who_name} voted {v} on {}", r.title()))
        }
    };
    let it = s.add_review_item(&r.id, who.reviewer_id(), &who_name, &n)?;
    record(
        s,
        origin,
        kind,
        summary,
        json!({ "review": r.id, "item": it.id, "kind": it.kind, "field": it.field, "target": it.target, "on": it.reply_to, "by": who_name }),
    );
    Ok(it)
}

/// Take back one of your own items.
pub fn withdraw(s: &Store, origin: &str, r: &Review, who: &Who, key: &str) -> Result<ReviewItem> {
    let it = s.find_review_item(&r.id, key)?;
    if it.reviewer_id.as_deref() != who.reviewer_id() {
        return Err(Error::invalid("review item", "not yours"));
    }
    if matches!(it.state, ReviewItemState::Merged | ReviewItemState::Withdrawn) {
        return Err(Error::invalid("review item", format!("{} is {}", it.short_id(), it.state.as_str())));
    }
    let it = s.set_review_item_state(&it.id, ReviewItemState::Withdrawn, None, None)?;
    record(
        s,
        origin,
        "review.withdraw",
        format!("{} withdrew {} from {}", who.name(s), it.short_id(), r.title()),
        json!({ "review": r.id, "item": it.id }),
    );
    Ok(it)
}

/// Accept or reject suggestions (`keys` empty: every open one).
pub fn decide(s: &Store, origin: &str, r: &Review, keys: &[String], accept: bool) -> Result<Vec<ReviewItem>> {
    let items: Vec<ReviewItem> = if keys.is_empty() {
        s.review_items(&r.id)?.into_iter().filter(|i| i.kind == ReviewItemKind::Suggestion && i.state == ReviewItemState::Open).collect()
    } else {
        keys.iter().map(|k| s.find_review_item(&r.id, k)).collect::<Result<_>>()?
    };
    for i in &items {
        if i.kind != ReviewItemKind::Suggestion {
            return Err(Error::invalid(
                "review item",
                format!("{} is a {}; only suggestions are accepted or rejected", i.short_id(), i.kind.as_str()),
            ));
        }
        if matches!(i.state, ReviewItemState::Merged | ReviewItemState::Withdrawn) {
            return Err(Error::invalid("review item", format!("{} is {}", i.short_id(), i.state.as_str())));
        }
    }
    let by = Who::Author.name(s);
    let st = if accept { ReviewItemState::Accepted } else { ReviewItemState::Rejected };
    let out: Vec<ReviewItem> = items.iter().map(|i| s.set_review_item_state(&i.id, st, Some(&by), None)).collect::<Result<_>>()?;
    if !out.is_empty() {
        let (kind, verb) = if accept { ("review.accept", "accepted") } else { ("review.reject", "rejected") };
        let ids: Vec<&str> = out.iter().map(|i| i.id.as_str()).collect();
        record(
            s,
            origin,
            kind,
            format!("{} suggestion{} {verb} in {}", out.len(), if out.len() == 1 { "" } else { "s" }, r.title()),
            json!({ "review": r.id, "items": ids, "by": by }),
        );
    }
    Ok(out)
}

/// What a merge did.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Merged {
    pub revisions: Vec<Revision>,
    pub merged: Vec<String>,
    /// Accepted suggestions left as they were: (item, why).
    pub conflicts: Vec<(String, String)>,
}

/// Apply one suggestion to a field value; Err: why it no longer fits.
fn apply_one(it: &ReviewItem, cur: &Value, force: bool) -> std::result::Result<Value, String> {
    let new = it.value.clone().unwrap_or(Value::Null);
    if it.quote.is_empty() {
        if !force && it.before.as_ref().is_some_and(|b| b != cur) {
            return Err(format!("{} changed since the suggestion", it.field));
        }
        return Ok(new);
    }
    let text = text_of(cur);
    let with = text_of(&new);
    let at = match (it.start, it.end) {
        (Some(a), Some(b)) if crate::store::journals::chars_slice(&text, a as usize, b as usize) == it.quote => Some((a as usize, b as usize)),
        _ => crate::store::journals::find_all(&text, &it.quote).first().map(|sp| (sp.start, sp.end)),
    };
    let (a, b) = at.ok_or_else(|| format!("\"{}\" is no longer in {}", it.quote, it.field))?;
    let mut out: String = text.chars().take(a).collect();
    out.push_str(&with);
    out.extend(text.chars().skip(b));
    Ok(Value::String(out))
}

/// Write changed fields back to the thing, recording the edit as its own kind of action.
fn write(s: &Store, origin: &str, r: &Review, target: &str, changed: &Map<String, Value>, all: &Map<String, Value>) -> Result<(ReviewKind, String)> {
    let st = |k: &str| changed.get(k).map(text_of);
    Ok(match (r.kind, target.is_empty()) {
        (ReviewKind::Entry, _) | (ReviewKind::Journal, false) => {
            let id = if target.is_empty() { r.ref_id.clone() } else { target.to_string() };
            let e = s.update_entry(&id, st("title").as_deref(), st("body").as_deref(), None)?;
            record(
                s,
                origin,
                "entry.edit",
                format!("merge review into entry {}", e.heading()),
                json!({ "journal": e.journal_id, "entry": e.id, "title": e.heading(), "review": r.id }),
            );
            (ReviewKind::Entry, e.id)
        }
        (ReviewKind::Journal, true) => {
            let j = s.update_journal(&r.ref_id, st("name").as_deref(), None)?;
            record(
                s,
                origin,
                "journal.edit",
                format!("merge review into journal {}", j.name),
                json!({ "journal": j.id, "name": j.name, "review": r.id }),
            );
            (ReviewKind::Journal, j.id)
        }
        (ReviewKind::Project, _) => {
            let p = s.update_project(&r.ref_id, st("name").as_deref(), st("description").as_deref())?;
            record(
                s,
                origin,
                "project.edit",
                format!("merge review into project {}", p.name),
                json!({ "project": p.id, "name": p.name, "review": r.id }),
            );
            (ReviewKind::Project, p.id)
        }
        (ReviewKind::Search, _) => {
            let mut spec = all.clone();
            spec.remove("name");
            let spec: SearchSpec = serde_json::from_value(Value::Object(spec)).map_err(|e| Error::invalid("search", e.to_string()))?;
            let l = s.update_live_search(&r.ref_id, st("name").as_deref(), Some(&spec))?;
            record(
                s,
                origin,
                "search.edit",
                format!("merge review into live search {}", l.name),
                json!({ "search": l.id, "name": l.name, "spec": l.spec, "review": r.id }),
            );
            (ReviewKind::Search, l.id)
        }
        (ReviewKind::Record, _) => {
            let m = model_of(s, r)?;
            let rec = s.update_record(&m, &r.ref_id, changed)?;
            let mut a = crate::designer::act_record_update(&m, Some(&rec), 1, &[], changed);
            a.summary = format!("merge review into {} record {}", m.name, rec.short_id());
            if let Some(p) = a.params.as_object_mut() {
                p.insert("review".into(), json!(r.id));
            }
            record(s, origin, a.kind, a.summary, a.params);
            (ReviewKind::Record, rec.id)
        }
    })
}

/// Merge the accepted suggestions: each changed thing gets a new revision. Suggestions that
/// no longer fit (the text moved or the field changed) are left accepted unless `force`.
pub fn merge(s: &Store, origin: &str, r: &Review, force: bool) -> Result<Merged> {
    let accepted: Vec<ReviewItem> =
        s.review_items(&r.id)?.into_iter().filter(|i| i.kind == ReviewItemKind::Suggestion && i.state == ReviewItemState::Accepted).collect();
    let mut targets: Vec<String> = vec![];
    for i in &accepted {
        if !targets.contains(&i.target) {
            targets.push(i.target.clone());
        }
    }
    let mut out = Merged::default();
    for t in targets {
        let before = content(s, r, &t)?;
        let mut work = before.clone();
        let mut used = vec![];
        for it in accepted.iter().filter(|i| i.target == t) {
            let cur = work.get(&it.field).cloned().unwrap_or(Value::Null);
            match apply_one(it, &cur, force) {
                Ok(v) => {
                    work.insert(it.field.clone(), v);
                    used.push(it.id.clone());
                }
                Err(why) => out.conflicts.push((it.id.clone(), why)),
            }
        }
        if used.is_empty() {
            continue;
        }
        let changed: Map<String, Value> = work.iter().filter(|(k, v)| before.get(*k) != Some(*v)).map(|(k, v)| (k.clone(), v.clone())).collect();
        let (kind, ref_id) = if changed.is_empty() {
            match (r.kind, t.is_empty()) {
                (ReviewKind::Journal, false) => (ReviewKind::Entry, t.clone()),
                _ => (r.kind, r.ref_id.clone()),
            }
        } else {
            write(s, origin, r, &t, &changed, &work)?
        };
        let after = content(s, r, &t)?;
        let rev = s.add_revision(kind, &ref_id, Some(&r.id), &Value::Object(before), &Value::Object(after), &used)?;
        for id in &used {
            s.set_review_item_state(id, ReviewItemState::Merged, None, Some(rev.rev))?;
        }
        out.merged.extend(used);
        out.revisions.push(rev);
    }
    if !out.merged.is_empty() || !out.conflicts.is_empty() {
        let revs: Vec<Value> = out.revisions.iter().map(|v| json!({ "kind": v.kind, "ref": v.ref_id, "rev": v.rev })).collect();
        record(
            s,
            origin,
            "review.merge",
            format!(
                "{} suggestion{} of {} merged{}",
                out.merged.len(),
                if out.merged.len() == 1 { "" } else { "s" },
                r.title(),
                if out.conflicts.is_empty() { String::new() } else { format!(", {} left (conflicts)", out.conflicts.len()) }
            ),
            json!({ "review": r.id, "items": out.merged, "revisions": revs, "conflicts": out.conflicts.len() }),
        );
    }
    Ok(out)
}

/// Every revision of the thing a review is of (a journal review: the journal's).
pub fn revisions(s: &Store, kind: ReviewKind, key: &str) -> Result<Vec<Revision>> {
    let (id, _, _) = resolve(s, kind, key)?;
    s.revisions(kind, &id)
}

/// Votes so far: (approve, changes, reject).
pub fn tally(items: &[ReviewItem]) -> (usize, usize, usize) {
    let mut t = (0, 0, 0);
    for i in items.iter().filter(|i| i.kind == ReviewItemKind::Vote && i.reply_to.is_none() && i.state != ReviewItemState::Withdrawn) {
        match i.value.as_ref().and_then(|v| v.as_str()) {
            Some("approve") => t.0 += 1,
            Some("changes") => t.1 += 1,
            Some("reject") => t.2 += 1,
            _ => {}
        }
    }
    t
}

/// One line for a thread item.
pub fn line(i: &ReviewItem) -> String {
    let at = if i.quote.is_empty() { String::new() } else { format!(" on \"{}\"", i.quote) };
    let field = if i.field.is_empty() { String::new() } else { format!(" [{}]", i.field) };
    let what = match i.kind {
        ReviewItemKind::Comment => format!("comment{field}{at}: {}", i.body),
        ReviewItemKind::Highlight => {
            format!("highlight{field}{at}{}", i.value.as_ref().map(|c| format!(" ({})", text_of(c))).unwrap_or_default())
        }
        ReviewItemKind::Suggestion => {
            let to = i.value.as_ref().map(text_of).unwrap_or_default();
            let why = if i.body.is_empty() { String::new() } else { format!(" ({})", i.body) };
            if i.quote.is_empty() { format!("suggest{field} → {to}{why}") } else { format!("suggest{field}{at} → \"{to}\"{why}") }
        }
        ReviewItemKind::Vote => format!("vote {}", i.value.as_ref().map(text_of).unwrap_or_default()),
    };
    let re = i.reply_to.as_deref().map(|r| format!(" ↳{}", crate::store::projects::short_of(r))).unwrap_or_default();
    format!("{} {:<9} {}{re}: {what}", i.short_id(), i.state.as_str(), i.who)
}

/// Close a review: no more items, and reviewers' exchanges end (see [`net::close`]).
pub fn close_local(s: &Store, origin: &str, r: &Review) -> Result<Review> {
    let r = s.set_review(&r.id, Some(ReviewStatus::Closed), None, None)?;
    record(s, origin, "review.close", format!("review {} closed", r.title()), json!({ "review": r.id }));
    Ok(r)
}

/// Open a closed review again (reviewers need new links).
pub fn reopen(s: &Store, origin: &str, r: &Review) -> Result<Review> {
    let r = s.set_review(&r.id, Some(ReviewStatus::Open), None, None)?;
    record(s, origin, "review.reopen", format!("review {} opened again", r.title()), json!({ "review": r.id }));
    Ok(r)
}

/// Delete a review and its thread (revisions it made stay).
pub fn remove(s: &Store, origin: &str, r: &Review) -> Result<()> {
    s.remove_review(&r.id)?;
    record(s, origin, "review.remove", format!("review {} removed", r.title()), json!({ "review": r.id }));
    Ok(())
}
