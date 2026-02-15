use std::path::Path;
use anyhow::{Context, Result};
use rusqlite::{Connection, params, OptionalExtension};
use shared::types::ServiceEntry;
use chrono::Utc;

pub struct CacheDb {
    conn: Connection,
}

impl CacheDb {
    /// Open or create the SQLite database with WAL mode enabled
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();

        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }

        let conn = Connection::open(path)
            .with_context(|| format!("Failed to open database: {}", path.display()))?;

        // Enable WAL mode for better concurrency and crash recovery
        conn.execute_batch("PRAGMA journal_mode=WAL;")
            .context("Failed to enable WAL mode")?;

        // Create tables if they don't exist
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS services (
                instance_name TEXT PRIMARY KEY,
                service_type  TEXT NOT NULL,
                hostname      TEXT NOT NULL,
                addresses     TEXT NOT NULL,
                port          INTEGER NOT NULL,
                txt           TEXT NOT NULL,
                first_seen    TEXT NOT NULL,
                last_seen     TEXT NOT NULL,
                ttl           INTEGER NOT NULL,
                alive         INTEGER NOT NULL DEFAULT 1
            );

            CREATE INDEX IF NOT EXISTS idx_service_type ON services(service_type);
            "#,
        )
        .context("Failed to create database schema")?;

        Ok(Self { conn })
    }

    /// Insert or update a service entry. Returns true if data changed.
    pub fn upsert_service(&self, entry: &ServiceEntry) -> Result<bool> {
        // Fix #7: compare meaningful fields in Rust instead of fragile SQL concatenation
        let existing = self
            .conn
            .query_row(
                "SELECT instance_name, service_type, hostname, addresses, port, txt,
                        first_seen, last_seen, ttl, alive
                 FROM services WHERE instance_name = ?1",
                params![&entry.instance_name],
                |row| Ok(Self::row_to_entry(row)?),
            )
            .optional()
            .context("Failed to query existing service")?;

        let changed = match &existing {
            Some(old) => service_data_changed(old, entry),
            None => true,
        };

        let addresses_json = serde_json::to_string(&entry.addresses)
            .context("Failed to serialize addresses")?;
        let txt_json = serde_json::to_string(&entry.txt)
            .context("Failed to serialize txt records")?;

        // Insert or replace
        self.conn.execute(
            r#"
            INSERT INTO services (
                instance_name, service_type, hostname, addresses, port, txt,
                first_seen, last_seen, ttl, alive
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ON CONFLICT(instance_name) DO UPDATE SET
                service_type = excluded.service_type,
                hostname = excluded.hostname,
                addresses = excluded.addresses,
                port = excluded.port,
                txt = excluded.txt,
                last_seen = excluded.last_seen,
                ttl = excluded.ttl,
                alive = excluded.alive
            "#,
            params![
                &entry.instance_name,
                &entry.service_type,
                &entry.hostname,
                &addresses_json,
                entry.port,
                &txt_json,
                entry.first_seen.to_rfc3339(),
                entry.last_seen.to_rfc3339(),
                entry.ttl,
                entry.alive as i32,
            ],
        )
        .context("Failed to upsert service")?;

        Ok(changed)
    }

    /// Mark a service as dead (not alive)
    pub fn mark_dead(&self, instance_name: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE services SET alive = 0, last_seen = ?1 WHERE instance_name = ?2",
            params![now, instance_name],
        )
        .context("Failed to mark service as dead")?;
        Ok(())
    }

    /// Get all services
    pub fn get_all_services(&self) -> Result<Vec<ServiceEntry>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT instance_name, service_type, hostname, addresses, port, txt,
                        first_seen, last_seen, ttl, alive
                 FROM services"
            )
            .context("Failed to prepare query")?;

        let services = stmt
            .query_map([], |row| {
                Ok(Self::row_to_entry(row)?)
            })
            .context("Failed to query services")?
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to collect services")?;

        Ok(services)
    }

    /// Get services filtered by type
    pub fn get_services_by_type(&self, service_type: &str) -> Result<Vec<ServiceEntry>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT instance_name, service_type, hostname, addresses, port, txt,
                        first_seen, last_seen, ttl, alive
                 FROM services WHERE service_type = ?1"
            )
            .context("Failed to prepare query")?;

        let services = stmt
            .query_map([service_type], |row| {
                Ok(Self::row_to_entry(row)?)
            })
            .context("Failed to query services by type")?
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to collect services")?;

        Ok(services)
    }

    /// Get a single service by instance name
    pub fn get_service(&self, instance_name: &str) -> Result<Option<ServiceEntry>> {
        let result = self
            .conn
            .query_row(
                "SELECT instance_name, service_type, hostname, addresses, port, txt,
                        first_seen, last_seen, ttl, alive
                 FROM services WHERE instance_name = ?1",
                params![instance_name],
                |row| Ok(Self::row_to_entry(row)?),
            )
            .optional()
            .context("Failed to query service")?;

        Ok(result)
    }

    /// Mark services as stale if not seen recently
    pub fn mark_stale(&self, stale_after_secs: u64) -> Result<u64> {
        let cutoff = Utc::now() - chrono::Duration::seconds(stale_after_secs as i64);
        let cutoff_str = cutoff.to_rfc3339();

        let count = self.conn.execute(
            "UPDATE services SET alive = 0 WHERE last_seen < ?1 AND alive = 1",
            params![cutoff_str],
        )
        .context("Failed to mark stale services")?;

        Ok(count as u64)
    }

    /// Prune old services from the database
    pub fn prune_stale(&self, prune_after_secs: u64) -> Result<u64> {
        let cutoff = Utc::now() - chrono::Duration::seconds(prune_after_secs as i64);
        let cutoff_str = cutoff.to_rfc3339();

        let count = self.conn.execute(
            "DELETE FROM services WHERE last_seen < ?1",
            params![cutoff_str],
        )
        .context("Failed to prune old services")?;

        Ok(count as u64)
    }

    /// Helper to convert a database row to ServiceEntry
    fn row_to_entry(row: &rusqlite::Row) -> Result<ServiceEntry, rusqlite::Error> {
        let addresses_json: String = row.get(3)?;
        let txt_json: String = row.get(5)?;
        let first_seen_str: String = row.get(6)?;
        let last_seen_str: String = row.get(7)?;
        let alive_int: i32 = row.get(9)?;

        let addresses = serde_json::from_str(&addresses_json)
            .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
                3,
                rusqlite::types::Type::Text,
                Box::new(e),
            ))?;

        let txt = serde_json::from_str(&txt_json)
            .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
                5,
                rusqlite::types::Type::Text,
                Box::new(e),
            ))?;

        let first_seen = chrono::DateTime::parse_from_rfc3339(&first_seen_str)
            .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
                6,
                rusqlite::types::Type::Text,
                Box::new(e),
            ))?
            .with_timezone(&Utc);

        let last_seen = chrono::DateTime::parse_from_rfc3339(&last_seen_str)
            .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
                7,
                rusqlite::types::Type::Text,
                Box::new(e),
            ))?
            .with_timezone(&Utc);

        Ok(ServiceEntry {
            instance_name: row.get(0)?,
            service_type: row.get(1)?,
            hostname: row.get(2)?,
            addresses,
            port: row.get::<_, u16>(4)?,
            txt,
            first_seen,
            last_seen,
            ttl: row.get::<_, u32>(8)?,
            alive: alive_int != 0,
        })
    }
}

/// Fix #7: compare meaningful service fields in Rust — avoids fragile SQL
/// concatenation that duplicated field order and serialization across two languages.
fn service_data_changed(old: &ServiceEntry, new: &ServiceEntry) -> bool {
    old.hostname != new.hostname
        || old.addresses != new.addresses
        || old.port != new.port
        || old.txt != new.txt
        || old.ttl != new.ttl
        || old.alive != new.alive
        || old.service_type != new.service_type
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::net::Ipv6Addr;

    fn test_entry() -> ServiceEntry {
        ServiceEntry {
            service_type: "_http._tcp".to_string(),
            instance_name: "test._http._tcp.local.".to_string(),
            hostname: "test.local.".to_string(),
            addresses: vec![Ipv6Addr::new(0xfd00, 0, 0, 1, 0, 0, 0, 1)],
            port: 8080,
            txt: HashMap::from([("path".to_string(), "/api".to_string())]),
            first_seen: Utc::now(),
            last_seen: Utc::now(),
            ttl: 4500,
            alive: true,
        }
    }

    #[test]
    fn test_create_and_query() {
        let db = CacheDb::open(":memory:").unwrap();
        let entry = test_entry();

        let changed = db.upsert_service(&entry).unwrap();
        assert!(changed, "First insert should report change");

        let retrieved = db.get_service(&entry.instance_name).unwrap().unwrap();
        assert_eq!(retrieved.hostname, entry.hostname);
        assert_eq!(retrieved.port, entry.port);
    }

    #[test]
    fn test_upsert_detects_changes() {
        let db = CacheDb::open(":memory:").unwrap();
        let mut entry = test_entry();

        db.upsert_service(&entry).unwrap();

        // No change - should return false
        let changed = db.upsert_service(&entry).unwrap();
        assert!(!changed, "Identical upsert should not report change");

        // Change port - should return true
        entry.port = 9090;
        let changed = db.upsert_service(&entry).unwrap();
        assert!(changed, "Modified entry should report change");
    }

    #[test]
    fn test_mark_dead() {
        let db = CacheDb::open(":memory:").unwrap();
        let entry = test_entry();

        db.upsert_service(&entry).unwrap();
        db.mark_dead(&entry.instance_name).unwrap();

        let retrieved = db.get_service(&entry.instance_name).unwrap().unwrap();
        assert!(!retrieved.alive);
    }

    #[test]
    fn test_get_services_by_type() {
        let db = CacheDb::open(":memory:").unwrap();

        let mut entry1 = test_entry();
        entry1.instance_name = "service1._http._tcp.local.".to_string();

        let mut entry2 = test_entry();
        entry2.instance_name = "service2._http._tcp.local.".to_string();

        let mut entry3 = test_entry();
        entry3.service_type = "_ssh._tcp".to_string();
        entry3.instance_name = "service3._ssh._tcp.local.".to_string();

        db.upsert_service(&entry1).unwrap();
        db.upsert_service(&entry2).unwrap();
        db.upsert_service(&entry3).unwrap();

        let http_services = db.get_services_by_type("_http._tcp").unwrap();
        assert_eq!(http_services.len(), 2);

        let ssh_services = db.get_services_by_type("_ssh._tcp").unwrap();
        assert_eq!(ssh_services.len(), 1);
    }
}
