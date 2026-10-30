use proc_macro2::TokenStream;
use quote::quote;
use serde::{Deserialize, Serialize};

use crate::{
    dump, err_from, BoolOpNotYetImplemented, CodeGen, CodeGenContext, ExprType,
    PythonOptions, SymbolTableScopes,
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum BoolOps {
    And,
    Or,
    Unknown,
}


#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BoolOp {
    pub op: BoolOps,
    /// All operands: Python collapses `a and b and c` into one BoolOp node
    /// with three values.
    pub values: Vec<ExprType>,
}


impl<'a> CodeGen for BoolOp {
    type Context = CodeGenContext;
    type Options = PythonOptions;
    type SymbolTable = SymbolTableScopes;

    fn to_rust(
        self,
        ctx: Self::Context,
        options: Self::Options,
        symbols: Self::SymbolTable,
    ) -> Result<TokenStream, Box<dyn std::error::Error>> {
        // Python's boolean operators return operands, not booleans; for now we
        // approximate with Rust's short-circuiting operators, folding every
        // operand (a BoolOp node can carry more than two).
        let mut rendered = Vec::new();
        for value in self.values.clone() {
            rendered.push(value.to_rust(ctx.clone(), options.clone(), symbols.clone())?);
        }

        match self.op {
            BoolOps::Or => {
                // `a or None` yields the Option-model None when `a` is
                // falsy — dropping the None silently returned the falsy
                // value instead (`0 or None` must be None, not 0).
                if let Some(last) = rendered.last() {
                    if last.to_string().trim() == "None" && rendered.len() == 2 {
                        let first = &rendered[0];
                        return Ok(quote!({
                            let __rython_or = #first;
                            if (__rython_or).is_truthy() { Some(__rython_or) } else { None }
                        }));
                    }
                }
                Ok(quote!(#((#rendered))||*))
            }
            BoolOps::And => Ok(quote!(#((#rendered))&&*)),

            _ => Err(err_from(BoolOpNotYetImplemented(self)).into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_and() {
        let options = PythonOptions::default();
        let result = crate::parse("1 and 2", "test_case.py").unwrap();
        tracing::info!("Python tree: {:?}", result);
        //tracing::info!("{}", result.to_rust().unwrap());

        let code = result
            .to_rust(
                CodeGenContext::Module("test_case".to_string()),
                options,
                SymbolTableScopes::new(),
            )
            .unwrap();
        tracing::info!("module: {:?}", code);
    }

    #[test]
    fn test_or() {
        let options = PythonOptions::default();
        let result = crate::parse("1 or 2", "test_case.py").unwrap();
        tracing::info!("Python tree: {:?}", result);
        //tracing::info!("{}", result);

        let code = result
            .to_rust(
                CodeGenContext::Module("test_case".to_string()),
                options,
                SymbolTableScopes::new(),
            )
            .unwrap();
        tracing::info!("module: {:?}", code);
    }
}
