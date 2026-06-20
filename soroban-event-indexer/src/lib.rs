//! # soroban-event-indexer
//!
//! An embeddable Rust library for indexing Soroban smart contract events.
//!
//! Stellar RPC's `getEvents` only retains ~7 days of history. Every dApp
//! that needs longer-lived event data (escrows, payments, ticketing, DeFi)
//! must build its own ingestion layer from scratch. This crate makes that
//! a 3-line job.
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use soroban_event_indexer::{EventIndexer, IndexerConfig, Network};
//!
//! fn main() -> anyhow::Result<()> {
//!     let indexer = EventIndexer::new(
//!         IndexerConfig::new("CCONTRACTIDHERE...")
//!             .network(Network::Testnet)
//!             .start_ledger(1_000_000),
//!     );
//!
//!     indexer.watch(|event| {
//!         println!("Event: {:?}", event);
//!         Ok(())
//!     })?;
//!
//!     Ok(())
//! }
//! ```

pub mod config;
pub mod error;
pub mod event;
pub mod indexer;
pub mod rpc;
pub mod scval;

#[cfg(feature = "sqlite")]
pub mod storage;

pub use config::{IndexerConfig, Network, PollInterval};
pub use error::IndexerError;
pub use event::{EventFilter, EventKind, IndexedEvent};
pub use indexer::{EventIndexer, StopHandle};
