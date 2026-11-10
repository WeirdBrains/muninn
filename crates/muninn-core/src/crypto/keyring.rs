//! Envelope encryption for the database (`keys/envelope.toml` + `keys/service/v{n}.toml`).
//!
//! The key formats and operations (wrap, unwrap, rotate, rekey, recovery, export) live in
//! `weirdnet_keys::keyring`. Muninn implements its two traits on `crate::paths::Paths`
//! (Muninn's folder layout and [`MUNINN_PROFILE`](super::MUNINN_PROFILE) labels over the
//! `weirdnet-storage` engine) and re-exports the operations here under their old names.
//! Storage only ever receives the raw DEK to apply, and opaque already-encrypted bytes to
//! write.
use crate::error::{Error, Result};

pub use weirdnet_keys::keyring::{
    EncryptedDatabase, Envelope, ExportItem, InitOutput, KeyFiles, ServiceKeyFile, export, init, load_service_file, recover, recover_master, rekey,
    rotate, service_files, unlock_with_master, unlock_with_password, verify_master,
};

/// The associated data the DEK is wrapped under (`muninn/dek`).
#[cfg(test)]
pub(crate) fn dek_aad() -> Vec<u8> {
    super::MUNINN_PROFILE.dek_aad()
}

/// The associated data a service key is password-wrapped under.
#[cfg(test)]
pub(crate) fn svc_aad(db_id: &str, v: u32) -> Vec<u8> {
    super::MUNINN_PROFILE.service_key_aad(db_id, v)
}

/// Read a key file that may be age-encrypted (asking for the password only if it is).
pub fn read_maybe_age(path: &std::path::Path, password: impl FnOnce() -> Result<secrecy::SecretString>) -> Result<zeroize::Zeroizing<String>> {
    weirdnet_keys::keyring::read_maybe_age::<Error>(path, password)
}

/// An export item named on the command line (`--only`).
pub fn parse_export_item(s: &str) -> Result<ExportItem> {
    ExportItem::from_name(s).ok_or_else(|| Error::invalid("--only", format!("`{}` (master, words, service, shards)", s.trim().to_ascii_lowercase())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::{Key32, combine_shards, from_words};
    use crate::model::{Bar, Duration, Symbol};
    use crate::paths::Paths;
    use crate::store::Store;
    use secrecy::SecretString;
    use std::path::{Path, PathBuf};

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
            type Error = Error;
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

    /// The rekey undo at the real storage boundary: SQLCipher on a synthetic database, with
    /// only the envelope write made to fail.
    #[test]
    fn a_rekey_whose_envelope_cannot_be_saved_is_rolled_back_on_the_real_database() {
        struct FailEnvelope<'a>(&'a Paths);
        impl KeyFiles for FailEnvelope<'_> {
            type Error = Error;
            fn profile(&self) -> weirdnet_keys::Profile {
                self.0.profile()
            }
            fn home(&self) -> PathBuf {
                KeyFiles::home(self.0)
            }
            fn envelope(&self) -> PathBuf {
                KeyFiles::envelope(self.0)
            }
            fn service_keys_dir(&self) -> PathBuf {
                KeyFiles::service_keys_dir(self.0)
            }
            fn prepare(&self) -> Result<()> {
                self.0.prepare()
            }
            fn ensure_dir(&self, d: &Path) -> Result<()> {
                KeyFiles::ensure_dir(self.0, d)
            }
            fn write_private(&self, p: &Path, b: &[u8]) -> Result<()> {
                if p == KeyFiles::envelope(self.0) {
                    return Err(Error::Io(std::io::Error::other("synthetic: envelope write refused")));
                }
                KeyFiles::write_private(self.0, p, b)
            }
        }
        impl EncryptedDatabase for FailEnvelope<'_> {
            fn database_exists(&self) -> bool {
                self.0.database_exists()
            }
            fn encrypt_plaintext(&self, k: &Key32) -> Result<()> {
                self.0.encrypt_plaintext(k)
            }
            fn open_keyed(&self, k: &Key32) -> Result<()> {
                self.0.open_keyed(k)
            }
            fn rekey_database(&self, o: &Key32, n: &Key32) -> Result<()> {
                self.0.rekey_database(o, n)
            }
            fn remove_database(&self) {
                self.0.remove_database()
            }
        }
        let (_t, paths) = setup();
        let pw = SecretString::from("pw");
        let out = init(&paths, &pw).unwrap();
        let master = from_words(&out.words).unwrap();
        let dek = unlock_with_password(&paths, &pw).unwrap();
        {
            let mut s = Store::open(&paths.db, Some(&dek.0)).unwrap();
            let src = s.add_source("x", "file", None).unwrap();
            let b = Bar { ts: 0, open: 1.0, high: 1.0, low: 1.0, close: 1.0, volume: 1.0, flags: 0 };
            s.upsert_bars(&src, &Symbol::new("BTC", "USD"), Duration(60), &[b]).unwrap();
        }
        let env_before = std::fs::read(KeyFiles::envelope(&paths)).unwrap();
        let e = rekey(&FailEnvelope(&paths), &master).unwrap_err();
        assert!(e.to_string().contains("envelope write refused"), "{e}");
        assert_eq!(std::fs::read(KeyFiles::envelope(&paths)).unwrap(), env_before, "the envelope is unchanged");
        assert_eq!(unlock_with_password(&paths, &pw).unwrap(), dek);
        assert_eq!(Store::open(&paths.db, Some(&dek.0)).unwrap().count_bars().unwrap(), 1, "the database is back on the stored DEK");
        // and a normal rekey still works afterwards
        rekey(&paths, &master).unwrap();
        let dek2 = unlock_with_password(&paths, &pw).unwrap();
        assert_ne!(dek2, dek);
        assert_eq!(Store::open(&paths.db, Some(&dek2.0)).unwrap().count_bars().unwrap(), 1);
    }
}
