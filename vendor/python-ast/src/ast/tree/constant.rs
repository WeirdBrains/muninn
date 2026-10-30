use std::fmt::*;

use litrs::Literal;
use tracing::debug;
use proc_macro2::*;
use quote::quote;

use crate::{CodeGen, CodeGenContext, PythonOptions, SymbolTableScopes};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub trait PyConstantTrait: Clone + Debug + PartialEq {
    type RustType;
}

#[derive(Clone, Debug, PartialEq)]
#[repr(transparent)]
pub struct Constant(pub Option<Literal<String>>);

impl Serialize for Constant {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.to_string().as_str())
    }
}

impl<'de> Deserialize<'de> for Constant {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let l = Literal::parse(s).expect("[3] Parsing the literal");
        Ok(Self(Some(l)))
    }
}

impl std::string::ToString for Constant {
    fn to_string(&self) -> String {
        match self.0.clone() {
            Some(c) => c.to_string(),
            None => "None".to_string(),
        }
    }
}






// This will mostly be invoked when the input is None.

// This is the fun bit of code that is responsible from converting from Python constants to Rust ones.

impl CodeGen for Constant {
    type Context = CodeGenContext;
    type Options = PythonOptions;
    type SymbolTable = SymbolTableScopes;

    fn to_rust(
        self,
        _ctx: Self::Context,
        _options: Self::Options,
        _symbols: Self::SymbolTable,
    ) -> std::result::Result<TokenStream, Box<dyn std::error::Error>> {
        match self.0 {
            Some(c) => {
                let v: TokenStream = c
                    .to_string()
                    .parse()
                    .map_err(|e| format!("cannot render constant `{}` as Rust tokens: {}", c, e))?;
                Ok(quote!(#v))
            }
            None => Ok(quote!(None)),
        }
    }
}

#[cfg(test)]
mod tests {
    use test_log::test;
    //use super::*;
    use crate::{symbols::SymbolTableScopes, CodeGen};
    use tracing::debug;

    #[test]
    fn parse_string() {
        let s = crate::parse("'I ate a bug'", "test.py").unwrap();
        let ast = s
            .to_rust(
                crate::CodeGenContext::Module("test".to_string()),
                crate::PythonOptions::default(),
                SymbolTableScopes::new(),
            )
            .unwrap();
        debug!("ast: {}", ast.to_string());

        assert_eq!("use stdpython :: * ; \"I ate a bug\"", ast.to_string());
    }

    #[test]
    fn parse_bytes() {
        let s = crate::parse("b'I ate a bug'", "test.py").unwrap();
        let ast = s
            .to_rust(
                crate::CodeGenContext::Module("test".to_string()),
                crate::PythonOptions::default(),
                SymbolTableScopes::new(),
            )
            .unwrap();

        assert_eq!("use stdpython :: * ; b\"I ate a bug\"", ast.to_string());
    }

    #[test]
    fn parse_number_int() {
        let s = crate::parse("871234234", "test.py").unwrap();
        let ast = s
            .to_rust(
                crate::CodeGenContext::Module("test".to_string()),
                crate::PythonOptions::default(),
                SymbolTableScopes::new(),
            )
            .unwrap();

        assert_eq!("use stdpython :: * ; 871234234", ast.to_string());
    }

    #[test]
    fn parse_number_neg_int() {
        let s = crate::parse("-871234234", "test.py").unwrap();
        let ast = s
            .to_rust(
                crate::CodeGenContext::Module("test".to_string()),
                crate::PythonOptions::default(),
                SymbolTableScopes::new(),
            )
            .unwrap();

        assert_eq!("use stdpython :: * ; - 871234234", ast.to_string());
    }

    #[test]
    fn parse_number_float() {
        let s = crate::parse("87123.4234", "test.py").unwrap();
        let ast = s
            .to_rust(
                crate::CodeGenContext::Module("test".to_string()),
                crate::PythonOptions::default(),
                SymbolTableScopes::new(),
            )
            .unwrap();

        assert_eq!("use stdpython :: * ; 87123.4234", ast.to_string());
    }

    #[test]
    fn parse_bool() {
        let s = crate::parse("True", "test.py").unwrap();
        let ast = s
            .to_rust(
                crate::CodeGenContext::Module("test".to_string()),
                crate::PythonOptions::default(),
                SymbolTableScopes::new(),
            )
            .unwrap();

        assert_eq!("use stdpython :: * ; true", ast.to_string());
    }

    #[test]
    fn parse_none() {
        let s = crate::parse("None", "test.py").unwrap();
        let ast = s
            .to_rust(
                crate::CodeGenContext::Module("test".to_string()),
                crate::PythonOptions::default(),
                SymbolTableScopes::new(),
            )
            .unwrap();

        assert_eq!("use stdpython :: * ; None", ast.to_string());
    }
}

// Pure-Rust constructors (vendored for muninn): these replace the pyo3
// `try_*` extractors and render each value exactly as they did.
impl Constant {
    pub fn none() -> Self {
        Self(None)
    }
    pub fn string(v: &str) -> Self {
        Self(Some(Literal::parse(format!("{:?}", v)).expect("string literal")))
    }
    pub fn bytes(v: &[u8]) -> Self {
        let escaped: String = v
            .iter()
            .flat_map(|b| std::ascii::escape_default(*b))
            .map(char::from)
            .collect();
        Self(Some(Literal::parse(format!("b\"{}\"", escaped)).expect("bytes literal")))
    }
    pub fn int(v: i64) -> Self {
        Self(Some(Literal::parse(format!("{}", v)).expect("int literal")))
    }
    pub fn float(v: f64) -> Self {
        let mut s = format!("{}", v);
        if v.is_finite() && !s.contains('.') && !s.contains('e') && !s.contains('E') {
            s.push_str(".0");
        }
        Self(Some(Literal::parse(s).expect("float literal")))
    }
    pub fn bool(v: bool) -> Self {
        Self(Some(Literal::parse(format!("{}", v)).expect("bool literal")))
    }
}
