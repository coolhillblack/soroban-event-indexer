//! Demonstrates resuming an indexer's position after a restart.
//!
//! `SqliteStorage::latest_cursor()` returns the RPC paging token of the
//! last stored event — useful if you're paginating manually, but it
//! doesn't map back to a ledger sequence number, which is what
//! `IndexerConfig.start_ledger` actually needs.
//!
//! Instead, use `SqliteStorage::latest_ledger()`, which reads the ledger
//! column directly. On restart, the indexer re-polls from that ledger
//! (potentially re-seeing events already in that ledger), but this is
//! safe: `soroban_events.id` has a UNIQUE constraint and inserts use
//! `INSERT OR IGNORE`, so duplicates are silently dropped. No event loss,
//! no duplicate rows.

use soroban_event_indexer::storage::sqlite::SqliteStorage;
use soroban_event_indexer::storage::EventStorage;
use soroban_event_indexer::{EventIndexer, IndexerConfig, Network};

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let db = SqliteStorage::open("events.db")?;
    db.migrate()?;

    // Resume from the last ledger we successfully stored, or fall back to
    // the indexer's own auto-detection (≈7 days back) on first run.
    let start_ledger = db.latest_ledger()?;

    match start_ledger {
        Some(ledger) => println!("Resuming from ledger {ledger}"),
        None => println!("No prior events found, starting fresh"),
    }

    let mut config =
        IndexerConfig::new("CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC")
            .network(Network::Testnet);

    if let Some(ledger) = start_ledger {
        config = config.start_ledger(ledger);
    }

    let indexer = EventIndexer::new(config);

    indexer.watch(move |event| {
        db.save_event(&event)?;
        println!("stored event {} from ledger {}", event.id, event.ledger);
        Ok(())
    })?;

    Ok(())
}