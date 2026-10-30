//! Python source to python-ast's tree, in pure Rust.
//!
//! `ruff_python_parser` parses; this module rebuilds the result as the
//! `python-ast` types CPython's `ast` module used to produce (same node
//! choices, positions and unsupported-construct errors), so python-ast's
//! code generation sees what it always saw. No libpython is involved.
use python_ast as pa;
use ruff_python_ast as ra;
use ruff_python_ast::{Expr as E, Stmt as S};
use ruff_text_size::{Ranged, TextRange};

type R<T> = std::result::Result<T, String>;

/// Byte offsets of line starts, for CPython-style 1-based lines and byte columns.
struct Lines(Vec<usize>);

impl Lines {
    fn new(src: &str) -> Self {
        let mut v = vec![0];
        v.extend(src.match_indices('\n').map(|(i, _)| i + 1));
        Lines(v)
    }
    fn at(&self, off: usize) -> (usize, usize) {
        let l = self.0.partition_point(|s| *s <= off);
        (l, off - self.0[l - 1])
    }
    fn pos(&self, r: TextRange) -> [Option<usize>; 4] {
        let (a, b) = self.at(r.start().to_usize());
        let (c, d) = self.at(r.end().to_usize());
        [Some(a), Some(b), Some(c), Some(d)]
    }
}

struct Cv<'a> {
    lines: &'a Lines,
}

macro_rules! at {
    ($s:expr, $r:expr, $t:ident { $($k:ident $(: $v:expr)?),* $(,)? }) => {{
        let [lineno, col_offset, end_lineno, end_col_offset] = $s.lines.pos($r);
        pa::$t { $($k $(: $v)?,)* lineno, col_offset, end_lineno, end_col_offset }
    }};
}

/// Parse a module the way `ast.parse` + python-ast extraction did; `filename` names it
/// (`user.py` becomes module `user`).
pub fn parse(src: &str, filename: &str) -> R<pa::Module> {
    let parsed = ruff_python_parser::parse_module(src).map_err(|e| {
        let lines = Lines::new(src);
        let (l, c) = lines.at(e.location.start().to_usize());
        format!("line {l}, column {}: {}", c + 1, e.error)
    })?;
    let lines = Lines::new(src);
    let cv = Cv { lines: &lines };
    let body = cv.stmts(&parsed.syntax().body)?;
    Ok(pa::Module {
        raw: pa::RawModule { body, type_ignores: vec![] },
        name: filename.replace(std::path::MAIN_SEPARATOR, "__").strip_suffix(".py").map(|n| pa::Name { id: n.to_string() }),
        filename: Some(filename.to_string()),
        ..Default::default()
    })
}

/// Python source to Rust source, as Rython's `rythonc` did (with the stdpython runtime).
pub fn to_rust(src: &str, filename: &str) -> R<String> {
    use pa::CodeGen;
    let module = parse(src, filename)?;
    let symbols = module.clone().find_symbols(pa::SymbolTableScopes::new());
    let options = pa::PythonOptions { with_std_python: true, ..Default::default() };
    module
        .to_rust(pa::CodeGenContext::Module(filename.to_string()), options, symbols)
        .map(|t| t.to_string())
        .map_err(|e| format!("failed to compile {filename}: {}", pa::format_error_chain(e.as_ref())))
}

fn ctx(c: ra::ExprContext) -> &'static str {
    match c {
        ra::ExprContext::Store => "Store",
        ra::ExprContext::Del => "Del",
        _ => "Load",
    }
}

fn binop(o: ra::Operator) -> pa::BinOps {
    use pa::BinOps as B;
    use ra::Operator as O;
    match o {
        O::Add => B::Add,
        O::Sub => B::Sub,
        O::Mult => B::Mult,
        O::MatMult => B::MatMult,
        O::Div => B::Div,
        O::Mod => B::Mod,
        O::Pow => B::Pow,
        O::LShift => B::LShift,
        O::RShift => B::RShift,
        O::BitOr => B::BitOr,
        O::BitXor => B::BitXor,
        O::BitAnd => B::BitAnd,
        O::FloorDiv => B::FloorDiv,
    }
}

fn cmpop(o: ra::CmpOp) -> pa::Compares {
    use pa::Compares as C;
    use ra::CmpOp as O;
    match o {
        O::Eq => C::Eq,
        O::NotEq => C::NotEq,
        O::Lt => C::Lt,
        O::LtE => C::LtE,
        O::Gt => C::Gt,
        O::GtE => C::GtE,
        O::Is => C::Is,
        O::IsNot => C::IsNot,
        O::In => C::In,
        O::NotIn => C::NotIn,
    }
}

fn unsupported_stmt(kind: &str) -> String {
    format!("the `{kind}` statement is not yet supported by rython")
}

impl Cv<'_> {
    fn stmts(&self, b: &[S]) -> R<Vec<pa::Statement>> {
        b.iter().map(|s| self.stmt(s)).collect()
    }

    fn stmt(&self, s: &S) -> R<pa::Statement> {
        let [lineno, col_offset, end_lineno, end_col_offset] = self.lines.pos(s.range());
        let statement = self.stmt_type(s).map_err(|e| match lineno {
            Some(l) if !e.starts_with("line ") => format!("line {l}: {e}"),
            _ => e,
        })?;
        Ok(pa::Statement { lineno, col_offset, end_lineno, end_col_offset, statement })
    }

    fn stmt_type(&self, s: &S) -> R<pa::StatementType> {
        use pa::StatementType as T;
        Ok(match s {
            S::FunctionDef(f) => {
                let d = self.funcdef(f)?;
                if f.is_async { T::AsyncFunctionDef(d) } else { T::FunctionDef(d) }
            }
            S::ClassDef(c) => {
                let mut bases = vec![];
                if let Some(a) = &c.arguments {
                    if !a.keywords.is_empty() {
                        return Err("class keywords are not supported".into());
                    }
                    for b in a.args.iter() {
                        match b {
                            E::Name(n) => bases.push(pa::Name { id: n.id.to_string() }),
                            _ => return Err("class bases must be plain names".into()),
                        }
                    }
                }
                T::ClassDef(pa::ClassDef { name: c.name.to_string(), bases, keywords: vec![], body: self.stmts(&c.body)? })
            }
            S::Return(r) => {
                let value = match &r.value {
                    Some(v) => self.expr(v)?,
                    None => pa::ExprType::NoneType(pa::Constant(None)),
                };
                let [lineno, col_offset, end_lineno, end_col_offset] = self.lines.pos(r.range);
                T::Return(Some(pa::Expr { value, ctx: None, lineno, col_offset, end_lineno, end_col_offset }))
            }
            S::Assign(a) => T::Assign(pa::Assign { targets: self.exprs(&a.targets)?, value: self.expr(&a.value)?, type_comment: None }),
            S::AnnAssign(a) => match &a.value {
                None => T::Pass,
                Some(v) => T::Assign(pa::Assign { targets: vec![self.expr(&a.target)?], value: self.expr(v)?, type_comment: None }),
            },
            S::AugAssign(a) => {
                T::AugAssign(at!(self, a.range, AugAssign { target: self.expr(&a.target)?, op: binop(a.op), value: self.expr(&a.value)? }))
            }
            S::For(f) => {
                let target = self.expr(&f.target)?;
                let iter = self.expr(&f.iter)?;
                let body = self.stmts(&f.body)?;
                let orelse = self.stmts(&f.orelse)?;
                if f.is_async {
                    T::AsyncFor(at!(self, f.range, AsyncFor { target, iter, body, orelse }))
                } else {
                    T::For(at!(self, f.range, For { target, iter, body, orelse }))
                }
            }
            S::While(w) => {
                T::While(at!(self, w.range, While { test: self.expr(&w.test)?, body: self.stmts(&w.body)?, orelse: self.stmts(&w.orelse)? }))
            }
            S::If(i) => T::If(self.if_chain(i.range, &i.test, &i.body, &i.elif_else_clauses)?),
            S::With(w) => {
                let items = w
                    .items
                    .iter()
                    .map(|i| {
                        Ok(pa::WithItem {
                            context_expr: self.expr(&i.context_expr)?,
                            optional_vars: i.optional_vars.as_deref().map(|v| self.expr(v)).transpose()?,
                        })
                    })
                    .collect::<R<Vec<_>>>()?;
                let body = self.stmts(&w.body)?;
                if w.is_async {
                    T::AsyncWith(at!(self, w.range, AsyncWith { items, body }))
                } else {
                    T::With(at!(self, w.range, With { items, body }))
                }
            }
            S::Raise(r) => T::Raise(at!(
                self,
                r.range,
                Raise { exc: r.exc.as_deref().map(|e| self.expr(e)).transpose()?, cause: r.cause.as_deref().map(|e| self.expr(e)).transpose()? }
            )),
            S::Try(t) => {
                if t.is_star {
                    return Err(unsupported_stmt("TryStar"));
                }
                let handlers = t
                    .handlers
                    .iter()
                    .map(|h| {
                        let ra::ExceptHandler::ExceptHandler(h) = h;
                        Ok(at!(
                            self,
                            h.range,
                            ExceptHandler {
                                exception_type: h.type_.as_deref().map(|e| self.expr(e)).transpose()?,
                                name: h.name.as_ref().map(|n| n.to_string()),
                                body: self.stmts(&h.body)?,
                            }
                        ))
                    })
                    .collect::<R<Vec<_>>>()?;
                T::Try(at!(
                    self,
                    t.range,
                    Try { body: self.stmts(&t.body)?, handlers, orelse: self.stmts(&t.orelse)?, finalbody: self.stmts(&t.finalbody)? }
                ))
            }
            S::Assert(a) => T::Assert { test: Box::new(self.expr(&a.test)?), msg: a.msg.as_deref().map(|m| self.expr(m).map(Box::new)).transpose()? },
            S::Import(i) => T::Import(pa::Import { names: i.names.iter().map(alias).collect() }),
            S::ImportFrom(i) => T::ImportFrom(pa::ImportFrom {
                module: i.module.as_ref().map(|m| m.to_string()).ok_or("relative imports without a module are not supported")?,
                names: i.names.iter().map(alias).collect(),
                level: i.level as usize,
            }),
            S::Expr(e) => {
                let [lineno, col_offset, end_lineno, end_col_offset] = self.lines.pos(e.range);
                let value = match &*e.value {
                    E::Named(_) | E::ListComp(_) | E::SetComp(_) | E::DictComp(_) | E::Starred(_) => {
                        return Err(format!("Unimplemented expression type {}", expr_kind(&e.value)));
                    }
                    v => self.expr(v)?,
                };
                T::Expr(pa::Expr { value, ctx: None, lineno, col_offset, end_lineno, end_col_offset })
            }
            S::Pass(_) => T::Pass,
            S::Break(_) => T::Break,
            S::Continue(_) => T::Continue,
            S::Delete(_) => return Err(unsupported_stmt("Delete")),
            S::TypeAlias(_) => return Err(unsupported_stmt("TypeAlias")),
            S::Match(_) => return Err(unsupported_stmt("Match")),
            S::Global(_) => return Err(unsupported_stmt("Global")),
            S::Nonlocal(_) => return Err(unsupported_stmt("Nonlocal")),
            S::IpyEscapeCommand(_) => return Err(unsupported_stmt("IpyEscapeCommand")),
        })
    }

    fn if_chain(&self, range: TextRange, test: &E, body: &[S], rest: &[ra::ElifElseClause]) -> R<pa::If> {
        let orelse = match rest.split_first() {
            None => vec![],
            Some((c, more)) => match &c.test {
                None => self.stmts(&c.body)?,
                Some(t) => {
                    // CPython: an elif is a nested If spanning to the end of the chain.
                    let end = rest.last().map(|l| l.range.end()).unwrap_or(c.range.end());
                    let r = TextRange::new(c.range.start(), end);
                    let [lineno, col_offset, end_lineno, end_col_offset] = self.lines.pos(r);
                    vec![pa::Statement {
                        lineno,
                        col_offset,
                        end_lineno,
                        end_col_offset,
                        statement: pa::StatementType::If(self.if_chain(r, t, &c.body, more)?),
                    }]
                }
            },
        };
        let r = at!(self, range, If { test: self.expr(test)?, body: self.stmts(body)?, orelse });
        Ok(r)
    }

    fn funcdef(&self, f: &ra::StmtFunctionDef) -> R<pa::FunctionDef> {
        Ok(pa::FunctionDef {
            name: f.name.to_string(),
            args: self.params(Some(&f.parameters))?,
            body: self.stmts(&f.body)?,
            decorator_list: f.decorator_list.iter().map(|d| self.expr(&d.expression)).collect::<R<_>>()?,
            returns: f.returns.as_deref().and_then(|r| self.expr(r).ok()).map(Box::new),
        })
    }

    fn param(&self, p: &ra::Parameter) -> R<pa::Parameter> {
        Ok(at!(
            self,
            p.range,
            Parameter {
                arg: p.name.to_string(),
                annotation: p.annotation.as_deref().map(|a| self.expr(a).map(Box::new)).transpose()?,
                type_comment: None,
            }
        ))
    }

    fn params(&self, p: Option<&ra::Parameters>) -> R<pa::Arguments> {
        let Some(p) = p else { return Ok(pa::Arguments::default()) };
        let plain = |v: &[ra::ParameterWithDefault]| v.iter().map(|p| self.param(&p.parameter)).collect::<R<Vec<_>>>();
        let mut defaults = vec![];
        for d in p.posonlyargs.iter().chain(p.args.iter()).filter_map(|p| p.default.as_deref()) {
            defaults.push(Box::new(self.expr(d)?));
        }
        let kw_defaults = p.kwonlyargs.iter().map(|k| k.default.as_deref().map(|d| self.expr(d).map(Box::new)).transpose()).collect::<R<Vec<_>>>()?;
        Ok(pa::Arguments {
            posonlyargs: plain(&p.posonlyargs)?,
            args: plain(&p.args)?,
            vararg: p.vararg.as_deref().map(|v| self.param(v)).transpose()?,
            kwonlyargs: plain(&p.kwonlyargs)?,
            kw_defaults,
            kwarg: p.kwarg.as_deref().map(|v| self.param(v)).transpose()?,
            defaults,
        })
    }

    fn exprs(&self, v: &[E]) -> R<Vec<pa::ExprType>> {
        v.iter().map(|e| self.expr(e)).collect()
    }

    fn bexpr(&self, e: &E) -> R<Box<pa::ExprType>> {
        self.expr(e).map(Box::new)
    }

    fn comps(&self, g: &[ra::Comprehension]) -> R<Vec<pa::Comprehension>> {
        g.iter()
            .map(|c| {
                Ok(pa::Comprehension { target: self.expr(&c.target)?, iter: self.expr(&c.iter)?, ifs: self.exprs(&c.ifs)?, is_async: c.is_async })
            })
            .collect()
    }

    fn expr(&self, e: &E) -> R<pa::ExprType> {
        use pa::ExprType as X;
        Ok(match e {
            E::BoolOp(b) => X::BoolOp(pa::BoolOp {
                op: match b.op {
                    ra::BoolOp::And => pa::BoolOps::And,
                    ra::BoolOp::Or => pa::BoolOps::Or,
                },
                values: self.exprs(&b.values)?,
            }),
            E::BinOp(b) => X::BinOp(pa::BinOp { op: binop(b.op), left: self.bexpr(&b.left)?, right: self.bexpr(&b.right)? }),
            E::UnaryOp(u) => X::UnaryOp(pa::UnaryOp {
                op: match u.op {
                    ra::UnaryOp::Invert => pa::Ops::Invert,
                    ra::UnaryOp::Not => pa::Ops::Not,
                    ra::UnaryOp::UAdd => pa::Ops::UAdd,
                    ra::UnaryOp::USub => pa::Ops::USub,
                },
                operand: self.bexpr(&u.operand)?,
            }),
            E::Lambda(l) => X::Lambda(at!(self, l.range, Lambda { args: self.params(l.parameters.as_deref())?, body: self.bexpr(&l.body)? })),
            E::If(i) => {
                X::IfExp(at!(self, i.range, IfExp { test: self.bexpr(&i.test)?, body: self.bexpr(&i.body)?, orelse: self.bexpr(&i.orelse)? }))
            }
            E::Dict(d) => {
                let mut keys = vec![];
                let mut values = vec![];
                for it in d.items.iter() {
                    keys.push(it.key.as_ref().map(|k| self.expr(k)).transpose()?);
                    values.push(self.expr(&it.value)?);
                }
                X::Dict(at!(self, d.range, Dict { keys, values }))
            }
            E::Set(s) => X::Set(at!(self, s.range, Set { elts: self.exprs(&s.elts)? })),
            E::ListComp(c) => X::ListComp(at!(self, c.range, ListComp { elt: self.bexpr(&c.elt)?, generators: self.comps(&c.generators)? })),
            E::SetComp(c) => X::SetComp(at!(self, c.range, SetComp { elt: self.bexpr(&c.elt)?, generators: self.comps(&c.generators)? })),
            E::Generator(c) => X::GeneratorExp(at!(self, c.range, GeneratorExp { elt: self.bexpr(&c.elt)?, generators: self.comps(&c.generators)? })),
            E::DictComp(c) => X::DictComp(at!(
                self,
                c.range,
                DictComp {
                    key: self.bexpr(c.key.as_deref().ok_or("dictionary unpacking in a comprehension is not supported")?)?,
                    value: self.bexpr(&c.value)?,
                    generators: self.comps(&c.generators)?,
                }
            )),
            E::Await(a) => X::Await(pa::Await { value: self.bexpr(&a.value)? }),
            E::Yield(y) => X::Yield(at!(self, y.range, Yield { value: y.value.as_deref().map(|v| self.bexpr(v)).transpose()? })),
            E::YieldFrom(y) => X::YieldFrom(at!(self, y.range, YieldFrom { value: self.bexpr(&y.value)? })),
            E::Compare(c) => X::Compare(pa::Compare {
                ops: c.ops.iter().map(|o| cmpop(*o)).collect(),
                left: self.bexpr(c.first_operand())?,
                comparators: self.exprs(&c.operands[1..])?,
            }),
            E::Call(c) => X::Call(pa::Call {
                func: self.bexpr(&c.func)?,
                args: self.exprs(&c.arguments.args)?,
                keywords: c
                    .arguments
                    .keywords
                    .iter()
                    .map(|k| Ok(at!(self, k.range, Keyword { arg: k.arg.as_ref().map(|a| a.to_string()), value: self.expr(&k.value)? })))
                    .collect::<R<_>>()?,
            }),
            E::FString(f) => X::JoinedStr(self.fstring(f)?),
            E::StringLiteral(s) => X::Constant(pa::Constant::string(s.value.to_str())),
            E::BytesLiteral(b) => X::Constant(pa::Constant::bytes(&b.value.bytes().collect::<Vec<u8>>())),
            E::NumberLiteral(n) => X::Constant(match &n.value {
                ra::Number::Int(i) => {
                    pa::Constant::int(i.as_i64().filter(|v| isize::try_from(*v).is_ok()).ok_or_else(|| format!("unsupported constant value `{i}`"))?)
                }
                ra::Number::Float(f) => pa::Constant::float(*f),
                ra::Number::Complex { .. } => return Err("unsupported constant value: complex numbers".into()),
            }),
            E::BooleanLiteral(b) => X::Constant(pa::Constant::bool(b.value)),
            E::NoneLiteral(_) => X::Constant(pa::Constant::none()),
            E::EllipsisLiteral(_) => return Err("unsupported constant value `Ellipsis`".into()),
            E::Attribute(a) => X::Attribute(pa::Attribute::new(self.expr(&a.value)?, a.attr.to_string(), ctx(a.ctx))),
            E::Subscript(s) => {
                let kind = match &*s.slice {
                    E::Slice(sl) => pa::SubscriptKind::Slice {
                        lower: sl.lower.as_deref().map(|v| self.bexpr(v)).transpose()?,
                        upper: sl.upper.as_deref().map(|v| self.bexpr(v)).transpose()?,
                        step: sl.step.as_deref().map(|v| self.bexpr(v)).transpose()?,
                    },
                    i => pa::SubscriptKind::Index(self.bexpr(i)?),
                };
                X::Subscript(at!(self, s.range, Subscript { value: self.bexpr(&s.value)?, kind }))
            }
            E::Starred(s) => X::Starred(at!(self, s.range, Starred { value: self.bexpr(&s.value)?, ctx: Some(ctx(s.ctx).to_string()) })),
            E::Name(n) => X::Name(pa::Name { id: n.id.to_string() }),
            E::List(l) => X::List(self.exprs(&l.elts)?),
            E::Tuple(t) => X::Tuple(at!(self, t.range, Tuple { elts: self.exprs(&t.elts)? })),
            other => return Err(format!("Unimplemented expression type {}", expr_kind(other))),
        })
    }

    /// An f-string (possibly implicitly concatenated with plain strings) as
    /// CPython's JoinedStr: adjacent literal text merged into one Constant.
    fn fstring(&self, f: &ra::ExprFString) -> R<pa::JoinedStr> {
        let mut values = vec![];
        let mut text = String::new();
        for part in f.value.iter() {
            match part {
                ra::FStringPartRef::Literal(s) => text.push_str(&s.value),
                ra::FStringPartRef::FString(fs) => self.elements(&fs.elements, &mut text, &mut values)?,
            }
        }
        if !text.is_empty() {
            values.push(pa::ExprType::Constant(pa::Constant::string(&text)));
        }
        Ok(at!(self, f.range, JoinedStr { values }))
    }

    fn elements(&self, els: &ra::InterpolatedStringElements, text: &mut String, values: &mut Vec<pa::ExprType>) -> R<()> {
        for el in els.iter() {
            match el {
                ra::InterpolatedStringElement::Literal(l) => text.push_str(&l.value),
                ra::InterpolatedStringElement::Interpolation(i) => {
                    let mut conversion = i.conversion;
                    if let Some(d) = &i.debug_text {
                        text.push_str(d.as_str());
                        if conversion == ra::ConversionFlag::None && i.format_spec.is_none() {
                            conversion = ra::ConversionFlag::Repr;
                        }
                    }
                    if !text.is_empty() {
                        values.push(pa::ExprType::Constant(pa::Constant::string(text)));
                        text.clear();
                    }
                    let format_spec = match &i.format_spec {
                        None => None,
                        Some(spec) => {
                            let (mut t, mut v) = (String::new(), vec![]);
                            self.elements(&spec.elements, &mut t, &mut v)?;
                            if !t.is_empty() {
                                v.push(pa::ExprType::Constant(pa::Constant::string(&t)));
                            }
                            Some(Box::new(pa::ExprType::JoinedStr(at!(self, spec.range, JoinedStr { values: v }))))
                        }
                    };
                    values.push(pa::ExprType::FormattedValue(at!(
                        self,
                        i.range,
                        FormattedValue { value: self.bexpr(&i.expression)?, conversion: conversion.to_byte().map(i32::from), format_spec }
                    )));
                }
            }
        }
        Ok(())
    }
}

fn alias(a: &ra::Alias) -> pa::Alias {
    pa::Alias { name: a.name.to_string(), asname: a.asname.as_ref().map(|n| n.to_string()) }
}

fn expr_kind(e: &E) -> &'static str {
    match e {
        E::Named(_) => "NamedExpr",
        E::ListComp(_) => "ListComp",
        E::SetComp(_) => "SetComp",
        E::DictComp(_) => "DictComp",
        E::Starred(_) => "Starred",
        E::TString(_) => "TemplateStr",
        E::Slice(_) => "Slice",
        E::IpyEscapeCommand(_) => "IpyEscapeCommand",
        _ => "expression",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Every corpus input translates to exactly what CPython-based rythonc 1.1.0 produced
    /// (tests/python/golden, made by scripts/py-golden.sh), or is rejected like it was.
    #[test]
    fn corpus_matches_cpython_rythonc() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let golden = root.join("tests/python/golden");
        let mut inputs = vec![];
        for d in ["tests/python/rythonc", "vendor/python-ast/pytests"] {
            for e in std::fs::read_dir(root.join(d)).unwrap() {
                let p = e.unwrap().path();
                if p.extension().is_some_and(|x| x == "py") {
                    inputs.push(p);
                }
            }
        }
        inputs.sort();
        assert!(inputs.len() >= 40, "corpus missing");
        let mut bad = vec![];
        for p in &inputs {
            let name = format!("{}_{}", p.parent().unwrap().file_name().unwrap().to_string_lossy(), p.file_stem().unwrap().to_string_lossy());
            let src = std::fs::read_to_string(p).unwrap();
            let ours = to_rust(&src, "user.py");
            let want = std::fs::read_to_string(golden.join(format!("{name}.rs")));
            match (ours, want) {
                (Ok(a), Ok(b)) if a.trim() == b.trim() => {}
                (Err(_), Err(_)) if golden.join(format!("{name}.err")).exists() => {}
                (Ok(_), Err(_)) => bad.push(format!("{name}: accepted, CPython rythonc rejected it")),
                (Err(e), Ok(_)) => bad.push(format!("{name}: rejected ({e}), CPython rythonc accepted it")),
                (Ok(a), Ok(b)) => {
                    let i = a.bytes().zip(b.bytes()).position(|(x, y)| x != y).unwrap_or(a.len().min(b.len()));
                    let s = i.saturating_sub(60);
                    bad.push(format!("{name}: differs at {i}\n  ours: {}\n  want: {}", &a[s..(i + 60).min(a.len())], &b[s..(i + 60).min(b.len())]));
                }
                (Err(e), Err(_)) => bad.push(format!("{name}: no golden ({e})")),
            }
        }
        assert!(bad.is_empty(), "{} of {} differ:\n{}", bad.len(), inputs.len(), bad.join("\n"));
    }

    #[test]
    fn positions_are_cpython_style() {
        let m = parse("x = 1\nif x:\n    y = f'{x=}'\nelif x > 2:\n    pass\n", "user.py").unwrap();
        let b = &m.raw.body;
        assert_eq!((b[1].lineno, b[1].col_offset, b[1].end_lineno), (Some(2), Some(0), Some(5)));
        let pa::StatementType::If(i) = &b[1].statement else { panic!() };
        assert_eq!((i.orelse[0].lineno, i.orelse[0].col_offset), (Some(4), Some(0)));
        let pa::StatementType::Assign(a) = &i.body[0].statement else { panic!() };
        let pa::ExprType::JoinedStr(j) = &a.value else { panic!() };
        assert!(matches!(&j.values[0], pa::ExprType::Constant(c) if c.to_string() == "\"x=\""));
        assert!(matches!(&j.values[1], pa::ExprType::FormattedValue(f) if f.conversion == Some(114)));
        assert_eq!(m.name.as_ref().unwrap().id, "user");
        assert!(parse("def f(:\n", "user.py").unwrap_err().contains("line 1"));
        assert!(to_rust("match x:\n    case 1:\n        pass\n", "user.py").unwrap_err().contains("Match"));
    }
}
