//! Frames: what flows between nodes, as JSON: `{model, fields, ids, columns, groups}` with
//! `fields` a list of `{name, type}`, `ids` one per row and `columns` an object of equally
//! long arrays, one per field. These helpers only reshape frames; they never look anything up.
use crate::error::{PipelineError, Result};
use serde_json::{Map, Value, json};

/// The rows of a frame (its `ids`).
pub fn rows(f: &Value) -> usize {
    f.get("ids").and_then(Value::as_array).map(Vec::len).unwrap_or(0)
}

/// A frame from `(name, type, values)` columns, row ids and groups.
pub fn columns_frame(name: &str, cols: Vec<(&str, &str, Vec<Value>)>, ids: Vec<String>, groups: Vec<Value>) -> Value {
    let fields: Vec<Value> = cols.iter().map(|(n, t, _)| json!({ "name": n, "type": t })).collect();
    let mut columns = Map::new();
    for (n, _, v) in cols {
        columns.insert(n.to_string(), Value::Array(v));
    }
    json!({ "model": name, "fields": fields, "ids": ids, "columns": columns, "groups": groups })
}

/// Rows of objects as a frame (columns in first-seen order; a column of numbers and nulls is
/// `real`, any other `text`).
pub fn rows_frame(name: &str, rows: &[Map<String, Value>]) -> Value {
    let mut names: Vec<String> = vec![];
    for r in rows {
        for k in r.keys() {
            if !names.contains(k) {
                names.push(k.clone());
            }
        }
    }
    let cols = names
        .iter()
        .map(|n| {
            let v: Vec<Value> = rows.iter().map(|r| r.get(n).cloned().unwrap_or(Value::Null)).collect();
            let t = if v.iter().all(|x| x.is_number() || x.is_null()) { "real" } else { "text" };
            (n.as_str(), t, v)
        })
        .collect();
    columns_frame(name, cols, (0..rows.len()).map(|i| i.to_string()).collect(), vec![])
}

/// Frames one after the other (columns by name; groups dropped). One frame comes back as is.
pub fn union(name: &str, frames: &[&Value]) -> Value {
    if frames.len() == 1 {
        return frames[0].clone();
    }
    let mut out: Vec<Map<String, Value>> = vec![];
    for f in frames {
        let cols = f.get("columns").and_then(Value::as_object).cloned().unwrap_or_default();
        for i in 0..rows(f) {
            out.push(cols.iter().map(|(k, v)| (k.clone(), v.get(i).cloned().unwrap_or(Value::Null))).collect());
        }
    }
    rows_frame(name, &out)
}

/// Only these columns (all of them when `columns` is empty).
pub fn select(f: &Value, columns: &[String]) -> Value {
    if columns.is_empty() {
        return f.clone();
    }
    let mut v = f.clone();
    if let Some(cols) = f.get("columns").and_then(Value::as_object) {
        v["columns"] = Value::Object(cols.iter().filter(|(k, _)| columns.contains(k)).map(|(k, x)| (k.clone(), x.clone())).collect());
    }
    v
}

/// Rows of a frame, kept where `keep(row)` holds (groups dropped).
pub fn filter_rows(f: &Value, keep: &dyn Fn(&Map<String, Value>) -> bool) -> Value {
    let cols = f.get("columns").and_then(Value::as_object).cloned().unwrap_or_default();
    let n = rows(f);
    let kept: Vec<usize> =
        (0..n).filter(|i| keep(&cols.iter().map(|(k, v)| (k.clone(), v.get(*i).cloned().unwrap_or(Value::Null))).collect())).collect();
    let mut out = f.clone();
    out["ids"] = Value::Array(kept.iter().map(|i| f["ids"][*i].clone()).collect());
    out["columns"] = Value::Object(cols.iter().map(|(k, v)| (k.clone(), Value::Array(kept.iter().map(|i| v[*i].clone()).collect()))).collect());
    out["groups"] = json!([]);
    out
}

/// The last `keep` rows of a frame (groups dropped when it is cut).
pub fn tail(f: &Value, keep: usize) -> Value {
    let n = f.get("ids").and_then(Value::as_array).map(Vec::len).unwrap_or_else(|| {
        f.get("columns").and_then(Value::as_object).and_then(|c| c.values().next()).and_then(Value::as_array).map(Vec::len).unwrap_or(0)
    });
    if n <= keep {
        return f.clone();
    }
    let from = n - keep;
    let mut out = f.clone();
    if let Some(ids) = f.get("ids").and_then(Value::as_array) {
        out["ids"] = Value::Array(ids[from..].to_vec());
    }
    if let Some(cols) = f.get("columns").and_then(Value::as_object) {
        out["columns"] = Value::Object(
            cols.iter().map(|(k, v)| (k.clone(), Value::Array(v.as_array().map(|a| a[from.min(a.len())..].to_vec()).unwrap_or_default()))).collect(),
        );
    }
    out["groups"] = json!([]);
    out
}

/// One CSV line (quotes, `""` inside quotes).
pub fn csv_line(l: &str) -> Vec<String> {
    let (mut out, mut cur, mut q) = (vec![], String::new(), false);
    let mut it = l.trim_end_matches('\r').chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '"' if q && it.peek() == Some(&'"') => {
                cur.push('"');
                it.next();
            }
            '"' => q = !q,
            ',' if !q => out.push(std::mem::take(&mut cur)),
            c => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// Text as a frame: a frame as is, a JSON array of objects, NDJSON (an object a line) or CSV
/// with a header row. CSV numbers read as numbers and empty cells as null.
pub fn read_data(text: &str, name: &str) -> Result<Value> {
    let t = text.trim_start();
    let rows: Vec<Map<String, Value>> = if t.starts_with('[') {
        let v: Vec<Value> = serde_json::from_str(t).map_err(|e| PipelineError::invalid("input", format!("JSON: {e}")))?;
        v.into_iter().map(obj).collect::<Result<_>>()?
    } else if t.starts_with('{') {
        if let Ok(v) = serde_json::from_str::<Value>(t)
            && v.get("columns").is_some()
        {
            return Ok(v);
        }
        t.lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).map_err(|e| PipelineError::invalid("input", format!("NDJSON: {e}"))).and_then(obj))
            .collect::<Result<_>>()?
    } else {
        let mut lines = t.lines().filter(|l| !l.trim().is_empty());
        let head = csv_line(lines.next().unwrap_or_default());
        lines.map(|l| head.iter().cloned().zip(csv_line(l).into_iter().map(|c| cell(&c))).collect()).collect()
    };
    Ok(rows_frame(name, &rows))
}

fn obj(v: Value) -> Result<Map<String, Value>> {
    match v {
        Value::Object(m) => Ok(m),
        o => Err(PipelineError::invalid("input", format!("expected objects, got {o}"))),
    }
}

fn cell(s: &str) -> Value {
    match s.trim().parse::<f64>() {
        Ok(n) if n.is_finite() => json!(n),
        _ if s.is_empty() => Value::Null,
        _ => json!(s),
    }
}

/// A value as cell text: null empty, a string as is, anything else as JSON.
pub fn cell_text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        o => o.to_string(),
    }
}

/// The frame as a Markdown table (at most `max` rows; columns starting with `_` left out).
pub fn markdown(f: &Value, max: usize) -> String {
    let cols = f.get("columns").and_then(Value::as_object).cloned().unwrap_or_default();
    let names: Vec<&String> = cols.keys().filter(|k| !k.starts_with('_')).collect();
    let n = rows(f);
    if names.is_empty() {
        return "(no columns)\n".into();
    }
    let mut s = format!("| {} |\n|{}\n", names.iter().map(|n| n.as_str()).collect::<Vec<_>>().join(" | "), "---|".repeat(names.len()));
    for i in 0..n.min(max) {
        let cells: Vec<String> = names.iter().map(|k| cell_text(&cols[*k][i]).replace('|', "\\|")).collect();
        s += &format!("| {} |\n", cells.join(" | "));
    }
    if n > max {
        s += &format!("\n… {} more rows\n", n - max);
    }
    s
}
