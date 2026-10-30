//! Polars code spaces: one Polars expression per line (the Rust DSL; a `pl.` prefix is
//! allowed), compiled to Rust against the `polars` crate.
//!
//! ```text
//! @title returns
//! where col("volume").gt(lit(0))
//! ret = col("close").pct_change(lit(1)) * lit(100)
//! vol = col("ret").rolling_std(RollingOptionsFixedWindow { window_size: 20, ..Default::default() })
//! ```
//!
//! The input records become a `DataFrame` (numbers as `Float64`, integers and timestamps as
//! `Int64`, text as `String`, plus `id`); `name = EXPR` adds a column, `where EXPR` keeps
//! the rows where it holds. Directives are those of math code spaces.
use super::math::{Directives, ident};
use crate::error::{Error, Result};

/// The polars version and features generated crates use (the first build compiles Polars;
/// later builds reuse it from the shared target directory).
pub const DEPENDENCY: &str = r#"polars = { version = "=0.55.2", default-features = false, features = ["lazy", "fmt", "rolling_window", "cum_agg", "diff", "pct_change", "abs", "round_series", "log", "ewma", "rank", "is_in", "strings", "dtype-datetime"] }"#;

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Column(String, String),
    Filter(String),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Program {
    pub steps: Vec<Step>,
    pub dirs: Directives,
}

fn balanced(s: &str) -> bool {
    let mut depth: i32 = 0;
    let mut quote = false;
    let mut esc = false;
    for c in s.chars() {
        match c {
            _ if esc => esc = false,
            '\\' if quote => esc = true,
            '"' => quote = !quote,
            '(' | '[' | '{' if !quote => depth += 1,
            ')' | ']' | '}' if !quote => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            return false;
        }
    }
    depth == 0 && !quote
}

fn clean(e: &str) -> String {
    let mut out = String::new();
    let mut quote = false;
    let b = e.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as char;
        if c == '"' && (i == 0 || b[i - 1] != b'\\') {
            quote = !quote;
        }
        let word_start = i == 0 || !(b[i - 1] as char).is_ascii_alphanumeric() && b[i - 1] != b'_';
        if !quote && word_start && e[i..].starts_with("pl.") {
            i += 3;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out.trim().to_string()
}

/// Parse a Polars source. Expressions are checked by the Rust compiler at build time; here
/// only names, brackets and quotes are.
pub fn parse(src: &str) -> Result<Program> {
    let mut p = Program::default();
    // an expression may continue on following lines while brackets are open
    let mut pending: Option<(usize, String)> = None;
    let mut lines: Vec<(usize, String)> = vec![];
    for (i, raw) in src.lines().enumerate() {
        let t = raw.trim();
        if pending.is_none() && (t.is_empty() || t.starts_with('#') || t.starts_with("//")) {
            continue;
        }
        let (n, mut acc) = pending.take().unwrap_or((i + 1, String::new()));
        if !acc.is_empty() {
            acc.push(' ');
        }
        acc.push_str(t);
        if acc.starts_with('.') && pending.is_none() && !lines.is_empty() && !acc.starts_with("..") {
            // a method chain continued on the next line
            let last = lines.last_mut().expect("checked");
            last.1.push(' ');
            last.1.push_str(&acc);
            if !balanced(&last.1) {
                let (n, a) = lines.pop().expect("checked");
                pending = Some((n, a));
            }
        } else if balanced(&acc) || acc.starts_with('@') {
            lines.push((n, acc));
        } else {
            pending = Some((n, acc));
        }
    }
    if let Some((n, _)) = pending {
        return Err(Error::invalid("polars", format!("line {n}: a bracket or quote is not closed")));
    }
    for (n, l) in lines {
        let err = |m: String| Error::invalid("polars", format!("line {n}: {m}"));
        if let Some(d) = l.strip_prefix('@') {
            p.dirs.read(d).map_err(err)?;
        } else if let Some(f) = l.strip_prefix("where ").or_else(|| l.strip_prefix("filter ")) {
            p.steps.push(Step::Filter(clean(f)));
        } else {
            let Some((name, e)) = l.split_once('=').filter(|(_, e)| !e.starts_with('=')) else {
                return Err(err("write NAME = EXPR, where EXPR, or a @directive".into()));
            };
            let name = name.trim();
            ident(name).map_err(err)?;
            if name == "id" || name.starts_with("__") {
                return Err(err(format!("`{name}` is reserved")));
            }
            let e = clean(e);
            if e.is_empty() {
                return Err(err(format!("{name} = …: the expression is missing")));
            }
            p.steps.push(Step::Column(name.to_string(), e));
        }
    }
    if !p.steps.iter().any(|s| matches!(s, Step::Column(..))) {
        return Err(Error::invalid("polars", "no columns: write NAME = EXPR lines, e.g. `ret = col(\"close\").pct_change(lit(1))`"));
    }
    Ok(p)
}

/// The Rust `user.rs` for a Polars source.
pub fn generate(src: &str) -> Result<String> {
    let p = parse(src)?;
    let mut chain = String::new();
    let mut names = vec![];
    for s in &p.steps {
        match s {
            Step::Column(n, e) => {
                chain += &format!("\n        .with_column(({e}).alias({n:?}))");
                names.push(format!("{n:?}"));
            }
            Step::Filter(e) => chain += &format!("\n        .filter({e})"),
        }
    }
    Ok(format!(
        r#"// Generated by Muninn from a Polars code space. Do not edit.
#![allow(unused, clippy::all)]
use muninn_plugin::prelude::*;
use polars::prelude::*;

const NAMES: &[&str] = &[{names}];

fn pe(e: PolarsError) -> Error {{
    Error::new(format!("polars: {{e}}"))
}}

fn to_df(frame: &Frame) -> Result<DataFrame> {{
    let mut cols: Vec<Column> = vec![Column::new("id".into(), frame.ids.clone())];
    cols.push(Column::new("__row".into(), (0..frame.len() as u64).collect::<Vec<u64>>()));
    for f in &frame.fields {{
        let c = frame.column(&f.name)?;
        let col = match f.ty.as_str() {{
            "real" => Column::new(f.name.as_str().into(), c.iter().map(|v| v.as_f64()).collect::<Vec<Option<f64>>>()),
            "integer" | "timestamp" => Column::new(f.name.as_str().into(), c.iter().map(|v| v.as_i64()).collect::<Vec<Option<i64>>>()),
            "bool" => Column::new(f.name.as_str().into(), c.iter().map(|v| v.as_bool()).collect::<Vec<Option<bool>>>()),
            _ => Column::new(f.name.as_str().into(), c.iter().map(|v| (!v.is_null()).then(|| v.text())).collect::<Vec<Option<String>>>()),
        }};
        cols.push(col);
    }}
    DataFrame::new_infer_height(cols).map_err(pe)
}}

fn floats(df: &DataFrame, name: &str) -> Result<Vec<f64>> {{
    let c = df.column(name).map_err(pe)?.cast(&DataType::Float64).map_err(pe)?;
    Ok(c.f64().map_err(pe)?.iter().map(|v| v.unwrap_or(f64::NAN)).collect())
}}

pub fn run(host: &Host) -> Result<View> {{
    let input = host.input()?;
    let out = to_df(&input)?
        .lazy(){chain}
        .collect()
        .map_err(pe)?;
    let rows: Vec<usize> = floats(&out, "__row")?.into_iter().map(|r| r as usize).collect();
    let frame = input.take(&rows);
    let mut cols = vec![];
    for n in NAMES {{
        cols.push((n.to_string(), floats(&out, n)?));
    }}
    muninn_plugin::columns::finish(host, &frame, &{dirs}, cols)
}}
"#,
        names = names.join(", "),
        dirs = p.dirs.output_rs()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sources_parse_and_generate() {
        let src =
            "@view table\nwhere pl.col(\"volume\").gt(lit(0))\nret = pl.col(\"close\")\n  .pct_change(lit(1))\n# c\nbig = col(\"ret\").gt(lit(1))";
        let p = parse(src).unwrap();
        assert_eq!(p.steps.len(), 3);
        assert_eq!(p.steps[1], Step::Column("ret".into(), "col(\"close\") .pct_change(lit(1))".into()));
        assert_eq!(p.steps[0], Step::Filter("col(\"volume\").gt(lit(0))".into()));
        let rs = generate(src).unwrap();
        assert!(rs.contains(".with_column((col(\"close\") .pct_change(lit(1))).alias(\"ret\"))"), "{rs}");
        assert!(parse("x = col(\"a\"").unwrap_err().to_string().contains("not closed"));
        assert!(parse("where col(\"a\")").unwrap_err().to_string().contains("no columns"));
        assert!(parse("id = lit(1)").unwrap_err().to_string().contains("reserved"));
        assert_eq!(clean("pl.col(\"pl.x\") + opl.y"), "col(\"pl.x\") + opl.y");
    }
}
