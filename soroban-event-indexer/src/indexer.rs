//! The main [`EventIndexer`] — the public entry point for the library.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use chrono::DateTime;
use tracing::{debug, error, info, warn};

use crate::config::IndexerConfig;
use crate::error::{IndexerError, Result};
use crate::event::{EventFilter, EventKind, IndexedEvent};
use crate::rpc::{GetEventsParams, PaginationOptions, RpcClient, RpcEventFilter, RpcEventInfo};
use crate::scval::ScValDecoded;

/// Seconds of ledger history available via RPC (7 days, conservative estimate)
const LEDGER_RETENTION_SECS: u32 = 7 * 24 * 3600;
/// Average ledger close time in seconds
const LEDGER_CLOSE_SECS: u32 = 6;
/// Ledgers retained ≈ 7 days worth
const LEDGER_RETENTION_COUNT: u32 = LEDGER_RETENTION_SECS / LEDGER_CLOSE_SECS;

/// The main indexer. Create one per contract you want to watch.
///
/// # Usage
///
/// ```rust,no_run
/// use soroban_event_indexer::{EventIndexer, IndexerConfig, Network};
///
/// fn main() -> anyhow::Result<()> {
///     EventIndexer::new(
///         IndexerConfig::new("CONTRACT_ID").network(Network::Testnet),
///     )
///     .watch(|event| {
///         println!("{}: {}", event.ledger, event.event_name());
///         Ok(())
///     })?;
///     Ok(())
/// }
/// ```
///
/// `watch` blocks the calling thread. To run it in the background
/// (e.g. inside a web server), spawn it on its own OS thread:
///
/// ```rust,no_run
/// # use soroban_event_indexer::{EventIndexer, IndexerConfig};
/// let indexer = EventIndexer::new(IndexerConfig::new("CONTRACT_ID"));
/// let stop = indexer.stop_handle();
///
/// std::thread::spawn(move || {
///     let _ = indexer.watch(|event| {
///         // handle event
///         Ok(())
///     });
/// });
///
/// // later, from anywhere:
/// stop.stop();
/// ```
pub struct EventIndexer {
    config: IndexerConfig,
    filter: Option<EventFilter>,
    stopped: Arc<AtomicBool>,
}

impl EventIndexer {
    /// Create a new indexer with the given configuration.
    pub fn new(config: IndexerConfig) -> Self {
        Self {
            config,
            filter: None,
            stopped: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Attach an optional event filter. Only matching events will be yielded.
    pub fn with_filter(mut self, filter: EventFilter) -> Self {
        self.filter = Some(filter);
        self
    }

    /// Returns a handle that, when called, will stop the indexer's poll loop.
    pub fn stop_handle(&self) -> StopHandle {
        StopHandle(Arc::clone(&self.stopped))
    }

    /// Start indexing and call `handler` for every new event.
    ///
    /// This blocks the calling thread and runs until [`StopHandle::stop`]
    /// is called or a fatal error occurs. For background use, run this
    /// inside `std::thread::spawn` (see struct-level docs).
    pub fn watch<F>(&self, mut handler: F) -> Result<()>
    where
        F: FnMut(IndexedEvent) -> Result<()>,
    {
        let client = RpcClient::new(self.config.network.rpc_url());

        let start_ledger = match self.config.start_ledger {
            Some(l) => l,
            None => {
                let latest = client.get_latest_ledger()?;
                let start = latest.saturating_sub(LEDGER_RETENTION_COUNT);
                info!("Auto-starting from ledger {start} (latest={latest})");
                start
            }
        };

        let mut current_ledger = start_ledger;

        info!(
            contract_id = %self.config.contract_id,
            network = %self.config.network.rpc_url(),
            start_ledger = current_ledger,
            "soroban-event-indexer starting"
        );

        loop {
            if self.stopped.load(Ordering::SeqCst) {
                info!("Indexer stopped");
                return Ok(());
            }

            let params = GetEventsParams {
                start_ledger: current_ledger,
                filters: vec![RpcEventFilter {
                    event_type: "contract".to_string(),
                    contract_ids: vec![self.config.contract_id.clone()],
                    topics: vec![],
                }],
                pagination: PaginationOptions {
                    limit: self.config.max_events_per_poll,
                },
            };

            match client.get_events(params) {
                Ok(response) => {
                    let latest = response.latest_ledger;
                    debug!(
                        "RPC returned {} events, latest_ledger={}",
                        response.events.len(),
                        latest
                    );

                    for raw_event in response.events {
                        let event = decode_event(raw_event);

                        let passes = self
                            .filter
                            .as_ref()
                            .map(|f| f.matches(&event))
                            .unwrap_or(true);

                        if passes {
                            handler(event)?;
                        }
                    }

                    if latest > current_ledger {
                        current_ledger = latest;
                    }
                }
                Err(IndexerError::RpcError { code, message }) => {
                    warn!("RPC error (will retry): code={code} message={message}");
                }
                Err(e) => {
                    error!("Transport error (will retry): {e}");
                }
            }

            std::thread::sleep(self.config.poll_interval.as_duration());
        }
    }
}

/// Decode a raw RPC event into the rich [`IndexedEvent`] type.
fn decode_event(raw: RpcEventInfo) -> IndexedEvent {
    let topics: Vec<ScValDecoded> = raw
        .topic
        .iter()
        .map(|t| ScValDecoded::from_base64(t))
        .collect();

    let value = ScValDecoded::from_base64(&raw.value);

    let ledger_closed_at = DateTime::parse_from_rfc3339(&raw.ledger_closed_at)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(|_| chrono::Utc::now());

    IndexedEvent {
        id: raw.id,
        paging_token: raw.paging_token,
        contract_id: raw.contract_id,
        ledger: raw.ledger,
        ledger_closed_at,
        tx_hash: raw.tx_hash,
        kind: EventKind::from(raw.event_type.as_str()),
        in_successful_call: raw.in_successful_contract_call,
        raw_topics: raw.topic,
        raw_value: raw.value,
        topics,
        value,
    }
}

/// A handle for cleanly stopping a running indexer from another thread.
pub struct StopHandle(Arc<AtomicBool>);

impl StopHandle {
    /// Signal the indexer to stop after its current poll completes.
    pub fn stop(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
}
