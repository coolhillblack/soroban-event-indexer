//! The main [`EventIndexer`] — the public entry point for the library.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tracing::{debug, info, warn};

use crate::config::IndexerConfig;
use crate::error::Result;
use crate::event::{decode_event, EventFilter, IndexedEvent};
use crate::rpc::{GetEventsParams, PaginationOptions, RpcClient, RpcEventFilter};

/// Cap on exponential backoff after repeated poll errors, so we never
/// wait longer than this between retries regardless of failure streak.
const MAX_BACKOFF_SECS: u64 = 60;

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
        let mut consecutive_errors: u32 = 0;

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

            let poll_span = tracing::info_span!("poll", ledger = current_ledger);
            let poll_result = poll_span.in_scope(|| -> Result<u32> {
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

                let response = client.get_events(params)?;

                debug!(
                    "RPC returned {} events, latest_ledger={}",
                    response.events.len(),
                    response.latest_ledger
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

                Ok(response.latest_ledger)
            });

            match poll_result {
                Ok(latest) => {
                    consecutive_errors = 0;
                    if latest > current_ledger {
                        current_ledger = latest;
                    }
                    std::thread::sleep(self.config.poll_interval.as_duration());
                }
                Err(e) => {
                    consecutive_errors += 1;
                    warn!("Poll failed (attempt {consecutive_errors}, will retry): {e}");
                    std::thread::sleep(backoff_delay(
                        self.config.poll_interval.as_duration(),
                        consecutive_errors,
                    ));
                }
            }
        }
    }
}

/// Compute the delay before the next retry after `attempt` consecutive
/// failures, doubling each time up to [`MAX_BACKOFF_SECS`].
///
/// `attempt` is 1 on the first failure. The base poll interval is used
/// as the starting point so a fast-polling indexer still backs off
/// meaningfully instead of hammering a struggling RPC endpoint.
fn backoff_delay(base: Duration, attempt: u32) -> Duration {
    let capped_attempt = attempt.min(10); // avoid overflow on the shift
    let multiplier = 1u64 << capped_attempt.saturating_sub(1);
    let backoff_secs = base.as_secs().max(1).saturating_mul(multiplier);
    Duration::from_secs(backoff_secs.min(MAX_BACKOFF_SECS))
}

/// A handle for cleanly stopping a running indexer from another thread.
pub struct StopHandle(Arc<AtomicBool>);

impl StopHandle {
    /// Signal the indexer to stop after its current poll completes.
    pub fn stop(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_failure_uses_base_interval() {
        let base = Duration::from_secs(5);
        assert_eq!(backoff_delay(base, 1), Duration::from_secs(5));
    }

    #[test]
    fn backoff_doubles_each_attempt() {
        let base = Duration::from_secs(2);
        assert_eq!(backoff_delay(base, 1), Duration::from_secs(2));
        assert_eq!(backoff_delay(base, 2), Duration::from_secs(4));
        assert_eq!(backoff_delay(base, 3), Duration::from_secs(8));
        assert_eq!(backoff_delay(base, 4), Duration::from_secs(16));
    }

    #[test]
    fn backoff_never_exceeds_cap() {
        let base = Duration::from_secs(5);
        // Even after many consecutive failures, we should never wait
        // longer than MAX_BACKOFF_SECS.
        for attempt in 1..=20 {
            let delay = backoff_delay(base, attempt);
            assert!(
                delay.as_secs() <= MAX_BACKOFF_SECS,
                "attempt {attempt} produced delay {delay:?}, exceeding cap"
            );
        }
    }

    #[test]
    fn sub_second_poll_interval_still_backs_off() {
        // A poll interval under 1 second shouldn't produce a zero-second
        // backoff; we floor the base at 1 second before multiplying.
        let base = Duration::from_millis(200);
        assert_eq!(backoff_delay(base, 1), Duration::from_secs(1));
        assert_eq!(backoff_delay(base, 2), Duration::from_secs(2));
    }

    #[test]
    fn zero_attempt_behaves_like_first_attempt() {
        // Defensive: attempt=0 shouldn't panic or underflow.
        let base = Duration::from_secs(3);
        assert_eq!(backoff_delay(base, 0), Duration::from_secs(3));
    }
}