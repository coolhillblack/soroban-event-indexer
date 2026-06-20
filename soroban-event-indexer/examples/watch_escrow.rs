//! Example: Watch a Trustless-Work-style escrow contract for "funded" events.
//!
//! Run with:
//!   cargo run --example watch_escrow

use soroban_event_indexer::{EventFilter, EventIndexer, IndexerConfig, Network};

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // ── 3 lines to start indexing ─────────────────────────────────────────
    let indexer = EventIndexer::new(
        IndexerConfig::new("CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC")
            .network(Network::Testnet)
            .start_ledger(199_616)
            .poll_interval_secs(6)
            .max_events_per_poll(200),
    )
    .with_filter(EventFilter::new().topic("funded").successful_only());
    // ─────────────────────────────────────────────────────────────────────

    println!("Watching escrow contract for 'funded' events...");
    println!("Ctrl+C to stop\n");

    indexer.watch(|event| {
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("  Ledger:     {}", event.ledger);
        println!("  Closed at:  {}", event.ledger_closed_at);
        println!("  Event:      {}", event.event_name());
        println!("  Tx Hash:    {}", event.tx_hash.as_deref().unwrap_or("n/a"));
        println!("  Topics:");
        for (i, t) in event.topics.iter().enumerate() {
            println!("    [{i}] {}", t.display());
        }
        println!("  Value:      {}", event.value.display());
        println!();
        Ok(())
    })?;

    Ok(())
}
