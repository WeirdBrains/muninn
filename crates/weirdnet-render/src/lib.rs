#![doc = include_str!("../README.md")]
mod canvas;
mod chart;
mod json;
mod table;

pub use chart::{chart_png, legend};
pub use json::json_png;
pub use table::table_png;

/// A table: column names and rows of cell text (each row as long as `columns`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Table {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// One line of a chart; `None` (or a non-finite number) is a gap.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Series {
    pub name: String,
    pub values: Vec<Option<f64>>,
}

/// A line chart: `x` positions (a missing one is its index) and the series over them.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Chart {
    pub x: Vec<Option<f64>>,
    pub series: Vec<Series>,
}

/// Something to draw, as read from JSON.
#[derive(Debug, Clone, PartialEq)]
pub enum Drawing {
    Table(Table),
    Chart { chart: Chart, width: usize, height: usize },
}

/// Default chart size when JSON does not give one.
pub const CHART_WIDTH: usize = 900;
pub const CHART_HEIGHT: usize = 320;

/// What a caller is willing to have drawn. Anything over a limit is an [`Error::TooLarge`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_columns: usize,
    pub max_rows: usize,
    /// Characters kept per cell; longer cells are cut and end in `~`.
    pub max_cell_chars: usize,
    pub max_series: usize,
    pub max_points: usize,
    pub max_width: usize,
    pub max_height: usize,
    pub max_pixels: usize,
    pub max_json_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_columns: 64,
            max_rows: 1_000,
            max_cell_chars: 80,
            max_series: 64,
            max_points: 100_000,
            max_width: 8_192,
            max_height: 16_384,
            max_pixels: 16 * 1024 * 1024,
            max_json_bytes: 8 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("too large: {0}")]
    TooLarge(String),
    #[error("nothing to draw: no finite values")]
    Empty,
    #[error("png: {0}")]
    Encode(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Drawing {
    /// Read a table or a chart from JSON (the forms are in the crate docs).
    pub fn from_json(v: &serde_json::Value, limits: &Limits) -> Result<Drawing> {
        json::drawing(v, limits)
    }
}

/// Draw a table or a chart.
pub fn render(d: &Drawing, limits: &Limits) -> Result<Vec<u8>> {
    match d {
        Drawing::Table(t) => table_png(t, limits),
        Drawing::Chart { chart, width, height } => chart_png(chart, *width, *height, limits),
    }
}

pub(crate) fn check_size(w: usize, h: usize, l: &Limits) -> Result<()> {
    if w == 0 || h == 0 {
        return Err(Error::Invalid(format!("image size {w}x{h}")));
    }
    if w > l.max_width || h > l.max_height || w.saturating_mul(h) > l.max_pixels {
        return Err(Error::TooLarge(format!("image {w}x{h} (at most {}x{}, {} pixels)", l.max_width, l.max_height, l.max_pixels)));
    }
    Ok(())
}
