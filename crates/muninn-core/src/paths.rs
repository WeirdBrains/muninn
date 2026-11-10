//! `~/.muninn` layout and permissions.
use crate::error::{Error, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub const DB_FILE: &str = "muninn.database";

#[derive(Debug, Clone)]
pub struct Paths {
    pub home: PathBuf,
    pub db: PathBuf,
}

impl Paths {
    /// Resolve the home directory: explicit value (flag / `MUNINN_HOME`), else `~/.muninn`.
    pub fn resolve(home: Option<PathBuf>, db: Option<PathBuf>) -> Result<Self> {
        let home = match home {
            Some(h) => h,
            None => dirs::home_dir().ok_or_else(|| Error::missing("home directory", "pass --home or set MUNINN_HOME"))?.join(".muninn"),
        };
        let db = db.unwrap_or_else(|| home.join("storage").join(DB_FILE));
        Ok(Self { home, db })
    }

    pub fn storage_dir(&self) -> PathBuf {
        self.home.join("storage")
    }
    pub fn keys_dir(&self) -> PathBuf {
        self.home.join("keys")
    }
    pub fn service_keys_dir(&self) -> PathBuf {
        self.keys_dir().join("service")
    }
    pub fn envelope(&self) -> PathBuf {
        self.keys_dir().join("envelope.toml")
    }
    pub fn themes_dir(&self) -> PathBuf {
        self.home.join("themes")
    }
    pub fn theme_dir(&self, name: &str) -> PathBuf {
        self.themes_dir().join(name)
    }
    /// PDFs made for printing.
    pub fn prints_dir(&self) -> PathBuf {
        self.home.join("prints")
    }
    pub fn preferences(&self) -> PathBuf {
        self.home.join("preferences.toml")
    }

    /// Create the directory tree with 0700 directories.
    pub fn ensure(&self) -> Result<()> {
        for d in [self.home.clone(), self.storage_dir(), self.keys_dir(), self.service_keys_dir(), self.themes_dir()] {
            ensure_dir(&d)?;
        }
        if let Some(parent) = self.db.parent() {
            ensure_dir(parent)?;
        }
        Ok(())
    }
}

impl crate::theme::ThemeDirs for Paths {
    fn themes_dir(&self) -> PathBuf {
        Paths::themes_dir(self)
    }
    fn theme_dir(&self, name: &str) -> PathBuf {
        Paths::theme_dir(self, name)
    }
    fn ensure_dir(&self, dir: &Path) -> std::io::Result<()> {
        ensure_dir(dir).map_err(into_io)
    }
    fn write_private(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        write_private(path, bytes).map_err(into_io)
    }
}

fn into_io(e: Error) -> std::io::Error {
    match e {
        Error::Io(e) => e,
        e => std::io::Error::other(e.to_string()),
    }
}

/// Muninn's key-file layout for the keyring (`keys/envelope.toml`, `keys/service/`).
impl crate::crypto::keyring::KeyFiles for Paths {
    fn home(&self) -> PathBuf {
        self.home.clone()
    }
    fn envelope(&self) -> PathBuf {
        Paths::envelope(self)
    }
    fn service_keys_dir(&self) -> PathBuf {
        Paths::service_keys_dir(self)
    }
    fn prepare(&self) -> Result<()> {
        self.ensure()
    }
    fn ensure_dir(&self, dir: &Path) -> Result<()> {
        ensure_dir(dir)
    }
    fn write_private(&self, path: &Path, bytes: &[u8]) -> Result<()> {
        write_private(path, bytes)
    }
}

/// Muninn's database for the keyring: the storage engine does the SQLCipher work; opening
/// through `Store` also creates and migrates Muninn's schema.
impl crate::crypto::keyring::EncryptedDatabase for Paths {
    fn database_exists(&self) -> bool {
        self.db.exists()
    }
    fn encrypt_plaintext(&self, dek: &crate::crypto::Key32) -> Result<()> {
        Ok(weirdnet_storage::encrypt_plaintext_in_place(&self.db, &dek.0)?)
    }
    fn open_keyed(&self, dek: &crate::crypto::Key32) -> Result<()> {
        crate::store::Store::open(&self.db, Some(&dek.0)).map(|_| ())
    }
    fn rekey_database(&self, old: &crate::crypto::Key32, new: &crate::crypto::Key32) -> Result<()> {
        // the engine leaves WAL mode before rekeying
        crate::store::Store::open(&self.db, Some(&old.0))?.rekey(&new.0)
    }
    fn remove_database(&self) {
        let _ = fs::remove_file(&self.db);
    }
}

/// Muninn's side-file extension for private writes (backups skip it).
pub const SIDE_EXT: &str = "tmp-muninn";

/// Create a folder (and parents) and make it private (0700 on Unix): the storage engine's.
pub fn ensure_dir(d: &Path) -> Result<()> {
    Ok(weirdnet_storage::files::ensure_dir(d)?)
}

/// Set Unix permission bits (no-op elsewhere): the storage engine's.
pub fn set_mode(p: &Path, mode: u32) -> Result<()> {
    Ok(weirdnet_storage::files::set_mode(p, mode)?)
}

/// Write a file with 0600 permissions through a private `tmp-muninn` side file renamed into
/// place: the storage engine's private write, with Muninn's side-file name.
pub fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    Ok(weirdnet_storage::files::write_private_with(path, bytes, SIDE_EXT)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_and_permissions() {
        let tmp = tempfile::tempdir().unwrap();
        let p = Paths::resolve(Some(tmp.path().join("m")), None).unwrap();
        p.ensure().unwrap();
        assert!(p.storage_dir().is_dir());
        assert!(p.service_keys_dir().is_dir());
        assert!(p.themes_dir().is_dir());
        assert_eq!(p.db, tmp.path().join("m/storage/muninn.database"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let m = fs::metadata(&p.home).unwrap().permissions().mode() & 0o777;
            assert_eq!(m, 0o700);
            write_private(&p.preferences(), b"x").unwrap();
            let m = fs::metadata(p.preferences()).unwrap().permissions().mode() & 0o777;
            assert_eq!(m, 0o600);
            assert!(!p.preferences().with_extension(SIDE_EXT).exists());
        }
    }
}
