//! Whether a string is a valid Python identifier, by the rules of
//! <https://docs.python.org/3/reference/lexical_analysis.html> (pure Rust: XID_Start or `_`,
//! then XID_Continue; NFKC normalisation is left to the parser).

/// Is `s` a valid Python identifier?
pub fn isidentifier(s: impl AsRef<str>) -> Result<bool, String> {
    let mut c = s.as_ref().chars();
    Ok(match c.next() {
        Some(f) => (f == '_' || unicode_ident::is_xid_start(f)) && c.all(unicode_ident::is_xid_continue),
        None => false,
    })
}

/// Trait that determines if a string contains a valid identifer based on Python rules.
pub trait IsIdentifier: AsRef<str> {
    fn isidentifier(&self) -> Result<bool, String> {
        isidentifier(self.as_ref())
    }
}

/// Blanket implementation for this trait.
impl<T: AsRef<str>> IsIdentifier for T {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers() {
        assert!(isidentifier("alpha").unwrap());
        assert!("_x1".isidentifier().unwrap());
        assert!(!isidentifier("0alpha").unwrap());
        assert!(!"".isidentifier().unwrap());
    }
}
