//! Version and data-directory facts.
use muninn_core::paths::Paths;
use std::path::PathBuf;

/// Muninn's version (the same as `mnnn --version`).
#[flutter_rust_bridge::frb(sync)]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Where Muninn keeps its data, and what exists there. Never creates anything.
pub struct DataDirInfo {
    pub home: String,
    pub database: String,
    pub database_exists: bool,
    pub keys_initialized: bool,
    pub themes_dir: String,
}

/// Resolve the data dir like `mnnn` does (`home`, else `MUNINN_HOME`, else `~/.muninn`).
pub fn data_dir_info(home: Option<String>) -> anyhow::Result<DataDirInfo> {
    let p = paths(home)?;
    Ok(DataDirInfo {
        home: p.home.display().to_string(),
        database: p.db.display().to_string(),
        database_exists: p.db.exists(),
        keys_initialized: p.envelope().exists(),
        themes_dir: p.themes_dir().display().to_string(),
    })
}

/// `home`, else `MUNINN_HOME`, else `~/.muninn` (the same order as `mnnn --home`).
pub(crate) fn paths(home: Option<String>) -> anyhow::Result<Paths> {
    let home = home.filter(|h| !h.is_empty()).or_else(|| std::env::var("MUNINN_HOME").ok().filter(|h| !h.is_empty()));
    Ok(Paths::resolve(home.map(PathBuf::from), None)?)
}

#[cfg(test)]
mod tests {
    #[test]
    fn data_dir_info_never_creates() {
        let home = tempfile::tempdir().unwrap();
        let d = super::data_dir_info(Some(home.path().join("h").display().to_string())).unwrap();
        assert!(!d.database_exists && !d.keys_initialized);
        assert!(!home.path().join("h").exists());
        assert_eq!(super::version(), env!("CARGO_PKG_VERSION"));
    }
}
