//! Hydrating and persisting `axolotchi_core::Device` rows.
//!
//! Only ever called for the `Effect::Persist` cases the engine actually
//! emits — a presence transition or an IP change — never on every sighting,
//! per the "only state changes hit SQLite" rule in `CLAUDE.md`.

use crate::{Result, StoreError};
use axolotchi_core::{Device, DeviceId, Presence, SightingSource};
use rusqlite::{params, Connection};
use std::collections::HashMap;

fn presence_to_row(presence: Presence) -> (&'static str, i64) {
    match presence {
        Presence::Present => ("present", 0),
        Presence::Missed(n) => ("missed", n as i64),
        Presence::Gone => ("gone", 0),
    }
}

fn presence_from_row(tag: &str, missed: i64) -> Result<Presence> {
    match tag {
        "present" => Ok(Presence::Present),
        "missed" => Ok(Presence::Missed(missed.max(0) as u32)),
        "gone" => Ok(Presence::Gone),
        other => Err(StoreError::CorruptPresence(other.to_string())),
    }
}

fn source_to_str(source: SightingSource) -> &'static str {
    match source {
        SightingSource::Active => "active",
        SightingSource::Passive => "passive",
    }
}

fn source_from_str(tag: &str) -> Result<SightingSource> {
    match tag {
        "active" => Ok(SightingSource::Active),
        "passive" => Ok(SightingSource::Passive),
        other => Err(StoreError::CorruptSightingSource(other.to_string())),
    }
}

/// Inserts or updates a device row to match its current in-memory state.
pub fn upsert_device(conn: &Connection, device: &Device) -> Result<()> {
    let (presence_tag, missed) = presence_to_row(device.presence);
    conn.execute(
        "INSERT INTO devices (id, ip, presence, missed, last_seen, last_source)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(id) DO UPDATE SET
            ip = excluded.ip,
            presence = excluded.presence,
            missed = excluded.missed,
            last_seen = excluded.last_seen,
            last_source = excluded.last_source",
        params![
            device.id,
            device.ip,
            presence_tag,
            missed,
            device.last_seen,
            source_to_str(device.last_source),
        ],
    )?;
    Ok(())
}

/// Rebuilds the full device table into a map keyed by device id, for
/// hydrating `axolotchi_core::State` on startup.
pub fn load_devices(conn: &Connection) -> Result<HashMap<DeviceId, Device>> {
    let mut stmt =
        conn.prepare("SELECT id, ip, presence, missed, last_seen, last_source FROM devices")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, String>(5)?,
        ))
    })?;

    let mut devices = HashMap::new();
    for row in rows {
        let (id, ip, presence_tag, missed, last_seen, source_tag) = row?;
        let presence = presence_from_row(&presence_tag, missed)?;
        let source = source_from_str(&source_tag)?;
        devices.insert(
            id.clone(),
            Device::hydrate(id, ip, presence, last_seen, source),
        );
    }
    Ok(devices)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrations::apply(&conn).unwrap();
        conn
    }

    #[test]
    fn round_trips_a_device_through_upsert_and_load() {
        let conn = db();
        let device = Device::hydrate(
            "aa:bb:cc:dd:ee:ff".into(),
            Some("10.0.0.5".into()),
            Presence::Missed(2),
            1234,
            SightingSource::Passive,
        );
        upsert_device(&conn, &device).unwrap();

        let loaded = load_devices(&conn).unwrap();
        let loaded = loaded.get("aa:bb:cc:dd:ee:ff").unwrap();
        assert_eq!(loaded.ip.as_deref(), Some("10.0.0.5"));
        assert_eq!(loaded.presence, Presence::Missed(2));
        assert_eq!(loaded.last_seen, 1234);
        assert_eq!(loaded.last_source, SightingSource::Passive);
    }

    #[test]
    fn upsert_overwrites_the_existing_row_for_the_same_device() {
        let conn = db();
        let first = Device::hydrate(
            "dev-1".into(),
            None,
            Presence::Present,
            0,
            SightingSource::Active,
        );
        upsert_device(&conn, &first).unwrap();

        let second = Device::hydrate(
            "dev-1".into(),
            Some("10.0.0.9".into()),
            Presence::Gone,
            999,
            SightingSource::Passive,
        );
        upsert_device(&conn, &second).unwrap();

        let loaded = load_devices(&conn).unwrap();
        assert_eq!(loaded.len(), 1);
        let loaded = &loaded["dev-1"];
        assert_eq!(loaded.presence, Presence::Gone);
        assert_eq!(loaded.ip.as_deref(), Some("10.0.0.9"));
    }

    #[test]
    fn load_devices_on_empty_table_returns_empty_map() {
        let conn = db();
        assert!(load_devices(&conn).unwrap().is_empty());
    }

    #[test]
    fn corrupt_presence_value_is_reported_as_an_error() {
        let conn = db();
        conn.execute(
            "INSERT INTO devices (id, ip, presence, missed, last_seen, last_source)
             VALUES ('dev-1', NULL, 'sleeping', 0, 0, 'active')",
            [],
        )
        .unwrap();
        assert!(matches!(
            load_devices(&conn),
            Err(StoreError::CorruptPresence(_))
        ));
    }
}
