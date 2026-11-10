//! Envelope encryption for a database key (`envelope.toml` + `service/v{n}.toml`).
//!
//! A random data key (DEK) keys the database. The DEK is stored wrapped twice: by the
//! current service key and by a master-derived KEK. The service key is stored wrapped by
//! the user's password (Argon2id). The master key, words and shards are never stored.
//!
//! This module owns every key format and operation: wrap, unwrap, rotate, rekey, recovery,
//! verification and export. It does not know the application's folders or its database:
//! both come in through [`KeyFiles`] and [`EncryptedDatabase`], which the application
//! implements. The database side only ever receives the raw DEK to apply, and the file side
//! only already-encrypted (or public) bytes.
use crate::aead::{Sealed, open, seal};
use crate::error::KeysError;
use crate::kdf::{master_kek, service_key};
use crate::key::Key32;
use crate::password::{PasswordSealed, password_open, password_seal};
use crate::profile::Profile;
use crate::shards;
use crate::words::to_words;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

/// Where the key files live, which labels they use, and how they are read and written.
/// The read, list and remove defaults use the local filesystem.
pub trait KeyFiles {
    /// The application's error; key errors convert into it.
    type Error: From<KeysError>;

    /// The labels this home's keys use. New data defaults to [`Profile::WEIRDNET`]; an
    /// application with existing data returns its own profile.
    fn profile(&self) -> Profile {
        Profile::WEIRDNET
    }
    /// The home shown in the not-initialized error.
    fn home(&self) -> PathBuf;
    fn envelope(&self) -> PathBuf;
    fn service_keys_dir(&self) -> PathBuf;
    /// Create the folders a first `init` needs.
    fn prepare(&self) -> Result<(), Self::Error>;
    fn ensure_dir(&self, dir: &Path) -> Result<(), Self::Error>;
    /// Write already-encrypted (or public) bytes with private permissions, atomically.
    fn write_private(&self, path: &Path, bytes: &[u8]) -> Result<(), Self::Error>;
    /// The file's bytes, or `None` when it does not exist.
    fn read(&self, path: &Path) -> Result<Option<Vec<u8>>, Self::Error> {
        match std::fs::read(path) {
            Ok(b) => Ok(Some(b)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(KeysError::Io(e).into()),
        }
    }
    /// The files directly in `dir` (none when it does not exist).
    fn list(&self, dir: &Path) -> Result<Vec<PathBuf>, Self::Error> {
        match std::fs::read_dir(dir) {
            Ok(rd) => Ok(rd.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect()),
            Err(_) => Ok(vec![]),
        }
    }
    /// Remove a file (a missing one is fine).
    fn remove(&self, path: &Path) -> Result<(), Self::Error> {
        match std::fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(KeysError::Io(e).into()),
            _ => Ok(()),
        }
    }
}

/// The database the DEK keys. The keyring decides when to create, convert or rekey it; the
/// implementation (a storage engine) does the work with the raw key it is handed.
pub trait EncryptedDatabase: KeyFiles {
    fn database_exists(&self) -> bool;
    /// Encrypt an existing plaintext database in place.
    fn encrypt_plaintext(&self, dek: &Key32) -> Result<(), Self::Error>;
    /// Open with the key, creating it if missing; fails on a wrong key.
    fn open_keyed(&self, dek: &Key32) -> Result<(), Self::Error>;
    /// Re-encrypt from `old` to `new`.
    fn rekey_database(&self, old: &Key32, new: &Key32) -> Result<(), Self::Error>;
    /// Delete a database a failed `init` created.
    fn remove_database(&self);
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Envelope {
    pub format: u32,
    /// The profile that wrote it (absent in envelopes from before profiles were recorded).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
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

fn utf8<E: From<KeysError>>(b: &[u8]) -> Result<&str, E> {
    std::str::from_utf8(b).map_err(|_| KeysError::invalid("key file", "not UTF-8").into())
}

fn toml_of<T: Serialize, E: From<KeysError>>(v: &T) -> Result<String, E> {
    toml::to_string_pretty(v).map_err(|e| KeysError::TomlSer(e).into())
}

fn from_toml<T: serde::de::DeserializeOwned, E: From<KeysError>>(b: &[u8]) -> Result<T, E> {
    toml::from_str(utf8::<E>(b)?).map_err(|e| KeysError::TomlDe(e).into())
}

impl Envelope {
    /// The envelope, if this home has one. One written under another profile is refused.
    pub fn load<P: KeyFiles + ?Sized>(files: &P) -> Result<Option<Self>, P::Error> {
        let Some(b) = files.read(&files.envelope())? else { return Ok(None) };
        let env: Envelope = from_toml(&b)?;
        let p = files.profile();
        if env.format != p.envelope_format || env.profile.as_deref() != p.name {
            return Err(KeysError::invalid(
                "keys",
                format!(
                    "the envelope is format {} ({}), but this application reads format {} ({}); nothing was changed",
                    env.format,
                    env.profile.as_deref().unwrap_or("no profile"),
                    p.envelope_format,
                    p.name.unwrap_or("no profile")
                ),
            )
            .into());
        }
        Ok(Some(env))
    }

    pub fn require<P: KeyFiles + ?Sized>(files: &P) -> Result<Self, P::Error> {
        Self::load(files)?.ok_or_else(|| KeysError::NotInitialized(files.home()).into())
    }

    pub fn save<P: KeyFiles + ?Sized>(&self, files: &P) -> Result<(), P::Error> {
        files.write_private(&files.envelope(), toml_of::<_, P::Error>(self)?.as_bytes())
    }
}

fn service_file<P: KeyFiles + ?Sized>(files: &P, v: u32) -> PathBuf {
    files.service_keys_dir().join(format!("v{v}.toml"))
}

pub fn load_service_file<P: KeyFiles + ?Sized>(files: &P, v: u32) -> Result<ServiceKeyFile, P::Error> {
    let p = service_file(files, v);
    let b = files.read(&p)?.ok_or_else(|| KeysError::Unlock(format!("service key v{v} is missing ({})", p.display())))?;
    from_toml(&b)
}

pub fn service_files<P: KeyFiles + ?Sized>(files: &P) -> Result<Vec<ServiceKeyFile>, P::Error> {
    let mut v = Vec::new();
    for p in files.list(&files.service_keys_dir())? {
        if p.extension().is_some_and(|x| x == "toml")
            && let Some(b) = files.read(&p)?
        {
            v.push(from_toml::<ServiceKeyFile, P::Error>(&b)?);
        }
    }
    v.sort_by_key(|f| f.version);
    Ok(v)
}

fn write_service_file<P: KeyFiles + ?Sized>(files: &P, db_id: &str, v: u32, key: &Key32, password: &SecretString) -> Result<(), P::Error> {
    let aad = files.profile().service_key_aad(db_id, v);
    let f = ServiceKeyFile { version: v, created_at: now_ms(), retired: false, wrapped: Some(password_seal(password, &key.0, &aad)?) };
    files.write_private(&service_file(files, v), toml_of::<_, P::Error>(&f)?.as_bytes())
}

fn retire_service_file<P: KeyFiles + ?Sized>(files: &P, v: u32) -> Result<(), P::Error> {
    if let Ok(mut f) = load_service_file(files, v) {
        f.retired = true;
        f.wrapped = None;
        files.write_private(&service_file(files, v), toml_of::<_, P::Error>(&f)?.as_bytes())?;
    }
    Ok(())
}

/// Result of `init`. Show once, then drop.
pub struct InitOutput {
    pub db_id: String,
    pub words: Zeroizing<String>,
    pub shards: Vec<Zeroizing<String>>,
    pub migrated_plain_db: bool,
}

/// Create master key, DEK, envelope and service key; encrypt (or create) the database.
pub fn init<P: EncryptedDatabase + ?Sized>(files: &P, password: &SecretString) -> Result<InitOutput, P::Error> {
    if files.read(&files.envelope())?.is_some() {
        return Err(KeysError::invalid("keys", format!("already initialized ({})", files.envelope().display())).into());
    }
    // Check the password before touching disk: a failed first run must leave nothing behind
    // that a retry would trip over (a fresh encrypted database without an envelope).
    if password.expose_secret().is_empty() {
        return Err(KeysError::invalid("password", "must not be empty").into());
    }
    let profile = files.profile();
    files.prepare()?;
    let master = Key32::random()?;
    let dek = Key32::random()?;
    let db_id = uuid::Uuid::new_v4().to_string();
    let svc = service_key(&profile, &master, &db_id, 1)?;
    let kek = master_kek(&profile, &master, &db_id)?;

    // Encrypt an existing plaintext database in place, or create a new encrypted one.
    let mut migrated = false;
    if files.database_exists() {
        files.encrypt_plaintext(&dek)?;
        migrated = true;
    }
    files.open_keyed(&dek)?;

    let finish = || -> Result<(), P::Error> {
        write_service_file(files, &db_id, 1, &svc, password)?;
        let env = Envelope {
            format: profile.envelope_format,
            profile: profile.name.map(str::to_string),
            db_id: db_id.clone(),
            created_at: now_ms(),
            service_version: 1,
            service_wrap: seal(&svc, &dek.0, &profile.dek_aad())?,
            master_wrap: seal(&kek, &dek.0, &profile.dek_aad())?,
        };
        env.save(files)
    };
    if let Err(e) = finish() {
        // Undo a database this call created; its key is about to be lost.
        if !migrated {
            files.remove_database();
        }
        let _ = files.remove(&service_file(files, 1));
        return Err(e);
    }
    Ok(InitOutput {
        db_id,
        words: to_words(&master)?,
        shards: shards::split(&profile, &master, shards::THRESHOLD, shards::COUNT)?,
        migrated_plain_db: migrated,
    })
}

/// Unlock the DEK with the password-wrapped current service key.
pub fn unlock_with_password<P: KeyFiles + ?Sized>(files: &P, password: &SecretString) -> Result<Key32, P::Error> {
    let profile = files.profile();
    let env = Envelope::require(files)?;
    let f = load_service_file(files, env.service_version)?;
    let w = f.wrapped.ok_or_else(|| KeysError::Unlock("current service key is retired".into()))?;
    let svc = Key32::from_slice(&password_open(password, &w, &profile.service_key_aad(&env.db_id, env.service_version))?)?;
    Ok(Key32::from_slice(&open(&svc, &env.service_wrap, &profile.dek_aad())?)?)
}

fn dek_from_master(profile: &Profile, env: &Envelope, master: &Key32) -> crate::Result<Key32> {
    let kek = master_kek(profile, master, &env.db_id)?;
    Key32::from_slice(
        &open(&kek, &env.master_wrap, &profile.dek_aad()).map_err(|_| KeysError::Unlock("master key does not match this database".into()))?,
    )
}

/// Unlock the DEK with the master key.
pub fn unlock_with_master<P: KeyFiles + ?Sized>(files: &P, master: &Key32) -> Result<Key32, P::Error> {
    let env = Envelope::require(files)?;
    Ok(dek_from_master(&files.profile(), &env, master)?)
}

/// Rotate: derive service key v{n+1}, re-wrap the DEK, retire the old one. The new service
/// file is written before the envelope points at it, so a failure leaves the old version in
/// force (and at worst an unused new file).
pub fn rotate<P: KeyFiles + ?Sized>(files: &P, master: &Key32, password: &SecretString) -> Result<u32, P::Error> {
    let profile = files.profile();
    let mut env = Envelope::require(files)?;
    let dek = dek_from_master(&profile, &env, master)?;
    let old = env.service_version;
    let new = old.checked_add(1).ok_or_else(|| KeysError::invalid("keys", "no service key versions left"))?;
    let svc = service_key(&profile, master, &env.db_id, new)?;
    write_service_file(files, &env.db_id, new, &svc, password)?;
    env.service_version = new;
    env.service_wrap = seal(&svc, &dek.0, &profile.dek_aad())?;
    env.save(files)?;
    retire_service_file(files, old)?;
    Ok(new)
}

/// Full rekey: new DEK, re-encrypt the database, verify, then re-wrap by service key and
/// master KEK. If the new envelope cannot be saved the database is rekeyed back to the old
/// DEK, so the stored envelope keeps matching it.
pub fn rekey<P: EncryptedDatabase + ?Sized>(files: &P, master: &Key32) -> Result<(), P::Error> {
    let profile = files.profile();
    let mut env = Envelope::require(files)?;
    let old = dek_from_master(&profile, &env, master)?;
    let new = Key32::random()?;
    let svc = service_key(&profile, master, &env.db_id, env.service_version)?;
    let kek = master_kek(&profile, master, &env.db_id)?;
    env.service_wrap = seal(&svc, &new.0, &profile.dek_aad())?;
    env.master_wrap = seal(&kek, &new.0, &profile.dek_aad())?;
    files.rekey_database(&old, &new)?;
    let commit = || -> Result<(), P::Error> {
        files.open_keyed(&new)?;
        env.save(files)
    };
    if let Err(e) = commit() {
        if files.rekey_database(&new, &old).is_err() {
            return Err(KeysError::crypto(
                "rekey failed after the database was re-encrypted, and undoing it failed too; the database is keyed with a key that is no longer stored — restore it from a backup",
            )
            .into());
        }
        return Err(e);
    }
    Ok(())
}

/// Recover access: verify the master, then re-wrap the current service key with a (new) password.
pub fn recover<P: KeyFiles + ?Sized>(files: &P, master: &Key32, new_password: &SecretString) -> Result<(), P::Error> {
    let profile = files.profile();
    let env = Envelope::require(files)?;
    dek_from_master(&profile, &env, master)?;
    let svc = service_key(&profile, master, &env.db_id, env.service_version)?;
    write_service_file(files, &env.db_id, env.service_version, &svc, new_password)
}

/// Verify a master key matches this database (its KEK unwraps the stored DEK).
pub fn verify_master<P: KeyFiles + ?Sized>(files: &P, master: &Key32) -> Result<(), P::Error> {
    unlock_with_master(files, master).map(|_| ())
}

/// Combine shards made under this home's profile and verify the result against it.
pub fn recover_master<P: KeyFiles + ?Sized>(files: &P, texts: &[String]) -> Result<Key32, P::Error> {
    let m = shards::combine(&files.profile(), texts)?;
    verify_master(files, &m)?;
    Ok(m)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportItem {
    Master,
    Words,
    Service,
    Shards,
}

impl ExportItem {
    pub const ALL: [ExportItem; 4] = [ExportItem::Master, ExportItem::Words, ExportItem::Service, ExportItem::Shards];

    /// `master`, `words`, `service` (or `service-keys`) or `shards`, in any case.
    pub fn from_name(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "master" => Some(Self::Master),
            "words" => Some(Self::Words),
            "service" | "service-keys" => Some(Self::Service),
            "shards" => Some(Self::Shards),
            _ => None,
        }
    }
}

/// Write each item as its own age (password) file and return the written paths. An
/// existing file of the same name is replaced, as before. If any item fails, the files this
/// call created are removed; files that were already there are never removed.
#[cfg(feature = "age")]
pub fn export<P: KeyFiles + ?Sized>(
    files: &P,
    master: &Key32,
    password: &SecretString,
    out: &Path,
    items: &[ExportItem],
) -> Result<Vec<PathBuf>, P::Error> {
    use crate::agefile;
    verify_master(files, master)?;
    files.ensure_dir(out)?;
    let profile = files.profile();
    let mut written = Vec::new();
    let mut run = || -> Result<(), P::Error> {
        let mut put = |name: &str, data: &[u8]| -> Result<(), P::Error> {
            let p = out.join(name);
            let existed = files.read(&p)?.is_some();
            files.write_private(&p, &agefile::encrypt(password, data)?)?;
            written.push((p, existed));
            Ok(())
        };
        for it in items {
            match it {
                ExportItem::Master => put("master.key.age", master.hex().as_bytes())?,
                ExportItem::Words => put("words.txt.age", to_words(master)?.as_bytes())?,
                ExportItem::Service => {
                    let env = Envelope::require(files)?;
                    let j = serde_json::json!({ "envelope": env, "service_keys": service_files(files)? });
                    put("service-keys.json.age", serde_json::to_string_pretty(&j).map_err(KeysError::from)?.as_bytes())?
                }
                ExportItem::Shards => {
                    for (i, s) in shards::split(&profile, master, shards::THRESHOLD, shards::COUNT)?.iter().enumerate() {
                        put(&format!("shard-{}.age", i + 1), s.as_bytes())?;
                    }
                }
            }
        }
        Ok(())
    };
    if let Err(e) = run() {
        for (p, existed) in &written {
            if !existed {
                let _ = files.remove(p);
            }
        }
        return Err(e);
    }
    Ok(written.into_iter().map(|(p, _)| p).collect())
}

/// Read a key file that may be age-encrypted (asking for the password only if it is).
#[cfg(feature = "age")]
pub fn read_maybe_age<E: From<KeysError>>(path: &Path, password: impl FnOnce() -> Result<SecretString, E>) -> Result<Zeroizing<String>, E> {
    use crate::agefile;
    let b = Zeroizing::new(std::fs::read(path).map_err(KeysError::from)?);
    let plain = if agefile::is_age(&b) { agefile::decrypt(&password()?, &b)? } else { Zeroizing::new(b.to_vec()) };
    Ok(Zeroizing::new(String::from_utf8_lossy(&plain).trim().to_string()))
}

#[cfg(test)]
mod tests;
