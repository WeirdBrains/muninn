//! Savepoints: all-or-nothing units that nest inside each other and inside transactions.
use crate::error::{Result, StorageError};
use rusqlite::Connection;

fn check(name: &str) -> Result<()> {
    let ok = name.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if ok { Ok(()) } else { Err(StorageError::invalid("savepoint", format!("`{name}` is not a plain name"))) }
}

/// Open a savepoint (`SAVEPOINT name`).
pub fn begin(conn: &Connection, name: &str) -> Result<()> {
    check(name)?;
    Ok(conn.execute_batch(&format!("SAVEPOINT {name}"))?)
}

/// Keep everything since `begin` (`RELEASE name`).
pub fn release(conn: &Connection, name: &str) -> Result<()> {
    check(name)?;
    Ok(conn.execute_batch(&format!("RELEASE {name}"))?)
}

/// Undo everything since `begin` and close the savepoint.
pub fn rollback(conn: &Connection, name: &str) -> Result<()> {
    check(name)?;
    Ok(conn.execute_batch(&format!("ROLLBACK TO {name}; RELEASE {name}"))?)
}

/// Run `f` all-or-nothing: released when it returns `Ok`, rolled back when it returns `Err`
/// (the rollback's own failure is ignored so `f`'s error is the one returned).
pub fn savepoint<T, E: From<StorageError>>(
    conn: &Connection,
    name: &str,
    f: impl FnOnce() -> std::result::Result<T, E>,
) -> std::result::Result<T, E> {
    begin(conn, name)?;
    match f() {
        Ok(v) => {
            release(conn, name)?;
            Ok(v)
        }
        Err(e) => {
            let _ = rollback(conn, name);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn savepoints_nest_and_roll_back() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE t (x INTEGER)").unwrap();
        let r: Result<()> = savepoint(&c, "outer", || {
            c.execute("INSERT INTO t VALUES (1)", [])?;
            let inner: Result<()> = savepoint(&c, "inner", || {
                c.execute("INSERT INTO t VALUES (2)", [])?;
                Err(StorageError::invalid("test", "boom"))
            });
            assert!(inner.is_err());
            Ok(())
        });
        r.unwrap();
        let n: i64 = c.query_row("SELECT COUNT(*) FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
        assert!(begin(&c, "bad name; DROP TABLE t").is_err());
        assert!(begin(&c, "").is_err());
    }
}
