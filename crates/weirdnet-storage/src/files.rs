//! Private folders and files for opaque bytes (key files, settings, exports): folders 0700,
//! files 0600 on Unix. Other platforms keep their default permissions.
use crate::error::Result;
use std::fs;
use std::io::Write;
use std::path::Path;

/// The side-file extension [`write_private`] uses.
pub const SIDE_EXT: &str = "tmp";

/// Create a folder (and its parents) and make it private (0700 on Unix).
pub fn ensure_dir(d: &Path) -> Result<()> {
    if !d.exists() {
        fs::create_dir_all(d)?;
    }
    set_mode(d, 0o700)
}

/// Set Unix permission bits (no-op elsewhere).
#[cfg(unix)]
pub fn set_mode(p: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(p, fs::Permissions::from_mode(mode))?;
    Ok(())
}

#[cfg(not(unix))]
pub fn set_mode(_p: &Path, _mode: u32) -> Result<()> {
    Ok(())
}

/// Create a new, empty file that is private from the start (0600 on Unix); fails if it exists.
pub fn create_private(p: &Path) -> Result<fs::File> {
    let mut o = fs::OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    let f = o.open(p)?;
    set_mode(p, 0o600)?;
    Ok(f)
}

/// Write `bytes` to `path` with 0600 permissions: into a side file (`path` with the
/// extension [`SIDE_EXT`]) that is private from creation, flushed to disk, then renamed over
/// `path`. On failure the side file is removed and `path` is left as it was. Missing parent
/// folders are created (with default permissions; use [`ensure_dir`] for private ones).
pub fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    write_private_with(path, bytes, SIDE_EXT)
}

/// [`write_private`] with the caller's side-file extension (so its backups can skip it).
pub fn write_private_with(path: &Path, bytes: &[u8], side_ext: &str) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
        && !parent.exists()
    {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(side_ext);
    if tmp.exists() {
        fs::remove_file(&tmp)?;
    }
    let r = (|| -> Result<()> {
        let mut f = create_private(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, path)?;
        Ok(())
    })();
    if r.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    r?;
    sync_parent(path);
    Ok(())
}

/// Flush a rename to disk (best effort; only meaningful on Unix).
pub(crate) fn sync_parent(path: &Path) {
    #[cfg(unix)]
    if let Some(parent) = path.parent()
        && let Ok(d) = fs::File::open(if parent.as_os_str().is_empty() { Path::new(".") } else { parent })
    {
        let _ = d.sync_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_writes_replace_atomically() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("a").join("secret.bin");
        write_private(&p, b"one").unwrap();
        write_private_with(&p, b"two", "side").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"two");
        assert!(!p.with_extension(SIDE_EXT).exists() && !p.with_extension("side").exists());
        // a failed write leaves the old file and no side file
        let dir_as_target = t.path().join("a");
        assert!(write_private(&dir_as_target, b"x").is_err());
        assert!(!dir_as_target.with_extension(SIDE_EXT).exists());
        assert_eq!(fs::read(&p).unwrap(), b"two");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o600);
            ensure_dir(&t.path().join("b/c")).unwrap();
            assert_eq!(fs::metadata(t.path().join("b/c")).unwrap().permissions().mode() & 0o777, 0o700);
        }
    }
}
