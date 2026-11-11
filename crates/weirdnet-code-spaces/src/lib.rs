//! # weirdnet-code-spaces
//!
//! Code spaces: small programs (math formulas, Polars expressions, Rust, typed Python, or a
//! library built elsewhere) compiled into plugins against the `weirdnet-plugin` SDK, cached
//! by content, loaded after an ABI check and approved by a trust hash.
//!
//! What stays with the application: where code spaces are stored, who may approve them,
//! the host methods a plugin calls (its data) and running a loaded plugin with a host
//! table. Everything here is parameterized by a [`Profile`] (the SDK, symbol names, hash
//! domains and the names messages use); [`WEIRDNET`] is the default.
//!
//! ```ignore
//! use weirdnet_code_spaces::{self as cs, Env, Lang, WEIRDNET};
//! # fn main() -> cs::Result<()> {
//! let env = Env::new("/tmp/app/code", "/tmp/app/code/target");
//! let src = "fast = sma(close, 10)";
//! let built = cs::build(&WEIRDNET, &env, Lang::Math, src, &mut |line| eprintln!("{line}"))?;
//! let plugin = cs::load(&WEIRDNET, &built.path)?; // then call plugin.run with a host table
//! # Ok(()) }
//! ```
//!
//! Features: `math` (default), `polars`, `python` (each adds its language; Python needs no
//! libpython), and `native-plugins` (default: running cargo and loading libraries, so
//! desktop only).
#![cfg_attr(docsrs, feature(doc_cfg))]

/// The plugin C ABI (version 2 for [`WEIRDNET`]): the SDK's own `abi.rs`. Version 1 has the
/// same layout, so a host table built from these types serves either.
#[path = "../sdk/src/abi.rs"]
pub mod abi;
pub mod error;
pub mod host;
pub mod lang;
#[cfg(feature = "math")]
pub mod math;
#[cfg(feature = "polars")]
pub mod polars;
#[cfg(feature = "python")]
pub mod pyconv;
#[cfg(feature = "python")]
pub mod python;
#[cfg(feature = "python")]
#[allow(clippy::all, unused, unexpected_cfgs, deprecated)]
mod python_ast;
pub mod spaces;

pub use error::{Result, SpacesError};
pub use host::Host;
pub use lang::Lang;

use error::Error;
use std::path::{Path, PathBuf};

/// Everything that names the application or its SDK. Hashes, generated code, symbol names
/// and messages all come from here, so an application keeps its formats by passing its own.
#[derive(Debug, Clone, Copy)]
pub struct Profile {
    /// The application, in messages and generated-code comments ("not a Weirdnet plugin").
    pub app: &'static str,
    /// The SDK's package and crate names (`weirdnet-plugin`, `weirdnet_plugin`).
    pub sdk_package: &'static str,
    pub sdk_crate: &'static str,
    pub sdk_version: &'static str,
    /// The SDK crate's files (path, contents), written next to builds.
    pub sdk_files: &'static [(&'static str, &'static str)],
    /// The ABI version plugins must report.
    pub abi_version: u32,
    /// The three exported symbols (abi, info, run).
    pub symbols: [&'static str; 3],
    /// Domain separators of the trust and line-up hashes (end them with a NUL).
    pub trust_domain: &'static [u8],
    pub lineup_domain: &'static [u8],
    /// Generated crates are named `{crate_prefix}{build key}`.
    pub crate_prefix: &'static str,
    /// Bumped when generated code changes (invalidates cached builds).
    pub codegen: u32,
    /// The variable that names cargo (else `cargo` on the PATH).
    pub cargo_env: &'static str,
    /// After "… compile to plugins with cargo (X not found). " when there is none.
    pub cargo_hint: &'static str,
    /// After "unknown function `f`" in math sources.
    pub math_help: &'static str,
}

/// The `weirdnet-plugin` SDK this crate carries (`sdk/`, the same files as the
/// `weirdnet-plugin` crate).
pub const WEIRDNET_SDK_FILES: &[(&str, &str)] = &[
    ("Cargo.toml", include_str!("../sdk/Cargo.toml.in")),
    ("src/lib.rs", include_str!("../sdk/src/lib.rs")),
    ("src/abi.rs", include_str!("../sdk/src/abi.rs")),
    ("src/json.rs", include_str!("../sdk/src/json.rs")),
    ("src/series.rs", include_str!("../sdk/src/series.rs")),
    ("src/columns.rs", include_str!("../sdk/src/columns.rs")),
    ("src/connector.rs", include_str!("../sdk/src/connector.rs")),
    ("README.md", include_str!("../sdk/README.md")),
    ("LICENSE", include_str!("../sdk/LICENSE")),
];

/// The default profile: the `weirdnet-plugin` SDK, ABI 2.
pub const WEIRDNET: Profile = Profile {
    app: "Weirdnet",
    sdk_package: "weirdnet-plugin",
    sdk_crate: "weirdnet_plugin",
    sdk_version: "0.1.0",
    sdk_files: WEIRDNET_SDK_FILES,
    abi_version: abi::ABI_VERSION,
    symbols: ["weirdnet_plugin_abi", "weirdnet_plugin_info", "weirdnet_plugin_run"],
    trust_domain: b"weirdnet-code-trust\0",
    lineup_domain: b"weirdnet-space-lineup\0",
    crate_prefix: "wn_code_",
    codegen: 1,
    cargo_env: "WEIRDNET_CARGO",
    cargo_hint: "Install Rust from https://rustup.rs, or set WEIRDNET_CARGO to cargo; prebuilt plugins run without it.",
    math_help: "",
};

fn hex16(h: blake3::Hash) -> String {
    h.to_hex()[..32].to_string()
}

/// Hash of the profile's SDK files.
pub fn sdk_hash(p: &Profile) -> String {
    let mut h = blake3::Hasher::new();
    for (n, c) in p.sdk_files {
        h.update(n.as_bytes());
        h.update(&[0]);
        h.update(c.as_bytes());
        h.update(&[0]);
    }
    hex16(h.finalize())
}

/// Write the profile's SDK crate into `dir` (files already there and equal are left alone).
pub fn write_sdk(p: &Profile, dir: &Path) -> Result<()> {
    for (n, c) in p.sdk_files {
        let f = dir.join(n);
        if let Some(parent) = f.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if std::fs::read_to_string(&f).ok().as_deref() != Some(*c) {
            std::fs::write(&f, c)?;
        }
    }
    Ok(())
}

/// Where code spaces are built and cached. The application chooses both directories.
#[derive(Debug, Clone, PartialEq)]
pub struct Env {
    /// The code directory: `sdk/`, `build/` and `plugins/` go under it.
    pub root: PathBuf,
    /// Cargo's target directory (shared by every build).
    pub target: PathBuf,
}

impl Env {
    pub fn new(root: impl Into<PathBuf>, target: impl Into<PathBuf>) -> Self {
        Env { root: root.into(), target: target.into() }
    }
    pub fn sdk_dir(&self, p: &Profile) -> PathBuf {
        self.root.join("sdk").join(sdk_dir_name(p))
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

fn sdk_dir_name(p: &Profile) -> String {
    format!("{}-{}", p.sdk_package, &sdk_hash(p)[..12])
}

/// The library a `plugin` source names (its path; `~/` is the home directory).
pub fn plugin_file(source: &str) -> PathBuf {
    let s = source.trim();
    match s.strip_prefix("~/") {
        Some(rest) => dirs::home_dir().map(|h| h.join(rest)).unwrap_or_else(|| PathBuf::from(s)),
        None => PathBuf::from(s),
    }
}

/// What an owner approves: the language and source (or the library's bytes), under the
/// profile's trust domain.
pub fn trust_hash_of(p: &Profile, lang: Lang, source: &str) -> Result<String> {
    let mut h = blake3::Hasher::new();
    h.update(p.trust_domain);
    h.update(lang.as_str().as_bytes());
    h.update(&[0]);
    if lang == Lang::Plugin {
        let f = plugin_file(source);
        let bytes = std::fs::read(&f).map_err(|e| Error::invalid("plugin", format!("{}: {e}", f.display())))?;
        h.update(&bytes);
    } else {
        h.update(source.as_bytes());
    }
    Ok(hex16(h.finalize()))
}

/// Approved as it is: `approved_hash` is the trust hash of this language and source (a
/// library that can't be read is never approved).
pub fn approved(p: &Profile, lang: Lang, source: &str, approved_hash: Option<&str>) -> bool {
    trust_hash_of(p, lang, source).ok().is_some_and(|h| approved_hash == Some(h.as_str()))
}

/// The extra dependency line a language's generated crate needs.
pub fn dependency(lang: Lang) -> &'static str {
    match lang {
        #[cfg(feature = "polars")]
        Lang::Polars => polars::DEPENDENCY,
        #[cfg(feature = "python")]
        Lang::Python => python::DEPENDENCY,
        _ => "",
    }
}

/// The cache key of a build: ABI, SDK, codegen, language, platform, dependencies, source.
pub fn build_key(p: &Profile, lang: Lang, source: &str) -> String {
    let mut h = blake3::Hasher::new();
    h.update(
        format!(
            "abi{}|sdk{}|codegen{}|{}|{}-{}\0",
            p.abi_version,
            sdk_hash(p),
            p.codegen,
            lang.as_str(),
            std::env::consts::ARCH,
            std::env::consts::OS
        )
        .as_bytes(),
    );
    h.update(dependency(lang).as_bytes());
    h.update(&[0]);
    h.update(source.as_bytes());
    hex16(h.finalize())
}

fn unsupported(lang: Lang) -> Error {
    Error::invalid("code", format!("{} code spaces need this crate's `{}` feature", lang.as_str(), lang.as_str()))
}

/// The cargo to build with: the profile's variable or `cargo` on the PATH, checked by
/// running it.
pub fn cargo(p: &Profile) -> Result<PathBuf> {
    let c = std::env::var_os(p.cargo_env).filter(|c| !c.is_empty()).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("cargo"));
    match std::process::Command::new(&c).arg("--version").output() {
        Ok(o) if o.status.success() => Ok(c),
        _ => Err(Error::missing(
            "Rust toolchain",
            format!("code spaces in math, polars, rust and python compile to plugins with cargo ({} not found). {}", c.display(), p.cargo_hint),
        )),
    }
}

/// Check a source without building: the language's own checks (a prebuilt plugin is
/// copied into the cache and loaded, never loaded in place).
pub fn check(p: &Profile, env: &Env, lang: Lang, source: &str) -> Result<Vec<String>> {
    let _ = env;
    Ok(match lang {
        #[cfg(feature = "math")]
        Lang::Math => {
            let m = math::parse_for(p, source)?;
            let fields: Vec<String> = m.fields().into_iter().collect();
            vec![format!("{} formula(s); input fields used: {}", m.lines.len(), if fields.is_empty() { "none".into() } else { fields.join(", ") })]
        }
        #[cfg(feature = "polars")]
        Lang::Polars => {
            let m = polars::parse(source)?;
            vec![format!("{} step(s); expressions are checked by the compiler at build time", m.steps.len())]
        }
        #[cfg(feature = "python")]
        Lang::Python => {
            let m = python::parse(source)?;
            let public: Vec<&str> = m.funcs.iter().filter(|f| !f.name.starts_with('_')).map(|f| f.name.as_str()).collect();
            pyconv::to_rust(source, "user.py").map_err(|e| Error::invalid("python", e))?;
            vec![format!("columns: {}", public.join(", "))]
        }
        Lang::Rust => {
            if !source.contains("fn run") {
                return Err(Error::invalid(
                    "rust",
                    format!("define `pub fn run(host: &Host) -> Result<View>` (with `use {}::prelude::*;`)", p.sdk_crate),
                ));
            }
            vec!["defines run; the compiler checks the rest at build time".into()]
        }
        #[cfg(feature = "native-plugins")]
        Lang::Plugin => {
            let f = plugin_file(source);
            let l = load(p, &build(p, env, lang, source, &mut |_| {})?.path)?;
            vec![format!("{}: ABI {} ({})", f.display(), p.abi_version, l.info)]
        }
        #[allow(unreachable_patterns)]
        other => return Err(unsupported(other)),
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

/// The `Cargo.toml` of a generated crate.
pub fn manifest(p: &Profile, key: &str, sdk: &Path, extra: &str) -> String {
    format!(
        "[package]\nname = \"{}{key}\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n\n[lib]\ncrate-type = [\"cdylib\"]\npath = \"src/lib.rs\"\n\n[dependencies]\n{} = {{ path = {:?} }}\n{extra}\n\n[profile.release]\nopt-level = 3\ndebug = false\n\n[workspace]\n",
        p.crate_prefix,
        p.sdk_package,
        sdk.display().to_string()
    )
}

/// The generated crate's `user.rs` (and `py.rs` for Python) for a source.
pub fn generate_user(p: &Profile, lang: Lang, source: &str) -> Result<(String, Option<String>, &'static str)> {
    let _ = p;
    Ok(match lang {
        #[cfg(feature = "math")]
        Lang::Math => (math::generate(p, source)?, None, ""),
        #[cfg(feature = "polars")]
        Lang::Polars => (polars::generate(p, source)?, None, polars::DEPENDENCY),
        Lang::Rust => {
            if !source.contains("fn run") {
                return Err(Error::invalid("rust", "define `pub fn run(host: &Host) -> Result<View>`"));
            }
            (source.to_string(), None, "")
        }
        #[cfg(feature = "python")]
        Lang::Python => {
            let user = python::generate(p, source)?;
            let py = python::transpile(source, Path::new("."))?;
            (user, Some(py), python::DEPENDENCY)
        }
        Lang::Plugin => return Err(Error::invalid("plugin", "prebuilt plugins are not compiled")),
        #[allow(unreachable_patterns)]
        other => return Err(unsupported(other)),
    })
}

/// The generated crate's `lib.rs`.
pub fn lib_rs(p: &Profile, lang: Lang) -> String {
    format!("#![allow(unused, clippy::all)]\nmod user;\n{}::plugin!(user::run, {:?});\n", p.sdk_crate, lang.as_str())
}

/// Write the crate of a source into `dir` (a build directory under the code root).
pub fn generate(p: &Profile, lang: Lang, source: &str, dir: &Path) -> Result<()> {
    let src = dir.join("src");
    std::fs::create_dir_all(&src)?;
    let (user, py, extra) = generate_user(p, lang, source)?;
    if let Some(py) = py {
        std::fs::write(src.join("py.rs"), py)?;
    }
    std::fs::write(src.join("user.rs"), user)?;
    std::fs::write(src.join("lib.rs"), lib_rs(p, lang))?;
    let key = dir.file_name().and_then(|n| n.to_str()).unwrap_or("x");
    std::fs::write(dir.join("Cargo.toml"), manifest(p, key, &dir.join("../../sdk").join(sdk_dir_name(p)), extra))?;
    Ok(())
}

#[cfg(feature = "native-plugins")]
mod native;
#[cfg(feature = "native-plugins")]
pub use native::{Loaded, build, load, run};

#[cfg(test)]
mod tests;
