use proc_macro2::TokenStream;
use quote::quote;
use serde::{Deserialize, Serialize};

use crate::python_ast::{CodeGen, CodeGenContext, ExprType, Node, ParameterList, PythonOptions, SymbolTableScopes, impl_node_with_positions};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Lambda {
    pub args: ParameterList,
    pub body: Box<ExprType>,
    pub lineno: Option<usize>,
    pub col_offset: Option<usize>,
    pub end_lineno: Option<usize>,
    pub end_col_offset: Option<usize>,
}

impl_node_with_positions!(Lambda { lineno, col_offset, end_lineno, end_col_offset });

impl CodeGen for Lambda {
    type Context = CodeGenContext;
    type Options = PythonOptions;
    type SymbolTable = SymbolTableScopes;

    fn to_rust(self, ctx: Self::Context, options: Self::Options, symbols: Self::SymbolTable) -> Result<TokenStream, Box<dyn std::error::Error>> {
        // Closure parameters must be bare names — `impl Trait` (which typed
        // function parameters use) is illegal in closure position; Rust
        // infers closure parameter types.
        let params: Vec<_> = self.args.args.iter().map(|param| crate::python_ast::safe_ident(&param.arg)).collect();
        let body = self.body.to_rust(ctx, options, symbols)?;

        Ok(quote! {
            |#(#params),*| #body
        })
    }
}

#[cfg(python_ast_upstream_tests)]
mod tests {
    use super::*;
    use crate::python_ast::create_parse_test;

    create_parse_test!(test_simple_lambda, "lambda x: x + 1", "lambda_test.py");
    create_parse_test!(test_lambda_with_args, "lambda x, y: x * y", "lambda_test.py");
    create_parse_test!(test_lambda_no_args, "lambda: 42", "lambda_test.py");
}
