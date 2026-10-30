use proc_macro2::TokenStream;
use quote::quote;
use serde::{Deserialize, Serialize};

use crate::{
    CodeGen, CodeGenContext, ExprType, Node, PythonOptions, Statement, SymbolTableScopes,
    };

/// Async for loop (async for target in iter: ...)
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AsyncFor {
    /// The target variable(s)
    pub target: ExprType,
    /// The iterable expression
    pub iter: ExprType,
    /// The body of the loop
    pub body: Vec<Statement>,
    /// The else clause (executed when the loop completes normally)
    pub orelse: Vec<Statement>,
    /// Position information
    pub lineno: Option<usize>,
    pub col_offset: Option<usize>,
    pub end_lineno: Option<usize>,
    pub end_col_offset: Option<usize>,
}


impl Node for AsyncFor {
    fn lineno(&self) -> Option<usize> { self.lineno }
    fn col_offset(&self) -> Option<usize> { self.col_offset }
    fn end_lineno(&self) -> Option<usize> { self.end_lineno }
    fn end_col_offset(&self) -> Option<usize> { self.end_col_offset }
}

impl CodeGen for AsyncFor {
    type Context = CodeGenContext;
    type Options = PythonOptions;
    type SymbolTable = SymbolTableScopes;

    fn find_symbols(self, symbols: Self::SymbolTable) -> Self::SymbolTable {
        // Process target, iter, body, and orelse
        let symbols = self.target.find_symbols(symbols);
        let symbols = self.iter.find_symbols(symbols);
        let symbols = self.body.into_iter().fold(symbols, |acc, stmt| stmt.find_symbols(acc));
        self.orelse.into_iter().fold(symbols, |acc, stmt| stmt.find_symbols(acc))
    }

    fn to_rust(
        self,
        ctx: Self::Context,
        options: Self::Options,
        symbols: Self::SymbolTable,
    ) -> Result<TokenStream, Box<dyn std::error::Error>> {
        let target = self.target.to_rust(ctx.clone(), options.clone(), symbols.clone())?;
        let iter_expr = self.iter.to_rust(ctx.clone(), options.clone(), symbols.clone())?;

        let has_else = !self.orelse.is_empty();
        // Break-tracking is only needed when a break belonging to this loop
        // could skip the else clause.
        let tracks_break = has_else && crate::loop_body_has_direct_break(&self.body);
        let body_ctx = crate::CodeGenContext::Loop {
            has_else: tracks_break,
            parent: Box::new(ctx.clone()),
        };
        let body_tokens: Result<Vec<TokenStream>, Box<dyn std::error::Error>> = self.body.into_iter()
            .map(|stmt| stmt.to_rust(body_ctx.clone(), options.clone(), symbols.clone()))
            .collect();
        let body_tokens = body_tokens?;

        // Full `async for` semantics need an async-stream protocol; until
        // then, iterate the expression synchronously. This preserves the
        // loop's iteration and target binding (previously the body ran once
        // and the iterator was discarded entirely).
        if !has_else {
            Ok(quote! {
                for #target in #iter_expr {
                    #(#body_tokens;)*
                }
            })
        } else {
            let else_body_tokens: Result<Vec<TokenStream>, Box<dyn std::error::Error>> = self.orelse.into_iter()
                .map(|stmt| stmt.to_rust(ctx.clone(), options.clone(), symbols.clone()))
                .collect();
            let else_body_tokens = else_body_tokens?;
            if tracks_break {
                Ok(quote! {
                    {
                        let mut __rython_broke = false;
                        for #target in #iter_expr {
                            #(#body_tokens;)*
                        }
                        if !__rython_broke {
                            #(#else_body_tokens;)*
                        }
                    }
                })
            } else {
                // No break can skip the else clause; run it unconditionally.
                Ok(quote! {
                    {
                        for #target in #iter_expr {
                            #(#body_tokens;)*
                        }
                        #(#else_body_tokens;)*
                    }
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    // Tests would go here - currently commented out as they need full AST infrastructure
    // create_parse_test!(test_simple_async_for, "async for item in async_iter:\n    pass", "test.py");
}