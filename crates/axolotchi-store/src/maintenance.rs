//! Nightly housekeeping: pruning long-`Gone` devices and reclaiming the
//! space they freed. Protects the SD card from growing forever with
//! devices that will never come back (a guest's phone, a decommissioned
//! sensor, ...).

use crate::Result;
use rusqlite::{params, Connection};

/// Deletes devices that have been `Gone` for longer than `older_than_secs`
/// relative to `now`. Returns how many rows were removed.
pub fn prune_gone_devices(conn: &Connection, now: i64, older_than_secs: i64) -> Result<usize> {
    let threshold = now - older_than_secs;
    let deleted = conn.execute(
        "DELETE FROM devices WHERE presence = 'gone' AND last_seen < ?1",
        params![threshold],
    )?;
    Ok(deleted)
}

/// Reclaims space freed by prior deletes. Meant to run right after pruning,
/// not on every startup — `VACUUM` rewrites the whole database file, which
/// is wasted SD card wear when there's nothing to reclaim.
pub fn vacuum(conn: &Connection) -> Result<()> {
    conn.execute_batch("VACUUM;")?;
    Ok(())
}

/// Flushes the WAL back into the main database file and truncates it — the
/// graceful-shutdown step, so a `SIGTERM` leaves a clean, minimal-sized
/// file rather than relying on SQLite's own eventual auto-checkpoint.
pub fn checkpoint(conn: &Connection) -> Result<()> {
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices::{load_devices, upsert_device};
    use crate::migrations;
    use axolotchi_core::{Device, Presence, SightingSource};

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrations::apply(&conn).unwrap();
        conn
    }

    fn device(id: &str, presence: Presence, last_seen: i64) -> Device {
        Device::hydrate(
            id.to_string(),
            None,
            presence,
            last_seen,
            SightingSource::Active,
        )
    }

    #[test]
    fn prunes_only_devices_gone_longer_than_the_threshold() {
        let conn = db();
        upsert_device(&conn, &device("old", Presence::Gone, 0)).unwrap();
        upsert_device(&conn, &device("recent", Presence::Gone, 900)).unwrap();
        upsert_device(&conn, &device("present", Presence::Present, 0)).unwrap();

        let deleted = prune_gone_devices(&conn, 1000, 500).unwrap();
        assert_eq!(deleted, 1);

        let remaining = load_devices(&conn).unwrap();
        assert!(!remaining.contains_key("old"));
        assert!(remaining.contains_key("recent"));
        assert!(
            remaining.contains_key("present"),
            "prune must never touch non-Gone devices"
        );
    }

    #[test]
    fn prune_with_nothing_old_enough_is_a_no_op() {
        let conn = db();
        upsert_device(&conn, &device("recent", Presence::Gone, 900)).unwrap();
        assert_eq!(prune_gone_devices(&conn, 1000, 500).unwrap(), 0);
    }

    #[test]
    fn vacuum_runs_without_error() {
        let conn = db();
        upsert_device(&conn, &device("dev-1", Presence::Present, 0)).unwrap();
        vacuum(&conn).unwrap();
    }

    #[test]
    fn checkpoint_runs_without_error() {
        let conn = db();
        upsert_device(&conn, &device("dev-1", Presence::Present, 0)).unwrap();
        checkpoint(&conn).unwrap();
    }
}
