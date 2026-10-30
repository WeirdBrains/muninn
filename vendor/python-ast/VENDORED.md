# python-ast 1.1.0 (vendored, modified)

From <https://github.com/rexlunae/rython> (crate `python-ast` 1.1.0), Apache-2.0: see
`LICENSE-APACHE`. Copyright the python-ast authors.

Muninn keeps only its AST types and Rust code generation. Changes made here:

- pyo3 and libpython are removed: every `FromPyObject` impl and derive, the CPython-based
  parser (`src/parser`), the pyo3 extraction helpers in `parser_utils.rs` and the
  `PyAttributeExtractor` trait are gone. Muninn builds this AST from `ruff_python_parser`.
- `isidentifier` is pure Rust (XID rules) instead of asking CPython.
- `PythonOptions::default()` no longer reads `sys.path` (an empty path).
- `Identifier`/`Name` conversion errors are `String`s.
- Rython's tests, examples and `Cargo.lock` are left out; Rython's Python test inputs are kept
  in `pytests/` to compare against in Muninn's tests.
- Formatted with `cargo fmt`.
- Added pure-Rust `Constant` constructors (ports of the removed `try_*` extractors) and `Attribute::new`.
