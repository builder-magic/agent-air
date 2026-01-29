// Session configuration types

use super::compactor::{LLMCompactorConfig, ToolCompaction};

/// LLM provider type
#[derive(Debug, Clone, PartialEq)]
pub enum LLMProvider {
    Anthropic,
    OpenAI,
    Google,
    Cohere,
    Bedrock,
}

/// Configuration for conversation compaction
#[derive(Debug, Clone)]
pub struct CompactionConfig {
    /// Context utilization threshold (0.0-1.0) that triggers compaction.
    /// For example, 0.75 means compact when 75% of context is used.
    pub threshold: f64,
    /// Number of recent turns to preserve during compaction.
    pub keep_recent_turns: usize,
    /// Strategy for handling old tool results.
    pub tool_compaction: ToolCompaction,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            threshold: 0.75,
            keep_recent_turns: 5,
            tool_compaction: ToolCompaction::Summarize,
        }
    }
}

/// Type of compaction strategy to use.
#[derive(Debug, Clone)]
pub enum CompactorType {
    /// Simple threshold-based compaction that summarizes/redacts old tool results.
    Threshold(CompactionConfig),
    /// LLM-based conversation summarization that uses an LLM to create intelligent summaries.
    LLM(LLMCompactorConfig),
}

impl Default for CompactorType {
    fn default() -> Self {
        CompactorType::Threshold(CompactionConfig::default())
    }
}

/// Configuration for creating an LLM session
#[derive(Debug, Clone)]
pub struct LLMSessionConfig {
    /// The LLM provider to use
    pub provider: LLMProvider,
    /// API key for the provider
    pub api_key: String,
    /// Model to use (e.g., "claude-3-sonnet-20240229", "gpt-4")
    pub model: String,
    /// Custom base URL for OpenAI-compatible providers.
    /// Only used when provider is OpenAI. If None, uses default OpenAI endpoint.
    pub base_url: Option<String>,
    /// Default maximum tokens for responses
    pub max_tokens: Option<u32>,
    /// Default system prompt
    pub system_prompt: Option<String>,
    /// Default temperature
    pub temperature: Option<f32>,
    /// Enable streaming responses
    pub streaming: bool,
    /// Model's context window size (for compaction decisions)
    pub context_limit: i32,
    /// Compaction configuration (None to disable compaction)
    pub compaction: Option<CompactorType>,
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

impl LLMSessionConfig {
    /// Creates a new Anthropic session config
    pub fn anthropic(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: LLMProvider::Anthropic,
            api_key: api_key.into(),
            model: model.into(),
            base_url: None,
            max_tokens: Some(4096),
            system_prompt: None,
            temperature: None,
            streaming: true,
            context_limit: 200_000, // Claude default
            compaction: Some(CompactorType::default()),
            azure_resource: None,
            azure_deployment: None,
            azure_api_version: None,
            bedrock_region: None,
            bedrock_access_key_id: None,
            bedrock_secret_access_key: None,
            bedrock_session_token: None,
        }
    }

    /// Creates a new OpenAI session config
    pub fn openai(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: LLMProvider::OpenAI,
            api_key: api_key.into(),
            model: model.into(),
            base_url: None,
            max_tokens: Some(4096),
            system_prompt: None,
            temperature: None,
            streaming: true,
            context_limit: 128_000, // GPT-4 default
            compaction: Some(CompactorType::default()),
            azure_resource: None,
            azure_deployment: None,
            azure_api_version: None,
            bedrock_region: None,
            bedrock_access_key_id: None,
            bedrock_secret_access_key: None,
            bedrock_session_token: None,
        }
    }

    /// Creates a new OpenAI-compatible session config with a custom base URL.
    ///
    /// Use this for providers like Groq, Together, Fireworks, etc. that have
    /// OpenAI-compatible APIs.
    pub fn openai_compatible(
        api_key: impl Into<String>,
        model: impl Into<String>,
        base_url: impl Into<String>,
        context_limit: i32,
    ) -> Self {
        Self {
            provider: LLMProvider::OpenAI,
            api_key: api_key.into(),
            model: model.into(),
            base_url: Some(base_url.into()),
            max_tokens: Some(4096),
            system_prompt: None,
            temperature: None,
            streaming: true,
            context_limit,
            compaction: Some(CompactorType::default()),
            azure_resource: None,
            azure_deployment: None,
            azure_api_version: None,
            bedrock_region: None,
            bedrock_access_key_id: None,
            bedrock_secret_access_key: None,
            bedrock_session_token: None,
        }
    }

    /// Creates a new Google (Gemini) session config
    pub fn google(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: LLMProvider::Google,
            api_key: api_key.into(),
            model: model.into(),
            base_url: None,
            max_tokens: Some(4096),
            system_prompt: None,
            temperature: None,
            streaming: true,
            context_limit: 1_000_000, // Gemini 2.5 default
            compaction: Some(CompactorType::default()),
            azure_resource: None,
            azure_deployment: None,
            azure_api_version: None,
            bedrock_region: None,
            bedrock_access_key_id: None,
            bedrock_secret_access_key: None,
            bedrock_session_token: None,
        }
    }

    /// Creates a new Azure OpenAI session config.
    ///
    /// Azure OpenAI uses a different URL format and authentication method.
    /// The endpoint is: https://{resource}.openai.azure.com/openai/deployments/{deployment}/chat/completions?api-version={version}
    ///
    /// # Arguments
    /// * `api_key` - Azure OpenAI API key
    /// * `resource` - Azure resource name (e.g., "my-openai-resource")
    /// * `deployment` - Deployment name (e.g., "gpt-4-deployment")
    pub fn azure_openai(
        api_key: impl Into<String>,
        resource: impl Into<String>,
        deployment: impl Into<String>,
    ) -> Self {
        Self {
            provider: LLMProvider::OpenAI,
            api_key: api_key.into(),
            model: String::new(), // Not used for Azure - deployment determines model
            base_url: None,
            max_tokens: Some(4096),
            system_prompt: None,
            temperature: None,
            streaming: true,
            context_limit: 128_000, // Azure OpenAI default
            compaction: Some(CompactorType::default()),
            azure_resource: Some(resource.into()),
            azure_deployment: Some(deployment.into()),
            azure_api_version: Some("2024-10-21".to_string()), // Latest stable version
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

    /// Creates a new Cohere session config
    pub fn cohere(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: LLMProvider::Cohere,
            api_key: api_key.into(),
            model: model.into(),
            base_url: None,
            max_tokens: Some(4096),
            system_prompt: None,
            temperature: None,
            streaming: true,
            context_limit: 128_000, // Command-R context limit
            compaction: Some(CompactorType::default()),
            azure_resource: None,
            azure_deployment: None,
            azure_api_version: None,
            bedrock_region: None,
            bedrock_access_key_id: None,
            bedrock_secret_access_key: None,
            bedrock_session_token: None,
        }
    }

    /// Creates a new Amazon Bedrock session config.
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
            max_tokens: Some(4096),
            system_prompt: None,
            temperature: None,
            streaming: true,
            context_limit: 200_000, // Claude on Bedrock default
            compaction: Some(CompactorType::default()),
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

    /// Enable or disable streaming
    pub fn with_streaming(mut self, streaming: bool) -> Self {
        self.streaming = streaming;
        self
    }

    /// Sets the default max tokens
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Sets the default system prompt
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Sets the default temperature
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// Sets the model's context window size
    pub fn with_context_limit(mut self, context_limit: i32) -> Self {
        self.context_limit = context_limit;
        self
    }

    /// Sets a custom base URL for OpenAI-compatible providers
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    /// Enables threshold compaction with custom configuration
    pub fn with_threshold_compaction(mut self, config: CompactionConfig) -> Self {
        self.compaction = Some(CompactorType::Threshold(config));
        self
    }

    /// Enables LLM-based compaction with custom configuration
    pub fn with_llm_compaction(mut self, config: LLMCompactorConfig) -> Self {
        self.compaction = Some(CompactorType::LLM(config));
        self
    }

    /// Enables compaction with the specified compactor type
    pub fn with_compaction(mut self, compactor_type: CompactorType) -> Self {
        self.compaction = Some(compactor_type);
        self
    }

    /// Disables compaction
    pub fn without_compaction(mut self) -> Self {
        self.compaction = None;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anthropic_config() {
        let config = LLMSessionConfig::anthropic("test-key", "claude-3-sonnet")
            .with_max_tokens(2048)
            .with_system_prompt("You are helpful.");

        assert_eq!(config.provider, LLMProvider::Anthropic);
        assert_eq!(config.api_key, "test-key");
        assert_eq!(config.model, "claude-3-sonnet");
        assert_eq!(config.max_tokens, Some(2048));
        assert_eq!(config.system_prompt, Some("You are helpful.".to_string()));
    }

    #[test]
    fn test_openai_config() {
        let config = LLMSessionConfig::openai("test-key", "gpt-4");

        assert_eq!(config.provider, LLMProvider::OpenAI);
        assert_eq!(config.model, "gpt-4");
    }
}
