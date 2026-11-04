//! Built-in parser code spaces. Sources pass data through as they have it (standard input
//! as raw text in a `text` column); shaping it is a code space's job, placed as a step.
//! These ship as templates: `mnnn code parsers` lists them, `--install` (or the first
//! `mnnn pipe - --format …`) makes them ordinary code spaces to view, copy and edit. Like
//! any code they run only once approved.
use crate::error::Result;
use crate::store::{CodeSpace, Lang, Store};

pub struct Builtin {
    pub name: &'static str,
    pub about: &'static str,
    pub source: &'static str,
}

pub const BUILTINS: &[Builtin] = &[
    Builtin { name: "parse-csv", about: "raw text → rows: CSV with a header line", source: include_str!("parsers/csv.rs") },
    Builtin { name: "parse-json", about: "raw text → rows: a JSON array of objects", source: include_str!("parsers/json.rs") },
    Builtin { name: "parse-ndjson", about: "raw text → rows: one JSON object a line", source: include_str!("parsers/ndjson.rs") },
    Builtin { name: "to-bars", about: "rows → price bars (timestamp, OHLC, volume) by column names", source: include_str!("parsers/to_bars.rs") },
];

pub fn builtin(name: &str) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|b| b.name == name)
}

/// The parser for `--format`: csv, json, ndjson (jsonl).
pub fn for_format(format: &str) -> Option<&'static Builtin> {
    builtin(match format.trim().to_ascii_lowercase().as_str() {
        "csv" => "parse-csv",
        "json" => "parse-json",
        "ndjson" | "jsonl" | "jsonlines" => "parse-ndjson",
        "bars" | "to-bars" => "to-bars",
        _ => return None,
    })
}

/// The code space of a built-in, made from its template when there is none of that name
/// (an existing one is used as it is: it may have been edited).
pub fn install(store: &Store, origin: &str, b: &Builtin) -> Result<(CodeSpace, bool)> {
    if let Ok(c) = store.find_code_space(b.name) {
        return Ok((c, false));
    }
    let c = store.create_code_space(b.name, b.about, Lang::Rust, b.source, Default::default())?;
    store.note_action(
        &crate::store::NewAction::new("code.create", origin, format!("built-in parser {} installed", b.name))
            .params(serde_json::json!({ "space": c.id, "builtin": b.name })),
    );
    Ok((c, true))
}

/// Raw text as a frame: one row, one `text` column, nothing parsed.
pub fn raw_frame(text: &str, name: &str) -> serde_json::Value {
    serde_json::json!({ "model": name, "fields": [{ "name": "text", "type": "text" }], "ids": ["0"], "columns": { "text": [text] }, "groups": [] })
}
