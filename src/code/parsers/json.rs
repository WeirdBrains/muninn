// parse-json: raw text holding a JSON array of objects (or one object) into rows, one
// column per key in the order keys first appear.
// A built-in parser: copy it (`mnnn code create my-json --from parse-json`) and change it.
use muninn_plugin::prelude::*;

pub fn rows_view(title: &str, objs: Vec<Json>) -> Result<View> {
    let mut names: Vec<String> = vec![];
    for o in &objs {
        for (k, _) in o.as_object().ok_or_else(|| Error::new("rows must be JSON objects"))? {
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

pub fn run(host: &Host) -> Result<View> {
    let input = host.input()?;
    let text: String = input.text("text")?.join("\n");
    let v = Json::parse(text.trim()).map_err(Error::new)?;
    let objs = match v {
        Json::Arr(a) => a,
        o @ Json::Obj(_) => vec![o],
        _ => return Err(Error::new("expected a JSON array of objects")),
    };
    host.log(format!("{} rows", objs.len()));
    rows_view("JSON", objs)
}
