use proc_macro2::TokenStream;
use quote::quote;
use serde::{Deserialize, Serialize};

use crate::{
    CodeGen, CodeGenContext, ExprType, Node, PythonOptions, SymbolTableScopes,
    };

/// List comprehension (e.g., [x ** 2 for x in range(10) if x % 2 == 0])
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ListComp {
    /// The element expression being computed
    pub elt: Box<ExprType>,
    /// The generators (for clauses)
    pub generators: Vec<Comprehension>,
    /// Position information
    pub lineno: Option<usize>,
    pub col_offset: Option<usize>,
    pub end_lineno: Option<usize>,
    pub end_col_offset: Option<usize>,
}

/// Set comprehension (e.g., {x for x in range(10) if x % 2 == 0})
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SetComp {
    /// The element expression being computed
    pub elt: Box<ExprType>,
    /// The generators (for clauses)
    pub generators: Vec<Comprehension>,
    /// Position information
    pub lineno: Option<usize>,
    pub col_offset: Option<usize>,
    pub end_lineno: Option<usize>,
    pub end_col_offset: Option<usize>,
}

/// Generator expression (e.g., (x for x in range(10) if x % 2 == 0))
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GeneratorExp {
    /// The element expression being computed
    pub elt: Box<ExprType>,
    /// The generators (for clauses)
    pub generators: Vec<Comprehension>,
    /// Position information
    pub lineno: Option<usize>,
    pub col_offset: Option<usize>,
    pub end_lineno: Option<usize>,
    pub end_col_offset: Option<usize>,
}

/// Dictionary comprehension (e.g., {k: v for k, v in items.items()})
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DictComp {
    /// The key expression being computed
    pub key: Box<ExprType>,
    /// The value expression being computed
    pub value: Box<ExprType>,
    /// The generators (for clauses)
    pub generators: Vec<Comprehension>,
    /// Position information
    pub lineno: Option<usize>,
    pub col_offset: Option<usize>,
    pub end_lineno: Option<usize>,
    pub end_col_offset: Option<usize>,
}

/// A comprehension generator (for x in iter if condition)
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Comprehension {
    /// The target variable(s) (e.g., x in "for x in range(10)")
    pub target: ExprType,
    /// The iterable expression (e.g., range(10) in "for x in range(10)")
    pub iter: ExprType,
    /// The conditions (if clauses)
    pub ifs: Vec<ExprType>,
    /// Whether this is an async comprehension
    pub is_async: bool,
}






impl Node for ListComp {
    fn lineno(&self) -> Option<usize> { self.lineno }
    fn col_offset(&self) -> Option<usize> { self.col_offset }
    fn end_lineno(&self) -> Option<usize> { self.end_lineno }
    fn end_col_offset(&self) -> Option<usize> { self.end_col_offset }
}

impl Node for SetComp {
    fn lineno(&self) -> Option<usize> { self.lineno }
    fn col_offset(&self) -> Option<usize> { self.col_offset }
    fn end_lineno(&self) -> Option<usize> { self.end_lineno }
    fn end_col_offset(&self) -> Option<usize> { self.end_col_offset }
}

impl Node for GeneratorExp {
    fn lineno(&self) -> Option<usize> { self.lineno }
    fn col_offset(&self) -> Option<usize> { self.col_offset }
    fn end_lineno(&self) -> Option<usize> { self.end_lineno }
    fn end_col_offset(&self) -> Option<usize> { self.end_col_offset }
}

impl Node for DictComp {
    fn lineno(&self) -> Option<usize> { self.lineno }
    fn col_offset(&self) -> Option<usize> { self.col_offset }
    fn end_lineno(&self) -> Option<usize> { self.end_lineno }
    fn end_col_offset(&self) -> Option<usize> { self.end_col_offset }
}

/// Lower a comprehension's generator clauses into nested `for` loops around
/// `inner`, binding each generator's real target name (so the element and
/// condition expressions can reference it) and applying `if` guards with
/// `continue`. Generators nest left-to-right, matching Python's evaluation
/// order, and later generators may reference earlier targets.
fn build_comprehension_loops(
    generators: &[Comprehension],
    inner: TokenStream,
    ctx: &CodeGenContext,
    options: &PythonOptions,
    symbols: &SymbolTableScopes,
) -> Result<TokenStream, Box<dyn std::error::Error>> {
    let mut acc = inner;
    for generator in generators.iter().rev() {
        let target = generator
            .target
            .clone()
            .to_rust(ctx.clone(), options.clone(), symbols.clone())?;
        let iter_expr = generator
            .iter
            .clone()
            .to_rust(ctx.clone(), options.clone(), symbols.clone())?;
        let conditions: Result<Vec<_>, _> = generator
            .ifs
            .iter()
            .map(|if_expr| {
                if_expr
                    .clone()
                    .to_rust(ctx.clone(), options.clone(), symbols.clone())
            })
            .collect();
        let conditions = conditions?;
        let guard = if conditions.is_empty() {
            quote!()
        } else {
            quote! { if !( #((#conditions))&&* ) { continue; } }
        };
        acc = quote! {
            for #target in #iter_expr {
                #guard
                #acc
            }
        };
    }
    Ok(acc)
}

impl CodeGen for ListComp {
    type Context = CodeGenContext;
    type Options = PythonOptions;
    type SymbolTable = SymbolTableScopes;

    fn find_symbols(self, symbols: Self::SymbolTable) -> Self::SymbolTable {
        // Process the element and generators
        let symbols = (*self.elt).clone().find_symbols(symbols);
        self.generators.into_iter().fold(symbols, |acc, generator| {
            let acc = generator.target.find_symbols(acc);
            let acc = generator.iter.find_symbols(acc);
            generator.ifs.into_iter().fold(acc, |acc, if_expr| if_expr.find_symbols(acc))
        })
    }

    fn to_rust(
        self,
        ctx: Self::Context,
        options: Self::Options,
        symbols: Self::SymbolTable,
    ) -> Result<TokenStream, Box<dyn std::error::Error>> {
        let elt = (*self.elt).clone().to_rust(ctx.clone(), options.clone(), symbols.clone())?;
        let loops = build_comprehension_loops(
            &self.generators,
            quote! { __rython_comp.push(#elt); },
            &ctx,
            &options,
            &symbols,
        )?;
        Ok(quote! {
            {
                let mut __rython_comp = Vec::new();
                #loops
                __rython_comp
            }
        })
    }
}

impl CodeGen for SetComp {
    type Context = CodeGenContext;
    type Options = PythonOptions;
    type SymbolTable = SymbolTableScopes;

    fn find_symbols(self, symbols: Self::SymbolTable) -> Self::SymbolTable {
        // Process the element and generators
        let symbols = (*self.elt).clone().find_symbols(symbols);
        self.generators.into_iter().fold(symbols, |acc, generator| {
            let acc = generator.target.find_symbols(acc);
            let acc = generator.iter.find_symbols(acc);
            generator.ifs.into_iter().fold(acc, |acc, if_expr| if_expr.find_symbols(acc))
        })
    }

    fn to_rust(
        self,
        ctx: Self::Context,
        options: Self::Options,
        symbols: Self::SymbolTable,
    ) -> Result<TokenStream, Box<dyn std::error::Error>> {
        let elt = (*self.elt).clone().to_rust(ctx.clone(), options.clone(), symbols.clone())?;
        let loops = build_comprehension_loops(
            &self.generators,
            quote! { __rython_comp.insert(#elt); },
            &ctx,
            &options,
            &symbols,
        )?;
        Ok(quote! {
            {
                let mut __rython_comp = std::collections::HashSet::new();
                #loops
                __rython_comp
            }
        })
    }
}

impl CodeGen for GeneratorExp {
    type Context = CodeGenContext;
    type Options = PythonOptions;
    type SymbolTable = SymbolTableScopes;

    fn find_symbols(self, symbols: Self::SymbolTable) -> Self::SymbolTable {
        // Process the element and generators
        let symbols = (*self.elt).clone().find_symbols(symbols);
        self.generators.into_iter().fold(symbols, |acc, generator| {
            let acc = generator.target.find_symbols(acc);
            let acc = generator.iter.find_symbols(acc);
            generator.ifs.into_iter().fold(acc, |acc, if_expr| if_expr.find_symbols(acc))
        })
    }

    fn to_rust(
        self,
        ctx: Self::Context,
        options: Self::Options,
        symbols: Self::SymbolTable,
    ) -> Result<TokenStream, Box<dyn std::error::Error>> {
        // Generator expressions are lowered eagerly (like a list
        // comprehension) and then turned back into an iterator; Python's lazy
        // evaluation is not modeled yet.
        let elt = (*self.elt).clone().to_rust(ctx.clone(), options.clone(), symbols.clone())?;
        let loops = build_comprehension_loops(
            &self.generators,
            quote! { __rython_comp.push(#elt); },
            &ctx,
            &options,
            &symbols,
        )?;
        Ok(quote! {
            {
                let mut __rython_comp = Vec::new();
                #loops
                __rython_comp.into_iter()
            }
        })
    }
}

impl CodeGen for DictComp {
    type Context = CodeGenContext;
    type Options = PythonOptions;
    type SymbolTable = SymbolTableScopes;

    fn find_symbols(self, symbols: Self::SymbolTable) -> Self::SymbolTable {
        // Process the key, value and generators
        let symbols = (*self.key).clone().find_symbols(symbols);
        let symbols = (*self.value).clone().find_symbols(symbols);
        self.generators.into_iter().fold(symbols, |acc, generator| {
            let acc = generator.target.find_symbols(acc);
            let acc = generator.iter.find_symbols(acc);
            generator.ifs.into_iter().fold(acc, |acc, if_expr| if_expr.find_symbols(acc))
        })
    }

    fn to_rust(
        self,
        ctx: Self::Context,
        options: Self::Options,
        symbols: Self::SymbolTable,
    ) -> Result<TokenStream, Box<dyn std::error::Error>> {
        let key = (*self.key).clone().to_rust(ctx.clone(), options.clone(), symbols.clone())?;
        let value = (*self.value).clone().to_rust(ctx.clone(), options.clone(), symbols.clone())?;
        let loops = build_comprehension_loops(
            &self.generators,
            quote! { __rython_comp.insert(#key, #value); },
            &ctx,
            &options,
            &symbols,
        )?;
        // PyDict, like dict literals: comprehension-built dicts preserve
        // insertion order too.
        Ok(quote! {
            {
                let mut __rython_comp = PyDict::new();
                #loops
                __rython_comp
            }
        })
    }
}

#[cfg(test)]
mod tests {
    // Note: These tests might need additional AST node implementations
    // create_parse_test!(test_simple_listcomp, "[x for x in range(5)]", "test.py");
    // create_parse_test!(test_listcomp_with_condition, "[x for x in range(10) if x % 2 == 0]", "test.py");
}