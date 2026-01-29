// Known OpenAI-compatible provider registry
//
// Maps provider names to their base URLs and default settings.

/// Information about a known OpenAI-compatible provider.
#[derive(Debug, Clone)]
pub struct ProviderInfo {
    /// Display name for the provider.
    pub name: &'static str,
    /// Base URL for the API endpoint.
    pub base_url: &'static str,
    /// Default context window size.
    pub context_limit: i32,
    /// Default model to use if none specified.
    pub default_model: &'static str,
    /// Environment variable for API key.
    pub env_var: &'static str,
    /// Environment variable for model override.
    pub model_env_var: &'static str,
    /// Whether this provider requires an API key.
    /// False for local providers like Ollama and LM Studio.
    pub requires_api_key: bool,
}

/// Known OpenAI-compatible providers.
///
/// These providers use the OpenAI API format but with different base URLs.
/// Add a provider here to enable using it by name in the config (e.g., `provider: groq`).
pub const KNOWN_PROVIDERS: &[(&str, ProviderInfo)] = &[
    (
        "groq",
        ProviderInfo {
            name: "Groq",
            base_url: "https://api.groq.com/openai/v1",
            context_limit: 131_072,
            default_model: "llama-3.3-70b-versatile",
            env_var: "GROQ_API_KEY",
            model_env_var: "GROQ_MODEL",
            requires_api_key: true,
        },
    ),
    (
        "together",
        ProviderInfo {
            name: "Together AI",
            base_url: "https://api.together.xyz/v1",
            context_limit: 131_072,
            default_model: "meta-llama/Meta-Llama-3.1-70B-Instruct-Turbo",
            env_var: "TOGETHER_API_KEY",
            model_env_var: "TOGETHER_MODEL",
            requires_api_key: true,
        },
    ),
    (
        "fireworks",
        ProviderInfo {
            name: "Fireworks AI",
            base_url: "https://api.fireworks.ai/inference/v1",
            context_limit: 131_072,
            default_model: "accounts/fireworks/models/llama-v3p1-70b-instruct",
            env_var: "FIREWORKS_API_KEY",
            model_env_var: "FIREWORKS_MODEL",
            requires_api_key: true,
        },
    ),
    (
        "mistral",
        ProviderInfo {
            name: "Mistral AI",
            base_url: "https://api.mistral.ai/v1",
            context_limit: 128_000,
            default_model: "mistral-large-latest",
            env_var: "MISTRAL_API_KEY",
            model_env_var: "MISTRAL_MODEL",
            requires_api_key: true,
        },
    ),
    (
        "perplexity",
        ProviderInfo {
            name: "Perplexity",
            base_url: "https://api.perplexity.ai/chat/completions",
            context_limit: 128_000,
            default_model: "llama-3.1-sonar-large-128k-online",
            env_var: "PERPLEXITY_API_KEY",
            model_env_var: "PERPLEXITY_MODEL",
            requires_api_key: true,
        },
    ),
    (
        "deepseek",
        ProviderInfo {
            name: "DeepSeek",
            base_url: "https://api.deepseek.com/v1",
            context_limit: 64_000,
            default_model: "deepseek-chat",
            env_var: "DEEPSEEK_API_KEY",
            model_env_var: "DEEPSEEK_MODEL",
            requires_api_key: true,
        },
    ),
    (
        "openrouter",
        ProviderInfo {
            name: "OpenRouter",
            base_url: "https://openrouter.ai/api/v1",
            context_limit: 200_000,
            default_model: "anthropic/claude-3.5-sonnet",
            env_var: "OPENROUTER_API_KEY",
            model_env_var: "OPENROUTER_MODEL",
            requires_api_key: true,
        },
    ),
    (
        "ollama",
        ProviderInfo {
            name: "Ollama",
            base_url: "http://localhost:11434/v1",
            context_limit: 128_000,
            default_model: "llama3.1",
            env_var: "OLLAMA_HOST", // Not an API key - just signals to enable Ollama
            model_env_var: "OLLAMA_MODEL",
            requires_api_key: false,
        },
    ),
    (
        "lmstudio",
        ProviderInfo {
            name: "LM Studio",
            base_url: "http://localhost:1234/v1",
            context_limit: 128_000,
            default_model: "local-model",
            env_var: "LMSTUDIO_HOST", // Not an API key - just signals to enable LM Studio
            model_env_var: "LMSTUDIO_MODEL",
            requires_api_key: false,
        },
    ),
    (
        "anyscale",
        ProviderInfo {
            name: "Anyscale",
            base_url: "https://api.endpoints.anyscale.com/v1",
            context_limit: 128_000,
            default_model: "meta-llama/Meta-Llama-3.1-70B-Instruct",
            env_var: "ANYSCALE_API_KEY",
            model_env_var: "ANYSCALE_MODEL",
            requires_api_key: true,
        },
    ),
    (
        "cerebras",
        ProviderInfo {
            name: "Cerebras",
            base_url: "https://api.cerebras.ai/v1",
            context_limit: 128_000,
            default_model: "llama3.1-70b",
            env_var: "CEREBRAS_API_KEY",
            model_env_var: "CEREBRAS_MODEL",
            requires_api_key: true,
        },
    ),
    (
        "sambanova",
        ProviderInfo {
            name: "SambaNova",
            base_url: "https://api.sambanova.ai/v1",
            context_limit: 128_000,
            default_model: "Meta-Llama-3.1-70B-Instruct",
            env_var: "SAMBANOVA_API_KEY",
            model_env_var: "SAMBANOVA_MODEL",
            requires_api_key: true,
        },
    ),
    (
        "xai",
        ProviderInfo {
            name: "xAI",
            base_url: "https://api.x.ai/v1",
            context_limit: 131_072,
            default_model: "grok-2-latest",
            env_var: "XAI_API_KEY",
            model_env_var: "XAI_MODEL",
            requires_api_key: true,
        },
    ),
    (
        "ai21",
        ProviderInfo {
            name: "AI21 Labs",
            base_url: "https://api.ai21.com/studio/v1",
            context_limit: 256_000,
            default_model: "jamba-1.5-large",
            env_var: "AI21_API_KEY",
            model_env_var: "AI21_MODEL",
            requires_api_key: true,
        },
    ),
];

/// Returns provider info for a known provider name.
///
/// Provider names are case-insensitive.
pub fn get_provider_info(name: &str) -> Option<&'static ProviderInfo> {
    let name_lower = name.to_lowercase();
    KNOWN_PROVIDERS
        .iter()
        .find(|(key, _)| *key == name_lower)
        .map(|(_, info)| info)
}

/// Returns true if this is a known OpenAI-compatible provider.
pub fn is_known_provider(name: &str) -> bool {
    get_provider_info(name).is_some()
}

/// Returns a list of all known provider names.
pub fn list_providers() -> Vec<&'static str> {
    KNOWN_PROVIDERS.iter().map(|(name, _)| *name).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_known_provider() {
        let info = get_provider_info("groq").unwrap();
        assert_eq!(info.name, "Groq");
        assert!(info.base_url.contains("groq.com"));
    }

    #[test]
    fn test_case_insensitive() {
        assert!(get_provider_info("GROQ").is_some());
        assert!(get_provider_info("Groq").is_some());
        assert!(get_provider_info("groq").is_some());
    }

    #[test]
    fn test_unknown_provider() {
        assert!(get_provider_info("unknown").is_none());
    }

    #[test]
    fn test_list_providers() {
        let providers = list_providers();
        assert!(providers.contains(&"groq"));
        assert!(providers.contains(&"together"));
        assert!(providers.contains(&"fireworks"));
    }
}
