//! Backup and restore: database (still encrypted), keys/, preferences.toml, themes/.
use crate::crypto::{self, Key32};
use crate::error::{Error, Result};
use crate::interact::Interact;
use crate::paths::{Paths, ensure_dir};
use crate::store::{Store, now_ms};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    pub format: u32,
    pub muninn_version: String,
    pub created_at: i64,
    pub service_key_version: Option<u32>,
    /// relative path -> BLAKE3 hex
    pub files: BTreeMap<String, String>,
}

fn copy_tree(src: &Path, dst: &Path) -> Result<()> {
    if !src.exists() {
        return Ok(());
    }
    ensure_dir(dst)?;
    for e in fs::read_dir(src)? {
        let e = e?;
        let p = e.path();
        let t = dst.join(e.file_name());
        if p.is_dir() {
            copy_tree(&p, &t)?;
        } else if !p.extension().is_some_and(|x| x == "tmp-muninn") {
            fs::copy(&p, &t)?;
        }
    }
    Ok(())
}

fn hash_tree(root: &Path, rel: &Path, out: &mut BTreeMap<String, String>) -> Result<()> {
    let dir = root.join(rel);
    if !dir.exists() {
        return Ok(());
    }
    let mut entries: Vec<_> = fs::read_dir(&dir)?.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let r = rel.join(e.file_name());
        if e.path().is_dir() {
            hash_tree(root, &r, out)?;
        } else if r != Path::new("manifest.json") {
            out.insert(r.to_string_lossy().replace('\\', "/"), crate::ingest::hash_file(&e.path())?);
        }
    }
    Ok(())
}

/// Create `muninn-backup-<timestamp>.tar.zst[.age]` in `out_dir`.
pub fn create(paths: &Paths, store: &Store, dek: &Key32, out_dir: &Path, encrypt: Option<&SecretString>) -> Result<PathBuf> {
    ensure_dir(out_dir)?;
    let stage = tempfile::Builder::new().prefix("muninn-backup").tempdir()?;
    let s = stage.path();
    ensure_dir(&s.join("storage"))?;
    store.backup_to(&s.join("storage").join(crate::paths::DB_FILE), Some(&dek.0))?;
    copy_tree(&paths.keys_dir(), &s.join("keys"))?;
    copy_tree(&paths.themes_dir(), &s.join("themes"))?;
    if paths.preferences().exists() {
        fs::copy(paths.preferences(), s.join("preferences.toml"))?;
    }
    let mut files = BTreeMap::new();
    hash_tree(s, Path::new(""), &mut files)?;
    let m = Manifest {
        format: 1,
        muninn_version: env!("CARGO_PKG_VERSION").into(),
        created_at: now_ms(),
        service_key_version: crypto::keyring::Envelope::load(paths)?.map(|e| e.service_version),
        files,
    };
    fs::write(s.join("manifest.json"), serde_json::to_vec_pretty(&m)?)?;

    let mut tar_bytes = Vec::new();
    {
        let enc = zstd::Encoder::new(&mut tar_bytes, 3)?;
        let mut b = tar::Builder::new(enc);
        b.append_dir_all(".", s)?;
        b.into_inner()?.finish()?;
    }
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%3fZ");
    let (name, bytes) = match encrypt {
        Some(pw) => (format!("muninn-backup-{stamp}.tar.zst.age"), crypto::age_encrypt(pw, &tar_bytes)?),
        None => (format!("muninn-backup-{stamp}.tar.zst"), tar_bytes),
    };
    let dest = out_dir.join(name);
    crate::paths::write_private(&dest, &bytes)?;
    Ok(dest)
}

/// Pick a backup file: the path itself, or the newest `muninn-backup-*` in a directory.
pub fn locate(from: &Path) -> Result<PathBuf> {
    if from.is_file() {
        return Ok(from.to_path_buf());
    }
    if from.is_dir() {
        let mut v: Vec<PathBuf> = fs::read_dir(from)?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("muninn-backup-")))
            .collect();
        v.sort();
        return v.pop().ok_or_else(|| Error::invalid("--from", format!("no muninn-backup-* files in {}", from.display())));
    }
    Err(Error::invalid("--from", format!("{} does not exist", from.display())))
}

#[derive(Debug, Default)]
pub struct RestoreReport {
    pub archive: PathBuf,
    pub files: usize,
    pub themes_restored: Vec<String>,
    pub themes_skipped: Vec<String>,
}

/// Restore an archive into `paths.home`, prompting before overwriting.
pub fn restore(paths: &Paths, from: &Path, io: &Interact, password: impl FnOnce() -> Result<SecretString>) -> Result<RestoreReport> {
    let archive = locate(from)?;
    let raw = fs::read(&archive)?;
    let tar_bytes = if crypto::is_age(&raw) { crypto::age_decrypt(&password()?, &raw)?.to_vec() } else { raw };
    let stage = tempfile::Builder::new().prefix("muninn-restore").tempdir()?;
    let s = stage.path();
    tar::Archive::new(zstd::Decoder::new(&tar_bytes[..])?).unpack(s)?;
    let m: Manifest = serde_json::from_slice(&fs::read(s.join("manifest.json")).map_err(|_| Error::invalid("backup", "manifest.json missing"))?)?;
    let mut actual = BTreeMap::new();
    hash_tree(s, Path::new(""), &mut actual)?;
    if actual != m.files {
        return Err(Error::invalid("backup", "file hashes do not match the manifest (corrupted archive)"));
    }
    paths.ensure()?;
    let mut rep = RestoreReport { archive: archive.clone(), files: m.files.len(), ..Default::default() };

    let src_db = s.join("storage").join(crate::paths::DB_FILE);
    if src_db.exists() {
        if (paths.db.exists() || paths.envelope().exists()) && !io.confirm("overwrite the existing database and keys with the backup?")? {
            return Err(Error::Aborted);
        }
        for ext in ["database-wal", "database-shm"] {
            let _ = fs::remove_file(paths.db.with_extension(ext));
        }
        fs::copy(&src_db, &paths.db)?;
        crate::paths::set_mode(&paths.db, 0o600)?;
        if s.join("keys").exists() {
            if paths.keys_dir().exists() {
                fs::remove_dir_all(paths.keys_dir())?;
            }
            copy_tree(&s.join("keys"), &paths.keys_dir())?;
        }
    }
    if s.join("preferences.toml").exists() {
        fs::copy(s.join("preferences.toml"), paths.preferences())?;
        crate::paths::set_mode(&paths.preferences(), 0o600)?;
    }
    if let Ok(rd) = fs::read_dir(s.join("themes")) {
        let mut names: Vec<_> = rd.flatten().filter(|e| e.path().is_dir()).collect();
        names.sort_by_key(|e| e.file_name());
        for e in names {
            let name = e.file_name().to_string_lossy().to_string();
            let dst = paths.theme_dir(&name);
            if dst.exists() {
                let same = {
                    let (mut a, mut b) = (BTreeMap::new(), BTreeMap::new());
                    hash_tree(&e.path(), Path::new(""), &mut a)?;
                    hash_tree(&dst, Path::new(""), &mut b)?;
                    a == b
                };
                if same {
                    rep.themes_restored.push(name);
                    continue;
                }
                match io.confirm(&format!("overwrite existing theme `{name}`?")) {
                    Ok(true) => fs::remove_dir_all(&dst)?,
                    Ok(false) | Err(Error::NeedsConfirmation(_)) => {
                        rep.themes_skipped.push(name);
                        continue;
                    }
                    Err(err) => return Err(err),
                }
            }
            copy_tree(&e.path(), &dst)?;
            rep.themes_restored.push(name);
        }
    }
    Ok(rep)
}
