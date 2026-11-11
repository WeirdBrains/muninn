//! JSON describing a table or a chart (the forms are in the crate docs).
use crate::{CHART_HEIGHT, CHART_WIDTH, Chart, Drawing, Error, Limits, Result, Series, Table, render};
use serde_json::{Map, Value};

fn bad<T>(m: impl Into<String>) -> Result<T> {
    Err(Error::Invalid(m.into()))
}

/// A JSON value as cell text: strings as they are, null as nothing, numbers and booleans as
/// written, anything nested as compact JSON.
pub(crate) fn cell(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        o => o.to_string(),
    }
}

fn rows_of(rows: &[Value], n: usize, l: &Limits) -> Result<Vec<Vec<String>>> {
    if rows.len() > l.max_rows {
        return Err(Error::TooLarge(format!("{} rows (at most {})", rows.len(), l.max_rows)));
    }
    rows.iter()
        .enumerate()
        .map(|(i, r)| match r {
            Value::Array(cells) if cells.len() == n => Ok(cells.iter().map(cell).collect()),
            Value::Array(cells) => bad(format!("row {} has {} cells for {n} columns", i + 1, cells.len())),
            _ => bad(format!("row {} is not a list of cells", i + 1)),
        })
        .collect()
}

fn numbers(v: Option<&Value>, what: &str, l: &Limits) -> Result<Vec<Option<f64>>> {
    let Some(v) = v else { return Ok(vec![]) };
    let Value::Array(a) = v else { return bad(format!("{what} is not a list of numbers")) };
    if a.len() > l.max_points {
        return Err(Error::TooLarge(format!("{} points (at most {})", a.len(), l.max_points)));
    }
    a.iter()
        .enumerate()
        .map(|(i, v)| match v {
            Value::Null => Ok(None),
            Value::Number(n) => Ok(n.as_f64()),
            _ => bad(format!("{what}: value {} is not a number or null", i + 1)),
        })
        .collect()
}

fn size(o: &Map<String, Value>, key: &str, default: usize) -> Result<usize> {
    match o.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => match v.as_u64() {
            Some(n) if n > 0 => Ok(usize::try_from(n).unwrap_or(usize::MAX)),
            _ => bad(format!("{key} is not a positive whole number")),
        },
    }
}

fn chart(o: &Map<String, Value>, l: &Limits) -> Result<Drawing> {
    let Some(Value::Array(ss)) = o.get("series") else { return bad("series is not a list") };
    if ss.len() > l.max_series {
        return Err(Error::TooLarge(format!("{} series (at most {})", ss.len(), l.max_series)));
    }
    let series = ss
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let Value::Object(s) = s else { return bad(format!("series {} is not an object", i + 1)) };
            let name = match s.get("name") {
                None | Some(Value::Null) => String::new(),
                Some(Value::String(n)) => n.clone(),
                Some(_) => return bad(format!("series {}: name is not text", i + 1)),
            };
            let values = numbers(Some(s.get("values").unwrap_or(&Value::Null)).filter(|v| !v.is_null()), &format!("series {:?}", name), l)?;
            Ok(Series { name, values })
        })
        .collect::<Result<Vec<_>>>()?;
    let x = match o.get("x") {
        None | Some(Value::Null) => (0..series.iter().map(|s| s.values.len()).max().unwrap_or(0)).map(|i| Some(i as f64)).collect(),
        v => numbers(v, "x", l)?,
    };
    Ok(Drawing::Chart { chart: Chart { x, series }, width: size(o, "width", CHART_WIDTH)?, height: size(o, "height", CHART_HEIGHT)? })
}

pub(crate) fn drawing(v: &Value, l: &Limits) -> Result<Drawing> {
    match v {
        Value::Object(o) if o.contains_key("columns") => {
            let Some(Value::Array(cols)) = o.get("columns") else { return bad("columns is not a list") };
            let columns = cols.iter().map(|c| c.as_str().map(str::to_string).ok_or(())).collect::<std::result::Result<Vec<_>, _>>();
            let Ok(columns) = columns else { return bad("columns are not all text") };
            if columns.len() > l.max_columns {
                return Err(Error::TooLarge(format!("{} columns (at most {})", columns.len(), l.max_columns)));
            }
            let rows = match o.get("rows") {
                None | Some(Value::Null) => vec![],
                Some(Value::Array(r)) => rows_of(r, columns.len(), l)?,
                Some(_) => return bad("rows is not a list"),
            };
            Ok(Drawing::Table(Table { columns, rows }))
        }
        Value::Object(o) if o.contains_key("series") => chart(o, l),
        Value::Array(items) if !items.is_empty() && items.iter().all(Value::is_object) => {
            if items.len() > l.max_rows {
                return Err(Error::TooLarge(format!("{} rows (at most {})", items.len(), l.max_rows)));
            }
            let mut columns: Vec<String> = items.iter().flat_map(|i| i.as_object().into_iter().flat_map(|o| o.keys().cloned())).collect();
            columns.sort();
            columns.dedup();
            if columns.len() > l.max_columns {
                return Err(Error::TooLarge(format!("{} columns (at most {})", columns.len(), l.max_columns)));
            }
            let rows = items.iter().map(|i| columns.iter().map(|c| i.get(c).map(cell).unwrap_or_default()).collect()).collect();
            Ok(Drawing::Table(Table { columns, rows }))
        }
        Value::Array(items) if !items.is_empty() && items.iter().all(Value::is_array) => {
            let n = items[0].as_array().map_or(0, Vec::len);
            if n > l.max_columns {
                return Err(Error::TooLarge(format!("{n} columns (at most {})", l.max_columns)));
            }
            let rows = rows_of(items, n, l)?;
            Ok(Drawing::Table(Table { columns: (1..=n).map(|i| i.to_string()).collect(), rows }))
        }
        _ => bad("expected a table (columns and rows, a list of objects or a list of rows) or a chart (series)"),
    }
}

/// Read JSON text (at most `max_json_bytes`) as a table or chart and draw it.
pub fn json_png(text: &str, limits: &Limits) -> Result<Vec<u8>> {
    if text.len() > limits.max_json_bytes {
        return Err(Error::TooLarge(format!("{} bytes of JSON (at most {})", text.len(), limits.max_json_bytes)));
    }
    let v: Value = serde_json::from_str(text).map_err(|e| Error::Invalid(format!("not JSON: {e}")))?;
    render(&drawing(&v, limits)?, limits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{chart_png, table_png};
    use serde_json::json;

    fn d(v: Value) -> Result<Drawing> {
        drawing(&v, &Limits::default())
    }

    fn table(cols: &[&str], rows: &[&[&str]]) -> Drawing {
        Drawing::Table(Table {
            columns: cols.iter().map(|s| s.to_string()).collect(),
            rows: rows.iter().map(|r| r.iter().map(|s| s.to_string()).collect()).collect(),
        })
    }

    #[test]
    fn json_tables_in_three_shapes() {
        assert_eq!(
            d(json!({"columns": ["a", "b"], "rows": [[1.5, "x"], [null, true]]})).unwrap(),
            table(&["a", "b"], &[&["1.5", "x"], &["", "true"]])
        );
        assert_eq!(d(json!({"columns": ["a"]})).unwrap(), table(&["a"], &[]));
        assert_eq!(d(json!([{"b": 1, "a": "x"}, {"c": [1, 2]}])).unwrap(), table(&["a", "b", "c"], &[&["x", "1", ""], &["", "", "[1,2]"]]));
        assert_eq!(d(json!([[1, 2], [3, 4]])).unwrap(), table(&["1", "2"], &[&["1", "2"], &["3", "4"]]));
    }

    #[test]
    fn json_charts() {
        let Drawing::Chart { chart, width, height } = d(json!({"series": [{"name": "t", "values": [1, null, 3]}]})).unwrap() else { panic!() };
        assert_eq!((width, height), (CHART_WIDTH, CHART_HEIGHT));
        assert_eq!(chart.x, vec![Some(0.0), Some(1.0), Some(2.0)]);
        assert_eq!(chart.series, vec![Series { name: "t".into(), values: vec![Some(1.0), None, Some(3.0)] }]);
        let Drawing::Chart { chart, width, height } = d(json!({"x": [10, 20], "series": [], "width": 64, "height": 32})).unwrap() else { panic!() };
        assert_eq!((chart.x, width, height), (vec![Some(10.0), Some(20.0)], 64, 32));
    }

    #[test]
    fn json_draws_the_same_as_the_data() {
        let l = Limits::default();
        let t = Table { columns: vec!["a".into()], rows: vec![vec!["1".into()]] };
        assert_eq!(json_png(r#"{"columns":["a"],"rows":[[1]]}"#, &l).unwrap(), table_png(&t, &l).unwrap());
        let c = Chart { x: vec![Some(0.0), Some(1.0)], series: vec![Series { name: "s".into(), values: vec![Some(1.0), Some(2.0)] }] };
        assert_eq!(
            json_png(r#"{"x":[0,1],"series":[{"name":"s","values":[1,2]}],"width":40,"height":30}"#, &l).unwrap(),
            chart_png(&c, 40, 30, &l).unwrap()
        );
    }

    #[test]
    fn malformed_json_is_an_error() {
        let l = Limits::default();
        let inv = |s: &str| matches!(json_png(s, &l), Err(Error::Invalid(_)));
        assert!(inv("{"), "not JSON");
        assert!(inv("42") && inv("\"x\"") && inv("[]") && inv("{}") && inv("[1, 2]") && inv("[{}, 1]"));
        assert!(inv(r#"{"columns": "a"}"#) && inv(r#"{"columns": [1]}"#) && inv(r#"{"columns": ["a"], "rows": [1]}"#));
        assert!(inv(r#"{"columns": ["a"], "rows": [[1, 2]]}"#) && inv(r#"{"columns": ["a"], "rows": {}}"#));
        assert!(inv(r#"[[1, 2], [3]]"#));
        assert!(inv(r#"{"series": 1}"#) && inv(r#"{"series": [1]}"#) && inv(r#"{"series": [{"name": 1}]}"#));
        assert!(inv(r#"{"series": [{"values": ["x"]}]}"#) && inv(r#"{"x": "a", "series": []}"#));
        assert!(inv(r#"{"series": [], "width": 0}"#) && inv(r#"{"series": [], "width": -1}"#) && inv(r#"{"series": [], "height": 1.5}"#));
        assert!(inv(r#"{"columns": []}"#), "no columns");
        assert_eq!(json_png(r#"{"series": [{"values": [null]}]}"#, &l), Err(Error::Empty));
        // too deep for the JSON parser is still just invalid input, not a crash
        assert!(inv(&format!("{}{}", "[".repeat(10_000), "]".repeat(10_000))));
    }

    #[test]
    fn json_limits() {
        let l = Limits::default();
        let big = |s: &str, l: &Limits| matches!(json_png(s, l), Err(Error::TooLarge(_)));
        assert!(big(r#"{"columns":["a"]}"#, &Limits { max_json_bytes: 5, ..l }));
        assert!(big(r#"{"columns":["a","b"]}"#, &Limits { max_columns: 1, ..l }));
        assert!(big(r#"[{"a":1,"b":2}]"#, &Limits { max_columns: 1, ..l }));
        assert!(big(r#"[[1,2]]"#, &Limits { max_columns: 1, ..l }));
        assert!(big(r#"{"columns":["a"],"rows":[[1],[2]]}"#, &Limits { max_rows: 1, ..l }));
        assert!(big(r#"[{"a":1},{"a":2}]"#, &Limits { max_rows: 1, ..l }));
        assert!(big(r#"{"series":[{"values":[1,2,3]}]}"#, &Limits { max_points: 2, ..l }));
        assert!(big(r#"{"series":[{"values":[1]},{"values":[2]}]}"#, &Limits { max_series: 1, ..l }));
        assert!(big(r#"{"series":[{"values":[1]}],"width":100000}"#, &l));
        assert!(big(r#"{"series":[{"values":[1]}],"width":5000,"height":4000}"#, &l), "over the pixel limit");
    }
}
