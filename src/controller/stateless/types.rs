use std::fmt;

use crate::controller::session::LLMProvider;

/// Default maximum tokens for responses when not specified.
pub const DEFAULT_MAX_TOKENS: u32 = 4096;

/// Configuration for creating a stateless executor.
#[derive(Debug, Clone)]
pub struct StatelessConfig {
    /// LLM provider (Anthropic, OpenAI, Google, Cohere, Bedrock).
    pub provider: LLMProvider,
    /// Provider API credentials.
    pub api_key: String,
    /// Default model for requests.
    pub model: String,
    /// Custom base URL for OpenAI-compatible providers.
    /// Only used when provider is OpenAI. If None, uses default OpenAI endpoint.
    pub base_url: Option<String>,
    /// Default max tokens (0 = use DEFAULT_MAX_TOKENS).
    pub max_tokens: u32,
    /// Default system prompt (can be overridden per request).
    pub system_prompt: Option<String>,
    /// Default temperature (None = provider default).
    pub temperature: Option<f32>,
    /// Azure OpenAI resource name (e.g., "my-resource").
    /// When set, the provider uses Azure OpenAI instead of standard OpenAI.
    pub azure_resource: Option<String>,
    /// Azure OpenAI deployment name (e.g., "gpt-4-deployment").
    pub azure_deployment: Option<String>,
    /// Azure OpenAI API version (e.g., "2024-10-21").
    pub azure_api_version: Option<String>,
    /// AWS region for Bedrock (e.g., "us-east-1").
    pub bedrock_region: Option<String>,
    /// AWS access key ID for Bedrock.
    pub bedrock_access_key_id: Option<String>,
    /// AWS secret access key for Bedrock.
    pub bedrock_secret_access_key: Option<String>,
    /// AWS session token for Bedrock (optional, for temporary credentials).
    pub bedrock_session_token: Option<String>,
}

impl StatelessConfig {
    /// Creates a new Anthropic config with required fields.
    pub fn anthropic(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: LLMProvider::Anthropic,
            api_key: api_key.into(),
            model: model.into(),
            base_url: None,
            max_tokens: DEFAULT_MAX_TOKENS,
            system_prompt: None,
            temperature: None,
            azure_resource: None,
            azure_deployment: None,
            azure_api_version: None,
            bedrock_region: None,
            bedrock_access_key_id: None,
            bedrock_secret_access_key: None,
            bedrock_session_token: None,
        }
    }

    /// Creates a new OpenAI config with required fields.
    pub fn openai(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: LLMProvider::OpenAI,
            api_key: api_key.into(),
            model: model.into(),
            base_url: None,
            max_tokens: DEFAULT_MAX_TOKENS,
            system_prompt: None,
            temperature: None,
            azure_resource: None,
            azure_deployment: None,
            azure_api_version: None,
            bedrock_region: None,
            bedrock_access_key_id: None,
            bedrock_secret_access_key: None,
            bedrock_session_token: None,
        }
    }

    /// Creates a new OpenAI-compatible config with a custom base URL.
    ///
    /// Use this for providers like Groq, Together, Fireworks, etc. that have
    /// OpenAI-compatible APIs.
    pub fn openai_compatible(
        api_key: impl Into<String>,
        model: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Self {
        Self {
            provider: LLMProvider::OpenAI,
            api_key: api_key.into(),
            model: model.into(),
            base_url: Some(base_url.into()),
            max_tokens: DEFAULT_MAX_TOKENS,
            system_prompt: None,
            temperature: None,
            azure_resource: None,
            azure_deployment: None,
            azure_api_version: None,
            bedrock_region: None,
            bedrock_access_key_id: None,
            bedrock_secret_access_key: None,
            bedrock_session_token: None,
        }
    }

    /// Creates a new Google (Gemini) config with required fields.
    pub fn google(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: LLMProvider::Google,
            api_key: api_key.into(),
            model: model.into(),
            base_url: None,
            max_tokens: DEFAULT_MAX_TOKENS,
            system_prompt: None,
            temperature: None,
            azure_resource: None,
            azure_deployment: None,
            azure_api_version: None,
            bedrock_region: None,
            bedrock_access_key_id: None,
            bedrock_secret_access_key: None,
            bedrock_session_token: None,
        }
    }

    /// Creates a new Azure OpenAI config.
    ///
    /// Azure OpenAI uses a different URL format and authentication method.
    pub fn azure_openai(
        api_key: impl Into<String>,
        resource: impl Into<String>,
        deployment: impl Into<String>,
    ) -> Self {
        Self {
            provider: LLMProvider::OpenAI,
            api_key: api_key.into(),
            model: String::new(),
            base_url: None,
            max_tokens: DEFAULT_MAX_TOKENS,
            system_prompt: None,
            temperature: None,
            azure_resource: Some(resource.into()),
            azure_deployment: Some(deployment.into()),
            azure_api_version: Some("2024-10-21".to_string()),
            bedrock_region: None,
            bedrock_access_key_id: None,
            bedrock_secret_access_key: None,
            bedrock_session_token: None,
        }
    }

    /// Sets the Azure API version.
    pub fn with_azure_api_version(mut self, version: impl Into<String>) -> Self {
        self.azure_api_version = Some(version.into());
        self
    }

    /// Creates a new Cohere config with required fields.
    pub fn cohere(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: LLMProvider::Cohere,
            api_key: api_key.into(),
            model: model.into(),
            base_url: None,
            max_tokens: DEFAULT_MAX_TOKENS,
            system_prompt: None,
            temperature: None,
            azure_resource: None,
            azure_deployment: None,
            azure_api_version: None,
            bedrock_region: None,
            bedrock_access_key_id: None,
            bedrock_secret_access_key: None,
            bedrock_session_token: None,
        }
    }

    /// Creates a new Amazon Bedrock config.
    ///
    /// # Arguments
    /// * `access_key_id` - AWS access key ID
    /// * `secret_access_key` - AWS secret access key
    /// * `region` - AWS region (e.g., "us-east-1")
    /// * `model` - Bedrock model ID (e.g., "anthropic.claude-3-sonnet-20240229-v1:0")
    pub fn bedrock(
        access_key_id: impl Into<String>,
        secret_access_key: impl Into<String>,
        region: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            provider: LLMProvider::Bedrock,
            api_key: String::new(), // Not used for Bedrock
            model: model.into(),
            base_url: None,
            max_tokens: DEFAULT_MAX_TOKENS,
            system_prompt: None,
            temperature: None,
            azure_resource: None,
            azure_deployment: None,
            azure_api_version: None,
            bedrock_region: Some(region.into()),
            bedrock_access_key_id: Some(access_key_id.into()),
            bedrock_secret_access_key: Some(secret_access_key.into()),
            bedrock_session_token: None,
        }
    }

    /// Sets the Bedrock session token for temporary credentials.
    pub fn with_bedrock_session_token(mut self, token: impl Into<String>) -> Self {
        self.bedrock_session_token = Some(token.into());
        self
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
