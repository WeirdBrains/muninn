# weirdnet-render

Structured data drawn to PNG. Inputs are plain data, not any application's types:

- [`Table`]: column names and rows of text cells, drawn as a grid with an 8×8 bitmap font
  (numbers right-aligned, long cells cut with `~`, characters without a glyph shown as `?`).
- [`Chart`]: x values and named series of numbers (gaps allowed), drawn as a line chart;
  [`legend`] names each series' color.
- JSON describing either one ([`json_png`], [`Drawing::from_json`]):
  - `{"columns": ["a", "b"], "rows": [[1, "x"], [2, null]]}`, a table;
  - `[{"a": 1, "b": "x"}, {"a": 2}]`, a table whose columns are the keys, sorted;
  - `[[1, "x"], [2, "y"]]`, a table with columns `1`, `2`, …;
  - `{"x": [0, 1, 2], "series": [{"name": "a", "values": [1, null, 3]}], "width": 900, "height": 320}`,
    a chart (`x`, `width` and `height` are optional).

Every entry point checks its input against [`Limits`] (columns, rows, cell length, series,
points, image width, height and pixel count, JSON size) and returns an [`Error`] instead of
drawing something unbounded. A chart with no finite value is [`Error::Empty`].

```rust
use weirdnet_render::{Limits, Table, table_png, json_png};
let t = Table { columns: vec!["name".into(), "temp".into()], rows: vec![vec!["north".into(), "21.5".into()]] };
let png = table_png(&t, &Limits::default()).unwrap();
assert_eq!(&png[1..4], b"PNG");
let png = json_png(r#"{"series":[{"name":"t","values":[1,2,3]}]}"#, &Limits::default()).unwrap();
assert_eq!(&png[1..4], b"PNG");
```

The crate knows nothing about where the data comes from; an application translates its own
result types into these (Muninn does that for code-space charts in reports).

BSD-3-Clause. The glyphs come from the `font8x8` crate (MIT).
