//! Math code spaces: one formula per line, compiled to Rust over columns.
//!
//! ```text
//! # moving averages on the input bars, per symbol (group the input by symbol)
//! @title SMA cross
//! @show close, fast, slow
//! fast = sma(close, 10)
//! slow = sma(close, 30)
//! cross = if(fast > slow, 1, 0)
//! @update signal = cross
//! ```
//!
//! A name is an input field or an earlier line; every value is a column (one number per
//! record, `NaN` when missing) computed per group of the input. See `docs/code-spaces.md`.
use crate::error::{Error, Result};
use std::collections::BTreeSet;

/// Settings shared by math, Polars and Python sources (`@name value` lines).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Directives {
    pub title: String,
    pub view: String,
    pub x: String,
    pub show: Vec<String>,
    pub updates: Vec<(String, String)>,
}

impl Directives {
    /// Read a directive (`title …`, `view chart`, …) without its `@`.
    pub fn read(&mut self, line: &str) -> std::result::Result<(), String> {
        let (k, v) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let v = v.trim();
        match k.to_ascii_lowercase().as_str() {
            "title" => self.title = v.to_string(),
            "view" => match v {
                "chart" | "table" | "value" => self.view = v.to_string(),
                _ => return Err(format!("@view {v}: use chart, table or value")),
            },
            "x" => {
                ident(v)?;
                self.x = v.to_string()
            }
            "show" => {
                for n in v.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                    ident(n)?;
                    self.show.push(n.to_string());
                }
            }
            "update" => {
                let (f, c) = v.split_once('=').ok_or("@update FIELD = NAME")?;
                let (f, c) = (f.trim(), c.trim());
                ident(f)?;
                ident(c)?;
                self.updates.push((f.to_string(), c.to_string()));
            }
            _ => return Err(format!("unknown @{k} (title, view, x, show, update)")),
        }
        Ok(())
    }

    /// The SDK's `columns::Output` for these settings.
    pub fn output_rs(&self) -> String {
        let list = |v: &[String]| v.iter().map(|s| format!("{s:?}.to_string()")).collect::<Vec<_>>().join(", ");
        let ups = self.updates.iter().map(|(f, c)| format!("({f:?}.to_string(), {c:?}.to_string())")).collect::<Vec<_>>().join(", ");
        format!(
            "muninn_plugin::columns::Output {{ title: {:?}.to_string(), view: {:?}.to_string(), x: {:?}.to_string(), show: vec![{}], updates: vec![{ups}] }}",
            self.title,
            self.view,
            self.x,
            list(&self.show)
        )
    }
}

pub(crate) fn ident(s: &str) -> std::result::Result<(), String> {
    let mut c = s.chars();
    let ok = c.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') && c.all(|c| c.is_ascii_alphanumeric() || c == '_');
    if ok { Ok(()) } else { Err(format!("`{s}` is not a name (letters, digits and _)")) }
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Id(String),
    Op(&'static str),
    L,
    R,
    Comma,
}

fn lex(s: &str) -> std::result::Result<Vec<Tok>, String> {
    let cs: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut out = vec![];
    const OPS: [&str; 18] = ["<=", ">=", "==", "!=", "&&", "||", "+", "-", "*", "/", "%", "^", "<", ">", "!", "=", "&", "|"];
    while i < cs.len() {
        let c = cs[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() || (c == '.' && cs.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let st = i;
            while i < cs.len() && (cs[i].is_ascii_digit() || cs[i] == '.' || cs[i] == '_') {
                i += 1;
            }
            if i < cs.len() && (cs[i] == 'e' || cs[i] == 'E') {
                i += 1;
                if i < cs.len() && (cs[i] == '+' || cs[i] == '-') {
                    i += 1;
                }
                while i < cs.len() && cs[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let t: String = cs[st..i].iter().filter(|c| **c != '_').collect();
            let v: f64 = t.parse().map_err(|_| format!("`{t}` is not a number"))?;
            if !v.is_finite() {
                return Err(format!("`{t}` is too large"));
            }
            out.push(Tok::Num(v));
        } else if c.is_ascii_alphabetic() || c == '_' {
            let st = i;
            let mut dotted = false;
            while i < cs.len() && (cs[i].is_ascii_alphanumeric() || cs[i] == '_') {
                i += 1;
                // `INPUT.field`: a field of a named input (a pipeline's fan-in)
                if !dotted && i + 1 < cs.len() && cs[i] == '.' && (cs[i + 1].is_ascii_alphabetic() || cs[i + 1] == '_') {
                    dotted = true;
                    i += 1;
                }
            }
            out.push(match cs[st..i].iter().collect::<String>().as_str() {
                "and" => Tok::Op("&&"),
                "or" => Tok::Op("||"),
                "not" => Tok::Op("!"),
                w => Tok::Id(w.to_string()),
            });
        } else if c == '(' {
            out.push(Tok::L);
            i += 1;
        } else if c == ')' {
            out.push(Tok::R);
            i += 1;
        } else if c == ',' {
            out.push(Tok::Comma);
            i += 1;
        } else {
            let two: String = cs[i..(i + 2).min(cs.len())].iter().collect();
            let op = OPS.iter().find(|o| two.starts_with(**o)).ok_or_else(|| format!("unexpected `{c}`"))?;
            i += op.len();
            out.push(Tok::Op(match *op {
                "=" => "==",
                "&" => "&&",
                "|" => "||",
                o => o,
            }));
        }
    }
    Ok(out)
}

/// An expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Num(f64),
    Name(String),
    Neg(Box<Expr>),
    Not(Box<Expr>),
    Bin(&'static str, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
}

struct P {
    t: Vec<Tok>,
    i: usize,
}

fn prec(op: &str) -> Option<(u8, bool)> {
    Some(match op {
        "||" => (1, false),
        "&&" => (2, false),
        "<" | "<=" | ">" | ">=" | "==" | "!=" => (4, false),
        "+" | "-" => (5, false),
        "*" | "/" | "%" => (6, false),
        "^" => (8, true),
        _ => return None,
    })
}

impl P {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.t.get(self.i).cloned();
        self.i += 1;
        t
    }
    fn expr(&mut self, min: u8) -> std::result::Result<Expr, String> {
        let mut lhs = match self.next() {
            Some(Tok::Num(n)) => Expr::Num(n),
            Some(Tok::Op("-")) => Expr::Neg(Box::new(self.expr(7)?)),
            Some(Tok::Op("+")) => self.expr(7)?,
            Some(Tok::Op("!")) => Expr::Not(Box::new(self.expr(3)?)),
            Some(Tok::L) => {
                let e = self.expr(0)?;
                if self.next() != Some(Tok::R) {
                    return Err("missing `)`".into());
                }
                e
            }
            Some(Tok::Id(n)) => {
                if self.peek() == Some(&Tok::L) {
                    self.i += 1;
                    let mut args = vec![];
                    if self.peek() == Some(&Tok::R) {
                        self.i += 1;
                    } else {
                        loop {
                            args.push(self.expr(0)?);
                            match self.next() {
                                Some(Tok::Comma) => {}
                                Some(Tok::R) => break,
                                _ => return Err(format!("{n}(…): missing `)` or `,`")),
                            }
                        }
                    }
                    Expr::Call(n, args)
                } else {
                    Expr::Name(n)
                }
            }
            Some(t) => return Err(format!("unexpected {}", tok_text(&t))),
            None => return Err("the formula ends too soon".into()),
        };
        while let Some(Tok::Op(op)) = self.peek().cloned() {
            let Some((p, right)) = prec(op) else { return Err(format!("unexpected `{op}`")) };
            if p < min {
                break;
            }
            self.i += 1;
            let rhs = self.expr(if right { p } else { p + 1 })?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }
}

fn tok_text(t: &Tok) -> String {
    match t {
        Tok::Num(n) => n.to_string(),
        Tok::Id(s) => format!("`{s}`"),
        Tok::Op(o) => format!("`{o}`"),
        Tok::L => "`(`".into(),
        Tok::R => "`)`".into(),
        Tok::Comma => "`,`".into(),
    }
}

pub fn parse_expr(s: &str) -> std::result::Result<Expr, String> {
    let mut p = P { t: lex(s)?, i: 0 };
    let e = p.expr(0)?;
    if let Some(t) = p.peek() {
        return Err(format!("unexpected {} after the formula", tok_text(t)));
    }
    Ok(e)
}

/// Functions: (name, arguments, Rust over columns with `{0}`, `{1}`, `{2}` as `&[f64]`).
const FUNCS: &[(&str, usize, &str)] = &[
    ("sma", 2, "s::sma({0}, {1})"),
    ("ema", 2, "s::ema({0}, {1})"),
    ("rolling_sum", 2, "s::rolling_sum({0}, {1})"),
    ("rolling_min", 2, "s::rolling_min({0}, {1})"),
    ("rolling_max", 2, "s::rolling_max({0}, {1})"),
    ("rolling_std", 2, "s::rolling_std({0}, {1})"),
    ("lag", 2, "s::lag({0}, {1})"),
    ("lead", 2, "s::lead({0}, {1})"),
    ("diff", 1, "s::diff({0})"),
    ("pct_change", 1, "s::pct_change({0})"),
    ("cumsum", 1, "s::cumsum({0})"),
    ("zscore", 1, "s::zscore({0})"),
    ("sum", 1, "s::sum({0})"),
    ("mean", 1, "s::mean({0})"),
    ("avg", 1, "s::mean({0})"),
    ("min", 1, "s::min({0})"),
    ("max", 1, "s::max({0})"),
    ("std", 1, "s::std({0})"),
    ("count", 1, "s::count({0})"),
    ("first", 1, "s::first({0})"),
    ("last", 1, "s::last({0})"),
    ("min", 2, "s::zip({0}, {1}, f64::min)"),
    ("max", 2, "s::zip({0}, {1}, f64::max)"),
    ("pow", 2, "s::zip({0}, {1}, f64::powf)"),
    ("coalesce", 2, "s::zip({0}, {1}, |x, y| if x.is_nan() { y } else { x })"),
    ("if", 3, "s::choose({0}, {1}, {2})"),
    ("abs", 1, "s::map({0}, f64::abs)"),
    ("sqrt", 1, "s::map({0}, f64::sqrt)"),
    ("ln", 1, "s::map({0}, f64::ln)"),
    ("log", 1, "s::map({0}, f64::ln)"),
    ("log10", 1, "s::map({0}, f64::log10)"),
    ("exp", 1, "s::map({0}, f64::exp)"),
    ("round", 1, "s::map({0}, f64::round)"),
    ("floor", 1, "s::map({0}, f64::floor)"),
    ("ceil", 1, "s::map({0}, f64::ceil)"),
    ("sign", 1, "s::map({0}, |x| if x.is_nan() || x == 0.0 { x } else { x.signum() })"),
    ("missing", 1, "s::map({0}, |x| s::flag(x.is_nan()))"),
];

/// The function names (for help).
pub fn functions() -> Vec<String> {
    let mut v: Vec<String> = FUNCS.iter().map(|(n, a, _)| format!("{n}/{a}")).collect();
    v.dedup();
    v
}

/// A parsed math source.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Program {
    /// (name, formula) in order.
    pub lines: Vec<(String, Expr)>,
    pub dirs: Directives,
}

impl Program {
    /// Input fields the formulas read (names that are not lines).
    pub fn fields(&self) -> BTreeSet<String> {
        let mut defined = BTreeSet::new();
        let mut used = BTreeSet::new();
        for (n, e) in &self.lines {
            names(e, &defined, &mut used);
            defined.insert(n.clone());
        }
        let mut more = vec![self.dirs.x.clone()];
        more.extend(self.dirs.show.iter().cloned());
        more.extend(self.dirs.updates.iter().map(|(_, c)| c.clone()));
        for m in more.into_iter().filter(|m| !m.is_empty() && !defined.contains(m)) {
            used.insert(m);
        }
        used
    }
}

fn names(e: &Expr, defined: &BTreeSet<String>, out: &mut BTreeSet<String>) {
    match e {
        Expr::Num(_) => {}
        Expr::Name(n) => {
            if !defined.contains(n) {
                out.insert(n.clone());
            }
        }
        Expr::Neg(a) | Expr::Not(a) => names(a, defined, out),
        Expr::Bin(_, a, b) => {
            names(a, defined, out);
            names(b, defined, out);
        }
        Expr::Call(_, args) => args.iter().for_each(|a| names(a, defined, out)),
    }
}

/// Parse a math source (lines `name = formula`, `@directive …`, `# comment`).
pub fn parse(src: &str) -> Result<Program> {
    let mut p = Program::default();
    let mut defined = BTreeSet::new();
    for (i, raw) in src.lines().enumerate() {
        let line = raw.split_once('#').map(|(a, _)| a).unwrap_or(raw).trim();
        let err = |m: String| Error::invalid("math", format!("line {}: {m}", i + 1));
        if line.is_empty() {
            continue;
        }
        if let Some(d) = line.strip_prefix('@') {
            p.dirs.read(d).map_err(err)?;
            continue;
        }
        let Some((n, f)) = line.split_once('=').filter(|(_, f)| !f.starts_with('=')) else {
            return Err(err("write NAME = FORMULA (or @title, @view, @x, @show, @update)".into()));
        };
        let n = n.trim();
        ident(n).map_err(err)?;
        if FUNCS.iter().any(|(f, _, _)| *f == n) {
            return Err(err(format!("`{n}` is a function; pick another name")));
        }
        if defined.contains(n) {
            return Err(err(format!("`{n}` is already defined")));
        }
        let e = parse_expr(f).map_err(err)?;
        check(&e).map_err(err)?;
        defined.insert(n.to_string());
        p.lines.push((n.to_string(), e));
    }
    if p.lines.is_empty() {
        return Err(Error::invalid("math", "no formulas: write NAME = FORMULA lines, e.g. `fast = sma(close, 10)`"));
    }
    for (_, c) in &p.dirs.updates {
        if !defined.contains(c) {
            return Err(Error::invalid("math", format!("@update: `{c}` is not one of the formulas")));
        }
    }
    Ok(p)
}

fn check(e: &Expr) -> std::result::Result<(), String> {
    match e {
        Expr::Call(n, args) => {
            if !FUNCS.iter().any(|(f, a, _)| f == n && *a == args.len()) {
                let arities: Vec<String> = FUNCS.iter().filter(|(f, _, _)| f == n).map(|(_, a, _)| a.to_string()).collect();
                return Err(if arities.is_empty() {
                    format!("unknown function `{n}` (see `mnnn code help math`)")
                } else {
                    format!("{n} takes {} argument(s), not {}", arities.join(" or "), args.len())
                });
            }
            args.iter().try_for_each(check)
        }
        Expr::Neg(a) | Expr::Not(a) => check(a),
        Expr::Bin(_, a, b) => check(a).and(check(b)),
        _ => Ok(()),
    }
}

fn rs(e: &Expr, defined: &BTreeSet<String>) -> String {
    let r = |e: &Expr| format!("&{}", rs(e, defined));
    match e {
        Expr::Num(n) => format!("s::k({n:?}, n)"),
        Expr::Name(x) if defined.contains(x) => format!("v_{x}.clone()"),
        Expr::Name(x) => format!("f_{}.clone()", var(x)),
        Expr::Neg(a) => format!("s::map({}, |x| -x)", r(a)),
        Expr::Not(a) => format!("s::map({}, |x| s::flag(!s::truthy(x)))", r(a)),
        Expr::Bin(op, a, b) => {
            let f = match *op {
                "+" | "-" | "*" | "/" | "%" => format!("|x, y| x {op} y"),
                "^" => "f64::powf".into(),
                "&&" => "|x, y| s::flag(s::truthy(x) && s::truthy(y))".into(),
                "||" => "|x, y| s::flag(s::truthy(x) || s::truthy(y))".into(),
                cmp => format!("|x, y| s::flag(x {cmp} y)"),
            };
            format!("s::zip({}, {}, {f})", r(a), r(b))
        }
        Expr::Call(n, args) => {
            let tpl = FUNCS.iter().find(|(f, a, _)| f == n && *a == args.len()).map(|x| x.2).unwrap_or("s::k(f64::NAN, n)");
            let mut out = tpl.to_string();
            for (i, a) in args.iter().enumerate() {
                out = out.replace(&format!("{{{i}}}"), &r(a));
            }
            out
        }
    }
}

/// A name as a Rust identifier (`slow.close` → `slow__close`).
fn var(x: &str) -> String {
    x.replace('.', "__")
}

/// The Rust `user.rs` for a math source.
pub fn generate(src: &str) -> Result<String> {
    let p = parse(src)?;
    let mut defined = BTreeSet::new();
    let mut body = String::new();
    let mut named = String::new();
    for f in p.fields() {
        match f.split_once('.') {
            // a named input's column, row by row beside the first input's rows (NaN past its end)
            Some((input, field)) => {
                let v = var(&f);
                named += &format!("    let n_{v} = host.input_named({input:?})?.f64({field:?})?;\n");
                body += &format!(
                    "        let f_{v}: Vec<f64> = (grp.start..grp.start + n).map(|i| n_{v}.get(i).copied().unwrap_or(f64::NAN)).collect();\n"
                );
            }
            None => body += &format!("        let f_{f} = g.f64({f:?})?;\n"),
        }
    }
    for (i, (n, e)) in p.lines.iter().enumerate() {
        body += &format!("        let v_{n}: Vec<f64> = {};\n", rs(e, &defined));
        body += &format!("        cols[{i}].1.extend_from_slice(&v_{n});\n");
        defined.insert(n.clone());
    }
    let names: Vec<String> = p.lines.iter().map(|(n, _)| format!("{n:?}")).collect();
    Ok(format!(
        r#"// Generated by Muninn from a math code space. Do not edit.
#![allow(unused, clippy::all)]
use muninn_plugin::prelude::*;
use muninn_plugin::series as s;

const NAMES: &[&str] = &[{names}];

pub fn run(host: &Host) -> Result<View> {{
    let frame = host.input()?;
{named}    let mut cols: Vec<(String, Vec<f64>)> = NAMES.iter().map(|n| (n.to_string(), Vec::with_capacity(frame.len()))).collect();
    for grp in frame.groups() {{
        let g = frame.slice(grp.start, grp.len);
        let n = g.len();
{body}    }}
    muninn_plugin::columns::finish(host, &frame, &{out}, cols)
}}
"#,
        names = names.join(", "),
        named = named,
        out = p.dirs.output_rs()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formulas_parse_with_precedence() {
        let e = parse_expr("a + b * c ^ 2 ^ 1 > 3 and not d").unwrap();
        let Expr::Bin("&&", l, r) = e else { panic!() };
        assert!(matches!(*r, Expr::Not(_)));
        let Expr::Bin(">", sum, _) = *l else { panic!() };
        let Expr::Bin("+", _, prod) = *sum else { panic!() };
        let Expr::Bin("*", _, pow) = *prod else { panic!() };
        assert!(matches!(*pow, Expr::Bin("^", _, ref rhs) if matches!(**rhs, Expr::Bin("^", _, _))), "^ is right-associative");
        assert_eq!(parse_expr("-x").unwrap(), Expr::Neg(Box::new(Expr::Name("x".into()))));
        assert_eq!(parse_expr("1e3").unwrap(), Expr::Num(1000.0));
        assert!(parse_expr("a +").is_err() && parse_expr("(a").is_err() && parse_expr("a b").is_err() && parse_expr("a $ b").is_err());
    }

    #[test]
    fn programs_check_and_generate() {
        let src = "# test\n@title T\n@view table\nfast = sma(close, 3)\nup = if(fast > close, 1, 0) # note\n@update signal = up\n@show close, fast";
        let p = parse(src).unwrap();
        assert_eq!(p.lines.len(), 2);
        assert_eq!(p.fields().into_iter().collect::<Vec<_>>(), vec!["close".to_string()]);
        assert_eq!((p.dirs.title.as_str(), p.dirs.view.as_str(), p.dirs.updates.len()), ("T", "table", 1));
        let rs = generate(src).unwrap();
        assert!(rs.contains("let f_close = g.f64(\"close\")?;") && rs.contains("s::sma(&f_close.clone(), &s::k(3.0, n))"), "{rs}");
        let bad = |s: &str| parse(s).unwrap_err().to_string();
        assert!(bad("x = nope(1)").contains("unknown function"));
        assert!(bad("x = sma(close)").contains("takes 2"));
        assert!(bad("\n\nx 1").contains("line 3"));
        assert!(bad("sma = 1").contains("is a function"));
        assert!(bad("@update a = b\nx = 1").contains("not one of the formulas"));
        assert!(bad("@view pie\nx = 1").contains("@view"));
        assert!(bad("# nothing").contains("no formulas"));
    }

    #[test]
    fn named_inputs_are_read_by_dotted_names() {
        let rs = generate("spread = fast.close - slow.close\nboth = spread + close").unwrap();
        assert!(rs.contains("let n_fast__close = host.input_named(\"fast\")?.f64(\"close\")?;"), "{rs}");
        assert!(rs.contains("let f_slow__close: Vec<f64>") && rs.contains("let f_close = g.f64(\"close\")?;"), "{rs}");
        assert!(rs.contains("s::zip(&f_fast__close.clone(), &f_slow__close.clone()"), "{rs}");
        assert!(parse_expr("a.b.c").is_err() && parse_expr("a.").is_err());
    }
}
