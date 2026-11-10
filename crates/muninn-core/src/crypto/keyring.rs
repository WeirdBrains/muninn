//! Envelope encryption for the database (`keys/envelope.toml` + `keys/service/v{n}.toml`).
//!
//! A random data key (DEK) keys SQLCipher. The DEK is stored wrapped twice: by the current
//! service key and by a master-derived KEK. The service key itself is stored wrapped by the
//! user's password (Argon2id). The master key, words and shards are never stored.
//!
//! This module owns every key format and operation (wrap, unwrap, rotate, rekey policy,
//! recovery, export). It does not know the app's folder layout or its database: both come in
//! through the two traits below, which the app implements (Muninn: `crate::paths::Paths`,
//! over the storage engine in `crate::store::engine`). Storage only ever receives the raw DEK
//! to apply, and opaque already-encrypted bytes to write.
use super::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Where the key files live and how they are written (privately).
pub trait KeyFiles {
    /// The home shown in the not-initialized error.
    fn home(&self) -> PathBuf;
    fn envelope(&self) -> PathBuf;
    fn service_keys_dir(&self) -> PathBuf;
    /// Create the folders a first `init` needs.
    fn prepare(&self) -> Result<()>;
    fn ensure_dir(&self, dir: &Path) -> Result<()>;
    /// Write already-encrypted (or public) bytes with private permissions.
    fn write_private(&self, path: &Path, bytes: &[u8]) -> Result<()>;
}

/// The database the DEK keys. This module decides when to create, convert or rekey it; the
/// implementation (the storage engine) does the work with the raw key it is handed.
pub trait EncryptedDatabase {
    fn database_exists(&self) -> bool;
    /// Encrypt an existing plaintext database in place.
    fn encrypt_plaintext(&self, dek: &Key32) -> Result<()>;
    /// Open with the key, creating (and migrating) it if missing; fails on a wrong key.
    fn open_keyed(&self, dek: &Key32) -> Result<()>;
    /// Re-encrypt from `old` to `new`.
    fn rekey_database(&self, old: &Key32, new: &Key32) -> Result<()>;
    /// Delete a database a failed `init` created.
    fn remove_database(&self);
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Envelope {
    pub format: u32,
    pub db_id: String,
    pub created_at: i64,
    pub service_version: u32,
    /// DEK wrapped by the current service key.
    pub service_wrap: Sealed,
    /// DEK wrapped by the master-derived KEK.
    pub master_wrap: Sealed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServiceKeyFile {
    pub version: u32,
    pub created_at: i64,
    pub retired: bool,
    /// Service key wrapped by the password; removed when retired.
    pub wrapped: Option<PasswordSealed>,
}

pub(crate) const DEK_AAD: &[u8] = b"muninn/dek";

pub(crate) fn svc_aad(db_id: &str, v: u32) -> Vec<u8> {
    format!("muninn/service-key/{db_id}/v{v}").into_bytes()
}

impl Envelope {
    pub fn load(paths: &(impl KeyFiles + ?Sized)) -> Result<Option<Self>> {
        let p = paths.envelope();
        if !p.exists() {
            return Ok(None);
        }
        Ok(Some(toml::from_str(&std::fs::read_to_string(p)?)?))
    }

    pub fn require(paths: &(impl KeyFiles + ?Sized)) -> Result<Self> {
        Self::load(paths)?.ok_or_else(|| Error::NotInitialized(paths.home()))
    }

    pub fn save(&self, paths: &(impl KeyFiles + ?Sized)) -> Result<()> {
        paths.write_private(&paths.envelope(), toml::to_string_pretty(self)?.as_bytes())
    }
}

fn service_file(paths: &(impl KeyFiles + ?Sized), v: u32) -> std::path::PathBuf {
    paths.service_keys_dir().join(format!("v{v}.toml"))
}

pub fn load_service_file(paths: &(impl KeyFiles + ?Sized), v: u32) -> Result<ServiceKeyFile> {
    let p = service_file(paths, v);
    if !p.exists() {
        return Err(Error::Unlock(format!("service key v{v} is missing ({})", p.display())));
    }
    Ok(toml::from_str(&std::fs::read_to_string(p)?)?)
}

pub fn service_files(paths: &(impl KeyFiles + ?Sized)) -> Result<Vec<ServiceKeyFile>> {
    let mut v = Vec::new();
    if let Ok(rd) = std::fs::read_dir(paths.service_keys_dir()) {
        for e in rd.flatten() {
            if e.path().extension().is_some_and(|x| x == "toml") {
                v.push(toml::from_str::<ServiceKeyFile>(&std::fs::read_to_string(e.path())?)?);
            }
        }
    }
    v.sort_by_key(|f| f.version);
    Ok(v)
}

fn write_service_file(paths: &(impl KeyFiles + ?Sized), db_id: &str, v: u32, key: &Key32, password: &SecretString) -> Result<()> {
    let f = ServiceKeyFile { version: v, created_at: now_ms(), retired: false, wrapped: Some(password_seal(password, &key.0, &svc_aad(db_id, v))?) };
    paths.write_private(&service_file(paths, v), toml::to_string_pretty(&f)?.as_bytes())
}

fn retire_service_file(paths: &(impl KeyFiles + ?Sized), v: u32) -> Result<()> {
    if let Ok(mut f) = load_service_file(paths, v) {
        f.retired = true;
        f.wrapped = None;
        paths.write_private(&service_file(paths, v), toml::to_string_pretty(&f)?.as_bytes())?;
    }
    Ok(())
}

/// Result of `keys init`. Show once, then drop.
pub struct InitOutput {
    pub db_id: String,
    pub words: Zeroizing<String>,
    pub shards: Vec<Zeroizing<String>>,
    pub migrated_plain_db: bool,
}

/// Create master key, DEK, envelope and service key; encrypt (or create) the database.
pub fn init(paths: &(impl KeyFiles + EncryptedDatabase + ?Sized), password: &SecretString) -> Result<InitOutput> {
    if paths.envelope().exists() {
        return Err(Error::invalid("keys", format!("already initialized ({})", paths.envelope().display())));
    }
    // Check the password before touching disk: a failed first run must leave nothing behind
    // that a retry would trip over (a fresh encrypted database without an envelope).
    if password.expose_secret().is_empty() {
        return Err(Error::invalid("password", "must not be empty"));
    }
    paths.prepare()?;
    let master = Key32::random()?;
    let dek = Key32::random()?;
    let db_id = uuid::Uuid::new_v4().to_string();
    let svc = service_key(&master, &db_id, 1)?;
    let kek = master_kek(&master, &db_id)?;

    // Encrypt an existing plaintext database in place, or create a new encrypted one.
    let mut migrated = false;
    if paths.database_exists() {
        paths.encrypt_plaintext(&dek)?;
        migrated = true;
    }
    paths.open_keyed(&dek)?;

    let finish = || -> Result<()> {
        write_service_file(paths, &db_id, 1, &svc, password)?;
        let env = Envelope {
            format: 1,
            db_id: db_id.clone(),
            created_at: now_ms(),
            service_version: 1,
            service_wrap: seal(&svc, &dek.0, DEK_AAD)?,
            master_wrap: seal(&kek, &dek.0, DEK_AAD)?,
        };
        env.save(paths)
    };
    if let Err(e) = finish() {
        // Undo a database this call created; its key is about to be lost.
        if !migrated {
            paths.remove_database();
        }
        let _ = std::fs::remove_file(service_file(paths, 1));
        return Err(e);
    }
    Ok(InitOutput { db_id, words: to_words(&master)?, shards: split_shards(&master)?, migrated_plain_db: migrated })
}

/// Unlock the DEK with the password-wrapped current service key.
pub fn unlock_with_password(paths: &(impl KeyFiles + ?Sized), password: &SecretString) -> Result<Key32> {
    let env = Envelope::require(paths)?;
    let f = load_service_file(paths, env.service_version)?;
    let w = f.wrapped.ok_or_else(|| Error::Unlock("current service key is retired".into()))?;
    let svc = Key32::from_slice(&password_open(password, &w, &svc_aad(&env.db_id, env.service_version))?)?;
    Key32::from_slice(&open(&svc, &env.service_wrap, DEK_AAD)?)
}

/// Unlock the DEK with the master key.
pub fn unlock_with_master(paths: &(impl KeyFiles + ?Sized), master: &Key32) -> Result<Key32> {
    let env = Envelope::require(paths)?;
    let kek = master_kek(master, &env.db_id)?;
    Key32::from_slice(&open(&kek, &env.master_wrap, DEK_AAD).map_err(|_| Error::Unlock("master key does not match this database".into()))?)
}

/// Rotate: derive service key v{n+1}, re-wrap the DEK, retire the old one.
pub fn rotate(paths: &(impl KeyFiles + ?Sized), master: &Key32, password: &SecretString) -> Result<u32> {
    let mut env = Envelope::require(paths)?;
    let dek = unlock_with_master(paths, master)?;
    let old = env.service_version;
    let new = old + 1;
    let svc = service_key(master, &env.db_id, new)?;
    write_service_file(paths, &env.db_id, new, &svc, password)?;
    env.service_version = new;
    env.service_wrap = seal(&svc, &dek.0, DEK_AAD)?;
    env.save(paths)?;
    retire_service_file(paths, old)?;
    Ok(new)
}

/// Full rekey: new DEK, `PRAGMA rekey`, re-wrap by service key and master KEK.
pub fn rekey(paths: &(impl KeyFiles + EncryptedDatabase + ?Sized), master: &Key32) -> Result<()> {
    let mut env = Envelope::require(paths)?;
    let old = unlock_with_master(paths, master)?;
    let new = Key32::random()?;
    paths.rekey_database(&old, &new)?;
    // Verify before committing the new envelope.
    paths.open_keyed(&new)?;
    let svc = service_key(master, &env.db_id, env.service_version)?;
    env.service_wrap = seal(&svc, &new.0, DEK_AAD)?;
    env.master_wrap = seal(&master_kek(master, &env.db_id)?, &new.0, DEK_AAD)?;
    env.save(paths)?;
    Ok(())
}

/// Recover access: verify the master, then re-wrap the current service key with a (new) password.
pub fn recover(paths: &(impl KeyFiles + ?Sized), master: &Key32, new_password: &SecretString) -> Result<()> {
    let env = Envelope::require(paths)?;
    unlock_with_master(paths, master)?;
    let svc = service_key(master, &env.db_id, env.service_version)?;
    write_service_file(paths, &env.db_id, env.service_version, &svc, new_password)
}

/// Verify a master key matches this database.
pub fn verify_master(paths: &(impl KeyFiles + ?Sized), master: &Key32) -> Result<()> {
    unlock_with_master(paths, master).map(|_| ())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportItem {
    Master,
    Words,
    Service,
    Shards,
}

impl ExportItem {
    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "master" => Ok(Self::Master),
            "words" => Ok(Self::Words),
            "service" | "service-keys" => Ok(Self::Service),
            "shards" => Ok(Self::Shards),
            o => Err(Error::invalid("--only", format!("`{o}` (master, words, service, shards)"))),
        }
    }
}

/// Write each item as its own age (password) file. Returns the written paths.
pub fn export(
    paths: &(impl KeyFiles + ?Sized),
    master: &Key32,
    password: &SecretString,
    out: &Path,
    items: &[ExportItem],
) -> Result<Vec<std::path::PathBuf>> {
    verify_master(paths, master)?;
    paths.ensure_dir(out)?;
    let mut written = Vec::new();
    let mut put = |name: &str, data: &[u8]| -> Result<()> {
        let p = out.join(name);
        paths.write_private(&p, &age_encrypt(password, data)?)?;
        written.push(p);
        Ok(())
    };
    for it in items {
        match it {
            ExportItem::Master => put("master.key.age", master.hex().as_bytes())?,
            ExportItem::Words => put("words.txt.age", to_words(master)?.as_bytes())?,
            ExportItem::Service => {
                let env = Envelope::require(paths)?;
                let j = serde_json::json!({ "envelope": env, "service_keys": service_files(paths)? });
                put("service-keys.json.age", serde_json::to_string_pretty(&j)?.as_bytes())?
            }
            ExportItem::Shards => {
                for (i, s) in split_shards(master)?.iter().enumerate() {
                    put(&format!("shard-{}.age", i + 1), s.as_bytes())?;
                }
            }
        }
    }
    Ok(written)
}

/// Read a key file that may be age-encrypted.
pub fn read_maybe_age(path: &Path, password: impl FnOnce() -> Result<SecretString>) -> Result<Zeroizing<String>> {
    let b = Zeroizing::new(std::fs::read(path)?);
    let plain = if is_age(&b) { age_decrypt(&password()?, &b)? } else { Zeroizing::new(b.to_vec()) };
    Ok(Zeroizing::new(String::from_utf8_lossy(&plain).trim().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Bar, Duration, Symbol};
    use crate::paths::Paths;
    use crate::store::Store;

    /// The keyring needs nothing but its two traits: a fake home (no app layout, no SQLCipher)
    /// sees exactly the calls the key policy makes, with the DEK the envelope wraps.
    #[test]
    fn keyring_drives_storage_only_through_its_traits() {
        use std::cell::RefCell;
        struct Fake {
            dir: PathBuf,
            calls: RefCell<Vec<String>>,
            keys: RefCell<Vec<[u8; 32]>>,
        }
        impl KeyFiles for Fake {
            fn home(&self) -> PathBuf {
                self.dir.clone()
            }
            fn envelope(&self) -> PathBuf {
                self.dir.join("envelope.toml")
            }
            fn service_keys_dir(&self) -> PathBuf {
                self.dir.join("service")
            }
            fn prepare(&self) -> Result<()> {
                Ok(std::fs::create_dir_all(self.service_keys_dir())?)
            }
            fn ensure_dir(&self, d: &Path) -> Result<()> {
                Ok(std::fs::create_dir_all(d)?)
            }
            fn write_private(&self, p: &Path, b: &[u8]) -> Result<()> {
                Ok(std::fs::write(p, b)?)
            }
        }
        impl EncryptedDatabase for Fake {
            fn database_exists(&self) -> bool {
                false
            }
            fn encrypt_plaintext(&self, _: &Key32) -> Result<()> {
                unreachable!()
            }
            fn open_keyed(&self, k: &Key32) -> Result<()> {
                self.calls.borrow_mut().push("open".into());
                self.keys.borrow_mut().push(k.0);
                Ok(())
            }
            fn rekey_database(&self, old: &Key32, new: &Key32) -> Result<()> {
                self.calls.borrow_mut().push("rekey".into());
                assert_eq!(self.keys.borrow().last(), Some(&old.0));
                assert_ne!(old.0, new.0);
                Ok(())
            }
            fn remove_database(&self) {
                self.calls.borrow_mut().push("remove".into());
            }
        }
        let t = tempfile::tempdir().unwrap();
        let f = Fake { dir: t.path().join("k"), calls: Default::default(), keys: Default::default() };
        let pw = SecretString::from("pw".to_string());
        let out = init(&f, &pw).unwrap();
        let dek = unlock_with_password(&f, &pw).unwrap();
        assert_eq!(f.keys.borrow()[0], dek.0, "the database is opened with the DEK the envelope wraps");
        let master = from_words(&out.words).unwrap();
        rekey(&f, &master).unwrap();
        let dek2 = unlock_with_password(&f, &pw).unwrap();
        assert_eq!(*f.calls.borrow(), ["open", "rekey", "open"]);
        assert_eq!(f.keys.borrow()[1], dek2.0);
        assert!(f.envelope().exists() && !t.path().join("k").join("storage").exists());
    }

    fn setup() -> (tempfile::TempDir, Paths) {
        let t = tempfile::tempdir().unwrap();
        let p = Paths::resolve(Some(t.path().join("h")), None).unwrap();
        p.ensure().unwrap();
        (t, p)
    }

    #[test]
    fn a_failed_first_init_leaves_nothing_behind_so_a_retry_works() {
        let (_t, paths) = setup();
        assert!(init(&paths, &SecretString::from("")).is_err());
        assert!(!paths.db.exists() && !paths.envelope().exists(), "an empty password must not create a database");
        init(&paths, &SecretString::from("pw1")).expect("the retry initializes");
        assert!(unlock_with_password(&paths, &SecretString::from("pw1")).is_ok());
    }

    #[test]
    fn init_unlock_rotate_rekey_recover() {
        let (_t, paths) = setup();
        let pw = SecretString::from("pw1");
        let out = init(&paths, &pw).unwrap();
        let master = from_words(&out.words).unwrap();
        let shards: Vec<String> = out.shards.iter().map(|s| s.to_string()).collect();
        assert_eq!(combine_shards(&shards[1..4]).unwrap(), master);

        let dek = unlock_with_password(&paths, &pw).unwrap();
        {
            let mut s = Store::open(&paths.db, Some(&dek.0)).unwrap();
            let src = s.add_source("x", "file", None).unwrap();
            let b = Bar { ts: 0, open: 1.0, high: 1.0, low: 1.0, close: 1.0, volume: 1.0, flags: 0 };
            s.upsert_bars(&src, &Symbol::new("BTC", "USD"), Duration(60), &[b]).unwrap();
        }
        // Encrypted on disk: no plaintext header, wrong key fails.
        let head = std::fs::read(&paths.db).unwrap();
        assert!(!head.starts_with(b"SQLite format 3"));
        assert!(Store::open(&paths.db, Some(&[7u8; 32])).is_err());
        assert!(unlock_with_password(&paths, &SecretString::from("bad")).is_err());

        // Rotation keeps the DB readable via password and master.
        assert_eq!(rotate(&paths, &master, &pw).unwrap(), 2);
        let dek2 = unlock_with_password(&paths, &pw).unwrap();
        assert_eq!(dek2, dek);
        assert_eq!(unlock_with_master(&paths, &master).unwrap(), dek);
        assert!(load_service_file(&paths, 1).unwrap().retired);

        // Rekey changes the DEK and keeps data.
        rekey(&paths, &master).unwrap();
        let dek3 = unlock_with_password(&paths, &pw).unwrap();
        assert_ne!(dek3, dek);
        assert_eq!(Store::open(&paths.db, Some(&dek3.0)).unwrap().count_bars().unwrap(), 1);
        assert!(Store::open(&paths.db, Some(&dek.0)).is_err());

        // Recover with a new password from shards.
        let m = combine_shards(&[shards[0].clone(), shards[2].clone(), shards[4].clone()]).unwrap();
        recover(&paths, &m, &SecretString::from("pw2")).unwrap();
        assert!(unlock_with_password(&paths, &pw).is_err());
        assert_eq!(unlock_with_password(&paths, &SecretString::from("pw2")).unwrap(), dek3);

        // A foreign master is rejected.
        assert!(verify_master(&paths, &Key32::random().unwrap()).is_err());
        // keys/ never contains the words or a shard.
        for e in walk(&paths.keys_dir()) {
            let s = std::fs::read_to_string(e).unwrap();
            assert!(!s.contains(out.words.split_whitespace().next().unwrap()) || !s.contains(&*out.words));
            assert!(!s.contains("muninn-shard"));
            assert!(!s.contains(&*master.hex()));
        }
    }

    fn walk(d: &Path) -> Vec<std::path::PathBuf> {
        let mut v = vec![];
        for e in std::fs::read_dir(d).unwrap().flatten() {
            if e.path().is_dir() {
                v.extend(walk(&e.path()));
            } else {
                v.push(e.path());
            }
        }
        v
    }

    #[test]
    fn plaintext_db_is_migrated() {
        let (_t, paths) = setup();
        {
            let mut s = Store::open(&paths.db, None).unwrap();
            let src = s.add_source("x", "file", None).unwrap();
            let b = Bar { ts: 0, open: 1.0, high: 1.0, low: 1.0, close: 1.0, volume: 1.0, flags: 0 };
            s.upsert_bars(&src, &Symbol::new("BTC", "USD"), Duration(60), &[b]).unwrap();
        }
        let pw = SecretString::from("pw");
        let out = init(&paths, &pw).unwrap();
        assert!(out.migrated_plain_db);
        let dek = unlock_with_password(&paths, &pw).unwrap();
        assert_eq!(Store::open(&paths.db, Some(&dek.0)).unwrap().count_bars().unwrap(), 1);
        assert!(init(&paths, &pw).is_err());
    }

    #[test]
    fn export_files_are_separate_and_decryptable() {
        let (t, paths) = setup();
        let pw = SecretString::from("pw");
        let out = init(&paths, &pw).unwrap();
        let master = from_words(&out.words).unwrap();
        let dir = t.path().join("exp");
        let files = export(&paths, &master, &pw, &dir, &[ExportItem::Master, ExportItem::Words, ExportItem::Service, ExportItem::Shards]).unwrap();
        assert_eq!(files.len(), 8);
        let m = read_maybe_age(&dir.join("master.key.age"), || Ok(pw.clone())).unwrap();
        assert_eq!(Key32::from_hex(&m).unwrap(), master);
        let shards: Vec<String> =
            (1..=3).map(|i| read_maybe_age(&dir.join(format!("shard-{i}.age")), || Ok(pw.clone())).unwrap().to_string()).collect();
        assert_eq!(combine_shards(&shards).unwrap(), master);
        assert!(read_maybe_age(&dir.join("words.txt.age"), || Ok(SecretString::from("wrong"))).is_err());
    }
}
