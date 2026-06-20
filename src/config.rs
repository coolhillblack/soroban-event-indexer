//! Indexer configuration — everything needed to describe what to watch.

use std::time::Duration;

/// Which Stellar network to connect to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Network {
    /// Stellar Mainnet
    Mainnet,
    /// Stellar Testnet
    Testnet,
    /// Stellar Futurenet (bleeding-edge)
    Futurenet,
    /// A fully custom RPC endpoint (e.g. private node, local sandbox)
    Custom(String),
}

impl Network {
    /// Returns the canonical RPC URL for the network.
    pub fn rpc_url(&self) -> String {
        match self {
            Network::Mainnet => {
                "https://mainnet.sorobanrpc.com".to_string()
            }
            Network::Testnet => "https://soroban-testnet.stellar.org".to_string(),
            Network::Futurenet => "https://rpc-futurenet.stellar.org".to_string(),
            Network::Custom(url) => url.clone(),
        }
    }
}

/// How often to poll the RPC for new events.
#[derive(Debug, Clone)]
pub enum PollInterval {
    /// Every N seconds (default: 6s, matching Stellar ledger close time)
    Seconds(u64),
    /// Custom duration
    Duration(Duration),
}

impl PollInterval {
    pub fn as_duration(&self) -> Duration {
        match self {
            PollInterval::Seconds(s) => Duration::from_secs(*s),
            PollInterval::Duration(d) => *d,
        }
    }
}

impl Default for PollInterval {
    fn default() -> Self {
        // One ledger close time ≈ 5–6 seconds
        PollInterval::Seconds(6)
    }
}

/// Full configuration for one indexer instance.
///
/// Use the builder methods to configure, then pass to [`EventIndexer::new`].
///
/// # Example
/// ```
/// use soroban_event_indexer::{IndexerConfig, Network};
///
/// let config = IndexerConfig::new("CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC")
///     .network(Network::Testnet)
///     .start_ledger(199_616)
///     .poll_interval_secs(6)
///     .max_events_per_poll(1000);
/// ```
#[derive(Debug, Clone)]
pub struct IndexerConfig {
    /// The Soroban contract ID to watch (Strkey-encoded, C...)
    pub contract_id: String,

    /// Which network / RPC endpoint to connect to
    pub network: Network,

    /// Ledger sequence number to start from.
    /// If None, starts from (latest_ledger - 7_days_of_ledgers) automatically.
    pub start_ledger: Option<u32>,

    /// Polling interval between RPC calls
    pub poll_interval: PollInterval,

    /// Maximum number of events to fetch per RPC call (1..=10000)
    pub max_events_per_poll: u32,
}

impl IndexerConfig {
    /// Create a new config for the given contract ID.
    pub fn new(contract_id: impl Into<String>) -> Self {
        Self {
            contract_id: contract_id.into(),
            network: Network::Testnet,
            start_ledger: None,
            poll_interval: PollInterval::default(),
            max_events_per_poll: 100,
        }
    }

    /// Set the target network.
    pub fn network(mut self, network: Network) -> Self {
        self.network = network;
        self
    }

    /// Set a custom RPC URL (shorthand for `network(Network::Custom(url))`).
    pub fn rpc_url(mut self, url: impl Into<String>) -> Self {
        self.network = Network::Custom(url.into());
        self
    }

    /// Set the starting ledger sequence number.
    pub fn start_ledger(mut self, ledger: u32) -> Self {
        self.start_ledger = Some(ledger);
        self
    }

    /// Set the polling interval in seconds.
    pub fn poll_interval_secs(mut self, secs: u64) -> Self {
        self.poll_interval = PollInterval::Seconds(secs);
        self
    }

    /// Set the maximum events to fetch per RPC round-trip.
    pub fn max_events_per_poll(mut self, limit: u32) -> Self {
        self.max_events_per_poll = limit.clamp(1, 10_000);
        self
    }
}
