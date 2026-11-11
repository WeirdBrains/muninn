//! A small JSON value with a parser and a writer: what the host and plugins exchange.
use std::fmt::{self, Write as _};

/// A JSON value. Objects keep their key order.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Json {
    #[default]
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    /// An empty object.
    pub fn obj() -> Json {
        Json::Obj(Vec::new())
    }
    /// Add or replace a key (on objects; anything else becomes an object).
    pub fn with(mut self, key: &str, v: impl Into<Json>) -> Json {
        self.set(key, v);
        self
    }
    pub fn set(&mut self, key: &str, v: impl Into<Json>) {
        if !matches!(self, Json::Obj(_)) {
            *self = Json::obj();
        }
        if let Json::Obj(o) = self {
            let v = v.into();
            match o.iter_mut().find(|(k, _)| k == key) {
                Some(slot) => slot.1 = v,
                None => o.push((key.to_string(), v)),
            }
        }
    }
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(o) => o.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn is_null(&self) -> bool {
        matches!(self, Json::Null)
    }
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Num(n) => Some(*n),
            Json::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            Json::Str(s) => s.trim().parse().ok(),
            _ => None,
        }
    }
    pub fn as_i64(&self) -> Option<i64> {
        self.as_f64().map(|f| f as i64)
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            Json::Num(n) => Some(*n != 0.0),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_array(&self) -> Option<&[Json]> {
        match self {
            Json::Arr(a) => Some(a),
            _ => None,
        }
    }
    pub fn as_object(&self) -> Option<&[(String, Json)]> {
        match self {
            Json::Obj(o) => Some(o),
            _ => None,
        }
    }
    /// Text for tables: strings as they are, numbers without a trailing `.0`, null empty.
    pub fn text(&self) -> String {
        match self {
            Json::Null => String::new(),
            Json::Str(s) => s.clone(),
            other => other.to_string(),
        }
    }

    /// Parse JSON text.
    pub fn parse(s: &str) -> Result<Json, String> {
        let mut p = Parser { b: s.as_bytes(), i: 0 };
        p.ws();
        let v = p.value(0)?;
        p.ws();
        if p.i != p.b.len() {
            return Err(format!("unexpected text at byte {}", p.i));
        }
        Ok(v)
    }
}

impl From<f64> for Json {
    fn from(v: f64) -> Self {
        Json::Num(v)
    }
}
impl From<i64> for Json {
    fn from(v: i64) -> Self {
        Json::Num(v as f64)
    }
}
impl From<usize> for Json {
    fn from(v: usize) -> Self {
        Json::Num(v as f64)
    }
}
impl From<bool> for Json {
    fn from(v: bool) -> Self {
        Json::Bool(v)
    }
}
impl From<&str> for Json {
    fn from(v: &str) -> Self {
        Json::Str(v.to_string())
    }
}
impl From<String> for Json {
    fn from(v: String) -> Self {
        Json::Str(v)
    }
}
impl<T: Into<Json>> From<Vec<T>> for Json {
    fn from(v: Vec<T>) -> Self {
        Json::Arr(v.into_iter().map(Into::into).collect())
    }
}
impl<T: Into<Json>> From<Option<T>> for Json {
    fn from(v: Option<T>) -> Self {
        v.map(Into::into).unwrap_or(Json::Null)
    }
}

fn write_str(f: &mut fmt::Formatter<'_>, s: &str) -> fmt::Result {
    f.write_char('"')?;
    for c in s.chars() {
        match c {
            '"' => f.write_str("\\\"")?,
            '\\' => f.write_str("\\\\")?,
            '\n' => f.write_str("\\n")?,
            '\r' => f.write_str("\\r")?,
            '\t' => f.write_str("\\t")?,
            c if (c as u32) < 0x20 => write!(f, "\\u{:04x}", c as u32)?,
            c => f.write_char(c)?,
        }
    }
    f.write_char('"')
}

impl fmt::Display for Json {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Json::Null => f.write_str("null"),
            Json::Bool(b) => write!(f, "{b}"),
            Json::Num(n) if !n.is_finite() => f.write_str("null"),
            Json::Num(n) if n.fract() == 0.0 && n.abs() < 9.0e15 => write!(f, "{}", *n as i64),
            Json::Num(n) => write!(f, "{n}"),
            Json::Str(s) => write_str(f, s),
            Json::Arr(a) => {
                f.write_char('[')?;
                for (i, v) in a.iter().enumerate() {
                    if i > 0 {
                        f.write_char(',')?;
                    }
                    write!(f, "{v}")?;
                }
                f.write_char(']')
            }
            Json::Obj(o) => {
                f.write_char('{')?;
                for (i, (k, v)) in o.iter().enumerate() {
                    if i > 0 {
                        f.write_char(',')?;
                    }
                    write_str(f, k)?;
                    write!(f, ":{v}")?;
                }
                f.write_char('}')
            }
        }
    }
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.i < self.b.len() && matches!(self.b[self.i], b' ' | b'\n' | b'\r' | b'\t') {
            self.i += 1;
        }
    }
    fn err<T>(&self, what: &str) -> Result<T, String> {
        Err(format!("{what} at byte {}", self.i))
    }
    fn eat(&mut self, lit: &str) -> bool {
        if self.b[self.i..].starts_with(lit.as_bytes()) {
            self.i += lit.len();
            true
        } else {
            false
        }
    }
    fn value(&mut self, depth: usize) -> Result<Json, String> {
        if depth > 128 {
            return self.err("nested too deeply");
        }
        self.ws();
        let Some(&c) = self.b.get(self.i) else { return self.err("unexpected end") };
        match c {
            b'n' if self.eat("null") => Ok(Json::Null),
            b't' if self.eat("true") => Ok(Json::Bool(true)),
            b'f' if self.eat("false") => Ok(Json::Bool(false)),
            b'"' => self.string().map(Json::Str),
            b'[' => {
                self.i += 1;
                let mut v = Vec::new();
                self.ws();
                if self.eat("]") {
                    return Ok(Json::Arr(v));
                }
                loop {
                    v.push(self.value(depth + 1)?);
                    self.ws();
                    if self.eat(",") {
                        continue;
                    }
                    if self.eat("]") {
                        return Ok(Json::Arr(v));
                    }
                    return self.err("expected , or ]");
                }
            }
            b'{' => {
                self.i += 1;
                let mut o = Vec::new();
                self.ws();
                if self.eat("}") {
                    return Ok(Json::Obj(o));
                }
                loop {
                    self.ws();
                    if self.b.get(self.i) != Some(&b'"') {
                        return self.err("expected a key");
                    }
                    let k = self.string()?;
                    self.ws();
                    if !self.eat(":") {
                        return self.err("expected :");
                    }
                    let v = self.value(depth + 1)?;
                    o.push((k, v));
                    self.ws();
                    if self.eat(",") {
                        continue;
                    }
                    if self.eat("}") {
                        return Ok(Json::Obj(o));
                    }
                    return self.err("expected , or }");
                }
            }
            b'-' | b'0'..=b'9' => {
                let start = self.i;
                while self.i < self.b.len() && matches!(self.b[self.i], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') {
                    self.i += 1;
                }
                let s = std::str::from_utf8(&self.b[start..self.i]).unwrap_or("");
                s.parse::<f64>().map(Json::Num).map_err(|_| format!("bad number `{s}` at byte {start}"))
            }
            _ => self.err("unexpected character"),
        }
    }
    fn hex4(&mut self) -> Result<u32, String> {
        let h = self.b.get(self.i..self.i + 4).ok_or("short \\u escape")?;
        let s = std::str::from_utf8(h).map_err(|_| "bad \\u escape")?;
        self.i += 4;
        u32::from_str_radix(s, 16).map_err(|_| "bad \\u escape".to_string())
    }
    fn string(&mut self) -> Result<String, String> {
        self.i += 1;
        let mut out = String::new();
        loop {
            let start = self.i;
            while self.i < self.b.len() && self.b[self.i] != b'"' && self.b[self.i] != b'\\' {
                self.i += 1;
            }
            out.push_str(std::str::from_utf8(&self.b[start..self.i]).map_err(|_| "invalid UTF-8".to_string())?);
            match self.b.get(self.i) {
                None => return self.err("unterminated string"),
                Some(b'"') => {
                    self.i += 1;
                    return Ok(out);
                }
                _ => {
                    self.i += 1;
                    let Some(&e) = self.b.get(self.i) else { return self.err("unterminated escape") };
                    self.i += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let mut c = self.hex4()?;
                            if (0xD800..0xDC00).contains(&c) && self.eat("\\u") {
                                let lo = self.hex4()?;
                                c = 0x10000 + ((c - 0xD800) << 10) + (lo.wrapping_sub(0xDC00) & 0x3FF);
                            }
                            out.push(char::from_u32(c).unwrap_or('\u{FFFD}'));
                        }
                        _ => return self.err("bad escape"),
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Json;

    #[test]
    fn round_trip() {
        let s = r#"{"a":[1,2.5,-3e2,true,null],"b":"x\"y\n\u00e9\ud83d\ude00","c":{}}"#;
        let v = Json::parse(s).unwrap();
        assert_eq!(v.get("a").unwrap().as_array().unwrap()[2].as_f64(), Some(-300.0));
        assert_eq!(v.get("b").unwrap().as_str(), Some("x\"y\né😀"));
        let again = Json::parse(&v.to_string()).unwrap();
        assert_eq!(v, again);
        assert!(Json::parse("[1,").is_err());
        assert!(Json::parse("{} x").is_err());
        assert_eq!(Json::Num(f64::NAN).to_string(), "null");
        assert_eq!(Json::obj().with("k", 2.0).with("k", "v").to_string(), r#"{"k":"v"}"#);
    }
}
