use proc_macro2::TokenStream;
use quote::quote;
use serde::{Deserialize, Serialize};

use crate::{CodeGen, CodeGenContext, ExprType, PythonOptions, SymbolTableScopes,
    Node, impl_node_with_positions
};

/// A subscript's bracket contents: a plain index or a slice.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum SubscriptKind {
    Index(Box<ExprType>),
    Slice {
        lower: Option<Box<ExprType>>,
        upper: Option<Box<ExprType>>,
        step: Option<Box<ExprType>>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Subscript {
    pub value: Box<ExprType>,
    pub kind: SubscriptKind,
    pub lineno: Option<usize>,
    pub col_offset: Option<usize>,
    pub end_lineno: Option<usize>,
    pub end_col_offset: Option<usize>,
}


impl_node_with_positions!(Subscript { lineno, col_offset, end_lineno, end_col_offset });

/// Lower the receiver of a subscript STORE as a place. Names and
/// attributes are places already; a nested subscript (`grid[i][j] = v`)
/// threads through py_index_mut so the store mutates the real container —
/// the Load lowering would yield a clone and silently drop the write.
/// Anything else (e.g. a call result) is rejected loudly.
pub(crate) fn subscript_receiver_place(
    expr: &ExprType,
    ctx: CodeGenContext,
    options: PythonOptions,
    symbols: SymbolTableScopes,
) -> Result<TokenStream, Box<dyn std::error::Error>> {
    match expr {
        ExprType::Subscript(sub) => {
            let inner = subscript_receiver_place(
                &sub.value,
                ctx.clone(),
                options.clone(),
                symbols.clone(),
            )?;
            match &sub.kind {
                SubscriptKind::Index(index) => {
                    let index = index
                        .clone()
                        .to_rust(ctx, options, symbols)?;
                    Ok(quote!((#inner).py_index_mut(#index)?))
                }
                SubscriptKind::Slice { .. } => Err(
                    "cannot assign through a slice (`x[a:b][...] = ...`)"
                        .to_string()
                        .into(),
                ),
            }
        }
        ExprType::Name(_) | ExprType::Attribute(_) => {
            expr.clone().to_rust(ctx, options, symbols)
        }
        other => Err(format!(
            "cannot assign into a subscript of this expression: {:?} (only \
             variables, attributes, and nested subscripts can be stored \
             through)",
            other
        )
        .into()),
    }
}

impl CodeGen for Subscript {
    type Context = CodeGenContext;
    type Options = PythonOptions;
    type SymbolTable = SymbolTableScopes;

    fn to_rust(
        self,
        ctx: Self::Context,
        options: Self::Options,
        symbols: Self::SymbolTable,
    ) -> Result<TokenStream, Box<dyn std::error::Error>> {
        let value = self.value.to_rust(ctx.clone(), options.clone(), symbols.clone())?;
        match self.kind {
            // Python index rules via PyIndex: negatives from the end, a
            // catchable IndexError/KeyError instead of a Rust panic.
            SubscriptKind::Index(index) => {
                let index = index.to_rust(ctx, options, symbols)?;
                Ok(quote! { (#value).py_index(#index)? })
            }
            // Slices clamp and never raise.
            SubscriptKind::Slice { lower, upper, step } => {
                let bound = |b: Option<Box<ExprType>>| -> Result<TokenStream, Box<dyn std::error::Error>> {
                    Ok(match b {
                        Some(e) => {
                            let t = e.to_rust(ctx.clone(), options.clone(), symbols.clone())?;
                            quote!(Some(#t))
                        }
                        None => quote!(None),
                    })
                };
                let lower = bound(lower)?;
                let upper = bound(upper)?;
                let step = bound(step)?;
                Ok(quote! { (#value).py_slice(#lower, #upper, #step) })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::create_parse_test;

    create_parse_test!(test_list_subscript, "a[0]", "subscript_test.py");
    create_parse_test!(test_dict_subscript, "d['key']", "subscript_test.py");
    create_parse_test!(test_nested_subscript, "matrix[i][j]", "subscript_test.py");
}