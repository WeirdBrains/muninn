use super::*;
use crate::words::from_words;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

/// An in-memory home: files in a map, a "database" that only remembers its key. Writes can
/// be made to fail by path suffix to exercise every failure path.
struct Mem {
    profile: Profile,
    files: RefCell<BTreeMap<PathBuf, Vec<u8>>>,
    db: RefCell<Option<[u8; 32]>>,
    plain_db: Cell<bool>,
    fail_write: RefCell<Option<String>>,
    fail_rekey_back: Cell<bool>,
    calls: RefCell<Vec<&'static str>>,
}

#[derive(Debug)]
enum AppError {
    Keys(KeysError),
    #[allow(dead_code)] // read through Debug in failure messages
    Disk(&'static str),
}

impl From<KeysError> for AppError {
    fn from(e: KeysError) -> Self {
        AppError::Keys(e)
    }
}

impl Mem {
    fn new(profile: Profile) -> Self {
        Mem {
            profile,
            files: Default::default(),
            db: Default::default(),
            plain_db: Cell::new(false),
            fail_write: Default::default(),
            fail_rekey_back: Cell::new(false),
            calls: Default::default(),
        }
    }
    fn fail_on(&self, suffix: &str) {
        *self.fail_write.borrow_mut() = Some(suffix.into());
    }
    fn heal(&self) {
        *self.fail_write.borrow_mut() = None;
    }
}

impl KeyFiles for Mem {
    type Error = AppError;
    fn profile(&self) -> Profile {
        self.profile
    }
    fn home(&self) -> PathBuf {
        "/mem".into()
    }
    fn envelope(&self) -> PathBuf {
        "/mem/keys/envelope.toml".into()
    }
    fn service_keys_dir(&self) -> PathBuf {
        "/mem/keys/service".into()
    }
    fn prepare(&self) -> Result<(), AppError> {
        Ok(())
    }
    fn ensure_dir(&self, _: &Path) -> Result<(), AppError> {
        Ok(())
    }
    fn write_private(&self, p: &Path, b: &[u8]) -> Result<(), AppError> {
        if self.fail_write.borrow().as_deref().is_some_and(|s| p.to_string_lossy().ends_with(s)) {
            return Err(AppError::Disk("disk full"));
        }
        self.files.borrow_mut().insert(p.to_path_buf(), b.to_vec());
        Ok(())
    }
    fn read(&self, p: &Path) -> Result<Option<Vec<u8>>, AppError> {
        Ok(self.files.borrow().get(p).cloned())
    }
    fn list(&self, d: &Path) -> Result<Vec<PathBuf>, AppError> {
        Ok(self.files.borrow().keys().filter(|p| p.parent() == Some(d)).cloned().collect())
    }
    fn remove(&self, p: &Path) -> Result<(), AppError> {
        self.files.borrow_mut().remove(p);
        Ok(())
    }
}

impl EncryptedDatabase for Mem {
    fn database_exists(&self) -> bool {
        self.plain_db.get() || self.db.borrow().is_some()
    }
    fn encrypt_plaintext(&self, dek: &Key32) -> Result<(), AppError> {
        self.calls.borrow_mut().push("encrypt");
        assert!(self.plain_db.replace(false));
        *self.db.borrow_mut() = Some(dek.0);
        Ok(())
    }
    fn open_keyed(&self, dek: &Key32) -> Result<(), AppError> {
        self.calls.borrow_mut().push("open");
        let mut db = self.db.borrow_mut();
        match *db {
            None => *db = Some(dek.0),
            Some(k) if k == dek.0 => {}
            Some(_) => return Err(AppError::Disk("wrong key")),
        }
        Ok(())
    }
    fn rekey_database(&self, old: &Key32, new: &Key32) -> Result<(), AppError> {
        self.calls.borrow_mut().push("rekey");
        if self.fail_rekey_back.get() && self.calls.borrow().iter().filter(|c| **c == "rekey").count() > 1 {
            return Err(AppError::Disk("rekey failed"));
        }
        let mut db = self.db.borrow_mut();
        if *db != Some(old.0) {
            return Err(AppError::Disk("wrong key"));
        }
        *db = Some(new.0);
        Ok(())
    }
    fn remove_database(&self) {
        self.calls.borrow_mut().push("remove");
        *self.db.borrow_mut() = None;
    }
}

/// A synthetic pre-profile application: format 1, no recorded profile, its own labels.
const LEGACY_APP: Profile = Profile {
    name: None,
    envelope_format: 1,
    label: "legacyapp",
    shard_prefix: "legacyapp-shard-v1",
    claim_salt: b"legacyapp/claim/v1",
    dark_shard_label: "legacyapp/dark/v1",
};

fn pw(s: &str) -> SecretString {
    SecretString::from(s)
}

fn texts(v: &[Zeroizing<String>]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn lifecycle_through_the_traits_only() {
    for profile in [Profile::WEIRDNET, LEGACY_APP] {
        let m = Mem::new(profile);
        let out = init(&m, &pw("pw1")).unwrap();
        assert!(!out.migrated_plain_db);
        let dek = unlock_with_password(&m, &pw("pw1")).unwrap();
        assert_eq!(*m.db.borrow(), Some(dek.0), "the database is keyed with the DEK the envelope wraps");
        let master = from_words(&out.words).unwrap();
        assert_eq!(unlock_with_master(&m, &master).unwrap(), dek);
        assert!(matches!(unlock_with_password(&m, &pw("bad")), Err(AppError::Keys(KeysError::Unlock(_)))));
        assert!(verify_master(&m, &Key32::random().unwrap()).is_err());

        assert_eq!(rotate(&m, &master, &pw("pw1")).unwrap(), 2);
        assert_eq!(unlock_with_password(&m, &pw("pw1")).unwrap(), dek);
        assert!(load_service_file(&m, 1).unwrap().retired && load_service_file(&m, 1).unwrap().wrapped.is_none());
        assert_eq!(service_files(&m).unwrap().iter().map(|f| f.version).collect::<Vec<_>>(), [1, 2]);

        rekey(&m, &master).unwrap();
        let dek2 = unlock_with_password(&m, &pw("pw1")).unwrap();
        assert_ne!(dek2, dek);
        assert_eq!(*m.db.borrow(), Some(dek2.0));

        let sh = texts(&out.shards);
        assert!(recover_master(&m, &sh[..2]).is_err(), "two of a 3-of-5 set recover nothing");
        let rm = recover_master(&m, &[sh[4].clone(), sh[0].clone(), sh[2].clone()]).unwrap();
        recover(&m, &rm, &pw("pw2")).unwrap();
        assert!(unlock_with_password(&m, &pw("pw1")).is_err());
        assert_eq!(unlock_with_password(&m, &pw("pw2")).unwrap(), dek2);
        assert!(matches!(init(&m, &pw("x")), Err(AppError::Keys(KeysError::Invalid { .. }))));

        // nothing stored holds the master, its words or a shard
        let master_hex = master.hex();
        for b in m.files.borrow().values() {
            let s = String::from_utf8_lossy(b);
            assert!(!s.contains(master_hex.as_str()) && !s.contains(out.words.as_str()) && !s.contains(profile.shard_prefix));
        }
    }
}

#[test]
fn plaintext_database_is_encrypted_in_place() {
    let m = Mem::new(Profile::WEIRDNET);
    m.plain_db.set(true);
    let out = init(&m, &pw("pw")).unwrap();
    assert!(out.migrated_plain_db);
    assert_eq!(*m.calls.borrow(), ["encrypt", "open"]);
}

#[test]
fn a_failed_init_leaves_nothing_and_a_retry_works() {
    let m = Mem::new(Profile::WEIRDNET);
    assert!(init(&m, &pw("")).is_err());
    assert!(m.files.borrow().is_empty() && m.db.borrow().is_none() && m.calls.borrow().is_empty());
    m.fail_on("envelope.toml");
    assert!(matches!(init(&m, &pw("pw")), Err(AppError::Disk(_))));
    assert!(m.files.borrow().is_empty(), "the v1 service file is removed");
    assert!(m.db.borrow().is_none(), "the database it created is removed");
    m.heal();
    init(&m, &pw("pw")).unwrap();
    assert!(unlock_with_password(&m, &pw("pw")).is_ok());
}

#[test]
fn a_rekey_that_cannot_save_is_undone() {
    let m = Mem::new(Profile::WEIRDNET);
    let out = init(&m, &pw("pw")).unwrap();
    let master = from_words(&out.words).unwrap();
    let dek = unlock_with_password(&m, &pw("pw")).unwrap();
    let before = m.files.borrow().clone();
    m.fail_on("envelope.toml");
    assert!(matches!(rekey(&m, &master), Err(AppError::Disk(_))));
    assert_eq!(*m.db.borrow(), Some(dek.0), "the database is back on the stored DEK");
    assert_eq!(*m.files.borrow(), before);
    m.heal();
    assert_eq!(unlock_with_password(&m, &pw("pw")).unwrap(), dek);
    // when the undo fails too, the error says so plainly
    m.fail_on("envelope.toml");
    m.fail_rekey_back.set(true);
    m.calls.borrow_mut().clear();
    let e = rekey(&m, &master).unwrap_err();
    assert!(matches!(&e, AppError::Keys(KeysError::Crypto(s)) if s.contains("restore it from a backup")), "{e:?}");
}

#[test]
fn a_rotation_that_cannot_save_keeps_the_old_version() {
    let m = Mem::new(Profile::WEIRDNET);
    let out = init(&m, &pw("pw")).unwrap();
    let master = from_words(&out.words).unwrap();
    m.fail_on("envelope.toml");
    assert!(rotate(&m, &master, &pw("pw")).is_err());
    m.heal();
    assert_eq!(Envelope::require(&m).unwrap().service_version, 1);
    assert!(unlock_with_password(&m, &pw("pw")).is_ok());
}

#[test]
fn tampered_files_are_rejected() {
    let m = Mem::new(Profile::WEIRDNET);
    init(&m, &pw("pw")).unwrap();
    let env_path = m.envelope();
    let mut env = Envelope::require(&m).unwrap();
    let good = env.clone();
    let mut ct = hex::decode(&env.service_wrap.ct).unwrap();
    ct[3] ^= 0x40;
    env.service_wrap.ct = hex::encode(ct);
    env.save(&m).unwrap();
    assert!(matches!(unlock_with_password(&m, &pw("pw")), Err(AppError::Keys(KeysError::Unlock(_)))));
    // an envelope moved to another db id no longer opens
    Envelope { db_id: "another".into(), ..good.clone() }.save(&m).unwrap();
    assert!(unlock_with_password(&m, &pw("pw")).is_err());
    good.save(&m).unwrap();
    // a damaged service file
    let svc = service_file(&m, 1);
    let mut f = load_service_file(&m, 1).unwrap();
    let w = f.wrapped.as_mut().unwrap();
    w.sealed.ct.replace_range(0..2, if &w.sealed.ct[0..2] == "00" { "01" } else { "00" });
    m.write_private(&svc, toml::to_string_pretty(&f).unwrap().as_bytes()).unwrap();
    assert!(matches!(unlock_with_password(&m, &pw("pw")), Err(AppError::Keys(KeysError::Unlock(_)))));
    m.files.borrow_mut().insert(env_path, b"not toml [".to_vec());
    assert!(matches!(Envelope::load(&m), Err(AppError::Keys(KeysError::TomlDe(_)))));
}

#[test]
fn profiles_are_separate_domains() {
    let w = Mem::new(Profile::WEIRDNET);
    let out = init(&w, &pw("pw")).unwrap();
    let master = from_words(&out.words).unwrap();
    let env = String::from_utf8(w.files.borrow()[&w.envelope()].clone()).unwrap();
    assert!(env.starts_with("format = 2\nprofile = \"weirdnet/keys/v1\"\n"), "{env}");

    // The legacy application refuses the Weirdnet home outright, changing nothing.
    let l = Mem::new(LEGACY_APP);
    *l.files.borrow_mut() = w.files.borrow().clone();
    let before = l.files.borrow().clone();
    assert!(matches!(unlock_with_password(&l, &pw("pw")), Err(AppError::Keys(KeysError::Invalid { .. }))));
    assert!(rotate(&l, &master, &pw("pw")).is_err() && recover(&l, &master, &pw("x")).is_err());
    assert_eq!(*l.files.borrow(), before);
    assert!(recover_master(&l, &texts(&out.shards)).is_err(), "another profile's shards do not parse");

    // Same envelope tags but different labels: every unwrap fails.
    let relabeled = Mem::new(Profile { label: "weirdnet/keys/v1-other", ..Profile::WEIRDNET });
    *relabeled.files.borrow_mut() = w.files.borrow().clone();
    assert!(matches!(unlock_with_password(&relabeled, &pw("pw")), Err(AppError::Keys(KeysError::Unlock(_)))));
    assert!(verify_master(&relabeled, &master).is_err());

    // And the Weirdnet side refuses a legacy home.
    let l2 = Mem::new(LEGACY_APP);
    init(&l2, &pw("pw")).unwrap();
    let w2 = Mem::new(Profile::WEIRDNET);
    *w2.files.borrow_mut() = l2.files.borrow().clone();
    assert!(Envelope::load(&w2).is_err());
}

#[test]
fn legacy_envelopes_stay_byte_identical() {
    // A fixed legacy envelope (synthetic values) survives load and save unchanged, with no
    // profile field added.
    let fixture = "format = 1\ndb_id = \"00000000-0000-4000-8000-000000000000\"\ncreated_at = 1700000000000\nservice_version = 3\n\n[service_wrap]\nnonce = \"000102030405060708090a0b0c0d0e0f1011121314151617\"\nct = \"aa\"\n\n[master_wrap]\nnonce = \"18191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f\"\nct = \"bb\"\n";
    let m = Mem::new(LEGACY_APP);
    m.files.borrow_mut().insert(m.envelope(), fixture.as_bytes().to_vec());
    let env = Envelope::require(&m).unwrap();
    assert_eq!(env.profile, None);
    env.save(&m).unwrap();
    assert_eq!(String::from_utf8(m.files.borrow()[&m.envelope()].clone()).unwrap(), fixture);
    // and the legacy labels are exactly the ones it was written with
    assert_eq!(LEGACY_APP.dek_aad(), b"legacyapp/dek");
    assert_eq!(LEGACY_APP.service_key_aad("db", 2), b"legacyapp/service-key/db/v2");
    assert_eq!(LEGACY_APP.service_info(4), "legacyapp/service/v4");
    assert_eq!(LEGACY_APP.kek_info(), "legacyapp/kek");
}

#[cfg(feature = "age")]
#[test]
fn exports_are_separate_decryptable_and_all_or_nothing() {
    let t = tempfile::tempdir().unwrap();
    struct Disk(PathBuf);
    impl KeyFiles for Disk {
        type Error = KeysError;
        fn home(&self) -> PathBuf {
            self.0.clone()
        }
        fn envelope(&self) -> PathBuf {
            self.0.join("envelope.toml")
        }
        fn service_keys_dir(&self) -> PathBuf {
            self.0.join("service")
        }
        fn prepare(&self) -> crate::Result<()> {
            Ok(std::fs::create_dir_all(self.service_keys_dir())?)
        }
        fn ensure_dir(&self, d: &Path) -> crate::Result<()> {
            Ok(std::fs::create_dir_all(d)?)
        }
        fn write_private(&self, p: &Path, b: &[u8]) -> crate::Result<()> {
            if p.file_name().is_some_and(|n| n == "shard-4.age") && self.0.join("fail").exists() {
                return Err(KeysError::crypto("disk full"));
            }
            Ok(std::fs::write(p, b)?)
        }
    }
    impl EncryptedDatabase for Disk {
        fn database_exists(&self) -> bool {
            false
        }
        fn encrypt_plaintext(&self, _: &Key32) -> crate::Result<()> {
            unreachable!()
        }
        fn open_keyed(&self, _: &Key32) -> crate::Result<()> {
            Ok(())
        }
        fn rekey_database(&self, _: &Key32, _: &Key32) -> crate::Result<()> {
            Ok(())
        }
        fn remove_database(&self) {}
    }
    let d = Disk(t.path().join("k"));
    let p = pw("pw");
    let out = init(&d, &p).unwrap();
    let master = from_words(&out.words).unwrap();
    let dir = t.path().join("exp");
    let files = export(&d, &master, &p, &dir, &ExportItem::ALL).unwrap();
    assert_eq!(files.len(), 8);
    let m = read_maybe_age::<KeysError>(&dir.join("master.key.age"), || Ok(p.clone())).unwrap();
    assert_eq!(Key32::from_hex(&m).unwrap(), master);
    let sh: Vec<String> =
        (2..=4).map(|i| read_maybe_age::<KeysError>(&dir.join(format!("shard-{i}.age")), || Ok(p.clone())).unwrap().to_string()).collect();
    assert_eq!(recover_master(&d, &sh).unwrap(), master);
    assert!(matches!(read_maybe_age::<KeysError>(&dir.join("words.txt.age"), || Ok(pw("wrong"))), Err(KeysError::Unlock(_))));
    let svc = read_maybe_age::<KeysError>(&dir.join("service-keys.json.age"), || Ok(p.clone())).unwrap();
    assert!(svc.contains("\"envelope\"") && svc.contains("\"service_keys\""));
    assert!(export(&d, &Key32::random().unwrap(), &p, &t.path().join("x"), &ExportItem::ALL).is_err());

    std::fs::write(t.path().join("k").join("fail"), b"").unwrap();
    let dir2 = t.path().join("exp2");
    assert!(export(&d, &master, &p, &dir2, &ExportItem::ALL).is_err());
    assert_eq!(std::fs::read_dir(&dir2).unwrap().count(), 0, "a failed export leaves no files");
    // files that were already in the folder survive a failed export (unrelated ones untouched,
    // same-named ones replaced but not removed)
    let dir3 = t.path().join("exp3");
    std::fs::create_dir_all(&dir3).unwrap();
    std::fs::write(dir3.join("notes.txt"), b"mine").unwrap();
    std::fs::write(dir3.join("master.key.age"), b"older export").unwrap();
    assert!(export(&d, &master, &p, &dir3, &ExportItem::ALL).is_err());
    let mut left: Vec<String> = std::fs::read_dir(&dir3).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    left.sort();
    assert_eq!(left, ["master.key.age", "notes.txt"]);
    assert_eq!(std::fs::read(dir3.join("notes.txt")).unwrap(), b"mine");
    assert_eq!(ExportItem::from_name(" Service-Keys "), Some(ExportItem::Service));
    assert_eq!(ExportItem::from_name("x"), None);
}
