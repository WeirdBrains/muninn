//! Core types: bars, sources, symbols, durations and bar ids.
use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Bar duration in whole seconds, rendered as `5m`, `1h`, `1d`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Duration(pub i64);

impl Duration {
    pub fn secs(self) -> i64 {
        self.0
    }
    pub fn millis(self) -> i64 {
        self.0 * 1000
    }
    /// Parse `30s`, `5m`, `5min`, `1h`, `1d`, `1w`.
    pub fn parse(s: &str) -> Result<Self> {
        let t = s.trim().to_ascii_lowercase();
        let idx = t.find(|c: char| !c.is_ascii_digit()).unwrap_or(t.len());
        let (num, unit) = t.split_at(idx);
        let n: i64 = num.parse().map_err(|_| Error::invalid("duration", format!("`{s}` (expected e.g. 5m, 1h, 1d)")))?;
        let mult = match unit {
            "s" | "sec" | "secs" => 1,
            "m" | "min" | "mins" | "t" => 60,
            "h" | "hr" | "hour" => 3600,
            "d" | "day" => 86_400,
            "w" | "wk" | "week" => 604_800,
            _ => return Err(Error::invalid("duration", format!("`{s}` (unknown unit)"))),
        };
        if n <= 0 {
            return Err(Error::invalid("duration", format!("`{s}` must be positive")));
        }
        Ok(Duration(n * mult))
    }
}

impl fmt::Display for Duration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = self.0;
        for (unit, m) in [("w", 604_800), ("d", 86_400), ("h", 3600), ("m", 60)] {
            if s % m == 0 {
                return write!(f, "{}{}", s / m, unit);
            }
        }
        write!(f, "{s}s")
    }
}

/// A trading pair, e.g. BTC/USDT.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Symbol {
    pub base: String,
    pub quote: String,
}

/// Quote currencies, longest first for suffix matching.
pub const KNOWN_QUOTES: &[&str] = &[
    "FDUSD", "USDT", "USDC", "BUSD", "TUSD", "USDP", "USD", "EUR", "GBP", "JPY", "AUD", "CAD", "CHF", "TRY", "BRL",
    "KRW", "DAI", "BTC", "ETH", "BNB",
];

/// Default known base assets.
pub const KNOWN_BASES: &[&str] = &[
    "BTC", "ETH", "SOL", "XRP", "ADA", "DOGE", "LTC", "BNB", "DOT", "AVAX", "LINK", "MATIC", "POL", "BCH", "XLM", "TRX",
    "ATOM", "UNI", "ETC", "XMR", "FIL", "NEAR", "APT", "ARB", "OP", "SHIB", "TON", "SUI", "PEPE", "AAVE",
];

/// Default asset aliases.
pub const KNOWN_ALIASES: &[(&str, &str)] = &[("XBT", "BTC"), ("XDG", "DOGE")];

impl Symbol {
    pub fn new(base: &str, quote: &str) -> Self {
        Self { base: base.to_ascii_uppercase(), quote: quote.to_ascii_uppercase() }
    }

    /// Parse `BTC/USDT`, `BTC-USD`, `btc_usd` or concatenated `BTCUSDT` (needs known quote).
    pub fn parse(s: &str) -> Result<Self> {
        Self::parse_with(s, KNOWN_QUOTES).ok_or_else(|| Error::invalid("symbol", format!("`{s}` (expected BASE/QUOTE)")))
    }

    pub fn parse_with(s: &str, quotes: &[&str]) -> Option<Self> {
        let t = s.trim().to_ascii_uppercase();
        for sep in ['/', '-', '_', ':'] {
            if let Some((b, q)) = t.split_once(sep) {
                if is_asset(b) && is_asset(q) {
                    return Some(Symbol::new(b, q));
                }
                return None;
            }
        }
        let mut qs: Vec<&str> = quotes.to_vec();
        qs.sort_by_key(|q| std::cmp::Reverse(q.len()));
        for q in qs {
            if let Some(b) = t.strip_suffix(q)
                && is_asset(b) {
                    return Some(Symbol::new(b, q));
                }
        }
        None
    }

    pub fn display(&self) -> String {
        format!("{}/{}", self.base, self.quote)
    }
}

fn is_asset(s: &str) -> bool {
    !s.is_empty() && s.len() <= 12 && s.chars().all(|c| c.is_ascii_alphanumeric())
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.base, self.quote)
    }
}

/// 32-byte BLAKE3 bar id: `BLAKE3("{ts_ms}|{source}|{base}/{quote}|{duration_s}")`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BarId(pub [u8; 32]);

impl BarId {
    pub fn compute(ts_ms: i64, source_name: &str, symbol: &Symbol, duration: Duration) -> Self {
        let s = format!("{}|{}|{}/{}|{}", ts_ms, source_name, symbol.base, symbol.quote, duration.0);
        BarId(*blake3::hash(s.as_bytes()).as_bytes())
    }
    pub fn hex(&self) -> String {
        hex::encode(self.0)
    }
}

/// Flag bits stored in `bars.flags`.
pub const FLAG_ZERO_VOLUME: i64 = 1;

/// A normalized bar (series metadata lives alongside, see `Series`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bar {
    pub ts: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub flags: i64,
}

/// Identifies a series of bars.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Series {
    pub source: Source,
    pub symbol: Symbol,
    pub duration: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Source {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub locator: Option<String>,
    pub created_at: i64,
}

/// A stored bar with series info (for export and the navigator).
#[derive(Debug, Clone, PartialEq)]
pub struct StoredBar {
    pub source: String,
    pub symbol: Symbol,
    pub duration: Duration,
    pub bar: Bar,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(Duration::parse("5m").unwrap(), Duration(300));
        assert_eq!(Duration::parse("1H").unwrap(), Duration(3600));
        assert_eq!(Duration::parse("15min").unwrap(), Duration(900));
        assert_eq!(Duration(300).to_string(), "5m");
        assert_eq!(Duration(86400).to_string(), "1d");
        assert_eq!(Duration(45).to_string(), "45s");
        assert!(Duration::parse("5x").is_err());
        assert!(Duration::parse("0m").is_err());
    }

    #[test]
    fn symbols() {
        assert_eq!(Symbol::parse("BTCUSDT").unwrap(), Symbol::new("BTC", "USDT"));
        assert_eq!(Symbol::parse("btc-usd").unwrap(), Symbol::new("BTC", "USD"));
        assert_eq!(Symbol::parse("ETH/BTC").unwrap(), Symbol::new("ETH", "BTC"));
        assert_eq!(Symbol::parse("ETHUSDC").unwrap(), Symbol::new("ETH", "USDC"));
        assert!(Symbol::parse("USDT").is_err());
    }

    #[test]
    fn bar_id_includes_duration_and_separators() {
        let s = Symbol::new("BTC", "USDT");
        let a = BarId::compute(0, "binance", &s, Duration(300));
        let b = BarId::compute(0, "binance", &s, Duration(3600));
        assert_ne!(a, b);
        assert_eq!(a, BarId::compute(0, "binance", &s, Duration(300)));
        // known vector
        let expect = blake3::hash(b"0|binance|BTC/USDT|300");
        assert_eq!(a.0, *expect.as_bytes());
    }
}

/// A per-bar price used by chart lines and the price filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PriceMetric {
    Open,
    High,
    Low,
    #[default]
    Close,
    /// (high + low + close) / 3
    Typical,
    /// (high + low) / 2
    Median,
    /// (open + high + low + close) / 4
    OhlcAvg,
    /// Volume-weighted typical price (over a range; per bar it is the typical price).
    Vwap,
}

impl PriceMetric {
    pub const ALL: [PriceMetric; 8] = [
        PriceMetric::Open,
        PriceMetric::High,
        PriceMetric::Low,
        PriceMetric::Close,
        PriceMetric::Typical,
        PriceMetric::Median,
        PriceMetric::OhlcAvg,
        PriceMetric::Vwap,
    ];
    pub fn value(self, b: &Bar) -> f64 {
        match self {
            PriceMetric::Open => b.open,
            PriceMetric::High => b.high,
            PriceMetric::Low => b.low,
            PriceMetric::Close => b.close,
            PriceMetric::Typical | PriceMetric::Vwap => (b.high + b.low + b.close) / 3.0,
            PriceMetric::Median => (b.high + b.low) / 2.0,
            PriceMetric::OhlcAvg => (b.open + b.high + b.low + b.close) / 4.0,
        }
    }
    /// SQL expression over the `bars b` alias (per bar).
    pub fn sql(self) -> &'static str {
        match self {
            PriceMetric::Open => "b.open",
            PriceMetric::High => "b.high",
            PriceMetric::Low => "b.low",
            PriceMetric::Close => "b.close",
            PriceMetric::Typical | PriceMetric::Vwap => "((b.high + b.low + b.close) / 3.0)",
            PriceMetric::Median => "((b.high + b.low) / 2.0)",
            PriceMetric::OhlcAvg => "((b.open + b.high + b.low + b.close) / 4.0)",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            PriceMetric::Open => "open",
            PriceMetric::High => "high",
            PriceMetric::Low => "low",
            PriceMetric::Close => "close",
            PriceMetric::Typical => "typical",
            PriceMetric::Median => "median",
            PriceMetric::OhlcAvg => "ohlc avg",
            PriceMetric::Vwap => "vwap",
        }
    }
    pub fn next(self) -> Self {
        let i = Self::ALL.iter().position(|m| *m == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }
}
