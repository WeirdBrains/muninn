//! The module defines Python-syntax arguments and maps them into Rust-syntax versions.
use proc_macro2::TokenStream;
use quote::quote;
use serde::{Deserialize, Serialize};

use crate::{
    CodeGen, CodeGenContext, ExprType, Node, PythonOptions, SymbolTableScopes,
};

/// A complete argument representation that can hold any Python expression.
/// This replaces the limited Arg enum to support all argument types.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Argument {
    /// The argument expression (can be any valid Python expression)
    pub value: ExprType,
    /// Position information
    pub lineno: Option<usize>,
    pub col_offset: Option<usize>,
    pub end_lineno: Option<usize>,
    pub end_col_offset: Option<usize>,
}

/// An argument value that can be any expression.
/// This replaces the old limited Arg enum.
pub type Arg = ExprType;

/// A function parameter definition with optional type annotation and default value.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Parameter {
    /// Parameter name
    pub arg: String,
    /// Optional type annotation
    pub annotation: Option<Box<ExprType>>,
    /// Optional type comment (deprecated Python feature)
    pub type_comment: Option<String>,
    /// Position information
    pub lineno: Option<usize>,
    pub col_offset: Option<usize>,
    pub end_lineno: Option<usize>,
    pub end_col_offset: Option<usize>,
}

/// Comprehensive function arguments structure supporting all Python argument types.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Arguments {
    /// Positional-only parameters (before / in Python 3.8+)
    pub posonlyargs: Vec<Parameter>,
    /// Regular positional parameters
    pub args: Vec<Parameter>,
    /// Variable positional parameter (*args)
    pub vararg: Option<Parameter>,
    /// Keyword-only parameters (after * or *args)
    pub kwonlyargs: Vec<Parameter>,
    /// Default values for keyword-only parameters (None = required)
    pub kw_defaults: Vec<Option<Box<ExprType>>>,
    /// Variable keyword parameter (**kwargs)
    pub kwarg: Option<Parameter>,
    /// Default values for regular positional parameters
    pub defaults: Vec<Box<ExprType>>,
}


/// Function call arguments supporting all Python call patterns.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct CallArguments {
    /// Positional arguments
    pub args: Vec<ExprType>,
    /// Keyword arguments
    pub keywords: Vec<crate::Keyword>,
}

// Implementation for new Argument struct

impl CodeGen for Argument {
    type Context = CodeGenContext;
    type Options = PythonOptions;
    type SymbolTable = SymbolTableScopes;

    fn to_rust(
        self,
        ctx: Self::Context,
        options: Self::Options,
        symbols: Self::SymbolTable,
    ) -> std::result::Result<TokenStream, Box<dyn std::error::Error>> {
        self.value.to_rust(ctx, options, symbols)
    }
}

// Implementation for Parameter struct

/// Whether an annotation means "optional": `Optional[T]` or a union with
/// None (`T | None`). Optional-annotated names hold an Option, and stores
/// into them wrap in Some.
pub(crate) fn is_optional_annotation(ann: &ExprType) -> bool {
    match ann {
        ExprType::Subscript(sub) => {
            matches!(sub.value.as_ref(), ExprType::Name(n) if n.id == "Optional")
        }
        ExprType::BinOp(op) if matches!(op.op, crate::BinOps::BitOr) => {
            crate::is_none_expr(&op.left) || crate::is_none_expr(&op.right)
        }
        _ => false,
    }
}

/// Map a Python type annotation to a Rust type, when the mapping is known.
/// `int`/`float`/`str`/`bool`/`bytes` map to concrete Rust types, and
/// `list[T]`/`dict[K, V]`/`set[T]` map to the corresponding std containers
/// when their element annotations map too. `Optional[T]` / `T | None` map
/// to `Option<T>`.
pub fn python_annotation_to_rust_type(annotation: &ExprType) -> Option<TokenStream> {
    match annotation {
        // T | None (and None | T) is Option<T>.
        ExprType::BinOp(op) if matches!(op.op, crate::BinOps::BitOr) => {
            let inner = if crate::is_none_expr(&op.left) {
                op.right.as_ref()
            } else if crate::is_none_expr(&op.right) {
                op.left.as_ref()
            } else {
                return None;
            };
            let inner = python_annotation_to_rust_type(inner)?;
            return Some(quote!(Option<#inner>));
        }
        _ => {}
    }
    match annotation {
        ExprType::Name(name) => match name.id.as_str() {
            "int" => Some(quote!(i64)),
            "float" => Some(quote!(f64)),
            "str" => Some(quote!(String)),
            "bool" => Some(quote!(bool)),
            "bytes" => Some(quote!(Vec<u8>)),
            _ => None,
        },
        // Subscripted generics over known element types: list[int] and
        // friends map to the concrete Rust containers codegen produces for
        // the corresponding literals.
        ExprType::Subscript(sub) => {
            let container = match sub.value.as_ref() {
                ExprType::Name(n) => n.id.as_str(),
                _ => return None,
            };
            match (&sub.kind, container) {
                (crate::SubscriptKind::Index(elt), "Optional") => {
                    let inner = python_annotation_to_rust_type(elt)?;
                    Some(quote!(Option<#inner>))
                }
                (crate::SubscriptKind::Index(elt), "list") => {
                    let inner = python_annotation_to_rust_type(elt)?;
                    Some(quote!(Vec<#inner>))
                }
                (crate::SubscriptKind::Index(elt), "set" | "frozenset") => {
                    let inner = python_annotation_to_rust_type(elt)?;
                    Some(quote!(std::collections::HashSet<#inner>))
                }
                (crate::SubscriptKind::Index(kv), "dict") => {
                    // dict[K, V] parses as a subscript with a tuple index.
                    // PyDict is the insertion-ordered map dict literals
                    // lower to.
                    if let ExprType::Tuple(t) = kv.as_ref() {
                        if let [k, v] = t.elts.as_slice() {
                            let k = python_annotation_to_rust_type(k)?;
                            let v = python_annotation_to_rust_type(v)?;
                            return Some(quote!(PyDict<#k, #v>));
                        }
                    }
                    None
                }
                _ => None,
            }
        }
        _ => None,
    }
}

impl CodeGen for Parameter {
    type Context = CodeGenContext;
    type Options = PythonOptions;
    type SymbolTable = SymbolTableScopes;

    fn to_rust(
        self,
        ctx: Self::Context,
        options: Self::Options,
        symbols: Self::SymbolTable,
    ) -> std::result::Result<TokenStream, Box<dyn std::error::Error>> {

        let param_name = crate::safe_ident(&self.arg);

        // Generate type annotation if present
        if let Some(annotation) = self.annotation {
            // A str parameter accepts anything convertible to String, so
            // call sites can pass &str literals as well as owned Strings;
            // the function prologue converts it (`let s: String = s.into()`).
            if matches!(&*annotation, ExprType::Name(n) if n.id == "str") {
                return Ok(quote!(#param_name: impl Into<String>));
            }
            // Known Python types map to concrete Rust types; anything else
            // falls back to rendering the annotation expression (e.g. a
            // user-defined class name).
            let rust_type = match python_annotation_to_rust_type(&annotation) {
                Some(mapped) => mapped,
                None => annotation.to_rust(ctx, options, symbols)?,
            };
            Ok(quote!(#param_name: #rust_type))
        } else {
            // Default to generic type for untyped parameters
            Ok(quote!(#param_name: impl Into<PyObject>))
        }
    }
}

// Implementation for Arguments struct

impl CodeGen for Arguments {
    type Context = CodeGenContext;
    type Options = PythonOptions;
    type SymbolTable = SymbolTableScopes;

    fn to_rust(
        self,
        ctx: Self::Context,
        options: Self::Options,
        symbols: Self::SymbolTable,
    ) -> std::result::Result<TokenStream, Box<dyn std::error::Error>> {
        let mut params = Vec::new();
        
        // Process positional-only arguments
        for arg in self.posonlyargs {
            let param = arg.to_rust(ctx.clone(), options.clone(), symbols.clone())?;
            params.push(param);
        }
        
        // Process regular positional arguments. Defaulted parameters lower
        // to plain required parameters: Rust has no default arguments, and
        // the old Option<T> wrapping neither type-checked against bodies
        // that use the parameter directly nor matched call sites (which
        // never wrapped values in Some). Callers that omit the argument
        // fail to compile either way; callers that pass it now work.
        for arg in self.args {
            let param = arg.to_rust(ctx.clone(), options.clone(), symbols.clone())?;
            params.push(param);
        }
        
        // Process *args
        if let Some(vararg) = self.vararg {
            let vararg_name = crate::safe_ident(&vararg.arg);
            params.push(quote!(#vararg_name: impl IntoIterator<Item = impl Into<PyObject>>));
        }
        
        // Process keyword-only arguments. Like positional defaults above,
        // these lower to plain required parameters.
        for arg in self.kwonlyargs {
            let param = arg.to_rust(ctx.clone(), options.clone(), symbols.clone())?;
            params.push(param);
        }
        
        // Process **kwargs
        if let Some(kwarg) = self.kwarg {
            let kwarg_name = crate::safe_ident(&kwarg.arg);
            params.push(quote!(#kwarg_name: impl IntoIterator<Item = (impl AsRef<str>, impl Into<PyObject>)>));
        }
        
        Ok(quote!(#(#params),*))
    }
}


// Implementation for CallArguments

impl CodeGen for CallArguments {
    type Context = CodeGenContext;
    type Options = PythonOptions;
    type SymbolTable = SymbolTableScopes;

    fn to_rust(
        self,
        ctx: Self::Context,
        options: Self::Options,
        symbols: Self::SymbolTable,
    ) -> std::result::Result<TokenStream, Box<dyn std::error::Error>> {
        let mut all_args = Vec::new();
        
        // Add positional arguments
        for arg in self.args {
            let rust_arg = arg.to_rust(ctx.clone(), options.clone(), symbols.clone())?;
            all_args.push(rust_arg);
        }
        
        // Add keyword arguments
        for keyword in self.keywords {
            let rust_kw = keyword.to_rust(ctx.clone(), options.clone(), symbols.clone())?;
            all_args.push(rust_kw);
        }
        
        Ok(quote!(#(#all_args),*))
    }
}


// Node trait implementations for position tracking
impl Node for Argument {
    fn lineno(&self) -> Option<usize> { self.lineno }
    fn col_offset(&self) -> Option<usize> { self.col_offset }
    fn end_lineno(&self) -> Option<usize> { self.end_lineno }
    fn end_col_offset(&self) -> Option<usize> { self.end_col_offset }
}

impl Node for Parameter {
    fn lineno(&self) -> Option<usize> { self.lineno }
    fn col_offset(&self) -> Option<usize> { self.col_offset }
    fn end_lineno(&self) -> Option<usize> { self.end_lineno }
    fn end_col_offset(&self) -> Option<usize> { self.end_col_offset }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parse, CodeGenContext, ExprType, PythonOptions, SymbolTableScopes};
    use test_log::test;

    #[test]
    fn test_simple_function_call() {
        let code = "func(1, 2, 3)";
        let result = parse(code, "test.py").unwrap();
        
        // Generate Rust code
        let options = PythonOptions::default();
        let symbols = SymbolTableScopes::new();
        let _rust_code = result.to_rust(
            CodeGenContext::Module("test".to_string()),
            options,
            symbols,
        ).unwrap();
        
        // Should generate function call with positional arguments
    }

    #[test]
    fn test_keyword_arguments() {
        // Keywords resolve against the callee's signature and land in
        // parameter order.
        let code = "def func(a, b):\n    pass\n\nfunc(b=2, a=1)";
        let result = parse(code, "test.py").unwrap();

        let options = PythonOptions::default();
        let symbols = result.clone().find_symbols(SymbolTableScopes::new());
        let rust_code = result.to_rust(
            CodeGenContext::Module("test".to_string()),
            options,
            symbols,
        ).unwrap().to_string();
        assert!(rust_code.contains("func (1 , 2)"), "generated: {}", rust_code);
    }

    #[test]
    fn test_mixed_arguments() {
        let code = "def func(a, b, c, d):\n    pass\n\nfunc(1, 2, d=4, c=3)";
        let result = parse(code, "test.py").unwrap();

        let options = PythonOptions::default();
        let symbols = result.clone().find_symbols(SymbolTableScopes::new());
        let rust_code = result.to_rust(
            CodeGenContext::Module("test".to_string()),
            options,
            symbols,
        ).unwrap().to_string();
        assert!(rust_code.contains("func (1 , 2 , 3 , 4)"), "generated: {}", rust_code);
    }

    #[test]
    fn test_function_with_defaults() {
        let code = r#"
def func(a, b=2, c=3):
    pass
        "#;
        let result = parse(code, "test.py").unwrap();
        
        let options = PythonOptions::default();
        let symbols = SymbolTableScopes::new();
        let _rust_code = result.to_rust(
            CodeGenContext::Module("test".to_string()),
            options,
            symbols,
        ).unwrap();
        
        // Should generate function with optional parameters
    }

    #[test]
    fn test_function_with_varargs() {
        let code = r#"
def func(a, *args):
    pass
        "#;
        let result = parse(code, "test.py").unwrap();
        
        let options = PythonOptions::default();
        let symbols = SymbolTableScopes::new();
        let _rust_code = result.to_rust(
            CodeGenContext::Module("test".to_string()),
            options,
            symbols,
        ).unwrap();
        
        // Should generate function with variable arguments
    }

    #[test]
    fn test_function_with_kwargs() {
        let code = r#"
def func(a, **kwargs):
    pass
        "#;
        let result = parse(code, "test.py").unwrap();
        
        let options = PythonOptions::default();
        let symbols = SymbolTableScopes::new();
        let _rust_code = result.to_rust(
            CodeGenContext::Module("test".to_string()),
            options,
            symbols,
        ).unwrap();
        
        // Should generate function with keyword arguments dict
    }

    #[test]
    fn test_complex_function_signature() {
        let code = r#"
def func(a, b=2, *args, c, d=4, **kwargs):
    pass
        "#;
        let result = parse(code, "test.py").unwrap();
        
        let options = PythonOptions::default();
        let symbols = SymbolTableScopes::new();
        let _rust_code = result.to_rust(
            CodeGenContext::Module("test".to_string()),
            options,
            symbols,
        ).unwrap();
        
        // Should generate function with all argument types
    }

    #[test]
    fn test_keyword_only_arguments() {
        let code = r#"
def func(a, *, b, c=3):
    pass
        "#;
        let result = parse(code, "test.py").unwrap();
        
        let options = PythonOptions::default();
        let symbols = SymbolTableScopes::new();
        let _rust_code = result.to_rust(
            CodeGenContext::Module("test".to_string()),
            options,
            symbols,
        ).unwrap();
        
        // Should generate function with keyword-only arguments
    }

    #[test]
    fn test_argument_unpacking_call() {
        // Note: This would require additional AST node support for Starred expressions
        let code = "func(*args, **kwargs)";
        let result = parse(code, "test.py");
        
        match result {
            Ok(ast) => {
                let options = PythonOptions::default();
                let symbols = SymbolTableScopes::new();
                let rust_code = ast.to_rust(
                    CodeGenContext::Module("test".to_string()),
                    options,
                    symbols,
                );
                
                match rust_code {
                    Ok(_code) => { /* Code generation succeeded as expected */ },
                    Err(_e) => { /* Expected error for unimplemented feature */ },
                }
            }
            Err(_e) => { /* Parse error expected for unimplemented features */ },
        }
    }

    #[test]
    fn test_arg_with_constant() {
        // Test that Arg (now ExprType) works with constants
        use litrs::Literal;
        let literal = Literal::parse("42").unwrap().into_owned();
        let constant = crate::Constant(Some(literal));
        let arg: Arg = ExprType::Constant(constant);
        
        let options = PythonOptions::default();
        let symbols = SymbolTableScopes::new();
        let rust_code = arg.to_rust(
            CodeGenContext::Module("test".to_string()),
            options,
            symbols,
        ).unwrap();
        
        assert!(rust_code.to_string().contains("42"));
    }

    #[test]
    fn test_arg_with_name() {
        // Test that Arg (now ExprType) works with name expressions
        let name_expr = ExprType::Name(crate::Name {
            id: "variable".to_string(),
        });
        let arg: Arg = name_expr;
        
        let options = PythonOptions::default();
        let symbols = SymbolTableScopes::new();
        let rust_code = arg.to_rust(
            CodeGenContext::Module("test".to_string()),
            options,
            symbols,
        ).unwrap();
        
        assert!(rust_code.to_string().contains("variable"));
    }
}
