// Configuration management for LLM agents
//
// Provides trait-based customization for config paths and system prompts.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::controller::{CompactionConfig, LLMSessionConfig, ToolCompaction};
use serde::Deserialize;

/// Trait for agent-specific configuration.
///
/// Implement this trait to provide custom state directory and system prompts
/// for your agent.
pub trait AgentConfig {
    /// The agent's state directory.
    ///
    /// All persistent state lives under this directory: configuration
    /// (`config.yaml`), database (`db/`), logs, etc.
    ///
    /// Paths starting with `~/` are expanded to the home directory.
    /// All other paths (absolute or relative) are used as-is.
    fn state_dir(&self) -> &str;

    /// The default system prompt for this agent
    fn default_system_prompt(&self) -> &str;

    /// The log file prefix for this agent (e.g., "multi_code", "europa")
    fn log_prefix(&self) -> &str;

    /// Agent name for display and logging
    fn name(&self) -> &str;

    /// Channel buffer size for internal communication channels.
    ///
    /// Returns None to use the default (500). Override to customize
    /// the buffer size for all async channels (LLM responses, tool results,
    /// UI events, etc.).
    ///
    /// Larger values reduce backpressure but use more memory.
    /// Smaller values provide tighter flow control.
    fn channel_buffer_size(&self) -> Option<usize> {
        None
    }

    /// Controls whether environment variables are scanned for LLM provider keys.
    ///
    /// Returns `EnvLoading::Enabled` by default, which scans all known provider
    /// env vars (ANTHROPIC_API_KEY, OPENAI_API_KEY, etc.). Override with
    /// `EnvLoading::Disabled` to skip env var scanning entirely — useful when
    /// providers are managed through a database or other mechanism.
    fn env_loading(&self) -> EnvLoading {
        EnvLoading::Enabled
    }
}

/// Controls whether `load_config` scans environment variables for LLM provider keys.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum EnvLoading {
    /// Scan all known provider env vars (default behavior).
    #[default]
    Enabled,
    /// Skip env var scanning entirely.
    Disabled,
}

/// A simple configuration for quick agent setup.
///
/// Use this when you don't need a custom config struct. Created via
/// `AgentAir::with_config()`.
///
/// # Example
///
/// ```ignore
/// let agent = AgentAir::with_config(
///     "my-agent",
///     "~/.my-agent",
///     "You are a helpful assistant."
/// )?;
/// ```
pub struct SimpleConfig {
    name: String,
    state_dir: String,
    system_prompt: String,
    log_prefix: String,
}

impl SimpleConfig {
    /// Create a new simple configuration.
    ///
    /// # Arguments
    /// * `name` - Agent name for display (e.g., "my-agent")
    /// * `state_dir` - State directory (e.g., "~/.my-agent")
    /// * `system_prompt` - Default system prompt for the agent
    pub fn new(
        name: impl Into<String>,
        state_dir: impl Into<String>,
        system_prompt: impl Into<String>,
    ) -> Self {
        let name = name.into();
        // Derive log prefix from name: lowercase, replace non-alphanumeric with underscores
        let log_prefix = name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect();

        Self {
            name,
            state_dir: state_dir.into(),
            system_prompt: system_prompt.into(),
            log_prefix,
        }
    }
}

impl AgentConfig for SimpleConfig {
    fn state_dir(&self) -> &str {
        &self.state_dir
    }

    fn default_system_prompt(&self) -> &str {
        &self.system_prompt
    }

    fn log_prefix(&self) -> &str {
        &self.log_prefix
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Provider configuration from YAML
///
/// Supported providers:
/// - `anthropic` - Anthropic Claude models
/// - `openai` - OpenAI GPT models
/// - `google` - Google Gemini models
/// - `groq` - Groq (Llama, Mixtral)
/// - `together` - Together AI
/// - `fireworks` - Fireworks AI
/// - `mistral` - Mistral AI
/// - `perplexity` - Perplexity
/// - `deepseek` - DeepSeek
/// - `openrouter` - OpenRouter (access to multiple providers)
/// - `ollama` - Local Ollama server
/// - `lmstudio` - Local LM Studio server
/// - `anyscale` - Anyscale Endpoints
/// - `cerebras` - Cerebras
/// - `sambanova` - SambaNova
/// - `xai` - xAI (Grok)
#[derive(Debug, Deserialize)]
pub struct ProviderConfig {
    /// Provider name (see above for supported values)
    pub provider: String,
    /// API token/key
    pub api_key: String,
    /// Model identifier (optional - uses provider default if not specified)
    #[serde(default)]
    pub model: String,
}

/// Root configuration structure from YAML
#[derive(Debug, Deserialize)]
pub struct ConfigFile {
    /// List of LLM provider configurations
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,

    /// Default provider to use (optional, defaults to first provider)
    pub default_provider: Option<String>,
}

/// LLM Registry - stores loaded provider configurations
pub struct LLMRegistry {
    configs: HashMap<String, LLMSessionConfig>,
    default_provider: Option<String>,
}

impl LLMRegistry {
    /// Creates an empty registry
    pub fn new() -> Self {
        Self {
            configs: HashMap::new(),
            default_provider: None,
        }
    }

    /// Load configuration from the specified config file path
    pub fn load_from_file(
        path: &PathBuf,
        default_system_prompt: &str,
    ) -> Result<Self, ConfigError> {
        let content = fs::read_to_string(path).map_err(|e| ConfigError::ReadError {
            path: path.display().to_string(),
            source: e.to_string(),
        })?;

        let config_file: ConfigFile =
            serde_yaml::from_str(&content).map_err(|e| ConfigError::ParseError {
                path: path.display().to_string(),
                source: e.to_string(),
            })?;

        let mut registry = Self::new();
        registry.default_provider = config_file.default_provider;

        for provider_config in config_file.providers {
            let session_config =
                Self::create_session_config(&provider_config, default_system_prompt)?;
            registry
                .configs
                .insert(provider_config.provider.clone(), session_config);

            // Set first provider as default if not specified
            if registry.default_provider.is_none() {
                registry.default_provider = Some(provider_config.provider);
            }
        }

        Ok(registry)
    }

    /// Insert a pre-built session config into the registry.
    ///
    /// If no default provider is set, the first inserted config becomes the default.
    pub fn insert(&mut self, name: String, config: LLMSessionConfig) {
        self.configs.insert(name.clone(), config);
        if self.default_provider.is_none() {
            self.default_provider = Some(name);
        }
    }

    /// Create session config from provider config
    pub fn create_session_config(
        config: &ProviderConfig,
        default_system_prompt: &str,
    ) -> Result<LLMSessionConfig, ConfigError> {
        use super::providers::get_provider_info;

        let provider_name = config.provider.to_lowercase();

        // Check if it's a known OpenAI-compatible provider
        let mut session_config = if let Some(info) = get_provider_info(&provider_name) {
            // Use model from config, or fall back to provider default
            let model = if config.model.is_empty() {
                info.default_model.to_string()
            } else {
                config.model.clone()
            };

            LLMSessionConfig::openai_compatible(
                &config.api_key,
                &model,
                info.base_url,
                info.context_limit,
            )
        } else {
            // Handle built-in providers
            match provider_name.as_str() {
                "anthropic" => {
                    let model = if config.model.is_empty() {
                        "claude-sonnet-4-20250514".to_string()
                    } else {
                        config.model.clone()
                    };
                    LLMSessionConfig::anthropic(&config.api_key, &model)
                }
                "openai" => {
                    let model = if config.model.is_empty() {
                        "gpt-4-turbo-preview".to_string()
                    } else {
                        config.model.clone()
                    };
                    LLMSessionConfig::openai(&config.api_key, &model)
                }
                "google" => {
                    let model = if config.model.is_empty() {
                        "gemini-2.5-flash".to_string()
                    } else {
                        config.model.clone()
                    };
                    LLMSessionConfig::google(&config.api_key, &model)
                }
                other => {
                    return Err(ConfigError::UnknownProvider {
                        provider: other.to_string(),
                    });
                }
            }
        };

        // Set system prompt from AgentConfig default
        session_config = session_config.with_system_prompt(default_system_prompt);

        // Configure aggressive compaction to avoid rate limits
        // With 0.05 threshold on 200K context = 10K tokens triggers compaction
        // keep_recent_turns=1 means only current turn keeps full tool results
        // All previous tool results are summarized to compact strings
        session_config = session_config.with_threshold_compaction(CompactionConfig {
            threshold: 0.05,
            keep_recent_turns: 1,
            tool_compaction: ToolCompaction::Summarize,
        });

        Ok(session_config)
    }

    /// Get the default session config
    pub fn get_default(&self) -> Option<&LLMSessionConfig> {
        self.default_provider
            .as_ref()
            .and_then(|p| self.configs.get(p))
            .or_else(|| self.configs.values().next())
    }

    /// Get session config by provider name
    pub fn get(&self, provider: &str) -> Option<&LLMSessionConfig> {
        self.configs.get(provider)
    }

    /// Get the default provider name
    pub fn default_provider_name(&self) -> Option<&str> {
        self.default_provider.as_deref()
    }

    /// Check if registry is empty
    pub fn is_empty(&self) -> bool {
        self.configs.is_empty()
    }

    /// Get list of available providers
    pub fn providers(&self) -> Vec<&str> {
        self.configs.keys().map(|s| s.as_str()).collect()
    }

    /// Inject environment context into all session prompts.
    ///
    /// This appends environment information (working directory, platform, date)
    /// to all configured system prompts, giving the LLM awareness of its
    /// execution context.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let registry = load_config(&config).with_environment_context();
    /// ```
    pub fn with_environment_context(mut self) -> Self {
        use super::environment::EnvironmentContext;

        let context = EnvironmentContext::gather();
        let context_section = context.to_prompt_section();

        for config in self.configs.values_mut() {
            if let Some(ref prompt) = config.system_prompt {
                config.system_prompt = Some(format!("{}\n\n{}", prompt, context_section));
            } else {
                config.system_prompt = Some(context_section.clone());
            }
        }

        self
    }
}

impl Default for LLMRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration errors
#[derive(Debug)]
pub enum ConfigError {
    /// Home directory not found
    NoHomeDirectory,
    /// Failed to read config file
    ReadError { path: String, source: String },
    /// Failed to parse config file
    ParseError { path: String, source: String },
    /// Unknown provider
    UnknownProvider { provider: String },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::NoHomeDirectory => write!(f, "Could not determine home directory"),
            ConfigError::ReadError { path, source } => {
                write!(f, "Failed to read config file '{}': {}", path, source)
            }
            ConfigError::ParseError { path, source } => {
                write!(f, "Failed to parse config file '{}': {}", path, source)
            }
            ConfigError::UnknownProvider { provider } => {
                write!(f, "Unknown provider: {}", provider)
            }
        }
    }
}

impl std::error::Error for ConfigError {}

/// Load config for an agent using its AgentConfig trait implementation.
///
/// Looks for `config.yaml` inside the agent's state directory.
/// Tries to load from the config file first, then falls back to environment variables.
/// Supports both absolute paths and paths relative to home directory.
pub fn load_config<A: AgentConfig>(agent_config: &A) -> LLMRegistry {
    let state_dir = agent_config.state_dir();
    let default_prompt = agent_config.default_system_prompt();

    // Resolve state dir - expand ~/ to home directory, otherwise use as-is
    let dir = if let Some(rest) = state_dir.strip_prefix("~/") {
        match dirs::home_dir() {
            Some(home) => home.join(rest),
            None => {
                tracing::debug!("Could not determine home directory");
                PathBuf::from(state_dir)
            }
        }
    } else {
        PathBuf::from(state_dir)
    };

    let path = dir.join("config.yaml");

    // Try loading from config file first
    match LLMRegistry::load_from_file(&path, default_prompt) {
        Ok(registry) if !registry.is_empty() => {
            tracing::info!("Loaded configuration from {}", path.display());
            return registry;
        }
        Ok(_) => {
            tracing::debug!("Config file empty, trying environment variables");
        }
        Err(e) => {
            tracing::debug!("Could not load config file: {}", e);
        }
    }

    // Check if env var scanning is disabled
    if matches!(agent_config.env_loading(), EnvLoading::Disabled) {
        tracing::debug!("Environment variable loading disabled by agent config");
        return LLMRegistry::new();
    }

    // Fall back to environment variables
    let mut registry = LLMRegistry::new();

    // Default compaction config for environment-based configuration
    let compaction = CompactionConfig {
        threshold: 0.05,
        keep_recent_turns: 1,
        tool_compaction: ToolCompaction::Summarize,
    };

    if let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") {
        let model = std::env::var("ANTHROPIC_MODEL")
            .unwrap_or_else(|_| "claude-sonnet-4-20250514".to_string());

        let config = LLMSessionConfig::anthropic(&api_key, &model)
            .with_system_prompt(default_prompt)
            .with_threshold_compaction(compaction.clone());

        registry.configs.insert("anthropic".to_string(), config);
        registry.default_provider = Some("anthropic".to_string());

        tracing::info!("Loaded Anthropic configuration from environment");
    }

    if let Ok(api_key) = std::env::var("OPENAI_API_KEY") {
        let model =
            std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4-turbo-preview".to_string());

        let config = LLMSessionConfig::openai(&api_key, &model)
            .with_system_prompt(default_prompt)
            .with_threshold_compaction(compaction.clone());

        registry.configs.insert("openai".to_string(), config);
        if registry.default_provider.is_none() {
            registry.default_provider = Some("openai".to_string());
        }

        tracing::info!("Loaded OpenAI configuration from environment");
    }

    if let Ok(api_key) = std::env::var("GOOGLE_API_KEY") {
        let model =
            std::env::var("GOOGLE_MODEL").unwrap_or_else(|_| "gemini-2.5-flash".to_string());

        let config = LLMSessionConfig::google(&api_key, &model)
            .with_system_prompt(default_prompt)
            .with_threshold_compaction(compaction.clone());

        registry.configs.insert("google".to_string(), config);
        if registry.default_provider.is_none() {
            registry.default_provider = Some("google".to_string());
        }

        tracing::info!("Loaded Google (Gemini) configuration from environment");
    }

    // Check for known OpenAI-compatible providers via environment variables
    for (name, info) in super::providers::KNOWN_PROVIDERS {
        // For providers that require API keys, the env var must contain the key
        // For local providers (Ollama, LM Studio), the env var just signals enablement
        let api_key = if info.requires_api_key {
            match std::env::var(info.env_var) {
                Ok(key) if !key.is_empty() => key,
                _ => continue, // Skip if no API key provided
            }
        } else {
            // Local provider - check if env var is set (any value enables it)
            if std::env::var(info.env_var).is_err() {
                continue;
            }
            String::new() // Empty API key for local providers
        };

        let model =
            std::env::var(info.model_env_var).unwrap_or_else(|_| info.default_model.to_string());

        let config = LLMSessionConfig::openai_compatible(
            &api_key,
            &model,
            info.base_url,
            info.context_limit,
        )
        .with_system_prompt(default_prompt)
        .with_threshold_compaction(compaction.clone());

        registry.configs.insert(name.to_string(), config);
        if registry.default_provider.is_none() {
            registry.default_provider = Some(name.to_string());
        }

        tracing::info!("Loaded {} configuration from environment", info.name);
    }

    registry
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_config() {
        let yaml = r#"
providers:
  - provider: anthropic
    api_key: test-key
    model: claude-sonnet-4-20250514
default_provider: anthropic
"#;
        let config: ConfigFile = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.providers.len(), 1);
        assert_eq!(config.providers[0].provider, "anthropic");
        assert_eq!(config.default_provider, Some("anthropic".to_string()));
    }

    #[test]
    fn test_parse_known_provider() {
        let yaml = r#"
providers:
  - provider: groq
    api_key: gsk_test_key
    model: llama-3.3-70b-versatile
"#;
        let config: ConfigFile = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.providers.len(), 1);
        assert_eq!(config.providers[0].provider, "groq");
    }

    #[test]
    fn test_known_provider_default_model() {
        // When model is not specified, it should use the provider's default
        let provider_config = ProviderConfig {
            provider: "groq".to_string(),
            api_key: "test-key".to_string(),
            model: String::new(), // Empty model
        };

        let session_config =
            LLMRegistry::create_session_config(&provider_config, "test prompt").unwrap();
        // Should use groq's default model
        assert_eq!(session_config.model, "llama-3.3-70b-versatile");
        // Should have groq's base_url set
        assert!(session_config.base_url.is_some());
        assert!(
            session_config
                .base_url
                .as_ref()
                .unwrap()
                .contains("groq.com")
        );
    }

    #[test]
    fn test_empty_registry() {
        let registry = LLMRegistry::new();
        assert!(registry.is_empty());
        assert!(registry.get_default().is_none());
    }

    #[test]
    fn test_env_loading_default_is_enabled() {
        let config = SimpleConfig::new("test", "~/.test", "prompt");
        assert_eq!(config.env_loading(), EnvLoading::Enabled);
    }

    #[test]
    fn test_env_loading_disabled_returns_empty_registry() {
        struct DisabledEnvConfig;

        impl AgentConfig for DisabledEnvConfig {
            fn state_dir(&self) -> &str {
                "/tmp/nonexistent-agent-air-test"
            }
            fn default_system_prompt(&self) -> &str {
                "test"
            }
            fn log_prefix(&self) -> &str {
                "test"
            }
            fn name(&self) -> &str {
                "test"
            }
            fn env_loading(&self) -> EnvLoading {
                EnvLoading::Disabled
            }
        }

        let registry = load_config(&DisabledEnvConfig);
        assert!(registry.is_empty());
    }
}
