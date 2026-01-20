// Configuration management for LLM agents
//
// Provides trait-based customization for config paths and system prompts.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::controller::{CompactionConfig, LLMProvider, LLMSessionConfig, ToolCompaction};
use serde::Deserialize;

/// Trait for agent-specific configuration.
///
/// Implement this trait to provide custom config paths and system prompts
/// for your agent.
pub trait AgentConfig {
    /// The config file path relative to home directory (e.g., ".multi_code/config.yaml")
    fn config_path(&self) -> &str;

    /// The default system prompt for this agent
    fn default_system_prompt(&self) -> &str;

    /// The log file prefix for this agent (e.g., "multi_code", "europa")
    fn log_prefix(&self) -> &str;

    /// Agent name for display and logging
    fn name(&self) -> &str;
}

/// Provider configuration from YAML
#[derive(Debug, Deserialize)]
pub struct ProviderConfig {
    /// Provider name: "anthropic" or "openai"
    pub provider: String,
    /// API token/key
    pub api_key: String,
    /// Model identifier
    pub model: String,
    /// Optional system prompt override
    pub system_prompt: Option<String>,
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
    pub fn load_from_file(path: &PathBuf, default_system_prompt: &str) -> Result<Self, ConfigError> {
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
            let session_config = Self::create_session_config(&provider_config, default_system_prompt)?;
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

    /// Create session config from provider config
    fn create_session_config(config: &ProviderConfig, default_system_prompt: &str) -> Result<LLMSessionConfig, ConfigError> {
        let provider = match config.provider.as_str() {
            "anthropic" => LLMProvider::Anthropic,
            "openai" => LLMProvider::OpenAI,
            other => {
                return Err(ConfigError::UnknownProvider {
                    provider: other.to_string(),
                })
            }
        };

        let mut session_config = match provider {
            LLMProvider::Anthropic => {
                LLMSessionConfig::anthropic(&config.api_key, &config.model)
            }
            LLMProvider::OpenAI => {
                LLMSessionConfig::openai(&config.api_key, &config.model)
            }
        };

        // Set system prompt
        let system_prompt = config
            .system_prompt
            .clone()
            .unwrap_or_else(|| default_system_prompt.to_string());
        session_config = session_config.with_system_prompt(system_prompt);

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
/// Tries to load from the config file first, then falls back to environment variables.
pub fn load_config<A: AgentConfig>(agent_config: &A) -> LLMRegistry {
    let config_path = agent_config.config_path();
    let default_prompt = agent_config.default_system_prompt();

    // Try loading from config file first
    if let Some(home) = dirs::home_dir() {
        let path = home.join(config_path);
        match LLMRegistry::load_from_file(&path, default_prompt) {
            Ok(registry) if !registry.is_empty() => {
                tracing::info!("Loaded configuration from ~/{}", config_path);
                return registry;
            }
            Ok(_) => {
                tracing::debug!("Config file empty, trying environment variables");
            }
            Err(e) => {
                tracing::debug!("Could not load config file: {}", e);
            }
        }
    }

    // Fall back to environment variables
    let mut registry = LLMRegistry::new();

    if let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") {
        let model = std::env::var("ANTHROPIC_MODEL")
            .unwrap_or_else(|_| "claude-sonnet-4-20250514".to_string());

        let config = LLMSessionConfig::anthropic(&api_key, &model)
            .with_system_prompt(default_prompt);

        registry.configs.insert("anthropic".to_string(), config);
        registry.default_provider = Some("anthropic".to_string());

        tracing::info!("Loaded Anthropic configuration from environment");
    }

    if let Ok(api_key) = std::env::var("OPENAI_API_KEY") {
        let model =
            std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4-turbo-preview".to_string());

        let config =
            LLMSessionConfig::openai(&api_key, &model).with_system_prompt(default_prompt);

        registry.configs.insert("openai".to_string(), config);
        if registry.default_provider.is_none() {
            registry.default_provider = Some("openai".to_string());
        }

        tracing::info!("Loaded OpenAI configuration from environment");
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
    fn test_empty_registry() {
        let registry = LLMRegistry::new();
        assert!(registry.is_empty());
        assert!(registry.get_default().is_none());
    }
}
