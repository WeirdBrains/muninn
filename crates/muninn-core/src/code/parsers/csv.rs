// parse-csv: raw text (a `text` column: standard input, or a source's stored files with
// `source:NAME`) into rows. Each `text` cell is parsed on its own (one file, one chunk):
// the first line names the columns; quoted cells may hold the delimiter and "" quotes;
// numbers stay numbers. Every row also gets `_name` and `_source` when the input has
// `name` and `source` columns (which file, which source it came from).
//
// Options (`code:parse-csv?delimiter=;&header=false`):
//   delimiter  one character, or `tab` (default `,`)
//   quote      one character (default `"`)
//   header     `false`: no header line; columns are column_1, column_2, … (default true)
//   skip_rows  lines to skip before the header (default 0)
// A built-in parser: copy it (`mnnn code create my-csv --from parse-csv`) and change it.
use muninn_plugin::prelude::*;

fn cells(line: &str, delim: char, quote: char) -> Vec<String> {
    let (mut out, mut cur, mut quoted) = (vec![], String::new(), false);
    let mut it = line.chars().peekable();
    while let Some(c) = it.next() {
        if c == quote && quoted && it.peek() == Some(&quote) {
            cur.push(quote);
            it.next();
        } else if c == quote {
            quoted = !quoted;
        } else if c == delim && !quoted {
            out.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    out.push(cur);
    out
}

fn value(s: &str) -> Json {
    let t = s.trim();
    match t.parse::<f64>() {
        Ok(n) if !t.is_empty() => Json::Num(n),
        _ if t.is_empty() => Json::Null,
        _ => Json::Str(s.to_string()),
    }
}

fn one_char(host: &Host, name: &str, default: char) -> Result<char> {
    match host.option(name) {
        None => Ok(default),
        Some(v) if v.eq_ignore_ascii_case("tab") || v == "\\t" => Ok('\t'),
        Some(v) if v.chars().count() == 1 => Ok(v.chars().next().unwrap_or(default)),
        Some(v) => Err(Error::new(format!("option {name}: `{v}` is not one character"))),
    }
}

pub fn run(host: &Host) -> Result<View> {
    let input = host.input()?;
    let delim = one_char(host, "delimiter", ',')?;
    let quote = one_char(host, "quote", '"')?;
    let header = host.option("header").map(|v| !matches!(v.to_ascii_lowercase().as_str(), "false" | "no" | "0")).unwrap_or(true);
    let skip: usize = match host.option("skip_rows") {
        Some(v) => v.trim().parse().map_err(|_| Error::new(format!("option skip_rows: `{v}` is not a number")))?,
        None => 0,
    };
    let texts = input.text("text")?;
    let names_in = if input.has("name") { input.text("name")? } else { vec![] };
    let sources_in = if input.has("source") { input.text("source")? } else { vec![] };
    let mut names: Vec<String> = vec![];
    let mut rows: Vec<Vec<(usize, Json)>> = vec![];
    for (doc, text) in texts.iter().enumerate() {
        let mut lines = text.lines().filter(|l| !l.trim().is_empty()).skip(skip).peekable();
        let head: Vec<String> = if header {
            match lines.next() {
                Some(l) => cells(l, delim, quote).iter().map(|h| h.trim().to_string()).collect(),
                None => continue,
            }
        } else {
            let n = lines.peek().map(|l| cells(l, delim, quote).len()).unwrap_or(0);
            (1..=n).map(|i| format!("column_{i}")).collect()
        };
        let mut at = vec![];
        for h in head.iter().chain(["_name".to_string(), "_source".to_string()].iter()) {
            at.push(match names.iter().position(|n| n == h) {
                Some(i) => i,
                None => {
                    names.push(h.clone());
                    names.len() - 1
                }
            });
        }
        for l in lines {
            let c = cells(l, delim, quote);
            let mut r: Vec<(usize, Json)> = (0..head.len()).map(|i| (at[i], c.get(i).map(|x| value(x)).unwrap_or(Json::Null))).collect();
            if let Some(n) = names_in.get(doc) {
                r.push((at[head.len()], Json::Str(n.clone())));
            }
            if let Some(s) = sources_in.get(doc) {
                r.push((at[head.len() + 1], Json::Str(s.clone())));
            }
            rows.push(r);
        }
    }
    // drop _name/_source when nothing filled them
    let used: Vec<bool> = (0..names.len()).map(|i| names[i].starts_with('_') && rows.iter().any(|r| r.iter().any(|(j, _)| *j == i)) || !names[i].starts_with('_')).collect();
    let keep: Vec<usize> = (0..names.len()).filter(|i| used[*i]).collect();
    let cols: Vec<&str> = keep.iter().map(|i| names[*i].as_str()).collect();
    let mut view = View::table("CSV", &cols);
    for r in &rows {
        view = view.row(keep.iter().map(|i| r.iter().find(|(j, _)| j == i).map(|(_, v)| v.clone()).unwrap_or(Json::Null)).collect());
    }
    host.log(format!("{} rows, {} columns from {} text(s)", rows.len(), cols.len(), texts.len()));
    Ok(view)
}
