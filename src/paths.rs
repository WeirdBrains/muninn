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

pub fn ensure_dir(d: &Path) -> Result<()> {
    if !d.exists() {
        fs::create_dir_all(d)?;
    }
    set_mode(d, 0o700)
}

/// Write a file atomically-ish with 0600 permissions.
pub fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp-muninn");
    fs::write(&tmp, bytes)?;
    set_mode(&tmp, 0o600)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

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
        }
    }
}
