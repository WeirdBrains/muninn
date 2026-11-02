//! Template build steps (2.17): a per-template shell command, run in a scratch copy of the
//! template's source folder, whose output folder becomes the template's files.
//!
//! A build only runs on a node whose owner approved that exact command (with its source and
//! output folders) for that template; a changed command asks again. Approvals are node-local
//! and never sync, so a build step another node set never runs here without consent here.
use super::{Actor, record, require};
use crate::error::{Error, Result};
use crate::store::{Org, Store, Template};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// The default output folder.
pub const DEFAULT_OUT: &str = "build";
/// The most a build may produce.
pub const MAX_OUTPUT: u64 = 64 * 1024 * 1024;

/// What a build produced.
#[derive(Debug, Clone, PartialEq)]
pub struct Built {
    pub html: String,
    pub css: String,
    pub assets: Vec<(String, String, Vec<u8>)>,
    /// The command's output (stdout then stderr), trimmed to its last lines.
    pub log: String,
    /// Things worth telling (e.g. index.html has no `{{content}}`).
    pub notes: Vec<String>,
}

/// The fingerprint approvals are kept under: the command, source and output folders.
pub fn cmd_hash(t: &Template) -> Option<String> {
    let cmd = t.build_cmd.as_deref()?;
    let mut h = Sha256::new();
    for part in [cmd, t.build_src.as_deref().unwrap_or(""), out_of(t)] {
        h.update(part.as_bytes());
        h.update([0]);
    }
    Some(hex::encode(h.finalize()))
}

fn out_of(t: &Template) -> &str {
    t.build_out.as_deref().filter(|o| !o.trim().is_empty()).unwrap_or(DEFAULT_OUT)
}

/// Check a build configuration before it is saved.
pub fn check(cmd: &str, src: &str, out: Option<&str>) -> Result<()> {
    if cmd.trim().is_empty() {
        return Err(Error::invalid("build", "the command is empty"));
    }
    if !Path::new(src).is_absolute() {
        return Err(Error::invalid("build", "the source folder must be an absolute path"));
    }
    if let Some(o) = out
        && (Path::new(o).is_absolute() || o.split(['/', '\\']).any(|p| p == ".."))
    {
        return Err(Error::invalid("build", "the output folder is a relative path inside the source folder"));
    }
    Ok(())
}

/// A build step: the command, its absolute source folder and its output folder.
#[derive(Debug, Clone, Copy)]
pub struct Step<'a> {
    pub cmd: &'a str,
    pub src: &'a str,
    pub out: Option<&'a str>,
}

/// Set or (with `None`) clear a template's build step (needs `publish`).
pub fn configure(s: &Store, origin: &str, a: &Actor, org: &Org, t: &Template, step: Option<Step>) -> Result<Template> {
    require(s, org, a, "publish")?;
    let cmd = step.map(|st| st.cmd);
    match step {
        Some(st) => {
            check(st.cmd, st.src, st.out)?;
            s.set_template_build(&t.id, Some(st.cmd.trim()), Some(st.src), st.out.map(str::trim).filter(|o| !o.is_empty()))?;
        }
        None => s.set_template_build(&t.id, None, None, None)?,
    }
    record(
        s,
        origin,
        "org.template.build_set",
        format!("build step of template {} in {} {}", t.name, org.name, if cmd.is_some() { "set" } else { "cleared" }),
        serde_json::json!({ "org": org.id, "template": t.id }),
    );
    s.template(&t.id)?.ok_or_else(|| Error::Other("template vanished".into()))
}

/// Is this template's current build command approved on this node?
pub fn approved(s: &Store, t: &Template) -> Result<bool> {
    match cmd_hash(t) {
        Some(h) => s.build_approved(&t.id, &h),
        None => Ok(false),
    }
}

/// The question to ask before running a build that is not approved yet.
pub fn question(t: &Template) -> String {
    format!(
        "run `{}` in a copy of {} and use its {} folder as template {}?",
        t.build_cmd.as_deref().unwrap_or(""),
        t.build_src.as_deref().unwrap_or("?"),
        out_of(t),
        t.name
    )
}

/// Run a template's build step and make its output the template's files (needs `publish`).
/// Without an approval for this exact command, `approve` must be true (the caller asked).
pub fn build(s: &Store, origin: &str, a: &Actor, org: &Org, t: &Template, approve: bool) -> Result<Built> {
    require(s, org, a, "publish")?;
    let hash = cmd_hash(t).ok_or_else(|| Error::invalid("build", format!("template {} has no build step (set one with --cmd)", t.name)))?;
    if !s.build_approved(&t.id, &hash)? {
        if !approve {
            return Err(Error::NeedsConfirmation(question(t)));
        }
        s.approve_build(&t.id, &hash)?;
    }
    let built = run(t)?;
    s.replace_template_files(&t.id, &built.html, &built.css, &built.assets)?;
    record(
        s,
        origin,
        "org.template.build",
        format!("built template {} in {} ({} asset(s))", t.name, org.name, built.assets.len()),
        serde_json::json!({ "org": org.id, "template": t.id, "assets": built.assets.len() }),
    );
    Ok(built)
}

/// Run the build in a scratch copy and read its output (no store changes).
pub fn run(t: &Template) -> Result<Built> {
    let cmd = t.build_cmd.as_deref().ok_or_else(|| Error::invalid("build", "no build command"))?;
    let src = PathBuf::from(t.build_src.as_deref().ok_or_else(|| Error::invalid("build", "no source folder"))?);
    if !src.is_dir() {
        return Err(Error::invalid("build", format!("{} is not a folder on this node", src.display())));
    }
    let scratch = tempfile::Builder::new().prefix("mnnn-build-").tempdir()?;
    copy_dir(&src, scratch.path())?;
    let output = shell(cmd).current_dir(scratch.path()).output()?;
    let mut log = String::from_utf8_lossy(&output.stdout).into_owned();
    log.push_str(&String::from_utf8_lossy(&output.stderr));
    let log = tail(&log, 40);
    if !output.status.success() {
        return Err(Error::Other(format!("the build failed ({}):\n{log}", output.status)));
    }
    let out = scratch.path().join(out_of(t));
    let index = out.join("index.html");
    if !index.is_file() {
        return Err(Error::Other(format!("the build made no {}/index.html", out_of(t))));
    }
    let html = std::fs::read_to_string(&index)?;
    let css = std::fs::read_to_string(out.join("style.css")).unwrap_or_default();
    let mut files = vec![];
    collect(&out, &out, &mut files)?;
    let mut total = 0u64;
    let mut assets = vec![];
    for (name, path) in files {
        if name == "index.html" || name == "style.css" {
            continue;
        }
        let data = std::fs::read(&path)?;
        total += data.len() as u64;
        if total > MAX_OUTPUT {
            return Err(Error::Other(format!("the build output is over {} MB", MAX_OUTPUT / 1024 / 1024)));
        }
        assets.push((name.clone(), super::mime_of(&name).to_string(), data));
    }
    let mut notes = vec![];
    if !html.contains("{{content}}") {
        notes.push("index.html has no {{content}}: pages will show only the template".into());
    }
    Ok(Built { html, css, assets, log, notes })
}

#[cfg(windows)]
fn shell(cmd: &str) -> std::process::Command {
    let mut c = std::process::Command::new("cmd");
    c.args(["/C", cmd]);
    c
}

#[cfg(not(windows))]
fn shell(cmd: &str) -> std::process::Command {
    let mut c = std::process::Command::new("sh");
    c.args(["-c", cmd]);
    c
}

fn tail(s: &str, lines: usize) -> String {
    let v: Vec<&str> = s.lines().collect();
    v[v.len().saturating_sub(lines)..].join("\n")
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let name = e.file_name();
        if name == ".git" {
            continue;
        }
        let ty = e.file_type()?;
        if ty.is_dir() {
            copy_dir(&e.path(), &to.join(&name))?;
        } else if ty.is_file() {
            std::fs::copy(e.path(), to.join(&name))?;
        }
    }
    Ok(())
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) -> Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<std::io::Result<_>>()?;
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let ty = e.file_type()?;
        if ty.is_dir() {
            collect(root, &e.path(), out)?;
        } else if ty.is_file() {
            let rel = e.path().strip_prefix(root).map_err(|e| Error::Other(e.to_string()))?.to_string_lossy().replace('\\', "/");
            out.push((rel, e.path()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Store, Org, Actor) {
        let s = Store::open_in_memory().unwrap();
        let o = super::super::create(&s, "test", "Lab", None, "").unwrap();
        (s, o, Actor::Owner)
    }

    #[test]
    fn check_rejects_bad_configs() {
        assert!(check("", "/src", None).is_err());
        assert!(check("make", "rel", None).is_err());
        assert!(check("make", "/src", Some("../out")).is_err());
        assert!(check("make", "/src", Some("/abs")).is_err());
        assert!(check("make", "/src", Some("dist")).is_ok());
    }

    #[test]
    fn hash_follows_command_source_and_output() {
        let (s, o, a) = setup();
        let (t, _) = s.save_template(&o.id, "site", "", "").unwrap();
        assert_eq!(cmd_hash(&t), None);
        let t1 = configure(&s, "test", &a, &o, &t, Some(Step { cmd: "make", src: "/src", out: None })).unwrap();
        let t2 = configure(&s, "test", &a, &o, &t, Some(Step { cmd: "make all", src: "/src", out: None })).unwrap();
        let t3 = configure(&s, "test", &a, &o, &t, Some(Step { cmd: "make", src: "/src", out: Some("dist") })).unwrap();
        assert_ne!(cmd_hash(&t1), cmd_hash(&t2));
        assert_ne!(cmd_hash(&t1), cmd_hash(&t3));
        let cleared = configure(&s, "test", &a, &o, &t, None).unwrap();
        assert_eq!(cleared.build_cmd, None);
    }

    #[cfg(unix)]
    #[test]
    fn build_asks_first_then_replaces_files() {
        let (s, o, a) = setup();
        let src = tempfile::tempdir().unwrap();
        std::fs::write(src.path().join("page.html"), "<main>{{content}}</main>").unwrap();
        let (t, _) = s.save_template(&o.id, "site", "old", "").unwrap();
        s.put_template_asset(&t.id, "stale.png", "image/png", &[1]).unwrap();
        let cmd = "mkdir -p out/img && cp page.html out/index.html && printf 'b{}' > out/style.css && printf x > out/img/a.txt && echo built";
        let t = configure(&s, "test", &a, &o, &t, Some(Step { cmd, src: src.path().to_str().unwrap(), out: Some("out") })).unwrap();
        assert!(matches!(build(&s, "test", &a, &o, &t, false), Err(Error::NeedsConfirmation(_))));
        let b = build(&s, "test", &a, &o, &t, true).unwrap();
        assert!(b.log.contains("built") && b.notes.is_empty());
        let t2 = s.template(&t.id).unwrap().unwrap();
        assert_eq!((t2.html.as_str(), t2.css.as_str()), ("<main>{{content}}</main>", "b{}"));
        let names: Vec<_> = s.template_assets(&t.id).unwrap().into_iter().map(|x| x.name).collect();
        assert_eq!(names, vec!["img/a.txt"]);
        // approved now: runs without asking; the source folder is untouched
        build(&s, "test", &a, &o, &t2, false).unwrap();
        assert!(!src.path().join("out").exists());
        // a changed command asks again
        let t3 = configure(&s, "test", &a, &o, &t2, Some(Step { cmd: "exit 3", src: src.path().to_str().unwrap(), out: None })).unwrap();
        assert!(matches!(build(&s, "test", &a, &o, &t3, false), Err(Error::NeedsConfirmation(_))));
        assert!(build(&s, "test", &a, &o, &t3, true).unwrap_err().to_string().contains("failed"));
    }

    #[cfg(unix)]
    #[test]
    fn build_needs_an_index() {
        let (s, o, a) = setup();
        let src = tempfile::tempdir().unwrap();
        let (t, _) = s.save_template(&o.id, "site", "", "").unwrap();
        let t = configure(&s, "test", &a, &o, &t, Some(Step { cmd: "mkdir build", src: src.path().to_str().unwrap(), out: None })).unwrap();
        assert!(build(&s, "test", &a, &o, &t, true).unwrap_err().to_string().contains("index.html"));
    }
}
