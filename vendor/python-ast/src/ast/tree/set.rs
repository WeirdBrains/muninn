use proc_macro2::TokenStream;
use quote::quote;
use serde::{Deserialize, Serialize};

use crate::{
    CodeGen, CodeGenContext, ExprType, PythonOptions, SymbolTableScopes,
    Node, impl_node_with_positions
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Set {
    pub elts: Vec<ExprType>,
    pub lineno: Option<usize>,
    pub col_offset: Option<usize>,
    pub end_lineno: Option<usize>,
    pub end_col_offset: Option<usize>,
}


impl_node_with_positions!(Set { lineno, col_offset, end_lineno, end_col_offset });

impl CodeGen for Set {
    type Context = CodeGenContext;
    type Options = PythonOptions;
    type SymbolTable = SymbolTableScopes;

    fn to_rust(
        self,
        ctx: Self::Context,
        options: Self::Options,
        symbols: Self::SymbolTable,
    ) -> Result<TokenStream, Box<dyn std::error::Error>> {
        let elements: Result<Vec<_>, _> = self.elts
            .into_iter()
            .map(|elt| elt.to_rust(ctx.clone(), options.clone(), symbols.clone()))
            .collect();
        
        let elements = elements?;
        
        Ok(quote! {
            std::collections::HashSet::from([#(#elements),*])
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::create_parse_test;

    create_parse_test!(test_empty_set, "set()", "set_test.py");
    create_parse_test!(test_simple_set, "{1, 2, 3}", "set_test.py");
    create_parse_test!(test_set_with_variables, "{x, y, z}", "set_test.py");
}