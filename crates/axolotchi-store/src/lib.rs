//! SQLite storage: migrations, repo functions, OUI loader.
//!
//! Stub crate (milestone 1): just enough to prove `rusqlite` builds with the
//! `bundled` feature under `cross` and that new connections open in WAL mode.
//! Schema and migrations land in milestone 3.

use rusqlite::Connection;

pub fn open(path: &str) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_in_memory_connection() {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        conn.execute_batch("SELECT 1;").expect("basic query works");
    }

    #[test]
    fn open_sets_wal_mode() {
        let dir = std::env::temp_dir().join(format!("axolotchi-test-{}.db", std::process::id()));
        let path = dir.to_str().expect("path is valid utf8");
        let conn = open(path).expect("open sets wal mode");
        let mode: String = conn
            .query_row("PRAGMA journal_mode;", [], |row| row.get(0))
            .expect("read journal_mode");
        assert_eq!(mode, "wal");
        drop(conn);
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(format!("{path}-wal"));
        let _ = std::fs::remove_file(format!("{path}-shm"));
    }
}
