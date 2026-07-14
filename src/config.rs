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

impl IndexerConfig {
    /// Builds an `IndexerConfig` from environment variables — useful for
    /// containerized or CLI deployments that prefer env-based config over
    /// hardcoding values in source.
    ///
    /// Recognized variables:
    /// - `CONTRACT_ID` (**required**) — the Soroban contract ID to watch.
    /// - `STELLAR_NETWORK` (optional, default: `testnet`) — one of
    ///   `mainnet`, `testnet`, `futurenet`, or any other value, which is
    ///   treated as a custom RPC URL.
    /// - `START_LEDGER` (optional, default: auto-detect) — ledger sequence
    ///   number to start polling from. If unset, the indexer starts from
    ///   roughly 7 days before the latest ledger (see [`EventIndexer::watch`]).
    /// - `POLL_INTERVAL_SECS` (optional, default: `6`) — seconds between polls.
    ///
    /// # Errors
    /// Returns an error if `CONTRACT_ID` is missing, or if `START_LEDGER` /
    /// `POLL_INTERVAL_SECS` are set but fail to parse as integers.
    pub fn from_env() -> crate::error::Result<Self> {
        use crate::error::IndexerError;

        let contract_id = std::env::var("CONTRACT_ID").map_err(|_| {
            IndexerError::Config("CONTRACT_ID environment variable is required".to_string())
        })?;

        let network = match std::env::var("STELLAR_NETWORK") {
            Ok(value) => match value.to_lowercase().as_str() {
                "mainnet" => Network::Mainnet,
                "testnet" => Network::Testnet,
                "futurenet" => Network::Futurenet,
                _ => Network::Custom(value),
            },
            Err(_) => Network::Testnet,
        };

        let start_ledger = match std::env::var("START_LEDGER") {
            Ok(value) => Some(value.parse::<u32>().map_err(|e| {
                IndexerError::Config(format!("invalid START_LEDGER ({value:?}): {e}"))
            })?),
            Err(_) => None,
        };

        let poll_interval = match std::env::var("POLL_INTERVAL_SECS") {
            Ok(value) => {
                let secs = value.parse::<u64>().map_err(|e| {
                    IndexerError::Config(format!("invalid POLL_INTERVAL_SECS ({value:?}): {e}"))
                })?;
                PollInterval::Seconds(secs)
            }
            Err(_) => PollInterval::default(),
        };

        Ok(IndexerConfig {
            contract_id,
            network,
            start_ledger,
            poll_interval,
            max_events_per_poll: 100,
        })
    }
}

#[cfg(test)]
mod from_env_tests {
    use super::*;
    use serial_test::serial;

    fn clear_env() {
        std::env::remove_var("CONTRACT_ID");
        std::env::remove_var("STELLAR_NETWORK");
        std::env::remove_var("START_LEDGER");
        std::env::remove_var("POLL_INTERVAL_SECS");
    }

    #[test]
    #[serial]
    fn builds_with_defaults() {
        clear_env();
        std::env::set_var("CONTRACT_ID", "CTESTCONTRACTID");

        let config = IndexerConfig::from_env().expect("should build config");

        assert_eq!(config.contract_id, "CTESTCONTRACTID");
        assert_eq!(config.network, Network::Testnet);
        assert_eq!(config.start_ledger, None);
        assert!(matches!(config.poll_interval, PollInterval::Seconds(6)));

        clear_env();
    }

    #[test]
    #[serial]
    fn reads_all_overrides() {
        clear_env();
        std::env::set_var("CONTRACT_ID", "CTESTCONTRACTID");
        std::env::set_var("STELLAR_NETWORK", "mainnet");
        std::env::set_var("START_LEDGER", "199616");
        std::env::set_var("POLL_INTERVAL_SECS", "10");

        let config = IndexerConfig::from_env().expect("should build config");

        assert_eq!(config.network, Network::Mainnet);
        assert_eq!(config.start_ledger, Some(199_616));
        assert!(matches!(config.poll_interval, PollInterval::Seconds(10)));

        clear_env();
    }

    #[test]
    #[serial]
    fn errors_without_contract_id() {
        clear_env();
        let result = IndexerConfig::from_env();
        assert!(result.is_err());
        clear_env();
    }
}