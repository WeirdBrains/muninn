//! The general SQLite / SQLCipher engine under the app's schema (the future `weirdnet-storage`):
//! opening with an injected raw database key, the key and rekey pragmas, in-place
//! plaintext-to-encrypted conversion, the pragma policy, an ordered migration runner and
//! private file modes.
//!
//! Schema-free and brand-free: callers supply the migrations and name the database in error
//! messages. Key boundary: the caller owns every key. This module only applies a raw 32-byte
//! key it is handed (and zeroizes the hex literal it builds); it never derives, wraps, stores,
//! recovers or exports one — that is `crate::crypto::keyring`'s job.
use crate::error::{Error, Result};
use rusqlite::{Connection, OpenFlags, Transaction};
use std::path::Path;
use zeroize::Zeroize;

/// A raw SQLCipher key (the caller's data-encryption key).
pub type DbKey = [u8; 32];

/// Create a folder (and parents) and make it private (0700 on Unix).
pub fn ensure_dir(d: &Path) -> Result<()> {
    if !d.exists() {
        std::fs::create_dir_all(d)?;
    }
    set_mode(d, 0o700)
}

#[cfg(unix)]
pub fn set_mode(p: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode))?;
    Ok(())
}

#[cfg(not(unix))]
pub fn set_mode(_p: &Path, _mode: u32) -> Result<()> {
    Ok(())
}

/// Apply a raw key (`PRAGMA key = x'…'`); SQLCipher's own logging stays off stderr.
pub fn apply_key(conn: &Connection, key: Option<&DbKey>) -> Result<()> {
    let _ = conn.query_row("PRAGMA cipher_log_level = NONE", [], |_| Ok(()));
    if let Some(k) = key {
        let mut lit = format!("x'{}'", hex::encode(k));
        let r = conn.pragma_update(None, "key", &lit);
        lit.zeroize();
        r?;
    }
    Ok(())
}

/// Re-encrypt an open database with a new raw key (`PRAGMA rekey`).
pub fn rekey(conn: &Connection, new_key: &DbKey) -> Result<()> {
    let mut lit = format!("x'{}'", hex::encode(new_key));
    let r = conn.pragma_update(None, "rekey", &lit);
    lit.zeroize();
    r?;
    Ok(())
}

fn check_key(conn: &Connection, what: &str) -> Result<()> {
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0))
        .map(|_| ())
        .map_err(|e| Error::Unlock(format!("wrong key or not a {what} database ({e})")))
}

/// Open (creating if needed, with a private parent folder and a 0600 file) and verify the key.
/// `what` names the database in the wrong-key error.
pub fn open(path: &Path, key: Option<&DbKey>, what: &str) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    let existed = path.exists();
    let conn = Connection::open(path)?;
    apply_key(&conn, key)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    check_key(&conn, what)?;
    if !existed {
        set_mode(path, 0o600)?;
    }
    Ok(conn)
}

/// Open an existing database read-only (no writes, no migrations) and verify the key.
pub fn open_readonly(path: &Path, key: Option<&DbKey>, what: &str) -> Result<Connection> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    apply_key(&conn, key)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    check_key(&conn, what)?;
    Ok(conn)
}

/// Encrypt an existing plaintext SQLite file in place with `key` (`sqlcipher_export` into a
/// sibling file, then rename over the original; stale WAL/SHM files are removed).
pub fn encrypt_plaintext_in_place(path: &Path, key: &DbKey) -> Result<()> {
    let tmp = path.with_extension("encrypting");
    if tmp.exists() {
        std::fs::remove_file(&tmp)?;
    }
    {
        let conn = Connection::open(path)?;
        conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0))
            .map_err(|_| Error::Unlock("existing database is not a plaintext SQLite file".into()))?;
        let mut lit = zeroize::Zeroizing::new(format!("x'{}'", hex::encode(key)));
        conn.execute(&format!("ATTACH DATABASE ?1 AS enc KEY \"{}\"", lit.as_str()), [tmp.to_string_lossy()])?;
        lit.zeroize();
        conn.query_row("SELECT sqlcipher_export('enc')", [], |_| Ok(()))?;
        conn.execute("DETACH DATABASE enc", [])?;
    }
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        for side in ["wal", "shm"] {
            let _ = std::fs::remove_file(path.with_extension(format!("{ext}-{side}")));
        }
    }
    std::fs::rename(&tmp, path)?;
    set_mode(path, 0o600)?;
    Ok(())
}

/// Apply `migrations[..upto]` not yet recorded in `schema_version`, in one transaction.
/// `applied(tx, i)` runs right after migration `i` (0-based) and before it is recorded:
/// the app's data upgrades and seed rows go there.
pub fn migrate(conn: &mut Connection, migrations: &[&str], upto: usize, mut applied: impl FnMut(&Transaction, usize) -> Result<()>) -> Result<()> {
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")?;
    conn.execute_batch("CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);")?;
    let v: i64 = conn.query_row("SELECT COALESCE(MAX(version), 0) FROM schema_version", [], |r| r.get(0))?;
    let tx = conn.transaction()?;
    for (i, m) in migrations.iter().enumerate().take(upto) {
        let target = i as i64 + 1;
        if v < target {
            tx.execute_batch(m)?;
            applied(&tx, i)?;
            tx.execute("INSERT INTO schema_version(version) VALUES (?1)", [target])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// The highest applied migration (`schema_version`).
pub fn schema_version(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: &[&str] = &["CREATE TABLE a (x INTEGER);", "CREATE TABLE b (y INTEGER);"];

    #[test]
    fn encrypted_open_rekey_and_wrong_key() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("d").join("x.database");
        let (k1, k2) = ([1u8; 32], [2u8; 32]);
        {
            let mut c = open(&p, Some(&k1), "test").unwrap();
            migrate(&mut c, M, M.len(), |_, _| Ok(())).unwrap();
            c.execute("INSERT INTO a VALUES (7)", []).unwrap();
        }
        assert!(!std::fs::read(&p).unwrap().starts_with(b"SQLite format 3"));
        let e = open(&p, Some(&k2), "test").unwrap_err().to_string();
        assert!(e.contains("wrong key or not a test database"), "{e}");
        {
            let c = open(&p, Some(&k1), "test").unwrap();
            c.query_row("PRAGMA journal_mode = DELETE", [], |_| Ok(())).unwrap();
            rekey(&c, &k2).unwrap();
        }
        assert!(open(&p, Some(&k1), "test").is_err());
        let c = open_readonly(&p, Some(&k2), "test").unwrap();
        assert_eq!(c.query_row("SELECT x FROM a", [], |r| r.get::<_, i64>(0)).unwrap(), 7);
        assert_eq!(schema_version(&c).unwrap(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn new_files_and_folders_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("d").join("x.database");
        open(&p, Some(&[1u8; 32]), "test").unwrap();
        assert_eq!(std::fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::metadata(p.parent().unwrap()).unwrap().permissions().mode() & 0o777, 0o700);
    }

    #[test]
    fn migrations_run_once_in_order_with_hooks() {
        let mut c = Connection::open_in_memory().unwrap();
        let mut seen = vec![];
        migrate(&mut c, M, 1, |_, i| {
            seen.push(i);
            Ok(())
        })
        .unwrap();
        migrate(&mut c, M, M.len(), |_, i| {
            seen.push(i);
            Ok(())
        })
        .unwrap();
        assert_eq!(seen, [0, 1]);
        assert_eq!(schema_version(&c).unwrap(), 2);
    }

    #[test]
    fn plaintext_converts_in_place() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("x.database");
        Connection::open(&p).unwrap().execute_batch("CREATE TABLE a (x INTEGER); INSERT INTO a VALUES (3);").unwrap();
        encrypt_plaintext_in_place(&p, &[5u8; 32]).unwrap();
        assert!(open(&p, None, "test").is_err());
        let c = open(&p, Some(&[5u8; 32]), "test").unwrap();
        assert_eq!(c.query_row("SELECT x FROM a", [], |r| r.get::<_, i64>(0)).unwrap(), 3);
        // not plaintext any more
        assert!(encrypt_plaintext_in_place(&p, &[5u8; 32]).is_err());
    }
}
