//! The `kv` table: small odds and ends that don't warrant their own schema
//! (sprite `slack_file` ids, device nicknames, quiet hours settings).

use crate::Result;
use rusqlite::{params, Connection, OptionalExtension};

pub fn kv_set(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO kv (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub fn kv_get(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row("SELECT value FROM kv WHERE key = ?1", params![key], |row| {
            row.get(0)
        })
        .optional()?)
}

pub fn kv_delete(conn: &Connection, key: &str) -> Result<()> {
    conn.execute("DELETE FROM kv WHERE key = ?1", params![key])?;
    Ok(())
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
    fn round_trips_a_value() {
        let conn = db();
        kv_set(&conn, "nickname:dev-1", "Kitchen Pi").unwrap();
        assert_eq!(
            kv_get(&conn, "nickname:dev-1").unwrap().as_deref(),
            Some("Kitchen Pi")
        );
    }

    #[test]
    fn missing_key_returns_none() {
        let conn = db();
        assert_eq!(kv_get(&conn, "does-not-exist").unwrap(), None);
    }

    #[test]
    fn setting_an_existing_key_overwrites_it() {
        let conn = db();
        kv_set(&conn, "k", "old").unwrap();
        kv_set(&conn, "k", "new").unwrap();
        assert_eq!(kv_get(&conn, "k").unwrap().as_deref(), Some("new"));
    }

    #[test]
    fn delete_removes_the_key() {
        let conn = db();
        kv_set(&conn, "k", "v").unwrap();
        kv_delete(&conn, "k").unwrap();
        assert_eq!(kv_get(&conn, "k").unwrap(), None);
    }
}
