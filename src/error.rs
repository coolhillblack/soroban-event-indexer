//! All error types for the indexer.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum IndexerError {
    #[error("RPC request failed: {0}")]
    Rpc(#[from] ureq::Error),

    #[error("RPC returned error response: code={code}, message={message}")]
    RpcError { code: i64, message: String },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Base64 decode error: {0}")]
    Base64(#[from] base64::DecodeError),

    #[error("No result and no error in RPC response")]
    EmptyResponse,

    #[error("Indexer was stopped externally")]
    Stopped,

    #[cfg(feature = "sqlite")]
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("Unexpected error: {0}")]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, IndexerError>;
