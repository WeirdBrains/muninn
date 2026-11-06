//! Read-only key status (`mnnn keys status`).
//!
//! Only metadata crosses to Dart: the database id, the database path, and which
//! service-key versions exist (active or retired). The master key, recovery words,
//! shards, passphrases, wrapped keys and raw key bytes stay in Rust (and are never
//! stored in plaintext by Muninn).
//!
//! Uses the same [`paths`] as the store (`unlock`, `data_dir_info`): the `home` the
//! app already resolved. On desktop that is `MUNINN_HOME` or `~/.muninn`; on a phone
//! it is the platform-secure app sandbox, not a desktop-style `~/.muninn`.
use super::info::paths;
use muninn_core::crypto::keyring::{self, Envelope};
use muninn_core::paths::Paths;

/// One on-disk service key file: version and whether it has been retired. No wrapped key.
pub struct ServiceKeyInfo {
    pub version: i64,
    pub retired: bool,
}

/// What `mnnn keys status` prints. No secrets.
pub struct KeysStatus {
    pub initialized: bool,
    pub db_id: String,
    pub database: String,
    pub service_version: i64,
    pub services: Vec<ServiceKeyInfo>,
}

pub(crate) fn keys_status_of(paths: &Paths) -> anyhow::Result<KeysStatus> {
    let database = paths.db.display().to_string();
    let Some(env) = Envelope::load(paths)? else {
        return Ok(KeysStatus { initialized: false, db_id: String::new(), database, service_version: 0, services: Vec::new() });
    };
    // Copy only version + retired; drop `wrapped` (the password-sealed service key).
    let services = keyring::service_files(paths)?.into_iter().map(|f| ServiceKeyInfo { version: i64::from(f.version), retired: f.retired }).collect();
    Ok(KeysStatus { initialized: true, db_id: env.db_id, database, service_version: i64::from(env.service_version), services })
}

/// `mnnn keys status`. Reads the envelope and service-key files; does not unlock the store
/// and does not create a data directory.
pub fn keys_status(home: Option<String>) -> anyhow::Result<KeysStatus> {
    keys_status_of(&paths(home)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use muninn_core::crypto::{self, keyring};
    use muninn_core::paths::Paths;
    use secrecy::SecretString;

    #[test]
    fn missing_home_is_uninitialized_and_creates_nothing() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("h");
        let s = keys_status(Some(root.display().to_string())).unwrap();
        assert!(!s.initialized);
        assert!(s.db_id.is_empty() && s.services.is_empty() && s.service_version == 0);
        assert!(!root.exists(), "status must not create the data dir");
    }

    #[test]
    fn status_after_init_is_metadata_only() {
        let home = tempfile::tempdir().unwrap();
        let paths = Paths::resolve(Some(home.path().to_path_buf()), None).unwrap();
        let out = keyring::init(&paths, &SecretString::from("pw-for-test")).unwrap();
        let s = keys_status_of(&paths).unwrap();
        assert!(s.initialized);
        assert_eq!(s.db_id, out.db_id);
        assert_eq!(s.service_version, 1);
        assert_eq!(s.services.len(), 1);
        assert_eq!((s.services[0].version, s.services[0].retired), (1, false));
        assert!(s.database.ends_with("muninn.database"), "{}", s.database);
        // The words exist on InitOutput for recovery; status must not echo them.
        assert_ne!(s.db_id, out.words.as_str());
        assert!(!s.db_id.contains(' '));
    }

    #[test]
    fn rotate_lists_retired_and_active_versions() {
        let home = tempfile::tempdir().unwrap();
        let paths = Paths::resolve(Some(home.path().to_path_buf()), None).unwrap();
        let pw = SecretString::from("pw-for-test");
        let out = keyring::init(&paths, &pw).unwrap();
        let master = crypto::from_words(out.words.as_str()).unwrap();
        let v = keyring::rotate(&paths, &master, &pw).unwrap();
        assert_eq!(v, 2);
        let s = keys_status_of(&paths).unwrap();
        assert_eq!(s.service_version, 2);
        assert_eq!(s.services.len(), 2);
        let v1 = s.services.iter().find(|x| x.version == 1).unwrap();
        let v2 = s.services.iter().find(|x| x.version == 2).unwrap();
        assert!(v1.retired && !v2.retired);
    }
}
