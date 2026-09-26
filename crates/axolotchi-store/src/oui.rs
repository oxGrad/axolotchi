//! MAC OUI vendor lookups, used to show a friendly vendor name instead of a
//! bare MAC address in device cards.

use crate::Result;
use rusqlite::{params, Connection, OptionalExtension};

/// A small starter set of vendor prefixes. Not exhaustive — see the file
/// comment in `data/oui_seed.csv` for where to get the full IEEE registry.
pub const SEED_OUI_CSV: &str = include_str!("../data/oui_seed.csv");

/// Parses `prefix,vendor` lines (six hex digits, `:`/`-` separators and
/// surrounding whitespace ignored; blank lines and `#` comments skipped)
/// and upserts them into the `oui` table. Returns how many rows were
/// imported.
pub fn import_oui(conn: &Connection, csv: &str) -> Result<usize> {
    let mut count = 0;

    for line in csv.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((prefix, vendor)) = line.split_once(',') else {
            continue;
        };
        let prefix = normalize_prefix(prefix);
        let vendor = vendor.trim();
        if prefix.len() != 6 || vendor.is_empty() {
            continue;
        }

        conn.execute(
            "INSERT INTO oui (prefix, vendor) VALUES (?1, ?2)
             ON CONFLICT(prefix) DO UPDATE SET vendor = excluded.vendor",
            params![prefix, vendor],
        )?;
        count += 1;
    }

    Ok(count)
}

/// Looks up a MAC address's vendor by its first three octets.
pub fn lookup_vendor(conn: &Connection, mac: &str) -> Result<Option<String>> {
    let prefix = normalize_prefix(mac);
    if prefix.len() < 6 {
        return Ok(None);
    }

    let vendor = conn
        .query_row(
            "SELECT vendor FROM oui WHERE prefix = ?1",
            params![&prefix[..6]],
            |row| row.get(0),
        )
        .optional()?;
    Ok(vendor)
}

fn normalize_prefix(raw: &str) -> String {
    raw.chars()
        .filter(|c| c.is_ascii_hexdigit())
        .collect::<String>()
        .to_ascii_uppercase()
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
    fn imports_seed_data_and_looks_up_by_mac() {
        let conn = db();
        let imported = import_oui(&conn, SEED_OUI_CSV).unwrap();
        assert_eq!(imported, 10);

        let vendor = lookup_vendor(&conn, "b8:27:eb:11:22:33").unwrap();
        assert_eq!(vendor.as_deref(), Some("Raspberry Pi Foundation"));
    }

    #[test]
    fn lookup_is_case_and_separator_insensitive() {
        let conn = db();
        import_oui(&conn, "AABBCC,Test Vendor\n").unwrap();

        assert_eq!(
            lookup_vendor(&conn, "aa-bb-cc-dd-ee-ff")
                .unwrap()
                .as_deref(),
            Some("Test Vendor")
        );
        assert_eq!(
            lookup_vendor(&conn, "AABBCCDDEEFF").unwrap().as_deref(),
            Some("Test Vendor")
        );
    }

    #[test]
    fn unknown_prefix_returns_none() {
        let conn = db();
        assert_eq!(lookup_vendor(&conn, "00:00:00:00:00:00").unwrap(), None);
    }

    #[test]
    fn reimporting_updates_vendor_for_existing_prefix() {
        let conn = db();
        import_oui(&conn, "AABBCC,Old Name\n").unwrap();
        import_oui(&conn, "AABBCC,New Name\n").unwrap();
        assert_eq!(
            lookup_vendor(&conn, "AABBCC").unwrap().as_deref(),
            Some("New Name")
        );
    }

    #[test]
    fn blank_lines_and_comments_are_skipped() {
        let conn = db();
        let imported = import_oui(&conn, "# a comment\n\nAABBCC,Vendor\n").unwrap();
        assert_eq!(imported, 1);
    }
}
