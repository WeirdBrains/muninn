//! Helpers for generated code.

/// Build an identifier for generated code from a Python name, escaping Rust
/// keywords so identifiers like `type`, `match`, or `move` don't produce
/// invalid Rust. Keywords that cannot be raw identifiers (`self`, `Self`,
/// `super`, `crate`) get a trailing underscore instead — except `self`,
/// which is left as-is so method receivers keep working.
pub fn safe_ident(name: &str) -> proc_macro2::Ident {
    use quote::format_ident;
    match name {
        "self" => format_ident!("{}", name),
        "Self" | "super" | "crate" => format_ident!("{}_", name),
        // Strict and reserved keywords, all legal as raw identifiers.
        "as" | "break" | "const" | "continue" | "dyn" | "else" | "enum" | "extern" | "false"
        | "fn" | "for" | "if" | "impl" | "in" | "let" | "loop" | "match" | "mod" | "move"
        | "mut" | "pub" | "ref" | "return" | "static" | "struct" | "trait" | "true" | "type"
        | "unsafe" | "use" | "where" | "while" | "async" | "await" | "abstract" | "become"
        | "box" | "do" | "final" | "macro" | "override" | "priv" | "try" | "typeof"
        | "unsized" | "virtual" | "yield" | "gen" => format_ident!("r#{}", name),
        _ => format_ident!("{}", name),
    }
}

