//! Opening, keying, rekeying, converting, migrating and copying SQLCipher databases.
use crate::error::{Result, StorageError};
use crate::files::{create_private, ensure_dir, set_mode, sync_parent};
use rusqlite::{Connection, OpenFlags, Transaction};
use std::path::Path;
use zeroize::Zeroizing;

/// A raw SQLCipher key (the caller's data-encryption key). Applied as-is, never derived.
pub type RawKey = [u8; 32];

const BUSY: std::time::Duration = std::time::Duration::from_secs(5);
const PLAINTEXT_HEADER: &[u8; 16] = b"SQLite format 3\0";

/// `x'<hex>'`, built in place so no unzeroized copy of the key is left behind.
fn key_literal(k: &RawKey) -> Zeroizing<String> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = Zeroizing::new(String::with_capacity(2 + 64 + 1));
    s.push_str("x'");
    for b in k {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s.push('\'');
    s
}

/// Whether this SQLite is SQLCipher (a key would otherwise be ignored).
pub fn has_cipher(conn: &Connection) -> bool {
    conn.query_row("PRAGMA cipher_version", [], |r| r.get::<_, String>(0)).is_ok()
}

fn require_cipher(conn: &Connection) -> Result<()> {
    if has_cipher(conn) { Ok(()) } else { Err(StorageError::NoCipher) }
}

/// Apply a raw key (`PRAGMA key = "x'…'"`); SQLCipher's own logging stays off stderr.
pub fn apply_key(conn: &Connection, key: Option<&RawKey>) -> Result<()> {
    let _ = conn.query_row("PRAGMA cipher_log_level = NONE", [], |_| Ok(()));
    if let Some(k) = key {
        require_cipher(conn)?;
        conn.pragma_update(None, "key", key_literal(k).as_str())?;
    }
    Ok(())
}

fn check_key(conn: &Connection, what: &str) -> Result<()> {
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0))
        .map(|_| ())
        .map_err(|e| StorageError::WrongKey { what: what.to_string(), detail: e.to_string() })
}

fn is_plaintext_file(path: &Path) -> Result<bool> {
    use std::io::Read;
    let mut head = [0u8; 16];
    let mut f = std::fs::File::open(path)?;
    let n = f.read(&mut head)?;
    Ok(n == 16 && &head == PLAINTEXT_HEADER)
}

/// Re-encrypt an open, keyed database with a new raw key (`PRAGMA rekey`).
///
/// SQLCipher cannot rekey in WAL mode, so the journal is switched to rollback (DELETE) mode
/// first. The rekey runs as one SQLCipher transaction: if it fails the file keeps its old
/// key. SQLCipher can report success without rekeying, so for a file the result is checked
/// on a fresh connection: a plaintext file (rekey cannot encrypt one) is
/// [`StorageError::NotEncrypted`], and a file the new key does not open is
/// [`StorageError::RekeyFailed`].
pub fn rekey(conn: &Connection, new_key: &RawKey) -> Result<()> {
    require_cipher(conn)?;
    conn.query_row("PRAGMA journal_mode = DELETE", [], |_| Ok(()))?;
    conn.pragma_update(None, "rekey", key_literal(new_key).as_str())?;
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0))?;
    if let Some(p) = conn.path().filter(|p| !p.is_empty()) {
        let p = Path::new(p);
        if is_plaintext_file(p)? {
            return Err(StorageError::NotEncrypted);
        }
        let check = Connection::open_with_flags(p, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        apply_key(&check, Some(new_key))?;
        check_key(&check, "rekeyed").map_err(|_| StorageError::RekeyFailed)?;
    }
    Ok(())
}

/// Open (creating if needed, with a private parent folder and a file that is 0600 from the
/// start) and verify the key. `what` names the database in the wrong-key error. A file this
/// call created is removed again if opening fails.
pub fn open(path: &Path, key: Option<&RawKey>, what: &str) -> Result<Connection> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        ensure_dir(parent)?;
    }
    let created = !path.exists();
    if created {
        drop(create_private(path)?);
    }
    let r = (|| {
        let conn = Connection::open(path)?;
        apply_key(&conn, key)?;
        conn.busy_timeout(BUSY)?;
        check_key(&conn, what)?;
        Ok(conn)
    })();
    if r.is_err() && created {
        let _ = std::fs::remove_file(path);
    }
    r
}

/// Open an existing database read-only (no writes, no migrations) and verify the key.
pub fn open_readonly(path: &Path, key: Option<&RawKey>, what: &str) -> Result<Connection> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    apply_key(&conn, key)?;
    conn.busy_timeout(BUSY)?;
    check_key(&conn, what)?;
    Ok(conn)
}

/// A private in-memory database (tests, scratch work).
pub fn open_in_memory() -> Result<Connection> {
    Ok(Connection::open_in_memory()?)
}

/// Open someone else's SQLite file read-only (URIs allowed), with their SQLCipher
/// passphrase if it has one. The passphrase goes to SQLCipher as given; nothing is derived
/// here and the key is not checked.
pub fn open_foreign_readonly(path: &Path, passphrase: Option<&str>) -> Result<Connection> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI)?;
    if let Some(k) = passphrase {
        require_cipher(&conn)?;
        conn.pragma_update(None, "key", k)?;
    }
    Ok(conn)
}

/// Encrypt an existing plaintext SQLite file in place with `key`.
///
/// `sqlcipher_export` writes a sibling file (`path` with the extension `encrypting`, private
/// from creation), which is reopened with the key and checked to hold as many schema entries
/// as the original before it is renamed over it; stale WAL/SHM files are removed. Until the
/// rename the original is untouched, and on any failure the sibling is removed.
pub fn encrypt_plaintext_in_place(path: &Path, key: &RawKey) -> Result<()> {
    let tmp = path.with_extension("encrypting");
    if tmp.exists() {
        std::fs::remove_file(&tmp)?;
    }
    drop(create_private(&tmp)?);
    let r = (|| -> Result<()> {
        let objects = {
            let conn = Connection::open(path)?;
            let _ = conn.query_row("PRAGMA cipher_log_level = NONE", [], |_| Ok(()));
            require_cipher(&conn)?;
            let n: i64 = conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get(0)).map_err(|_| StorageError::NotPlaintext)?;
            let lit = key_literal(key);
            let mut sql = Zeroizing::new(String::with_capacity(48 + lit.len()));
            sql.push_str("ATTACH DATABASE ?1 AS enc KEY \"");
            sql.push_str(&lit);
            sql.push('"');
            conn.execute(sql.as_str(), [tmp.to_string_lossy()])?;
            conn.query_row("SELECT sqlcipher_export('enc')", [], |_| Ok(()))?;
            conn.execute("DETACH DATABASE enc", [])?;
            n
        };
        let enc = Connection::open_with_flags(&tmp, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        apply_key(&enc, Some(key))?;
        let m: i64 = enc.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get(0))?;
        drop(enc);
        if m != objects || is_plaintext_file(&tmp)? {
            return Err(StorageError::invalid("conversion", "the encrypted copy does not match the original"));
        }
        Ok(())
    })();
    if let Err(e) = r {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        for side in ["wal", "shm"] {
            let _ = std::fs::remove_file(path.with_extension(format!("{ext}-{side}")));
        }
    }
    std::fs::rename(&tmp, path)?;
    set_mode(path, 0o600)?;
    sync_parent(path);
    Ok(())
}

/// Apply `migrations[..upto]` not yet recorded in `schema_version`, all in one transaction
/// (any failure leaves the database as it was). `applied(tx, i)` runs right after migration
/// `i` (0-based) and before it is recorded: data upgrades and seed rows go there.
///
/// Every call also sets the pragma policy: foreign keys on, WAL, `synchronous = NORMAL`.
pub fn migrate<E: From<StorageError>>(
    conn: &mut Connection,
    migrations: &[&str],
    upto: usize,
    mut applied: impl FnMut(&Transaction, usize) -> std::result::Result<(), E>,
) -> std::result::Result<(), E> {
    let sql = |r: rusqlite::Result<()>| r.map_err(|e| E::from(StorageError::from(e)));
    sql(conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;"))?;
    sql(conn.execute_batch("CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);"))?;
    let v: i64 =
        conn.query_row("SELECT COALESCE(MAX(version), 0) FROM schema_version", [], |r| r.get(0)).map_err(|e| E::from(StorageError::from(e)))?;
    let tx = conn.transaction().map_err(|e| E::from(StorageError::from(e)))?;
    for (i, m) in migrations.iter().enumerate().take(upto) {
        let target = i as i64 + 1;
        if v < target {
            sql(tx.execute_batch(m))?;
            applied(&tx, i)?;
            tx.execute("INSERT INTO schema_version(version) VALUES (?1)", [target]).map_err(|e| E::from(StorageError::from(e)))?;
        }
    }
    tx.commit().map_err(|e| E::from(StorageError::from(e)))?;
    Ok(())
}

/// The highest applied migration (`schema_version`).
pub fn schema_version(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))?)
}

/// Copy an open database to `dest` with SQLite's online backup, keyed with `key` (an
/// encrypted source stays encrypted). `dest` is replaced, private from creation, and removed
/// again if the copy fails.
pub fn snapshot_to(conn: &Connection, dest: &Path, key: Option<&RawKey>) -> Result<()> {
    if dest.exists() {
        std::fs::remove_file(dest)?;
    }
    drop(create_private(dest)?);
    let r = (|| -> Result<()> {
        let mut dst = Connection::open(dest)?;
        apply_key(&dst, key)?;
        {
            let b = rusqlite::backup::Backup::new(conn, &mut dst)?;
            b.run_to_completion(1024, std::time::Duration::from_millis(0), None)?;
        }
        drop(dst);
        Ok(())
    })();
    if r.is_err() {
        let _ = std::fs::remove_file(dest);
    }
    r?;
    set_mode(dest, 0o600)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: &[&str] = &["CREATE TABLE a (x INTEGER);", "CREATE TABLE b (y INTEGER);"];

    fn none(_: &Transaction, _: usize) -> Result<()> {
        Ok(())
    }

    #[test]
    fn key_literal_is_raw_hex() {
        let mut k = [0u8; 32];
        k[0] = 0xab;
        k[31] = 0x01;
        let l = key_literal(&k);
        assert_eq!(l.as_str(), format!("x'{}'", hex::encode(k)));
    }

    #[test]
    fn encrypted_open_rekey_and_wrong_key() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("d").join("x.database");
        let (k1, k2) = ([1u8; 32], [2u8; 32]);
        {
            let mut c = open(&p, Some(&k1), "test").unwrap();
            assert!(has_cipher(&c));
            migrate(&mut c, M, M.len(), none).unwrap();
            c.execute("INSERT INTO a VALUES (7)", []).unwrap();
        }
        assert!(!std::fs::read(&p).unwrap().starts_with(b"SQLite format 3"));
        let e = open(&p, Some(&k2), "test").unwrap_err();
        assert!(matches!(e, StorageError::WrongKey { .. }));
        assert!(e.to_string().starts_with("wrong key or not a test database ("), "{e}");
        {
            let c = open(&p, Some(&k1), "test").unwrap();
            rekey(&c, &k2).unwrap();
        }
        assert!(open(&p, Some(&k1), "test").is_err());
        let c = open_readonly(&p, Some(&k2), "test").unwrap();
        assert_eq!(c.query_row("SELECT x FROM a", [], |r| r.get::<_, i64>(0)).unwrap(), 7);
        assert_eq!(schema_version(&c).unwrap(), 2);
        // a read-only connection cannot rekey: reported, and the key stays as it was
        assert!(matches!(rekey(&c, &k1), Err(StorageError::RekeyFailed)));
        drop(c);
        assert!(open(&p, Some(&k2), "test").is_ok());
    }

    #[test]
    fn rekey_refuses_to_pretend_a_plaintext_file_is_encrypted() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("plain.db");
        let c = open(&p, None, "test").unwrap();
        c.execute_batch("CREATE TABLE a (x INTEGER)").unwrap();
        // SQLCipher either refuses or silently does nothing; either way it is an error here
        assert!(rekey(&c, &[3u8; 32]).is_err());
        drop(c);
        let c = open(&p, None, "test").unwrap();
        assert_eq!(c.query_row("SELECT count(*) FROM a", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
    }

    #[test]
    fn a_failed_first_open_leaves_no_file() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("x.database");
        std::fs::write(&p, b"definitely not a database, longer than a header").unwrap();
        assert!(open(&p, Some(&[1u8; 32]), "test").is_err());
        assert!(p.exists(), "an existing file is never removed");
        let q = t.path().join("missing").join("y.database");
        assert!(open_readonly(&q, None, "test").is_err());
        assert!(!q.exists());
    }

    #[cfg(unix)]
    #[test]
    fn new_files_and_folders_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("d").join("x.database");
        let c = open(&p, Some(&[1u8; 32]), "test").unwrap();
        assert_eq!(std::fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::metadata(p.parent().unwrap()).unwrap().permissions().mode() & 0o777, 0o700);
        let snap = t.path().join("snap.database");
        snapshot_to(&c, &snap, Some(&[1u8; 32])).unwrap();
        assert_eq!(std::fs::metadata(&snap).unwrap().permissions().mode() & 0o777, 0o600);
        assert!(open(&snap, Some(&[1u8; 32]), "test").is_ok());
        assert!(open(&snap, Some(&[2u8; 32]), "test").is_err());
    }

    #[test]
    fn migrations_run_once_in_order_with_hooks() {
        let mut c = Connection::open_in_memory().unwrap();
        let mut seen = vec![];
        migrate(&mut c, M, 1, |_, i| {
            seen.push(i);
            Ok::<(), StorageError>(())
        })
        .unwrap();
        migrate(&mut c, M, M.len(), |_, i| {
            seen.push(i);
            Ok::<(), StorageError>(())
        })
        .unwrap();
        assert_eq!(seen, [0, 1]);
        assert_eq!(schema_version(&c).unwrap(), 2);
    }

    #[test]
    fn a_failing_migration_or_hook_rolls_back_everything() {
        let mut c = Connection::open_in_memory().unwrap();
        migrate(&mut c, M, 1, none).unwrap();
        let bad: &[&str] = &[M[0], M[1], "CREATE TABLE c (z INTEGER); THIS IS NOT SQL;"];
        assert!(migrate(&mut c, bad, bad.len(), none).is_err());
        assert_eq!(schema_version(&c).unwrap(), 1);
        assert!(!crate::table_exists(&c, "b").unwrap() && !crate::table_exists(&c, "c").unwrap());
        let hook = migrate(&mut c, M, M.len(), |_, _| Err(StorageError::invalid("test", "hook failed")));
        assert!(hook.is_err());
        assert_eq!(schema_version(&c).unwrap(), 1);
        assert!(!crate::table_exists(&c, "b").unwrap());
    }

    #[test]
    fn plaintext_converts_in_place() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("x.database");
        Connection::open(&p).unwrap().execute_batch("CREATE TABLE a (x INTEGER); INSERT INTO a VALUES (3);").unwrap();
        encrypt_plaintext_in_place(&p, &[5u8; 32]).unwrap();
        assert!(!p.with_extension("encrypting").exists());
        assert!(open(&p, None, "test").is_err());
        let c = open(&p, Some(&[5u8; 32]), "test").unwrap();
        assert_eq!(c.query_row("SELECT x FROM a", [], |r| r.get::<_, i64>(0)).unwrap(), 3);
        drop(c);
        // not plaintext any more: refused, and the encrypted file is untouched
        let before = std::fs::read(&p).unwrap();
        assert!(matches!(encrypt_plaintext_in_place(&p, &[5u8; 32]), Err(StorageError::NotPlaintext)));
        assert_eq!(std::fs::read(&p).unwrap(), before);
        assert!(!p.with_extension("encrypting").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }
}
