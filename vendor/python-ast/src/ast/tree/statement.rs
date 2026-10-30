use proc_macro2::TokenStream;
use quote::quote;

use crate::{
    dump, err_from, Assign, AsyncFor, AsyncWith, AugAssign, Call, ClassDef,
    CodeGen, CodeGenContext, Expr, ExprType, For, FunctionDef, If, Import, ImportFrom, Node,
    PythonOptions, Raise, StatementNotYetImplemented, SymbolTableScopes, Try, While, With,
};

use tracing::debug;

use serde::{Deserialize, Serialize};

/// AST node types that can be used as a statement implement this type.
pub trait PyStatementTrait: Clone + PartialEq {
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Statement {
    pub lineno: Option<usize>,
    pub col_offset: Option<usize>,
    pub end_lineno: Option<usize>,
    pub end_col_offset: Option<usize>,
    pub statement: StatementType,
}


impl Node for Statement {
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

impl CodeGen for Statement {
    type Context = CodeGenContext;
    type Options = PythonOptions;
    type SymbolTable = SymbolTableScopes;

    fn find_symbols(self, symbols: Self::SymbolTable) -> Self::SymbolTable {
        self.statement.clone().find_symbols(symbols)
    }

    fn to_rust(
        self,
        ctx: Self::Context,
        options: Self::Options,
        symbols: Self::SymbolTable,
    ) -> Result<TokenStream, Box<dyn std::error::Error>> {
        let (lineno, col_offset) = (self.lineno, self.col_offset);
        let (end_lineno, end_col_offset) = (self.end_lineno, self.end_col_offset);
        self.statement
            .clone()
            .to_rust(ctx, options, symbols)
            .map_err(|e| {
                let location = crate::SourceLocation::with_span(
                    "<module>",
                    lineno,
                    col_offset.map(|c| c + 1),
                    end_lineno,
                    end_col_offset,
                );
                Box::<dyn std::error::Error>::from(crate::codegen_error(
                    location,
                    crate::format_error_chain(e.as_ref()),
                    "",
                ))
            })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum StatementType {
    AsyncFunctionDef(FunctionDef),
    Assert {
        test: Box<ExprType>,
        msg: Option<Box<ExprType>>,
    },
    Assign(Assign),
    AugAssign(AugAssign),
    Break,
    Continue,
    ClassDef(ClassDef),
    Call(Call),
    Pass,
    Return(Option<Expr>),
    Import(Import),
    ImportFrom(ImportFrom),
    Expr(Expr),
    FunctionDef(FunctionDef),
    If(If),
    For(For),
    While(While),
    Try(Try),
    AsyncWith(AsyncWith),
    AsyncFor(AsyncFor),
    Raise(Raise),
    With(With),

    Unimplemented(String),
}


/// A Python `return` lowered for the current context: inside a try-block
/// closure it signals out via PyFlow so the try lowering can run the
/// finally body and re-return; elsewhere it returns Ok directly.
fn return_tokens(ctx: &CodeGenContext, value: TokenStream) -> TokenStream {
    if ctx.in_try_block() {
        quote!(return Ok(PyFlow::Return(#value)))
    } else {
        quote!(return Ok(#value))
    }
}

/// Whether a statement list contains a function-level `return` anywhere —
/// looking through control flow (including nested trys and their handlers)
/// but not into nested function or class definitions. The try lowering uses
/// this to pick its closure's carrier type: bodies with returns thread the
/// returned value out through PyFlow.
/// Does this statement list contain a `break`/`continue` that targets a
/// loop OUTSIDE the list? A loop nested *within* the list owns its own
/// breaks, so its body is not searched — but its `else` clause is, since
/// a break there targets the enclosing loop, as in Python. Nested
/// function and class bodies are separate scopes and never searched.
pub fn body_breaks_outward(body: &[Statement]) -> bool {
    body.iter().any(|stmt| match &stmt.statement {
        StatementType::Break | StatementType::Continue => true,
        StatementType::If(s) => {
            body_breaks_outward(&s.body) || body_breaks_outward(&s.orelse)
        }
        // A loop captures breaks in its BODY; only its else clause can
        // break outward.
        StatementType::For(s) => body_breaks_outward(&s.orelse),
        StatementType::While(s) => body_breaks_outward(&s.orelse),
        StatementType::AsyncFor(s) => body_breaks_outward(&s.orelse),
        StatementType::Try(s) => {
            body_breaks_outward(&s.body)
                || s.handlers.iter().any(|h| body_breaks_outward(&h.body))
                || body_breaks_outward(&s.orelse)
                || body_breaks_outward(&s.finalbody)
        }
        StatementType::With(s) => body_breaks_outward(&s.body),
        StatementType::AsyncWith(s) => body_breaks_outward(&s.body),
        _ => false,
    })
}

pub fn body_contains_function_return(body: &[Statement]) -> bool {
    body.iter().any(|stmt| match &stmt.statement {
        StatementType::Return(_) => true,
        StatementType::If(s) => {
            body_contains_function_return(&s.body) || body_contains_function_return(&s.orelse)
        }
        StatementType::For(s) => {
            body_contains_function_return(&s.body) || body_contains_function_return(&s.orelse)
        }
        StatementType::While(s) => {
            body_contains_function_return(&s.body) || body_contains_function_return(&s.orelse)
        }
        StatementType::AsyncFor(s) => {
            body_contains_function_return(&s.body) || body_contains_function_return(&s.orelse)
        }
        StatementType::Try(s) => {
            body_contains_function_return(&s.body)
                || s.handlers
                    .iter()
                    .any(|h| body_contains_function_return(&h.body))
                || body_contains_function_return(&s.orelse)
                || body_contains_function_return(&s.finalbody)
        }
        StatementType::With(s) => body_contains_function_return(&s.body),
        StatementType::AsyncWith(s) => body_contains_function_return(&s.body),
        _ => false,
    })
}

/// Whether a loop body contains a `break` that belongs to that loop —
/// looking through `if`/`try`/`with` blocks but not into nested loops
/// (whose breaks are their own) or nested definitions. Loops with an `else`
/// clause only need break-tracking machinery when this is true.
pub fn loop_body_has_direct_break(body: &[Statement]) -> bool {
    body.iter().any(|stmt| match &stmt.statement {
        StatementType::Break => true,
        StatementType::If(s) => {
            loop_body_has_direct_break(&s.body) || loop_body_has_direct_break(&s.orelse)
        }
        StatementType::Try(s) => {
            loop_body_has_direct_break(&s.body)
                || s.handlers
                    .iter()
                    .any(|h| loop_body_has_direct_break(&h.body))
                || loop_body_has_direct_break(&s.orelse)
                || loop_body_has_direct_break(&s.finalbody)
        }
        StatementType::With(s) => loop_body_has_direct_break(&s.body),
        StatementType::AsyncWith(s) => loop_body_has_direct_break(&s.body),
        _ => false,
    })
}

impl CodeGen for StatementType {
    type Context = CodeGenContext;
    type Options = PythonOptions;
    type SymbolTable = SymbolTableScopes;

    fn find_symbols(self, symbols: Self::SymbolTable) -> Self::SymbolTable {
        match self {
            StatementType::Assign(a) => a.find_symbols(symbols),
            StatementType::AugAssign(a) => a.find_symbols(symbols),
            StatementType::ClassDef(c) => c.find_symbols(symbols),
            StatementType::FunctionDef(f) => f.find_symbols(symbols),
            // Async functions register like ordinary ones, so call sites
            // know they return Result and append `?` (before `.await`).
            StatementType::AsyncFunctionDef(f) => f.find_symbols(symbols),
            StatementType::Import(i) => i.find_symbols(symbols),
            StatementType::ImportFrom(i) => i.find_symbols(symbols),
            StatementType::Expr(e) => e.find_symbols(symbols),
            StatementType::If(i) => i.find_symbols(symbols),
            StatementType::For(f) => f.find_symbols(symbols),
            StatementType::While(w) => w.find_symbols(symbols),
            StatementType::Try(t) => t.find_symbols(symbols),
            StatementType::AsyncWith(aw) => aw.find_symbols(symbols),
            StatementType::AsyncFor(af) => af.find_symbols(symbols),
            StatementType::Raise(r) => r.find_symbols(symbols),
            StatementType::With(w) => w.find_symbols(symbols),
            _ => symbols,
        }
    }

    fn to_rust(
        self,
        ctx: Self::Context,
        options: Self::Options,
        symbols: Self::SymbolTable,
    ) -> Result<TokenStream, Box<dyn std::error::Error>> {
        match self {
            StatementType::AsyncFunctionDef(s) => {
                let func_def = s.to_rust(Self::Context::Async(Box::new(ctx)), options, symbols)?;
                Ok(quote!(#func_def))
            }
            StatementType::Assert { test, msg } => {
                let test_tokens =
                    crate::condition_to_rust(&test, ctx.clone(), options.clone(), symbols.clone())?;
                let msg_tokens = match msg {
                    Some(m) => {
                        let m = m.to_rust(ctx.clone(), options, symbols)?;
                        quote!(format!("{}", #m))
                    }
                    None => quote!(String::new()),
                };
                // A failed assert raises AssertionError. Functions return
                // Result<T, PyException>, so raising is returning Err: it is
                // caught by an enclosing try's closure or propagates out of
                // the function, as in Python.
                Ok(quote! {
                    if !(#test_tokens) {
                        return Err(PyException::new("AssertionError", #msg_tokens));
                    }
                })
            }
            StatementType::Assign(a) => a.to_rust(ctx, options, symbols),
            StatementType::AugAssign(a) => a.to_rust(ctx, options, symbols),
            StatementType::Break => {
                // A break whose loop lies outside an enclosing try-block
                // closure cannot be a Rust `break` here — it would escape
                // the closure. Signal it out instead; the try lowering
                // replays it after the finally clause, as Python orders it.
                if ctx.break_crosses_try_closure() {
                    return Ok(if ctx.break_target_has_else() {
                        quote! {{ __rython_broke = true; return Ok(PyFlow::Break); }}
                    } else {
                        quote! {return Ok(PyFlow::Break);}
                    });
                }
                // Inside a loop that has an `else` clause, breaking must also
                // record that the loop did not complete normally.
                if matches!(ctx, Self::Context::Loop { has_else: true, .. }) {
                    Ok(quote! {{ __rython_broke = true; break; }})
                } else {
                    Ok(quote! {break;})
                }
            }
            StatementType::Call(c) => c.to_rust(ctx, options, symbols),
            StatementType::ClassDef(c) => c.to_rust(ctx, options, symbols),
            StatementType::Continue => {
                if ctx.break_crosses_try_closure() {
                    Ok(quote! {return Ok(PyFlow::Continue);})
                } else {
                    Ok(quote! {continue;})
                }
            }
            StatementType::Pass => Ok(quote! {}),
            StatementType::FunctionDef(s) => s.to_rust(ctx, options, symbols),
            StatementType::Import(s) => s.to_rust(ctx, options, symbols),
            StatementType::ImportFrom(s) => s.to_rust(ctx, options, symbols),
            StatementType::Expr(s) => s.to_rust(ctx, options, symbols),
            // Functions return Result<T, PyException>; a Python return wraps
            // its value in Ok (bare return / return None yield Ok(())).
            // Inside a try block's closure, a return must first break out of
            // the closure: it becomes Ok(PyFlow::Return(value)), which
            // the try lowering turns back into a function return — after
            // running the finally body, as Python requires.
            StatementType::Return(None) => Ok(return_tokens(&ctx, quote!(()))),
            StatementType::Return(Some(e)) => {
                let value = if matches!(e.value, ExprType::NoneType(_)) {
                    quote!(())
                } else {
                    let tokens = e.clone().to_rust(ctx.clone(), options.clone(), symbols)?;
                    // A `-> str` function returning an attribute chain reads
                    // a String field through the shared receiver: clone it
                    // out. Python strings are immutable, so the clone
                    // reproduces Python's semantics exactly (a bare field
                    // read would move out of &self and not compile).
                    if options.clone_str_attribute_returns
                        && matches!(e.value, ExprType::Attribute(_))
                    {
                        quote!((#tokens).clone())
                    } else {
                        tokens
                    }
                };
                Ok(return_tokens(&ctx, value))
            }
            StatementType::If(i) => i.to_rust(ctx, options, symbols),
            StatementType::For(f) => f.to_rust(ctx, options, symbols),
            StatementType::While(w) => w.to_rust(ctx, options, symbols),
            StatementType::Try(t) => t.to_rust(ctx, options, symbols),
            StatementType::AsyncWith(aw) => aw.to_rust(ctx, options, symbols),
            StatementType::AsyncFor(af) => af.to_rust(ctx, options, symbols),
            StatementType::Raise(r) => r.to_rust(ctx, options, symbols),
            StatementType::With(w) => w.to_rust(ctx, options, symbols),
            _ => {
                let error = err_from(StatementNotYetImplemented(self));
                Err(error.into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_pass_statement() {
        let statement = StatementType::Pass;
        let options = PythonOptions::default();
        let tokens = statement.clone().to_rust(
            CodeGenContext::Module("".to_string()),
            options,
            SymbolTableScopes::new(),
        );

        debug!("statement: {:?}, tokens: {:?}", statement, tokens);
        assert_eq!(tokens.unwrap().is_empty(), true);
    }

    #[test]
    fn check_break_statement() {
        let statement = StatementType::Break;
        let options = PythonOptions::default();
        let tokens = statement.clone().to_rust(
            CodeGenContext::Module("".to_string()),
            options,
            SymbolTableScopes::new(),
        );

        debug!("statement: {:?}, tokens: {:?}", statement, tokens);
        assert_eq!(tokens.unwrap().is_empty(), false);
    }

    #[test]
    fn check_continue_statement() {
        let statement = StatementType::Continue;
        let options = PythonOptions::default();
        let tokens = statement.clone().to_rust(
            CodeGenContext::Module("".to_string()),
            options,
            SymbolTableScopes::new(),
        );

        debug!("statement: {:?}, tokens: {:?}", statement, tokens);
        assert_eq!(tokens.unwrap().is_empty(), false);
    }

    #[test]
    fn return_with_nothing() {
        let tree = crate::parse("return", "<none>").unwrap();
        assert_eq!(tree.raw.body.len(), 1);
        assert_eq!(
            tree.raw.body[0].statement,
            StatementType::Return(Some(Expr {
                value: crate::tree::ExprType::NoneType(crate::tree::Constant(None)),
                lineno: Some(1),
                col_offset: Some(0),
                end_lineno: Some(1),
                end_col_offset: Some(6),
                ..Default::default()
            }))
        );
    }

    #[test]
    fn return_with_expr() {
        let lit = litrs::Literal::Integer(litrs::IntegerLit::parse(String::from("8")).unwrap());
        let tree = crate::parse("return 8", "<none>").unwrap();
        assert_eq!(tree.raw.body.len(), 1);
        assert_eq!(
            tree.raw.body[0].statement,
            StatementType::Return(Some(Expr {
                value: crate::tree::ExprType::Constant(crate::tree::Constant(Some(lit))),
                lineno: Some(1),
                col_offset: Some(0),
                end_lineno: Some(1),
                end_col_offset: Some(8),
                ..Default::default()
            }))
        );
    }

    #[test]
    fn does_module_compile() {
        let options = PythonOptions::default();
        let result = crate::parse(
            "#test comment
def foo():
    continue
    pass
",
            "test_case",
        )
        .unwrap();
        tracing::info!("{:?}", result);
        let code = result.to_rust(
            CodeGenContext::Module("".to_string()),
            options,
            SymbolTableScopes::new(),
        );
        tracing::info!("module: {:?}", code);
    }
}
