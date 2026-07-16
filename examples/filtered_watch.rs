//! Watches a contract for only "transfer" events from successful
//! transactions, and logs a decoded summary of each one.
//!
//! Run with:
//!   cargo run --example filtered_watch

use soroban_event_indexer::{EventFilter, EventIndexer, IndexerConfig, Network};

fn main() -> anyhow::Result<()> {
    let config = IndexerConfig::new("CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC")
        .network(Network::Testnet);

    EventIndexer::new(config)
        .with_filter(
            EventFilter::new()
                .topic("transfer")
                .successful_only(),
        )
        .watch(|event| {
            println!(
                "[ledger {}] {} -> {}",
                event.ledger,
                event.event_name(),
                event.value.display()
            );
            Ok(())
        })?;

    Ok(())
}