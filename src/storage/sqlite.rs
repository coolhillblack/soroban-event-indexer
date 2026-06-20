//! SQLite persistence backend using `rusqlite`.
//!
//! Creates a single `soroban_events` table and writes one row per event.
//! The `id` column (the RPC event ID) has a UNIQUE constraint, so duplicate
//! inserts are silently ignored — making the indexer safe to restart.
//!
//! # Feature flag
//! Only compiled when `features = ["sqlite"]` (enabled by default).

use crate::error::Result;
use crate::event::IndexedEvent;
use crate::storage::EventStorage;
use rusqlite::{params, Connection};
use std::sync::Mutex;

/// SQLite-backed event store.
///
/// ```rust,no_run
/// use soroban_event_indexer::storage::{sqlite::SqliteStorage, EventStorage};
/// use soroban_event_indexer::{EventIndexer, IndexerConfig};
///
/// # fn main() -> anyhow::Result<()> {
/// let db = SqliteStorage::open("events.db")?;
/// db.migrate()?;
///
/// let indexer = EventIndexer::new(IndexerConfig::new("CONTRACT_ID"));
/// indexer.watch(move |event| {
///     db.save_event(&event)?;
///     Ok(())
/// })?;
/// # Ok(()) }
/// ```
pub struct SqliteStorage {
    conn: Mutex<Connection>,
}

impl SqliteStorage {
    /// Open (or create) a SQLite database at the given path.
    /// Use `":memory:"` for an in-memory database.
    pub fn open(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Run the schema migration — creates the `soroban_events` table
    /// if it does not already exist. Safe to call on every startup.
    pub fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS soroban_events (
                id                  TEXT PRIMARY KEY,
                paging_token        TEXT NOT NULL,
                contract_id         TEXT NOT NULL,
                ledger              INTEGER NOT NULL,
                ledger_closed_at    TEXT NOT NULL,
                tx_hash             TEXT,
                kind                TEXT NOT NULL,
                in_successful_call  INTEGER NOT NULL DEFAULT 1,
                topics_json         TEXT NOT NULL,
                value_json          TEXT NOT NULL,
                raw_topics_json     TEXT NOT NULL,
                raw_value           TEXT NOT NULL,
                indexed_at          TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_events_contract ON soroban_events (contract_id);
            CREATE INDEX IF NOT EXISTS idx_events_ledger ON soroban_events (ledger);
            CREATE INDEX IF NOT EXISTS idx_events_kind ON soroban_events (kind);",
        )?;
        Ok(())
    }

    /// Returns the paging token of the most recently stored event, if any.
    /// Useful for resuming an indexer after a restart.
    pub fn latest_cursor(&self) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT paging_token FROM soroban_events ORDER BY ledger DESC LIMIT 1")?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    /// Returns the ledger sequence of the most recently stored event, if any.
    /// Use this (not `latest_cursor`) to resume an indexer via
    /// `IndexerConfig.start_ledger`, since RPC's pagination cursor doesn't
    /// map back to a ledger number — but the ledger we already stored does.
    pub fn latest_ledger(&self) -> Result<Option<u32>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT ledger FROM soroban_events ORDER BY ledger DESC LIMIT 1")?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            let ledger: i64 = row.get(0)?;
            Ok(Some(ledger as u32))
        } else {
            Ok(None)
        }
    }
}

impl EventStorage for SqliteStorage {
    fn save_event(&self, event: &IndexedEvent) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        let kind = format!("{:?}", event.kind).to_lowercase();
        let topics_json = serde_json::to_string(&event.topics).unwrap_or_default();
        let value_json = serde_json::to_string(&event.value).unwrap_or_default();
        let raw_topics_json = serde_json::to_string(&event.raw_topics).unwrap_or_default();

        conn.execute(
            "INSERT OR IGNORE INTO soroban_events
                (id, paging_token, contract_id, ledger, ledger_closed_at,
                 tx_hash, kind, in_successful_call,
                 topics_json, value_json, raw_topics_json, raw_value)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                event.id,
                event.paging_token,
                event.contract_id,
                event.ledger,
                event.ledger_closed_at.to_rfc3339(),
                event.tx_hash,
                kind,
                event.in_successful_call as i32,
                topics_json,
                value_json,
                raw_topics_json,
                event.raw_value,
            ],
        )?;
        Ok(())
    }

    fn count(&self) -> Result<u64> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM soroban_events", [], |row| row.get(0))?;
        Ok(count as u64)
    }
}
