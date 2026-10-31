//! Code spaces: compile algorithms to plugins and run them against the store.
//!
//! Every language ends up as a native plugin (a `cdylib`) speaking the stable C ABI in
//! [`abi`] (see `plugin/muninn-plugin` and `docs/code-spaces.md`):
//!
//! | lang | pipeline |
//! |------|----------|
//! | `math` | formulas → generated Rust → cdylib |
//! | `polars` | Polars expressions → generated Rust (polars crate) → cdylib |
//! | `rust` | Rust against the SDK → cdylib |
//! | `python` | typed Python → ruff parser → python-ast → Rust → cdylib |
//! | `plugin` | a library built elsewhere (the source is its path) |
//!
//! Builds are cached by content hash under `MUNINN_HOME/code/plugins`, and loaded with
//! `libloading` after an ABI check. Building needs a Rust toolchain (`cargo`); Muninn itself
//! does not: without one, compiled languages report how to get it, and prebuilt plugins
//! still load.
//!
//! Trust: a plugin is native code with the rights of the user running `mnnn`. A code space
//! runs only after its owner approved the exact source (or library file) — new or changed
//! code asks first. Run logs never contain secrets the host has (keys, passwords): the host
//! API has no method that returns them.
#[path = "../../plugin/muninn-plugin/src/abi.rs"]
pub mod abi;
pub mod bars;
pub mod host;
pub mod math;
pub mod pipe;
pub mod pipeline;
pub mod png;
pub mod polars;
pub mod pyconv;
pub mod python;
pub mod view;

use crate::error::{Error, Result};
use crate::store::{CodeChange, CodeInput, CodeSpace, Lang, NewAction, RunInfo, Store, now_ms};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// Bumped when generated code changes (invalidates cached builds).
pub const CODEGEN: u32 = 1;
/// The SDK version plugins are built against.
pub const SDK_VERSION: &str = "1.0.0";

/// The SDK crate, embedded so code spaces build without the Muninn sources.
pub const SDK_FILES: &[(&str, &str)] = &[
    ("Cargo.toml", include_str!("../../plugin/muninn-plugin/Cargo.toml")),
    ("src/lib.rs", include_str!("../../plugin/muninn-plugin/src/lib.rs")),
    ("src/abi.rs", include_str!("../../plugin/muninn-plugin/src/abi.rs")),
    ("src/json.rs", include_str!("../../plugin/muninn-plugin/src/json.rs")),
    ("src/series.rs", include_str!("../../plugin/muninn-plugin/src/series.rs")),
    ("src/columns.rs", include_str!("../../plugin/muninn-plugin/src/columns.rs")),
];

fn hex16(h: blake3::Hash) -> String {
    h.to_hex()[..32].to_string()
}

/// Hash of the embedded SDK.
pub fn sdk_hash() -> String {
    let mut h = blake3::Hasher::new();
    for (n, c) in SDK_FILES {
        h.update(n.as_bytes());
        h.update(&[0]);
        h.update(c.as_bytes());
        h.update(&[0]);
    }
    hex16(h.finalize())
}

/// Write the SDK crate into `dir` (for `mnnn code sdk` and builds).
pub fn write_sdk(dir: &Path) -> Result<()> {
    for (n, c) in SDK_FILES {
        let p = dir.join(n);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if std::fs::read_to_string(&p).ok().as_deref() != Some(*c) {
            std::fs::write(&p, c)?;
        }
    }
    Ok(())
}

static HOME: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();

/// Tell the library where `MUNINN_HOME` is (front ends call this once; rules use it).
pub fn set_home(home: &Path) {
    *HOME.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner()) = Some(home.to_path_buf());
}

/// Where code spaces are built and cached.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeEnv {
    /// `MUNINN_HOME/code`.
    pub root: PathBuf,
    /// Cargo's target directory (shared by every build; `MUNINN_CODE_TARGET` moves it).
    pub target: PathBuf,
}

impl CodeEnv {
    pub fn new(home: &Path) -> Self {
        let root = home.join("code");
        let target = std::env::var_os("MUNINN_CODE_TARGET").filter(|t| !t.is_empty()).map(PathBuf::from).unwrap_or_else(|| root.join("target"));
        CodeEnv { root, target }
    }

    /// The home set with [`set_home`], else the one the database is in.
    pub fn for_store(store: &Store) -> Result<Self> {
        if let Some(h) = HOME.get().and_then(|m| m.lock().ok().and_then(|g| g.clone())) {
            return Ok(Self::new(&h));
        }
        let db = store.conn.path().filter(|p| !p.is_empty()).map(PathBuf::from);
        match db.as_ref().and_then(|p| p.parent()) {
            Some(dir) if dir.file_name().is_some_and(|n| n == "storage") => Ok(Self::new(dir.parent().unwrap_or(dir))),
            Some(dir) => Ok(Self::new(dir)),
            None => Err(Error::missing("code space home", "set MUNINN_HOME (code spaces are built under MUNINN_HOME/code)")),
        }
    }

    pub fn sdk_dir(&self) -> PathBuf {
        self.root.join("sdk").join(format!("muninn-plugin-{}", &sdk_hash()[..12]))
    }
    pub fn build_dir(&self, key: &str) -> PathBuf {
        self.root.join("build").join(key)
    }
    pub fn plugins_dir(&self) -> PathBuf {
        self.root.join("plugins")
    }
    /// The cached library of a build key.
    pub fn plugin_path(&self, key: &str) -> PathBuf {
        self.plugins_dir().join(format!("{key}.{}", std::env::consts::DLL_EXTENSION))
    }
}

fn plugin_file(source: &str) -> PathBuf {
    let s = source.trim();
    match s.strip_prefix("~/") {
        Some(rest) => dirs::home_dir().map(|h| h.join(rest)).unwrap_or_else(|| PathBuf::from(s)),
        None => PathBuf::from(s),
    }
}

/// What the owner approves: the language and source (or the library's bytes).
pub fn trust_hash(space: &CodeSpace) -> Result<String> {
    trust_hash_of(space.lang, &space.source)
}

pub fn trust_hash_of(lang: Lang, source: &str) -> Result<String> {
    let mut h = blake3::Hasher::new();
    h.update(b"muninn-code-trust\0");
    h.update(lang.as_str().as_bytes());
    h.update(&[0]);
    if lang == Lang::Plugin {
        let p = plugin_file(source);
        let bytes = std::fs::read(&p).map_err(|e| Error::invalid("plugin", format!("{}: {e}", p.display())))?;
        h.update(&bytes);
    } else {
        h.update(source.as_bytes());
    }
    Ok(hex16(h.finalize()))
}

/// The cache key of a build.
pub fn build_key(lang: Lang, source: &str) -> String {
    let mut h = blake3::Hasher::new();
    h.update(
        format!("abi{}|sdk{}|codegen{CODEGEN}|{}|{}-{}\0", abi::ABI_VERSION, sdk_hash(), lang.as_str(), std::env::consts::ARCH, std::env::consts::OS)
            .as_bytes(),
    );
    h.update(
        match lang {
            Lang::Polars => polars::DEPENDENCY,
            Lang::Python => python::DEPENDENCY,
            _ => "",
        }
        .as_bytes(),
    );
    h.update(&[0]);
    h.update(source.as_bytes());
    hex16(h.finalize())
}

/// The cargo to build with: `MUNINN_CARGO` or `cargo` on the PATH, checked by running it.
pub fn cargo() -> Result<PathBuf> {
    let c = std::env::var_os("MUNINN_CARGO").filter(|c| !c.is_empty()).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("cargo"));
    match std::process::Command::new(&c).arg("--version").output() {
        Ok(o) if o.status.success() => Ok(c),
        _ => Err(Error::missing(
            "Rust toolchain",
            format!(
                "code spaces in math, polars, rust and python compile to plugins with cargo ({} not found). Install Rust from https://rustup.rs, or set MUNINN_CARGO to cargo; prebuilt plugins run without it (`--lang plugin`). See docs/code-spaces.md",
                c.display()
            ),
        )),
    }
}

/// Check a source without building: the language's own checks (a prebuilt plugin is
/// copied into the cache and loaded, never loaded in place).
pub fn check(env: &CodeEnv, lang: Lang, source: &str) -> Result<Vec<String>> {
    Ok(match lang {
        Lang::Math => {
            let p = math::parse(source)?;
            let fields: Vec<String> = p.fields().into_iter().collect();
            vec![format!("{} formula(s); input fields used: {}", p.lines.len(), if fields.is_empty() { "none".into() } else { fields.join(", ") })]
        }
        Lang::Polars => {
            let p = polars::parse(source)?;
            vec![format!("{} step(s); expressions are checked by the compiler at build time", p.steps.len())]
        }
        Lang::Python => {
            let p = python::parse(source)?;
            let public: Vec<&str> = p.funcs.iter().filter(|f| !f.name.starts_with('_')).map(|f| f.name.as_str()).collect();
            pyconv::to_rust(source, "user.py").map_err(|e| Error::invalid("python", e))?;
            vec![format!("columns: {}", public.join(", "))]
        }
        Lang::Rust => {
            if !source.contains("fn run") {
                return Err(Error::invalid("rust", "define `pub fn run(host: &Host) -> Result<View>` (with `use muninn_plugin::prelude::*;`)"));
            }
            vec!["defines run; the compiler checks the rest at build time".into()]
        }
        Lang::Plugin => {
            let p = plugin_file(source);
            let l = load(&build(env, lang, source, &mut |_| {})?.path)?;
            vec![format!("{}: ABI {} ({})", p.display(), abi::ABI_VERSION, l.info)]
        }
    })
}

/// A built plugin.
#[derive(Debug, Clone, PartialEq)]
pub struct Built {
    pub path: PathBuf,
    pub key: String,
    /// Already built (nothing compiled).
    pub cached: bool,
}

fn manifest(key: &str, sdk: &Path, extra: &str) -> String {
    format!(
        "[package]\nname = \"mnnn_code_{key}\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n\n[lib]\ncrate-type = [\"cdylib\"]\npath = \"src/lib.rs\"\n\n[dependencies]\nmuninn-plugin = {{ path = {:?} }}\n{extra}\n\n[profile.release]\nopt-level = 3\ndebug = false\n\n[workspace]\n",
        sdk.display().to_string()
    )
}

/// Write the crate of a source; returns the extra dependencies line.
fn generate(lang: Lang, source: &str, dir: &Path) -> Result<()> {
    let src = dir.join("src");
    std::fs::create_dir_all(&src)?;
    let (user, extra) = match lang {
        Lang::Math => (math::generate(source)?, ""),
        Lang::Polars => (polars::generate(source)?, polars::DEPENDENCY),
        Lang::Rust => {
            if !source.contains("fn run") {
                return Err(Error::invalid("rust", "define `pub fn run(host: &Host) -> Result<View>`"));
            }
            (source.to_string(), "")
        }
        Lang::Python => {
            let user = python::generate(source)?;
            let py = python::transpile(source, dir)?;
            std::fs::write(src.join("py.rs"), py)?;
            (user, python::DEPENDENCY)
        }
        Lang::Plugin => return Err(Error::invalid("plugin", "prebuilt plugins are not compiled")),
    };
    std::fs::write(src.join("user.rs"), user)?;
    std::fs::write(
        src.join("lib.rs"),
        format!("#![allow(unused, clippy::all)]\nmod user;\nmuninn_plugin::plugin!(user::run, {:?});\n", lang.as_str()),
    )?;
    let key = dir.file_name().and_then(|n| n.to_str()).unwrap_or("x");
    std::fs::write(dir.join("Cargo.toml"), manifest(key, &dir.join("../../sdk").join(sdk_dir_name()), extra))?;
    Ok(())
}

fn sdk_dir_name() -> String {
    format!("muninn-plugin-{}", &sdk_hash()[..12])
}

fn copy_in(from: &Path, to: &Path) -> Result<()> {
    let dir = to.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".{}.tmp{}", to.file_name().and_then(|n| n.to_str()).unwrap_or("plugin"), std::process::id()));
    std::fs::copy(from, &tmp)?;
    std::fs::rename(&tmp, to)?;
    Ok(())
}

/// Build (or find in the cache) the plugin of a source. Cached libraries are content
/// addressed and replaced by rename, never rewritten, so a loaded one stays valid. `log` gets cargo's output lines.
pub fn build(env: &CodeEnv, lang: Lang, source: &str, log: &mut dyn FnMut(&str)) -> Result<Built> {
    if lang == Lang::Plugin {
        let p = plugin_file(source);
        let key = trust_hash_of(lang, source)?;
        let out = env.plugin_path(&key);
        let cached = out.exists();
        if !cached {
            copy_in(&p, &out)?;
        }
        return Ok(Built { path: out, key, cached });
    }
    let key = build_key(lang, source);
    let out = env.plugin_path(&key);
    if out.exists() {
        return Ok(Built { path: out, key, cached: true });
    }
    let cargo = cargo()?;
    write_sdk(&env.sdk_dir())?;
    let dir = env.build_dir(&key);
    generate(lang, source, &dir)?;
    log(&format!("building {} code ({key})…", lang.as_str()));
    let mut child = std::process::Command::new(&cargo)
        .args(["build", "--release", "--color", "never", "--manifest-path"])
        .arg(dir.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", &env.target)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| Error::missing("Rust toolchain", format!("could not run {}: {e}", cargo.display())))?;
    let mut lines = vec![];
    if let Some(err) = child.stderr.take() {
        use std::io::BufRead;
        for l in std::io::BufReader::new(err).lines().map_while(std::result::Result::ok) {
            log(&l);
            lines.push(l);
        }
    }
    let status = child.wait()?;
    if !status.success() {
        let tail = lines.len().saturating_sub(80);
        return Err(Error::Other(format!("build failed (lines refer to the generated crate in {}):\n{}", dir.display(), lines[tail..].join("\n"))));
    }
    let lib = env.target.join("release").join(format!("{}mnnn_code_{key}{}", std::env::consts::DLL_PREFIX, std::env::consts::DLL_SUFFIX));
    copy_in(&lib, &out)?;
    log(&format!("built {}", out.display()));
    Ok(Built { path: out, key, cached: false })
}

/// A loaded plugin.
#[derive(Clone, Copy)]
pub struct Loaded {
    pub run: abi::PluginRun,
    pub info: &'static str,
}

impl std::fmt::Debug for Loaded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Loaded({})", self.info)
    }
}

type Libraries = Mutex<HashMap<PathBuf, Loaded>>;
static LOADED: OnceLock<Libraries> = OnceLock::new();

/// Load a plugin (once per process; libraries stay loaded) after checking its ABI.
pub fn load(path: &Path) -> Result<Loaded> {
    let map = LOADED.get_or_init(Default::default);
    let mut map = map.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(l) = map.get(path) {
        return Ok(*l);
    }
    let bad = |m: String| Error::invalid("plugin", format!("{}: {m}", path.display()));
    // SAFETY: loading runs the library's initializers; the owner approved this code (see
    // the trust notes above). The library is never unloaded, so its symbols stay valid.
    let lib: &'static libloading::Library = Box::leak(Box::new(unsafe { libloading::Library::new(path) }.map_err(|e| bad(e.to_string()))?));
    let loaded = unsafe {
        let abi_fn = lib.get::<abi::PluginAbi>(abi::SYM_ABI).map_err(|_| bad("not a Muninn plugin (no muninn_plugin_abi)".into()))?;
        let v = abi_fn();
        if v != abi::ABI_VERSION {
            return Err(bad(format!(
                "built for plugin ABI {v}; this Muninn speaks ABI {} — rebuild it against muninn-plugin {SDK_VERSION}",
                abi::ABI_VERSION
            )));
        }
        let info_fn = lib.get::<abi::PluginInfo>(abi::SYM_INFO).map_err(|_| bad("no muninn_plugin_info".into()))?;
        let info = std::str::from_utf8(info_fn().as_bytes()).map_err(|_| bad("info is not UTF-8".into()))?;
        let run = *lib.get::<abi::PluginRun>(abi::SYM_RUN).map_err(|_| bad("no muninn_plugin_run".into()))?;
        Loaded { run, info }
    };
    map.insert(path.to_path_buf(), loaded);
    Ok(loaded)
}

thread_local! {
    static RUNNING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

struct Guard;
impl Drop for Guard {
    fn drop(&mut self) {
        RUNNING.with(|r| r.set(false));
    }
}

/// Whether a code space can run as it is (approved for its current source).
pub fn approved(space: &CodeSpace) -> bool {
    trust_hash(space).ok().is_some_and(|h| space.approved_hash.as_deref() == Some(h.as_str()))
}

fn need_approval(space: &CodeSpace) -> Error {
    Error::NeedsConfirmation(format!(
        "code space {} is {} and runs as native code with your rights; review it (`mnnn code show {}`) and approve it (`mnnn code approve {}`)",
        space.name,
        if space.approved_hash.is_some() { "changed since it was approved" } else { "new" },
        space.name,
        space.name
    ))
}

/// Create a code space (recorded in history).
pub fn create(store: &Store, origin: &str, name: &str, description: &str, lang: Lang, source: &str, input: CodeInput) -> Result<CodeSpace> {
    let c = store.create_code_space(name, description, lang, source, input)?;
    store.note_action(
        &NewAction::new("code.create", origin, format!("create code space {} ({})", c.name, c.lang.as_str()))
            .params(json!({ "id": c.id, "name": c.name, "lang": c.lang.as_str() })),
    );
    Ok(c)
}

/// Change a code space (recorded; a new source needs approving again).
pub fn edit(store: &Store, origin: &str, id: &str, ch: &CodeChange) -> Result<CodeSpace> {
    let before = store.code_space(id)?.ok_or_else(|| Error::invalid("code space", format!("`{id}` not found")))?;
    let c = store.update_code_space(id, ch)?;
    let mut what = vec![];
    if c.name != before.name {
        what.push(format!("renamed from {}", before.name));
    }
    if c.source != before.source || c.lang != before.lang {
        what.push("source".to_string());
    }
    if c.input != before.input {
        what.push(format!("input {}", c.input.text()));
    }
    if c.description != before.description {
        what.push("description".into());
    }
    let summary = format!("edit code space {}{}", c.name, if what.is_empty() { String::new() } else { format!(": {}", what.join(", ")) });
    store.note_action(&NewAction::new("code.edit", origin, summary).params(json!({ "id": c.id, "name": c.name, "changed": what })));
    Ok(c)
}

/// Remove a code space (and its project items).
pub fn remove(store: &Store, origin: &str, id: &str) -> Result<CodeSpace> {
    let c = store.code_space(id)?.ok_or_else(|| Error::invalid("code space", format!("`{id}` not found")))?;
    store.remove_code_space(&c.id)?;
    store.note_action(
        &NewAction::new("code.remove", origin, format!("remove code space {}", c.name))
            .params(json!({ "id": c.id, "name": c.name, "lang": c.lang.as_str(), "source": c.source })),
    );
    Ok(c)
}

/// Approve the current source of a code space to run.
pub fn approve(store: &Store, origin: &str, space: &CodeSpace) -> Result<String> {
    let h = trust_hash(space)?;
    store.approve_code_space(&space.id, Some(&h))?;
    store.note_action(
        &NewAction::new("code.approve", origin, format!("approve code space {} to run", space.name))
            .params(json!({ "id": space.id, "name": space.name, "hash": h })),
    );
    Ok(h)
}

/// Build a code space (recorded in history when something was compiled).
pub fn build_space(store: &Store, env: &CodeEnv, origin: &str, space: &CodeSpace, log: &mut dyn FnMut(&str)) -> Result<Built> {
    let t = std::time::Instant::now();
    let b = build(env, space.lang, &space.source, log)?;
    note_build(store, origin, space, &b, t.elapsed().as_secs_f64());
    Ok(b)
}

/// Record a build made elsewhere (e.g. on a TUI thread); a cached one is not recorded.
pub fn note_build(store: &Store, origin: &str, space: &CodeSpace, b: &Built, secs: f64) {
    if !b.cached {
        store.note_action(
            &NewAction::new("code.build", origin, format!("build code space {} ({}, {secs:.1}s)", space.name, space.lang.as_str()))
                .params(json!({ "id": space.id, "name": space.name, "key": b.key })),
        );
    }
}

/// Run a code space: approval check, build if needed, load, run, record. A plugin's own
/// error is a run with status `error` (returned, not raised).
pub fn run(store: &Store, env: &CodeEnv, origin: &str, space: &CodeSpace, log: &mut dyn FnMut(&str)) -> Result<RunInfo> {
    // the default input (a convenience: a code space works on whatever is piped in)
    if !approved(space) {
        return Err(need_approval(space));
    }
    let src = pipe::Source::of(space.input.clone());
    let input = match &src {
        // a model query loads lazily, when the plugin asks
        pipe::Source::Model(_) | pipe::Source::None => None,
        s => pipe::frame(store, env, origin, s, log)?,
    };
    run_on(store, env, origin, space, input, &src.text(), log)
}

/// Run a code space over named inputs (fan-in): `host.input()` is the first,
/// `host.input_named(NAME)` any of them, `host.inputs()` their names.
pub fn run_named(
    store: &Store,
    env: &CodeEnv,
    origin: &str,
    space: &CodeSpace,
    inputs: Vec<(String, serde_json::Value)>,
    from: &str,
    log: &mut dyn FnMut(&str),
) -> Result<RunInfo> {
    NAMED.with(|n| *n.borrow_mut() = inputs.clone());
    let first = inputs.into_iter().next().map(|x| x.1);
    let r = run_on(store, env, origin, space, first, from, log);
    NAMED.with(|n| n.borrow_mut().clear());
    r
}

thread_local! {
    /// The named inputs of the run starting now (see [`run_named`]).
    static NAMED: std::cell::RefCell<Vec<(String, serde_json::Value)>> = const { std::cell::RefCell::new(vec![]) };
}

/// Run a code space over `input` (a frame; `None`: its default model query, if any).
/// `from` names where the input came from, for the history.
pub fn run_on(
    store: &Store,
    env: &CodeEnv,
    origin: &str,
    space: &CodeSpace,
    input: Option<serde_json::Value>,
    from: &str,
    log: &mut dyn FnMut(&str),
) -> Result<RunInfo> {
    if !approved(space) {
        return Err(need_approval(space));
    }
    if RUNNING.with(|r| r.replace(true)) {
        return Err(Error::invalid(
            "code space",
            format!("{} was started while another code space runs (code spaces do not start each other)", space.name),
        ));
    }
    let guard = Guard;
    let built = build_space(store, env, origin, space, log)?;
    let plugin = load(&built.path)?;
    // recorded now (the plugin's writes follow it in history); rules on `code.run` fire
    // once the result is in
    let quiet = store.rules_rt.off.replace(true);
    let action = store.record_action(
        &NewAction::new("code.run", origin, format!("run code space {}", space.name))
            .params(json!({ "id": space.id, "name": space.name, "lang": space.lang.as_str(), "input": from })),
    );
    store.rules_rt.off.set(quiet);
    let action = action?;
    let t = std::time::Instant::now();
    let mut r = host::Run::new(store, space);
    r.input = input;
    r.named = NAMED.with(|n| std::mem::take(&mut *n.borrow_mut()));
    let table = r.table();
    // SAFETY: an approved plugin with a checked ABI; `table` and `r` outlive the call.
    let code = unsafe { (plugin.run)(&table) };
    let ms = t.elapsed().as_millis() as u64;
    let mut logs = std::mem::take(&mut r.logs);
    logs.extend(r.notes.iter().map(|n| format!("note: {n}")));
    let (status, error) = match (code, &r.result, &r.error) {
        (abi::RUN_OK, Some(_), _) => ("ok", None),
        (abi::RUN_OK, None, _) => ("error", Some("the plugin returned no result".to_string())),
        (abi::RUN_PANIC, _, e) => ("error", Some(e.clone().unwrap_or_else(|| "the plugin panicked".into()))),
        (abi::RUN_ERROR, _, e) => ("error", Some(e.clone().unwrap_or_else(|| "the plugin reported an error".into()))),
        (c, _, _) => ("error", Some(format!("the plugin returned an unknown code {c}"))),
    };
    let view = if status == "ok" { r.result.as_ref().map(serde_json::to_value).transpose()? } else { None };
    let writes = r.writes;
    let summary = match (&error, &r.result) {
        (None, Some(v)) => {
            format!("run code space {}: {}{}", space.name, v.summary(), if writes > 0 { format!(", {writes} write(s)") } else { String::new() })
        }
        (e, _) => format!("run code space {}: error: {}", space.name, e.as_deref().unwrap_or("").lines().next().unwrap_or("")),
    };
    let params = json!({ "id": space.id, "name": space.name, "lang": space.lang.as_str(), "input": from, "status": status, "writes": writes, "ms": ms, "error": error });
    store.update_action(action, &summary, &params)?;
    let info = RunInfo { at: now_ms(), status: status.into(), view, error, logs, writes, ms, action: Some(action) };
    store.set_code_run(&space.id, &info)?;
    drop(guard);
    crate::rules::after_action(store, action);
    Ok(info)
}

/// Read an input written as [`CodeInput::text`]: `MODEL [where C; C] [matching "TEXT"]
/// [group F,F] [sort F,-F] [limit N]`; empty or `none` is no input.
pub fn parse_input(s: &str) -> Result<CodeInput> {
    let s = s.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("none") {
        return Ok(CodeInput::default());
    }
    // another source: a live search, a journal, a code space's output
    if let Some((k, n)) = s.split_once(':') {
        let k = k.trim().to_ascii_lowercase();
        if ["search", "journal", "code"].contains(&k.as_str()) {
            if n.trim().is_empty() {
                return Err(Error::invalid("input", format!("`{k}:` needs a name")));
            }
            return Ok(CodeInput { from: format!("{k}:{}", n.trim()), ..Default::default() });
        }
        if k == "model" {
            return parse_input(n);
        }
    }
    const KEYS: [&str; 5] = ["where", "matching", "group", "sort", "limit"];
    // split at the keywords (outside quotes)
    let mut parts: Vec<(String, String)> = vec![("model".into(), String::new())];
    let mut quoted = false;
    for w in s.split(' ') {
        let lw = w.to_ascii_lowercase();
        if !quoted && KEYS.contains(&lw.as_str()) {
            if parts.iter().any(|(k, _)| *k == lw) {
                return Err(Error::invalid("input", format!("`{lw}` is given twice")));
            }
            parts.push((lw, String::new()));
            continue;
        }
        quoted ^= w.matches('"').count() % 2 == 1;
        let cur = &mut parts.last_mut().expect("model part").1;
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(w);
    }
    let list = |v: &str| v.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect::<Vec<_>>();
    let mut i = CodeInput::default();
    for (k, v) in parts {
        // `where a = 1; sort x` reads as well as `where a = 1 sort x`
        let v = v.trim().trim_end_matches([';', ',']).trim_end();
        if v.is_empty() {
            return Err(Error::invalid("input", format!("`{k}` needs a value (MODEL where FIELD OP VALUE; … group F sort F limit N)")));
        }
        match k.as_str() {
            "model" if v.contains(' ') => {
                return Err(Error::invalid("input", format!("`{v}`: one model name, then where/matching/group/sort/limit")));
            }
            "model" => i.model = v.to_string(),
            "where" => i.filter = v.split(';').map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).collect(),
            "matching" => i.text = v.trim_matches('"').to_string(),
            "group" => i.group = list(v),
            "sort" => i.sort = list(v),
            _ => i.limit = Some(v.parse().map_err(|_| Error::invalid("input", format!("limit `{v}` is not a number")))?),
        }
    }
    for c in &i.filter {
        crate::designer::parse_where(std::slice::from_ref(c))?;
    }
    Ok(i)
}

/// The view of a run, when it has one.
pub fn run_view(info: &RunInfo) -> Option<view::ResultView> {
    info.view.clone().and_then(|v| serde_json::from_value(v).ok())
}

/// A starting source for a new code space.
pub fn template(lang: Lang) -> &'static str {
    match lang {
        Lang::Math => {
            "# One formula per line over the input's fields (see `mnnn help code-spaces`).\n@title Moving averages\n@show close, fast, slow\nfast = sma(close, 10)\nslow = sma(close, 30)\ncross = if(fast > slow, 1, 0)\n"
        }
        Lang::Polars => {
            "# One Polars expression per line: NAME = EXPR or where EXPR.\n@title Returns\nret = col(\"close\").pct_change(lit(1)) * lit(100)\n"
        }
        Lang::Rust => {
            "use muninn_plugin::prelude::*;\n\npub fn run(host: &Host) -> Result<View> {\n    let bars = host.input()?;\n    let close = bars.f64(\"close\")?;\n    let mean = close.iter().sum::<f64>() / close.len().max(1) as f64;\n    Ok(View::table(\"Summary\", &[\"bars\", \"mean close\"]).row(vec![(bars.len() as f64).into(), mean.into()]))\n}\n"
        }
        Lang::Python => {
            "# Typed functions: each public def is a column, run once per record.\n# @title Range\ndef spread(high: float, low: float) -> float:\n    return high - low\n"
        }
        Lang::Plugin => "/path/to/libplugin.so\n",
    }
}

/// Status line of the toolchains (for `mnnn code check` and the TUI).
pub fn toolchain_status() -> Vec<(String, std::result::Result<String, String>)> {
    let cargo = cargo().map(|c| {
        std::process::Command::new(&c).arg("--version").output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default()
    });
    vec![("cargo".into(), cargo.map_err(|e| e.to_string())), ("python".into(), Ok("built in (ruff_python_parser 0.0.20, python-ast 1.1.0)".into()))]
}

/// The last run as plain JSON (for `--json`).
pub fn run_json(info: &RunInfo) -> Value {
    serde_json::to_value(info).unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests;
