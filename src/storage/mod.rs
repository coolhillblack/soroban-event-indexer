//! Optional persistence backends (feature-gated).
//!
//! Enable with `features = ["sqlite"]` (default) for local SQLite storage.

#[cfg(feature = "sqlite")]
pub mod sqlite;

use crate::error::Result;
use crate::event::IndexedEvent;

/// Common trait for storage backends.
/// Implement this if you want to write a custom backend (e.g. Postgres).
pub trait EventStorage {
    /// Persist a single event. Implementations should be idempotent
    /// (calling twice with the same event ID is a no-op).
    fn save_event(&self, event: &IndexedEvent) -> Result<()>;

    /// Return the total number of events stored.
    fn count(&self) -> Result<u64>;
}
