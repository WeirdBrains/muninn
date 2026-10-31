//! # muninn-plugin
//!
//! The SDK for Muninn code spaces. A code space is compiled into a shared library that
//! Muninn loads and runs; this crate is the safe side of that: the [`Host`] API (load,
//! update and create records, create models), [`Frame`]s of loaded records, result
//! [`View`]s, and the [`plugin!`] macro that exports the C ABI ([`abi`]).
//!
//! ```ignore
//! use muninn_plugin::prelude::*;
//!
//! pub fn run(host: &Host) -> Result<View> {
//!     let bars = host.input()?;                       // the code space's load request
//!     let close = bars.f64("close")?;
//!     let sma = series::sma(&close, &[20.0]);
//!     Ok(View::chart("close and SMA 20").x("timestamp", bars.f64("timestamp")?, true)
//!         .line("close", close).line("sma20", sma))
//! }
//! muninn_plugin::plugin!(run);
//! ```
//!
//! Muninn writes this crate next to every code space it builds (`mnnn code sdk` writes a
//! copy for plugins built elsewhere). See `docs/code-spaces.md`.
pub mod abi;
pub mod columns;
pub mod json;
pub mod series;

pub use json::Json;
use std::fmt;

/// The SDK version (the ABI version is [`abi::ABI_VERSION`]).
pub const SDK_VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod prelude {
    pub use crate::series;
    pub use crate::{Error, Frame, Host, Json, Load, Result, View};
}

/// An error for the host to show.
#[derive(Debug, Clone, PartialEq)]
pub struct Error(pub String);

impl Error {
    pub fn new(msg: impl Into<String>) -> Self {
        Error(msg.into())
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}
impl From<String> for Error {
    fn from(s: String) -> Self {
        Error(s)
    }
}
impl From<&str> for Error {
    fn from(s: &str) -> Self {
        Error(s.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// The host for one run.
pub struct Host {
    raw: *const abi::MnHost,
}

impl Host {
    /// # Safety
    /// `raw` must be the host table passed to `muninn_plugin_run`, valid for the run.
    pub unsafe fn from_raw(raw: *const abi::MnHost) -> Result<Host> {
        if raw.is_null() {
            return Err("no host".into());
        }
        let v = (*raw).abi_version;
        if v != abi::ABI_VERSION {
            return Err(Error(format!("host ABI {v}, plugin ABI {}", abi::ABI_VERSION)));
        }
        Ok(Host { raw })
    }

    /// Call a host method with a JSON request.
    pub fn call(&self, method: &str, request: &Json) -> Result<Json> {
        let req = request.to_string();
        let mut out = abi::MnBuf::EMPTY;
        // SAFETY: the host table is valid for the run; strings are borrowed for the call only.
        let (code, text) = unsafe {
            let h = &*self.raw;
            let code = (h.call)(h.ctx, abi::MnStr::new(method), abi::MnStr::new(&req), &mut out);
            let text =
                if out.ptr.is_null() { String::new() } else { String::from_utf8_lossy(std::slice::from_raw_parts(out.ptr, out.len)).into_owned() };
            (h.free)(h.ctx, out);
            (code, text)
        };
        if code != abi::CALL_OK {
            return Err(Error(text));
        }
        if text.is_empty() {
            return Ok(Json::Null);
        }
        Json::parse(&text).map_err(|e| Error(format!("bad reply from {method}: {e}")))
    }

    /// The host methods this Muninn knows.
    pub fn methods(&self) -> Result<Vec<String>> {
        Ok(self.call("methods", &Json::Null)?.as_array().unwrap_or(&[]).iter().filter_map(|m| m.as_str().map(str::to_string)).collect())
    }

    /// Records as the code space's input says (model, filters, grouping, sorting).
    pub fn input(&self) -> Result<Frame> {
        Frame::from_json(&self.call("input", &Json::Null)?)
    }

    /// The names of the inputs, when several feed this run (a pipeline's fan-in).
    pub fn inputs(&self) -> Result<Vec<String>> {
        Ok(self.call("inputs", &Json::Null)?.as_array().unwrap_or(&[]).iter().filter_map(|m| m.as_str().map(str::to_string)).collect())
    }

    /// One input by name (a pipeline's fan-in: the upstream node's id, or the edge's name).
    pub fn input_named(&self, name: &str) -> Result<Frame> {
        Frame::from_json(&self.call("input_named", &Json::obj().with("name", name))?)
    }

    /// The code space's input request itself (what [`Host::input`] loads).
    pub fn input_request(&self) -> Result<Json> {
        self.call("input_request", &Json::Null)
    }

    /// Every model with its fields: `[{"name": "signals", "version": 2, "fields": [{"name", "type", "ref"}]}]`.
    pub fn models(&self) -> Result<Json> {
        self.call("models", &Json::Null)
    }

    /// Load records.
    pub fn load(&self, q: &Load) -> Result<Frame> {
        Frame::from_json(&self.call("load", &q.to_json())?)
    }

    /// Change one record by id: `values` is an object of FIELD: VALUE.
    pub fn update(&self, model: &str, id: &str, values: Json) -> Result<()> {
        self.call("update", &Json::obj().with("model", model).with("id", id).with("values", values)).map(|_| ())
    }

    /// Change every record matching all conditions (`price > 3`); returns how many.
    pub fn update_where(&self, model: &str, conditions: &[&str], values: Json) -> Result<usize> {
        let w: Vec<Json> = conditions.iter().map(|c| Json::from(*c)).collect();
        let r = self.call("update_where", &Json::obj().with("model", model).with("where", Json::Arr(w)).with("values", values))?;
        Ok(r.get("count").and_then(Json::as_f64).unwrap_or(0.0) as usize)
    }

    /// Change many records, each by id with its own values (one history entry).
    pub fn update_each(&self, model: &str, updates: Vec<(String, Json)>) -> Result<usize> {
        let u: Vec<Json> = updates.into_iter().map(|(id, v)| Json::obj().with("id", id).with("values", v)).collect();
        let r = self.call("update_each", &Json::obj().with("model", model).with("updates", Json::Arr(u)))?;
        Ok(r.get("count").and_then(Json::as_f64).unwrap_or(0.0) as usize)
    }

    /// Create a record; returns its id.
    pub fn create(&self, model: &str, values: Json) -> Result<String> {
        let r = self.call("create", &Json::obj().with("model", model).with("values", values))?;
        Ok(r.get("id").and_then(Json::as_str).unwrap_or_default().to_string())
    }

    /// Create many records (one history entry); returns their ids.
    pub fn create_many(&self, model: &str, records: Vec<Json>) -> Result<Vec<String>> {
        let r = self.call("create_many", &Json::obj().with("model", model).with("records", Json::Arr(records)))?;
        Ok(r.get("ids").and_then(Json::as_array).unwrap_or(&[]).iter().filter_map(|i| i.as_str().map(str::to_string)).collect())
    }

    /// Create a model with the designer's field syntax (`"symbol:text!; score:real; idea:ref:ideas"`),
    /// applied as v1. Nothing happens if it exists already (returns false).
    pub fn create_model(&self, name: &str, fields: &str, about: &str) -> Result<bool> {
        let r = self.call("model_create", &Json::obj().with("name", name).with("fields", fields).with("about", about))?;
        Ok(r.get("created").and_then(Json::as_bool).unwrap_or(false))
    }

    /// A line for the run log (shown with the result, never printed by the host).
    pub fn log(&self, text: impl Into<String>) {
        let _ = self.call("log", &Json::obj().with("text", text.into()));
    }
}

/// A load request: which model, and which records in what order.
#[derive(Debug, Clone, Default)]
pub struct Load {
    pub model: String,
    pub conditions: Vec<String>,
    pub sort: Vec<String>,
    pub group: Vec<String>,
    pub text: String,
    pub limit: Option<usize>,
}

impl Load {
    pub fn model(m: &str) -> Self {
        Load { model: m.into(), ..Default::default() }
    }
    /// A condition: `price > 3`, `side = buy`, `note ~ halving`.
    pub fn filter(mut self, c: &str) -> Self {
        self.conditions.push(c.into());
        self
    }
    /// A sort field; `-price` sorts descending.
    pub fn sort(mut self, f: &str) -> Self {
        self.sort.push(f.into());
        self
    }
    /// Group by a field (rows come ordered by group; see [`Frame::groups`]).
    pub fn group(mut self, f: &str) -> Self {
        self.group.push(f.into());
        self
    }
    pub fn text(mut self, t: &str) -> Self {
        self.text = t.into();
        self
    }
    pub fn limit(mut self, n: usize) -> Self {
        self.limit = Some(n);
        self
    }
    pub fn to_json(&self) -> Json {
        let s = |v: &[String]| Json::Arr(v.iter().map(|x| Json::from(x.as_str())).collect());
        Json::obj()
            .with("model", self.model.as_str())
            .with("where", s(&self.conditions))
            .with("sort", s(&self.sort))
            .with("group", s(&self.group))
            .with("text", self.text.as_str())
            .with("limit", self.limit)
    }
}

/// One field of a frame.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldInfo {
    pub name: String,
    /// `text`, `integer`, `real`, `bool`, `timestamp`, `json` or `ref:MODEL`.
    pub ty: String,
}

/// Rows `start .. start + len` share the group `key`.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub key: Vec<(String, Json)>,
    pub start: usize,
    pub len: usize,
}

/// Loaded records, by column.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Frame {
    pub model: String,
    pub fields: Vec<FieldInfo>,
    pub ids: Vec<String>,
    pub columns: Vec<(String, Vec<Json>)>,
    pub groups: Vec<Group>,
}

impl Frame {
    pub fn from_json(j: &Json) -> Result<Frame> {
        let model = j.get("model").and_then(Json::as_str).unwrap_or_default().to_string();
        let fields = j
            .get("fields")
            .and_then(Json::as_array)
            .unwrap_or(&[])
            .iter()
            .map(|f| FieldInfo {
                name: f.get("name").and_then(Json::as_str).unwrap_or_default().into(),
                ty: f.get("type").and_then(Json::as_str).unwrap_or_default().into(),
            })
            .collect();
        let ids: Vec<String> = j.get("ids").and_then(Json::as_array).unwrap_or(&[]).iter().map(Json::text).collect();
        let columns: Vec<(String, Vec<Json>)> = j
            .get("columns")
            .and_then(Json::as_object)
            .unwrap_or(&[])
            .iter()
            .map(|(k, v)| (k.clone(), v.as_array().unwrap_or(&[]).to_vec()))
            .collect();
        if let Some((name, c)) = columns.iter().find(|(_, c)| c.len() != ids.len()) {
            return Err(Error(format!("column {name} has {} values for {} records", c.len(), ids.len())));
        }
        let groups = j
            .get("groups")
            .and_then(Json::as_array)
            .unwrap_or(&[])
            .iter()
            .map(|g| Group {
                key: g.get("key").and_then(Json::as_object).unwrap_or(&[]).to_vec(),
                start: g.get("start").and_then(Json::as_f64).unwrap_or(0.0) as usize,
                len: g.get("len").and_then(Json::as_f64).unwrap_or(0.0) as usize,
            })
            .collect();
        Ok(Frame { model, fields, ids, columns, groups })
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }
    pub fn has(&self, name: &str) -> bool {
        name == "id" || self.columns.iter().any(|(n, _)| n == name)
    }
    /// The values of a field (`Null` where a record has none).
    pub fn column(&self, name: &str) -> Result<&[Json]> {
        self.columns.iter().find(|(n, _)| n == name).map(|(_, c)| c.as_slice()).ok_or_else(|| {
            Error(format!(
                "{} has no field `{name}` (fields: {})",
                self.model,
                self.columns.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", ")
            ))
        })
    }
    /// A field as numbers (`NaN` where missing or not a number; timestamps are epoch ms).
    pub fn f64(&self, name: &str) -> Result<Vec<f64>> {
        if name == "row" && !self.has("row") {
            return Ok(series::row(self.len()));
        }
        Ok(self.column(name)?.iter().map(|v| v.as_f64().unwrap_or(f64::NAN)).collect())
    }
    /// A field as text (empty where missing).
    pub fn text(&self, name: &str) -> Result<Vec<String>> {
        if name == "id" {
            return Ok(self.ids.clone());
        }
        Ok(self.column(name)?.iter().map(Json::text).collect())
    }
    /// One value.
    pub fn get(&self, row: usize, name: &str) -> Json {
        self.column(name).ok().and_then(|c| c.get(row).cloned()).unwrap_or_default()
    }
    /// Rows `start .. start + len` as a frame of their own.
    pub fn slice(&self, start: usize, len: usize) -> Frame {
        let end = (start + len).min(self.len());
        let start = start.min(end);
        Frame {
            model: self.model.clone(),
            fields: self.fields.clone(),
            ids: self.ids[start..end].to_vec(),
            columns: self.columns.iter().map(|(n, c)| (n.clone(), c[start..end].to_vec())).collect(),
            groups: vec![],
        }
    }
    /// The given rows (in that order) as a frame of their own.
    pub fn take(&self, rows: &[usize]) -> Frame {
        let rows: Vec<usize> = rows.iter().copied().filter(|r| *r < self.len()).collect();
        Frame {
            model: self.model.clone(),
            fields: self.fields.clone(),
            ids: rows.iter().map(|r| self.ids[*r].clone()).collect(),
            columns: self.columns.iter().map(|(n, c)| (n.clone(), rows.iter().map(|r| c[*r].clone()).collect())).collect(),
            groups: vec![],
        }
    }
    /// The groups of a grouped load, or one group with every row.
    pub fn groups(&self) -> Vec<Group> {
        if self.groups.is_empty() {
            vec![Group { key: vec![], start: 0, len: self.len() }]
        } else {
            self.groups.clone()
        }
    }
}

/// What a run shows: a table, a chart, text, or tiles of those. Muninn shows tables as list
/// tiles and charts as chart tiles (either can switch to the other), side by side for tiles.
#[derive(Debug, Clone, PartialEq)]
pub enum View {
    Table { title: String, columns: Vec<String>, rows: Vec<Vec<Json>> },
    Chart { title: String, x_name: String, x: Vec<f64>, x_time: bool, series: Vec<(String, Vec<f64>)> },
    Text { title: String, text: String },
    Tiles { title: String, below: bool, tiles: Vec<View> },
}

impl View {
    pub fn table(title: &str, columns: &[&str]) -> View {
        View::Table { title: title.into(), columns: columns.iter().map(|c| c.to_string()).collect(), rows: vec![] }
    }
    /// Add a row (tables only).
    pub fn row(mut self, values: Vec<Json>) -> View {
        if let View::Table { rows, .. } = &mut self {
            rows.push(values);
        }
        self
    }
    pub fn chart(title: &str) -> View {
        View::Chart { title: title.into(), x_name: "row".into(), x: vec![], x_time: false, series: vec![] }
    }
    /// The x axis (`time`: epoch milliseconds).
    pub fn x(mut self, name: &str, values: Vec<f64>, time: bool) -> View {
        if let View::Chart { x_name, x, x_time, .. } = &mut self {
            *x_name = name.into();
            *x = values;
            *x_time = time;
        }
        self
    }
    /// A line on the chart.
    pub fn line(mut self, name: &str, values: Vec<f64>) -> View {
        if let View::Chart { series, .. } = &mut self {
            series.push((name.into(), values));
        }
        self
    }
    pub fn text(title: &str, text: &str) -> View {
        View::Text { title: title.into(), text: text.into() }
    }
    /// Tiles side by side (`below`: stacked).
    pub fn tiles(title: &str, below: bool, tiles: Vec<View>) -> View {
        View::Tiles { title: title.into(), below, tiles }
    }

    pub fn to_json(&self) -> Json {
        match self {
            View::Table { title, columns, rows } => Json::obj()
                .with("kind", "table")
                .with("title", title.as_str())
                .with("columns", Json::Arr(columns.iter().map(|c| Json::from(c.as_str())).collect()))
                .with("rows", Json::Arr(rows.iter().map(|r| Json::Arr(r.clone())).collect())),
            View::Chart { title, x_name, x, x_time, series } => Json::obj()
                .with("kind", "chart")
                .with("title", title.as_str())
                .with("x", Json::obj().with("name", x_name.as_str()).with("time", *x_time).with("values", nums(x)))
                .with("series", Json::Arr(series.iter().map(|(n, v)| Json::obj().with("name", n.as_str()).with("values", nums(v))).collect())),
            View::Text { title, text } => Json::obj().with("kind", "text").with("title", title.as_str()).with("text", text.as_str()),
            View::Tiles { title, below, tiles } => Json::obj()
                .with("kind", "tiles")
                .with("title", title.as_str())
                .with("dir", if *below { "below" } else { "right" })
                .with("tiles", Json::Arr(tiles.iter().map(View::to_json).collect())),
        }
    }
}

fn nums(v: &[f64]) -> Json {
    Json::Arr(v.iter().map(|x| if x.is_finite() { Json::Num(*x) } else { Json::Null }).collect())
}

fn panic_text(p: &(dyn std::any::Any + Send)) -> String {
    p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_else(|| "panic".into())
}

/// Run `f` for `muninn_plugin_run`: hands its view or error to the host; panics are caught.
///
/// # Safety
/// `raw` must be the host table passed to `muninn_plugin_run`.
#[doc(hidden)]
pub unsafe fn __run(raw: *const abi::MnHost, f: fn(&Host) -> Result<View>) -> i32 {
    let host = match Host::from_raw(raw) {
        Ok(h) => h,
        Err(_) => return abi::RUN_ERROR,
    };
    // a panic message goes to the host, not to the terminal
    std::panic::set_hook(Box::new(|_| {}));
    let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&host)));
    let _ = std::panic::take_hook();
    match out {
        Ok(Ok(view)) => match host.call("result", &view.to_json()) {
            Ok(_) => abi::RUN_OK,
            Err(e) => {
                let _ = host.call("error", &Json::obj().with("message", format!("the host refused the result: {e}")));
                abi::RUN_ERROR
            }
        },
        Ok(Err(e)) => {
            let _ = host.call("error", &Json::obj().with("message", e.0));
            abi::RUN_ERROR
        }
        Err(p) => {
            let _ = host.call("error", &Json::obj().with("message", format!("panicked: {}", panic_text(&*p))));
            abi::RUN_PANIC
        }
    }
}

/// Export the plugin ABI for a `fn(&Host) -> Result<View>` (and the language it came from).
#[macro_export]
macro_rules! plugin {
    ($run:path) => {
        $crate::plugin!($run, "rust");
    };
    ($run:path, $lang:literal) => {
        #[no_mangle]
        pub extern "C" fn muninn_plugin_abi() -> u32 {
            $crate::abi::ABI_VERSION
        }
        #[no_mangle]
        pub extern "C" fn muninn_plugin_info() -> $crate::abi::MnStr {
            const INFO: &str = concat!("{\"sdk\":\"1.0.0\",\"lang\":\"", $lang, "\"}");
            $crate::abi::MnStr::new(INFO)
        }
        /// # Safety
        /// Called by Muninn with a valid host table.
        #[no_mangle]
        pub unsafe extern "C" fn muninn_plugin_run(host: *const $crate::abi::MnHost) -> i32 {
            $crate::__run(host, $run)
        }
    };
}
