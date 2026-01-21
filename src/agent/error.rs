//! Error types for the agent module.

use thiserror::Error;

use crate::client::error::LlmError;

/// Error type for agent operations.
#[derive(Error, Debug)]
pub enum AgentError {
    /// Tool registration failed.
    #[error("Tool registration failed: {0}")]
    ToolRegistration(String),

    /// Session creation failed due to LLM error.
    #[error("Session creation failed: {0}")]
    Session(#[from] LlmError),

    /// Controller not initialized.
    #[error("Controller not initialized")]
    ControllerNotInitialized,

    /// No LLM configuration found for provider.
    #[error("No LLM configuration found for provider: {0}")]
    NoConfiguration(String),
}
