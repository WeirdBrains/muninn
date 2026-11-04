// parse-csv: raw text (a `text` column, as standard input arrives) into rows. The first
// line names the columns; quoted cells may hold commas and "" quotes; numbers stay numbers.
// A built-in parser: copy it (`mnnn code create my-csv --from parse-csv`) and change it.
use muninn_plugin::prelude::*;

fn cells(line: &str) -> Vec<String> {
    let (mut out, mut cur, mut quoted) = (vec![], String::new(), false);
    let mut it = line.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '"' if quoted && it.peek() == Some(&'"') => {
                cur.push('"');
                it.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut cur)),
            c => cur.push(c),
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

pub fn run(host: &Host) -> Result<View> {
    let input = host.input()?;
    let text: String = input.text("text")?.join("\n");
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let head = cells(lines.next().ok_or_else(|| Error::new("no header line"))?);
    let names: Vec<&str> = head.iter().map(|h| h.trim()).collect();
    let mut view = View::table("CSV", &names);
    let mut n = 0;
    for l in lines {
        let c = cells(l);
        view = view.row((0..names.len()).map(|i| c.get(i).map(|x| value(x)).unwrap_or(Json::Null)).collect());
        n += 1;
    }
    host.log(format!("{n} rows, {} columns", names.len()));
    Ok(view)
}
