# python-ast 1.1.0 (vendored, modified)

From <https://github.com/rexlunae/rython> (crate `python-ast` 1.1.0), Apache-2.0: see
`LICENSE-APACHE`. Copyright the python-ast authors.

This crate (`weirdnet-code-spaces`, first in Muninn) keeps only its AST types and Rust code
generation. Changes made here:

- pyo3 and libpython are removed: every `FromPyObject` impl and derive, the CPython-based
  parser (`src/parser`), the pyo3 extraction helpers in `parser_utils.rs` and the
  `PyAttributeExtractor` trait are gone. `pyconv.rs` builds this AST from `ruff_python_parser`.
- `isidentifier` is pure Rust (XID rules) instead of asking CPython.
- `PythonOptions::default()` no longer reads `sys.path` (an empty path).
- `Identifier`/`Name` conversion errors are `String`s.
- Rython's tests, examples and `Cargo.lock` are left out; Rython's Python test inputs are kept
  in `tests/python/pytests/` (of this crate) for the corpus test in `pyconv.rs`.
- Formatted with `cargo fmt`.
- Added pure-Rust `Constant` constructors (ports of the removed `try_*` extractors) and `Attribute::new`.
- Weirdnet: moved from `vendor/python-ast` (its own crate, excluded from the workspace) into
  this module, so the packaged `weirdnet-code-spaces` carries it and needs no unpublished
  path dependency. Paths are `crate::python_ast::…` instead of `crate::…`; the two
  `#[macro_export]` macros are `#[doc(hidden)]`, name `$crate::python_ast::Node` and are
  re-exported in `mod.rs`. The upstream unit tests (`#[cfg(test)]` modules) call the CPython
  parser that was removed and were never compiled here; they are kept behind
  `#[cfg(python_ast_upstream_tests)]`, which nothing sets. The translation is tested by the
  corpus test against CPython goldens instead. The README is kept but no longer attached as
  the `ast` module's docs (its examples call that parser too, so they can't run as
  doctests). No code generation changed.
