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
use muninn_core::crypto::{self, keyring::{self, Envelope, ExportItem}};
use muninn_core::paths::Paths;
use secrecy::{ExposeSecret, SecretString};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicI64, Ordering},
    LazyLock, Mutex,
};
use std::time::{Duration, Instant};

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


/// A one-time reveal of recovery words and/or shards. The secrets stay in Rust;
/// Dart fetches one line at a time and must call [`keys_reveal_dismiss`] afterwards.
pub struct RevealHandle {
    pub id: i64,
    pub db_id: String,
    pub word_count: i64,
    pub shard_count: i64,
    pub migrated_plain_db: bool,
}

struct Reveal {
    words: Vec<SecretString>,
    shards: Vec<SecretString>,
    created: Instant,
}

static REVEALS: LazyLock<Mutex<HashMap<i64, Reveal>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
static NEXT_REVEAL: AtomicI64 = AtomicI64::new(1);
const REVEAL_TTL: Duration = Duration::from_secs(15 * 60);

fn reveals() -> std::sync::MutexGuard<'static, HashMap<i64, Reveal>> {
    REVEALS.lock().unwrap_or_else(|e| e.into_inner())
}

fn expire(map: &mut HashMap<i64, Reveal>) {
    let now = Instant::now();
    map.retain(|_, r| now.saturating_duration_since(r.created) < REVEAL_TTL);
}

fn put_reveal(db_id: String, migrated: bool, words: Vec<SecretString>, shards: Vec<SecretString>) -> RevealHandle {
    let mut map = reveals();
    expire(&mut map);
    let id = NEXT_REVEAL.fetch_add(1, Ordering::Relaxed);
    let handle = RevealHandle {
        id,
        db_id: db_id.clone(),
        word_count: i64::try_from(words.len()).unwrap_or(0),
        shard_count: i64::try_from(shards.len()).unwrap_or(0),
        migrated_plain_db: migrated,
    };
    map.insert(id, Reveal { words, shards, created: Instant::now() });
    handle
}

fn split_words(s: &str) -> Vec<SecretString> {
    s.split_whitespace().map(|w| SecretString::from(w.to_string())).collect()
}

fn master_from(
    paths: &Paths,
    words: Option<String>,
    shards: Vec<String>,
    words_file: Option<String>,
    master_file: Option<String>,
    file_password: Option<String>,
) -> anyhow::Result<crypto::Key32> {
    let file_pw = || {
        let p = file_password.clone().unwrap_or_default();
        if p.is_empty() {
            Err(muninn_core::Error::invalid("password", "needed to open an encrypted key file"))
        } else {
            Ok(SecretString::from(p))
        }
    };
    let key = if let Some(w) = words.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
        crypto::from_words(&w)?
    } else if let Some(f) = words_file.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
        let raw = keyring::read_maybe_age(Path::new(&f), file_pw)?;
        crypto::from_words(raw.as_str())?
    } else if let Some(f) = master_file.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
        let raw = keyring::read_maybe_age(Path::new(&f), file_pw)?;
        crypto::Key32::from_hex(raw.as_str())?
    } else {
        let shards: Vec<String> = shards.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        if shards.len() < 3 {
            return Err(anyhow::anyhow!("type the 24 recovery words, at least 3 shards, or pick a words / master file"));
        }
        crypto::combine_shards(&shards)?
    };
    keyring::verify_master(paths, &key)?;
    Ok(key)
}

/// `mnnn keys init`. Secrets stay in a reveal session; only counts and the database id cross immediately.
pub fn keys_init(home: Option<String>, password: String) -> anyhow::Result<RevealHandle> {
    if password.is_empty() {
        return Err(anyhow::anyhow!("the password must not be empty"));
    }
    let paths = paths(home)?;
    let out = keyring::init(&paths, &SecretString::from(password))?;
    Ok(put_reveal(out.db_id, out.migrated_plain_db, split_words(out.words.as_str()), out.shards.into_iter().map(|s| SecretString::from(s.to_string())).collect()))
}

/// One recovery word (1-based). Empty if the session has no words (shards-only).
pub fn keys_reveal_word(id: i64, index: i64) -> anyhow::Result<String> {
    let mut map = reveals();
    expire(&mut map);
    let r = map.get(&id).ok_or_else(|| anyhow::anyhow!("reveal session expired or dismissed"))?;
    let i = usize::try_from(index.saturating_sub(1)).unwrap_or(usize::MAX);
    r.words.get(i).map(|w| w.expose_secret().to_string()).ok_or_else(|| anyhow::anyhow!("no word {index}"))
}

/// One dark shard (1-based).
pub fn keys_reveal_shard(id: i64, index: i64) -> anyhow::Result<String> {
    let mut map = reveals();
    expire(&mut map);
    let r = map.get(&id).ok_or_else(|| anyhow::anyhow!("reveal session expired or dismissed"))?;
    let i = usize::try_from(index.saturating_sub(1)).unwrap_or(usize::MAX);
    r.shards.get(i).map(|s| s.expose_secret().to_string()).ok_or_else(|| anyhow::anyhow!("no shard {index}"))
}

/// Forget a reveal session (the TUI Reveal/Hide hide path). Safe to call twice.
pub fn keys_reveal_dismiss(id: i64) {
    reveals().remove(&id);
}

/// `mnnn keys rotate`. Returns the new service-key version. No key material is returned.
pub fn keys_rotate(
    home: Option<String>,
    password: String,
    words: Option<String>,
    shards: Vec<String>,
    words_file: Option<String>,
    master_file: Option<String>,
    file_password: Option<String>,
) -> anyhow::Result<i64> {
    if password.is_empty() {
        return Err(anyhow::anyhow!("the password must not be empty"));
    }
    let paths = paths(home)?;
    let master = master_from(&paths, words, shards, words_file, master_file, file_password)?;
    let v = keyring::rotate(&paths, &master, &SecretString::from(password))?;
    Ok(i64::from(v))
}

/// `mnnn keys rekey`. Locks any open session first (the TUI refuses to rekey while open).
pub fn keys_rekey(
    home: Option<String>,
    words: Option<String>,
    shards: Vec<String>,
    words_file: Option<String>,
    master_file: Option<String>,
    file_password: Option<String>,
) -> anyhow::Result<()> {
    crate::api::data::lock();
    let paths = paths(home)?;
    let master = master_from(&paths, words, shards, words_file, master_file, file_password)?;
    keyring::rekey(&paths, &master)?;
    Ok(())
}

/// `mnnn keys export`. Writes age-encrypted files and returns their paths. No secrets come back.
pub fn keys_export(
    home: Option<String>,
    out: String,
    export_password: String,
    only: Vec<String>,
    confirm_combined: bool,
    words: Option<String>,
    shards: Vec<String>,
    words_file: Option<String>,
    master_file: Option<String>,
    file_password: Option<String>,
) -> anyhow::Result<Vec<String>> {
    if export_password.is_empty() {
        return Err(anyhow::anyhow!("export password must not be empty"));
    }
    let items: Vec<ExportItem> = if only.is_empty() {
        vec![ExportItem::Master, ExportItem::Words, ExportItem::Service, ExportItem::Shards]
    } else {
        only.iter().map(|s| ExportItem::parse(s)).collect::<muninn_core::Result<_>>()?
    };
    let has_master = items.contains(&ExportItem::Master) || items.contains(&ExportItem::Words);
    if has_master && items.contains(&ExportItem::Shards) && !confirm_combined {
        return Err(anyhow::anyhow!(
            "writing the master key/words and shards to one directory defeats the vault/shard split; confirm to continue"
        ));
    }
    let paths = paths(home)?;
    let master = master_from(&paths, words, shards, words_file, master_file, file_password)?;
    let written = keyring::export(&paths, &master, &SecretString::from(export_password), &PathBuf::from(out), &items)?;
    Ok(written.into_iter().map(|p| p.display().to_string()).collect())
}

/// `mnnn keys recover`. Re-wraps the service key with a new password. Optionally opens a words reveal (`--print-words`).
pub fn keys_recover(
    home: Option<String>,
    new_password: String,
    print_words: bool,
    words: Option<String>,
    shards: Vec<String>,
    words_file: Option<String>,
    master_file: Option<String>,
    file_password: Option<String>,
) -> anyhow::Result<Option<RevealHandle>> {
    if new_password.is_empty() {
        return Err(anyhow::anyhow!("the password must not be empty"));
    }
    let paths = paths(home)?;
    let master = master_from(&paths, words, shards, words_file, master_file, file_password)?;
    keyring::recover(&paths, &master, &SecretString::from(new_password))?;
    if !print_words {
        return Ok(None);
    }
    let phrase = crypto::to_words(&master)?;
    Ok(Some(put_reveal(String::new(), false, split_words(phrase.as_str()), Vec::new())))
}

/// `mnnn keys shards` without `--out`: new shards in a reveal session. Old shards cannot be mixed in.
pub fn keys_reissue_shards(
    home: Option<String>,
    words: Option<String>,
    shards: Vec<String>,
    words_file: Option<String>,
    master_file: Option<String>,
    file_password: Option<String>,
) -> anyhow::Result<RevealHandle> {
    let paths = paths(home)?;
    let master = master_from(&paths, words, shards, words_file, master_file, file_password)?;
    let new_shards = crypto::split_shards(&master)?;
    Ok(put_reveal(String::new(), false, Vec::new(), new_shards.into_iter().map(|s| SecretString::from(s.to_string())).collect()))
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

    #[test]
    fn init_reveal_is_ephemeral_and_status_stays_metadata_only() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().display().to_string();
        let h = keys_init(Some(root.clone()), "pw-for-test".into()).unwrap();
        assert_eq!(h.word_count, 24);
        assert_eq!(h.shard_count, 5);
        assert!(!h.db_id.contains(' '));
        let w1 = keys_reveal_word(h.id, 1).unwrap();
        assert!(!w1.contains(' ') && !w1.is_empty());
        let mut words = Vec::new();
        for i in 1..=h.word_count {
            words.push(keys_reveal_word(h.id, i).unwrap());
        }
        assert_eq!(words.len(), 24);
        let phrase = words.join(" ");
        keys_reveal_dismiss(h.id);
        assert!(keys_reveal_word(h.id, 1).is_err(), "dismiss must wipe the session");
        let s = keys_status(Some(root.clone())).unwrap();
        assert!(s.initialized && s.db_id == h.db_id);
        assert!(!s.db_id.contains(' '));
        assert_ne!(s.db_id, phrase);
        assert!(!phrase.split_whitespace().any(|w| s.database.contains(w) && w.len() > 3 && s.db_id.contains(w)));
        // rotate using the collected words (as Dart would after the user typed them later)
        let v = keys_rotate(Some(root.clone()), "pw-for-test".into(), Some(phrase.clone()), vec![], None, None, None).unwrap();
        assert_eq!(v, 2);
        let s = keys_status(Some(root.clone())).unwrap();
        assert_eq!(s.service_version, 2);
        keys_rekey(Some(root.clone()), Some(phrase.clone()), vec![], None, None, None).unwrap();
        let out = tempfile::tempdir().unwrap();
        let written = keys_export(
            Some(root.clone()),
            out.path().display().to_string(),
            "export-pw".into(),
            vec!["words".into()],
            false,
            Some(phrase.clone()),
            vec![],
            None,
            None,
            None,
        )
        .unwrap();
        assert!(written.iter().any(|p| p.ends_with("words.txt.age")), "{written:?}");
        let age = std::fs::read(out.path().join("words.txt.age")).unwrap();
        assert!(!String::from_utf8_lossy(&age).contains(&phrase), "export must be age-encrypted");
        keys_recover(Some(root.clone()), "new-pw".into(), false, Some(phrase.clone()), vec![], None, None, None).unwrap();
        let sh = keys_reissue_shards(Some(root), Some(phrase), vec![], None, None, None).unwrap();
        assert_eq!(sh.word_count, 0);
        assert_eq!(sh.shard_count, 5);
        let shard = keys_reveal_shard(sh.id, 1).unwrap();
        assert!(shard.starts_with("muninn-shard-"), "{shard}");
        keys_reveal_dismiss(sh.id);
    }

    #[test]
    fn init_rejects_empty_password_and_does_not_init() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().display().to_string();
        assert!(keys_init(Some(root.clone()), String::new()).is_err());
        assert!(!keys_status(Some(root)).unwrap().initialized);
    }

    #[test]
    fn export_combined_requires_confirm() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().display().to_string();
        let h = keys_init(Some(root.clone()), "pw-for-test".into()).unwrap();
        let mut words = Vec::new();
        for i in 1..=h.word_count {
            words.push(keys_reveal_word(h.id, i).unwrap());
        }
        keys_reveal_dismiss(h.id);
        let phrase = words.join(" ");
        let out = tempfile::tempdir().unwrap();
        let err = keys_export(
            Some(root),
            out.path().display().to_string(),
            "export-pw".into(),
            vec![],
            false,
            Some(phrase),
            vec![],
            None,
            None,
            None,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("vault/shard split"), "{err}");
    }
}
