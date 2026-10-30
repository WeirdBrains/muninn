use proc_macro2::TokenStream;
use quote::quote;
use serde::{Deserialize, Serialize};

use crate::{
    dump, err_from, Attribute, Await, BinOp, BoolOp, Call, CodeGen, CodeGenContext, Compare,
    Constant, Dict, DictComp, ExprTypeNotYetImplemented, FormattedValue, GeneratorExp, IfExp,
    JoinedStr, Lambda, ListComp, Name, NamedExpr, Node, PythonOptions, Set, SetComp, Starred,
    Subscript, SymbolTableScopes, Tuple, UnaryOp, Yield, YieldFrom,
};

/// Mostly this shouldn't be used, but it exists so that we don't have to manually implement FromPyObject on all of ExprType
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[repr(transparent)]
pub struct Container<T>(pub T);


#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub enum ExprType {
    BoolOp(BoolOp),
    NamedExpr(NamedExpr),
    BinOp(BinOp),
    UnaryOp(UnaryOp),
    Lambda(Lambda),
    IfExp(IfExp),
    Dict(Dict),
    Set(Set),
    ListComp(ListComp),
    DictComp(DictComp),
    SetComp(SetComp),
    GeneratorExp(GeneratorExp),
    Await(Await),
    Yield(Yield),
    YieldFrom(YieldFrom),
    Compare(Compare),
    Call(Call),
    FormattedValue(FormattedValue),
    JoinedStr(JoinedStr),
    Constant(Constant),

    /// These can appear in a few places, such as the left side of an assignment.
    Attribute(Attribute),
    Subscript(Subscript),
    Starred(Starred),
    Name(Name),
    List(Vec<ExprType>),
    Tuple(Tuple),
    /*Slice(),*/
    NoneType(Constant),

    Unimplemented(String),
    #[default]
    Unknown,
}


impl<'a> CodeGen for ExprType {
    type Context = CodeGenContext;
    type Options = PythonOptions;
    type SymbolTable = SymbolTableScopes;

    fn to_rust(
        self,
        ctx: Self::Context,
        options: Self::Options,
        symbols: Self::SymbolTable,
    ) -> std::result::Result<TokenStream, Box<dyn std::error::Error>> {
        match self {
            ExprType::Attribute(attribute) => attribute.to_rust(ctx, options, symbols),
            ExprType::Await(func) => func.to_rust(ctx, options, symbols),
            ExprType::BinOp(binop) => binop.to_rust(ctx, options, symbols),
            ExprType::BoolOp(boolop) => boolop.to_rust(ctx, options, symbols),
            ExprType::Call(call) => call.to_rust(ctx, options, symbols),
            ExprType::Compare(c) => c.to_rust(ctx, options, symbols),
            ExprType::Constant(c) => c.to_rust(ctx, options, symbols),
            ExprType::Lambda(l) => l.to_rust(ctx, options, symbols),
            ExprType::IfExp(i) => i.to_rust(ctx, options, symbols),
            ExprType::Dict(d) => d.to_rust(ctx, options, symbols),
            ExprType::Set(s) => s.to_rust(ctx, options, symbols),
            ExprType::ListComp(lc) => lc.to_rust(ctx, options, symbols),
            ExprType::DictComp(dc) => dc.to_rust(ctx, options, symbols),
            ExprType::SetComp(sc) => sc.to_rust(ctx, options, symbols),
            ExprType::GeneratorExp(ge) => ge.to_rust(ctx, options, symbols),
            ExprType::Tuple(t) => t.to_rust(ctx, options, symbols),
            ExprType::Subscript(s) => s.to_rust(ctx, options, symbols),
            ExprType::Starred(s) => s.to_rust(ctx, options, symbols),
            ExprType::Yield(y) => y.to_rust(ctx, options, symbols),
            ExprType::YieldFrom(yf) => yf.to_rust(ctx, options, symbols),
            ExprType::JoinedStr(js) => js.to_rust(ctx, options, symbols),
            ExprType::FormattedValue(fv) => fv.to_rust(ctx, options, symbols),
            ExprType::List(l) => {
                let mut elements = Vec::new();
                let mut has_starred = false;
                
                for li in l {
                    let code = li
                        .clone()
                        .to_rust(ctx.clone(), options.clone(), symbols.clone())?;
                    
                    // Check if this is a starred expression
                    if matches!(li, ExprType::Starred(_)) {
                        has_starred = true;
                        let code_str = code.to_string();
                        // Special handling for sys::argv unpacking
                        if code_str.contains("sys :: argv") {
                            // Mark that we need special sys::argv handling with a unique marker
                            elements.push(quote! { __STARRED_ARGV_MARKER__ });
                        } else {
                            elements.push(code);
                        }
                    } else {
                        elements.push(code);
                    }
                }
                
                // If we have starred expressions, handle them specially
                if has_starred {
                    let mut final_elements = Vec::new();
                    let mut has_argv_starred = false;
                    
                    for element in elements {
                        let elem_str = element.to_string();
                        if elem_str.contains("__STARRED_ARGV_MARKER__") {
                            has_argv_starred = true;
                            continue; // Skip the placeholder
                        } else {
                            final_elements.push(element);
                        }
                    }
                    
                    // Build the vector with proper unpacking
                    if has_argv_starred {
                        if final_elements.is_empty() {
                            // Only sys::argv unpacking
                            Ok(quote! {
                                (*sys::argv).clone()
                            })
                        } else {
                            // Mix of regular elements and sys::argv unpacking
                            // Clone each element to avoid ownership issues
                            Ok(quote! {
                                {
                                    let mut vec = Vec::new();
                                    #(vec.push((#final_elements).clone().to_string());)*
                                    vec.extend((*sys::argv).iter().cloned());
                                    vec
                                }
                            })
                        }
                    } else {
                        // Other starred expressions (not sys::argv)
                        Ok(quote! {
                            vec![#(#final_elements),*]
                        })
                    }
                } else {
                    // Elements keep their own types: [1, 2, 3] must become a
                    // Vec<i64>, not a Vec<String>.
                    Ok(quote! {
                        vec![#(#elements),*]
                    })
                }
            }
            ExprType::Name(name) => name.to_rust(ctx, options, symbols),
            // Python's None is Rust's Option::None: `x = None` initializes
            // an Option, `f(None)` passes one, `d.get(k)` results compare
            // against it.
            ExprType::NoneType(_) => Ok(quote!(None)),
            ExprType::UnaryOp(operand) => operand.to_rust(ctx, options, symbols),

            _ => {
                let error = err_from(ExprTypeNotYetImplemented(self));
                Err(error.into())
            }
        }
    }
}

/// An Expr only contains a single value key, which leads to the actual expression,
/// which is one of several types.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Expr {
    pub value: ExprType,
    pub ctx: Option<String>,
    pub lineno: Option<usize>,
    pub col_offset: Option<usize>,
    pub end_lineno: Option<usize>,
    pub end_col_offset: Option<usize>,
}


impl CodeGen for Expr {
    type Context = CodeGenContext;
    type Options = PythonOptions;
    type SymbolTable = SymbolTableScopes;

    fn to_rust(
        self,
        ctx: Self::Context,
        options: Self::Options,
        symbols: Self::SymbolTable,
    ) -> std::result::Result<TokenStream, Box<dyn std::error::Error>> {
        // Delegate to the (complete) ExprType dispatch rather than keeping a
        // second, drifting copy of the match here. NoneType statements
        // generate no code.
        if matches!(self.value, ExprType::NoneType(_)) {
            return Ok(quote!());
        }
        self.value.to_rust(ctx, options, symbols)
    }
}

impl Node for Expr {
    fn lineno(&self) -> Option<usize> {
        self.lineno
    }

    fn col_offset(&self) -> Option<usize> {
        self.col_offset
    }

    fn end_lineno(&self) -> Option<usize> {
        self.end_lineno
    }

    fn end_col_offset(&self) -> Option<usize> {
        self.end_col_offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_call_expression() {
        let expression = crate::parse("test()", "test.py").unwrap();
        let mut options = PythonOptions::default();
        options.with_std_python = false;
        let symbols = SymbolTableScopes::new();
        let tokens = expression
            .clone()
            .to_rust(CodeGenContext::Module("test".to_string()), options, symbols)
            .unwrap();
        assert_eq!(
            tokens.to_string(),
            "fn __module_init__ () -> Result < () , PyException > { test () ; Ok (()) } \
             fn main () { if let Err (e) = __module_init__ () { eprintln ! (\"{}\" , e) ; \
             std :: process :: exit (1) ; } }"
        );
    }
}

/// Lower an expression in condition position (if/while/ternary/assert
/// tests): Python implicitly calls bool() on it. Boolean operators recurse
/// into their operands and `not` negates a condition; comparisons already
/// yield bool; anything else is wrapped in stdpython's Truthy::is_truthy,
/// giving Python's truth table (empty string/collection and zero are
/// false).
pub fn condition_to_rust(
    expr: &ExprType,
    ctx: CodeGenContext,
    options: PythonOptions,
    symbols: SymbolTableScopes,
) -> Result<TokenStream, Box<dyn std::error::Error>> {
    match expr {
        ExprType::BoolOp(op)
            if matches!(op.op, crate::BoolOps::And | crate::BoolOps::Or) =>
        {
            let mut parts = Vec::new();
            for value in &op.values {
                parts.push(condition_to_rust(
                    value,
                    ctx.clone(),
                    options.clone(),
                    symbols.clone(),
                )?);
            }
            Ok(match op.op {
                crate::BoolOps::And => quote!(#((#parts))&&*),
                _ => quote!(#((#parts))||*),
            })
        }
        ExprType::UnaryOp(u) if matches!(u.op, crate::Ops::Not) => {
            let inner = condition_to_rust(&u.operand, ctx, options, symbols)?;
            Ok(quote!(!(#inner)))
        }
        // Comparisons (including `in` and `is None`) already produce bool.
        ExprType::Compare(_) => expr.clone().to_rust(ctx, options, symbols),
        // Bool literals are already bool.
        ExprType::Constant(c) if matches!(&c.0, Some(litrs::Literal::Bool(_))) => {
            expr.clone().to_rust(ctx, options, symbols)
        }
        other => {
            let tokens = other.clone().to_rust(ctx, options, symbols)?;
            Ok(quote!((#tokens).is_truthy()))
        }
    }
}
