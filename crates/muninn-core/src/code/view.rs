//! The result of a run: a view that follows the content view rules. A table shows as a list
//! tile and a chart as a chart tile, and either can switch to the other (`t` in the TUI);
//! tiles split right or below like the workspace. The plugin sends it with the `result`
//! host method (`View::to_json` in the SDK).
use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// At most this many table rows and chart points are kept (more are cut, with a note).
pub const MAX_ROWS: usize = 5_000;
pub const MAX_POINTS: usize = 20_000;
/// At most this many tiles, nested at most this deep.
pub const MAX_TILES: usize = 12;
pub const MAX_DEPTH: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Dir {
    #[default]
    Right,
    Below,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Axis {
    #[serde(default)]
    pub name: String,
    /// Epoch milliseconds.
    #[serde(default)]
    pub time: bool,
    pub values: Vec<Option<f64>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Line {
    pub name: String,
    pub values: Vec<Option<f64>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResultView {
    Table {
        #[serde(default)]
        title: String,
        columns: Vec<String>,
        #[serde(default)]
        rows: Vec<Vec<Value>>,
    },
    Chart {
        #[serde(default)]
        title: String,
        x: Axis,
        #[serde(default)]
        series: Vec<Line>,
    },
    Text {
        #[serde(default)]
        title: String,
        text: String,
    },
    Tiles {
        #[serde(default)]
        title: String,
        #[serde(default)]
        dir: Dir,
        tiles: Vec<ResultView>,
    },
}

fn cell(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Number(n) => n.as_f64().map(crate::export::docs::num).unwrap_or_else(|| n.to_string()),
        o => o.to_string(),
    }
}

impl ResultView {
    /// Parse and check a view sent by a plugin; overlong tables and charts are cut (the
    /// returned notes say so).
    pub fn from_plugin(v: Value) -> Result<(ResultView, Vec<String>)> {
        let mut view: ResultView = serde_json::from_value(v).map_err(|e| Error::invalid("result", format!("not a view: {e}")))?;
        let mut notes = vec![];
        let mut tiles = 0;
        view.check(0, &mut tiles, &mut notes)?;
        Ok((view, notes))
    }

    fn check(&mut self, depth: usize, tiles: &mut usize, notes: &mut Vec<String>) -> Result<()> {
        let bad = |m: String| Err(Error::invalid("result", m));
        match self {
            ResultView::Table { title, columns, rows } => {
                *tiles += 1;
                if columns.is_empty() {
                    return bad(format!("table {title:?} has no columns"));
                }
                if let Some(i) = rows.iter().position(|r| r.len() != columns.len()) {
                    return bad(format!("table {title:?}: row {} has {} values for {} columns", i + 1, rows[i].len(), columns.len()));
                }
                if rows.len() > MAX_ROWS {
                    notes.push(format!("table {title:?}: showing the first {MAX_ROWS} of {} rows", rows.len()));
                    rows.truncate(MAX_ROWS);
                }
            }
            ResultView::Chart { title, x, series } => {
                *tiles += 1;
                if let Some(s) = series.iter().find(|s| s.values.len() != x.values.len()) {
                    return bad(format!("chart {title:?}: line {} has {} points for {} x values", s.name, s.values.len(), x.values.len()));
                }
                if x.values.len() > MAX_POINTS {
                    let cut = x.values.len() - MAX_POINTS;
                    notes.push(format!("chart {title:?}: showing the last {MAX_POINTS} of {} points", x.values.len()));
                    x.values.drain(..cut);
                    for s in series.iter_mut() {
                        s.values.drain(..cut);
                    }
                }
            }
            ResultView::Text { .. } => *tiles += 1,
            ResultView::Tiles { title, tiles: ts, .. } => {
                if depth >= MAX_DEPTH {
                    return bad(format!("tiles {title:?} nest deeper than {MAX_DEPTH}"));
                }
                if ts.is_empty() {
                    return bad(format!("tiles {title:?} is empty"));
                }
                for t in ts.iter_mut() {
                    t.check(depth + 1, tiles, notes)?;
                }
            }
        }
        if *tiles > MAX_TILES {
            return bad(format!("more than {MAX_TILES} tiles"));
        }
        Ok(())
    }

    pub fn title(&self) -> &str {
        match self {
            ResultView::Table { title, .. } | ResultView::Chart { title, .. } | ResultView::Text { title, .. } | ResultView::Tiles { title, .. } => {
                title
            }
        }
    }

    /// The leaf views, in order.
    pub fn leaves(&self) -> Vec<&ResultView> {
        match self {
            ResultView::Tiles { tiles, .. } => tiles.iter().flat_map(|t| t.leaves()).collect(),
            v => vec![v],
        }
    }

    /// One line: `table (3 columns, 20 rows)`, `chart (2 lines, 500 points)`, …
    pub fn summary(&self) -> String {
        match self {
            ResultView::Table { columns, rows, .. } => format!("table ({} columns, {} rows)", columns.len(), rows.len()),
            ResultView::Chart { x, series, .. } => format!("chart ({} lines, {} points)", series.len(), x.values.len()),
            ResultView::Text { text, .. } => format!("text ({} lines)", text.lines().count()),
            ResultView::Tiles { tiles, .. } => format!("{} tiles: {}", tiles.len(), tiles.iter().map(|t| t.summary()).collect::<Vec<_>>().join(", ")),
        }
    }

    /// The view as a table (a chart lists x and its lines; text has one column).
    pub fn as_table(&self) -> (Vec<String>, Vec<Vec<String>>) {
        match self {
            ResultView::Table { columns, rows, .. } => (columns.clone(), rows.iter().map(|r| r.iter().map(cell).collect()).collect()),
            ResultView::Chart { x, series, .. } => {
                let mut head = vec![if x.name.is_empty() { "x".to_string() } else { x.name.clone() }];
                head.extend(series.iter().map(|s| s.name.clone()));
                let f = |v: Option<f64>| v.map(crate::export::docs::num).unwrap_or_default();
                let rows = (0..x.values.len())
                    .map(|i| {
                        let mut r = vec![match (x.time, x.values[i]) {
                            (true, Some(t)) => crate::ingest::time::fmt_ts(t as i64),
                            (_, v) => f(v),
                        }];
                        r.extend(series.iter().map(|s| f(s.values[i])));
                        r
                    })
                    .collect();
                (head, rows)
            }
            ResultView::Text { text, .. } => (vec!["text".into()], text.lines().map(|l| vec![l.to_string()]).collect()),
            ResultView::Tiles { .. } => (vec![], vec![]),
        }
    }

    /// The view as chart lines: a chart as is; a table's number columns against its first
    /// column (or the row number). `None` when there is nothing to draw.
    pub fn as_chart(&self) -> Option<(Axis, Vec<Line>)> {
        match self {
            ResultView::Chart { x, series, .. } => Some((x.clone(), series.clone())),
            ResultView::Table { columns, rows, .. } => {
                let num = |c: usize| rows.iter().map(|r| r[c].as_f64()).collect::<Vec<_>>();
                let is_num = |c: usize| rows.iter().any(|r| r[c].is_number()) && rows.iter().all(|r| r[c].is_number() || r[c].is_null());
                let (x, from) = if !columns.is_empty() && is_num(0) {
                    let time = matches!(columns[0].as_str(), "timestamp" | "time" | "date" | "_created_at" | "_updated_at");
                    (Axis { name: columns[0].clone(), time, values: num(0) }, 1)
                } else {
                    (Axis { name: "row".into(), time: false, values: (0..rows.len()).map(|i| Some(i as f64)).collect() }, 0)
                };
                let lines: Vec<Line> =
                    (from..columns.len()).filter(|c| is_num(*c)).map(|c| Line { name: columns[c].clone(), values: num(c) }).collect();
                (!lines.is_empty()).then_some((x, lines))
            }
            _ => None,
        }
    }

    /// Plain text for a terminal (`mnnn code run`): tables are cut to `rows` rows and
    /// charts get a sparkline per line.
    pub fn text_lines(&self, rows: usize) -> Vec<String> {
        let mut out = vec![];
        self.lines_into(&mut out, rows, 0);
        out
    }

    fn lines_into(&self, out: &mut Vec<String>, max: usize, depth: usize) {
        let pad = "  ".repeat(depth);
        if !self.title().is_empty() {
            out.push(format!("{pad}== {} ==", self.title()));
        }
        match self {
            ResultView::Tiles { tiles, .. } => {
                for t in tiles {
                    t.lines_into(out, max, depth + 1);
                }
            }
            ResultView::Text { text, .. } => out.extend(text.lines().map(|l| format!("{pad}{l}"))),
            ResultView::Chart { x, series, .. } => {
                out.push(format!("{pad}{} points over {}", x.values.len(), if x.name.is_empty() { "x" } else { &x.name }));
                for s in series {
                    let vals: Vec<f64> = s.values.iter().flatten().copied().filter(|v| v.is_finite()).collect();
                    let (lo, hi) = vals.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(*v), b.max(*v)));
                    let last = vals.last().map(|v| crate::export::docs::num(*v)).unwrap_or_else(|| "-".into());
                    let range = if vals.is_empty() {
                        "no values".to_string()
                    } else {
                        format!("{} … {}", crate::export::docs::num(lo), crate::export::docs::num(hi))
                    };
                    out.push(format!("{pad}{:<12} {}  {range}  last {last}", s.name, sparkline(&s.values, 40)));
                }
            }
            ResultView::Table { .. } => {
                let (head, rows) = self.as_table();
                let shown = &rows[..rows.len().min(max)];
                let mut w: Vec<usize> = head.iter().map(|h| h.chars().count()).collect();
                for r in shown {
                    for (i, c) in r.iter().enumerate() {
                        w[i] = w[i].max(c.chars().count()).min(40);
                    }
                }
                let line = |r: &[String]| {
                    let cells: Vec<String> =
                        r.iter().enumerate().map(|(i, c)| format!("{:<w$}", c.chars().take(40).collect::<String>(), w = w[i])).collect();
                    format!("{pad}{}", cells.join("  ").trim_end())
                };
                out.push(line(&head));
                out.extend(shown.iter().map(|r| line(r)));
                if rows.len() > shown.len() {
                    out.push(format!("{pad}… {} more rows", rows.len() - shown.len()));
                }
            }
        }
    }
}

/// `▁▂▃▅▇` over at most `width` buckets (the last value of each).
pub fn sparkline(values: &[Option<f64>], width: usize) -> String {
    const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    if values.is_empty() || width == 0 {
        return String::new();
    }
    let n = values.len().min(width);
    let pts: Vec<Option<f64>> = (0..n).map(|i| values[((i + 1) * values.len() / n).saturating_sub(1)].filter(|v| v.is_finite())).collect();
    let (lo, hi) = pts.iter().flatten().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(*v), b.max(*v)));
    pts.iter()
        .map(|p| match p {
            None => ' ',
            Some(v) if hi > lo => BARS[(((v - lo) / (hi - lo)) * 7.0).round() as usize],
            Some(_) => BARS[3],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn views_parse_check_and_switch() {
        let (v, notes) = ResultView::from_plugin(json!({
            "kind": "tiles", "title": "t", "dir": "below", "tiles": [
                {"kind": "table", "columns": ["timestamp", "close", "side"], "rows": [[0, 1.5, "buy"], [60000, 2.5, "sell"]]},
                {"kind": "chart", "x": {"name": "row", "values": [0, 1]}, "series": [{"name": "a", "values": [1, null]}]},
                {"kind": "text", "text": "hello"}
            ]
        }))
        .unwrap();
        assert!(notes.is_empty());
        assert_eq!(v.leaves().len(), 3);
        let (x, lines) = v.leaves()[0].as_chart().unwrap();
        assert_eq!((x.time, lines.len(), lines[0].name.as_str()), (true, 1, "close"), "text columns are not drawn");
        let (head, rows) = v.leaves()[1].as_table();
        assert_eq!((head, rows[1][1].as_str()), (vec!["row".to_string(), "a".to_string()], ""));
        let txt = v.text_lines(10).join("\n");
        assert!(txt.contains("== t ==") && txt.contains("sell") && txt.contains("hello"), "{txt}");
        assert!(ResultView::from_plugin(json!({"kind": "table", "columns": ["a"], "rows": [[1, 2]]})).is_err());
        assert!(ResultView::from_plugin(json!({"kind": "chart", "x": {"values": [1]}, "series": [{"name": "a", "values": []}]})).is_err());
        assert!(ResultView::from_plugin(json!({"kind": "pie"})).is_err());
        let rows: Vec<Value> = (0..MAX_ROWS + 3).map(|i| json!([i])).collect();
        let (v, notes) = ResultView::from_plugin(json!({"kind": "table", "columns": ["n"], "rows": rows})).unwrap();
        assert_eq!((notes.len(), v.as_table().1.len()), (1, MAX_ROWS));
        assert_eq!(sparkline(&[Some(0.0), Some(1.0)], 10), "▁█");
    }
}
