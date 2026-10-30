//! The rule engine: matches every recorded action against the enabled rules and runs the
//! then-actions of those that match. Rules are stored by [`crate::store::rules`]; the
//! command-line syntax for conditions and then-actions is parsed and printed here so the
//! CLI and the TUI share it. See `docs/rules.md`.
//!
//! Loop protection: actions of kind `rule.*` never trigger rules; a rule already running
//! in the current chain is not started again; chains stop at [`MAX_DEPTH`] rules and one
//! outermost action starts at most [`MAX_RUNS`] runs. Blocked runs are recorded as
//! `rule.run` with status `blocked`.
//!
//! Provenance: while a rule runs, every action it records carries `rule_chain` (rule ids,
//! outermost first) in its params, and is a child of the `rule.run` action in the history.
use crate::error::{Error, Result};
use crate::export::files::{OutFormat, Target};
use crate::store::Store;
use crate::store::{ActionRecord, ApplyOpts, Condition, FieldOp, ItemKind, ItemRef, Model, NewAction, Op, OwnerKind, Rule, Then};
use serde::Serialize;
use serde_json::{Value, json};
use std::path::PathBuf;

/// Most rules in one chain (a rule whose action triggers a rule whose action triggers …).
pub const MAX_DEPTH: usize = 3;
/// Most rule runs started by one outermost action.
pub const MAX_RUNS: u32 = 50;
/// Origin of actions taken by rules.
pub const ORIGIN: &str = "rule";

/// One kind of action rules can react to.
pub struct Event {
    pub kind: &'static str,
    pub about: &'static str,
    /// Params fields (`name: meaning`).
    pub fields: &'static [&'static str],
}

const ITEM: &[&str] = &["project: project id", "item: item number", "kind: item kind", "ref: item id"];
const JOURNAL: &[&str] = &["journal: journal id", "name: journal name"];
const ENTRY: &[&str] = &["journal: journal id", "entry: entry id", "title: entry title"];
const NOTE: &[&str] = &["journal: journal id", "entry: entry id", "note: note id"];
const SEARCH: &[&str] = &["search: live search id", "name: search name", "spec: the search"];
const PROJECT: &[&str] = &["project: project id", "name: project name"];
const REPORT: &[&str] = &["report: report id", "name: report name"];
const SECTION: &[&str] = &["report: report id", "section: section id", "kind: section kind", "position: position"];
const DOC_EXPORT: &[&str] = &["format: output format", "out: output path", "files: files written"];
const STATE: &[&str] = &["before.*: the state before", "after.*: the state after"];
const RULE: &[&str] = &["rule: rule id", "name: rule name"];
const MODEL: &[&str] = &["model: model id", "name: model name", "version: applied version"];
const MIGRATE: &[&str] = &[
    "model: model id",
    "name: model name",
    "from: version before",
    "to: version after",
    "steps: migration steps",
    "lost: values lost",
    "records: records migrated",
];
const RECORD: &[&str] = &[
    "model: model id",
    "name: model name",
    "record: record id (one record)",
    "count: records changed",
    "where: the conditions used",
    "values.FIELD: the values",
];

/// Every action kind in the history, with its fields. `mnnn rule events` prints this.
pub const EVENTS: &[Event] = &[
    Event {
        kind: "import",
        about: "an import finished",
        fields: &[
            "run: import run id",
            "status: completed, failed, …",
            "files_done: files read",
            "files_total: files given",
            "files: the files",
            "inserted: bars added",
        ],
    },
    Event {
        kind: "export",
        about: "bars exported",
        fields: &["out: output path", "format: format", "bars: bars written", "scope: what was exported", "from: start", "to: end"],
    },
    Event { kind: "journal.export", about: "a journal exported", fields: DOC_EXPORT },
    Event { kind: "entry.export", about: "journal entries exported", fields: DOC_EXPORT },
    Event { kind: "report.export", about: "a report exported", fields: DOC_EXPORT },
    Event { kind: "project.export", about: "a project exported", fields: DOC_EXPORT },
    Event { kind: "journal.print", about: "a journal printed", fields: JOURNAL },
    Event { kind: "source.add", about: "a source added", fields: &["source: source id", "kind: source kind"] },
    Event { kind: "source.rename", about: "a source renamed", fields: &["from: old name", "to: new name", "bars: bars"] },
    Event { kind: "source.remove", about: "a source removed", fields: &["source: source name", "bars: bars removed"] },
    Event { kind: "symbol.add", about: "a symbol added", fields: &["symbol: symbol", "aliases: aliases"] },
    Event { kind: "author.add", about: "an author added", fields: &["author: author id", "name: name"] },
    Event { kind: "author.edit", about: "an author changed", fields: &["author: author id", "name: name"] },
    Event { kind: "author.remove", about: "an author removed", fields: &["author: author id", "name: name"] },
    Event { kind: "author.use", about: "the current author changed", fields: &["author: author id", "name: name"] },
    Event { kind: "search.save", about: "a live search saved", fields: SEARCH },
    Event { kind: "search.fork", about: "a live search forked", fields: SEARCH },
    Event { kind: "search.edit", about: "a live search changed", fields: SEARCH },
    Event { kind: "search.remove", about: "a live search removed", fields: SEARCH },
    Event { kind: "project.create", about: "a project created", fields: PROJECT },
    Event { kind: "project.edit", about: "a project changed", fields: PROJECT },
    Event { kind: "project.remove", about: "a project removed", fields: PROJECT },
    Event { kind: "project.add", about: "an item added to a project", fields: ITEM },
    Event { kind: "project.drop", about: "an item taken out of a project", fields: ITEM },
    Event { kind: "project.import", about: "a project archive imported", fields: PROJECT },
    Event { kind: "journal.create", about: "a journal created", fields: JOURNAL },
    Event { kind: "journal.edit", about: "a journal changed", fields: JOURNAL },
    Event { kind: "journal.remove", about: "a journal removed", fields: JOURNAL },
    Event { kind: "journal.fork", about: "a journal forked", fields: JOURNAL },
    Event { kind: "entry.add", about: "an entry written", fields: ENTRY },
    Event { kind: "entry.edit", about: "an entry changed", fields: ENTRY },
    Event { kind: "entry.remove", about: "an entry removed", fields: ENTRY },
    Event { kind: "entry.copy", about: "an entry copied", fields: ENTRY },
    Event { kind: "note.add", about: "a note added", fields: NOTE },
    Event { kind: "note.edit", about: "a note changed", fields: NOTE },
    Event { kind: "note.remove", about: "a note removed", fields: &["journal: journal id", "id: note id"] },
    Event { kind: "highlight.add", about: "a highlight added", fields: NOTE },
    Event { kind: "highlight.remove", about: "a highlight removed", fields: &["journal: journal id", "id: highlight id"] },
    Event { kind: "image.add", about: "an image attached", fields: NOTE },
    Event { kind: "report.create", about: "a report created", fields: REPORT },
    Event { kind: "report.edit", about: "a report changed", fields: REPORT },
    Event { kind: "report.remove", about: "a report removed", fields: REPORT },
    Event { kind: "report.section.add", about: "a report section added", fields: SECTION },
    Event { kind: "report.section.edit", about: "a report section changed", fields: SECTION },
    Event { kind: "report.section.move", about: "a report section moved", fields: SECTION },
    Event { kind: "report.section.remove", about: "a report section removed", fields: SECTION },
    Event { kind: "filter", about: "the TUI filter changed", fields: STATE },
    Event { kind: "filter.time", about: "the TUI time range changed", fields: STATE },
    Event { kind: "filter.clear", about: "the TUI filters cleared", fields: STATE },
    Event { kind: "theme", about: "the theme changed", fields: STATE },
    Event { kind: "theme.mode", about: "light or dark mode changed", fields: STATE },
    Event { kind: "model.create", about: "a model created", fields: MODEL },
    Event {
        kind: "model.edit",
        about: "a model's draft or name changed",
        fields: &["model: model id", "name: model name", "ops: field changes", "draft: a draft is waiting"],
    },
    Event { kind: "model.apply", about: "a model's design applied (a migration)", fields: MIGRATE },
    Event { kind: "model.rollback", about: "a model rolled back to an earlier design", fields: MIGRATE },
    Event { kind: "model.remove", about: "a model removed", fields: &["model: model id", "name: model name", "records: records removed"] },
    Event { kind: "record.create", about: "a model record created", fields: RECORD },
    Event { kind: "record.update", about: "model records changed", fields: RECORD },
    Event { kind: "record.delete", about: "model records deleted", fields: RECORD },
    Event { kind: "rule.create", about: "a rule created (never triggers rules)", fields: RULE },
    Event { kind: "rule.edit", about: "a rule changed (never triggers rules)", fields: RULE },
    Event { kind: "rule.remove", about: "a rule removed (never triggers rules)", fields: RULE },
    Event { kind: "rule.enable", about: "a rule turned on (never triggers rules)", fields: RULE },
    Event { kind: "rule.disable", about: "a rule turned off (never triggers rules)", fields: RULE },
    Event {
        kind: "rule.run",
        about: "a rule ran (never triggers rules)",
        fields: &["rule: rule id", "trigger: action id", "status: ok, failed or blocked", "results: per then-action"],
    },
];

/// Fields every action has (besides its params).
pub const COMMON_FIELDS: &[&str] =
    &["kind: action kind", "summary: what happened", "origin: cli, tui or rule", "author: author name", "action: action number"];

fn pattern_matches(p: &str, kind: &str) -> bool {
    let p = p.trim();
    p == "*" || p == kind || p.strip_suffix(".*").is_some_and(|pre| kind.starts_with(&format!("{pre}.")) || kind == pre)
}

/// Check an `--on` pattern against the event catalog.
pub fn check_pattern(p: &str) -> Result<String> {
    let p = p.trim();
    if p.is_empty() {
        return Err(Error::invalid("--on", "is empty"));
    }
    if EVENTS.iter().any(|e| pattern_matches(p, e.kind)) {
        Ok(p.to_string())
    } else {
        Err(Error::invalid("--on", format!("`{p}` matches no action kind (see `mnnn rule events`)")))
    }
}

/// `--on import,journal.*` and repeated flags both work.
pub fn parse_on(v: &[String]) -> Result<Vec<String>> {
    v.iter().flat_map(|s| s.split([',', ' '])).filter(|s| !s.trim().is_empty()).map(check_pattern).collect()
}

// ---------------------------------------------------------------------------- syntax

/// Split `a; b; "c;d"` on semicolons outside quotes (the TUI takes lists on one line).
pub fn split_list(s: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let mut q: Option<char> = None;
    for c in s.chars() {
        match (q, c) {
            (None, '"' | '\'') => {
                q = Some(c);
                cur.push(c)
            }
            (Some(o), c) if c == o => {
                q = None;
                cur.push(c)
            }
            (None, ';') => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

/// A word, and whether it was `key=value` written without quotes before the `=`.
pub(crate) fn tokenize(s: &str) -> Result<Vec<(String, Option<String>)>> {
    let mut out = vec![];
    let mut it = s.chars().peekable();
    loop {
        while it.peek().is_some_and(|c| c.is_whitespace()) {
            it.next();
        }
        if it.peek().is_none() {
            break;
        }
        let mut w = String::new();
        let mut key = None;
        let mut quoted_any = false;
        while let Some(&c) = it.peek() {
            if c.is_whitespace() {
                break;
            }
            it.next();
            match c {
                '"' | '\'' => {
                    quoted_any = true;
                    let mut closed = false;
                    for d in it.by_ref() {
                        if d == c {
                            closed = true;
                            break;
                        }
                        w.push(d);
                    }
                    if !closed {
                        return Err(Error::invalid("then", format!("unclosed {c} in `{s}`")));
                    }
                }
                '=' if key.is_none() && !quoted_any && !w.is_empty() && w.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') => {
                    key = Some(std::mem::take(&mut w))
                }
                c => w.push(c),
            }
        }
        out.push(match key {
            Some(k) => (k, Some(w)),
            None => (w, None),
        });
    }
    Ok(out)
}

/// Parse `FIELD OP VALUE`, e.g. `inserted > 0`, `format=pdf`, `title ~ "btc halving"`.
pub fn parse_condition(s: &str) -> Result<Condition> {
    let s = s.trim();
    let end = s.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-')).unwrap_or(s.len());
    let field = &s[..end];
    if field.is_empty() {
        return Err(Error::invalid("--if", format!("`{s}`: start with a field, e.g. `inserted > 0`")));
    }
    let rest = s[end..].trim_start();
    let Some(op) = Op::ALL.into_iter().find(|o| rest.starts_with(o.as_str())) else {
        return Err(Error::invalid("--if", format!("`{s}`: expected an operator (= != ~ !~ > >= < <= ? !?)")));
    };
    let mut value = rest[op.as_str().len()..].trim().to_string();
    if value.len() >= 2 && (value.starts_with('"') && value.ends_with('"') || value.starts_with('\'') && value.ends_with('\'')) {
        value = value[1..value.len() - 1].to_string();
    }
    if op.unary() != value.is_empty() {
        return Err(Error::invalid("--if", format!("`{s}`: {}", if op.unary() { "`?` and `!?` take no value" } else { "missing a value" })));
    }
    Ok(Condition { field: field.to_string(), op, value })
}

pub fn condition_text(c: &Condition) -> String {
    if c.op.unary() { format!("{} {}", c.field, c.op.as_str()) } else { format!("{} {} {}", c.field, c.op.as_str(), quote(&c.value)) }
}

pub(crate) fn quote(s: &str) -> String {
    if !s.is_empty() && !s.contains(|c: char| c.is_whitespace() || "\"';=".contains(c)) {
        s.to_string()
    } else if s.contains('"') {
        format!("'{s}'")
    } else {
        format!("\"{s}\"")
    }
}

/// Parse one then-action, resolving names to ids:
///
/// `status TEXT` · `search [SEARCH]` · `export-report REPORT to=PATH [format=F]` ·
/// `export-journal [JOURNAL] to=PATH [format=F]` ·
/// `add-to-project [PROJECT] [item=subject|search:S|journal:J|report:R]` ·
/// `entry [JOURNAL] TEXT [title=T]` · `note [JOURNAL] TEXT`
pub fn parse_then(store: &Store, s: &str) -> Result<Then> {
    let toks = tokenize(s)?;
    let Some(((verb, None), rest)) = toks.split_first().map(|(a, b)| (a.clone(), b)) else {
        return Err(Error::invalid(
            "--then",
            format!(
                "`{s}`: start with an action (status, search, export-report, export-journal, add-to-project, entry, note, model-create, model-edit, model-apply, model-remove, record-create, record-update, record-delete)"
            ),
        ));
    };
    let mut pos: Vec<String> = vec![];
    let mut kv: Vec<(String, String)> = vec![];
    for (w, v) in rest {
        match v {
            Some(v) => kv.push((w.to_ascii_lowercase(), v.clone())),
            None => pos.push(w.clone()),
        }
    }
    let bad = |m: String| Error::invalid("--then", format!("`{s}`: {m}"));
    let take = |kv: &mut Vec<(String, String)>, k: &str| kv.iter().position(|(a, _)| a == k).map(|i| kv.remove(i).1);
    let fmt = |kv: &mut Vec<(String, String)>| -> Result<Option<String>> {
        take(kv, "format").map(|f| OutFormat::parse(&f).map(|f| f.as_str().to_string())).transpose()
    };
    let journal = |n: &str| store.find_journal(n).map(|j| j.id);
    let flag = |kv: &mut Vec<(String, String)>, k: &str| -> Result<bool> {
        take(kv, k).map(|v| yes(&v).map_err(bad)).transpose().map(|b| b.unwrap_or(false))
    };
    let one_model = |pos: &[String], what: &str| -> Result<Option<String>> {
        match pos {
            [] => Ok(None),
            [n] => Ok(Some(store.find_model(n)?.id)),
            _ => Err(bad(format!("{what} takes one MODEL (quote values with spaces: field=\"a b\")"))),
        }
    };
    let t = match verb.to_ascii_lowercase().as_str() {
        "status" | "message" | "say" => {
            if pos.is_empty() {
                return Err(bad("status needs a text".into()));
            }
            Then::Status { text: pos.join(" ") }
        }
        "search" | "run-search" => match pos.as_slice() {
            [] => Then::RunSearch { search: None },
            [n] => Then::RunSearch { search: Some(store.find_live_search(n)?.id) },
            _ => return Err(bad("search takes one live search".into())),
        },
        "export-report" => {
            let [n] = pos.as_slice() else { return Err(bad("export-report takes one report".into())) };
            let to = take(&mut kv, "to").ok_or_else(|| bad("give to=PATH".into()))?;
            Then::ExportReport { report: store.find_report(n)?.id, format: fmt(&mut kv)?, to }
        }
        "export-journal" => {
            let j = match pos.as_slice() {
                [] => None,
                [n] => Some(journal(n)?),
                _ => return Err(bad("export-journal takes one journal".into())),
            };
            let to = take(&mut kv, "to").ok_or_else(|| bad("give to=PATH".into()))?;
            Then::ExportJournal { journal: j, format: fmt(&mut kv)?, to }
        }
        "add-to-project" | "project" => {
            let project = match pos.as_slice() {
                [] => None,
                [n] => Some(store.find_project(n)?.id),
                _ => return Err(bad("add-to-project takes one project".into())),
            };
            let item = match take(&mut kv, "item") {
                None => ItemRef::Subject,
                Some(i) => match i.split_once(':') {
                    None if i == "subject" => ItemRef::Subject,
                    Some(("search", n)) => ItemRef::LiveSearch(store.find_live_search(n)?.id),
                    Some(("journal", n)) => ItemRef::Journal(journal(n)?),
                    Some(("report", n)) => ItemRef::Report(store.find_report(n)?.id),
                    Some(("model", n)) => ItemRef::Model(store.find_model(n)?.id),
                    _ => return Err(bad(format!("item `{i}`: subject, search:NAME, journal:NAME, report:NAME or model:NAME"))),
                },
            };
            Then::AddToProject { project, item }
        }
        "entry" | "write-entry" => {
            let title = take(&mut kv, "title").unwrap_or_default();
            match pos.as_slice() {
                [t] => Then::WriteEntry { journal: None, title, text: t.clone() },
                [j, t] => Then::WriteEntry { journal: Some(journal(j)?), title, text: t.clone() },
                _ => return Err(bad("entry takes [JOURNAL] \"TEXT\" (quote the text)".into())),
            }
        }
        "note" | "add-note" => match pos.as_slice() {
            [t] => Then::AddNote { journal: None, text: t.clone() },
            [j, t] => Then::AddNote { journal: Some(journal(j)?), text: t.clone() },
            _ => return Err(bad("note takes [JOURNAL] \"TEXT\" (quote the text)".into())),
        },
        "model-create" | "create-model" => {
            let [n] = pos.as_slice() else { return Err(bad("model-create takes one NAME (quote it if it has spaces)".into())) };
            let fields = take(&mut kv, "fields").unwrap_or_default();
            let about = take(&mut kv, "about").unwrap_or_default();
            let draft = flag(&mut kv, "draft")?;
            Then::ModelCreate { name: n.clone(), fields, about, draft }
        }
        "model-edit" | "edit-model" => {
            let model = one_model(&pos, "model-edit")?;
            let (mut ops, mut apply, mut destructive) = (vec![], false, false);
            for (k, v) in std::mem::take(&mut kv) {
                match k.as_str() {
                    "add" => {
                        if let Some(m) = model.as_ref().and_then(|m| store.model(m).ok().flatten()) {
                            crate::designer::parse_field(store, &m, &v, None)?;
                        }
                        ops.push(FieldOp::Add { spec: v })
                    }
                    "change" => ops.push(FieldOp::Change { spec: v }),
                    "drop" => ops.push(FieldOp::Drop { field: v }),
                    "rename" => ops.push(crate::designer::parse_rename(&v)?),
                    "move" => ops.push(crate::designer::parse_move(&v)?),
                    "apply" => apply = yes(&v).map_err(bad)?,
                    "destructive" => destructive = yes(&v).map_err(bad)?,
                    _ => kv.push((k, v)),
                }
            }
            if ops.is_empty() {
                return Err(bad("model-edit needs add=, change=, drop=, rename= or move=".into()));
            }
            Then::ModelEdit { model, ops, apply, destructive }
        }
        "model-apply" | "apply-model" => Then::ModelApply { model: one_model(&pos, "model-apply")?, destructive: flag(&mut kv, "destructive")? },
        "model-remove" | "remove-model" => Then::ModelRemove { model: one_model(&pos, "model-remove")?, destructive: flag(&mut kv, "destructive")? },
        "record-create" | "create-record" | "record" => {
            let model = one_model(&pos, "record-create")?;
            if kv.iter().any(|(k, _)| k == "id" || k == "where") {
                return Err(bad("record-create takes FIELD=VALUE (id= and where= are for record-update and record-delete)".into()));
            }
            Then::RecordCreate { model, values: std::mem::take(&mut kv).into_iter().map(|(k, v)| (k, Value::String(v))).collect() }
        }
        "record-update" | "update-record" | "record-delete" | "delete-record" => {
            let update = verb.to_ascii_lowercase().contains("update");
            let what = if update { "record-update" } else { "record-delete" };
            let model = one_model(&pos, what)?;
            let id = take(&mut kv, "id");
            let filter = take(&mut kv, "where").map(|w| crate::designer::parse_where(&[w])).transpose()?.unwrap_or_default();
            if id.is_none() && filter.is_empty() {
                return Err(bad(format!("{what} needs id=RECORD or where=\"FIELD OP VALUE\"")));
            }
            if update {
                if kv.is_empty() {
                    return Err(bad("record-update needs FIELD=VALUE to set".into()));
                }
                Then::RecordUpdate { model, id, filter, values: std::mem::take(&mut kv).into_iter().map(|(k, v)| (k, Value::String(v))).collect() }
            } else {
                Then::RecordDelete { model, id, filter }
            }
        }
        v => {
            return Err(bad(format!(
                "unknown action `{v}` (status, search, export-report, export-journal, add-to-project, entry, note, model-create, model-edit, model-apply, model-remove, record-create, record-update, record-delete)"
            )));
        }
    };
    if let Some((k, _)) = kv.first() {
        return Err(bad(format!("`{k}=` does not belong to {}", t.verb())));
    }
    Ok(t)
}

fn yes(v: &str) -> std::result::Result<bool, String> {
    match v.trim().to_ascii_lowercase().as_str() {
        "yes" | "y" | "true" | "on" | "1" => Ok(true),
        "no" | "n" | "false" | "off" | "0" => Ok(false),
        o => Err(format!("`{o}`: use yes or no")),
    }
}

fn name_of(store: &Store, kind: ItemKind, id: &str) -> String {
    let n = match kind {
        ItemKind::LiveSearch => store.live_search(id).ok().flatten().map(|s| s.name),
        ItemKind::Journal => store.journal(id).ok().flatten().map(|j| j.name),
        ItemKind::Report => store.report(id).ok().flatten().map(|r| r.name),
        ItemKind::RuleSet => store.rule(id).ok().flatten().map(|r| r.name),
        ItemKind::Model => store.model(id).ok().flatten().map(|m| m.name),
        ItemKind::Result => None,
    };
    n.unwrap_or_else(|| crate::store::short(id).to_string())
}

/// A then-action in the syntax [`parse_then`] reads, with names instead of ids.
pub fn then_text(store: &Store, t: &Then) -> String {
    let mut w = vec![t.verb().to_string()];
    let mut opt = |kind, id: &Option<String>| {
        if let Some(i) = id {
            w.push(quote(&name_of(store, kind, i)));
        }
    };
    match t {
        Then::Status { text } => return format!("status {}", quote(text)),
        Then::RunSearch { search } => opt(ItemKind::LiveSearch, search),
        Then::ExportReport { report, to, format } => {
            w.push(quote(&name_of(store, ItemKind::Report, report)));
            w.push(format!("to={}", quote(to)));
            w.extend(format.iter().map(|f| format!("format={f}")));
        }
        Then::ExportJournal { journal, to, format } => {
            opt(ItemKind::Journal, journal);
            w.push(format!("to={}", quote(to)));
            w.extend(format.iter().map(|f| format!("format={f}")));
        }
        Then::AddToProject { project, item } => {
            if let Some(p) = project {
                w.push(quote(&store.project(p).ok().flatten().map(|p| p.name).unwrap_or_else(|| crate::store::short(p).into())));
            }
            let it = match item {
                ItemRef::Subject => None,
                ItemRef::LiveSearch(i) => Some(format!("search:{}", name_of(store, ItemKind::LiveSearch, i))),
                ItemRef::Journal(i) => Some(format!("journal:{}", name_of(store, ItemKind::Journal, i))),
                ItemRef::Report(i) => Some(format!("report:{}", name_of(store, ItemKind::Report, i))),
                ItemRef::Model(i) => Some(format!("model:{}", name_of(store, ItemKind::Model, i))),
            };
            w.extend(it.map(|i| format!("item={}", quote(&i))));
        }
        Then::WriteEntry { journal, title, text } => {
            opt(ItemKind::Journal, journal);
            w.push(format!("\"{}\"", text.replace('"', "'")));
            if !title.is_empty() {
                w.push(format!("title={}", quote(title)));
            }
        }
        Then::AddNote { journal, text } => {
            opt(ItemKind::Journal, journal);
            w.push(format!("\"{}\"", text.replace('"', "'")));
        }
        Then::ModelCreate { name, fields, about, draft } => {
            w.push(quote(name));
            if !fields.is_empty() {
                w.push(format!("fields={}", quote(fields)));
            }
            if !about.is_empty() {
                w.push(format!("about={}", quote(about)));
            }
            if *draft {
                w.push("draft=yes".into());
            }
        }
        Then::ModelEdit { model, ops, apply, destructive } => {
            opt(ItemKind::Model, model);
            w.extend(ops.iter().map(crate::designer::op_arg));
            if *apply {
                w.push("apply=yes".into());
            }
            if *destructive {
                w.push("destructive=yes".into());
            }
        }
        Then::ModelApply { model, destructive } | Then::ModelRemove { model, destructive } => {
            opt(ItemKind::Model, model);
            if *destructive {
                w.push("destructive=yes".into());
            }
        }
        Then::RecordCreate { model, values } => {
            opt(ItemKind::Model, model);
            w.extend(values.iter().map(|(k, v)| format!("{k}={}", quote(&crate::designer::value_text(v)))));
        }
        Then::RecordUpdate { model, id, filter, values } => {
            opt(ItemKind::Model, model);
            w.extend(id.iter().map(|i| format!("id={}", quote(i))));
            if !filter.is_empty() {
                w.push(format!("where={}", quote(&filter.iter().map(condition_text).collect::<Vec<_>>().join("; "))));
            }
            w.extend(values.iter().map(|(k, v)| format!("{k}={}", quote(&crate::designer::value_text(v)))));
        }
        Then::RecordDelete { model, id, filter } => {
            opt(ItemKind::Model, model);
            w.extend(id.iter().map(|i| format!("id={}", quote(i))));
            if !filter.is_empty() {
                w.push(format!("where={}", quote(&filter.iter().map(condition_text).collect::<Vec<_>>().join("; "))));
            }
        }
    }
    w.join(" ")
}

/// Where a rule lives, in words: `journal Notes`, `search btc`, `standalone`; plus projects.
pub fn home(store: &Store, r: &Rule) -> String {
    let mut s = match (r.owner_kind, &r.owner_id) {
        (OwnerKind::Journal, Some(i)) => format!("journal {}", name_of(store, ItemKind::Journal, i)),
        (OwnerKind::LiveSearch, Some(i)) => format!("search {}", name_of(store, ItemKind::LiveSearch, i)),
        (OwnerKind::Model, Some(i)) => format!("model {}", name_of(store, ItemKind::Model, i)),
        _ => String::new(),
    };
    let ps: Vec<String> = store.projects_with(ItemKind::RuleSet, &r.id).unwrap_or_default().into_iter().map(|p| p.name).collect();
    if !ps.is_empty() {
        if !s.is_empty() {
            s.push_str(", ");
        }
        s.push_str(&format!("project {}", ps.join(", ")));
    }
    if s.is_empty() { "standalone".into() } else { s }
}

/// One line about a rule: `on import → 2 actions, on`.
pub fn brief(r: &Rule) -> String {
    let n = r.spec.then.len();
    format!("on {} → {n} action{}{}", r.spec.on.join(", "), if n == 1 { "" } else { "s" }, if r.enabled { "" } else { " (off)" })
}

// ---------------------------------------------------------------------------- matching

fn path<'a>(mut v: &'a Value, p: &str) -> Option<&'a Value> {
    for k in p.split('.').filter(|k| !k.is_empty()) {
        v = match v {
            Value::Object(m) => m.get(k)?,
            Value::Array(a) => a.get(k.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    (!v.is_null()).then_some(v)
}

/// The value of a field on an action: `kind`, `summary`, `origin`, `author`, `action`,
/// `before.X`/`after.X`, otherwise a params path (`inserted`, `spec.query`, `params.x`).
pub fn field_value(store: &Store, a: &ActionRecord, field: &str) -> Option<Value> {
    match field {
        "kind" => Some(a.kind.clone().into()),
        "summary" => Some(a.summary.clone().into()),
        "origin" => Some(a.origin.clone().into()),
        "author" => Some(crate::history::author_label(store, a.author_id.as_deref()).into()),
        "author_id" => a.author_id.clone().map(Into::into),
        "action" | "id" => Some(a.id.into()),
        f => {
            if let Some(p) = f.strip_prefix("before.") {
                return a.before.as_ref().and_then(|v| path(v, p)).cloned();
            }
            if let Some(p) = f.strip_prefix("after.") {
                return a.after.as_ref().and_then(|v| path(v, p)).cloned();
            }
            path(&a.params, f.strip_prefix("params.").unwrap_or(f)).cloned()
        }
    }
}

fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        o => o.to_string(),
    }
}

fn num(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        Value::Bool(b) => Some(*b as u8 as f64),
        _ => None,
    }
}

fn glob(p: &str, s: &str) -> bool {
    let parts: Vec<&str> = p.split('*').collect();
    if parts.len() == 1 {
        return p == s;
    }
    let mut rest = s;
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            let Some(r) = rest.strip_prefix(part) else { return false };
            rest = r;
        } else if i == parts.len() - 1 {
            return rest.ends_with(part);
        } else {
            let Some(at) = rest.find(part) else { return false };
            rest = &rest[at + part.len()..];
        }
    }
    true
}

fn scalar_eq(v: &Value, want: &str) -> bool {
    match (num(v), want.trim().parse::<f64>()) {
        (Some(a), Ok(b)) if !matches!(v, Value::Bool(_)) => a == b,
        _ => glob(&want.to_lowercase(), &text(v).to_lowercase()),
    }
}

/// Does `actual` (absent: `None`) satisfy `op value`?
pub fn compare(actual: Option<&Value>, op: Op, want: &str) -> bool {
    let Some(v) = actual else { return matches!(op, Op::Missing | Op::Ne | Op::NotContains) };
    let contains = |v: &Value| text(v).to_lowercase().contains(&want.to_lowercase());
    let order = |v: &Value| -> Option<std::cmp::Ordering> {
        let v = match v {
            Value::Array(a) => Value::from(a.len()),
            o => o.clone(),
        };
        match (num(&v), want.trim().parse::<f64>()) {
            (Some(a), Ok(b)) => a.partial_cmp(&b),
            _ => Some(text(&v).to_lowercase().cmp(&want.to_lowercase())),
        }
    };
    let arr = v.as_array();
    match op {
        Op::Exists => true,
        Op::Missing => false,
        Op::Eq => arr.map_or_else(|| scalar_eq(v, want), |a| a.iter().any(|x| scalar_eq(x, want))),
        Op::Ne => arr.map_or_else(|| !scalar_eq(v, want), |a| !a.iter().any(|x| scalar_eq(x, want))),
        Op::Contains => arr.map_or_else(|| contains(v), |a| a.iter().any(contains)),
        Op::NotContains => arr.map_or_else(|| !contains(v), |a| !a.iter().any(contains)),
        Op::Gt => order(v).is_some_and(|o| o.is_gt()),
        Op::Ge => order(v).is_some_and(|o| o.is_ge()),
        Op::Lt => order(v).is_some_and(|o| o.is_lt()),
        Op::Le => order(v).is_some_and(|o| o.is_le()),
    }
}

/// Ids the action is about (journal, search, report, project, item ref, …).
fn mentions(a: &ActionRecord) -> Vec<String> {
    ["journal", "search", "report", "project", "ref", "entry", "model", "record"]
        .iter()
        .filter_map(|k| a.params.get(*k).and_then(Value::as_str).map(str::to_string))
        .collect()
}

/// Is the action about where the rule lives (its journal or search, its projects or their items)?
pub fn is_related(store: &Store, r: &Rule, a: &ActionRecord) -> bool {
    let m = mentions(a);
    if let Some(o) = &r.owner_id
        && m.contains(o)
    {
        return true;
    }
    let projects = store.projects_with(ItemKind::RuleSet, &r.id).unwrap_or_default();
    if r.owner_id.is_none() && projects.is_empty() {
        return true;
    }
    projects.iter().any(|p| {
        m.contains(&p.id)
            || store.project_items(&p.id).unwrap_or_default().iter().any(|i| m.contains(&i.ref_id))
            || store.project_journals(&p.id).unwrap_or_default().iter().any(|j| m.contains(&j.id))
    })
}

/// How a rule fares against one action (`mnnn rule test`).
#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub rule: String,
    pub name: String,
    pub action: i64,
    pub kind: String,
    pub enabled: bool,
    /// The `--on` pattern that matched.
    pub on: Option<String>,
    /// `None` when the rule is not `--related`.
    pub related: Option<bool>,
    pub conditions: Vec<CondCheck>,
    pub matched: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CondCheck {
    pub condition: String,
    pub actual: Option<Value>,
    pub ok: bool,
}

pub fn check(store: &Store, r: &Rule, a: &ActionRecord) -> Check {
    let on = if a.kind.starts_with("rule.") { None } else { r.spec.on.iter().find(|p| pattern_matches(p, &a.kind)).cloned() };
    let related = r.spec.related.then(|| is_related(store, r, a));
    let conditions: Vec<CondCheck> = r
        .spec
        .when
        .iter()
        .map(|c| {
            let actual = field_value(store, a, &c.field);
            CondCheck { condition: condition_text(c), ok: compare(actual.as_ref(), c.op, &c.value), actual }
        })
        .collect();
    let matched = on.is_some() && related != Some(false) && conditions.iter().all(|c| c.ok);
    Check { rule: r.id.clone(), name: r.name.clone(), action: a.id, kind: a.kind.clone(), enabled: r.enabled, on, related, conditions, matched }
}

// ---------------------------------------------------------------------------- templates

/// Fill `{kind} {summary} {origin} {author} {action} {rule} {date} {time} {now}` and
/// `{FIELD}` (any field, e.g. `{inserted}`, `{params.out}`); unknown ones stay as written.
pub fn expand(store: &Store, r: &Rule, a: &ActionRecord, t: &str) -> String {
    let now = chrono::Local::now();
    let mut out = String::new();
    let mut rest = t;
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        let Some(j) = after.find('}') else {
            out.push_str(&rest[i..]);
            return out;
        };
        let key = &after[..j];
        let v = match key {
            "rule" => Some(r.name.clone()),
            "date" => Some(now.format("%Y-%m-%d").to_string()),
            "time" => Some(now.format("%H%M%S").to_string()),
            "now" => Some(now.format("%Y-%m-%d %H:%M").to_string()),
            k if !k.is_empty() && !k.contains(char::is_whitespace) => field_value(store, a, k).map(|v| text(&v)),
            _ => None,
        };
        match v {
            Some(v) => out.push_str(&v),
            None => out.push_str(&rest[i..i + j + 2]),
        }
        rest = &after[j + 1..];
    }
    out.push_str(rest);
    out
}

// ---------------------------------------------------------------------------- running

fn own(r: &Rule, kind: OwnerKind) -> Option<String> {
    (r.owner_kind == kind).then(|| r.owner_id.clone()).flatten()
}

/// An export path: templates filled, a leading `~/` is the home folder.
fn out_path(store: &Store, r: &Rule, a: &ActionRecord, to: &str) -> PathBuf {
    let p = expand(store, r, a, to);
    match (p.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(h)) => PathBuf::from(h).join(rest),
        _ => PathBuf::from(p),
    }
}

fn out_format(format: &Option<String>, to: &str) -> Result<OutFormat> {
    if let Some(f) = format {
        return OutFormat::parse(f);
    }
    let ext = std::path::Path::new(to).extension().and_then(|e| e.to_str()).unwrap_or("");
    OutFormat::ALL
        .into_iter()
        .find(|f| f.ext().eq_ignore_ascii_case(ext))
        .or_else(|| OutFormat::parse(ext).ok())
        .ok_or_else(|| Error::invalid("export", format!("cannot tell the format of `{to}`; give format=…")))
}

fn subject(a: &ActionRecord) -> Result<(ItemKind, String)> {
    let p = |k: &str| a.params.get(k).and_then(Value::as_str).map(str::to_string);
    let k = a.kind.as_str();
    let found = if k.starts_with("search.") {
        p("search").map(|i| (ItemKind::LiveSearch, i))
    } else if k.starts_with("report.") {
        p("report").map(|i| (ItemKind::Report, i))
    } else if k.starts_with("model.") || k.starts_with("record.") {
        p("model").map(|i| (ItemKind::Model, i))
    } else if ["journal.", "entry.", "note.", "highlight.", "image."].iter().any(|x| k.starts_with(x)) {
        p("journal").map(|i| (ItemKind::Journal, i))
    } else {
        None
    };
    found.ok_or_else(|| Error::invalid("add-to-project", format!("a `{k}` action has no live search, journal, report or model to add")))
}

/// What a then-action would do, in words (`mnnn rule dry-run`). Nothing is changed.
pub fn plan(store: &Store, r: &Rule, a: &ActionRecord, t: &Then) -> Result<String> {
    let journal = |j: &Option<String>| -> Result<String> {
        let id =
            j.clone().or_else(|| own(r, OwnerKind::Journal)).ok_or_else(|| Error::invalid(t.verb(), "name a journal, or put the rule in one"))?;
        Ok(store.journal(&id)?.ok_or_else(|| Error::invalid("journal", format!("`{id}` no longer exists")))?.name)
    };
    Ok(match t {
        Then::Status { text } => format!("show \"{}\"", expand(store, r, a, text)),
        Then::RunSearch { search } => {
            let id = search
                .clone()
                .or_else(|| own(r, OwnerKind::LiveSearch))
                .ok_or_else(|| Error::invalid("search", "name a live search, or put the rule in one"))?;
            let s = store.live_search(&id)?.ok_or_else(|| Error::invalid("live search", format!("`{id}` no longer exists")))?;
            format!("run live search {}", s.name)
        }
        Then::ExportReport { report, to, format } => {
            let rep = store.report(report)?.ok_or_else(|| Error::invalid("report", format!("`{report}` no longer exists")))?;
            let to = out_path(store, r, a, to).display().to_string();
            format!("export report {} to {} ({})", rep.name, to, out_format(format, &to)?.as_str())
        }
        Then::ExportJournal { journal: j, to, format } => {
            let to = out_path(store, r, a, to).display().to_string();
            format!("export journal {} to {} ({})", journal(j)?, to, out_format(format, &to)?.as_str())
        }
        Then::AddToProject { project, item } => {
            let p = target_project(store, r, project)?;
            let (k, id) = item_of(a, item)?;
            format!("add {} {} to project {}", k.label(), name_of(store, k, &id), p.name)
        }
        Then::WriteEntry { journal: j, title, text } => {
            let t = expand(store, r, a, title);
            format!("write entry {}in {}: {}", if t.is_empty() { String::new() } else { format!("\"{t}\" ") }, journal(j)?, expand(store, r, a, text))
        }
        Then::AddNote { journal: j, text } => format!("add a note to {}: {}", journal(j)?, expand(store, r, a, text)),
        Then::ModelCreate { name, fields, draft, .. } => {
            let name = expand(store, r, a, name);
            if store.models()?.iter().any(|m| m.name == crate::store::models::slug(&name)) {
                format!("model {name} exists; nothing to do")
            } else {
                let n = split_list(fields).len();
                format!("create model {name} with {n} field{}{}", if n == 1 { "" } else { "s" }, if *draft { " as a draft" } else { "" })
            }
        }
        Then::ModelEdit { model: m, ops, apply, .. } => {
            let m = model_of(store, r, t, m)?;
            let d = crate::designer::apply_ops(store, &m, m.pending(), ops)?;
            let what = ops.iter().map(crate::designer::op_text).collect::<Vec<_>>().join(", ");
            if *apply {
                let p = store.plan_model(&m, &d)?;
                format!(
                    "edit model {}: {what}; then apply: {}",
                    m.name,
                    crate::designer::plan_lines(store, &p)[1..].iter().map(|l| l.trim()).collect::<Vec<_>>().join("; ")
                )
            } else {
                format!("edit the draft of model {}: {what}", m.name)
            }
        }
        Then::ModelApply { model: m, .. } => {
            let m = model_of(store, r, t, m)?;
            let p = store.plan_model(&m, m.pending())?;
            crate::designer::plan_lines(store, &p).iter().map(|l| l.trim()).collect::<Vec<_>>().join("; ")
        }
        Then::ModelRemove { model: m, .. } => {
            let m = model_of(store, r, t, m)?;
            format!("remove model {} and its {} records", m.name, store.count_records(&m)?)
        }
        Then::RecordCreate { model: m, values } => {
            let m = model_of(store, r, t, m)?;
            format!("create a {} record: {}", m.name, values_text(&filled(store, r, a, values)))
        }
        Then::RecordUpdate { model: m, id, filter, values } => {
            let m = model_of(store, r, t, m)?;
            format!("update {}: {}", which(store, r, a, &m, id, filter)?, values_text(&filled(store, r, a, values)))
        }
        Then::RecordDelete { model: m, id, filter } => {
            let m = model_of(store, r, t, m)?;
            format!("delete {}", which(store, r, a, &m, id, filter)?)
        }
    })
}

fn target_project(store: &Store, r: &Rule, project: &Option<String>) -> Result<crate::store::Project> {
    if let Some(p) = project {
        return store.project(p)?.ok_or_else(|| Error::invalid("project", format!("`{p}` no longer exists")));
    }
    let mut ps = store.projects_with(ItemKind::RuleSet, &r.id)?;
    match ps.len() {
        1 => Ok(ps.remove(0)),
        0 => Err(Error::invalid("add-to-project", "name a project, or add the rule to one")),
        n => Err(Error::invalid("add-to-project", format!("the rule is in {n} projects; name one"))),
    }
}

fn item_of(a: &ActionRecord, item: &ItemRef) -> Result<(ItemKind, String)> {
    Ok(match item {
        ItemRef::Subject => subject(a)?,
        ItemRef::LiveSearch(i) => (ItemKind::LiveSearch, i.clone()),
        ItemRef::Journal(i) => (ItemKind::Journal, i.clone()),
        ItemRef::Report(i) => (ItemKind::Report, i.clone()),
        ItemRef::Model(i) => (ItemKind::Model, i.clone()),
    })
}

fn record(store: &Store, kind: &str, summary: String, params: Value) {
    store.note_action(&NewAction::new(kind, ORIGIN, summary).params(params));
}

fn note(store: &Store, a: crate::designer::Act) {
    record(store, a.kind, a.summary, a.params);
}

/// The model a then-action names, else the one the rule lives in.
fn model_of(store: &Store, r: &Rule, t: &Then, m: &Option<String>) -> Result<Model> {
    let id = m.clone().or_else(|| own(r, OwnerKind::Model)).ok_or_else(|| Error::invalid(t.verb(), "name a model, or put the rule in one"))?;
    store.model(&id)?.ok_or_else(|| Error::invalid("model", format!("`{id}` no longer exists")))
}

/// Values with their templates filled.
fn filled(store: &Store, r: &Rule, a: &ActionRecord, v: &serde_json::Map<String, Value>) -> serde_json::Map<String, Value> {
    v.iter()
        .map(|(k, x)| {
            let x = match x {
                Value::String(s) => Value::String(expand(store, r, a, s)),
                o => o.clone(),
            };
            (k.clone(), x)
        })
        .collect()
}

fn values_text(v: &serde_json::Map<String, Value>) -> String {
    if v.is_empty() {
        return "(defaults)".into();
    }
    v.iter().map(|(k, x)| format!("{k}={}", quote(&crate::designer::value_text(x)))).collect::<Vec<_>>().join(" ")
}

fn conds(store: &Store, r: &Rule, a: &ActionRecord, f: &[Condition]) -> Vec<Condition> {
    f.iter().map(|c| Condition { value: expand(store, r, a, &c.value), ..c.clone() }).collect()
}

/// `trades record 1a2b3c4d` or `3 trades records`.
fn which(store: &Store, r: &Rule, a: &ActionRecord, m: &Model, id: &Option<String>, f: &[Condition]) -> Result<String> {
    if let Some(i) = id {
        let rec = store.find_record(m, &expand(store, r, a, i))?;
        return Ok(format!("{} record {}", m.name, rec.short_id()));
    }
    let n = store.records(m, &crate::store::RecordQuery { filter: conds(store, r, a, f), ..Default::default() })?.len();
    Ok(format!("{n} {} record{}", m.name, if n == 1 { "" } else { "s" }))
}

/// A refused migration says how to allow it in a rule.
fn in_rule(e: Error) -> Error {
    match e {
        Error::NeedsConfirmation(m) => Error::invalid("model", format!("{m}; add destructive=yes to allow it")),
        o => o,
    }
}

/// Do one then-action; the message says what happened.
pub fn exec(store: &Store, r: &Rule, a: &ActionRecord, t: &Then) -> Result<String> {
    let journal = |j: &Option<String>| -> Result<crate::store::Journal> {
        let id =
            j.clone().or_else(|| own(r, OwnerKind::Journal)).ok_or_else(|| Error::invalid(t.verb(), "name a journal, or put the rule in one"))?;
        store.journal(&id)?.ok_or_else(|| Error::invalid("journal", format!("`{id}` no longer exists")))
    };
    match t {
        Then::Status { text } => Ok(expand(store, r, a, text)),
        Then::RunSearch { search } => {
            let id = search
                .clone()
                .or_else(|| own(r, OwnerKind::LiveSearch))
                .ok_or_else(|| Error::invalid("search", "name a live search, or put the rule in one"))?;
            let s = store.live_search(&id)?.ok_or_else(|| Error::invalid("live search", format!("`{id}` no longer exists")))?;
            let n = crate::live::run(store, &s.spec)?.len();
            Ok(format!("live search {}: {}", s.name, crate::live::matches(n)))
        }
        Then::ExportReport { report, to, format } => {
            let rep = store.report(report)?.ok_or_else(|| Error::invalid("report", format!("`{report}` no longer exists")))?;
            let to = out_path(store, r, a, to);
            let f = out_format(format, &to.to_string_lossy())?;
            let files = crate::export::files::write_recorded(store, &Target::Report(rep.id), f, &to, ORIGIN)?;
            Ok(format!("exported report {} to {} ({} file{})", rep.name, to.display(), files.len(), if files.len() == 1 { "" } else { "s" }))
        }
        Then::ExportJournal { journal: j, to, format } => {
            let jr = journal(j)?;
            let to = out_path(store, r, a, to);
            let f = out_format(format, &to.to_string_lossy())?;
            let files = crate::export::files::write_recorded(store, &Target::Journal { id: jr.id, entries: vec![] }, f, &to, ORIGIN)?;
            Ok(format!("exported journal {} to {} ({} file{})", jr.name, to.display(), files.len(), if files.len() == 1 { "" } else { "s" }))
        }
        Then::AddToProject { project, item } => {
            let p = target_project(store, r, project)?;
            let (k, id) = item_of(a, item)?;
            let (it, new) = store.add_project_item(&p.id, k, &id, json!({}))?;
            let (name, _) = crate::live::describe_item(store, &it);
            if new {
                record(
                    store,
                    "project.add",
                    format!("add {} {name} to project {}", k.label(), p.name),
                    json!({ "project": p.id, "item": it.id, "kind": it.kind, "ref": it.ref_id }),
                );
                Ok(format!("added {} {name} to project {}", k.label(), p.name))
            } else {
                Ok(format!("{name} is already in project {}", p.name))
            }
        }
        Then::WriteEntry { journal: j, title, text } => {
            let jr = journal(j)?;
            let e = store.add_entry(&jr.id, &expand(store, r, a, title), &expand(store, r, a, text), None)?;
            record(
                store,
                "entry.add",
                format!("write entry {} in {}", e.heading(), jr.name),
                json!({ "journal": e.journal_id, "entry": e.id, "title": e.heading() }),
            );
            Ok(format!("wrote entry {} in {}", e.heading(), jr.name))
        }
        Then::AddNote { journal: j, text } => {
            let jr = journal(j)?;
            let n = store.add_note(&jr.id, None, None, &expand(store, r, a, text))?;
            record(store, "note.add", format!("add note to {}", jr.name), json!({ "journal": jr.id, "entry": null, "note": n.id }));
            Ok(format!("added a note to {}", jr.name))
        }
        Then::ModelCreate { name, fields, about, draft } => {
            let name = expand(store, r, a, name);
            if let Some(m) = store.models()?.into_iter().find(|m| m.name == crate::store::models::slug(&name)) {
                return Ok(format!("model {} exists", m.name));
            }
            let (m, _) = store.create_model(&name, &expand(store, r, a, about), crate::store::Design::default(), false)?;
            let design = match crate::designer::parse_fields(store, &m, fields)
                .and_then(|f| store.set_model_draft(&m.id, Some(crate::store::Design { fields: f })))
            {
                Ok(m) => m,
                Err(e) => {
                    let _ = store.remove_model(&m.id, true);
                    return Err(e);
                }
            };
            note(store, crate::designer::act_model_create(&design));
            if *draft {
                return Ok(format!("created model {} (draft)", design.name));
            }
            let (m, p) = store.apply_model(&m.id, &ApplyOpts { note: format!("rule {}", r.name), ..Default::default() })?;
            note(store, crate::designer::act_model_apply(store, &m, &p, false));
            Ok(format!("created model {} v{}", m.name, m.version))
        }
        Then::ModelEdit { model: m, ops, apply, destructive } => {
            let m = model_of(store, r, t, m)?;
            let before = m.draft.clone();
            let m = store.edit_model(&m.id, ops)?;
            if !*apply || m.draft.is_none() {
                note(store, crate::designer::act_model_edit(&m, ops, None));
                return Ok(format!("edited the draft of model {}", m.name));
            }
            // edit and apply go together: a refused migration leaves the draft as it was
            let (n, p) = match store.apply_model(&m.id, &ApplyOpts { destructive: *destructive, note: format!("rule {}", r.name) }) {
                Ok(done) => done,
                Err(e) => {
                    let _ = store.set_model_draft(&m.id, before);
                    return Err(in_rule(e));
                }
            };
            note(store, crate::designer::act_model_edit(&m, ops, None));
            let m = n;
            note(store, crate::designer::act_model_apply(store, &m, &p, false));
            Ok(format!("model {} is now v{} ({} steps)", m.name, m.version, p.steps.len()))
        }
        Then::ModelApply { model: m, destructive } => {
            let m = model_of(store, r, t, m)?;
            let (m, p) = store.apply_model(&m.id, &ApplyOpts { destructive: *destructive, note: format!("rule {}", r.name) }).map_err(in_rule)?;
            note(store, crate::designer::act_model_apply(store, &m, &p, false));
            Ok(format!("model {} is now v{} ({} steps)", m.name, m.version, p.steps.len()))
        }
        Then::ModelRemove { model: m, destructive } => {
            let m = model_of(store, r, t, m)?;
            let (m, n) = store.remove_model(&m.id, *destructive).map_err(in_rule)?;
            note(store, crate::designer::act_model_remove(&m, n));
            Ok(format!("removed model {} ({n} records)", m.name))
        }
        Then::RecordCreate { model: m, values } => {
            let m = model_of(store, r, t, m)?;
            let rec = store.create_record(&m, &filled(store, r, a, values))?;
            note(store, crate::designer::act_record_create(&m, &rec));
            Ok(format!("created {} record {}", m.name, rec.short_id()))
        }
        Then::RecordUpdate { model: m, id, filter, values } => {
            let m = model_of(store, r, t, m)?;
            let v = filled(store, r, a, values);
            if let Some(i) = id {
                let rec = store.update_record(&m, &expand(store, r, a, i), &v)?;
                note(store, crate::designer::act_record_update(&m, Some(&rec), 1, &[], &v));
                return Ok(format!("updated {} record {}", m.name, rec.short_id()));
            }
            let f = conds(store, r, a, filter);
            let n = store.update_records(&m, &f, &v)?;
            note(store, crate::designer::act_record_update(&m, None, n, &f, &v));
            Ok(format!("updated {n} {} record{}", m.name, if n == 1 { "" } else { "s" }))
        }
        Then::RecordDelete { model: m, id, filter } => {
            let m = model_of(store, r, t, m)?;
            if let Some(i) = id {
                let rec = store.delete_record(&m, &expand(store, r, a, i))?;
                note(store, crate::designer::act_record_delete(&m, Some(&rec), 1, &[]));
                return Ok(format!("deleted {} record {}", m.name, rec.short_id()));
            }
            let f = conds(store, r, a, filter);
            let n = store.delete_records(&m, &f)?;
            note(store, crate::designer::act_record_delete(&m, None, n, &f));
            Ok(format!("deleted {n} {} record{}", m.name, if n == 1 { "" } else { "s" }))
        }
    }
}

/// The action `rule test`/`dry-run` checks by default: the newest action on the
/// current branch that a person did, skipping rule runs and what they did.
pub fn last_trigger(store: &Store) -> Result<Option<ActionRecord>> {
    let mut at = store.history_head()?;
    while let Some(id) = at {
        let Some(a) = store.action(id)? else { break };
        if !a.kind.starts_with("rule.") && a.params.get("rule_chain").is_none() {
            return Ok(Some(a));
        }
        at = a.parent;
    }
    Ok(None)
}

/// Called by [`Store::record_action`] after every action.
pub(crate) fn after_action(store: &Store, id: i64) {
    let rt = &store.rules_rt;
    if rt.off.get() {
        return;
    }
    let Ok(Some(a)) = store.action(id) else { return };
    if a.kind.starts_with("rule.") {
        return;
    }
    let chain = rt.chain.borrow().clone();
    if chain.is_empty() {
        rt.budget.set(MAX_RUNS);
    }
    let rules = match store.enabled_rules() {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("could not read rules: {e}");
            return;
        }
    };
    for r in rules {
        if !check(store, &r, &a).matched {
            continue;
        }
        let blocked = if chain.contains(&r.id) {
            Some(format!("loop: {} is already running for this action", r.name))
        } else if chain.len() >= MAX_DEPTH {
            Some(format!("too deep: {MAX_DEPTH} rules already ran in a chain"))
        } else if rt.budget.get() == 0 {
            Some(format!("limit: {MAX_RUNS} rule runs for one action"))
        } else {
            None
        };
        match blocked {
            Some(why) => block(store, &r, &a, &why),
            None => {
                rt.budget.set(rt.budget.get() - 1);
                run(store, &r, &a);
            }
        }
    }
}

fn block(store: &Store, r: &Rule, a: &ActionRecord, why: &str) {
    tracing::warn!("rule {} not run on #{} {}: {why}", r.name, a.id, a.kind);
    record(
        store,
        "rule.run",
        format!("rule {} blocked on #{} {}: {why}", r.name, a.id, a.kind),
        json!({ "rule": r.id, "name": r.name, "trigger": a.id, "trigger_kind": a.kind, "status": "blocked", "messages": [why] }),
    );
    let _ = store.note_rule_run(&r.id, "blocked");
    if store.rules_rt.echo.get() {
        eprintln!("rule {}: blocked ({why})", r.name);
    }
}

/// The outcome of one run.
#[derive(Debug, Clone, Serialize)]
pub struct RunResult {
    pub action: Option<i64>,
    pub status: String,
    pub messages: Vec<String>,
}

/// Run a rule's then-actions for an action (it is recorded as `rule.run`, a parent of
/// whatever the then-actions record).
pub fn run(store: &Store, r: &Rule, a: &ActionRecord) -> RunResult {
    let rt = &store.rules_rt;
    let base = json!({ "rule": r.id, "name": r.name, "trigger": a.id, "trigger_kind": a.kind });
    let mut params = base.clone();
    params["status"] = "running".into();
    let run_id = store.record_action(&NewAction::new("rule.run", ORIGIN, format!("rule {} on #{} {}", r.name, a.id, a.kind)).params(params)).ok();
    rt.chain.borrow_mut().push(r.id.clone());
    let mut results = vec![];
    let mut messages = vec![];
    let mut failed = false;
    for t in &r.spec.then {
        match exec(store, r, a, t) {
            Ok(m) => {
                results.push(json!({ "do": t.verb(), "ok": true, "message": m }));
                messages.push(m);
            }
            Err(e) => {
                failed = true;
                let m = format!("{} failed: {e}", t.verb());
                results.push(json!({ "do": t.verb(), "ok": false, "error": e.to_string() }));
                messages.push(m);
            }
        }
    }
    rt.chain.borrow_mut().pop();
    let status = if failed { "failed" } else { "ok" };
    if let Some(id) = run_id {
        let mut p = base;
        p["status"] = status.into();
        p["results"] = results.into();
        p["messages"] = messages.clone().into();
        let summary = format!("rule {} on #{} {}: {}", r.name, a.id, a.kind, messages.join("; "));
        if let Err(e) = store.update_action(id, &summary, &p) {
            tracing::warn!("could not record the run of rule {}: {e}", r.name);
        }
    }
    let _ = store.note_rule_run(&r.id, status);
    if rt.echo.get() {
        for m in &messages {
            eprintln!("rule {}: {m}", r.name);
        }
    }
    RunResult { action: run_id, status: status.into(), messages }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{EntryFormat, NewRule, RuleSpec, SearchSpec};

    fn act(s: &Store, kind: &str, params: Value) -> ActionRecord {
        let id = s.record_action(&NewAction::new(kind, "cli", format!("did {kind}")).params(params)).unwrap();
        s.action(id).unwrap().unwrap()
    }

    fn rule(s: &Store, on: &str, when: &[&str], then: &[&str]) -> Rule {
        let spec = RuleSpec {
            on: parse_on(&[on.to_string()]).unwrap(),
            when: when.iter().map(|w| parse_condition(w).unwrap()).collect(),
            then: then.iter().map(|t| parse_then(s, t).unwrap()).collect(),
            ..Default::default()
        };
        s.create_rule(&NewRule { name: format!("r{}", s.rules().unwrap().len()), spec, enabled: true, ..Default::default() }).unwrap()
    }

    #[test]
    fn patterns_and_conditions() {
        assert!(pattern_matches("journal.*", "journal.export") && pattern_matches("*", "import") && !pattern_matches("entry.*", "entryx"));
        assert!(check_pattern("nope").is_err() && check_pattern("report.section.*").is_ok());
        let c = parse_condition("inserted>=10").unwrap();
        assert_eq!((c.field.as_str(), c.op, c.value.as_str()), ("inserted", Op::Ge, "10"));
        assert_eq!(parse_condition("title ~ \"btc halving\"").unwrap().value, "btc halving");
        assert_eq!(parse_condition("out ?").unwrap().op, Op::Exists);
        assert!(parse_condition("out ? x").is_err() && parse_condition("= 3").is_err() && parse_condition("x 3").is_err());
        assert!(compare(Some(&json!(401)), Op::Gt, "0"));
        assert!(compare(Some(&json!("PDF")), Op::Eq, "pdf"));
        assert!(compare(Some(&json!("/tmp/a.pdf")), Op::Eq, "*.pdf"));
        assert!(compare(Some(&json!(["a.csv", "b.csv"])), Op::Contains, "b."));
        assert!(compare(Some(&json!(["a", "b"])), Op::Ge, "2"), "arrays compare by length");
        assert!(compare(None, Op::Missing, "") && compare(None, Op::Ne, "x") && !compare(None, Op::Eq, "x"));
        assert!(compare(Some(&json!("2026-10-29")), Op::Gt, "2026-01-01"));
    }

    #[test]
    fn then_syntax_round_trips() {
        let s = Store::open_in_memory().unwrap();
        let j = s.create_journal("Lab notes", None, EntryFormat::Markdown).unwrap();
        s.create_report("Weekly", "").unwrap();
        s.save_live_search("btc", &SearchSpec::default(), None).unwrap();
        s.create_project("P", "").unwrap();
        for t in [
            "status \"imported {inserted} bars\"",
            "search btc",
            "export-report Weekly to=/tmp/w-{date}.pdf",
            "export-journal \"Lab notes\" to=/tmp/j.html format=html",
            "add-to-project P item=search:btc",
            "entry \"Lab notes\" \"saw {summary}\" title=Auto",
            "note \"x = y\"",
        ] {
            let parsed = parse_then(&s, t).unwrap();
            assert_eq!(parse_then(&s, &then_text(&s, &parsed)).unwrap(), parsed, "{t}");
        }
        assert_eq!(
            parse_then(&s, "entry \"Lab notes\" hi").unwrap(),
            Then::WriteEntry { journal: Some(j.id), title: String::new(), text: "hi".into() }
        );
        assert!(parse_then(&s, "export-report Weekly").is_err(), "needs to=");
        assert!(parse_then(&s, "status hi to=x").is_err());
        assert!(parse_then(&s, "explode").is_err());
        assert_eq!(split_list("status \"a;b\"; search btc ;"), vec!["status \"a;b\"", "search btc"]);
    }

    #[test]
    fn fields_and_templates() {
        let s = Store::open_in_memory().unwrap();
        let r = rule(&s, "*", &[], &["status x"]);
        let a = act(&s, "export", json!({ "out": "/tmp/x.csv", "bars": 3, "nested": { "k": [1, 2] } }));
        assert_eq!(field_value(&s, &a, "bars"), Some(json!(3)));
        assert_eq!(field_value(&s, &a, "params.nested.k.1"), Some(json!(2)));
        assert_eq!(field_value(&s, &a, "origin"), Some(json!("cli")));
        assert_eq!(field_value(&s, &a, "nope"), None);
        assert_eq!(expand(&s, &r, &a, "{rule}: {bars} bars to {out} {unknown} {"), format!("{}: 3 bars to /tmp/x.csv {{unknown}} {{", r.name));
    }

    #[test]
    fn rules_fire_record_runs_and_stop_loops() {
        let s = Store::open_in_memory().unwrap();
        let j = s.create_journal("Log", None, EntryFormat::Markdown).unwrap();
        // writes an entry on every entry.add in the same journal: would loop forever
        let r = rule(&s, "entry.add", &[], &["entry Log \"echo of {title}\""]);
        let first = s.add_entry(&j.id, "hello", "", None).unwrap();
        act(&s, "entry.add", json!({ "journal": j.id, "entry": first.id, "title": "hello" }));
        let entries = s.entries(&j.id).unwrap();
        assert_eq!(entries.len(), 2, "the rule ran once and did not trigger itself");
        assert_eq!(entries[1].body, "echo of hello");
        let runs = s.actions_of_kind("rule.run").unwrap();
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].params["status"], "ok");
        assert_eq!(runs[1].params["status"], "blocked");
        let r = s.rule(&r.id).unwrap().unwrap();
        assert_eq!((r.runs, r.last_status.as_deref()), (2, Some("ok")), "the blocked echo ran inside the run, which finished last");
        // the entry the rule wrote is a child of its run and carries the chain
        let added = s.actions_of_kind("entry.add").unwrap().pop().unwrap();
        assert_eq!(added.origin, "rule");
        assert_eq!(added.parent, Some(runs[0].id));
        assert_eq!(added.params["rule_chain"], json!([r.id]));
    }

    #[test]
    fn conditions_related_and_failures() {
        let s = Store::open_in_memory().unwrap();
        let j = s.create_journal("Mine", None, EntryFormat::Markdown).unwrap();
        let other = s.create_journal("Other", None, EntryFormat::Markdown).unwrap();
        let mut r = rule(&s, "journal.*", &["name ~ e"], &["status \"{name} changed\"", "search"]);
        r = s.update_rule(&r.id, &crate::store::RuleChange { owner: Some((OwnerKind::Journal, Some(j.id.clone()))), ..Default::default() }).unwrap();
        let mut sp = r.spec.clone();
        sp.related = true;
        let r = s.update_rule(&r.id, &crate::store::RuleChange { spec: Some(sp), ..Default::default() }).unwrap();
        let a = act(&s, "journal.edit", json!({ "journal": other.id, "name": "Other" }));
        let c = check(&s, &r, &a);
        assert!(!c.matched && c.related == Some(false) && c.conditions[0].ok);
        assert!(s.actions_of_kind("rule.run").unwrap().is_empty());
        act(&s, "journal.edit", json!({ "journal": j.id, "name": "Mine" }));
        let run = s.actions_of_kind("rule.run").unwrap().pop().unwrap();
        // the status works, the search has no target: the run carries on and fails
        assert_eq!(run.params["status"], "failed");
        assert_eq!(run.params["messages"][0], "Mine changed");
        assert!(run.summary.contains("search failed"));
        // disabled rules do not run
        s.set_rule_enabled(&r.id, false).unwrap();
        act(&s, "journal.edit", json!({ "journal": j.id, "name": "Mine" }));
        assert_eq!(s.actions_of_kind("rule.run").unwrap().len(), 1);
    }

    #[test]
    fn add_to_project_and_dry_run() {
        let s = Store::open_in_memory().unwrap();
        let p = s.create_project("P", "").unwrap();
        let l = s.save_live_search("btc", &SearchSpec::default(), None).unwrap();
        let r = rule(&s, "search.save", &[], &["add-to-project"]);
        s.add_project_item(&p.id, ItemKind::RuleSet, &r.id, json!({})).unwrap();
        let a = act(&s, "search.save", json!({ "search": l.id, "name": "btc" }));
        assert_eq!(s.project_items(&p.id).unwrap().len(), 2, "rule + the search");
        assert_eq!(plan(&s, &r, &a, &r.spec.then[0]).unwrap(), "add live search btc to project P");
        assert!(home(&s, &r).contains("project P"));
        let imp = act(&s, "import", json!({}));
        assert!(plan(&s, &r, &imp, &r.spec.then[0]).is_err(), "an import has no subject");
    }

    #[test]
    fn exports_to_a_templated_path() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open_in_memory().unwrap();
        let rep = s.create_report("Weekly", "").unwrap();
        let to = format!("{}/weekly-{{action}}.html", dir.path().display());
        rule(&s, "import", &["inserted > 0"], &[&format!("export-report Weekly to=\"{to}\"")]);
        act(&s, "import", json!({ "inserted": 0 }));
        assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none(), "condition not met");
        let a = act(&s, "import", json!({ "inserted": 5 }));
        assert!(dir.path().join(format!("weekly-{}.html", a.id)).exists());
        let exp = s.actions_of_kind("report.export").unwrap();
        assert_eq!((exp.len(), exp[0].origin.as_str(), exp[0].params["report"].as_str()), (1, "rule", Some(rep.id.as_str())));
    }

    #[test]
    fn model_then_actions_parse_plan_and_run() {
        let s = Store::open_in_memory().unwrap();
        s.create_model("signals", "", Default::default(), true).unwrap();
        let sig = s.find_model("signals").unwrap();
        s.edit_model(&sig.id, &[FieldOp::Add { spec: "symbol:text!".into() }, FieldOp::Add { spec: "price:real".into() }]).unwrap();
        s.apply_model(&sig.id, &ApplyOpts::default()).unwrap();
        for t in [
            "model-create notes fields=\"text:text!; n:integer default=0\" about=\"from rules\"",
            "model-create drafts fields=\"a:text\" draft=yes",
            "model-edit signals add=\"note:text\" rename=\"price:cost\" apply=yes",
            "model-edit signals drop=price destructive=yes",
            "model-apply signals",
            "model-remove signals destructive=yes",
            "record-create signals symbol={symbol} price=1",
            "record-update signals id=abc price=2",
            "record-update signals where=\"symbol = BTC*\" price=",
            "record-delete signals where=\"price < 1\"",
        ] {
            let parsed = parse_then(&s, t).unwrap_or_else(|e| panic!("{t}: {e}"));
            assert_eq!(parse_then(&s, &then_text(&s, &parsed)).unwrap(), parsed, "{t}");
        }
        assert!(parse_then(&s, "record-update signals price=2").is_err(), "needs id= or where=");
        assert!(parse_then(&s, "record-update signals id=x").is_err(), "needs values");
        assert!(parse_then(&s, "record-delete signals").is_err());
        assert!(parse_then(&s, "model-edit signals add=\"nope\"").is_err(), "checked against the model");
        assert!(parse_then(&s, "model-edit nosuch add=\"a:text\"").is_err());
        assert!(check_pattern("model.apply").is_ok() && check_pattern("record.*").is_ok());

        // a rule living in the model feeds it; dry runs say what would happen
        let r = s
            .create_rule(&NewRule {
                name: "feed".into(),
                owner_kind: OwnerKind::Model,
                owner_id: Some(sig.id.clone()),
                spec: RuleSpec {
                    on: vec!["import".into()],
                    then: vec![parse_then(&s, "record-create symbol={symbol} price={price}").unwrap()],
                    ..Default::default()
                },
                // off: only the explicit runs below (an enabled rule fires on `act` too)
                enabled: false,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(home(&s, &r), "model signals");
        let a = act(&s, "import", json!({ "symbol": "BTC/USDT", "price": 42 }));
        let p = plan(&s, &r, &a, &r.spec.then[0]).unwrap();
        assert!(p.contains("signals") && p.contains("BTC/USDT"), "{p}");
        let out = run(&s, &r, &a);
        assert_eq!(out.status, "ok", "{:?}", out.messages);
        let rs = s.records(&s.model(&sig.id).unwrap().unwrap(), &Default::default()).unwrap();
        assert_eq!((rs.len(), rs[0].values["price"].clone()), (1, json!(42.0)));
        let made = s.actions_of_kind("record.create").unwrap();
        assert_eq!((made.len(), made[0].origin.as_str()), (1, "rule"));

        // migrations from rules: destructive ones need destructive=yes
        let mk = |t: &str| Rule { spec: RuleSpec { then: vec![parse_then(&s, t).unwrap()], ..Default::default() }, ..r.clone() };
        let drop = mk("model-edit drop=price apply=yes");
        let e = exec(&s, &drop, &a, &drop.spec.then[0]).unwrap_err().to_string();
        assert!(e.contains("destructive=yes"), "{e}");
        let ok = mk("model-edit drop=price apply=yes destructive=yes");
        let out = run(&s, &ok, &a);
        assert_eq!(out.status, "ok", "{:?}", out.messages);
        let m = s.model(&sig.id).unwrap().unwrap();
        assert!(m.design.field("price").is_none() && m.version == 3, "{m:?}");
        let v = s.model_versions(&sig.id).unwrap();
        assert_eq!((v[2].note.as_str(), v[2].rule_id.as_deref()), ("rule feed", Some(r.id.as_str())));

        // updates and deletes by filter; model-create is idempotent
        let up = mk("record-update where=\"symbol = btc*\" symbol=done");
        assert!(exec(&s, &up, &a, &up.spec.then[0]).unwrap().contains('1'));
        let del = mk("record-delete where=\"symbol = done\"");
        exec(&s, &del, &a, &del.spec.then[0]).unwrap();
        assert_eq!(s.count_records(&m).unwrap(), 0);
        let c = mk("model-create notes fields=\"text:text\"");
        exec(&s, &c, &a, &c.spec.then[0]).unwrap();
        assert!(exec(&s, &c, &a, &c.spec.then[0]).unwrap().contains("exists"));
        assert_eq!(s.find_model("notes").unwrap().version, 1);
        let rm = mk("model-remove notes");
        exec(&s, &rm, &a, &rm.spec.then[0]).unwrap();
        assert!(s.find_model("notes").is_err());
        let ev: Vec<&str> = EVENTS.iter().map(|e| e.kind).collect();
        for k in ["model.create", "model.edit", "model.apply", "model.rollback", "model.remove", "record.create", "record.update", "record.delete"] {
            assert!(ev.contains(&k), "{k}");
        }
        assert_eq!(subject(&made[0]).unwrap(), (ItemKind::Model, sig.id.clone()));
    }
}
