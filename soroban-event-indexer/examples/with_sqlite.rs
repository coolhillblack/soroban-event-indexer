//! Example: Index events and persist them to SQLite.
//!
//! Run with:
//!   cargo run --example with_sqlite --features sqlite

#[cfg(feature = "sqlite")]
fn run() -> anyhow::Result<()> {
    use soroban_event_indexer::storage::{sqlite::SqliteStorage, EventStorage};
    use soroban_event_indexer::{EventIndexer, IndexerConfig, Network};

    tracing_subscriber::fmt().init();

    let db = SqliteStorage::open("soroban_events.db")?;
    db.migrate()?;

    let cursor = db.latest_cursor()?;
    let total = db.count()?;
    println!("Database has {total} events. Latest cursor: {cursor:?}");

    let indexer = EventIndexer::new(
        IndexerConfig::new("CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC")
            .network(Network::Testnet),
    );

    indexer.watch(move |event| {
        db.save_event(&event)?;
        let count = db.count()?;
        println!("[{count} stored] ledger={} event={}", event.ledger, event.event_name());
        Ok(())
    })?;

    Ok(())
}

fn main() -> anyhow::Result<()> {
    #[cfg(feature = "sqlite")]
    run()?;

    #[cfg(not(feature = "sqlite"))]
    eprintln!("Run with --features sqlite");

    Ok(())
}
