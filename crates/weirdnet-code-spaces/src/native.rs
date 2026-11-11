//! Running cargo and loading libraries (the `native-plugins` feature; desktop only).
use crate::error::Error;
use crate::{Built, Env, Lang, Profile, Result, abi, build_key, cargo, generate, plugin_file, trust_hash_of, write_sdk};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

fn copy_in(from: &Path, to: &Path) -> Result<()> {
    let dir = to.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".{}.tmp{}", to.file_name().and_then(|n| n.to_str()).unwrap_or("plugin"), std::process::id()));
    std::fs::copy(from, &tmp)?;
    std::fs::rename(&tmp, to)?;
    Ok(())
}

/// Build (or find in the cache) the plugin of a source. Cached libraries are content
/// addressed and replaced by rename, never rewritten, so a loaded one stays valid. A
/// `plugin` source is copied in under its trust hash. `log` gets cargo's output lines.
pub fn build(p: &Profile, env: &Env, lang: Lang, source: &str, log: &mut dyn FnMut(&str)) -> Result<Built> {
    if lang == Lang::Plugin {
        let f = plugin_file(source);
        let key = trust_hash_of(p, lang, source)?;
        let out = env.plugin_path(&key);
        let cached = out.exists();
        if !cached {
            copy_in(&f, &out)?;
        }
        return Ok(Built { path: out, key, cached });
    }
    let key = build_key(p, lang, source);
    let out = env.plugin_path(&key);
    if out.exists() {
        return Ok(Built { path: out, key, cached: true });
    }
    let cargo = cargo(p)?;
    write_sdk(p, &env.sdk_dir(p))?;
    let dir = env.build_dir(&key);
    generate(p, lang, source, &dir)?;
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
        return Err(Error::Build(format!("build failed (lines refer to the generated crate in {}):\n{}", dir.display(), lines[tail..].join("\n"))));
    }
    let lib = env.target.join("release").join(format!("{}{}{key}{}", std::env::consts::DLL_PREFIX, p.crate_prefix, std::env::consts::DLL_SUFFIX));
    copy_in(&lib, &out)?;
    log(&format!("built {}", out.display()));
    Ok(Built { path: out, key, cached: false })
}

/// A loaded plugin: its run function (call it with a host table) and its info JSON.
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

/// Loaded libraries by path, symbol name and ABI version (a profile with other symbols or
/// another version checks the library again).
type Libraries = Mutex<HashMap<(PathBuf, &'static str, u32), Loaded>>;
static LOADED: OnceLock<Libraries> = OnceLock::new();

/// Load a plugin (once per process and profile; libraries stay loaded) after checking it
/// exports the profile's symbols and ABI version.
///
/// Loading runs the library's initializers with the process's rights: load only what the
/// owner approved (see [`crate::approved`]), from the cache [`build`] fills.
pub fn load(p: &Profile, path: &Path) -> Result<Loaded> {
    let map = LOADED.get_or_init(Default::default);
    let mut map = map.lock().unwrap_or_else(|e| e.into_inner());
    let id = (path.to_path_buf(), p.symbols[0], p.abi_version);
    if let Some(l) = map.get(&id) {
        return Ok(*l);
    }
    let bad = |m: String| Error::invalid("plugin", format!("{}: {m}", path.display()));
    let [sym_abi, sym_info, sym_run] = p.symbols;
    // SAFETY: loading runs the library's initializers; the owner approved this code. The
    // library is never unloaded, so its symbols stay valid.
    let lib: &'static libloading::Library = Box::leak(Box::new(unsafe { libloading::Library::new(path) }.map_err(|e| bad(e.to_string()))?));
    // SAFETY: the symbols are looked up with the ABI's signatures; the version is checked
    // before info and run are used.
    let loaded = unsafe {
        let abi_fn = lib.get::<abi::PluginAbi>(sym_abi.as_bytes()).map_err(|_| bad(format!("not a {} plugin (no {sym_abi})", p.app)))?;
        let v = abi_fn();
        if v != p.abi_version {
            return Err(bad(format!(
                "built for plugin ABI {v}; this {} speaks ABI {} — rebuild it against {} {}",
                p.app, p.abi_version, p.sdk_package, p.sdk_version
            )));
        }
        let info_fn = lib.get::<abi::PluginInfo>(sym_info.as_bytes()).map_err(|_| bad(format!("no {sym_info}")))?;
        let info = std::str::from_utf8(info_fn().as_bytes()).map_err(|_| bad("info is not UTF-8".into()))?;
        let run = *lib.get::<abi::PluginRun>(sym_run.as_bytes()).map_err(|_| bad(format!("no {sym_run}")))?;
        Loaded { run, info }
    };
    map.insert(id, loaded);
    Ok(loaded)
}

/// Run a loaded plugin with `host` (a host table of the profile's ABI version). Returns the
/// plugin's code ([`abi::RUN_OK`], `RUN_ERROR`, `RUN_PANIC`, or what else it returned).
///
/// The plugin runs as native code with the process's rights: run only what the owner
/// approved ([`crate::approved`]); [`load`] has checked its ABI.
pub fn run(p: &Profile, plugin: &Loaded, host: &mut dyn crate::Host) -> i32 {
    // SAFETY: `plugin` was loaded with its ABI checked against this profile.
    unsafe { crate::host::run_with(p.abi_version, plugin.run, host) }
}
