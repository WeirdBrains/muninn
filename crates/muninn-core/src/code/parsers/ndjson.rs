// parse-ndjson: raw text with one JSON object a line (NDJSON, JSON lines) into rows.
// A built-in parser: copy it (`mnnn code create my-ndjson --from parse-ndjson`) and change it.
use muninn_plugin::prelude::*;

pub fn run(host: &Host) -> Result<View> {
    let input = host.input()?;
    let text: String = input.text("text")?.join("\n");
    let mut objs = vec![];
    for (i, l) in text.lines().enumerate().filter(|(_, l)| !l.trim().is_empty()) {
        objs.push(Json::parse(l.trim()).map_err(|e| Error::new(format!("line {}: {e}", i + 1)))?);
    }
    let mut names: Vec<String> = vec![];
    for o in &objs {
        for (k, _) in o.as_object().ok_or_else(|| Error::new("each line must be a JSON object"))? {
            if !names.contains(k) {
                names.push(k.clone());
            }
        }
    }
    let cols: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut view = View::table("NDJSON", &cols);
    for o in &objs {
        view = view.row(names.iter().map(|k| o.get(k).cloned().unwrap_or(Json::Null)).collect());
    }
    host.log(format!("{} rows", objs.len()));
    Ok(view)
}
