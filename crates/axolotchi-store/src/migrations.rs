//! A hand-rolled migration runner: no extra dependency, just `PRAGMA
//! user_version` tracking how many of `MIGRATIONS` have already run.

use crate::Result;
use rusqlite::Connection;

const MIGRATIONS: &[&str] = &[r#"
    CREATE TABLE devices (
        id          TEXT PRIMARY KEY,
        ip          TEXT,
        presence    TEXT NOT NULL,
        missed      INTEGER NOT NULL DEFAULT 0,
        last_seen   INTEGER NOT NULL,
        last_source TEXT NOT NULL
    );

    CREATE TABLE oui (
        prefix TEXT PRIMARY KEY,
        vendor TEXT NOT NULL
    );

    CREATE TABLE kv (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
"#];

/// Applies any migrations in `MIGRATIONS` that haven't run against this
/// connection yet, bumping `user_version` as it goes.
pub fn apply(conn: &Connection) -> Result<()> {
    let current: i64 = conn.query_row("PRAGMA user_version;", [], |row| row.get(0))?;
    let current = current.max(0) as usize;

    for (i, migration) in MIGRATIONS.iter().enumerate().skip(current) {
        conn.execute_batch(migration)?;
        conn.pragma_update(None, "user_version", (i + 1) as i64)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_creates_expected_tables() {
        let conn = Connection::open_in_memory().unwrap();
        apply(&conn).unwrap();

        for table in ["devices", "oui", "kv"] {
            let exists: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(exists, "expected table `{table}` to exist");
        }
    }

    #[test]
    fn apply_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        apply(&conn).unwrap();
        apply(&conn).unwrap();

        let version: i64 = conn
            .query_row("PRAGMA user_version;", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, MIGRATIONS.len() as i64);
    }
}
