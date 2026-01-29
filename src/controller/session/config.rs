// Session configuration types

use super::compactor::{LLMCompactorConfig, ToolCompaction};

/// LLM provider type
#[derive(Debug, Clone, PartialEq)]
pub enum LLMProvider {
    Anthropic,
    OpenAI,
    Google,
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
        }
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
