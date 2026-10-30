# Python translation corpus

`rythonc/*.py` are the test inputs of Rython's `rythonc` 1.1.0 (Apache-2.0, same license
as `vendor/python-ast`); `vendor/python-ast/pytests/*.py` are python-ast 1.1.0's.

`golden/*.rs` is what the reference `rythonc` 1.1.0, which parses with CPython, produces for
each input (`golden/*.err` when it rejects the input). Muninn's pure-Rust path
(`src/code/pyconv.rs`) must produce the same Rust token stream; see the
`corpus_matches_cpython_rythonc` test.

CPython is used only to make the goldens, never in builds or at runtime:

    scripts/py-golden.sh   # builds the reference rythonc under target/, rewrites golden/
