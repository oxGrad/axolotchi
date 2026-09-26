//! SQLite storage: migrations, repo functions, OUI loader, and the ring
//! buffer that keeps raw sightings out of SQLite entirely.
//!
//! `open()` runs pending migrations before handing back the connection, so
//! every caller gets a ready-to-use schema. Wiring this crate into
//! `axolotchid`'s hydrate/persist/ring-buffer flow lands in milestone 4,
//! once `axolotchi-net` actually produces sightings to hydrate from,
//! persist, and buffer.

mod devices;
mod error;
mod migrations;
mod oui;
mod ring;

pub use devices::{load_devices, upsert_device};
pub use error::{Result, StoreError};
pub use oui::{import_oui, lookup_vendor, SEED_OUI_CSV};
pub use ring::{RawSighting, RingBuffer};

use rusqlite::Connection;

pub fn open(path: &str) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    migrations::apply(&conn)?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_sets_wal_mode_and_runs_migrations() {
        let dir = std::env::temp_dir().join(format!("axolotchi-test-{}.db", std::process::id()));
        let path = dir.to_str().expect("path is valid utf8");
        let conn = open(path).expect("open sets up the database");

        let mode: String = conn
            .query_row("PRAGMA journal_mode;", [], |row| row.get(0))
            .expect("read journal_mode");
        assert_eq!(mode, "wal");
        assert!(load_devices(&conn).expect("schema is ready").is_empty());

        drop(conn);
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(format!("{path}-wal"));
        let _ = std::fs::remove_file(format!("{path}-shm"));
    }

    #[test]
    fn open_works_on_an_in_memory_database() {
        let conn = open(":memory:").expect("open works in-memory");
        assert!(load_devices(&conn).expect("schema is ready").is_empty());
    }
}
