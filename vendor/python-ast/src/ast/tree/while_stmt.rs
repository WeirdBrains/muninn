use proc_macro2::TokenStream;
use quote::quote;
use serde::{Deserialize, Serialize};

use crate::{
    CodeGen, CodeGenContext, ExprType, PythonOptions, SymbolTableScopes,
    Node, impl_node_with_positions
};

use super::Statement;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct While {
    pub test: ExprType,
    pub body: Vec<Statement>,
    pub orelse: Vec<Statement>,
    pub lineno: Option<usize>,
    pub col_offset: Option<usize>,
    pub end_lineno: Option<usize>,
    pub end_col_offset: Option<usize>,
}


impl_node_with_positions!(While { lineno, col_offset, end_lineno, end_col_offset });

impl CodeGen for While {
    type Context = CodeGenContext;
    type Options = PythonOptions;
    type SymbolTable = SymbolTableScopes;

    fn find_symbols(self, symbols: Self::SymbolTable) -> Self::SymbolTable {
        let symbols = self.test.find_symbols(symbols);
        let symbols = self.body.into_iter().fold(symbols, |acc, stmt| stmt.find_symbols(acc));
        self.orelse.into_iter().fold(symbols, |acc, stmt| stmt.find_symbols(acc))
    }

    fn to_rust(
        self,
        ctx: Self::Context,
        options: Self::Options,
        symbols: Self::SymbolTable,
    ) -> Result<TokenStream, Box<dyn std::error::Error>> {
        let test =
            crate::condition_to_rust(&self.test, ctx.clone(), options.clone(), symbols.clone())?;

        let has_else = !self.orelse.is_empty();
        // Break-tracking is only needed when the else clause could be skipped
        // by a break belonging to this loop; otherwise the flag would be
        // declared mutable but never written.
        let tracks_break = has_else && crate::loop_body_has_direct_break(&self.body);
        // Body statements compile inside a Loop context so `break` can honor
        // the else clause; the test and else clause are outside the loop.
        let body_ctx = crate::CodeGenContext::Loop {
            has_else: tracks_break,
            parent: Box::new(ctx.clone()),
        };
        let body_stmts: Result<Vec<_>, _> = self.body
            .into_iter()
            .map(|stmt| stmt.to_rust(body_ctx.clone(), options.clone(), symbols.clone()))
            .collect();
        let body_stmts = body_stmts?;

        if !has_else {
            Ok(quote! {
                while #test {
                    #(#body_stmts;)*
                }
            })
        } else {
            // Python's while/else: the else clause runs iff the loop exited
            // because the condition became false, not via `break`.
            let else_stmts: Result<Vec<_>, _> = self.orelse
                .into_iter()
                .map(|stmt| stmt.to_rust(ctx.clone(), options.clone(), symbols.clone()))
                .collect();
            let else_stmts = else_stmts?;

            if tracks_break {
                Ok(quote! {
                    {
                        let mut __rython_broke = false;
                        while #test {
                            #(#body_stmts;)*
                        }
                        if !__rython_broke {
                            #(#else_stmts;)*
                        }
                    }
                })
            } else {
                // No break can skip the else clause; run it unconditionally.
                Ok(quote! {
                    {
                        while #test {
                            #(#body_stmts;)*
                        }
                        #(#else_stmts;)*
                    }
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::create_parse_test;

    create_parse_test!(test_simple_while, "while x > 0:\n    x -= 1", "while_test.py");
    create_parse_test!(test_while_else, "while x > 0:\n    x -= 1\nelse:\n    print('done')", "while_test.py");
    create_parse_test!(test_while_true, "while True:\n    break", "while_test.py");
}