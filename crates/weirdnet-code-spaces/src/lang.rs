//! What a code space is written in.
use serde::{Deserialize, Serialize};

/// What a code space is written in. Serialized in snake case (`"math"`, `"plugin"`, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lang {
    /// Formulas over columns (`sma20 = sma(close, 20)`), compiled through generated Rust.
    Math,
    /// Polars expressions (`ret = col("close").pct_change(lit(1))`), compiled through generated Rust.
    Polars,
    /// Rust against the profile's plugin SDK.
    Rust,
    /// Typed Python functions, translated to Rust in process (ruff parser, python-ast codegen).
    Python,
    /// A library built elsewhere (the source is its path).
    Plugin,
}

impl Lang {
    pub const ALL: [Lang; 5] = [Lang::Math, Lang::Polars, Lang::Rust, Lang::Python, Lang::Plugin];
    pub fn as_str(self) -> &'static str {
        match self {
            Lang::Math => "math",
            Lang::Polars => "polars",
            Lang::Rust => "rust",
            Lang::Python => "python",
            Lang::Plugin => "plugin",
        }
    }
    pub fn parse(s: &str) -> Option<Lang> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "math" | "formula" | "formulas" => Lang::Math,
            "polars" | "pl" => Lang::Polars,
            "rust" | "rs" => Lang::Rust,
            "python" | "py" | "rython" => Lang::Python,
            "plugin" | "prebuilt" | "lib" => Lang::Plugin,
            _ => return None,
        })
    }
    /// The source is code to compile (not a path to a built library).
    pub fn compiled(self) -> bool {
        self != Lang::Plugin
    }
    /// File extension for editing the source.
    pub fn ext(self) -> &'static str {
        match self {
            Lang::Math => "math",
            Lang::Polars => "polars",
            Lang::Rust => "rs",
            Lang::Python => "py",
            Lang::Plugin => "txt",
        }
    }
}
