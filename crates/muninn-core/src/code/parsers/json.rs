// parse-json: raw text holding JSON (an array of objects, one object, or a document with
// the rows somewhere inside) into rows, one column per key in the order keys first appear.
// Each `text` cell is parsed on its own (one file each with `source:NAME`). Rows that are
// arrays (`[1502942400000, "4261.48", …]`) become column_1, column_2, ….
//
// Options (`code:parse-json?records=data.items`):
//   records  where the rows live: a dotted path (`data.items`) or a JSON pointer (`/data/items`)
// A built-in parser: copy it (`mnnn code create my-json --from parse-json`) and change it.
use muninn_plugin::prelude::*;

pub fn rows_view(title: &str, objs: Vec<Json>) -> Result<View> {
    let objs: Vec<Json> = objs
        .into_iter()
        .map(|o| match o {
            Json::Arr(a) => Json::Obj(a.into_iter().enumerate().map(|(i, v)| (format!("column_{}", i + 1), v)).collect()),
            o => o,
        })
        .collect();
    let mut names: Vec<String> = vec![];
    for o in &objs {
        for (k, _) in o.as_object().ok_or_else(|| Error::new("rows must be JSON objects or arrays"))? {
            if !names.contains(k) {
                names.push(k.clone());
            }
        }
    }
    let cols: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut view = View::table(title, &cols);
    for o in &objs {
        view = view.row(names.iter().map(|k| o.get(k).cloned().unwrap_or(Json::Null)).collect());
    }
    Ok(view)
}

fn select(v: Json, path: &str) -> Result<Json> {
    let p = path.trim().trim_start_matches("$.").trim_start_matches('$');
    let segs: Vec<&str> = if let Some(ptr) = p.strip_prefix('/') { ptr.split('/').collect() } else { p.split('.').collect() };
    let mut cur = v;
    for seg in segs.into_iter().filter(|s| !s.is_empty()) {
        cur = match cur {
            Json::Obj(o) => o.into_iter().find(|(k, _)| k == seg).map(|(_, v)| v),
            Json::Arr(a) => seg.parse::<usize>().ok().and_then(|i| a.into_iter().nth(i)),
            _ => None,
        }
        .ok_or_else(|| Error::new(format!("records: `{seg}` not found in the document")))?;
    }
    Ok(cur)
}

pub fn run(host: &Host) -> Result<View> {
    let input = host.input()?;
    let records = host.option("records");
    let mut objs = vec![];
    for text in input.text("text")? {
        let mut v = Json::parse(text.trim()).map_err(Error::new)?;
        if let Some(p) = &records {
            v = select(v, p)?;
        }
        match v {
            Json::Arr(a) => objs.extend(a),
            o @ Json::Obj(_) => objs.push(o),
            _ => return Err(Error::new("expected a JSON array of objects (or give records=PATH)")),
        }
    }
    host.log(format!("{} rows", objs.len()));
    rows_view("JSON", objs)
}
