//! Blocking JSON-RPC 2.0 client for the Stellar RPC `getEvents` method.
//!
//! This module handles only transport and response deserialization.
//! Business logic lives in the `indexer` module.

use crate::error::{IndexerError, Result};
use serde::{Deserialize, Serialize};

// ─── Request shapes ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct JsonRpcRequest<'a, P: Serialize> {
    jsonrpc: &'static str,
    id: u64,
    method: &'a str,
    params: P,
}

#[derive(Debug, Serialize)]
pub struct GetEventsParams {
    #[serde(rename = "startLedger")]
    pub start_ledger: u32,
    pub filters: Vec<RpcEventFilter>,
    pub pagination: PaginationOptions,
}

#[derive(Debug, Serialize)]
pub struct RpcEventFilter {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(rename = "contractIds")]
    pub contract_ids: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub topics: Vec<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct PaginationOptions {
    pub limit: u32,
}

// ─── Response shapes ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct JsonRpcResponse<T> {
    result: Option<T>,
    error: Option<RpcErrorBody>,
}

#[derive(Debug, Deserialize)]
struct RpcErrorBody {
    code: i64,
    message: String,
}

#[derive(Debug, Deserialize)]
pub struct GetEventsResponse {
    pub events: Vec<RpcEventInfo>,
    #[serde(rename = "latestLedger")]
    pub latest_ledger: u32,
}

/// Raw event as returned by the Stellar RPC.
/// Topics and value are base64-encoded XDR (ScVal).
#[derive(Debug, Clone, Deserialize)]
pub struct RpcEventInfo {
    #[serde(rename = "type")]
    pub event_type: String,
    pub ledger: u32,
    #[serde(rename = "ledgerClosedAt")]
    pub ledger_closed_at: String,
    #[serde(rename = "contractId")]
    pub contract_id: String,
    pub id: String,
    #[serde(rename = "pagingToken")]
    pub paging_token: String,
    /// Base64-encoded XDR ScVal entries (topics)
    pub topic: Vec<String>,
    /// Base64-encoded XDR ScVal (event value / data)
    pub value: String,
    #[serde(rename = "inSuccessfulContractCall")]
    pub in_successful_contract_call: bool,
    #[serde(rename = "txHash", default)]
    pub tx_hash: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LatestLedgerResult {
    sequence: u32,
}

/// Thin blocking wrapper around `ureq` for hitting the Stellar RPC.
pub struct RpcClient {
    rpc_url: String,
    request_id: std::sync::atomic::AtomicU64,
}

impl RpcClient {
    pub fn new(rpc_url: impl Into<String>) -> Self {
        Self {
            rpc_url: rpc_url.into(),
            request_id: std::sync::atomic::AtomicU64::new(1),
        }
    }

    /// Fetch the latest ledger sequence from the RPC.
    pub fn get_latest_ledger(&self) -> Result<u32> {
        let id = self.next_id();
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "getLatestLedger",
            "params": {}
        });

        let resp: JsonRpcResponse<LatestLedgerResult> = ureq::post(&self.rpc_url)
            .send_json(body)?
            .into_json()?;

        if let Some(err) = resp.error {
            return Err(IndexerError::RpcError {
                code: err.code,
                message: err.message,
            });
        }

        Ok(resp.result.map(|r| r.sequence).unwrap_or(0))
    }

    /// Call `getEvents` and return the raw response.
    pub fn get_events(&self, params: GetEventsParams) -> Result<GetEventsResponse> {
        let id = self.next_id();
        let req = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method: "getEvents",
            params,
        };

        let resp: JsonRpcResponse<GetEventsResponse> = ureq::post(&self.rpc_url)
            .send_json(serde_json::to_value(&req)?)?
            .into_json()?;

        if let Some(err) = resp.error {
            return Err(IndexerError::RpcError {
                code: err.code,
                message: err.message,
            });
        }

        resp.result.ok_or(IndexerError::EmptyResponse)
    }

    fn next_id(&self) -> u64 {
        self.request_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }
}
