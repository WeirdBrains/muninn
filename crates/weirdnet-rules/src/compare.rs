//! How a condition compares a JSON value: numbers as numbers, text case-insensitively with
//! `*` globs for `=`, arrays by any element (and by length for `<`/`>`), missing values only
//! satisfying `!?`, `!=` and `!~`.
use crate::model::Op;
use serde_json::Value;

/// A dotted path into a JSON value (`spec.query`, `files.0`); `null` counts as absent.
pub fn path<'a>(mut v: &'a Value, p: &str) -> Option<&'a Value> {
    for k in p.split('.').filter(|k| !k.is_empty()) {
        v = match v {
            Value::Object(m) => m.get(k)?,
            Value::Array(a) => a.get(k.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    (!v.is_null()).then_some(v)
}

/// A value as text: strings as they are, anything else as JSON.
pub fn text(v: &Value) -> String {
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

/// `*` matches any run of characters; no other wildcards.
pub fn glob(p: &str, s: &str) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn operators() {
        assert!(compare(Some(&json!(401)), Op::Gt, "0"));
        assert!(compare(Some(&json!("PDF")), Op::Eq, "pdf"));
        assert!(compare(Some(&json!("/tmp/a.pdf")), Op::Eq, "*.pdf"));
        assert!(compare(Some(&json!(["a.csv", "b.csv"])), Op::Contains, "b."));
        assert!(compare(Some(&json!(["a", "b"])), Op::Ge, "2"), "arrays compare by length");
        assert!(compare(None, Op::Missing, "") && compare(None, Op::Ne, "x") && !compare(None, Op::Eq, "x"));
        assert!(compare(None, Op::NotContains, "x") && !compare(None, Op::Exists, "") && !compare(None, Op::Gt, "0"));
        assert!(compare(Some(&json!("2026-10-29")), Op::Gt, "2026-01-01"));
        assert!(compare(Some(&json!(true)), Op::Eq, "true") && !compare(Some(&json!(true)), Op::Eq, "1"), "booleans are text for =");
        assert!(compare(Some(&json!("10")), Op::Eq, "10.0") && compare(Some(&json!(" 3 ")), Op::Lt, "4"));
        assert!(compare(Some(&json!(["x", "y"])), Op::Ne, "z") && !compare(Some(&json!(["x", "y"])), Op::Ne, "Y"));
        assert!(compare(Some(&json!("Hello")), Op::NotContains, "bye") && !compare(Some(&json!("Hello")), Op::NotContains, "ELL"));
        assert!(compare(Some(&json!(1.5)), Op::Le, "1.5") && !compare(Some(&json!("abc")), Op::Lt, "abb"));
        assert!(compare(Some(&json!({"a": 1})), Op::Exists, ""));
    }

    #[test]
    fn paths_and_globs() {
        let v = json!({"spec": {"query": "btc", "n": null}, "files": ["a", "b"]});
        assert_eq!(path(&v, "spec.query"), Some(&json!("btc")));
        assert_eq!(path(&v, "files.1"), Some(&json!("b")));
        assert_eq!(path(&v, "spec.n"), None, "null is absent");
        assert_eq!(path(&v, "files.x"), None);
        assert_eq!(path(&v, "spec.query.deeper"), None);
        assert!(glob("a*c*e", "abcde") && glob("*", "") && !glob("a*c", "abcd") && glob("a**", "a"));
        assert_eq!(text(&json!(3)), "3");
    }
}
