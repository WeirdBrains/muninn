//! The text forms: `on` patterns, conditions, lists, words and quoting.
use crate::error::{Result, RulesError};
use crate::model::{Condition, Op};

/// Does an `on` pattern (`*`, `kind`, `prefix.*`) match an event kind?
pub fn matches_kind(p: &str, kind: &str) -> bool {
    let p = p.trim();
    p == "*" || p == kind || p.strip_suffix(".*").is_some_and(|pre| kind.starts_with(&format!("{pre}.")) || kind == pre)
}

/// Check an `on` pattern against the application's event kinds. `hint` is appended to the
/// "matches no action kind" message (for example where the catalog is listed).
pub fn check_pattern(p: &str, kinds: &[&str], hint: &str) -> Result<String> {
    let p = p.trim();
    if p.is_empty() {
        return Err(RulesError::invalid("--on", "is empty"));
    }
    if kinds.iter().any(|k| matches_kind(p, k)) {
        Ok(p.to_string())
    } else {
        Err(RulesError::invalid("--on", format!("`{p}` matches no action kind{hint}")))
    }
}

/// `import,journal.*` and repeated values both work.
pub fn parse_on(v: &[String], kinds: &[&str], hint: &str) -> Result<Vec<String>> {
    v.iter().flat_map(|s| s.split([',', ' '])).filter(|s| !s.trim().is_empty()).map(|p| check_pattern(p, kinds, hint)).collect()
}

/// Split `a; b; "c;d"` on semicolons outside quotes (lists on one line).
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

/// Words of a then-action: each word, and its key when written `key=value` (the key
/// unquoted, before the first `=`). Quotes group words and are removed.
pub fn tokenize(s: &str) -> Result<Vec<(String, Option<String>)>> {
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
                        return Err(RulesError::invalid("then", format!("unclosed {c} in `{s}`")));
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
    parse_condition_with(s, &[])
}

/// [`parse_condition`] where fields starting with one of `call_prefixes` (like `svc:`) are
/// call-style: `PREFIX NAME/TOOL(args).path`, whose arguments may hold any character but `)`.
pub fn parse_condition_with(s: &str, call_prefixes: &[&str]) -> Result<Condition> {
    let s = s.trim();
    let end = if let Some(pre) = call_prefixes.iter().find(|p| s.starts_with(**p)) {
        let open = s.find('(');
        let from = match (open, s.find(')')) {
            (Some(o), Some(c)) if o < s.find(char::is_whitespace).unwrap_or(s.len()) => c + 1,
            _ => pre.len(),
        };
        from + s[from..].find(|c: char| !(c.is_ascii_alphanumeric() || "_.-:/".contains(c))).unwrap_or(s.len() - from)
    } else {
        s.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-')).unwrap_or(s.len())
    };
    let field = &s[..end];
    if field.is_empty() {
        return Err(RulesError::invalid("--if", format!("`{s}`: start with a field, e.g. `inserted > 0`")));
    }
    let rest = s[end..].trim_start();
    let Some(op) = Op::ALL.into_iter().find(|o| rest.starts_with(o.as_str())) else {
        return Err(RulesError::invalid("--if", format!("`{s}`: expected an operator (= != ~ !~ > >= < <= ? !?)")));
    };
    let mut value = rest[op.as_str().len()..].trim().to_string();
    if value.len() >= 2 && (value.starts_with('"') && value.ends_with('"') || value.starts_with('\'') && value.ends_with('\'')) {
        value = value[1..value.len() - 1].to_string();
    }
    if op.unary() != value.is_empty() {
        return Err(RulesError::invalid("--if", format!("`{s}`: {}", if op.unary() { "`?` and `!?` take no value" } else { "missing a value" })));
    }
    Ok(Condition { field: field.to_string(), op, value })
}

/// A condition in the form [`parse_condition`] reads.
pub fn condition_text(c: &Condition) -> String {
    if c.op.unary() { format!("{} {}", c.field, c.op.as_str()) } else { format!("{} {} {}", c.field, c.op.as_str(), quote(&c.value)) }
}

/// A value quoted only when it has to be.
pub fn quote(s: &str) -> String {
    if !s.is_empty() && !s.contains(|c: char| c.is_whitespace() || "\"';=".contains(c)) {
        s.to_string()
    } else if s.contains('"') {
        format!("'{s}'")
    } else {
        format!("\"{s}\"")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KINDS: &[&str] = &["import", "journal.export", "report.section.add", "schedule"];

    #[test]
    fn patterns() {
        assert!(matches_kind("journal.*", "journal.export") && matches_kind("*", "import") && !matches_kind("entry.*", "entryx"));
        assert!(matches_kind("journal.*", "journal"));
        assert!(check_pattern("nope", KINDS, "").is_err() && check_pattern("report.section.*", KINDS, "").is_ok());
        assert_eq!(check_pattern("x", KINDS, " (see the list)").unwrap_err().to_string(), "invalid --on: `x` matches no action kind (see the list)");
        assert_eq!(parse_on(&["import,journal.*".into(), "schedule".into()], KINDS, "").unwrap(), ["import", "journal.*", "schedule"]);
        assert!(check_pattern(" ", KINDS, "").is_err());
    }

    #[test]
    fn conditions_parse_print_and_round_trip() {
        let c = parse_condition("inserted>=10").unwrap();
        assert_eq!((c.field.as_str(), c.op, c.value.as_str()), ("inserted", Op::Ge, "10"));
        assert_eq!(parse_condition("title ~ \"btc halving\"").unwrap().value, "btc halving");
        assert_eq!(parse_condition("out ?").unwrap().op, Op::Exists);
        assert!(parse_condition("out ? x").is_err() && parse_condition("= 3").is_err() && parse_condition("x 3").is_err());
        for s in ["inserted > 0", "title ~ \"btc halving\"", "out ?", "name = 'say \"hi\"'", "a.b-c != x"] {
            let c = parse_condition(s).unwrap();
            assert_eq!(parse_condition(&condition_text(&c)).unwrap(), c, "{s}");
        }
        // call-style fields only when the application names the prefix
        let c = parse_condition_with("svc:a/b(q=x y,n=2).count > 1", &["svc:"]).unwrap();
        assert_eq!((c.field.as_str(), c.op), ("svc:a/b(q=x y,n=2).count", Op::Gt));
        assert!(parse_condition("svc:a/b(q=x y).count > 1").is_err());
    }

    #[test]
    fn lists_words_and_quotes() {
        assert_eq!(split_list("a; b ;\"c;d\"; ;'e;f'"), ["a", "b", "\"c;d\"", "'e;f'"]);
        let t = tokenize("entry \"two words\" title=T x='a b' \"k=v\"").unwrap();
        assert_eq!(t[1], ("two words".to_string(), None));
        assert_eq!(t[2], ("title".to_string(), Some("T".to_string())));
        assert_eq!(t[3], ("x".to_string(), Some("a b".to_string())));
        assert_eq!(t[4], ("k=v".to_string(), None), "a quoted = is not a key");
        assert_eq!(tokenize("status \"open").unwrap_err().to_string(), "invalid then: unclosed \" in `status \"open`");
        assert_eq!(quote("plain"), "plain");
        assert_eq!(quote("two words"), "\"two words\"");
        assert_eq!(quote("say \"hi\""), "'say \"hi\"'");
        assert_eq!(quote(""), "\"\"");
    }
}
