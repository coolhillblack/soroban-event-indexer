//! The core [`IndexedEvent`] type and associated filter types.

use crate::scval::ScValDecoded;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The kind of Soroban event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventKind {
    /// Emitted by a contract via `env.events().publish(...)`
    Contract,
    /// System-level events (token transfers, etc.)
    System,
    /// Diagnostic events (only when diagnostic mode enabled on the node)
    Diagnostic,
}

impl From<&str> for EventKind {
    fn from(s: &str) -> Self {
        match s {
            "system" => EventKind::System,
            "diagnostic" => EventKind::Diagnostic,
            _ => EventKind::Contract,
        }
    }
}

/// A fully decoded Soroban contract event as indexed by this library.
///
/// Both topics and value are decoded from base64 XDR ScVal into
/// the human-friendly [`ScValDecoded`] enum.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedEvent {
    /// The unique event ID from the RPC (e.g. "0000859036408881152-0000000003")
    pub id: String,

    /// Opaque paging token — use this as a cursor for the next RPC call
    pub paging_token: String,

    /// Contract that emitted the event
    pub contract_id: String,

    /// The ledger sequence number in which this event was emitted
    pub ledger: u32,

    /// Wall-clock time of the ledger close
    pub ledger_closed_at: DateTime<Utc>,

    /// The transaction hash that produced this event
    pub tx_hash: Option<String>,

    /// Event type: contract / system / diagnostic
    pub kind: EventKind,

    /// Whether the event came from a successful contract call
    pub in_successful_call: bool,

    /// Decoded topics (up to 4 ScVal entries)
    pub topics: Vec<ScValDecoded>,

    /// Decoded event data / value
    pub value: ScValDecoded,

    /// Raw base64 topics (preserved for downstream use)
    pub raw_topics: Vec<String>,

    /// Raw base64 value (preserved for downstream use)
    pub raw_value: String,
}

impl IndexedEvent {
    /// Convenience: get the first topic as a display string (usually the event name/symbol)
    pub fn event_name(&self) -> String {
        self.topics
            .first()
            .map(|t| t.display())
            .unwrap_or_else(|| "<unnamed>".to_string())
    }

    /// Returns true if this event matches a given topic prefix.
    /// E.g. `event.matches_topic("transfer")` checks if topic[0] is Symbol("transfer").
    pub fn matches_topic(&self, name: &str) -> bool {
        self.topics.first().is_some_and(|t| {
            matches!(t, ScValDecoded::Symbol(s) | ScValDecoded::String(s) if s == name)
        })
    }
}

// ─── Filter types ─────────────────────────────────────────────────────────────

/// Optional filter for what events to receive in your callback.
///
/// All specified fields must match (AND semantics).
/// Unset fields match anything.
#[derive(Debug, Clone, Default)]
pub struct EventFilter {
    /// Only events with this first topic (usually the event name)
    pub topic_name: Option<String>,
    /// Only events of this kind (default: all)
    pub kind: Option<EventKind>,
    /// Only events from successful contract calls
    pub only_successful: bool,
}

impl EventFilter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter to a specific event name (matches first topic as Symbol or String)
    pub fn topic(mut self, name: impl Into<String>) -> Self {
        self.topic_name = Some(name.into());
        self
    }

    /// Only receive contract events (not system or diagnostic)
    pub fn contract_only(mut self) -> Self {
        self.kind = Some(EventKind::Contract);
        self
    }

    /// Only receive events from successful contract calls
    pub fn successful_only(mut self) -> Self {
        self.only_successful = true;
        self
    }

    /// Returns true if this event passes the filter.
    pub fn matches(&self, event: &IndexedEvent) -> bool {
        if let Some(ref kind) = self.kind {
            if &event.kind != kind {
                return false;
            }
        }
        if self.only_successful && !event.in_successful_call {
            return false;
        }
        if let Some(ref name) = self.topic_name {
            if !event.matches_topic(name) {
                return false;
            }
        }
        true
    }
}
