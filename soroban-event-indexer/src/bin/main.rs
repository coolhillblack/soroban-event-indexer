//! `soroban-indexer` — CLI binary for watching contract events.
//!
//! Usage:
//!   soroban-indexer watch --contract CONTRACT_ID [--network testnet] [--from LEDGER]
//!   soroban-indexer watch --contract CONTRACT_ID --db events.db --topic transfer

#[cfg(feature = "cli")]
mod cli {
    use clap::{Parser, Subcommand};
    use soroban_event_indexer::{EventFilter, EventIndexer, IndexerConfig, Network};
    use tracing_subscriber::EnvFilter;

    #[derive(Parser)]
    #[command(
        name = "soroban-indexer",
        about = "Watch and index Soroban smart contract events",
        version
    )]
    struct Cli {
        #[command(subcommand)]
        command: Commands,
    }

    #[derive(Subcommand)]
    enum Commands {
        /// Watch a contract and print events to stdout
        Watch {
            /// Contract ID (C... strkey)
            #[arg(short, long)]
            contract: String,

            /// Network: mainnet, testnet, futurenet, or a custom RPC URL
            #[arg(short, long, default_value = "testnet")]
            network: String,

            /// Starting ledger (default: ~7 days ago)
            #[arg(short, long)]
            from: Option<u32>,

            /// SQLite database path for persistence (optional)
            #[cfg(feature = "sqlite")]
            #[arg(short, long)]
            db: Option<String>,

            /// Poll interval in seconds (default: 6)
            #[arg(short, long, default_value = "6")]
            poll: u64,

            /// Filter to a specific event name (e.g. "transfer")
            #[arg(short, long)]
            topic: Option<String>,
        },
    }

    pub fn run() -> anyhow::Result<()> {
        tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::from_default_env().add_directive("soroban_event_indexer=info".parse()?),
            )
            .init();

        let cli = Cli::parse();

        match cli.command {
            Commands::Watch {
                contract,
                network,
                from,
                #[cfg(feature = "sqlite")]
                db,
                poll,
                topic,
            } => {
                let net = match network.to_lowercase().as_str() {
                    "mainnet" => Network::Mainnet,
                    "futurenet" => Network::Futurenet,
                    "testnet" => Network::Testnet,
                    url => Network::Custom(url.to_string()),
                };

                let mut config = IndexerConfig::new(&contract)
                    .network(net)
                    .poll_interval_secs(poll)
                    .max_events_per_poll(100);

                if let Some(ledger) = from {
                    config = config.start_ledger(ledger);
                }

                let mut indexer = EventIndexer::new(config);

                if let Some(name) = topic {
                    indexer = indexer.with_filter(EventFilter::new().topic(name));
                }

                println!("Watching contract: {contract}");
                println!("Press Ctrl+C to stop.\n");

                #[cfg(feature = "sqlite")]
                let db_storage = if let Some(path) = db {
                    use soroban_event_indexer::storage::sqlite::SqliteStorage;
                    let storage = SqliteStorage::open(&path)?;
                    storage.migrate()?;
                    println!("Persisting to: {path}");
                    Some(storage)
                } else {
                    None
                };

                indexer.watch(move |event| {
                    println!(
                        "[ledger {}] {} | topics: {} | value: {}",
                        event.ledger,
                        event.event_name(),
                        event
                            .topics
                            .iter()
                            .map(|t| t.display())
                            .collect::<Vec<_>>()
                            .join(", "),
                        event.value.display(),
                    );

                    #[cfg(feature = "sqlite")]
                    if let Some(ref storage) = db_storage {
                        use soroban_event_indexer::storage::EventStorage;
                        storage.save_event(&event)?;
                    }

                    Ok(())
                })?;
            }
        }

        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    #[cfg(feature = "cli")]
    cli::run()?;

    #[cfg(not(feature = "cli"))]
    eprintln!("Compile with --features cli to enable the CLI binary.");

    Ok(())
}
