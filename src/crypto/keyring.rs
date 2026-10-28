//! Envelope encryption for the database (`keys/envelope.toml` + `keys/service/v{n}.toml`).
//!
//! A random data key (DEK) keys SQLCipher. The DEK is stored wrapped twice: by the current
//! service key and by a master-derived KEK. The service key itself is stored wrapped by the
//! user's password (Argon2id). The master key, words and shards are never stored.
use super::*;
use crate::paths::{Paths, write_private};
use crate::store::{Store, now_ms};
use serde::{Deserialize, Serialize};
use std::path::Path;

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

const DEK_AAD: &[u8] = b"muninn/dek";

fn svc_aad(db_id: &str, v: u32) -> Vec<u8> {
    format!("muninn/service-key/{db_id}/v{v}").into_bytes()
}

impl Envelope {
    pub fn load(paths: &Paths) -> Result<Option<Self>> {
        let p = paths.envelope();
        if !p.exists() {
            return Ok(None);
        }
        Ok(Some(toml::from_str(&std::fs::read_to_string(p)?)?))
    }

    pub fn require(paths: &Paths) -> Result<Self> {
        Self::load(paths)?.ok_or_else(|| Error::NotInitialized(paths.home.clone()))
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        write_private(&paths.envelope(), toml::to_string_pretty(self)?.as_bytes())
    }
}

fn service_file(paths: &Paths, v: u32) -> std::path::PathBuf {
    paths.service_keys_dir().join(format!("v{v}.toml"))
}

pub fn load_service_file(paths: &Paths, v: u32) -> Result<ServiceKeyFile> {
    let p = service_file(paths, v);
    if !p.exists() {
        return Err(Error::Unlock(format!("service key v{v} is missing ({})", p.display())));
    }
    Ok(toml::from_str(&std::fs::read_to_string(p)?)?)
}

pub fn service_files(paths: &Paths) -> Result<Vec<ServiceKeyFile>> {
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

fn write_service_file(paths: &Paths, db_id: &str, v: u32, key: &Key32, password: &SecretString) -> Result<()> {
    let f = ServiceKeyFile { version: v, created_at: now_ms(), retired: false, wrapped: Some(password_seal(password, &key.0, &svc_aad(db_id, v))?) };
    write_private(&service_file(paths, v), toml::to_string_pretty(&f)?.as_bytes())
}

fn retire_service_file(paths: &Paths, v: u32) -> Result<()> {
    if let Ok(mut f) = load_service_file(paths, v) {
        f.retired = true;
        f.wrapped = None;
        write_private(&service_file(paths, v), toml::to_string_pretty(&f)?.as_bytes())?;
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
pub fn init(paths: &Paths, password: &SecretString) -> Result<InitOutput> {
    if paths.envelope().exists() {
        return Err(Error::invalid("keys", format!("already initialized ({})", paths.envelope().display())));
    }
    paths.ensure()?;
    let master = Key32::random()?;
    let dek = Key32::random()?;
    let db_id = uuid::Uuid::new_v4().to_string();
    let svc = service_key(&master, &db_id, 1)?;
    let kek = master_kek(&master, &db_id)?;

    // Encrypt an existing plaintext database in place, or create a new encrypted one.
    let mut migrated = false;
    if paths.db.exists() {
        encrypt_plain_db(&paths.db, &dek)?;
        migrated = true;
    }
    Store::open(&paths.db, Some(&dek.0))?;

    write_service_file(paths, &db_id, 1, &svc, password)?;
    let env = Envelope {
        format: 1,
        db_id: db_id.clone(),
        created_at: now_ms(),
        service_version: 1,
        service_wrap: seal(&svc, &dek.0, DEK_AAD)?,
        master_wrap: seal(&kek, &dek.0, DEK_AAD)?,
    };
    env.save(paths)?;
    Ok(InitOutput { db_id, words: to_words(&master)?, shards: split_shards(&master)?, migrated_plain_db: migrated })
}

fn encrypt_plain_db(path: &Path, dek: &Key32) -> Result<()> {
    let tmp = path.with_extension("encrypting");
    if tmp.exists() {
        std::fs::remove_file(&tmp)?;
    }
    {
        let conn = rusqlite::Connection::open(path)?;
        conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0))
            .map_err(|_| Error::Unlock("existing database is not a plaintext SQLite file".into()))?;
        let mut lit = Zeroizing::new(format!("x'{}'", hex::encode(dek.0)));
        conn.execute(&format!("ATTACH DATABASE ?1 AS enc KEY \"{}\"", lit.as_str()), [tmp.to_string_lossy()])?;
        lit.zeroize();
        conn.query_row("SELECT sqlcipher_export('enc')", [], |_| Ok(()))?;
        conn.execute("DETACH DATABASE enc", [])?;
    }
    for ext in ["database-wal", "database-shm"] {
        let _ = std::fs::remove_file(path.with_extension(ext));
    }
    std::fs::rename(&tmp, path)?;
    crate::paths::set_mode(path, 0o600)?;
    Ok(())
}

/// Unlock the DEK with the password-wrapped current service key.
pub fn unlock_with_password(paths: &Paths, password: &SecretString) -> Result<Key32> {
    let env = Envelope::require(paths)?;
    let f = load_service_file(paths, env.service_version)?;
    let w = f.wrapped.ok_or_else(|| Error::Unlock("current service key is retired".into()))?;
    let svc = Key32::from_slice(&password_open(password, &w, &svc_aad(&env.db_id, env.service_version))?)?;
    Key32::from_slice(&open(&svc, &env.service_wrap, DEK_AAD)?)
}

/// Unlock the DEK with the master key.
pub fn unlock_with_master(paths: &Paths, master: &Key32) -> Result<Key32> {
    let env = Envelope::require(paths)?;
    let kek = master_kek(master, &env.db_id)?;
    Key32::from_slice(&open(&kek, &env.master_wrap, DEK_AAD).map_err(|_| Error::Unlock("master key does not match this database".into()))?)
}

/// Rotate: derive service key v{n+1}, re-wrap the DEK, retire the old one.
pub fn rotate(paths: &Paths, master: &Key32, password: &SecretString) -> Result<u32> {
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
pub fn rekey(paths: &Paths, master: &Key32) -> Result<()> {
    let mut env = Envelope::require(paths)?;
    let old = unlock_with_master(paths, master)?;
    let new = Key32::random()?;
    {
        let store = Store::open(&paths.db, Some(&old.0))?;
        store.conn.query_row("PRAGMA journal_mode = DELETE", [], |_| Ok(()))?;
        store.rekey(&new.0)?;
    }
    // Verify before committing the new envelope.
    Store::open(&paths.db, Some(&new.0))?;
    let svc = service_key(master, &env.db_id, env.service_version)?;
    env.service_wrap = seal(&svc, &new.0, DEK_AAD)?;
    env.master_wrap = seal(&master_kek(master, &env.db_id)?, &new.0, DEK_AAD)?;
    env.save(paths)?;
    Ok(())
}

/// Recover access: verify the master, then re-wrap the current service key with a (new) password.
pub fn recover(paths: &Paths, master: &Key32, new_password: &SecretString) -> Result<()> {
    let env = Envelope::require(paths)?;
    unlock_with_master(paths, master)?;
    let svc = service_key(master, &env.db_id, env.service_version)?;
    write_service_file(paths, &env.db_id, env.service_version, &svc, new_password)
}

/// Verify a master key matches this database.
pub fn verify_master(paths: &Paths, master: &Key32) -> Result<()> {
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
pub fn export(paths: &Paths, master: &Key32, password: &SecretString, out: &Path, items: &[ExportItem]) -> Result<Vec<std::path::PathBuf>> {
    verify_master(paths, master)?;
    crate::paths::ensure_dir(out)?;
    let mut written = Vec::new();
    let mut put = |name: &str, data: &[u8]| -> Result<()> {
        let p = out.join(name);
        write_private(&p, &age_encrypt(password, data)?)?;
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

    fn setup() -> (tempfile::TempDir, Paths) {
        let t = tempfile::tempdir().unwrap();
        let p = Paths::resolve(Some(t.path().join("h")), None).unwrap();
        p.ensure().unwrap();
        (t, p)
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
