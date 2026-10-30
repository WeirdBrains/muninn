use proc_macro2::TokenStream;
use quote::quote;

use crate::{dump, err_from, CodeGen, CodeGenContext, ExprType, PythonOptions, SymbolTableScopes,
    UnaryOpNotYetImplemented,
};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Ops {
    Invert,
    Not,
    UAdd,
    USub,

    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct UnaryOp {
    pub op: Ops,
    pub operand: Box<ExprType>,
}


impl CodeGen for UnaryOp {
    type Context = CodeGenContext;
    type Options = PythonOptions;
    type SymbolTable = SymbolTableScopes;

    fn to_rust(
        self,
        ctx: Self::Context,
        options: Self::Options,
        symbols: Self::SymbolTable,
    ) -> Result<TokenStream, Box<dyn std::error::Error>> {
        let operand = self.operand.clone().to_rust(ctx, options, symbols)?;
        match self.op {
            // `~x` is Rust's bitwise complement, but `not x` is a
            // TRUTHINESS test: `not 5` is False, where `!5i64` is -6.
            Ops::Invert => Ok(quote!(!#operand)),
            Ops::Not => Ok(quote!(!(#operand).is_truthy())),
            // Rust has no unary plus; Python's `+x` is the identity for
            // numbers, so emit the operand alone (parenthesized).
            Ops::UAdd => Ok(quote!((#operand))),
            Ops::USub => Ok(quote!(-#operand)),
            _ => Err(err_from(UnaryOpNotYetImplemented(self)).into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_not() {
        let options = PythonOptions::default();
        let result = crate::parse("not True", "test").unwrap();
        tracing::info!("Python tree: {:?}", result);
        //tracing::info!("{}", result);

        let code = result
            .to_rust(
                CodeGenContext::Module("test".to_string()),
                options,
                SymbolTableScopes::new(),
            )
            .unwrap();
        tracing::info!("module: {:?}", code);
    }
}
