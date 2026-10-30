use proc_macro2::TokenStream;
use quote::quote;

use crate::{CodeGen, CodeGenContext, ExprType, PythonOptions, SymbolTableScopes};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
//#[pyo3(transparent)]
pub struct Attribute {
    pub value: Box<ExprType>,
    pub attr: String,
    ctx: String,
}


impl<'a> CodeGen for Attribute {
    type Context = CodeGenContext;
    type Options = PythonOptions;
    type SymbolTable = SymbolTableScopes;

    fn to_rust(
        self,
        ctx: Self::Context,
        options: Self::Options,
        symbols: Self::SymbolTable,
    ) -> Result<TokenStream, Box<dyn std::error::Error>> {
        let value_tokens = self.value.to_rust(ctx, options, symbols)?;
        let value_str = value_tokens.to_string();
        let attr = crate::safe_ident(&self.attr);
        
        // Determine if this is a module access or a field/method access
        // Module names are typically lowercase and match Python stdlib modules
        let is_module_access = matches!(value_str.as_str(),
            "sys" | "os" | "subprocess" | "json" | "urllib" | "xml" | "asyncio" |
            "time" | "math" | "random" | "heapq" | "functools" | "textwrap" | "itertools" | "re" | "hashlib" | "csv" | "io" |
            // `datetime` covers both the runtime module and the datetime
            // TYPE from `from datetime import datetime` — either way the
            // attribute is a path item (datetime::strptime, datetime::now),
            // never a field on a value.
            "datetime" |
            "os :: path" | "os::path" // for nested modules
        );
        
        if is_module_access {
            // Use :: for module access (Python's sys.executable becomes sys::executable)
            // Special handling for LazyLock static variables that need
            // dereferencing. os::environ is NOT here: it is a live-view
            // unit struct whose methods auto-ref.
            let needs_deref = matches!((value_str.as_str(), self.attr.as_str()),
                ("sys", "executable") | ("sys", "argv")
            );
            
            if needs_deref {
                // Wrap dereferenced values in parentheses to ensure correct precedence
                // This prevents *sys::executable.to_string() and ensures (*sys::executable).to_string()
                Ok(quote!((*#value_tokens::#attr)))
            } else {
                Ok(quote!(#value_tokens::#attr))
            }
        } else {
            // Use . for field/method access (Python's obj.field becomes obj.field)
            Ok(quote!(#value_tokens.#attr))
        }
    }
}

impl Attribute {
    /// Builds an attribute node; `ctx` is the CPython context name
    /// ("Load", "Store" or "Del").
    pub fn new(value: ExprType, attr: impl Into<String>, ctx: impl Into<String>) -> Self {
        Self { value: Box::new(value), attr: attr.into(), ctx: ctx.into() }
    }
}
