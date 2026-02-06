use std::error::Error;
use std::fmt;

/// Error returned by LLM provider API calls.
#[derive(Debug, Clone)]
pub struct LlmError {
    /// Provider-specific error code.
    pub error_code: String,
    /// Human-readable error message.
    pub error_message: String,
}

impl LlmError {
    /// Create a new LLM error with code and message.
    pub fn new(error_code: impl Into<String>, error_message: impl Into<String>) -> Self {
        Self {
            error_code: error_code.into(),
            error_message: error_message.into(),
        }
    }
}

impl fmt::Display for LlmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.error_code, self.error_message)
    }
}

impl Error for LlmError {}
