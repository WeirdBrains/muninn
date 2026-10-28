//! JSON / NDJSON reader: records (objects or positional arrays) -> an all-string Polars frame.
use super::{canonical, expand_path, hash_bytes, IngestOptions, IngestReport, Input, Session};
use crate::error::{Error, Result};
use crate::interact::Interact;
use crate::store::Store;
use polars::prelude::*;
use serde_json::Value;
use std::path::Path;

/// Follow a dotted path (`data.items`), JSON pointer (`/data/items`) or `$.data.items`.
pub fn select<'a>(v: &'a Value, path: Option<&str>) -> Result<&'a Value> {
    let Some(p) = path.map(str::trim).filter(|p| !p.is_empty() && *p != "$" && *p != ".") else {
        return Ok(v);
    };
    if p.starts_with('/') {
        return v.pointer(p).ok_or_else(|| Error::invalid("--records", format!("`{p}` not found in the document")));
    }
    let p = p.trim_start_matches("$.").trim_start_matches('$');
    let mut cur = v;
    for seg in p.split('.').filter(|s| !s.is_empty()) {
        cur = match cur {
            Value::Object(m) => m.get(seg),
            Value::Array(a) => seg.parse::<usize>().ok().and_then(|i| a.get(i)),
            _ => None,
        }
        .ok_or_else(|| Error::invalid("--records", format!("`{seg}` not found in the document")))?;
    }
    Ok(cur)
}

fn cell(v: &Value) -> Option<String> {
    match v {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        other => Some(other.to_string()),
    }
}

/// Build a frame from records: objects (keys become columns) or arrays (positional `column_N`).
pub fn records_to_frame(records: &[Value]) -> Result<DataFrame> {
    if records.is_empty() {
        return Err(Error::invalid("input", "no records found"));
    }
    let mut names: Vec<String> = Vec::new();
    let positional = records[0].is_array();
    if positional {
        let w = records.iter().filter_map(|r| r.as_array().map(Vec::len)).max().unwrap_or(0);
        names = (1..=w).map(|i| format!("column_{i}")).collect();
    } else {
        for r in records {
            let Value::Object(m) = r else {
                return Err(Error::invalid("input", "records must all be objects or all be arrays"));
            };
            for k in m.keys() {
                if !names.contains(k) {
                    names.push(k.clone());
                }
            }
        }
    }
    let mut cols: Vec<Vec<Option<String>>> = vec![Vec::with_capacity(records.len()); names.len()];
    for r in records {
        for (i, n) in names.iter().enumerate() {
            let v = match r {
                Value::Array(a) => a.get(i),
                Value::Object(m) => m.get(n),
                _ => None,
            };
            cols[i].push(v.and_then(cell));
        }
    }
    let columns: Vec<Column> = names
        .iter()
        .zip(cols)
        .map(|(n, c)| Column::new(n.as_str().into(), c))
        .collect();
    Ok(DataFrame::new_infer_height(columns)?)
}

/// Parse JSON or NDJSON bytes into records.
pub fn parse_records(bytes: &[u8], ndjson: bool, path: Option<&str>) -> Result<Vec<Value>> {
    if ndjson {
        let mut v = Vec::new();
        for (i, line) in bytes.split(|b| *b == b'\n').enumerate() {
            let l = std::str::from_utf8(line).map_err(|e| Error::invalid("ndjson", e.to_string()))?.trim();
            if l.is_empty() {
                continue;
            }
            v.push(serde_json::from_str(l).map_err(|e| Error::invalid("ndjson", format!("line {}: {e}", i + 1)))?);
        }
        return Ok(v);
    }
    let doc: Value = serde_json::from_slice(bytes)?;
    match select(&doc, path)? {
        Value::Array(a) => Ok(a.clone()),
        Value::Object(_) => Err(Error::invalid("--records", "the selected value is an object; point --records at the array of bars")),
        _ => Err(Error::invalid("--records", "the selected value is not an array")),
    }
}

pub fn is_ndjson(p: &Path) -> bool {
    p.extension().is_some_and(|e| matches!(e.to_string_lossy().to_ascii_lowercase().as_str(), "ndjson" | "jsonl"))
}

/// Parse JSON (array/object) or NDJSON bytes into a frame.
pub fn read_json_bytes(b: &[u8], ndjson: bool) -> Result<DataFrame> {
    records_to_frame(&parse_records(b, ndjson, None)?)
}

pub fn read_json_file(p: &Path, records: Option<&str>) -> Result<DataFrame> {
    let b = std::fs::read(p)?;
    records_to_frame(&parse_records(&b, is_ndjson(p), records)?)
}

pub fn ingest_json(store: &mut Store, io: Interact, opts: IngestOptions, path: &Path, records: Option<&str>) -> Result<Vec<IngestReport>> {
    let files = expand_path(path, &["json", "ndjson", "jsonl"])?;
    let kind = if path.is_dir() { "directory" } else { "file" };
    let mut s = Session::new(store, io, opts, kind, &canonical(path));
    let mut out = Vec::new();
    let prog = s.opts.progress.clone();
    for f in files {
        let bytes = super::progress::read_counted(&f, prog.clone())?;
        let frame = records_to_frame(&parse_records(&bytes, is_ndjson(&f), records)?)?;
        out.push(s.ingest(Input {
            locator: canonical(&f),
            name_hint: f.file_name().map(|n| n.to_string_lossy().to_string()),
            content_hash: hash_bytes(&bytes),
            frame,
        })?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn objects_arrays_and_paths() {
        let doc = br#"{"data":{"bars":[{"t":"2024-01-01 00:00:00","o":1,"h":2,"l":0.5,"c":1.5,"v":10}]}}"#;
        let r = parse_records(doc, false, Some("data.bars")).unwrap();
        let df = records_to_frame(&r).unwrap();
        assert_eq!(df.height(), 1);
        assert_eq!(df.width(), 6);
        assert!(parse_records(doc, false, Some("/data/bars")).is_ok());
        assert!(parse_records(doc, false, Some("$.data.bars")).is_ok());
        assert!(parse_records(doc, false, Some("data.nope")).is_err());
        assert!(parse_records(doc, false, None).is_err());
        let kl = br#"[[1502942400000,"4261.48","4261.48","4261.48","4261.48","1.77",1502942699999]]"#;
        let df = records_to_frame(&parse_records(kl, false, None).unwrap()).unwrap();
        assert_eq!(df.get_column_names()[0].as_str(), "column_1");
        let nd = b"{\"a\":1}\n\n{\"a\":2}\n";
        assert_eq!(parse_records(nd, true, None).unwrap().len(), 2);
    }
}
