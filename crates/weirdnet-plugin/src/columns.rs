//! Computed columns to a result view (and writes): what generated code spaces (math,
//! Polars, Python) end with, and handy in Rust ones.
use crate::{Frame, Host, Json, Result, View};

/// How to show computed columns.
#[derive(Debug, Clone, Default)]
pub struct Output {
    pub title: String,
    /// `chart` (default), `table` or `value` (the last value of each column, per group).
    pub view: String,
    /// The x axis: a field or computed column; empty: `timestamp` if the input has it, else `row`.
    pub x: String,
    /// Columns to show, in order; empty: all computed ones.
    pub show: Vec<String>,
    /// (field, column): write a computed column into a field of the input records.
    pub updates: Vec<(String, String)>,
}

/// Most tiles a grouped chart gets.
pub const MAX_GROUP_TILES: usize = 12;

fn key_text(k: &[(String, Json)]) -> String {
    k.iter().map(|(_, v)| v.text()).collect::<Vec<_>>().join(" ")
}

fn num(v: f64) -> Json {
    if v.is_finite() {
        Json::Num(v)
    } else {
        Json::Null
    }
}

/// Write the updates, then build the view. `cols` are full columns, row for row with `frame`.
pub fn finish(host: &Host, frame: &Frame, out: &Output, cols: Vec<(String, Vec<f64>)>) -> Result<View> {
    let col = |name: &str| -> Result<Vec<f64>> {
        match cols.iter().find(|(n, _)| n == name) {
            Some((_, v)) => Ok(v.clone()),
            None => frame.f64(name),
        }
    };
    if !out.updates.is_empty() {
        let mut vals = vec![];
        for (field, c) in &out.updates {
            vals.push((field.clone(), col(c)?));
        }
        let ups: Vec<(String, Json)> =
            frame.ids.iter().enumerate().map(|(i, id)| (id.clone(), vals.iter().fold(Json::obj(), |o, (f, v)| o.with(f, num(v[i]))))).collect();
        if !ups.is_empty() {
            let n = host.update_each(&frame.model, ups)?;
            host.log(format!("updated {n} {} records", frame.model));
        }
    }
    let show: Vec<String> = if out.show.is_empty() { cols.iter().map(|(n, _)| n.clone()).collect() } else { out.show.clone() };
    let mut shown = vec![];
    for s in &show {
        shown.push((s.clone(), col(s)?));
    }
    let x = if !out.x.is_empty() {
        out.x.clone()
    } else if frame.has("timestamp") {
        "timestamp".into()
    } else {
        "row".into()
    };
    let time = x == "_created_at" || x == "_updated_at" || frame.fields.iter().any(|f| f.name == x && f.ty == "timestamp");
    let xs = col(&x)?;
    let groups = frame.groups();
    let gfields: Vec<String> = groups.first().map(|g| g.key.iter().map(|(k, _)| k.clone()).collect()).unwrap_or_default();
    let title = if out.title.is_empty() { show.join(", ") } else { out.title.clone() };
    match out.view.as_str() {
        "table" => {
            let mut head: Vec<&str> = gfields.iter().map(String::as_str).collect();
            head.push(&x);
            head.extend(show.iter().map(String::as_str));
            let mut v = View::table(&title, &head);
            for g in &groups {
                for i in g.start..g.start + g.len {
                    let mut r: Vec<Json> = g.key.iter().map(|(_, k)| k.clone()).collect();
                    r.push(num(xs[i]));
                    r.extend(shown.iter().map(|(_, c)| num(c[i])));
                    v = v.row(r);
                }
            }
            Ok(v)
        }
        "value" | "values" => {
            let mut head: Vec<&str> = gfields.iter().map(String::as_str).collect();
            head.extend(show.iter().map(String::as_str));
            let mut v = View::table(&title, &head);
            for g in &groups {
                let mut r: Vec<Json> = g.key.iter().map(|(_, k)| k.clone()).collect();
                for (_, c) in &shown {
                    let last = c[g.start..g.start + g.len].iter().rev().find(|x| x.is_finite()).copied().unwrap_or(f64::NAN);
                    r.push(num(last));
                }
                v = v.row(r);
            }
            Ok(v)
        }
        _ => {
            let chart = |t: &str, a: usize, n: usize| {
                shown.iter().fold(View::chart(t).x(&x, xs[a..a + n].to_vec(), time), |v, (name, c)| v.line(name, c[a..a + n].to_vec()))
            };
            if groups.len() <= 1 {
                return Ok(chart(&title, 0, frame.len()));
            }
            if groups.len() > MAX_GROUP_TILES {
                host.log(format!("{} groups: charting the first {MAX_GROUP_TILES}", groups.len()));
            }
            let tiles = groups.iter().take(MAX_GROUP_TILES).map(|g| chart(&key_text(&g.key), g.start, g.len)).collect();
            Ok(View::tiles(&title, true, tiles))
        }
    }
}
