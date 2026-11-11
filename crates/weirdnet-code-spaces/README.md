# weirdnet-code-spaces

Code spaces for applications built on Weirdnet: small programs compiled into native
plugins against the [`weirdnet-plugin`](../weirdnet-plugin) SDK and run against the
application's data through host methods.

- **Languages** (`Lang`): math formulas (`math` feature, default), Polars expressions
  (`polars`), Rust, typed Python translated in process with no libpython (`python`; ruff's
  parser and a vendored python-ast, see `src/python_ast/VENDORED.md`), and prebuilt
  libraries.
- **Builds** (`native-plugins`, default): generated crates against the SDK this crate
  carries (`sdk/`, written next to builds so they work offline), cargo with a shared target
  directory, libraries cached by a content key (ABI, SDK, codegen, language, platform,
  dependencies, source).
- **Loading**: once per process, after checking the profile's symbols and ABI version.
- **Trust**: a trust hash of language and source (a prebuilt library's bytes) that an
  owner approves; spaces approve their line-up (`spaces::lineup_hash`). Who may approve,
  and where approvals are stored, is the application's.
- **Spaces**: `@imports` headers, renaming an imported space, and the cycle and depth check
  over the application's spaces (`spaces::Graph`).
- **Host**: `Host` is the application's methods; `run` hands a loaded plugin a host table
  over it.
- **Result views** (`view`): the table, chart, text and tiles a plugin returns, parsed,
  checked and cut to their limits, with table, chart, text-line and sparkline conversions;
  Drawing them (in a UI, or as images with `weirdnet-render`) is the application's.

Everything that names the application or SDK comes from a `Profile`: `WEIRDNET` (SDK
`weirdnet-plugin`, ABI 2), or an application's own, so it keeps its hashes, generated code
and messages.

Plugins run as native code with the application's rights. Approving one is trusting it;
this crate checks hashes and the ABI, it does not sandbox.

Native builds and loading need a desktop platform with a Rust toolchain; prebuilt plugins
need no toolchain.
