use std::fmt;

use crate::controller::session::LLMProvider;

/// Default maximum tokens for responses when not specified.
pub const DEFAULT_MAX_TOKENS: u32 = 4096;

/// Configuration for creating a stateless executor.
#[derive(Debug, Clone)]
pub struct StatelessConfig {
    /// LLM provider (Anthropic, OpenAI).
    pub provider: LLMProvider,
    /// Provider API credentials.
    pub api_key: String,
    /// Default model for requests.
    pub model: String,
    /// Default max tokens (0 = use DEFAULT_MAX_TOKENS).
    pub max_tokens: u32,
    /// Default system prompt (can be overridden per request).
    pub system_prompt: Option<String>,
    /// Default temperature (None = provider default).
    pub temperature: Option<f32>,
}

impl StatelessConfig {
    /// Creates a new Anthropic config with required fields.
    pub fn anthropic(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: LLMProvider::Anthropic,
            api_key: api_key.into(),
            model: model.into(),
            max_tokens: DEFAULT_MAX_TOKENS,
            system_prompt: None,
            temperature: None,
        }
    }

    /// Creates a new OpenAI config with required fields.
    pub fn openai(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: LLMProvider::OpenAI,
            api_key: api_key.into(),
            model: model.into(),
            max_tokens: DEFAULT_MAX_TOKENS,
            system_prompt: None,
            temperature: None,
        }
    }

    /// Sets the max tokens.
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Sets the system prompt.
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Sets the temperature.
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// Validates the config and returns an error if invalid.
    pub fn validate(&self) -> Result<(), StatelessError> {
        if self.api_key.is_empty() {
            return Err(StatelessError::MissingApiKey);
        }
        if self.model.is_empty() {
            return Err(StatelessError::MissingModel);
        }
        Ok(())
    }
}

/// Result from a stateless execution.
#[derive(Debug, Clone, Default)]
pub struct StatelessResult {
    /// Concatenated text content from the LLM response.
    pub text: String,
    /// Number of tokens in the prompt sent to the LLM.
    pub input_tokens: i64,
    /// Number of tokens in the LLM's response.
    pub output_tokens: i64,
    /// Name of the model that generated the response.
    pub model: String,
    /// Why the LLM stopped generating (e.g., "end_turn", "max_tokens").
    pub stop_reason: Option<String>,
}

/// Errors that can occur during stateless execution.
#[derive(Debug, Clone, PartialEq)]
pub enum StatelessError {
    /// API key is required but was empty.
    MissingApiKey,
    /// Model is required but was empty.
    MissingModel,
    /// Input cannot be empty.
    EmptyInput,
    /// Context/request was cancelled.
    Cancelled,
    /// Stream was interrupted by callback error.
    StreamInterrupted,
    /// Execution failed with underlying error.
    ExecutionFailed { op: String, message: String },
}

impl fmt::Display for StatelessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StatelessError::MissingApiKey => write!(f, "stateless: API key is required"),
            StatelessError::MissingModel => write!(f, "stateless: model is required"),
            StatelessError::EmptyInput => write!(f, "stateless: input cannot be empty"),
            StatelessError::Cancelled => write!(f, "stateless: request cancelled"),
            StatelessError::StreamInterrupted => {
                write!(f, "stateless: stream interrupted by callback")
            }
            StatelessError::ExecutionFailed { op, message } => {
                write!(f, "stateless: {}: {}", op, message)
            }
        }
    }
}

impl std::error::Error for StatelessError {}

/// Request options that can override config defaults.
#[derive(Debug, Clone, Default)]
pub struct RequestOptions {
    /// Override the model for this request.
    pub model: Option<String>,
    /// Override max tokens for this request.
    pub max_tokens: Option<u32>,
    /// Override system prompt for this request.
    pub system_prompt: Option<String>,
    /// Override temperature for this request.
    pub temperature: Option<f32>,
}

impl RequestOptions {
    /// Creates empty request options.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the model override.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Sets the max tokens override.
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Sets the system prompt override.
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Sets the temperature override.
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }
}

/// Callback for streaming text chunks.
/// Return Ok(()) to continue streaming, or Err to stop early.
pub type StreamCallback = Box<dyn FnMut(&str) -> Result<(), ()> + Send>;
