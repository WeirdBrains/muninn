//! Python list expressions are represented as `ExprType::List` and generated
//! by the `"List"` arm of the expression codegen (see `expression.rs`). The
//! second, unused `List<'a>` AST type that used to live here — with its own,
//! divergent codegen — was removed as dead code.

// It's fairly easy to break the automatic parsing of parameter structs, so we need to have fairly sophisticated
// test coverage for the various types of
#[cfg(python_ast_upstream_tests)]
mod tests {
    use crate::python_ast::ExprType;
    use crate::python_ast::StatementType;
    use std::panic;
    use test_log::test;

    #[test]
    fn parse_list() {
        let module = crate::python_ast::parse("[1, 2, 3]", "nothing.py").unwrap();
        let statement = module.raw.body[0].statement.clone();
        match statement {
            StatementType::Expr(e) => match e.value {
                ExprType::List(list) => {
                    tracing::debug!("{:#?}", list);
                    assert_eq!(list.len(), 3);
                }
                _ => panic!("Could not find inner expression"),
            },
            _ => panic!("Could not find outer expression."),
        }
    }
}
