//! Database error types.

use thiserror::Error;

/// Errors from database operations.
#[derive(Error, Debug)]
pub enum DbError {
    /// LMDB environment or transaction error.
    #[error("Database error: {0}")]
    Env(#[from] heed::Error),

    /// Failed to serialize a value.
    #[error("Serialization error: {0}")]
    Serialize(String),

    /// Failed to deserialize a value.
    #[error("Deserialization error: {0}")]
    Deserialize(String),
}
