//! CSV reader (Polars).
use crate::error::{Error, Result};
use polars::prelude::*;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct CsvOptions {
    pub delimiter: char,
    pub quote: char,
    pub escape: Option<char>,
    pub header: bool,
    pub skip_rows: usize,
}

impl Default for CsvOptions {
    fn default() -> Self {
        Self { delimiter: ',', quote: '"', escape: None, header: true, skip_rows: 0 }
    }
}

fn byte(c: char, what: &'static str) -> Result<u8> {
    if c.is_ascii() {
        Ok(c as u8)
    } else {
        Err(Error::invalid(what, format!("`{c}` must be a single ASCII character")))
    }
}

/// Read every column as a string; numeric/timestamp parsing happens in normalization.
pub fn read_csv(path: &Path, o: &CsvOptions) -> Result<DataFrame> {
    let sep = byte(o.delimiter, "--delimiter")?;
    let quote = byte(o.quote, "--quote")?;
    if let Some(e) = o.escape {
        // Polars only supports RFC 4180 escaping (doubling the quote character).
        if e != o.quote {
            return Err(Error::invalid(
                "--escape",
                format!("`{e}`: only the quote character itself (RFC 4180 doubling) is supported"),
            ));
        }
    }
    let df = CsvReadOptions::default()
        .with_has_header(o.header)
        .with_skip_rows(o.skip_rows)
        .with_infer_schema_length(Some(0))
        .map_parse_options(|p| p.with_separator(sep).with_quote_char(Some(quote)).with_truncate_ragged_lines(true))
        .try_into_reader_with_file_path(Some(path.to_path_buf()))?
        .finish()?;
    Ok(df)
}
